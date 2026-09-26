//! TCP server + wire protocol. See PROTOCOL.md.
use crate::encode::H264;
use crate::pairing::{self, Check};
use crate::settings::{self, Encoder, Mode, Profile, Settings, TouchMode};
use crate::win::{capture::Capture, display, encode::MfEncoder, input::Injector};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{self, Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

pub const PORT: u16 = 7070;
pub const BEACON_PORT: u16 = 7071;
const PROTOCOL: u32 = 2;

// Message types. Control messages carry JSON; VIDEO and INPUT are binary. See PROTOCOL.md.
pub const HELLO: u8 = 1;
pub const VIDEO: u8 = 2;
pub const INPUT: u8 = 3;
pub const CONFIG: u8 = 4;
pub const PAIR_REQUIRED: u8 = 5;
pub const PAIR: u8 = 6;
pub const PAIRED: u8 = 7;
pub const ERROR: u8 = 8;
pub const RESIZE: u8 = 9;
pub const PING: u8 = 10;
pub const PONG: u8 = 11;
pub const STATS_MSG: u8 = 12;
const MAX_MSG: usize = 16 << 20;

pub static STATUS: Mutex<String> = Mutex::new(String::new());
/// "Redmi Pad 2 · USB" while a tablet is connected.
pub static SESSION: Mutex<Option<String>> = Mutex::new(None);

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

fn send_json(w: &mut impl Write, kind: u8, v: &Value) -> io::Result<()> {
    write_msg(w, kind, &serde_json::to_vec(v)?)
}

/// Tells the tablet why the session ends (it shows `message` as is), then fails the session.
fn refuse(w: &mut impl Write, message: &str) -> io::Error {
    let _ = send_json(w, ERROR, &json!({ "message": message }));
    io::Error::new(io::ErrorKind::PermissionDenied, message.to_string())
}


pub fn computer_name() -> String {
    std::env::var("COMPUTERNAME").or_else(|_| std::env::var("HOSTNAME")).unwrap_or_else(|_| "PC".into())
}

/// Announces this PC on the LAN once a second so tablets can list it without typing an IP.
/// Payload: `TABDISPLAY {"id":…,"name":…}`; the tablet takes the address from the packet's source.
pub fn beacon() {
    let msg = format!("TABDISPLAY {}", json!({ "id": pairing::pc_id(), "name": computer_name() }));
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

#[derive(Deserialize)]
struct Hello {
    v: u32,
    device_id: String,
    device_name: String,
    #[serde(default)]
    token: String,
    /// Largest size with the tablet screen's aspect ratio that its decoder handles.
    decodable: (u32, u32),
}

use crate::win::input::Rect;
/// Where tablet input goes and whether touch acts as a mouse; None = input off.
type Target = Option<(Rect, bool)>;

/// Live numbers for the UI (None when no tablet is connected).
#[derive(Clone, Default, Serialize)]
pub struct Stats {
    pub width: usize,
    pub height: usize,
    /// Frames sent per second.
    pub fps: f32,
    pub mbps: f32,
    pub encode_ms: f32,
    /// Round trip PC -> tablet -> PC.
    pub rtt_ms: u32,
    /// Frames the tablet actually showed per second.
    pub tablet_fps: u32,
}

pub static STATS: Mutex<Option<Stats>> = Mutex::new(None);

/// What a session's reader thread (tablet -> PC) shares with its streaming loop.
struct Shared {
    start: Instant,
    alive: AtomicBool,
    target: Mutex<Target>,
    /// The tablet's decodable size; changes when it rotates (RESIZE).
    tablet: Mutex<(u32, u32)>,
    /// Set by RESIZE: rebuild the pipeline for the new size.
    rebuild: AtomicBool,
    rtt_ms: AtomicU32,
    tablet_fps: AtomicU32,
}

fn handle(mut stream: TcpStream) -> io::Result<()> {
    stream.set_nodelay(true)?;
    let (kind, payload) = read_msg(&mut stream)?;
    let hello = match serde_json::from_slice::<Hello>(&payload) {
        Ok(h) if kind == HELLO && h.v == PROTOCOL => h,
        _ => return Err(refuse(&mut stream, "Versões diferentes do TabDisplay no PC e no tablet: atualize os dois.")),
    };
    // USB arrives through `adb reverse` on loopback: the cable is proof enough. Wi-Fi needs pairing.
    let usb = stream.peer_addr()?.ip().is_loopback();
    if !usb && !pairing::is_paired(&hello.device_id, &hello.token) {
        pair(&mut stream, &hello).inspect_err(|_| pairing::cancel())?;
    }
    let session = format!("{} · {}", hello.device_name, if usb { "USB" } else { "Wi‑Fi" });
    set_status(format!("Conectado: {session}"));
    *SESSION.lock().unwrap() = Some(session);

    let shared = Arc::new(Shared {
        start: Instant::now(),
        alive: AtomicBool::new(true),
        target: Mutex::new(None),
        tablet: Mutex::new(hello.decodable),
        rebuild: AtomicBool::new(false),
        rtt_ms: AtomicU32::new(0),
        tablet_fps: AtomicU32::new(0),
    });
    let (mut rd, reader) = (stream.try_clone()?, shared.clone());
    thread::spawn(move || {
        let mut injector = Injector::default();
        while let Ok((kind, p)) = read_msg(&mut rd) {
            match kind {
                INPUT => {
                    let target = *reader.target.lock().unwrap();
                    if let (Some((rect, as_mouse)), Some(frame)) = (target, crate::input::parse(&p)) {
                        injector.inject(&frame, rect, as_mouse);
                    }
                }
                RESIZE => {
                    if let Ok(v) = serde_json::from_slice::<Value>(&p) {
                        if let (Some(w), Some(h)) = (v["decodable"][0].as_u64(), v["decodable"][1].as_u64()) {
                            *reader.tablet.lock().unwrap() = (w as u32, h as u32);
                            reader.rebuild.store(true, Ordering::Relaxed);
                        }
                    }
                }
                PONG => {
                    if let Some(t) = serde_json::from_slice::<Value>(&p).ok().and_then(|v| v["t"].as_u64()) {
                        let rtt = (reader.start.elapsed().as_millis() as u64).saturating_sub(t);
                        reader.rtt_ms.store(rtt as u32, Ordering::Relaxed);
                    }
                }
                STATS_MSG => {
                    if let Some(fps) = serde_json::from_slice::<Value>(&p).ok().and_then(|v| v["fps"].as_u64()) {
                        reader.tablet_fps.store(fps as u32, Ordering::Relaxed);
                    }
                }
                _ => {}
            }
        }
        reader.alive.store(false, Ordering::Relaxed);
    });

    // The virtual monitor lives for the whole session; rebuilds only change its mode.
    let mut virtual_display = None;
    let mut result = Ok(());
    while shared.alive.load(Ordering::Relaxed) && result.is_ok() {
        result = stream_once(&mut stream, &shared, &mut virtual_display);
    }
    *STATS.lock().unwrap() = None;
    *SESSION.lock().unwrap() = None;
    drop(virtual_display);
    let _ = stream.shutdown(Shutdown::Both);
    result
}

/// Size, fps and Mbps for the chosen quality profile.
fn plan(s: &Settings, tablet: (u32, u32)) -> ((u32, u32), u32, u32) {
    match s.profile {
        Profile::Performance => ((tablet.0 / 2 & !15, tablet.1 / 2 & !15), 60, 10),
        Profile::Balanced => (tablet, 60, 20),
        Profile::Quality => (tablet, 60, 40),
        Profile::Custom => (fit(s.resolution.unwrap_or(tablet), tablet), s.fps, s.bitrate_mbps),
    }
}

/// Shows a code on the PC and waits for the tablet to send it back. Ok = paired (token sent).
fn pair(stream: &mut TcpStream, hello: &Hello) -> io::Result<()> {
    set_status(format!("Pareando com {}", hello.device_name));
    pairing::start(&hello.device_name);
    crate::show_window(); // the code is on the PC screen; the window may be in the tray
    let ask = |wrong: bool| json!({ "pc_id": pairing::pc_id(), "pc_name": computer_name(), "wrong": wrong });
    send_json(stream, PAIR_REQUIRED, &ask(false))?;
    stream.set_read_timeout(Some(Duration::from_secs(180)))?;
    loop {
        let (kind, p) = read_msg(stream)?;
        if kind != PAIR {
            continue;
        }
        let code = serde_json::from_slice::<Value>(&p).ok().and_then(|v| v["code"].as_str().map(String::from)).unwrap_or_default();
        match pairing::check(&code) {
            Check::Ok => {
                let token = pairing::complete(&hello.device_id, &hello.device_name);
                send_json(stream, PAIRED, &json!({ "pc_id": pairing::pc_id(), "token": token }))?;
                return stream.set_read_timeout(None);
            }
            Check::Wrong => send_json(stream, PAIR_REQUIRED, &ask(true))?,
            Check::Over => return Err(refuse(stream, "O código expirou ou teve tentativas demais. Conecte de novo para gerar outro.")),
        }
    }
}

/// Streams with the current settings until they change, the tablet rotates, capture is lost, or the
/// tablet leaves.
fn stream_once(stream: &mut TcpStream, shared: &Shared, virtual_display: &mut Option<display::VirtualDisplay>) -> io::Result<()> {
    let version = settings::VERSION.load(Ordering::Relaxed);
    let s = settings::get();
    let tablet = *shared.tablet.lock().unwrap();
    let ((w, h), fps, mbps) = plan(&s, tablet);

    let mut label = match s.mode {
        Mode::Extend => {
            let hz = fps.max(60);
            let configured = match virtual_display {
                Some(d) => d.configure(w, h, hz, s.position),
                None => display::attach(w, h, hz, s.position).map(|d| *virtual_display = Some(d)),
            };
            match configured {
                Ok(()) => "Estendendo".to_string(),
                Err(e) => {
                    *virtual_display = None;
                    format!("Espelhando (estender falhou: {e})")
                }
            }
        }
        Mode::Mirror => {
            *virtual_display = None;
            "Espelhando".to_string()
        }
    };
    let device = match virtual_display {
        Some(d) => Some(d.device.clone()),
        None if s.mode == Mode::Mirror => s.mirror_monitor.clone(),
        None => None,
    };
    let mut cap = open_capture(device.as_deref())?;
    *shared.target.lock().unwrap() = s.touch.then_some((cap.rect, s.touch_mode == TouchMode::Mouse));
    let (cw, ch) = (cap.width, cap.height);
    let (mut encode, kind) = encoder(cw, ch, fps, mbps, s.encoder)?;
    label += &format!(" {cw}x{ch} · {fps} fps · {mbps} Mbps · {kind}");
    set_status(label);

    send_json(stream, CONFIG, &json!({ "width": cw, "height": ch }))?;

    // Frame pacing: capture as fast as the desktop updates, send at most `fps`, and never
    // drop the last update of a burst (it's sent once the interval has passed).
    let interval = Duration::from_secs(1) / fps.max(1);
    let (mut frame, mut nal) = (Vec::new(), Vec::new());
    let (mut last, mut pending) = (Instant::now() - interval, false);
    // Hardware decoders (the tablet's MediaTek one) hold a few frames before showing them, so a lone
    // update (cursor move, a typed letter) would sit in the decoder until the screen changes again.
    // Repeating the last frame for a moment pushes it out; identical frames encode to a few bytes.
    const FLUSH: Duration = Duration::from_millis(250);
    let mut last_change = Instant::now() - FLUSH;
    // Fell back to mirroring because the driver was down: switch to extending once it's back.
    let waiting_for_driver = s.mode == Mode::Extend && virtual_display.is_none() && display::find_device().is_none();
    // Once a second: ping for the round trip, and publish stats for the UI.
    let mut tick = Instant::now();
    let (mut frames, mut bytes, mut encode_time) = (0u32, 0usize, Duration::ZERO);
    while shared.alive.load(Ordering::Relaxed) && settings::VERSION.load(Ordering::Relaxed) == version && !shared.rebuild.swap(false, Ordering::Relaxed) {
        if tick.elapsed() >= Duration::from_secs(1) {
            if waiting_for_driver && display::find_device().is_some() {
                break;
            }
            let secs = tick.elapsed().as_secs_f32();
            let stats = Stats {
                width: cw,
                height: ch,
                fps: frames as f32 / secs,
                mbps: bytes as f32 * 8.0 / secs / 1e6,
                encode_ms: if frames > 0 { encode_time.as_secs_f32() * 1000.0 / frames as f32 } else { 0.0 },
                rtt_ms: shared.rtt_ms.load(Ordering::Relaxed),
                tablet_fps: shared.tablet_fps.load(Ordering::Relaxed),
            };
            // The tablet echoes `t` (round trip) and may show the rest in its stats overlay.
            let ping = json!({ "t": shared.start.elapsed().as_millis() as u64, "rtt_ms": stats.rtt_ms, "fps": stats.fps.round(), "mbps": stats.mbps });
            *STATS.lock().unwrap() = Some(stats);
            send_json(stream, PING, &ping)?;
            (tick, frames, bytes, encode_time) = (Instant::now(), 0, 0, Duration::ZERO);
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
            let t = Instant::now();
            encode(&frame, &mut nal)?;
            encode_time += t.elapsed();
            if !nal.is_empty() {
                write_msg(stream, VIDEO, &nal)?;
                (frames, bytes) = (frames + 1, bytes + nal.len());
            }
            (last, pending) = (Instant::now(), false);
        }
    }
    *shared.target.lock().unwrap() = None;
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
fn encoder(w: usize, h: usize, fps: u32, mbps: u32, choice: Encoder) -> io::Result<(EncodeFn, &'static str)> {
    let bitrate = mbps * 1_000_000;
    if choice != Encoder::Cpu {
        match MfEncoder::new(w, h, fps, bitrate) {
            Ok(mut e) => return Ok((Box::new(move |f, out| e.encode(f, out).map_err(io::Error::other)), "GPU")),
            Err(err) if choice == Encoder::Gpu => return Err(io::Error::other(err)),
            Err(err) => eprintln!("hardware encoder unavailable ({err}), using openh264"),
        }
    }
    let mut e = H264::new(w, h, fps, bitrate).map_err(io::Error::other)?;
    Ok((Box::new(move |f, out| e.encode(f, out).map_err(io::Error::other)), "CPU"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn framing_roundtrip() {
        let mut buf = Vec::new();
        write_msg(&mut buf, VIDEO, &[1, 2, 3]).unwrap();
        write_msg(&mut buf, INPUT, &[]).unwrap();
        let mut r = &buf[..];
        assert_eq!(read_msg(&mut r).unwrap(), (VIDEO, vec![1, 2, 3]));
        assert_eq!(read_msg(&mut r).unwrap(), (INPUT, vec![]));
        assert!(read_msg(&mut r).is_err());

        let huge = [VIDEO, 0xff, 0xff, 0xff, 0xff];
        assert!(read_msg(&mut &huge[..]).is_err());
    }

    #[test]
    fn quality_profiles() {
        let tablet = (2304, 1440);
        let mut s = Settings::default();
        assert_eq!(plan(&s, tablet), ((2304, 1440), 60, 20)); // Balanced is the default
        s.profile = Profile::Performance;
        assert_eq!(plan(&s, tablet), ((1152, 720), 60, 10));
        s.profile = Profile::Quality;
        assert_eq!(plan(&s, tablet).2, 40);
        s = Settings { profile: Profile::Custom, resolution: Some((2560, 1600)), fps: 90, bitrate_mbps: 30, ..s };
        assert_eq!(plan(&s, tablet), ((2304, 1440), 90, 30)); // custom still fits the decoder
        assert_eq!(plan(&s, (1440, 2304)).0, (2304, 1440)); // a portrait tablet decodes the landscape size too
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
