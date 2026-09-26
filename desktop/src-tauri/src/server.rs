//! TCP server + wire protocol. See PROTOCOL.md.
use crate::encode::H264;
use crate::pairing::{self, Check};
use crate::settings::{self, Encoder, Mode, Profile, Settings, TouchMode};
use crate::sys::{capture::Capture, display, encode::HwEncoder, input::Injector};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{self, Read, Write};
use crate::tls::{self, Accepted, Conn};
use std::net::{Shutdown, TcpListener};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

pub const PORT: u16 = 7070;
pub const BEACON_PORT: u16 = 7071;
const PROTOCOL: u32 = 3;

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
pub const SCROLL: u8 = 13;
pub const PROFILE: u8 = 14;
pub const AUDIO: u8 = 15;
const MAX_MSG: usize = 16 << 20;

/// Tablets that can be connected at once (the Windows driver offers as many virtual monitors).
pub const MAX_TABLETS: usize = 2;

/// What the PC says while no tablet is connected ("Aguardando tablet", a port error...).
pub static STATUS: Mutex<String> = Mutex::new(String::new());

/// What the UI shows about one connected tablet.
#[derive(Clone, Serialize)]
pub struct SessionInfo {
    pub id: u64,
    /// "Redmi Pad 2 · USB"
    pub name: String,
    pub status: String,
    pub stats: Option<Stats>,
}

struct Entry {
    info: SessionInfo,
    device_id: String,
    /// To end the session from outside (the same tablet reconnecting).
    conn: Conn,
}

static SESSIONS: Mutex<Vec<Entry>> = Mutex::new(Vec::new());
static NEXT_ID: AtomicU64 = AtomicU64::new(1);
/// One tablet pairs at a time: there is a single code on the PC screen.
static PAIRING: Mutex<()> = Mutex::new(());

pub fn sessions() -> Vec<SessionInfo> {
    SESSIONS.lock().unwrap().iter().map(|e| e.info.clone()).collect()
}

/// Adds a tablet to the live sessions. A tablet that reconnects replaces its own stale session.
fn register(device_id: &str, name: String, conn: Conn) -> Result<Registered, String> {
    let mut all = SESSIONS.lock().unwrap();
    if let Some(i) = all.iter().position(|e| e.device_id == device_id) {
        let _ = all.remove(i).conn.shutdown(Shutdown::Both);
    }
    if all.len() >= MAX_TABLETS {
        return Err(format!("O TabDisplay já está com {MAX_TABLETS} tablets conectados. Desconecte um deles primeiro."));
    }
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let status = format!("Conectado: {name}");
    eprintln!("status: {status}");
    all.push(Entry { info: SessionInfo { id, name, status, stats: None }, device_id: device_id.into(), conn });
    Ok(Registered(id))
}

/// Removes the session when the connection's thread is done, however it ends.
struct Registered(u64);

impl Drop for Registered {
    fn drop(&mut self) {
        SESSIONS.lock().unwrap().retain(|e| e.info.id != self.0);
    }
}

fn with_session(id: u64, f: impl FnOnce(&mut SessionInfo)) {
    if let Some(e) = SESSIONS.lock().unwrap().iter_mut().find(|e| e.info.id == id) {
        f(&mut e.info);
    }
}

fn set_status(s: impl Into<String>) {
    let s = s.into();
    eprintln!("status: {s}");
    *STATUS.lock().unwrap() = s;
}

fn set_session_status(id: u64, s: impl Into<String>) {
    let s = s.into();
    eprintln!("status [{id}]: {s}");
    with_session(id, |info| info.status = s);
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
    #[cfg(target_os = "macos")]
    if let Ok(o) = std::process::Command::new("scutil").args(["--get", "ComputerName"]).output() {
        let name = String::from_utf8_lossy(&o.stdout).trim().to_string();
        if !name.is_empty() {
            return name;
        }
    }
    std::env::var("COMPUTERNAME").or_else(|_| std::env::var("HOSTNAME")).unwrap_or_else(|_| "PC".into())
}

/// Announces this PC once a second on every network the PC has (Wi‑Fi, Ethernet, and a tablet
/// tethered by USB) so tablets can list it without typing an IP. A plain broadcast to
/// 255.255.255.255 only leaves through whichever interface owns the default route, which usually
/// isn't the USB one (tethering rarely offers internet access), so each interface's own directed
/// broadcast address is used instead: the OS routes it out that interface because the address is
/// only reachable there. Payload: `TABDISPLAY {"id":…,"name":…}`; the tablet takes the address
/// from the packet's source.
pub fn beacon() {
    let msg = format!("TABDISPLAY {}", json!({ "id": pairing::pc_id(), "name": computer_name() }));
    loop {
        for iface in if_addrs::get_if_addrs().unwrap_or_default() {
            let if_addrs::IfAddr::V4(v4) = iface.addr else { continue };
            let Some(broadcast) = v4.broadcast else { continue };
            if v4.ip.is_loopback() {
                continue;
            }
            if let Ok(sock) = std::net::UdpSocket::bind((v4.ip, 0)) {
                let _ = sock.set_broadcast(true);
                let _ = sock.send_to(msg.as_bytes(), (broadcast, BEACON_PORT));
            }
        }
        thread::sleep(Duration::from_secs(1));
    }
}

/// Accepts tablets forever; each connection gets its own thread.
pub fn run() {
    let listener = match TcpListener::bind(("0.0.0.0", PORT)) {
        Ok(l) => l,
        Err(e) => return set_status(format!("Erro ao abrir porta {PORT}: {e}")),
    };
    set_status("Aguardando tablet");
    for stream in listener.incoming().flatten() {
        thread::spawn(move || {
            let conn = match tls::accept(stream) {
                Ok(Accepted::Tls(conn)) => conn,
                Ok(Accepted::Plain(mut old)) => {
                    // Protocol 2 was plaintext: the message reaches the old app as a plain ERROR.
                    let _ = refuse(&mut old, "Este TabDisplay do PC é mais novo (conexão criptografada): atualize o app do tablet.");
                    return;
                }
                Err(_) => return, // probe or a client that fumbled the handshake: not a session
            };
            match handle(conn) {
                // The tablet's USB probe connects and hangs up without a HELLO: not a session.
                Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => {}
                Err(e) => eprintln!("session ended: {e}"),
                Ok(()) => {}
            }
        });
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

use crate::sys::input::Rect;
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
    /// "GPU" or "CPU": which encoder produced the current stream.
    pub encoder_kind: &'static str,
    /// Round trip PC -> tablet -> PC.
    pub rtt_ms: u32,
    /// Frames the tablet actually showed per second.
    pub tablet_fps: u32,
    /// The Auto profile is running below Balanced because the network is struggling.
    pub reduced: bool,
}


/// What a session's reader thread (tablet -> PC) shares with its streaming loop.
struct Shared {
    /// Key of this tablet's entry in `SESSIONS`.
    id: u64,
    start: Instant,
    alive: AtomicBool,
    target: Mutex<Target>,
    /// The tablet's decodable size; changes when it rotates (RESIZE).
    tablet: Mutex<(u32, u32)>,
    /// Set by RESIZE: rebuild the pipeline for the new size.
    rebuild: AtomicBool,
    rtt_ms: AtomicU32,
    tablet_fps: AtomicU32,
    /// State of the Auto quality profile; outlives rebuilds so a step change sticks.
    adapt: Mutex<Adapt>,
}

/// Auto quality: the presets from best (0) to lightest. Steps down when the link struggles for a few seconds,
/// back up after a long calm stretch.
const LADDER: [Profile; 3] = [Profile::Quality, Profile::Balanced, Profile::Performance];
const ADAPT_START: usize = 1;

struct Adapt {
    level: usize,
    bad: u32,
    good: u32,
    /// Seconds left during which stepping back up is not allowed (after a step down).
    hold: u32,
}

impl Adapt {
    fn new() -> Self {
        Self { level: ADAPT_START, bad: 0, good: 0, hold: 0 }
    }

    /// Feeds one second of stats; true if the level changed. `sent_fps` is what the PC sent, `shown_fps` what
    /// the tablet displayed (a static desktop sends few frames, so only compare them when the PC is busy).
    fn tick(&mut self, rtt_ms: u32, sent_fps: f32, shown_fps: u32) -> bool {
        let struggling = rtt_ms > 150 || (sent_fps >= 10.0 && (shown_fps as f32) < sent_fps * 0.6);
        (self.bad, self.good) = if struggling { (self.bad + 1, 0) } else { (0, self.good + 1) };
        self.hold = self.hold.saturating_sub(1);
        if self.bad >= 3 && self.level + 1 < LADDER.len() {
            (self.level, self.bad, self.hold) = (self.level + 1, 0, 60);
            return true;
        }
        if self.good >= 20 && self.hold == 0 && self.level > 0 {
            (self.level, self.good) = (self.level - 1, 0);
            return true;
        }
        false
    }
}

fn handle(mut stream: Conn) -> io::Result<()> {
    let (kind, payload) = read_msg(&mut stream)?;
    let hello = match serde_json::from_slice::<Hello>(&payload) {
        Ok(h) if kind == HELLO && h.v == PROTOCOL => h,
        _ => return Err(refuse(&mut stream, "Versões diferentes do TabDisplay no PC e no tablet: atualize os dois.")),
    };
    // USB arrives through `adb reverse` on loopback: the cable is proof enough. Wi-Fi needs pairing.
    let usb = stream.peer_addr()?.ip().is_loopback();
    if !usb && !pairing::is_paired(&hello.device_id, &hello.token) {
        let Ok(_pairing) = PAIRING.try_lock() else {
            return Err(refuse(&mut stream, "Outro tablet está sendo pareado agora. Tente de novo em instantes."));
        };
        pair(&mut stream, &hello).inspect_err(|_| pairing::cancel())?;
    }
    let session = format!("{} · {}", hello.device_name, if usb { "USB" } else { "Wi‑Fi" });
    let registered = match register(&hello.device_id, session, stream.try_clone()?) {
        Ok(r) => r,
        Err(message) => return Err(refuse(&mut stream, &message)),
    };
    let id = registered.0;

    let shared = Arc::new(Shared {
        id,
        start: Instant::now(),
        alive: AtomicBool::new(true),
        target: Mutex::new(None),
        tablet: Mutex::new(hello.decodable),
        rebuild: AtomicBool::new(false),
        rtt_ms: AtomicU32::new(0),
        tablet_fps: AtomicU32::new(0),
        adapt: Mutex::new(Adapt::new()),
    });
    // System audio: captured and encoded on its own thread; `stream_once` writes the packets.
    let (alive, audio_on) = (shared.clone(), || settings::get().audio);
    let audio = crate::audio::spawn(move || alive.alive.load(Ordering::Relaxed), audio_on);
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
                PROFILE => {
                    // The tablet picked a quality preset: same effect as the PC UI (rebuilds the session).
                    if let Some(profile) = preset(&p) {
                        let mut s = settings::get();
                        if s.profile != profile {
                            s.profile = profile;
                            settings::set(s);
                        }
                    }
                }
                SCROLL => {
                    let target = *reader.target.lock().unwrap();
                    if let (Some((rect, _)), Some(s)) = (target, crate::input::parse_scroll(&p)) {
                        injector.scroll(&s, rect);
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
        result = stream_once(&mut stream, &shared, &mut virtual_display, &audio);
    }
    drop(virtual_display);
    drop(registered);
    let _ = stream.shutdown(Shutdown::Both);
    result
}

/// The preset in a PROFILE message from the tablet; Custom is only settable from the PC.
fn preset(payload: &[u8]) -> Option<Profile> {
    let v: Value = serde_json::from_slice(payload).ok()?;
    match serde_json::from_value(v["profile"].clone()).ok()? {
        Profile::Custom => None,
        p => Some(p),
    }
}

/// Size, fps and Mbps for the chosen quality profile.
fn plan(s: &Settings, tablet: (u32, u32), auto_level: usize) -> ((u32, u32), u32, u32) {
    match s.profile {
        Profile::Auto => plan(&Settings { profile: LADDER[auto_level], ..s.clone() }, tablet, auto_level),
        Profile::Performance => ((tablet.0 / 2 & !15, tablet.1 / 2 & !15), 60, 10),
        Profile::Balanced => (tablet, 60, 20),
        Profile::Quality => (tablet, 60, 40),
        Profile::Custom => (fit(s.resolution.unwrap_or(tablet), tablet), s.fps, s.bitrate_mbps),
    }
}

/// Shows a code on the PC and waits for the tablet to send it back. Ok = paired (token sent).
fn pair(stream: &mut Conn, hello: &Hello) -> io::Result<()> {
    set_status(format!("Pareando com {}", hello.device_name));
    let code = pairing::start(&hello.device_name);
    if std::env::var_os("TABDISPLAY_DEBUG").is_some() {
        eprintln!("pairing code: {code}"); // for testing on a machine whose screen we can't see
    }
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
fn stream_once(stream: &mut Conn, shared: &Shared, virtual_display: &mut Option<display::VirtualDisplay>, audio: &Receiver<Vec<u8>>) -> io::Result<()> {
    let version = settings::VERSION.load(Ordering::Relaxed);
    let s = settings::get();
    let tablet = *shared.tablet.lock().unwrap();
    let level = shared.adapt.lock().unwrap().level;
    let ((w, h), fps, mbps) = plan(&s, tablet, level);

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
    let mut cap = open_capture(device.as_deref(), tablet)?;
    *shared.target.lock().unwrap() = s.touch.then_some((cap.rect, s.touch_mode == TouchMode::Mouse));
    let (cw, ch) = (cap.width, cap.height);
    let (mut encode, kind) = encoder(cw, ch, fps, mbps, s.encoder)?;
    label += &format!(" {cw}x{ch} · {fps} fps · {mbps} Mbps · {kind}");
    set_session_status(shared.id, label);

    send_json(stream, CONFIG, &json!({ "width": cw, "height": ch }))?;
    // Tell the tablet which preset is active (a rebuild after either side changed it lands here).
    send_json(stream, PROFILE, &json!({ "profile": s.profile }))?;

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
    let waiting_for_driver = s.mode == Mode::Extend && virtual_display.is_none() && display::find_devices().is_empty();
    // Once a second: ping for the round trip, and publish stats for the UI.
    let mut tick = Instant::now();
    let (mut frames, mut bytes, mut encode_time) = (0u32, 0usize, Duration::ZERO);
    while shared.alive.load(Ordering::Relaxed) && settings::VERSION.load(Ordering::Relaxed) == version && !shared.rebuild.swap(false, Ordering::Relaxed) {
        if tick.elapsed() >= Duration::from_secs(1) {
            if waiting_for_driver && !display::find_devices().is_empty() {
                break;
            }
            let secs = tick.elapsed().as_secs_f32();
            let stats = Stats {
                width: cw,
                height: ch,
                fps: frames as f32 / secs,
                mbps: bytes as f32 * 8.0 / secs / 1e6,
                encode_ms: if frames > 0 { encode_time.as_secs_f32() * 1000.0 / frames as f32 } else { 0.0 },
                encoder_kind: kind,
                rtt_ms: shared.rtt_ms.load(Ordering::Relaxed),
                tablet_fps: shared.tablet_fps.load(Ordering::Relaxed),
                reduced: s.profile == Profile::Auto && level > ADAPT_START,
            };
            if s.profile == Profile::Auto && shared.adapt.lock().unwrap().tick(stats.rtt_ms, stats.fps, stats.tablet_fps) {
                shared.rebuild.store(true, Ordering::Relaxed);
            }
            // The tablet echoes `t` (round trip) and may show the rest in its stats overlay.
            let ping = json!({ "t": shared.start.elapsed().as_millis() as u64, "rtt_ms": stats.rtt_ms, "fps": stats.fps.round(), "mbps": stats.mbps });
            with_session(shared.id, |info| info.stats = Some(stats.clone()));
            send_json(stream, PING, &ping)?;
            (tick, frames, bytes, encode_time) = (Instant::now(), 0, 0, Duration::ZERO);
        }
        for packet in audio.try_iter() {
            if s.audio {
                write_msg(stream, AUDIO, &packet)?;
            }
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
fn open_capture(device: Option<&str>, tablet: (u32, u32)) -> io::Result<Capture> {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        match Capture::open(device, |size| fit(size, tablet)) {
            Ok(c) => return Ok(c),
            Err(_) if Instant::now() < deadline => thread::sleep(Duration::from_millis(100)),
            Err(e) if device.is_some() => {
                eprintln!("capture {device:?} failed ({e}), using primary");
                return Capture::open(None, |size| fit(size, tablet)).map_err(io::Error::other);
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
        match HwEncoder::new(w, h, fps, bitrate) {
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
        assert_eq!(plan(&s, tablet, 1), ((2304, 1440), 60, 20)); // Balanced is the default
        s.profile = Profile::Performance;
        assert_eq!(plan(&s, tablet, 1), ((1152, 720), 60, 10));
        s.profile = Profile::Quality;
        assert_eq!(plan(&s, tablet, 1).2, 40);
        s = Settings { profile: Profile::Custom, resolution: Some((2560, 1600)), fps: 90, bitrate_mbps: 30, ..s };
        assert_eq!(plan(&s, tablet, 1), ((2304, 1440), 90, 30)); // custom still fits the decoder
        assert_eq!(plan(&s, (1440, 2304), 1).0, (2304, 1440)); // a portrait tablet decodes the landscape size too
    }

    #[test]
    fn auto_steps_down_on_bad_link_and_back_up_when_calm() {
        let mut a = Adapt::new();
        assert!(!a.tick(200, 0.0, 0) && !a.tick(200, 0.0, 0)); // two bad seconds: not yet
        assert!(a.tick(200, 0.0, 0) && a.level == 2); // third: Balanced -> Performance
        assert!(!a.tick(200, 0.0, 0) && a.level == 2); // already at the lightest
        for _ in 0..50 {
            a.tick(20, 30.0, 30); // calm, but still held
        }
        assert_eq!(a.level, 2);
        let mut up = false;
        for _ in 0..40 {
            up |= a.tick(20, 30.0, 30);
        }
        assert!(up && a.level < 2);
        // The tablet showing far fewer frames than were sent counts as struggling too.
        let mut b = Adapt::new();
        assert!(!b.tick(10, 60.0, 20) && !b.tick(10, 60.0, 20) && b.tick(10, 60.0, 20));
    }

    #[test]
    fn tablet_can_pick_presets_but_not_custom() {
        assert!(preset(br#"{"profile":"quality"}"#) == Some(Profile::Quality));
        assert!(preset(br#"{"profile":"custom"}"#).is_none());
        assert!(preset(br#"{"profile":"nope"}"#).is_none());
        assert!(preset(b"x").is_none());
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
