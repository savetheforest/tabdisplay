//! TCP server + wire protocol. See PROTOCOL.md.
use crate::encode::H264;
use crate::settings::{self, Encoder, Mode, Settings};
use crate::win::{capture::Capture, display, encode::MfEncoder, input};
use std::io::{self, Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

pub const PORT: u16 = 7070;
pub const HELLO: u8 = 1;
pub const VIDEO: u8 = 2;
pub const TOUCH: u8 = 3;
pub const CONFIG: u8 = 4;
const MAX_MSG: usize = 16 << 20;

pub static STATUS: Mutex<String> = Mutex::new(String::new());

fn set_status(s: impl Into<String>) {
    let s = s.into();
    eprintln!("status: {s}");
    *STATUS.lock().unwrap() = s;
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

pub const BEACON_PORT: u16 = 7071;

/// Announces this PC on the LAN once a second so tablets can list it without typing an IP.
/// Payload: `TABDISPLAY <computer name>`; the tablet takes the address from the packet's source.
pub fn beacon() {
    let name = std::env::var("COMPUTERNAME").unwrap_or_else(|_| "PC".into());
    let msg = format!("TABDISPLAY {name}");
    let Ok(sock) = std::net::UdpSocket::bind("0.0.0.0:0") else { return };
    let _ = sock.set_broadcast(true);
    loop {
        let _ = sock.send_to(msg.as_bytes(), ("255.255.255.255", BEACON_PORT));
        thread::sleep(Duration::from_secs(1));
    }
}

/// Accepts one tablet at a time, forever.
pub fn run() {
    let listener = match TcpListener::bind(("0.0.0.0", PORT)) {
        Ok(l) => l,
        Err(e) => return set_status(format!("Erro ao abrir porta {PORT}: {e}")),
    };
    set_status("Aguardando tablet");
    for stream in listener.incoming().flatten() {
        match handle(stream) {
            // The tablet's USB probe connects and hangs up without a HELLO: not a session.
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => continue,
            Err(e) => eprintln!("session ended: {e}"),
            Ok(()) => {}
        }
        set_status("Aguardando tablet");
    }
}

type Rect = (i32, i32, i32, i32);

fn handle(mut stream: TcpStream) -> io::Result<()> {
    stream.set_nodelay(true)?;
    let (kind, hello) = read_msg(&mut stream)?;
    if kind != HELLO || hello.len() != 12 {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "expected HELLO"));
    }
    let peer = stream.peer_addr().map(|a| a.ip().to_string()).unwrap_or_default();
    set_status(format!("Conectado: {}", if peer == "127.0.0.1" { "USB".to_string() } else { peer }));
    let tablet = (be_u32(&hello, 0), be_u32(&hello, 4));
    eprintln!("tablet {}x{} @ {}dpi", tablet.0, tablet.1, be_u32(&hello, 8));
    if display::ensure_modes(&[tablet]).unwrap_or(false) {
        eprintln!("new resolution written to the driver; it needs a restart to offer it");
    }

    // Tablet -> PC: touches go to whatever rect is being streamed (None = touch off).
    // `alive` turns false when the tablet disconnects.
    let target: Arc<Mutex<Option<Rect>>> = Arc::new(Mutex::new(None));
    let alive = Arc::new(AtomicBool::new(true));
    let (mut rd, reader_target, reader_alive) = (stream.try_clone()?, target.clone(), alive.clone());
    thread::spawn(move || {
        while let Ok((kind, p)) = read_msg(&mut rd) {
            if let (TOUCH, 9, Some(rect)) = (kind, p.len(), *reader_target.lock().unwrap()) {
                input::touch(p[0], be_f32(&p, 1), be_f32(&p, 5), rect);
            }
        }
        reader_alive.store(false, Ordering::Relaxed);
    });

    let mut result = Ok(());
    while alive.load(Ordering::Relaxed) && result.is_ok() {
        result = stream_once(&mut stream, tablet, &target, &alive);
    }
    let _ = stream.shutdown(Shutdown::Both);
    result
}

/// Streams with the current settings until they change, capture is lost, or the tablet leaves.
fn stream_once(stream: &mut TcpStream, tablet: (u32, u32), target: &Mutex<Option<Rect>>, alive: &AtomicBool) -> io::Result<()> {
    let version = settings::VERSION.load(Ordering::Relaxed);
    let s = settings::get();

    // Dropping `_virtual` at the end of this function detaches the virtual monitor.
    let (_virtual, device, mut label) = match s.mode {
        Mode::Extend => {
            let (w, h) = fit(s.resolution.unwrap_or(tablet), tablet);
            let _ = display::ensure_modes(&[(w, h)]); // a new mode needs a driver restart; attach says so
            match display::attach(w, h, s.fps.max(60), s.position) {
                Ok(d) => {
                    let dev = d.device.clone();
                    (Some(d), Some(dev), "Estendendo".to_string())
                }
                Err(e) => (None, None, format!("Espelhando (estender falhou: {e})")),
            }
        }
        Mode::Mirror => (None, s.mirror_monitor.clone(), "Espelhando".to_string()),
    };
    let mut cap = open_capture(device.as_deref())?;
    *target.lock().unwrap() = s.touch.then_some(cap.rect);
    let (w, h) = (cap.width, cap.height);
    let (mut encode, kind) = encoder(w, h, &s)?;
    label += &format!(" {w}x{h} · {} fps · {} Mbps · {kind}", s.fps, s.bitrate_mbps);
    set_status(label);

    let mut config = (w as u32).to_be_bytes().to_vec();
    config.extend_from_slice(&(h as u32).to_be_bytes());
    write_msg(stream, CONFIG, &config)?;

    // Frame pacing: capture as fast as the desktop updates, send at most `fps`, and never
    // drop the last update of a burst (it's sent once the interval has passed).
    let interval = Duration::from_secs(1) / s.fps.max(1);
    let (mut frame, mut nal) = (Vec::new(), Vec::new());
    let (mut last, mut pending) = (Instant::now() - interval, false);
    // Hardware decoders (the tablet's MediaTek one) hold a few frames before showing them, so a lone
    // update (cursor move, a typed letter) would sit in the decoder until the screen changes again.
    // Repeating the last frame for a moment pushes it out; identical frames encode to a few bytes.
    const FLUSH: Duration = Duration::from_millis(250);
    let mut last_change = Instant::now() - FLUSH;
    // Fell back to mirroring because the driver was down: switch to extending once it's back.
    let waiting_for_driver = s.mode == Mode::Extend && _virtual.is_none() && display::find_device().is_none();
    let mut next_check = Instant::now();
    while alive.load(Ordering::Relaxed) && settings::VERSION.load(Ordering::Relaxed) == version {
        if waiting_for_driver && Instant::now() >= next_check {
            if display::find_device().is_some() {
                break;
            }
            next_check = Instant::now() + Duration::from_secs(1);
        }
        let wait = interval.saturating_sub(last.elapsed()).as_millis().max(1) as u32;
        match cap.next(&mut frame, wait) {
            Ok(true) => (pending, last_change) = (true, Instant::now()),
            Ok(false) => pending |= last_change.elapsed() < FLUSH,
            Err(e) => {
                // e.g. ACCESS_LOST on a mode change, UAC prompt or lock screen: rebuild.
                eprintln!("capture lost: {e}");
                thread::sleep(Duration::from_millis(300));
                break;
            }
        }
        if pending && last.elapsed() >= interval {
            encode(&frame, &mut nal)?;
            if !nal.is_empty() {
                write_msg(stream, VIDEO, &nal)?;
            }
            (last, pending) = (Instant::now(), false);
        }
    }
    *target.lock().unwrap() = None;
    Ok(())
}

/// Shrinks `size` (keeping its aspect ratio) until the tablet can decode it. The tablet reports its
/// largest decodable size in HELLO; either orientation of that fits.
fn fit((w, h): (u32, u32), (tw, th): (u32, u32)) -> (u32, u32) {
    let (long, short) = (tw.max(th) as f64, tw.min(th) as f64);
    let scale = (long / w.max(h) as f64).min(short / w.min(h) as f64).min(1.0);
    let round = |v: u32| (v as f64 * scale) as u32 & !15;
    if scale >= 1.0 { (w, h) } else { (round(w), round(h)) }
}

/// A just-attached monitor takes a moment to show up in DXGI; unknown monitors fall back to primary.
fn open_capture(device: Option<&str>) -> io::Result<Capture> {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        match Capture::open(device) {
            Ok(c) => return Ok(c),
            Err(_) if Instant::now() < deadline => thread::sleep(Duration::from_millis(100)),
            Err(e) if device.is_some() => {
                eprintln!("capture {device:?} failed ({e}), using primary");
                return Capture::open(None).map_err(io::Error::other);
            }
            Err(e) => return Err(io::Error::other(e)),
        }
    }
}

type EncodeFn = Box<dyn FnMut(&[u8], &mut Vec<u8>) -> io::Result<()>>;

/// Per the settings: GPU (Media Foundation), CPU (openh264), or GPU falling back to CPU.
fn encoder(w: usize, h: usize, s: &Settings) -> io::Result<(EncodeFn, &'static str)> {
    let bitrate = s.bitrate_mbps * 1_000_000;
    if s.encoder != Encoder::Cpu {
        match MfEncoder::new(w, h, s.fps, bitrate) {
            Ok(mut e) => return Ok((Box::new(move |f, out| e.encode(f, out).map_err(io::Error::other)), "GPU")),
            Err(err) if s.encoder == Encoder::Gpu => return Err(io::Error::other(err)),
            Err(err) => eprintln!("hardware encoder unavailable ({err}), using openh264"),
        }
    }
    let mut e = H264::new(w, h, s.fps, bitrate).map_err(io::Error::other)?;
    Ok((Box::new(move |f, out| e.encode(f, out).map_err(io::Error::other)), "CPU"))
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

    #[test]
    fn fit_to_tablet_decoder() {
        let tablet = (2304, 1440);
        assert_eq!(fit((1920, 1200), tablet), (1920, 1200));
        assert_eq!(fit((1200, 1920), tablet), (1200, 1920)); // portrait fits the swapped limit
        assert_eq!(fit((2560, 1600), tablet), (2304, 1440));
        assert_eq!(fit((2560, 1440), tablet), (2304, 1296));
    }
}
