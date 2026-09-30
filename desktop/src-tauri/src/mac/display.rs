//! Virtual monitor via CoreGraphics' private CGVirtualDisplay (what DeskPad and BetterDisplay use).
//! The monitor exists exactly while the CGVirtualDisplay object lives: no driver and no service, so there
//! is never a stray second screen while no tablet is connected.
//! ponytail: private API, may change between macOS versions; check it on each major macOS release.
use crate::settings::Position;
use objc2::encode::{Encode, Encoding};
use objc2::msg_send;
use objc2::runtime::{AnyClass, AnyObject, Bool};
use std::ffi::{c_void, CStr};
use std::io;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

/// Offered in the UI (the tablet's own size is always available too).
pub const PRESETS: &[(u32, u32)] = &[
    (1280, 800),
    (1920, 1200),
    (2560, 1600),
    (1920, 1080),
    (2560, 1440),
];

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct CGPoint {
    pub x: f64,
    pub y: f64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct CGSize {
    pub width: f64,
    pub height: f64,
}

unsafe impl Encode for CGSize {
    const ENCODING: Encoding = Encoding::Struct("CGSize", &[Encoding::Double, Encoding::Double]);
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct CGRect {
    pub origin: CGPoint,
    pub size: CGSize,
}

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    pub fn CGMainDisplayID() -> u32;
    pub fn CGDisplayBounds(display: u32) -> CGRect;
    fn CGBeginDisplayConfiguration(config: *mut *mut c_void) -> i32;
    fn CGConfigureDisplayOrigin(config: *mut c_void, display: u32, x: i32, y: i32) -> i32;
    fn CGCompleteDisplayConfiguration(config: *mut c_void, option: u32) -> i32;
    fn CGConfigureDisplayWithDisplayMode(
        config: *mut c_void,
        display: u32,
        mode: *const c_void,
        options: *const c_void,
    ) -> i32;
    fn CGDisplayCopyAllDisplayModes(display: u32, options: *const c_void) -> *const c_void;
    fn CGDisplayModeGetWidth(mode: *const c_void) -> usize;
    fn CGDisplayModeGetPixelWidth(mode: *const c_void) -> usize;
    fn CGDisplayModeGetPixelHeight(mode: *const c_void) -> usize;
    static kCGDisplayShowDuplicateLowResolutionModes: *const c_void;
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    static kCFBooleanTrue: *const c_void;
    static kCFTypeDictionaryKeyCallBacks: c_void;
    static kCFTypeDictionaryValueCallBacks: c_void;
    fn CFDictionaryCreate(
        allocator: *const c_void,
        keys: *const *const c_void,
        values: *const *const c_void,
        count: isize,
        key_cb: *const c_void,
        value_cb: *const c_void,
    ) -> *const c_void;
    fn CFArrayGetCount(array: *const c_void) -> isize;
    fn CFArrayGetValueAtIndex(array: *const c_void, index: isize) -> *const c_void;
    fn CFRelease(cf: *const c_void);
}

/// Runs `f` with the display's mode that is `pw` points wide and `w`x`h` pixels (the Retina variant),
/// if macOS offers one. The low-resolution duplicates are only listed when asked for.
unsafe fn with_retina_mode(id: u32, pw: u32, w: u32, h: u32, f: impl FnOnce(*const c_void)) {
    unsafe {
        let options = CFDictionaryCreate(
            std::ptr::null(),
            &kCGDisplayShowDuplicateLowResolutionModes,
            &kCFBooleanTrue,
            1,
            &kCFTypeDictionaryKeyCallBacks,
            &kCFTypeDictionaryValueCallBacks,
        );
        let modes = CGDisplayCopyAllDisplayModes(id, options);
        CFRelease(options);
        if modes.is_null() {
            return;
        }
        let found = (0..CFArrayGetCount(modes))
            .map(|i| CFArrayGetValueAtIndex(modes, i))
            .find(|&m| {
                CGDisplayModeGetWidth(m) == pw as usize
                    && CGDisplayModeGetPixelWidth(m) == w as usize
                    && CGDisplayModeGetPixelHeight(m) == h as usize
            });
        if let Some(mode) = found {
            f(mode);
        }
        CFRelease(modes);
    }
}

unsafe extern "C" {
    fn dispatch_get_global_queue(identifier: isize, flags: usize) -> *mut AnyObject;
}

/// The live virtual monitors' CGDirectDisplayIDs (one per connected tablet).
static CURRENT: Mutex<Vec<u32>> = Mutex::new(Vec::new());
/// Gives each virtual monitor its own serial number, so macOS doesn't take two tablets for one screen.
static SERIAL: AtomicU32 = AtomicU32::new(1);

pub struct VirtualDisplay {
    /// CGDirectDisplayID as text (what capture::open takes).
    pub device: String,
    id: u32,
    display: *mut AnyObject,
    /// What `configure` last applied; repeating it would reconfigure the desktop for the other tablets too.
    applied: Option<(u32, u32, u32, Position)>,
}

// The object is only created, configured and released by the session thread that owns it.
unsafe impl Send for VirtualDisplay {}

impl Drop for VirtualDisplay {
    fn drop(&mut self) {
        unsafe {
            let _: () = msg_send![self.display, release];
        }
        CURRENT.lock().unwrap().retain(|&id| id != self.id);
    }
}

/// macOS needs nothing installed, only the Screen Recording and Accessibility permissions.
pub fn driver_state() -> &'static str {
    match super::permissions::granted() {
        (true, true) => "ok",
        _ => "sem-permissao",
    }
}

/// Ids of the virtual monitors that are up (so the UI can leave them out of the mirror list).
pub fn find_devices() -> Vec<String> {
    CURRENT.lock().unwrap().iter().map(u32::to_string).collect()
}

fn class(name: &CStr) -> io::Result<&'static AnyClass> {
    AnyClass::get(name)
        .ok_or_else(|| io::Error::other("monitor virtual indisponível nesta versão do macOS"))
}

unsafe fn nsstring(s: &CStr) -> io::Result<*mut AnyObject> {
    Ok(unsafe { msg_send![class(c"NSString")?, stringWithUTF8String: s.as_ptr()] })
}

/// Creates the virtual monitor at `w`x`h`@`hz`, next to the main display.
pub fn attach(w: u32, h: u32, hz: u32, pos: Position) -> io::Result<VirtualDisplay> {
    unsafe {
        let desc: *mut AnyObject = msg_send![class(c"CGVirtualDisplayDescriptor")?, new];
        let _: () = msg_send![desc, setQueue: dispatch_get_global_queue(0, 0)];
        let _: () = msg_send![desc, setName: nsstring(c"TabDisplay")?];
        // Big enough for either orientation, so rotating is just a mode change.
        let side = w.max(h);
        let _: () = msg_send![desc, setMaxPixelsWide: side];
        let _: () = msg_send![desc, setMaxPixelsHigh: side];
        // An 11" tablet, so macOS reasons about it like a real screen of that size.
        let _: () = msg_send![desc, setSizeInMillimeters: CGSize { width: 250.0, height: 156.0 }];
        let _: () = msg_send![desc, setVendorID: 0x5444u32];
        let _: () = msg_send![desc, setProductID: 0x0001u32];
        let _: () = msg_send![desc, setSerialNum: SERIAL.fetch_add(1, Ordering::Relaxed)];

        let alloc: *mut AnyObject = msg_send![class(c"CGVirtualDisplay")?, alloc];
        let display: *mut AnyObject = msg_send![alloc, initWithDescriptor: desc];
        let _: () = msg_send![desc, release];
        if display.is_null() {
            return Err(io::Error::other("o macOS não criou o monitor virtual"));
        }
        let id: u32 = msg_send![display, displayID];
        CURRENT.lock().unwrap().push(id);
        let mut vd = VirtualDisplay {
            device: id.to_string(),
            id,
            display,
            applied: None,
        };
        vd.configure(w, h, hz, pos)?;
        Ok(vd)
    }
}

impl VirtualDisplay {
    /// Changes size/refresh/position in place (rotation, profile changes); the monitor stays plugged in.
    /// `w`x`h` are pixels; the monitor is Retina (HiDPI): `w/2`x`h/2` points drawn at 2x, like a real
    /// 11" tablet screen. (Measured: without HiDPI macOS halves the mode anyway, to 1x pixels.)
    pub fn configure(&mut self, w: u32, h: u32, hz: u32, pos: Position) -> io::Result<()> {
        if self.applied == Some((w, h, hz, pos)) {
            return Ok(());
        }
        let (pw, ph) = (w / 2, h / 2);
        unsafe {
            let settings: *mut AnyObject = msg_send![class(c"CGVirtualDisplaySettings")?, new];
            let _: () = msg_send![settings, setHiDPI: 1u32];
            let mode_alloc: *mut AnyObject = msg_send![class(c"CGVirtualDisplayMode")?, alloc];
            let mode: *mut AnyObject = msg_send![mode_alloc, initWithWidth: pw as usize, height: ph as usize, refreshRate: hz as f64];
            let modes: *mut AnyObject = msg_send![class(c"NSArray")?, arrayWithObject: mode];
            let _: () = msg_send![settings, setModes: modes];
            let applied: Bool = msg_send![self.display, applySettings: settings];
            let _: () = msg_send![mode, release];
            let _: () = msg_send![settings, release];
            if !applied.as_bool() {
                return Err(io::Error::other(format!("o macOS recusou {w}x{h}")));
            }

            // The mode switch lands asynchronously; wait for it before placing the monitor.
            for _ in 0..50 {
                let b = CGDisplayBounds(self.id);
                if b.size.width as u32 == pw && b.size.height as u32 == ph {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(40));
            }
            if std::env::var_os("TABDISPLAY_DEBUG").is_some() {
                let b = CGDisplayBounds(self.id);
                eprintln!(
                    "virtual display {} asked {w}x{h}@{hz} px, bounds {}x{} pt, pixels {:?}",
                    self.id,
                    b.size.width,
                    b.size.height,
                    super::capture::pixel_size(self.id)
                );
            }
            // Next to the main display and the other tablets' monitors.
            let (mut l, mut t, mut r, mut b) = {
                let m = CGDisplayBounds(CGMainDisplayID());
                (
                    m.origin.x as i32,
                    m.origin.y as i32,
                    (m.origin.x + m.size.width) as i32,
                    (m.origin.y + m.size.height) as i32,
                )
            };
            for &other in CURRENT.lock().unwrap().iter().filter(|&&o| o != self.id) {
                let o = CGDisplayBounds(other);
                (l, t) = (l.min(o.origin.x as i32), t.min(o.origin.y as i32));
                (r, b) = (
                    r.max((o.origin.x + o.size.width) as i32),
                    b.max((o.origin.y + o.size.height) as i32),
                );
            }
            let (x, y) = match pos {
                Position::Right => (r, 0),
                Position::Left => (l - pw as i32, 0),
                Position::Above => (0, t - ph as i32),
                Position::Below => (0, b),
            };
            let mut config = std::ptr::null_mut();
            if CGBeginDisplayConfiguration(&mut config) == 0 {
                // macOS starts the monitor in the 1x variant of the mode; pick the 2x (Retina) one.
                with_retina_mode(self.id, pw, w, h, |mode| {
                    CGConfigureDisplayWithDisplayMode(config, self.id, mode, std::ptr::null());
                });
                CGConfigureDisplayOrigin(config, self.id, x, y);
                CGCompleteDisplayConfiguration(config, 1); // kCGConfigureForSession
            }
            if std::env::var_os("TABDISPLAY_DEBUG").is_some() {
                eprintln!(
                    "after retina switch: pixels {:?}",
                    super::capture::pixel_size(self.id)
                );
            }
        }
        self.applied = Some((w, h, hz, pos));
        Ok(())
    }
}
