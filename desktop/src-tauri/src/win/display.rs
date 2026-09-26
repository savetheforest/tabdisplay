//! Virtual monitor via the Virtual Display Driver (github.com/VirtualDrivers/Virtual-Display-Driver).
//! The TabDisplay service (service.rs) keeps the driver's device disabled while no tablet is connected and
//! enables it for a session; here we attach the monitor it plugs in, at the right mode and position.
//! The driver reads its mode list (XML) only when it starts, and its own pipe commands that reload it crash
//! it (mttvdd 25.7: access violation, then Code 43), so we never use those.
use super::service::{self, Lease};
use crate::settings::Position;
use std::fs;
use std::io;
use std::thread::sleep;
use std::time::{Duration, Instant};
use windows::core::PCWSTR;
use windows::Win32::Foundation::POINTL;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::WindowsAndMessaging::*;

const SETTINGS: &str = r"C:\VirtualDisplayDriver\vdd_settings.xml";
/// Substring of the virtual monitor's device interface path.
const MONITOR_ID: &str = "MTT1337";
/// Offered in the UI; written to the driver in both orientations.
pub const PRESETS: &[(u32, u32)] = &[(1280, 800), (1920, 1200), (2560, 1600), (1920, 1080), (2560, 1440)];
/// With more modes than about this (resolutions x refresh rates) the driver plugs in no monitor at all.
const MAX_REFRESH: u32 = 120;

/// The attached virtual monitor. Dropping it detaches the monitor, then releases the service lease,
/// which unplugs it.
pub struct VirtualDisplay {
    /// GDI name like `\\.\DISPLAY5`, matches `DXGI_OUTPUT_DESC::DeviceName`.
    pub device: String,
    _lease: Option<Lease>,
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

/// "ok" (service running), "sem-servico" (driver installed, service not running) or "ausente".
pub fn driver_state() -> &'static str {
    match (fs::metadata(SETTINGS).is_ok(), service::available()) {
        (false, _) => "ausente",
        (true, true) => "ok",
        (true, false) => "sem-servico",
    }
}

fn entry(w: u32, h: u32) -> String {
    format!("<resolution><width>{w}</width><height>{h}</height><refresh_rate>60</refresh_rate></resolution>")
}

/// Makes the driver's mode list hold the presets and `extra` (both orientations) and stay small enough
/// for the driver. Returns true if the file changed; the driver only sees it after it (re)starts.
/// Needs write access to the driver folder: the service and the installer call this, not the app.
pub fn ensure_modes(extra: &[(u32, u32)]) -> io::Result<bool> {
    let Ok(xml) = fs::read_to_string(SETTINGS) else { return Ok(false) };
    let new = with_modes(&xml, extra)?;
    if new == xml {
        return Ok(false);
    }
    fs::write(SETTINGS, new)?;
    Ok(true)
}

fn with_modes(xml: &str, extra: &[(u32, u32)]) -> io::Result<String> {
    let mut xml = trim(xml);
    let mut added = String::new();
    for &(w, h) in PRESETS.iter().chain(extra) {
        for e in [entry(w, h), entry(h, w)] {
            if !xml.contains(&e) && !added.contains(&e) {
                added += &e;
            }
        }
    }
    let i = xml.find("<resolutions>").ok_or_else(|| io::Error::other("vdd_settings.xml sem <resolutions>"))? + "<resolutions>".len();
    xml.insert_str(i, &added);
    Ok(xml)
}

/// Drops the driver's stock 30 Hz resolutions and global refresh rates above MAX_REFRESH.
fn trim(xml: &str) -> String {
    let mut out = String::with_capacity(xml.len());
    let mut rest = xml;
    while let Some(start) = rest.find("<resolution>") {
        let end = rest[start..].find("</resolution>").map_or(rest.len(), |e| start + e + "</resolution>".len());
        out.push_str(&rest[..start]);
        if !rest[start..end].contains("<refresh_rate>30</refresh_rate>") {
            out.push_str(&rest[start..end]);
        }
        rest = &rest[end..];
    }
    out.push_str(rest);
    out.lines()
        .filter(|l| {
            let l = l.trim();
            let hz = l.strip_prefix("<g_refresh_rate>").and_then(|v| v.strip_suffix("</g_refresh_rate>"));
            hz.and_then(|v| v.parse::<u32>().ok()).is_none_or(|hz| hz <= MAX_REFRESH)
        })
        .map(|l| format!("{l}\n"))
        .collect()
}

/// Plugs in (through the service) and attaches the virtual monitor at `w`x`h`@`hz`, next to the primary.
pub fn attach(w: u32, h: u32, hz: u32, pos: Position) -> io::Result<VirtualDisplay> {
    // Without the service (e.g. a dev build) fall back to a driver that's already enabled.
    let lease = if service::available() { Some(service::enable(w, h)?) } else { None };
    let deadline = Instant::now() + Duration::from_secs(10);
    let device = loop {
        match find_device() {
            Some(d) => break d,
            None if Instant::now() < deadline && lease.is_some() => sleep(Duration::from_millis(200)),
            None => return Err(io::Error::other("monitor virtual indisponível (serviço do TabDisplay parado?)")),
        }
    };
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
    let display = VirtualDisplay { device, _lease: lease };
    if current_size(&display.device) != Some((w, h)) {
        return Err(io::Error::other(format!("{w}x{h} indisponível no monitor virtual")));
    }
    Ok(display)
}

/// GDI name of the virtual monitor (attached or not), if it is plugged in.
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

#[cfg(test)]
mod tests {
    use super::*;

    const STOCK: &str = "<vdd_settings>\n<global>\n<g_refresh_rate>60</g_refresh_rate>\n<g_refresh_rate>144</g_refresh_rate>\n</global>\n<resolutions>\n<resolution>\n<width>800</width>\n<height>600</height>\n<refresh_rate>30</refresh_rate>\n</resolution>\n</resolutions>\n</vdd_settings>\n";

    #[test]
    fn trims_stock_modes_and_adds_ours_once() {
        let xml = with_modes(STOCK, &[(2304, 1440)]).unwrap();
        assert!(!xml.contains("<height>600</height>")); // stock 800x600@30
        assert!(!xml.contains("<g_refresh_rate>144"));
        assert!(xml.contains("<g_refresh_rate>60</g_refresh_rate>"));
        assert!(xml.contains(&entry(2304, 1440)) && xml.contains(&entry(1440, 2304)));
        assert_eq!(with_modes(&xml, &[(2304, 1440)]).unwrap(), xml); // idempotent: no needless driver restarts
    }
}
