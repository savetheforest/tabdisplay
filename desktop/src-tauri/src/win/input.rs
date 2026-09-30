//! Tablet input -> Windows. Touch and pen are injected as real Windows touch/pen (synthetic pointer
//! devices), so Windows itself provides scrolling, pinch zoom, press-and-hold right click, Windows Ink and
//! pen pressure. "Mouse" mode turns the first finger into a plain mouse for apps that ignore touch.
use crate::input::{
    Action, Contact, Key, KeyEvent, Kind, Scroll, MOD_ALT, MOD_CTRL, MOD_META, MOD_SHIFT,
};
use windows::Win32::Foundation::{POINT, RECT};
use windows::Win32::UI::Controls::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::Input::Pointer::*;
use windows::Win32::UI::WindowsAndMessaging::*;

pub type Rect = (i32, i32, i32, i32);

/// Lives for one session (one thread). Pointer devices are created on first use.
pub struct Injector {
    touch: Option<HSYNTHETICPOINTERDEVICE>,
    pen: Option<HSYNTHETICPOINTERDEVICE>,
    active_touch: [Option<Contact>; 256],
    active_pen: Option<Contact>,
    mouse_id: Option<u8>,
    mouse_last: Option<Contact>,
    last_rect: Option<Rect>,
    held_modifiers: u8,
}

impl Default for Injector {
    fn default() -> Self {
        Self {
            touch: None,
            pen: None,
            active_touch: [None; 256],
            active_pen: None,
            mouse_id: None,
            mouse_last: None,
            last_rect: None,
            held_modifiers: 0,
        }
    }
}

impl Drop for Injector {
    fn drop(&mut self) {
        self.release_all();
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
        Action::Up | Action::Cancel => POINTER_FLAG_UP,
        Action::Hover => POINTER_FLAG_UPDATE | POINTER_FLAG_INRANGE,
        Action::Leave => POINTER_FLAG_UPDATE,
    }
}

impl Injector {
    /// Injects one input frame (all contacts of one tablet MotionEvent) into the monitor at `rect`.
    pub fn inject(&mut self, frame: &[Contact], rect: Rect, as_mouse: bool) {
        self.last_rect = Some(rect);
        let touches: Vec<&Contact> = frame.iter().filter(|c| c.kind == Kind::Touch).collect();
        if as_mouse {
            let selected = self
                .mouse_id
                .and_then(|id| touches.iter().copied().find(|c| c.id == id))
                .or_else(|| {
                    self.mouse_id
                        .is_none()
                        .then(|| touches.iter().copied().find(|c| c.action == Action::Down))
                        .flatten()
                });
            if let Some(contact) = selected {
                if self.mouse_id.is_none() && contact.action == Action::Down {
                    self.mouse_id = Some(contact.id);
                }
                mouse(contact, rect);
                self.mouse_last = Some(*contact);
                if matches!(contact.action, Action::Up | Action::Cancel) {
                    self.mouse_id = None;
                    self.mouse_last = None;
                }
            }
        } else if !touches.is_empty() {
            self.touch(&touches, rect);
        }
        if let Some(pen) = frame.iter().find(|c| c.kind == Kind::Pen) {
            self.pen(pen, rect);
            self.active_pen = match pen.action {
                Action::Down | Action::Move => Some(*pen),
                Action::Up | Action::Cancel | Action::Leave => None,
                Action::Hover => self.active_pen,
            };
        }
    }

    /// Moves the mouse to the point and turns the wheel there (Windows sends it to the window under the cursor).
    pub fn scroll(&mut self, s: &Scroll, rect: Rect) {
        const NOTCH: f32 = 120.0; // WHEEL_DELTA
        let at = Contact {
            id: 0,
            kind: Kind::Touch,
            action: Action::Move,
            x: s.x,
            y: s.y,
            pressure: 0.0,
            tilt_x: 0,
            tilt_y: 0,
            barrel: false,
            eraser: false,
        };
        mouse(&at, rect);
        let wheel = |flag: MOUSE_EVENT_FLAGS, amount: f32| INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    mouseData: (amount * NOTCH).round() as i32 as u32,
                    dwFlags: flag,
                    ..Default::default()
                },
            },
        };
        let mut inputs = Vec::new();
        if s.dy != 0.0 {
            inputs.push(wheel(MOUSEEVENTF_WHEEL, s.dy));
        }
        if s.dx != 0.0 {
            inputs.push(wheel(MOUSEEVENTF_HWHEEL, s.dx));
        }
        if !inputs.is_empty() {
            unsafe { SendInput(&inputs, size_of::<INPUT>() as i32) };
        }
    }

    /// Injects only the bounded logical keys accepted by the protocol. Text uses the Unicode
    /// path so Android IME composition is not confused with Windows virtual-key layouts.
    pub fn key(&mut self, event: &KeyEvent) {
        match event {
            KeyEvent::Text(text) => {
                let mut inputs = Vec::with_capacity(text.encode_utf16().count() * 2);
                for unit in text.encode_utf16() {
                    inputs.push(key_input(0, unit, KEYEVENTF_UNICODE));
                    inputs.push(key_input(0, unit, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP));
                }
                send_keys(&inputs);
            }
            KeyEvent::Key {
                key,
                modifiers,
                down,
            } => {
                let Some(vk) = virtual_key(*key) else { return };
                let mut inputs = Vec::new();
                if *down {
                    for bit in [MOD_CTRL, MOD_SHIFT, MOD_ALT, MOD_META] {
                        if modifiers & bit != 0 && self.held_modifiers & bit == 0 {
                            inputs.push(key_input(modifier_vk(bit), 0, KEYBD_EVENT_FLAGS(0)));
                            self.held_modifiers |= bit;
                        }
                    }
                    inputs.push(key_input(vk, 0, KEYBD_EVENT_FLAGS(0)));
                } else {
                    inputs.push(key_input(vk, 0, KEYEVENTF_KEYUP));
                    for bit in [MOD_META, MOD_ALT, MOD_SHIFT, MOD_CTRL] {
                        if modifiers & bit != 0 && self.held_modifiers & bit != 0 {
                            inputs.push(key_input(modifier_vk(bit), 0, KEYEVENTF_KEYUP));
                            self.held_modifiers &= !bit;
                        }
                    }
                }
                send_keys(&inputs);
            }
            KeyEvent::Cancel => self.release_keys(),
        }
    }

    fn release_keys(&mut self) {
        let mut inputs = Vec::new();
        for bit in [MOD_META, MOD_ALT, MOD_SHIFT, MOD_CTRL] {
            if self.held_modifiers & bit != 0 {
                inputs.push(key_input(modifier_vk(bit), 0, KEYEVENTF_KEYUP));
            }
        }
        self.held_modifiers = 0;
        send_keys(&inputs);
    }

    fn touch(&mut self, contacts: &[&Contact], rect: Rect) {
        let Some(device) = device(&mut self.touch, PT_TOUCH, 10) else {
            return;
        };
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
                            pointerInfo: POINTER_INFO {
                                pointerType: PT_TOUCH,
                                pointerId: c.id as u32,
                                pointerFlags: f,
                                ptPixelLocation: p,
                                ..Default::default()
                            },
                            touchFlags: 0,
                            touchMask: TOUCH_MASK_CONTACTAREA | TOUCH_MASK_PRESSURE,
                            rcContact: RECT {
                                left: p.x - 2,
                                top: p.y - 2,
                                right: p.x + 2,
                                bottom: p.y + 2,
                            },
                            pressure: (c.pressure.clamp(0.0, 1.0) * 1024.0) as u32,
                            ..Default::default()
                        },
                    },
                }
            })
            .collect();
        unsafe {
            if InjectSyntheticPointerInput(device, &infos).is_err() {
                crate::telemetry::warn("InjectSyntheticPointerInput(touch) failed".to_string());
            }
        }
        for c in contacts {
            match c.action {
                Action::Down | Action::Move => self.active_touch[c.id as usize] = Some(**c),
                Action::Up | Action::Cancel | Action::Leave => {
                    self.active_touch[c.id as usize] = None
                }
                Action::Hover => {}
            }
        }
    }

    fn pen(&mut self, c: &Contact, rect: Rect) {
        let Some(device) = device(&mut self.pen, PT_PEN, 1) else {
            return;
        };
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
            if InjectSyntheticPointerInput(device, &[info]).is_err() {
                crate::telemetry::warn("InjectSyntheticPointerInput(pen) failed".to_string());
            }
        }
    }

    /// Releases only contacts created by this session before the synthetic devices are destroyed.
    fn release_all(&mut self) {
        self.release_keys();
        let Some(rect) = self.last_rect else { return };
        let releases = self
            .active_touch
            .iter()
            .filter_map(|c| {
                c.map(|last| Contact {
                    action: Action::Up,
                    ..last
                })
            })
            .collect::<Vec<_>>();
        if !releases.is_empty() {
            let refs = releases.iter().collect::<Vec<_>>();
            self.touch(&refs, rect);
        }
        if let Some(last) = self.active_pen {
            let release = Contact {
                action: Action::Up,
                ..last
            };
            self.pen(&release, rect);
        }
        if let Some(last) = self.mouse_last.take() {
            mouse(
                &Contact {
                    action: Action::Up,
                    ..last
                },
                rect,
            );
        }
        self.mouse_id = None;
        self.active_touch.fill(None);
        self.active_pen = None;
    }
}

fn key_input(vk: u16, scan: u16, flags: KEYBD_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(vk),
                wScan: scan,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

fn send_keys(inputs: &[INPUT]) {
    if !inputs.is_empty()
        && unsafe { SendInput(inputs, size_of::<INPUT>() as i32) } != inputs.len() as u32
    {
        crate::telemetry::warn("SendInput(keyboard) failed".to_string());
    }
}

fn modifier_vk(bit: u8) -> u16 {
    match bit {
        MOD_CTRL => 0x11,
        MOD_SHIFT => 0x10,
        MOD_ALT => 0x12,
        MOD_META => 0x5B,
        _ => 0,
    }
}

fn virtual_key(key: Key) -> Option<u16> {
    Some(match key {
        Key::Enter => 0x0D,
        Key::Backspace => 0x08,
        Key::Tab => 0x09,
        Key::Escape => 0x1B,
        Key::Delete => 0x2E,
        Key::ArrowLeft => 0x25,
        Key::ArrowRight => 0x27,
        Key::ArrowUp => 0x26,
        Key::ArrowDown => 0x28,
        Key::Home => 0x24,
        Key::End => 0x23,
        Key::Space => 0x20,
    })
}

fn device(
    slot: &mut Option<HSYNTHETICPOINTERDEVICE>,
    kind: POINTER_INPUT_TYPE,
    max: u32,
) -> Option<HSYNTHETICPOINTERDEVICE> {
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
        let (vx, vy) = (
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
        );
        let (vw, vh) = (
            GetSystemMetrics(SM_CXVIRTUALSCREEN),
            GetSystemMetrics(SM_CYVIRTUALSCREEN),
        );
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
                    dwFlags: MOUSEEVENTF_MOVE
                        | MOUSEEVENTF_ABSOLUTE
                        | MOUSEEVENTF_VIRTUALDESK
                        | button,
                    ..Default::default()
                },
            },
        };
        if SendInput(&[input], size_of::<INPUT>() as i32) != 1 {
            crate::telemetry::warn("SendInput(mouse) failed".to_string());
        }
    }
}
