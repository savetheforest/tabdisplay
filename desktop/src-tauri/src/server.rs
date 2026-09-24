//! TCP server + wire protocol. See PROTOCOL.md.
use crate::encode::H264;
use crate::win::{capture::Capture, input};
use std::io::{self, Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

pub const PORT: u16 = 7070;
pub const HELLO: u8 = 1;
pub const VIDEO: u8 = 2;
pub const TOUCH: u8 = 3;
pub const CONFIG: u8 = 4;
const MAX_MSG: usize = 16 << 20;

pub static STATUS: Mutex<String> = Mutex::new(String::new());

fn set_status(s: impl Into<String>) {
    *STATUS.lock().unwrap() = s.into();
}

pub fn write_msg(w: &mut impl Write, kind: u8, payload: &[u8]) -> io::Result<()> {
    let mut msg = Vec::with_capacity(5 + payload.len());
    msg.push(kind);
    msg.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    msg.extend_from_slice(payload);
    w.write_all(&msg)
}

pub fn read_msg(r: &mut impl Read) -> io::Result<(u8, Vec<u8>)> {
    let mut hdr = [0u8; 5];
    r.read_exact(&mut hdr)?;
    let len = u32::from_be_bytes(hdr[1..5].try_into().unwrap()) as usize;
    if len > MAX_MSG {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "message too large"));
    }
    let mut payload = vec![0; len];
    r.read_exact(&mut payload)?;
    Ok((hdr[0], payload))
}

fn be_u32(p: &[u8], i: usize) -> u32 {
    u32::from_be_bytes(p[i..i + 4].try_into().unwrap())
}

fn be_f32(p: &[u8], i: usize) -> f32 {
    f32::from_be_bytes(p[i..i + 4].try_into().unwrap())
}

/// Accepts one tablet at a time, forever.
pub fn run() {
    let listener = match TcpListener::bind(("0.0.0.0", PORT)) {
        Ok(l) => l,
        Err(e) => return set_status(format!("Erro ao abrir porta {PORT}: {e}")),
    };
    set_status("Aguardando tablet");
    for stream in listener.incoming().flatten() {
        let peer = stream.peer_addr().map(|a| a.to_string()).unwrap_or_default();
        set_status(format!("Conectado: {peer}"));
        if let Err(e) = handle(stream) {
            eprintln!("session ended: {e}");
        }
        set_status("Aguardando tablet");
    }
}

fn handle(mut stream: TcpStream) -> io::Result<()> {
    stream.set_nodelay(true)?;
    let (kind, hello) = read_msg(&mut stream)?;
    if kind != HELLO || hello.len() != 12 {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "expected HELLO"));
    }
    eprintln!("tablet {}x{} @ {}dpi", be_u32(&hello, 0), be_u32(&hello, 4), be_u32(&hello, 8));

    let mut cap = Capture::primary().map_err(io::Error::other)?;
    let (w, h, rect) = (cap.width, cap.height, cap.rect);
    let mut enc = H264::new(w, h).map_err(io::Error::other)?;
    let mut config = (w as u32).to_be_bytes().to_vec();
    config.extend_from_slice(&(h as u32).to_be_bytes());
    write_msg(&mut stream, CONFIG, &config)?;

    // Tablet -> PC: touch events. Flags `alive` = false when the tablet disconnects.
    let alive = Arc::new(AtomicBool::new(true));
    let mut rd = stream.try_clone()?;
    let reader_alive = alive.clone();
    thread::spawn(move || {
        while let Ok((kind, p)) = read_msg(&mut rd) {
            if kind == TOUCH && p.len() == 9 {
                input::touch(p[0], be_f32(&p, 1), be_f32(&p, 5), rect);
            }
        }
        reader_alive.store(false, Ordering::Relaxed);
    });

    // PC -> tablet: video. The first frame of a fresh duplication is the full desktop, so it's a keyframe start.
    let (mut frame, mut nal) = (Vec::new(), Vec::new());
    let result = (|| {
        while alive.load(Ordering::Relaxed) {
            if !cap.next(&mut frame, 100).map_err(io::Error::other)? {
                continue;
            }
            enc.encode(&frame, &mut nal).map_err(io::Error::other)?;
            if !nal.is_empty() {
                write_msg(&mut stream, VIDEO, &nal)?;
            }
        }
        Ok(())
    })();
    let _ = stream.shutdown(Shutdown::Both);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn framing_roundtrip() {
        let mut buf = Vec::new();
        write_msg(&mut buf, VIDEO, &[1, 2, 3]).unwrap();
        write_msg(&mut buf, TOUCH, &[]).unwrap();
        let mut r = &buf[..];
        assert_eq!(read_msg(&mut r).unwrap(), (VIDEO, vec![1, 2, 3]));
        assert_eq!(read_msg(&mut r).unwrap(), (TOUCH, vec![]));
        assert!(read_msg(&mut r).is_err());

        let huge = [VIDEO, 0xff, 0xff, 0xff, 0xff];
        assert!(read_msg(&mut &huge[..]).is_err());
    }
}
