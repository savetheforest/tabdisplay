//! Tablet input -> macOS. macOS has no touch-injection API, so touch becomes mouse gestures:
//! tap = click (double tap = double click), press and hold = right click, drag = drag,
//! two fingers = scroll. The pen drives the mouse with pressure. Needs the Accessibility permission.
//! Pinch (two fingers moving apart/together) sends Cmd+= / Cmd+-, the zoom shortcut of Safari, Photos, Maps, Preview...
//! ponytail: macOS has no public API for synthetic magnify gestures; use the private gesture CGEvent if apps that only
//! react to real pinches (no zoom shortcut) matter.
use super::display::CGPoint;
use crate::input::{Action, Contact, Kind, Scroll};
use std::ffi::c_void;
use std::time::{Duration, Instant};

pub type Rect = (i32, i32, i32, i32);

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGEventCreateMouseEvent(source: *const c_void, kind: u32, position: CGPoint, button: u32) -> *mut c_void;
    fn CGEventCreateScrollWheelEvent2(source: *const c_void, units: u32, count: u32, wheel1: i32, wheel2: i32, wheel3: i32) -> *mut c_void;
    fn CGEventCreateKeyboardEvent(source: *const c_void, keycode: u16, down: bool) -> *mut c_void;
    fn CGEventSetFlags(event: *mut c_void, flags: u64);
    fn CGEventSetIntegerValueField(event: *mut c_void, field: u32, value: i64);
    fn CGEventSetDoubleValueField(event: *mut c_void, field: u32, value: f64);
    fn CGEventPost(tap: u32, event: *mut c_void);
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFRelease(cf: *const c_void);
}

// CGEventType
const LEFT_DOWN: u32 = 1;
const LEFT_UP: u32 = 2;
const RIGHT_DOWN: u32 = 3;
const RIGHT_UP: u32 = 4;
const MOVED: u32 = 5;
const LEFT_DRAGGED: u32 = 6;
const RIGHT_DRAGGED: u32 = 7;
// CGEventField
const CLICK_STATE: u32 = 1;
const PRESSURE: u32 = 2;

const KEY_EQUALS: u16 = 24; // Cmd+= zooms in
const KEY_MINUS: u16 = 27;
const FLAG_COMMAND: u64 = 0x10_0000;
/// Points the fingers must spread or pinch (in total) for one zoom step.
const ZOOM_STEP: f64 = 60.0;

const HOLD_FOR_RIGHT_CLICK: Duration = Duration::from_millis(500);
const DOUBLE_CLICK: Duration = Duration::from_millis(400);
/// Points a finger may wander before a touch counts as a drag instead of a tap.
const SLOP: f64 = 8.0;

struct Touch {
    start: Instant,
    at: CGPoint,
    dragging: bool,
}

#[derive(Default)]
pub struct Injector {
    touch: Option<Touch>,
    /// Centroid of a two-finger scroll in progress.
    scroll: Option<CGPoint>,
    /// Distance between the first two fingers, and how much of it is still owed to a zoom step.
    spread: Option<f64>,
    zoom: f64,
    /// Where and when the last tap was, and how many in a row (for double clicks).
    last_tap: Option<(Instant, CGPoint, i64)>,
    pen_button: Option<(u32, u32)>,
}

fn point(c: &Contact, (l, t, r, b): Rect) -> CGPoint {
    CGPoint { x: l as f64 + c.x.clamp(0.0, 1.0) as f64 * (r - l) as f64, y: t as f64 + c.y.clamp(0.0, 1.0) as f64 * (b - t) as f64 }
}

fn distance(a: CGPoint, b: CGPoint) -> f64 {
    ((a.x - b.x).powi(2) + (a.y - b.y).powi(2)).sqrt()
}

fn post(kind: u32, at: CGPoint, button: u32, clicks: i64, pressure: Option<f64>) {
    unsafe {
        let e = CGEventCreateMouseEvent(std::ptr::null(), kind, at, button);
        if e.is_null() {
            return;
        }
        if clicks > 0 {
            CGEventSetIntegerValueField(e, CLICK_STATE, clicks);
        }
        if let Some(p) = pressure {
            CGEventSetDoubleValueField(e, PRESSURE, p);
        }
        CGEventPost(0, e); // kCGHIDEventTap
        CFRelease(e);
    }
}

/// Presses Cmd + `key`.
fn command_key(key: u16) {
    unsafe {
        for down in [true, false] {
            let e = CGEventCreateKeyboardEvent(std::ptr::null(), key, down);
            if !e.is_null() {
                CGEventSetFlags(e, FLAG_COMMAND);
                CGEventPost(0, e);
                CFRelease(e);
            }
        }
    }
}

fn scroll(dy: f64, dx: f64) {
    unsafe {
        let e = CGEventCreateScrollWheelEvent2(std::ptr::null(), 0, 2, dy.round() as i32, dx.round() as i32, 0); // pixel units
        if !e.is_null() {
            CGEventPost(0, e);
            CFRelease(e);
        }
    }
}

impl Injector {
    /// Injects one input frame (all contacts of one tablet MotionEvent) into the display at `rect`.
    pub fn inject(&mut self, frame: &[Contact], rect: Rect, _as_mouse: bool) {
        if let Some(pen) = frame.iter().find(|c| c.kind == Kind::Pen) {
            self.pen(pen, rect);
        }
        let touches: Vec<&Contact> = frame.iter().filter(|c| c.kind == Kind::Touch).collect();
        if touches.is_empty() {
            return;
        }
        let down: Vec<&&Contact> = touches.iter().filter(|c| c.action != Action::Up).collect();

        if down.len() >= 2 {
            // Two fingers: scroll by how far their midpoint moved (natural direction, like a trackpad), or,
            // when the fingers moved apart/together more than the midpoint moved, zoom.
            let n = down.len() as f64;
            let mid = down.iter().map(|c| point(c, rect)).fold(CGPoint::default(), |a, p| CGPoint { x: a.x + p.x / n, y: a.y + p.y / n });
            let spread = distance(point(down[0], rect), point(down[1], rect));
            if let (Some(prev), Some(prev_spread)) = (self.scroll, self.spread.replace(spread)) {
                let change = spread - prev_spread;
                if change.abs() > distance(mid, prev) {
                    self.zoom += change;
                    while self.zoom.abs() >= ZOOM_STEP {
                        command_key(if self.zoom > 0.0 { KEY_EQUALS } else { KEY_MINUS });
                        self.zoom -= ZOOM_STEP.copysign(self.zoom);
                    }
                } else {
                    scroll(mid.y - prev.y, mid.x - prev.x);
                }
            }
            self.scroll = Some(mid);
            if let Some(t) = self.touch.take().filter(|t| t.dragging) {
                post(LEFT_UP, t.at, 0, 1, None);
            }
            return;
        }
        if self.scroll.is_some() {
            // The rest of a two-finger gesture: ignore until every finger is up.
            if down.is_empty() {
                (self.scroll, self.spread, self.zoom) = (None, None, 0.0);
            }
            return;
        }

        let c = touches[0];
        let at = point(c, rect);
        match c.action {
            Action::Down => {
                post(MOVED, at, 0, 0, None);
                self.touch = Some(Touch { start: Instant::now(), at, dragging: false });
            }
            Action::Move => {
                let Some(t) = &mut self.touch else { return };
                if !t.dragging && distance(at, t.at) > SLOP {
                    post(LEFT_DOWN, t.at, 0, 1, None);
                    t.dragging = true;
                }
                if t.dragging {
                    post(LEFT_DRAGGED, at, 0, 0, None);
                    t.at = at;
                }
            }
            Action::Up => {
                let Some(t) = self.touch.take() else { return };
                if t.dragging {
                    post(LEFT_UP, at, 0, 1, None);
                } else if t.start.elapsed() >= HOLD_FOR_RIGHT_CLICK {
                    post(RIGHT_DOWN, t.at, 1, 1, None);
                    post(RIGHT_UP, t.at, 1, 1, None);
                } else {
                    let clicks = match self.last_tap {
                        Some((when, where_, n)) if when.elapsed() < DOUBLE_CLICK && distance(where_, t.at) < SLOP * 2.0 => n + 1,
                        _ => 1,
                    };
                    post(LEFT_DOWN, t.at, 0, clicks, None);
                    post(LEFT_UP, t.at, 0, clicks, None);
                    self.last_tap = Some((Instant::now(), t.at, clicks));
                }
            }
            Action::Hover | Action::Leave => {}
        }
    }

    /// The pen is a mouse with pressure; its side button makes it a right button.
    /// Moves the cursor to the point and scrolls there (a mouse wheel notch is about 40 px).
    pub fn scroll(&mut self, s: &Scroll, rect: Rect) {
        let (l, t, r, b) = rect;
        let at = CGPoint { x: l as f64 + s.x.clamp(0.0, 1.0) as f64 * (r - l) as f64, y: t as f64 + s.y.clamp(0.0, 1.0) as f64 * (b - t) as f64 };
        post(MOVED, at, 0, 0, None);
        // Android: dy > 0 scrolls up, dx > 0 scrolls right; CG wheel2 > 0 moves content right.
        scroll(s.dy as f64 * 40.0, -(s.dx as f64) * 40.0);
    }

    fn pen(&mut self, c: &Contact, rect: Rect) {
        let at = point(c, rect);
        let pressure = Some(c.pressure.clamp(0.0, 1.0) as f64);
        match c.action {
            Action::Hover => post(MOVED, at, 0, 0, None),
            Action::Down => {
                let (down, button) = if c.barrel { (RIGHT_DOWN, 1) } else { (LEFT_DOWN, 0) };
                post(down, at, button, 1, pressure);
                self.pen_button = Some(if c.barrel { (RIGHT_DRAGGED, RIGHT_UP) } else { (LEFT_DRAGGED, LEFT_UP) });
            }
            Action::Move => {
                if let Some((dragged, _)) = self.pen_button {
                    post(dragged, at, if dragged == RIGHT_DRAGGED { 1 } else { 0 }, 0, pressure);
                }
            }
            Action::Up | Action::Leave => {
                if let Some((dragged, up)) = self.pen_button.take() {
                    post(up, at, if dragged == RIGHT_DRAGGED { 1 } else { 0 }, 1, Some(0.0));
                }
            }
        }
    }
}
