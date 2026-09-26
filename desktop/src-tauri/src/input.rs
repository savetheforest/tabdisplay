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

/// None if the payload is malformed.
pub fn parse_scroll(p: &[u8]) -> Option<Scroll> {
    if p.len() != 16 {
        return None;
    }
    let v: [f32; 4] = std::array::from_fn(|i| f32::from_be_bytes(p[i * 4..i * 4 + 4].try_into().unwrap()));
    Some(Scroll { x: v[0], y: v[1], dx: v[2], dy: v[3] })
}

/// None if the payload is malformed.
pub fn parse(p: &[u8]) -> Option<Vec<Contact>> {
    let (&count, rest) = p.split_first()?;
    if rest.len() != count as usize * CONTACT_LEN {
        return None;
    }
    let f32_at = |c: &[u8], i: usize| f32::from_be_bytes(c[i..i + 4].try_into().unwrap());
    rest.chunks_exact(CONTACT_LEN)
        .map(|c| {
            Some(Contact {
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
                    _ => return None,
                },
                barrel: c[3] & 1 != 0,
                eraser: c[3] & 2 != 0,
                x: f32_at(c, 4),
                y: f32_at(c, 8),
                pressure: f32_at(c, 12),
                tilt_x: c[16] as i8,
                tilt_y: c[17] as i8,
            })
        })
        .collect()
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
        assert_eq!(parse_scroll(&p), Some(Scroll { x: 0.5, y: 0.25, dx: -1.0, dy: 3.0 }));
        assert!(parse_scroll(&p[..15]).is_none());
    }

    #[test]
    fn parses_two_finger_frame_and_pen() {
        let mut p = vec![3u8];
        for (id, kind, action, buttons, x, y, pressure, tx, ty) in
            [(0u8, 0u8, 1u8, 0u8, 0.25f32, 0.5f32, 0.3f32, 0i8, 0i8), (1, 0, 0, 0, 0.75, 0.5, 0.4, 0, 0), (7, 1, 3, 3, 0.5, 0.1, 0.0, -20, 45)]
        {
            p.extend([id, kind, action, buttons]);
            for v in [x, y, pressure] {
                p.extend(v.to_be_bytes());
            }
            p.extend([tx as u8, ty as u8]);
        }
        let frame = parse(&p).unwrap();
        assert_eq!(frame.len(), 3);
        assert_eq!((frame[1].id, frame[1].kind, frame[1].action, frame[1].x), (1, Kind::Touch, Action::Down, 0.75));
        let pen = frame[2];
        assert_eq!((pen.kind, pen.action, pen.barrel, pen.eraser, pen.tilt_x, pen.tilt_y), (Kind::Pen, Action::Hover, true, true, -20, 45));

        assert!(parse(&p[..p.len() - 1]).is_none()); // truncated
        assert!(parse(&[]).is_none());
    }
}
