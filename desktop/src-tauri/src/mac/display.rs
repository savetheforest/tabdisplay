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
use std::sync::Mutex;

/// Offered in the UI (the tablet's own size is always available too).
pub const PRESETS: &[(u32, u32)] = &[(1280, 800), (1920, 1200), (2560, 1600), (1920, 1080), (2560, 1440)];

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
}

unsafe extern "C" {
    fn dispatch_get_global_queue(identifier: isize, flags: usize) -> *mut AnyObject;
}

/// The live virtual monitor's CGDirectDisplayID.
static CURRENT: Mutex<Option<u32>> = Mutex::new(None);

pub struct VirtualDisplay {
    /// CGDirectDisplayID as text (what capture::open takes).
    pub device: String,
    id: u32,
    display: *mut AnyObject,
}

// The object is only created, configured and released by the session thread that owns it.
unsafe impl Send for VirtualDisplay {}

impl Drop for VirtualDisplay {
    fn drop(&mut self) {
        unsafe {
            let _: () = msg_send![self.display, release];
        }
        *CURRENT.lock().unwrap() = None;
    }
}

/// macOS needs nothing installed, only the Screen Recording and Accessibility permissions.
pub fn driver_state() -> &'static str {
    match super::permissions::granted() {
        (true, true) => "ok",
        _ => "sem-permissao",
    }
}

/// Id of the virtual monitor, if one is up (so the UI can leave it out of the mirror list).
pub fn find_device() -> Option<String> {
    CURRENT.lock().unwrap().map(|id| id.to_string())
}

fn class(name: &CStr) -> io::Result<&'static AnyClass> {
    AnyClass::get(name).ok_or_else(|| io::Error::other("monitor virtual indisponível nesta versão do macOS"))
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
        let _: () = msg_send![desc, setSerialNum: 0x0001u32];

        let alloc: *mut AnyObject = msg_send![class(c"CGVirtualDisplay")?, alloc];
        let display: *mut AnyObject = msg_send![alloc, initWithDescriptor: desc];
        let _: () = msg_send![desc, release];
        if display.is_null() {
            return Err(io::Error::other("o macOS não criou o monitor virtual"));
        }
        let id: u32 = msg_send![display, displayID];
        *CURRENT.lock().unwrap() = Some(id);
        let mut vd = VirtualDisplay { device: id.to_string(), id, display };
        vd.configure(w, h, hz, pos)?;
        Ok(vd)
    }
}

impl VirtualDisplay {
    /// Changes size/refresh/position in place (rotation, profile changes); the monitor stays plugged in.
    pub fn configure(&mut self, w: u32, h: u32, hz: u32, pos: Position) -> io::Result<()> {
        unsafe {
            let settings: *mut AnyObject = msg_send![class(c"CGVirtualDisplaySettings")?, new];
            // ponytail: 1x (pixels = points) keeps capture and input simple; a HiDPI option would render
            // at 2x for sharper text at the cost of smaller content.
            let _: () = msg_send![settings, setHiDPI: 0u32];
            let mode_alloc: *mut AnyObject = msg_send![class(c"CGVirtualDisplayMode")?, alloc];
            let mode: *mut AnyObject = msg_send![mode_alloc, initWithWidth: w as usize, height: h as usize, refreshRate: hz as f64];
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
                if b.size.width as u32 == w && b.size.height as u32 == h {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(40));
            }
            let main = CGDisplayBounds(CGMainDisplayID());
            let (mw, mh) = (main.size.width as i32, main.size.height as i32);
            let (x, y) = match pos {
                Position::Right => (mw, 0),
                Position::Left => (-(w as i32), 0),
                Position::Above => (0, -(h as i32)),
                Position::Below => (0, mh),
            };
            let mut config = std::ptr::null_mut();
            if CGBeginDisplayConfiguration(&mut config) == 0 {
                CGConfigureDisplayOrigin(config, self.id, x, y);
                CGCompleteDisplayConfiguration(config, 1); // kCGConfigureForSession
            }
        }
        Ok(())
    }
}
