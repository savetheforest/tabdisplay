//! macOS privacy permissions TabDisplay needs, requested at launch so the prompts appear together:
//! Screen Recording (capture the virtual monitor) and Accessibility (turn tablet touches into clicks).
//! Local Network is prompted by macOS itself on the first discovery broadcast.
use std::ffi::c_void;

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGPreflightScreenCaptureAccess() -> bool;
    fn CGRequestScreenCaptureAccess() -> bool;
}

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    static kAXTrustedCheckOptionPrompt: *const c_void;
    fn AXIsProcessTrustedWithOptions(options: *const c_void) -> bool;
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    static kCFBooleanTrue: *const c_void;
    static kCFTypeDictionaryKeyCallBacks: c_void;
    static kCFTypeDictionaryValueCallBacks: c_void;
    fn CFDictionaryCreate(allocator: *const c_void, keys: *const *const c_void, values: *const *const c_void, count: isize, key_cb: *const c_void, value_cb: *const c_void) -> *const c_void;
    fn CFRelease(cf: *const c_void);
}

/// Asks for whatever is still missing (macOS shows each prompt only once per app version).
pub fn request() {
    unsafe {
        if !CGPreflightScreenCaptureAccess() {
            CGRequestScreenCaptureAccess();
        }
        let options = CFDictionaryCreate(
            std::ptr::null(),
            &kAXTrustedCheckOptionPrompt,
            &kCFBooleanTrue,
            1,
            &kCFTypeDictionaryKeyCallBacks,
            &kCFTypeDictionaryValueCallBacks,
        );
        AXIsProcessTrustedWithOptions(options);
        CFRelease(options);
    }
}

/// (screen recording, accessibility) granted?
pub fn granted() -> (bool, bool) {
    unsafe { (CGPreflightScreenCaptureAccess(), AXIsProcessTrustedWithOptions(std::ptr::null())) }
}
