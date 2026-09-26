//! The TabDisplay Windows service (`tabdisplay.exe --service`, installed by the installer, runs as SYSTEM).
//! It owns the Virtual Display Driver's device so the app never needs admin:
//! - the device stays **disabled** while no tablet is connected, so Windows has no extra monitor;
//! - the app takes a [`Lease`] per session (`ENABLE w h` over a named pipe); the device is enabled while
//!   any lease is open and disabled when the last one closes, including when the app crashes.
use super::{display, driver};
use std::ffi::OsString;
use std::fs::{File, OpenOptions};
use std::io::{self, BufRead, BufReader, Write};
use std::os::windows::io::FromRawHandle;
use std::sync::{Mutex, OnceLock};
use windows::core::PCWSTR;
use windows::Win32::Foundation::{GetLastError, ERROR_FILE_NOT_FOUND, ERROR_PIPE_CONNECTED};
use windows::Win32::Security::Authorization::{ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1};
use windows::Win32::Security::{PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES};
use windows::Win32::Storage::FileSystem::PIPE_ACCESS_DUPLEX;
use windows::Win32::System::Pipes::*;
use windows_service::service::*;
use windows_service::service_control_handler::{self, ServiceControlHandlerResult, ServiceStatusHandle};
use windows_service::{define_windows_service, service_dispatcher};

pub const NAME: &str = "TabDisplay";
const PIPE: &str = r"\\.\pipe\tabdisplay-service";
/// SYSTEM and admins: full; any signed-in user: read/write (so the unelevated app can talk to it).
const SDDL: &str = "D:(A;;GA;;;SY)(A;;GA;;;BA)(A;;GRGW;;;AU)";

// ---- app side -------------------------------------------------------------------------------------------

/// A session's hold on the virtual monitor. The monitor exists while any lease is open.
pub struct Lease(#[allow(dead_code)] File);

/// Whether the service is running (checks its pipe without connecting).
pub fn available() -> bool {
    let name: Vec<u16> = PIPE.encode_utf16().chain([0]).collect();
    unsafe { WaitNamedPipeW(PCWSTR(name.as_ptr()), 1).as_bool() || GetLastError() != ERROR_FILE_NOT_FOUND }
}

/// Plugs in the virtual monitor, making sure the driver offers `w`x`h`.
pub fn enable(w: u32, h: u32) -> io::Result<Lease> {
    let pipe = request(&format!("ENABLE {w} {h}"))?;
    Ok(Lease(pipe))
}

/// Restarts the driver (e.g. after it crashed). No UAC: the service does it.
pub fn restart() -> io::Result<()> {
    request("RESTART").map(drop)
}

fn request(cmd: &str) -> io::Result<File> {
    let mut pipe = OpenOptions::new().read(true).write(true).open(PIPE)?;
    writeln!(pipe, "{cmd}")?;
    let mut reply = String::new();
    BufReader::new(&pipe).read_line(&mut reply)?;
    match reply.trim_end() {
        "OK" => Ok(pipe),
        err => Err(io::Error::other(err.trim_start_matches("ERR ").to_string())),
    }
}

// ---- service side ---------------------------------------------------------------------------------------

define_windows_service!(ffi_service_main, service_main);

/// Entry point for `tabdisplay.exe --service` (called by the Service Control Manager).
pub fn run() -> i32 {
    match service_dispatcher::start(NAME, ffi_service_main) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("service: {e}");
            1
        }
    }
}

static STATUS: OnceLock<ServiceStatusHandle> = OnceLock::new();
/// Open leases; the device is enabled iff this is > 0.
static LEASES: Mutex<usize> = Mutex::new(0);

fn set_state(state: ServiceState) {
    if let Some(handle) = STATUS.get() {
        let _ = handle.set_service_status(ServiceStatus {
            service_type: ServiceType::OWN_PROCESS,
            current_state: state,
            controls_accepted: if state == ServiceState::Running { ServiceControlAccept::STOP | ServiceControlAccept::SHUTDOWN } else { ServiceControlAccept::empty() },
            exit_code: ServiceExitCode::Win32(0),
            checkpoint: 0,
            wait_hint: std::time::Duration::default(),
            process_id: None,
        });
    }
}

fn service_main(_args: Vec<OsString>) {
    let handler = |control| match control {
        ServiceControl::Stop | ServiceControl::Shutdown => {
            set_state(ServiceState::StopPending);
            let _ = driver::set_enabled(false);
            set_state(ServiceState::Stopped);
            std::process::exit(0);
        }
        ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
        _ => ServiceControlHandlerResult::NotImplemented,
    };
    let Ok(handle) = service_control_handler::register(NAME, handler) else { return };
    let _ = STATUS.set(handle);

    // No tablet yet: no virtual monitor. Also tidy the driver's mode list (it reads it when enabled).
    let _ = display::ensure_modes(&[]);
    let _ = driver::set_enabled(false);
    set_state(ServiceState::Running);
    serve();
}

/// One pipe instance per client, each on its own thread.
fn serve() {
    let name: Vec<u16> = PIPE.encode_utf16().chain([0]).collect();
    let sddl: Vec<u16> = SDDL.encode_utf16().chain([0]).collect();
    let mut sd = PSECURITY_DESCRIPTOR::default();
    // Leaked on purpose: used for every pipe instance for the life of the service.
    if unsafe { ConvertStringSecurityDescriptorToSecurityDescriptorW(PCWSTR(sddl.as_ptr()), SDDL_REVISION_1, &mut sd, None) }.is_err() {
        return;
    }
    let sa = SECURITY_ATTRIBUTES { nLength: size_of::<SECURITY_ATTRIBUTES>() as u32, lpSecurityDescriptor: sd.0, bInheritHandle: false.into() };
    loop {
        let pipe = unsafe {
            CreateNamedPipeW(
                PCWSTR(name.as_ptr()),
                PIPE_ACCESS_DUPLEX,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT,
                PIPE_UNLIMITED_INSTANCES,
                512,
                512,
                0,
                Some(&sa),
            )
        };
        if pipe.is_invalid() {
            std::thread::sleep(std::time::Duration::from_secs(1));
            continue;
        }
        let connected = unsafe { ConnectNamedPipe(pipe, None) }.is_ok() || unsafe { GetLastError() } == ERROR_PIPE_CONNECTED;
        let file = unsafe { File::from_raw_handle(pipe.0 as _) }; // closes the handle when dropped
        if connected {
            std::thread::spawn(move || client(file));
        }
    }
}

/// Serves one app connection. A connection that enabled the monitor holds a lease until it closes.
fn client(pipe: File) {
    let mut out = &pipe;
    let mut holds_lease = false;
    for line in BufReader::new(&pipe).lines() {
        let Ok(line) = line else { break };
        let reply = match handle(&line, &mut holds_lease) {
            Ok(()) => "OK".to_string(),
            Err(e) => format!("ERR {e}"),
        };
        if writeln!(out, "{reply}").is_err() {
            break;
        }
    }
    if holds_lease {
        release();
    }
}

fn handle(line: &str, holds_lease: &mut bool) -> io::Result<()> {
    let mut words = line.split_whitespace();
    match words.next() {
        Some("ENABLE") if !*holds_lease => {
            let mut n = || words.next().and_then(|v| v.parse().ok()).ok_or_else(|| io::Error::other("ENABLE w h"));
            let (w, h) = (n()?, n()?);
            let mut leases = LEASES.lock().unwrap();
            let new_mode = display::ensure_modes(&[(w, h)])?;
            if *leases == 0 {
                driver::set_enabled(true)?; // a freshly enabled driver reads the mode list
            } else if new_mode {
                driver::restart()?;
            }
            *leases += 1;
            *holds_lease = true;
            Ok(())
        }
        Some("RESTART") => {
            let leases = LEASES.lock().unwrap();
            if *leases > 0 { driver::restart() } else { driver::set_enabled(false) } // also clears Code 43
        }
        _ => Err(io::Error::other(format!("comando desconhecido: {line}"))),
    }
}

fn release() {
    let mut leases = LEASES.lock().unwrap();
    *leases = leases.saturating_sub(1);
    if *leases == 0 {
        let _ = driver::set_enabled(false);
    }
}
