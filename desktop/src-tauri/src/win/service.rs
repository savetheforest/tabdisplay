//! The TabDisplay Windows service (`tabdisplay.exe --service`, installed by the installer, runs as SYSTEM).
//! It owns the Virtual Display Driver's device so the app never needs admin:
//! - the device stays **disabled** while no tablet is connected, so Windows has no extra monitor;
//! - the app takes a [`Lease`] per session (`ENABLE w h` over a named pipe); the device is enabled while
//!   any lease is open and disabled when the last one closes, including when the app crashes.
use super::{display, driver};
use std::ffi::OsString;
use std::fs::{File, OpenOptions};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::os::windows::io::{AsRawHandle, FromRawHandle};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use windows::core::PCWSTR;
use windows::Win32::Foundation::{
    GetLastError, ERROR_FILE_NOT_FOUND, ERROR_PIPE_CONNECTED, HANDLE,
};
use windows::Win32::Security::Authorization::{
    ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
use windows::Win32::Security::{PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES};
use windows::Win32::Storage::FileSystem::PIPE_ACCESS_DUPLEX;
use windows::Win32::System::Pipes::*;
use windows_service::service::*;
use windows_service::service_control_handler::{
    self, ServiceControlHandlerResult, ServiceStatusHandle,
};
use windows_service::{define_windows_service, service_dispatcher};

pub const NAME: &str = "TabDisplay";
const PIPE: &str = r"\\.\pipe\tabdisplay-service";
/// SYSTEM/admins: full; interactive local users: read/write. Named pipes are
/// local OS objects, so network clients never receive this control surface.
const SDDL: &str = "D:(A;;GA;;;SY)(A;;GA;;;BA)(A;;GRGW;;;IU)";
const MAX_CLIENTS: usize = 8;
const MAX_LINE: usize = 256;
const MAX_COMMANDS_PER_SECOND: u32 = 20;
const READ_PROGRESS_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_MODE: u32 = 7680;
const MAX_PIXELS: u64 = 16_777_216;

// ---- app side -------------------------------------------------------------------------------------------

/// A session's hold on the virtual monitor. The monitor exists while any lease is open.
pub struct Lease {
    pipe: File,
    /// Sizes the driver is known to offer (each request covers both orientations).
    known: Vec<(u32, u32)>,
}

impl Lease {
    /// Makes sure the driver offers `w`x`h`. A new size restarts the driver, so the monitor briefly
    /// unplugs and comes back under a new GDI name. Rotating to a known size costs nothing.
    pub fn ensure_mode(&mut self, w: u32, h: u32) -> io::Result<()> {
        if self.known.iter().any(|&k| k == (w, h) || k == (h, w)) {
            return Ok(());
        }
        call(&self.pipe, &format!("MODE {w} {h}"))?;
        self.known.push((w, h));
        Ok(())
    }
}

/// Whether the service is running (checks its pipe without connecting).
pub fn available() -> bool {
    let name: Vec<u16> = PIPE.encode_utf16().chain([0]).collect();
    unsafe {
        WaitNamedPipeW(PCWSTR(name.as_ptr()), 1).as_bool() || GetLastError() != ERROR_FILE_NOT_FOUND
    }
}

/// Plugs in the virtual monitor, making sure the driver offers `w`x`h`.
pub fn enable(w: u32, h: u32) -> io::Result<Lease> {
    let pipe = request(&format!("ENABLE {w} {h}"))?;
    Ok(Lease {
        pipe,
        known: vec![(w, h)],
    })
}

/// Restarts the driver (e.g. after it crashed). No UAC: the service does it.
pub fn restart() -> io::Result<()> {
    request("RESTART").map(drop)
}

fn request(cmd: &str) -> io::Result<File> {
    let pipe = OpenOptions::new().read(true).write(true).open(PIPE)?;
    call(&pipe, cmd)?;
    Ok(pipe)
}

/// One command, one reply line ("OK" or "ERR <why>").
fn call(mut pipe: &File, cmd: &str) -> io::Result<()> {
    writeln!(pipe, "{cmd}")?;
    let mut reply = String::new();
    BufReader::new(pipe).read_line(&mut reply)?;
    match reply.trim_end() {
        "OK" => Ok(()),
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
/// All mode/driver operations are serialized, but the lease counter is not held
/// over driver I/O. This prevents a slow restart from blocking cleanup forever.
static TOPOLOGY: Mutex<()> = Mutex::new(());
static CLIENTS: AtomicUsize = AtomicUsize::new(0);
static COMMAND_RATE: OnceLock<Mutex<(Instant, u32)>> = OnceLock::new();

fn set_state(state: ServiceState) {
    if let Some(handle) = STATUS.get() {
        let _ = handle.set_service_status(ServiceStatus {
            service_type: ServiceType::OWN_PROCESS,
            current_state: state,
            controls_accepted: if state == ServiceState::Running {
                ServiceControlAccept::STOP | ServiceControlAccept::SHUTDOWN
            } else {
                ServiceControlAccept::empty()
            },
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
    let Ok(handle) = service_control_handler::register(NAME, handler) else {
        return;
    };
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
    if unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            PCWSTR(sddl.as_ptr()),
            SDDL_REVISION_1,
            &mut sd,
            None,
        )
    }
    .is_err()
    {
        return;
    }
    let sa = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: sd.0,
        bInheritHandle: false.into(),
    };
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
        let connected = unsafe { ConnectNamedPipe(pipe, None) }.is_ok()
            || unsafe { GetLastError() } == ERROR_PIPE_CONNECTED;
        let file = unsafe { File::from_raw_handle(pipe.0 as _) }; // closes the handle when dropped
        if connected {
            let accepted = CLIENTS
                .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                    (n < MAX_CLIENTS).then_some(n + 1)
                })
                .is_ok();
            if accepted {
                std::thread::spawn(move || {
                    let _guard = ClientGuard;
                    client(file);
                });
            }
        }
    }
}

struct ClientGuard;

impl Drop for ClientGuard {
    fn drop(&mut self) {
        CLIENTS.fetch_sub(1, Ordering::AcqRel);
    }
}

/// Serves one app connection. A connection that enabled the monitor holds a lease until it closes.
fn client(pipe: File) {
    if set_nonblocking(&pipe).is_err() {
        return;
    }
    let mut out = &pipe;
    let mut holds_lease = false;
    let mut input = BufReader::new(&pipe);
    loop {
        let line = match read_command(&mut input) {
            Ok(Some(line)) => line,
            Ok(None) | Err(_) => break,
        };
        if !allow_command() {
            let _ = writeln!(out, "ERR limite de comandos excedido");
            break;
        }
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

/// Read at most MAX_LINE bytes before the newline. A client cannot make the
/// service allocate an unbounded command string by withholding its newline.
fn set_nonblocking(pipe: &File) -> io::Result<()> {
    let mode = PIPE_NOWAIT;
    unsafe {
        SetNamedPipeHandleState(
            HANDLE(pipe.as_raw_handle() as _),
            Some(&mode as *const _),
            None,
            None,
        )
    }
    .map_err(|error| io::Error::other(error.to_string()))
}

fn read_command<R: Read>(reader: &mut BufReader<R>) -> io::Result<Option<String>> {
    read_command_with_timeout(reader, READ_PROGRESS_TIMEOUT)
}

fn read_command_with_timeout<R: Read>(
    reader: &mut BufReader<R>,
    timeout: Duration,
) -> io::Result<Option<String>> {
    let mut bytes = Vec::with_capacity(MAX_LINE);
    let mut deadline = Instant::now() + timeout;
    loop {
        let mut byte = [0u8; 1];
        match reader.read(&mut byte) {
            Ok(0) => {
                return if bytes.is_empty() {
                    Ok(None)
                } else {
                    Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "comando sem newline",
                    ))
                };
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                if Instant::now() >= deadline {
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "comando sem progresso",
                    ));
                }
                std::thread::sleep(Duration::from_millis(10));
                continue;
            }
            Err(error) => return Err(error),
            Ok(_) => {
                deadline = Instant::now() + timeout;
            }
        }
        if byte[0] == b'\n' {
            return String::from_utf8(bytes)
                .map(Some)
                .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "comando não UTF-8"));
        }
        if bytes.len() >= MAX_LINE {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "comando longo demais",
            ));
        }
        if byte[0] != b'\r' {
            bytes.push(byte[0]);
        }
    }
}

fn allow_command() -> bool {
    let rate = COMMAND_RATE.get_or_init(|| Mutex::new((Instant::now(), 0)));
    let mut rate = rate.lock().unwrap();
    if rate.0.elapsed() >= Duration::from_secs(1) {
        *rate = (Instant::now(), 0);
    }
    if rate.1 >= MAX_COMMANDS_PER_SECOND {
        return false;
    }
    rate.1 += 1;
    true
}

fn handle(line: &str, holds_lease: &mut bool) -> io::Result<()> {
    let mut words = line.split_whitespace();
    let command = words.next();
    let mut dimensions = || {
        let w = words
            .next()
            .and_then(|v| v.parse::<u32>().ok())
            .ok_or_else(|| io::Error::other("esperava: <comando> w h"))?;
        let h = words
            .next()
            .and_then(|v| v.parse::<u32>().ok())
            .ok_or_else(|| io::Error::other("esperava: <comando> w h"))?;
        if w < 16
            || h < 16
            || w > MAX_MODE
            || h > MAX_MODE
            || w % 16 != 0
            || h % 16 != 0
            || (w as u64) * h as u64 > MAX_PIXELS
        {
            return Err(io::Error::other("dimensões fora do limite"));
        }
        if words.next().is_some() {
            return Err(io::Error::other("argumentos extras"));
        }
        Ok((w, h))
    };
    match command {
        Some("MODE") if *holds_lease => {
            let (w, h) = dimensions()?;
            let _topology = TOPOLOGY.lock().unwrap();
            if display::ensure_modes(&[(w, h)])? {
                driver::restart()?;
            }
            Ok(())
        }
        Some("ENABLE") if !*holds_lease => {
            let (w, h) = dimensions()?;
            let _topology = TOPOLOGY.lock().unwrap();
            let leases = *LEASES.lock().unwrap();
            let new_mode = display::ensure_modes(&[(w, h)])?;
            if leases == 0 {
                driver::set_enabled(true)?; // a freshly enabled driver reads the mode list
            } else if new_mode {
                driver::restart()?;
            }
            *LEASES.lock().unwrap() += 1;
            *holds_lease = true;
            Ok(())
        }
        Some("RESTART") => {
            if words.next().is_some() {
                return Err(io::Error::other("argumentos extras"));
            }
            let _topology = TOPOLOGY.lock().unwrap();
            if *LEASES.lock().unwrap() > 0 {
                driver::restart()
            } else {
                driver::set_enabled(false)
            } // also clears Code 43
        }
        _ => Err(io::Error::other(format!("comando desconhecido: {line}"))),
    }
}

fn release() {
    let _topology = TOPOLOGY.lock().unwrap();
    let mut leases = LEASES.lock().unwrap();
    *leases = leases.saturating_sub(1);
    if *leases == 0 {
        let _ = driver::set_enabled(false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{self, Cursor, Read};

    #[test]
    fn command_reader_bounds_long_and_partial_input() {
        let mut long = BufReader::new(Cursor::new(vec![b'x'; MAX_LINE + 1]));
        assert_eq!(
            read_command(&mut long).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );

        let mut partial = BufReader::new(Cursor::new(b"ENABLE 1280 800".to_vec()));
        assert_eq!(
            read_command(&mut partial).unwrap_err().kind(),
            io::ErrorKind::UnexpectedEof
        );

        let mut valid = BufReader::new(Cursor::new(b"RESTART\r\n".to_vec()));
        assert_eq!(
            read_command(&mut valid).unwrap().as_deref(),
            Some("RESTART")
        );
    }

    #[test]
    fn invalid_dimensions_and_extra_arguments_stop_before_driver() {
        let mut lease = false;
        assert!(handle("ENABLE 0 800", &mut lease).is_err());
        assert!(handle("ENABLE 1280 800 extra", &mut lease).is_err());
        assert!(handle("RESTART extra", &mut lease).is_err());
        assert!(!lease);
    }

    struct NeverReady;

    impl Read for NeverReady {
        fn read(&mut self, _buf: &mut [u8]) -> io::Result<usize> {
            Err(io::ErrorKind::WouldBlock.into())
        }
    }

    #[test]
    fn slow_peer_times_out_without_growing_or_holding_the_command() {
        let mut reader = BufReader::new(NeverReady);
        assert_eq!(
            read_command_with_timeout(&mut reader, Duration::from_millis(1))
                .unwrap_err()
                .kind(),
            io::ErrorKind::TimedOut
        );
    }
}
