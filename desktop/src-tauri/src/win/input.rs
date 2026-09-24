use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;

pub const DOWN: u8 = 0;
pub const UP: u8 = 2;

/// Moves the mouse to normalized (x, y) inside `rect` (left, top, right, bottom) and presses/releases the left button.
pub fn touch(action: u8, x: f32, y: f32, rect: (i32, i32, i32, i32)) {
    let (l, t, r, b) = rect;
    let px = l as f32 + x.clamp(0.0, 1.0) * (r - l - 1) as f32;
    let py = t as f32 + y.clamp(0.0, 1.0) * (b - t - 1) as f32;
    unsafe {
        let (vx, vy) = (GetSystemMetrics(SM_XVIRTUALSCREEN), GetSystemMetrics(SM_YVIRTUALSCREEN));
        let (vw, vh) = (GetSystemMetrics(SM_CXVIRTUALSCREEN), GetSystemMetrics(SM_CYVIRTUALSCREEN));
        let button = match action {
            DOWN => MOUSEEVENTF_LEFTDOWN,
            UP => MOUSEEVENTF_LEFTUP,
            _ => MOUSE_EVENT_FLAGS(0),
        };
        let input = INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    dx: ((px - vx as f32) * 65535.0 / (vw - 1) as f32) as i32,
                    dy: ((py - vy as f32) * 65535.0 / (vh - 1) as f32) as i32,
                    dwFlags: MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK | button,
                    ..Default::default()
                },
            },
        };
        SendInput(&[input], size_of::<INPUT>() as i32);
    }
}
