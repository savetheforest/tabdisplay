//! Virtual monitor via the Virtual Display Driver (github.com/VirtualDrivers/Virtual-Display-Driver).
//! Its pipe commands that reload the driver crash it (mttvdd 25.7: access violation, then Code 43),
//! so we never reload: the mode list is written to its XML, which the driver reads when it starts,
//! and at runtime we only attach/detach the monitor and switch modes with ChangeDisplaySettingsEx.
use crate::settings::Position;
use std::fs;
use std::io;
use std::process::Command;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{GetLastError, ERROR_FILE_NOT_FOUND, POINTL};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::Pipes::WaitNamedPipeW;
use windows::Win32::UI::WindowsAndMessaging::*;

const SETTINGS: &str = r"C:\VirtualDisplayDriver\vdd_settings.xml";
/// Substring of the virtual monitor's device interface path.
const MONITOR_ID: &str = "MTT1337";
/// Offered in the UI; written to the driver in both orientations.
pub const PRESETS: &[(u32, u32)] = &[(1280, 800), (1920, 1200), (2560, 1600), (1920, 1080), (2560, 1440)];

/// Detaches the virtual monitor from the desktop when dropped.
pub struct VirtualDisplay {
    /// GDI name like `\\.\DISPLAY5`, matches `DXGI_OUTPUT_DESC::DeviceName`.
    pub device: String,
}

impl Drop for VirtualDisplay {
    fn drop(&mut self) {
        let _ = apply(&self.device, DEVMODEW {
            dmSize: size_of::<DEVMODEW>() as u16,
            dmFields: DM_POSITION | DM_PELSWIDTH | DM_PELSHEIGHT, // all zero = detach
            ..Default::default()
        });
    }
}

/// "ausente" (not installed), "ok", or "parado" (installed but not running, e.g. Code 43).
/// Windows keeps listing a crashed driver's monitor, so "running" means its pipe exists.
pub fn driver_state() -> &'static str {
    match (fs::metadata(SETTINGS).is_ok(), driver_running()) {
        (false, _) => "ausente",
        (true, true) => "ok",
        (true, false) => "parado",
    }
}

fn driver_running() -> bool {
    let name = wide(r"\\.\pipe\MTTVirtualDisplayPipe");
    // Doesn't connect; fails with FILE_NOT_FOUND only if no such pipe exists.
    unsafe { WaitNamedPipeW(PCWSTR(name.as_ptr()), 1).as_bool() || GetLastError() != ERROR_FILE_NOT_FOUND }
}

fn entry(w: u32, h: u32) -> String {
    format!("<resolution><width>{w}</width><height>{h}</height><refresh_rate>60</refresh_rate></resolution>")
}

/// Adds the presets and `extra` (both orientations) to the driver's mode list.
/// Returns true if anything was added; the driver only sees it after a restart.
pub fn ensure_modes(extra: &[(u32, u32)]) -> io::Result<bool> {
    let Ok(xml) = fs::read_to_string(SETTINGS) else { return Ok(false) };
    let mut added = String::new();
    for &(w, h) in PRESETS.iter().chain(extra) {
        for e in [entry(w, h), entry(h, w)] {
            if !xml.contains(&e) && !added.contains(&e) {
                added += &e;
            }
        }
    }
    if added.is_empty() {
        return Ok(false);
    }
    let i = xml.find("<resolutions>").ok_or_else(|| io::Error::other("vdd_settings.xml sem <resolutions>"))?;
    let i = i + "<resolutions>".len();
    fs::write(SETTINGS, format!("{}{added}{}", &xml[..i], &xml[i..]))?;
    Ok(true)
}

/// Restarts the driver through `tabdisplay.exe --restart-driver` behind a UAC prompt; waits for it.
pub fn restart_driver() -> io::Result<()> {
    let exe = std::env::current_exe()?.display().to_string().replace('\'', "''");
    let ps = format!("Start-Process -FilePath '{exe}' -ArgumentList '--restart-driver' -Verb RunAs -Wait");
    let mut cmd = Command::new("powershell");
    cmd.args(["-NoProfile", "-Command", &ps]);
    std::os::windows::process::CommandExt::creation_flags(&mut cmd, 0x0800_0000); // CREATE_NO_WINDOW
    cmd.status()?;
    Ok(())
}

/// Attaches the virtual monitor at `w`x`h`@`hz`, next to the primary monitor.
pub fn attach(w: u32, h: u32, hz: u32, pos: Position) -> io::Result<VirtualDisplay> {
    let device = find_device().ok_or_else(|| io::Error::other("driver de monitor virtual parado"))?;
    let (pw, ph) = unsafe { (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN)) };
    let (x, y) = match pos {
        Position::Right => (pw, 0),
        Position::Left => (-(w as i32), 0),
        Position::Above => (0, -(h as i32)),
        Position::Below => (0, ph),
    };
    let mut mode = DEVMODEW {
        dmSize: size_of::<DEVMODEW>() as u16,
        dmPelsWidth: w,
        dmPelsHeight: h,
        dmDisplayFrequency: hz,
        dmFields: DM_POSITION | DM_PELSWIDTH | DM_PELSHEIGHT | DM_DISPLAYFREQUENCY,
        ..Default::default()
    };
    mode.Anonymous1.Anonymous2.dmPosition = POINTL { x, y };
    if apply(&device, mode) != DISP_CHANGE_SUCCESSFUL && hz != 60 {
        mode.dmDisplayFrequency = 60; // refresh rate not offered: fall back
        apply(&device, mode);
    }
    let display = VirtualDisplay { device };
    if current_size(&display.device) != Some((w, h)) {
        return Err(io::Error::other(format!("{w}x{h} indisponível no driver: clique em Reiniciar driver")));
    }
    Ok(display)
}

/// GDI name of the virtual monitor (attached or not), if the driver is running.
pub fn find_device() -> Option<String> {
    unsafe {
        let mut adapter = DISPLAY_DEVICEW { cb: size_of::<DISPLAY_DEVICEW>() as u32, ..Default::default() };
        let mut i = 0;
        while EnumDisplayDevicesW(PCWSTR::null(), i, &mut adapter, 0).as_bool() {
            i += 1;
            let name = wide(&from_wide(&adapter.DeviceName));
            let mut monitor = DISPLAY_DEVICEW { cb: size_of::<DISPLAY_DEVICEW>() as u32, ..Default::default() };
            if EnumDisplayDevicesW(PCWSTR(name.as_ptr()), 0, &mut monitor, EDD_GET_DEVICE_INTERFACE_NAME).as_bool()
                && from_wide(&monitor.DeviceID).contains(MONITOR_ID)
            {
                return Some(from_wide(&adapter.DeviceName));
            }
        }
        None
    }
}

fn current_size(device: &str) -> Option<(u32, u32)> {
    let name = wide(device);
    let mut mode = DEVMODEW { dmSize: size_of::<DEVMODEW>() as u16, ..Default::default() };
    unsafe { EnumDisplaySettingsW(PCWSTR(name.as_ptr()), ENUM_CURRENT_SETTINGS, &mut mode).as_bool() }
        .then_some((mode.dmPelsWidth, mode.dmPelsHeight))
}

/// Stages `mode` for `device` and applies it.
fn apply(device: &str, mut mode: DEVMODEW) -> DISP_CHANGE {
    let name = wide(device);
    unsafe {
        let r = ChangeDisplaySettingsExW(PCWSTR(name.as_ptr()), Some(&mut mode), None, CDS_UPDATEREGISTRY | CDS_NORESET, None);
        if r != DISP_CHANGE_SUCCESSFUL {
            return r;
        }
        ChangeDisplaySettingsExW(PCWSTR::null(), None, None, CDS_TYPE(0), None)
    }
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain([0]).collect()
}

fn from_wide(s: &[u16]) -> String {
    String::from_utf16_lossy(&s[..s.iter().position(|&c| c == 0).unwrap_or(s.len())])
}
