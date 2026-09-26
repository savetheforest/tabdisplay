//! Virtual monitor via the Virtual Display Driver (github.com/VirtualDrivers/Virtual-Display-Driver).
//! The TabDisplay service (service.rs) keeps the driver's device disabled while no tablet is connected and
//! enables it for a session; here we attach the monitor it plugs in, at the right mode and position.
//! The driver reads its mode list (XML) only when it starts, and its own pipe commands that reload it crash
//! it (mttvdd 25.7: access violation, then Code 43), so we never use those.
use super::service::{self, Lease};
use crate::settings::Position;
use std::fs;
use std::io;
use std::sync::Mutex;
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
/// Virtual monitors the driver is asked to offer: one per tablet that can be connected at once.
pub const MAX_TABLETS: usize = 2;
/// GDI names of the virtual monitors sessions currently use, so two sessions never share one.
static CLAIMED: Mutex<Vec<String>> = Mutex::new(Vec::new());
/// With more modes than about this (resolutions x refresh rates) the driver plugs in no monitor at all.
const MAX_REFRESH: u32 = 120;

/// The attached virtual monitor. Dropping it detaches the monitor, then releases the service lease,
/// which unplugs it.
pub struct VirtualDisplay {
    /// GDI name like `\\.\DISPLAY5`, matches `DXGI_OUTPUT_DESC::DeviceName`.
    pub device: String,
    lease: Option<Lease>,
    /// What `configure` last applied: repeating it would make Windows re-set the mode, which drops every other
    /// session's screen capture (ACCESS_LOST) and starts a rebuild ping-pong between tablets.
    applied: Option<(u32, u32, u32, Position)>,
}

impl Drop for VirtualDisplay {
    fn drop(&mut self) {
        CLAIMED.lock().unwrap().retain(|d| d != &self.device);
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
    let mut xml = with_count(&trim(xml));
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

/// Sets how many virtual monitors the driver offers (`<count>`), if the file has that setting.
fn with_count(xml: &str) -> String {
    let (open, close) = ("<count>", "</count>");
    let Some(start) = xml.find(open).map(|i| i + open.len()) else { return xml.to_string() };
    let Some(end) = xml[start..].find(close).map(|i| start + i) else { return xml.to_string() };
    format!("{}{}{}", &xml[..start], MAX_TABLETS, &xml[end..])
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
    let mut display = VirtualDisplay { device: String::new(), lease, applied: None };
    display.configure(w, h, hz, pos)?;
    Ok(display)
}

impl VirtualDisplay {
    /// Changes size/refresh/position in place: the monitor stays plugged in (windows on it stay put),
    /// unless the size is new to the driver, which then restarts.
    pub fn configure(&mut self, w: u32, h: u32, hz: u32, pos: Position) -> io::Result<()> {
        if let Some(lease) = &mut self.lease {
            lease.ensure_mode(w, h)?;
        }
        let deadline = Instant::now() + Duration::from_secs(10);
        self.device = loop {
            match self.pick_device() {
                Some(d) => break d,
                None if Instant::now() < deadline && self.lease.is_some() => sleep(Duration::from_millis(200)),
                None => return Err(io::Error::other("monitor virtual indisponível (serviço do TabDisplay parado ou todos em uso?)")),
            }
        };
        // Next to everything else on the desktop (other tablets' monitors included), not just the primary.
        if self.applied == Some((w, h, hz, pos)) && current_size(&self.device) == Some((w, h)) {
            return Ok(()); // already in this mode
        }
        let (l, t, r, b) = desktop_bounds_without(&self.device);
        let (x, y) = match pos {
            Position::Right => (r, 0),
            Position::Left => (l - w as i32, 0),
            Position::Above => (0, t - h as i32),
            Position::Below => (0, b),
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
        if apply(&self.device, mode) != DISP_CHANGE_SUCCESSFUL && hz != 60 {
            mode.dmDisplayFrequency = 60; // refresh rate not offered: fall back
            apply(&self.device, mode);
        }
        if current_size(&self.device) != Some((w, h)) {
            return Err(io::Error::other(format!("{w}x{h} indisponível no monitor virtual")));
        }
        self.applied = Some((w, h, hz, pos));
        Ok(())
    }
}

impl VirtualDisplay {
    /// Keeps the monitor this session already has, else claims a plugged-in one no other session uses.
    /// (GDI names change when the driver restarts, hence the re-check.)
    fn pick_device(&mut self) -> Option<String> {
        let mut claimed = CLAIMED.lock().unwrap();
        let all = find_devices();
        if all.contains(&self.device) {
            return Some(self.device.clone());
        }
        claimed.retain(|d| d != &self.device); // stale name
        self.applied = None;
        let free = all.into_iter().find(|d| !claimed.contains(d))?;
        claimed.push(free.clone());
        Some(free)
    }
}

/// Bounding box (left, top, right, bottom) of every monitor on the desktop except `device`. With no other
/// monitor, the primary's rect (0, 0, w, h).
fn desktop_bounds_without(device: &str) -> (i32, i32, i32, i32) {
    let mut bounds: Option<(i32, i32, i32, i32)> = None;
    unsafe {
        let mut adapter = DISPLAY_DEVICEW { cb: size_of::<DISPLAY_DEVICEW>() as u32, ..Default::default() };
        let mut i = 0;
        while EnumDisplayDevicesW(PCWSTR::null(), i, &mut adapter, 0).as_bool() {
            i += 1;
            let name = from_wide(&adapter.DeviceName);
            if name == device || adapter.StateFlags & DISPLAY_DEVICE_ATTACHED_TO_DESKTOP != DISPLAY_DEVICE_ATTACHED_TO_DESKTOP {
                continue;
            }
            let wide_name = wide(&name);
            let mut mode = DEVMODEW { dmSize: size_of::<DEVMODEW>() as u16, ..Default::default() };
            if !EnumDisplaySettingsW(PCWSTR(wide_name.as_ptr()), ENUM_CURRENT_SETTINGS, &mut mode).as_bool() {
                continue;
            }
            let p = mode.Anonymous1.Anonymous2.dmPosition;
            let rect = (p.x, p.y, p.x + mode.dmPelsWidth as i32, p.y + mode.dmPelsHeight as i32);
            bounds = Some(match bounds {
                None => rect,
                Some(b) => (b.0.min(rect.0), b.1.min(rect.1), b.2.max(rect.2), b.3.max(rect.3)),
            });
        }
    }
    bounds.unwrap_or_else(|| unsafe { (0, 0, GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN)) })
}

/// GDI name of a virtual monitor (attached or not), if one is plugged in.
pub fn find_device() -> Option<String> {
    find_devices().into_iter().next()
}

/// GDI names of every plugged-in virtual monitor (attached or not).
pub fn find_devices() -> Vec<String> {
    let mut found = Vec::new();
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
                found.push(from_wide(&adapter.DeviceName));
            }
        }
    }
    found
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
    fn sets_monitor_count() {
        let xml = "<monitors>
<count>1</count>
</monitors>
<resolutions></resolutions>";
        assert!(with_modes(xml, &[]).unwrap().contains(&format!("<count>{MAX_TABLETS}</count>")));
    }

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
