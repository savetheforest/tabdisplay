//! INPUT message (tablet -> PC): one frame with every touch/pen contact of a tablet MotionEvent.
//! Payload: `[count u8]`, then per contact 18 bytes (big-endian):
//! `[id u8][kind u8: 0 touch, 1 pen][action u8][buttons u8: 1 barrel, 2 eraser][x f32][y f32][pressure f32][tilt_x i8][tilt_y i8]`
//! x/y are 0..1 over the video; pressure 0..1; tilt in degrees (-90..90).

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Kind {
    Touch,
    Pen,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Action {
    Down,
    Move,
    Up,
    /// Explicit cancellation; injectors must release without synthesizing a new click.
    Cancel,
    /// Pen near the screen, not touching.
    Hover,
    /// Pen left the screen's range.
    Leave,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Contact {
    pub id: u8,
    pub kind: Kind,
    pub action: Action,
    pub x: f32,
    pub y: f32,
    pub pressure: f32,
    pub tilt_x: i8,
    pub tilt_y: i8,
    pub barrel: bool,
    pub eraser: bool,
}

const CONTACT_LEN: usize = 18;
/// Windows' synthetic touch device is created with room for ten contacts.
pub const MAX_CONTACTS: usize = 10;

/// SCROLL message (tablet -> PC): a mouse wheel / trackpad scroll at a point.
/// Payload: `[x f32][y f32][dx f32][dy f32]` (big-endian); x/y are 0..1 over the video, dx/dy are wheel
/// notches with Android's sign (dy > 0 scrolls up, dx > 0 scrolls right).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Scroll {
    pub x: f32,
    pub y: f32,
    pub dx: f32,
    pub dy: f32,
}

/// Bounded keyboard input from the tablet. Text is a committed IME string; key events are logical
/// names, never Android keycodes or an arbitrary native scancode.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeyEvent {
    Text(String),
    Key { key: Key, modifiers: u8, down: bool },
    Cancel,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Enter,
    Backspace,
    Tab,
    Escape,
    Delete,
    ArrowLeft,
    ArrowRight,
    ArrowUp,
    ArrowDown,
    Home,
    End,
    Space,
}

pub const MOD_CTRL: u8 = 1;
pub const MOD_SHIFT: u8 = 2;
pub const MOD_ALT: u8 = 4;
pub const MOD_META: u8 = 8;
pub const MAX_KEY_TEXT: usize = 16 << 10;

/// Parses the versioned KEY payload. Unknown keys, modifiers, sources and oversized text are rejected.
pub fn parse_key(payload: &[u8]) -> Option<KeyEvent> {
    let value: serde_json::Value = serde_json::from_slice(payload).ok()?;
    if value["version"].as_u64()? != 1 || value["source"].as_str()? != "tablet" {
        return None;
    }
    match value["action"].as_str()? {
        "text" => {
            let text = value["text"].as_str()?.to_owned();
            (text.as_bytes().len() <= MAX_KEY_TEXT).then_some(KeyEvent::Text(text))
        }
        "cancel" => Some(KeyEvent::Cancel),
        "key" => {
            let key = match value["key"].as_str()? {
                "Enter" => Key::Enter,
                "Backspace" => Key::Backspace,
                "Tab" => Key::Tab,
                "Escape" => Key::Escape,
                "Delete" => Key::Delete,
                "ArrowLeft" => Key::ArrowLeft,
                "ArrowRight" => Key::ArrowRight,
                "ArrowUp" => Key::ArrowUp,
                "ArrowDown" => Key::ArrowDown,
                "Home" => Key::Home,
                "End" => Key::End,
                "Space" => Key::Space,
                _ => return None,
            };
            let modifiers = value["modifiers"].as_u64()?.try_into().ok()?;
            if modifiers & !(MOD_CTRL | MOD_SHIFT | MOD_ALT | MOD_META) != 0 {
                return None;
            }
            Some(KeyEvent::Key {
                key,
                modifiers,
                down: value["down"].as_bool()?,
            })
        }
        _ => None,
    }
}

/// None if the payload is malformed.
pub fn parse_scroll(p: &[u8]) -> Option<Scroll> {
    if p.len() != 16 {
        return None;
    }
    let v: [f32; 4] =
        std::array::from_fn(|i| f32::from_be_bytes(p[i * 4..i * 4 + 4].try_into().unwrap()));
    if !v.iter().all(|value| value.is_finite())
        || !(0.0..=1.0).contains(&v[0])
        || !(0.0..=1.0).contains(&v[1])
    {
        return None;
    }
    Some(Scroll {
        x: v[0],
        y: v[1],
        dx: v[2],
        dy: v[3],
    })
}

/// None if the payload is malformed.
pub fn parse(p: &[u8]) -> Option<Vec<Contact>> {
    let (&count, rest) = p.split_first()?;
    if count as usize > MAX_CONTACTS {
        return None;
    }
    if rest.len() != count as usize * CONTACT_LEN {
        return None;
    }
    let f32_at = |c: &[u8], i: usize| f32::from_be_bytes(c[i..i + 4].try_into().unwrap());
    let mut ids = [false; 256];
    let mut contacts = Vec::with_capacity(count as usize);
    for c in rest.chunks_exact(CONTACT_LEN) {
        if ids[c[0] as usize] || c[3] & !3 != 0 {
            return None;
        }
        ids[c[0] as usize] = true;
        let contact = Contact {
            id: c[0],
            kind: match c[1] {
                0 => Kind::Touch,
                1 => Kind::Pen,
                _ => return None,
            },
            action: match c[2] {
                0 => Action::Down,
                1 => Action::Move,
                2 => Action::Up,
                3 => Action::Hover,
                4 => Action::Leave,
                5 => Action::Cancel,
                _ => return None,
            },
            barrel: c[3] & 1 != 0,
            eraser: c[3] & 2 != 0,
            x: f32_at(c, 4),
            y: f32_at(c, 8),
            pressure: f32_at(c, 12),
            tilt_x: c[16] as i8,
            tilt_y: c[17] as i8,
        };
        if !contact.x.is_finite()
            || !contact.y.is_finite()
            || !contact.pressure.is_finite()
            || !(0.0..=1.0).contains(&contact.x)
            || !(0.0..=1.0).contains(&contact.y)
            || !(0.0..=1.0).contains(&contact.pressure)
            || contact.tilt_x.abs() > 90
            || contact.tilt_y.abs() > 90
        {
            return None;
        }
        contacts.push(contact);
    }
    Some(contacts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_scroll() {
        let mut p = Vec::new();
        for v in [0.5f32, 0.25, -1.0, 3.0] {
            p.extend(v.to_be_bytes());
        }
        assert_eq!(
            parse_scroll(&p),
            Some(Scroll {
                x: 0.5,
                y: 0.25,
                dx: -1.0,
                dy: 3.0
            })
        );
        assert!(parse_scroll(&p[..15]).is_none());
    }

    #[test]
    fn parses_two_finger_frame_and_pen() {
        let mut p = vec![3u8];
        for (id, kind, action, buttons, x, y, pressure, tx, ty) in [
            (0u8, 0u8, 1u8, 0u8, 0.25f32, 0.5f32, 0.3f32, 0i8, 0i8),
            (1, 0, 0, 0, 0.75, 0.5, 0.4, 0, 0),
            (7, 1, 3, 3, 0.5, 0.1, 0.0, -20, 45),
        ] {
            p.extend([id, kind, action, buttons]);
            for v in [x, y, pressure] {
                p.extend(v.to_be_bytes());
            }
            p.extend([tx as u8, ty as u8]);
        }
        let frame = parse(&p).unwrap();
        assert_eq!(frame.len(), 3);
        assert_eq!(
            (frame[1].id, frame[1].kind, frame[1].action, frame[1].x),
            (1, Kind::Touch, Action::Down, 0.75)
        );
        let pen = frame[2];
        assert_eq!(
            (pen.kind, pen.action, pen.barrel, pen.eraser, pen.tilt_x, pen.tilt_y),
            (Kind::Pen, Action::Hover, true, true, -20, 45)
        );

        assert!(parse(&p[..p.len() - 1]).is_none()); // truncated
        assert!(parse(&[]).is_none());
    }

    #[test]
    fn shared_fixture_and_invalid_numbers() {
        let payload = crate::test_fixtures::hex("input-touch-pen.hex");
        let frame = parse(&payload).unwrap();
        assert_eq!(frame.len(), 3);
        assert_eq!(
            (frame[0].x, frame[1].action, frame[2].tilt_x),
            (0.25, Action::Down, -20)
        );

        let mut nan = payload.clone();
        nan[5..9].copy_from_slice(&f32::NAN.to_be_bytes());
        assert!(parse(&nan).is_none());
        let mut unknown = payload;
        unknown[3] = 9;
        assert!(parse(&unknown).is_none());
        let mut duplicate = crate::test_fixtures::hex("input-touch-pen.hex");
        duplicate[19] = 0;
        assert!(parse(&duplicate).is_none());
        assert!(parse_scroll(&crate::test_fixtures::hex("scroll.hex")).is_some());
    }

    #[test]
    fn parses_bounded_unicode_key_events_without_native_codes() {
        let text = serde_json::json!({ "version": 1, "source": "tablet", "action": "text", "text": "emoji 😀\nacentos" });
        assert_eq!(
            parse_key(&serde_json::to_vec(&text).unwrap()),
            Some(KeyEvent::Text("emoji 😀\nacentos".into()))
        );
        let key = serde_json::json!({ "version": 1, "source": "tablet", "action": "key", "key": "Enter", "modifiers": 3, "down": true });
        assert_eq!(
            parse_key(&serde_json::to_vec(&key).unwrap()),
            Some(KeyEvent::Key {
                key: Key::Enter,
                modifiers: 3,
                down: true
            })
        );
        let huge = serde_json::json!({ "version": 1, "source": "tablet", "action": "text", "text": "x".repeat(MAX_KEY_TEXT + 1) });
        assert!(parse_key(&serde_json::to_vec(&huge).unwrap()).is_none());
        let native = serde_json::json!({ "version": 1, "source": "tablet", "action": "key", "key": "AndroidKeycode42", "modifiers": 0, "down": true });
        assert!(parse_key(&serde_json::to_vec(&native).unwrap()).is_none());
    }
}
