//! Tablet input -> Windows. Touch and pen are injected as real Windows touch/pen (synthetic pointer
//! devices), so Windows itself provides scrolling, pinch zoom, press-and-hold right click, Windows Ink and
//! pen pressure. "Mouse" mode turns the first finger into a plain mouse for apps that ignore touch.
use crate::input::{Action, Contact, Kind};
use windows::Win32::Foundation::{POINT, RECT};
use windows::Win32::UI::Controls::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::Input::Pointer::*;
use windows::Win32::UI::WindowsAndMessaging::*;

pub type Rect = (i32, i32, i32, i32);

/// Lives for one session (one thread). Pointer devices are created on first use.
#[derive(Default)]
pub struct Injector {
    touch: Option<HSYNTHETICPOINTERDEVICE>,
    pen: Option<HSYNTHETICPOINTERDEVICE>,
}

impl Drop for Injector {
    fn drop(&mut self) {
        unsafe {
            if let Some(d) = self.touch.take() {
                DestroySyntheticPointerDevice(d);
            }
            if let Some(d) = self.pen.take() {
                DestroySyntheticPointerDevice(d);
            }
        }
    }
}

fn to_screen(c: &Contact, (l, t, r, b): Rect) -> POINT {
    POINT {
        x: l + (c.x.clamp(0.0, 1.0) * (r - l - 1) as f32).round() as i32,
        y: t + (c.y.clamp(0.0, 1.0) * (b - t - 1) as f32).round() as i32,
    }
}

fn flags(action: Action) -> POINTER_FLAGS {
    match action {
        Action::Down => POINTER_FLAG_DOWN | POINTER_FLAG_INRANGE | POINTER_FLAG_INCONTACT,
        Action::Move => POINTER_FLAG_UPDATE | POINTER_FLAG_INRANGE | POINTER_FLAG_INCONTACT,
        Action::Up => POINTER_FLAG_UP,
        Action::Hover => POINTER_FLAG_UPDATE | POINTER_FLAG_INRANGE,
        Action::Leave => POINTER_FLAG_UPDATE,
    }
}

impl Injector {
    /// Injects one input frame (all contacts of one tablet MotionEvent) into the monitor at `rect`.
    pub fn inject(&mut self, frame: &[Contact], rect: Rect, as_mouse: bool) {
        let touches: Vec<&Contact> = frame.iter().filter(|c| c.kind == Kind::Touch).collect();
        if as_mouse {
            if let Some(first) = touches.first() {
                mouse(first, rect);
            }
        } else if !touches.is_empty() {
            self.touch(&touches, rect);
        }
        if let Some(pen) = frame.iter().find(|c| c.kind == Kind::Pen) {
            self.pen(pen, rect);
        }
    }

    fn touch(&mut self, contacts: &[&Contact], rect: Rect) {
        let Some(device) = device(&mut self.touch, PT_TOUCH, 10) else { return };
        let infos: Vec<POINTER_TYPE_INFO> = contacts
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let p = to_screen(c, rect);
                let mut f = flags(c.action);
                if i == 0 {
                    f |= POINTER_FLAG_PRIMARY;
                }
                POINTER_TYPE_INFO {
                    r#type: PT_TOUCH,
                    Anonymous: POINTER_TYPE_INFO_0 {
                        touchInfo: POINTER_TOUCH_INFO {
                            pointerInfo: POINTER_INFO { pointerType: PT_TOUCH, pointerId: c.id as u32, pointerFlags: f, ptPixelLocation: p, ..Default::default() },
                            touchFlags: 0,
                            touchMask: TOUCH_MASK_CONTACTAREA | TOUCH_MASK_PRESSURE,
                            rcContact: RECT { left: p.x - 2, top: p.y - 2, right: p.x + 2, bottom: p.y + 2 },
                            pressure: (c.pressure.clamp(0.0, 1.0) * 1024.0) as u32,
                            ..Default::default()
                        },
                    },
                }
            })
            .collect();
        unsafe {
            let _ = InjectSyntheticPointerInput(device, &infos);
        }
    }

    fn pen(&mut self, c: &Contact, rect: Rect) {
        let Some(device) = device(&mut self.pen, PT_PEN, 1) else { return };
        let mut pen_flags = 0;
        if c.barrel {
            pen_flags |= PEN_FLAG_BARREL;
        }
        if c.eraser {
            pen_flags |= PEN_FLAG_ERASER;
        }
        let info = POINTER_TYPE_INFO {
            r#type: PT_PEN,
            Anonymous: POINTER_TYPE_INFO_0 {
                penInfo: POINTER_PEN_INFO {
                    pointerInfo: POINTER_INFO {
                        pointerType: PT_PEN,
                        pointerId: 0,
                        pointerFlags: flags(c.action) | POINTER_FLAG_PRIMARY,
                        ptPixelLocation: to_screen(c, rect),
                        ..Default::default()
                    },
                    penFlags: pen_flags,
                    penMask: PEN_MASK_PRESSURE | PEN_MASK_TILT_X | PEN_MASK_TILT_Y,
                    pressure: (c.pressure.clamp(0.0, 1.0) * 1024.0) as u32,
                    tiltX: c.tilt_x as i32,
                    tiltY: c.tilt_y as i32,
                    ..Default::default()
                },
            },
        };
        unsafe {
            let _ = InjectSyntheticPointerInput(device, &[info]);
        }
    }
}

fn device(slot: &mut Option<HSYNTHETICPOINTERDEVICE>, kind: POINTER_INPUT_TYPE, max: u32) -> Option<HSYNTHETICPOINTERDEVICE> {
    if slot.is_none() {
        // DEFAULT feedback: Windows draws its touch/pen visuals, which also show up on the tablet.
        *slot = unsafe { CreateSyntheticPointerDevice(kind, max, POINTER_FEEDBACK_DEFAULT) }.ok();
    }
    *slot
}

/// Moves the mouse to the contact and presses/releases the left button.
fn mouse(c: &Contact, rect: Rect) {
    let p = to_screen(c, rect);
    unsafe {
        let (vx, vy) = (GetSystemMetrics(SM_XVIRTUALSCREEN), GetSystemMetrics(SM_YVIRTUALSCREEN));
        let (vw, vh) = (GetSystemMetrics(SM_CXVIRTUALSCREEN), GetSystemMetrics(SM_CYVIRTUALSCREEN));
        let button = match c.action {
            Action::Down => MOUSEEVENTF_LEFTDOWN,
            Action::Up => MOUSEEVENTF_LEFTUP,
            _ => MOUSE_EVENT_FLAGS(0),
        };
        let input = INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    dx: ((p.x - vx) as f32 * 65535.0 / (vw - 1) as f32) as i32,
                    dy: ((p.y - vy) as f32 * 65535.0 / (vh - 1) as f32) as i32,
                    dwFlags: MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK | button,
                    ..Default::default()
                },
            },
        };
        SendInput(&[input], size_of::<INPUT>() as i32);
    }
}
