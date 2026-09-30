//! TCP server + wire protocol. See PROTOCOL.md.
use crate::encode::H264;
use crate::pairing::{self, Check};
use crate::settings::{self, Encoder, Mode, Profile, Settings, TouchMode};
use crate::sys::{capture::Capture, display, encode::HwEncoder, input::Injector};
use crate::tls::{self, Accepted, Conn};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{self, Read, Write};
use std::net::{Shutdown, TcpListener};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
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
pub const KEYFRAME: u8 = 16;
pub const PAUSE: u8 = 17;
pub const RESUME: u8 = 18;
/// One-shot, explicitly requested plain-text transfer. It is never auto-pasted or persisted.
pub const TEXT: u8 = 19;
/// Logical keyboard/text input, only for clients advertising the keyboard capability.
pub const KEY: u8 = 20;
const MAX_MSG: usize = 16 << 20;
const MAX_CONTROL: usize = 64 << 10;
pub const MAX_TEXT: usize = 16 << 10;
const MAX_VIDEO: usize = 8 << 20;
const MAX_AUDIO: usize = 256 << 10;
const MAX_INPUT: usize = 1 + 255 * 18;
const MIN_DIMENSION: u32 = 16;
const MAX_DIMENSION: u32 = 7680;
const MAX_PIXELS: u64 = 16_777_216;
const MAX_FPS: u32 = 240;
const HELLO_DEADLINE: Duration = Duration::from_secs(10);
const PAIR_DEADLINE: Duration = Duration::from_secs(180);
const HEARTBEAT_TIMEOUT: Duration = Duration::from_secs(15);

/// Tablets that can be connected at once (the Windows driver offers as many virtual monitors).
pub const MAX_TABLETS: usize = 2;
const MAX_HANDSHAKES: usize = 16;
static HANDSHAKES: AtomicUsize = AtomicUsize::new(0);

/// What the PC says while no tablet is connected ("Aguardando tablet", a port error...).
pub static STATUS: Mutex<String> = Mutex::new(String::new());

/// What the UI shows about one connected tablet.
#[derive(Clone, Serialize)]
pub struct SessionInfo {
    pub id: u64,
    /// Stable authenticated device identifier; exposed only to the local UI for per-tablet presets.
    pub device_id: String,
    /// "Redmi Pad 2 · USB"
    pub name: String,
    pub status: String,
    /// Effective quality for this authenticated tablet; it is not the global default.
    pub profile: Profile,
    pub stats: Option<Stats>,
    /// The tablet advertised the explicit clipboard-text action.
    pub text_transfer: bool,
    pub keyboard: bool,
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
static TEXT_INBOX: Mutex<Option<ReceivedText>> = Mutex::new(None);
static RECORDING: Mutex<Option<ActiveRecording>> = Mutex::new(None);
static LAST_RECORDING_ERROR: Mutex<Option<String>> = Mutex::new(None);

#[derive(Clone, Serialize)]
pub struct RecordingInfo {
    pub active: bool,
    pub session_id: Option<u64>,
    pub status: String,
    pub path: Option<String>,
    pub next_segment: u32,
    pub queued_bytes: usize,
    pub error: Option<String>,
}

struct ActiveRecording {
    session_id: u64,
    base_path: PathBuf,
    next_segment: u32,
    destination: Option<crate::recording::Destination>,
    recorder: crate::recording::Recorder,
    writer: crate::recording::SegmentWriter,
    error: Option<String>,
}

impl ActiveRecording {
    fn new(session_id: u64, path: PathBuf) -> Result<Self, String> {
        let destination = crate::recording::Destination::prepare(&path)
            .map_err(|error| format!("Destino da gravação inválido: {error:?}"))?;
        Ok(Self {
            session_id,
            base_path: destination.final_path().to_path_buf(),
            next_segment: 0,
            destination: Some(destination),
            recorder: crate::recording::Recorder::new(crate::recording::Format::MatroskaH264Opus),
            writer: crate::recording::SegmentWriter::new(),
            error: None,
        })
    }

    fn fail(&mut self, error: impl Into<String>) {
        let message = error.into();
        self.error = Some(message.clone());
        self.recorder.cancel();
        *LAST_RECORDING_ERROR.lock().unwrap() = Some(message);
    }

    fn configure(&mut self, config: crate::recording::StreamConfig) {
        if self.error.is_some() {
            return;
        }
        let previous = self.recorder.config();
        let outcome = match self.recorder.set_config(config) {
            Ok(outcome) => outcome,
            Err(error) => {
                self.fail(format!("Configuração de gravação rejeitada: {error:?}"));
                return;
            }
        };
        if outcome != crate::recording::ConfigOutcome::SegmentRequired {
            return;
        }
        let Some(previous_config) = previous else {
            self.fail("A gravação recebeu uma rotação sem configuração anterior.");
            return;
        };
        let packets = self.recorder.drain_segment();
        if !packets.is_empty() {
            let Some(destination) = self.destination.take() else {
                self.fail("A gravação perdeu o destino do segmento anterior.");
                return;
            };
            if let Err(error) = self.writer.submit(destination, previous_config, packets) {
                self.fail(format!("A fila do writer falhou: {error:?}"));
                return;
            }
        }
        self.next_segment = self.next_segment.saturating_add(1);
        let path = crate::recording::Destination::rotated_path(&self.base_path, self.next_segment);
        self.destination = match crate::recording::Destination::prepare(path) {
            Ok(destination) => Some(destination),
            Err(error) => {
                self.fail(format!(
                    "Não foi possível abrir o próximo segmento: {error:?}"
                ));
                None
            }
        };
    }

    fn video(&mut self, pts_ms: u64, keyframe: bool, data: Vec<u8>) {
        if self.error.is_some() {
            return;
        }
        if let Err(error) = self.recorder.push_video(pts_ms, keyframe, data) {
            self.fail(format!("A gravação parou no vídeo: {error:?}"));
        }
    }

    fn audio(&mut self, pts_samples: u64, data: Vec<u8>) {
        if self.error.is_some() {
            return;
        }
        if let Err(error) = self.recorder.push_audio(pts_samples, data) {
            self.fail(format!("A gravação parou no áudio: {error:?}"));
        }
    }

    fn finish(mut self) -> Result<Vec<PathBuf>, String> {
        let previous_error = self.error.clone();
        if previous_error.is_none() {
            if let (Some(config), Some(destination)) =
                (self.recorder.config(), self.destination.take())
            {
                let packets = self.recorder.drain_segment();
                if !packets.is_empty() {
                    self.writer
                        .submit(destination, config, packets)
                        .map_err(|error| format!("A fila do writer falhou: {error:?}"))?;
                }
            }
        }
        let paths = self
            .writer
            .finish()
            .map_err(|error| format!("Falha ao finalizar a gravação: {error:?}"))?;
        if let Some(error) = previous_error {
            Err(error)
        } else {
            Ok(paths)
        }
    }
}

pub fn recording_status() -> RecordingInfo {
    let active = RECORDING.lock().unwrap();
    if let Some(recording) = active.as_ref() {
        return RecordingInfo {
            active: true,
            session_id: Some(recording.session_id),
            status: recording
                .error
                .clone()
                .unwrap_or_else(|| format!("{:?}", recording.recorder.status())),
            path: Some(recording.base_path.to_string_lossy().into_owned()),
            next_segment: recording.next_segment,
            queued_bytes: recording.recorder.queued_bytes(),
            error: recording.error.clone(),
        };
    }
    RecordingInfo {
        active: false,
        session_id: None,
        status: "Idle".into(),
        path: None,
        next_segment: 0,
        queued_bytes: 0,
        error: LAST_RECORDING_ERROR.lock().unwrap().clone(),
    }
}

pub fn start_recording(session_id: u64, path: String) -> Result<(), String> {
    if !sessions().iter().any(|session| session.id == session_id) {
        return Err("Sessão do tablet não encontrada.".into());
    }
    let mut active = RECORDING.lock().unwrap();
    if active.is_some() {
        return Err("Já existe uma gravação ativa.".into());
    }
    let recording = ActiveRecording::new(session_id, PathBuf::from(path))?;
    *LAST_RECORDING_ERROR.lock().unwrap() = None;
    *active = Some(recording);
    Ok(())
}

pub fn stop_recording() -> Result<Vec<String>, String> {
    let recording = RECORDING
        .lock()
        .unwrap()
        .take()
        .ok_or_else(|| "Nenhuma gravação ativa.".to_string())?;
    recording.finish().map(|paths| {
        paths
            .into_iter()
            .map(|path| path.to_string_lossy().into())
            .collect()
    })
}

fn finish_recording_for_session(session_id: u64) {
    let should_finish = RECORDING
        .lock()
        .unwrap()
        .as_ref()
        .is_some_and(|recording| recording.session_id == session_id);
    if !should_finish {
        return;
    }
    let recording = RECORDING.lock().unwrap().take();
    if let Some(recording) = recording {
        if let Err(error) = recording.finish() {
            *LAST_RECORDING_ERROR.lock().unwrap() = Some(error);
        }
    }
}

pub fn record_config(session_id: u64, config: crate::recording::StreamConfig) {
    if let Some(recording) = RECORDING
        .lock()
        .unwrap()
        .as_mut()
        .filter(|recording| recording.session_id == session_id)
    {
        recording.configure(config);
    }
}

pub fn record_video(session_id: u64, pts_ms: u64, keyframe: bool, data: &[u8]) {
    if let Some(recording) = RECORDING
        .lock()
        .unwrap()
        .as_mut()
        .filter(|recording| recording.session_id == session_id)
    {
        recording.video(pts_ms, keyframe, data.to_vec());
    }
}

pub fn record_audio(session_id: u64, pts_samples: u64, data: &[u8]) {
    if let Some(recording) = RECORDING
        .lock()
        .unwrap()
        .as_mut()
        .filter(|recording| recording.session_id == session_id)
    {
        recording.audio(pts_samples, data.to_vec());
    }
}

#[derive(Clone, Serialize)]
pub struct ReceivedText {
    pub session_id: u64,
    pub sender: String,
    pub text: String,
}

pub fn sessions() -> Vec<SessionInfo> {
    SESSIONS
        .lock()
        .unwrap()
        .iter()
        .map(|e| e.info.clone())
        .collect()
}

/// Takes the newest explicitly received text, if any. The inbox is process-memory only.
pub fn take_text() -> Option<ReceivedText> {
    TEXT_INBOX.lock().unwrap().take()
}

fn clear_text_for_session(session_id: u64) {
    let mut inbox = TEXT_INBOX.lock().unwrap();
    if inbox
        .as_ref()
        .is_some_and(|received| received.session_id == session_id)
    {
        *inbox = None;
    }
}

/// Sends one explicitly requested text payload to an authenticated tablet.
pub fn send_text(session_id: u64, text: &str) -> Result<(), String> {
    validate_text(text).map_err(str::to_string)?;
    let mut conn = {
        let all = SESSIONS.lock().unwrap();
        let entry = all
            .iter()
            .find(|entry| entry.info.id == session_id)
            .ok_or_else(|| "Sessão do tablet não encontrada.".to_string())?;
        if !entry.info.text_transfer {
            return Err("O tablet não oferece transferência de texto; atualize o app.".into());
        }
        entry.conn.try_clone().map_err(|e| e.to_string())?
    };
    send_json(
        &mut conn,
        TEXT,
        &json!({ "version": 1, "source": "pc", "text": text }),
    )
    .map_err(|e| e.to_string())
}

/// Revokes a tablet and closes any live session for that device.
pub fn disconnect_device(device_id: &str) {
    let stale = {
        let mut all = SESSIONS.lock().unwrap();
        all.iter()
            .position(|entry| entry.device_id == device_id)
            .map(|index| {
                let entry = all.remove(index);
                (entry.info.id, entry.conn)
            })
    };
    if let Some((session_id, conn)) = stale {
        clear_text_for_session(session_id);
        let _ = conn.shutdown(Shutdown::Both);
    }
}

/// Returns only session metrics. Names, addresses, tokens and device identifiers are intentionally absent.
pub fn metrics_report() -> Value {
    let metrics = sessions()
        .into_iter()
        .filter_map(|session| session.stats)
        .collect::<Vec<_>>();
    json!({
        "schema": "tabdisplay.metrics.v1",
        "app_version": env!("CARGO_PKG_VERSION"),
        "sessions": metrics,
    })
}

/// Adds a tablet to the live sessions. A tablet that reconnects replaces its own stale session.
fn register(
    device_id: &str,
    name: String,
    conn: Conn,
    text_transfer: bool,
    keyboard: bool,
) -> Result<Registered, String> {
    let (stale, id) = {
        let mut all = SESSIONS.lock().unwrap();
        let stale = all.iter().position(|e| e.device_id == device_id).map(|i| {
            let entry = all.remove(i);
            (entry.info.id, entry.conn)
        });
        if all.len() >= MAX_TABLETS {
            drop(all);
            if let Some((old_id, old)) = stale {
                clear_text_for_session(old_id);
                let _ = old.shutdown(Shutdown::Both);
            }
            return Err(format!("O TabDisplay já está com {MAX_TABLETS} tablets conectados. Desconecte um deles primeiro."));
        }
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let status = format!("Conectado: {name}");
        eprintln!("status: {status}");
        all.push(Entry {
            info: SessionInfo {
                id,
                device_id: device_id.into(),
                name,
                status,
                profile: settings::get().profile,
                stats: None,
                text_transfer,
                keyboard,
            },
            device_id: device_id.into(),
            conn,
        });
        (stale, id)
    };
    if let Some((old_id, old)) = stale {
        clear_text_for_session(old_id);
        let _ = old.shutdown(Shutdown::Both);
    }
    Ok(Registered(id))
}

/// Removes the session when the connection's thread is done, however it ends.
struct Registered(u64);

impl Drop for Registered {
    fn drop(&mut self) {
        SESSIONS.lock().unwrap().retain(|e| e.info.id != self.0);
        clear_text_for_session(self.0);
    }
}

fn with_session(id: u64, f: impl FnOnce(&mut SessionInfo)) {
    if let Some(e) = SESSIONS
        .lock()
        .unwrap()
        .iter_mut()
        .find(|e| e.info.id == id)
    {
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
    if !is_known_type(kind) || payload.len() > payload_limit(kind) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "message too large or unknown type",
        ));
    }
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
    if !is_known_type(hdr[0]) || len > MAX_MSG || len > payload_limit(hdr[0]) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "message too large",
        ));
    }
    let mut payload = vec![0; len];
    r.read_exact(&mut payload)?;
    Ok((hdr[0], payload))
}

fn read_msg_deadline(r: &mut Conn, timeout: Duration) -> io::Result<(u8, Vec<u8>)> {
    let deadline = Instant::now() + timeout;
    let mut hdr = [0u8; 5];
    read_exact_deadline(r, &mut hdr, deadline)?;
    let kind = hdr[0];
    let len = u32::from_be_bytes(hdr[1..5].try_into().unwrap()) as usize;
    if !is_known_type(kind) || len > MAX_MSG || len > payload_limit(kind) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "message too large or unknown type",
        ));
    }
    let mut payload = vec![0; len];
    read_exact_deadline(r, &mut payload, deadline)?;
    Ok((kind, payload))
}

fn read_exact_deadline(r: &mut Conn, buf: &mut [u8], deadline: Instant) -> io::Result<()> {
    let mut offset = 0;
    while offset < buf.len() {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "message deadline exceeded",
            ));
        }
        r.set_read_timeout(Some(remaining))?;
        match r.read(&mut buf[offset..]) {
            Ok(0) => return Err(io::ErrorKind::UnexpectedEof.into()),
            Ok(read) => offset += read,
            Err(e)
                if e.kind() == io::ErrorKind::WouldBlock || e.kind() == io::ErrorKind::TimedOut =>
            {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "message deadline exceeded",
                ));
            }
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

fn is_known_type(kind: u8) -> bool {
    matches!(kind, HELLO..=KEY)
}

fn payload_limit(kind: u8) -> usize {
    match kind {
        VIDEO => MAX_VIDEO,
        AUDIO => MAX_AUDIO,
        INPUT => MAX_INPUT,
        _ => MAX_CONTROL,
    }
}

fn validate_text(text: &str) -> Result<(), &'static str> {
    if text.as_bytes().len() > MAX_TEXT {
        Err("O texto é grande demais (máximo de 16 KiB).")
    } else {
        Ok(())
    }
}

fn parse_text(payload: &[u8], source: &str) -> Option<String> {
    let value: Value = serde_json::from_slice(payload).ok()?;
    if value["version"].as_u64()? != 1 || value["source"].as_str()? != source {
        return None;
    }
    let text = value["text"].as_str()?.to_owned();
    validate_text(&text).ok().map(|_| text)
}

fn valid_dimensions((width, height): (u32, u32)) -> bool {
    width >= MIN_DIMENSION
        && height >= MIN_DIMENSION
        && width <= MAX_DIMENSION
        && height <= MAX_DIMENSION
        && width % 16 == 0
        && height % 16 == 0
        && (width as u64) * (height as u64) <= MAX_PIXELS
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
    if let Ok(o) = std::process::Command::new("scutil")
        .args(["--get", "ComputerName"])
        .output()
    {
        let name = String::from_utf8_lossy(&o.stdout).trim().to_string();
        if !name.is_empty() {
            return name;
        }
    }
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "PC".into())
}

/// Announces this PC once a second on every network the PC has (Wi‑Fi, Ethernet, and a tablet
/// tethered by USB) so tablets can list it without typing an IP. A plain broadcast to
/// 255.255.255.255 only leaves through whichever interface owns the default route, which usually
/// isn't the USB one (tethering rarely offers internet access), so each interface's own directed
/// broadcast address is used instead: the OS routes it out that interface because the address is
/// only reachable there. Payload: `TABDISPLAY {"id":…,"name":…}`; the tablet takes the address
/// from the packet's source.
pub fn beacon() {
    let msg = format!(
        "TABDISPLAY {}",
        json!({ "id": pairing::pc_id(), "name": computer_name() })
    );
    loop {
        for iface in if_addrs::get_if_addrs().unwrap_or_default() {
            let if_addrs::IfAddr::V4(v4) = iface.addr else {
                continue;
            };
            let Some(broadcast) = v4.broadcast else {
                continue;
            };
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
        Err(e) => {
            crate::telemetry::warn(format!("could not open port {PORT}: {e}"));
            return set_status(format!("Erro ao abrir porta {PORT}: {e}"));
        }
    };
    set_status("Aguardando tablet");
    for stream in listener.incoming().flatten() {
        if HANDSHAKES
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                (n < MAX_HANDSHAKES).then_some(n + 1)
            })
            .is_err()
        {
            let _ = stream.shutdown(Shutdown::Both);
            continue;
        }
        thread::spawn(move || {
            let _guard = HandshakeGuard;
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

struct HandshakeGuard;

impl Drop for HandshakeGuard {
    fn drop(&mut self) {
        HANDSHAKES.fetch_sub(1, Ordering::AcqRel);
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
    screen: (u32, u32),
    dpi: u32,
    #[serde(default)]
    video_modes: Vec<VideoMode>,
    /// v3 audio packets may carry a sequence/PTS header when this capability is true.
    #[serde(default)]
    audio_timestamps: bool,
    /// Explicit one-shot clipboard text action; absent on older tablet clients.
    #[serde(default)]
    text_transfer: bool,
    /// Explicit tablet keyboard/IME capability; absent on older clients.
    #[serde(default)]
    keyboard: bool,
}

#[derive(Clone, Copy, Deserialize)]
struct VideoMode {
    width: u32,
    height: u32,
    fps: u32,
}

#[derive(Clone)]
struct TabletCaps {
    decodable: (u32, u32),
    modes: Vec<VideoMode>,
}

impl TabletCaps {
    fn from_hello(hello: &Hello) -> Self {
        let modes = if hello.video_modes.is_empty() {
            vec![VideoMode {
                width: hello.decodable.0,
                height: hello.decodable.1,
                fps: 60,
            }]
        } else {
            hello.video_modes.clone()
        };
        Self {
            decodable: hello.decodable,
            modes,
        }
    }

    fn effective(&self, requested: (u32, u32), fps: u32) -> ((u32, u32), u32) {
        let eligible = self
            .modes
            .iter()
            .copied()
            .filter(|mode| mode.fps <= fps)
            .collect::<Vec<_>>();
        let fitting = eligible.iter().copied().filter(|mode| {
            (mode.width >= requested.0 && mode.height >= requested.1)
                || (mode.width >= requested.1 && mode.height >= requested.0)
        });
        let mode = fitting
            .max_by_key(|mode| (mode.fps, mode.width as u64 * mode.height as u64))
            .or_else(|| {
                eligible
                    .into_iter()
                    .max_by_key(|mode| (mode.fps, mode.width as u64 * mode.height as u64))
            })
            .unwrap_or(VideoMode {
                width: self.decodable.0,
                height: self.decodable.1,
                fps: 60,
            });
        (fit(requested, (mode.width, mode.height)), mode.fps)
    }
}

fn validate_hello(hello: &Hello) -> Result<(), &'static str> {
    if hello.device_id.is_empty() || hello.device_id.len() > 128 {
        return Err("HELLO inválido: identificador do tablet ausente ou grande demais.");
    }
    if hello.device_name.is_empty() || hello.device_name.len() > 128 {
        return Err("HELLO inválido: nome do tablet ausente ou grande demais.");
    }
    if hello.token.len() > 512 {
        return Err("HELLO inválido: token grande demais.");
    }
    if !valid_dimensions(hello.screen) || !valid_dimensions(hello.decodable) {
        return Err("HELLO inválido: dimensões de tela não suportadas.");
    }
    if !(72..=1000).contains(&hello.dpi) {
        return Err("HELLO inválido: DPI fora do limite.");
    }
    if hello.video_modes.len() > 32
        || hello.video_modes.iter().any(|mode| {
            !valid_dimensions((mode.width, mode.height)) || !(1..=MAX_FPS).contains(&mode.fps)
        })
    {
        return Err("HELLO inválido: capacidades de vídeo fora do limite.");
    }
    Ok(())
}

use crate::sys::input::Rect;
/// Where tablet input goes and whether touch acts as a mouse; None = input off.
type Target = Option<(Rect, bool)>;

/// Live numbers for the UI (None when no tablet is connected).
#[derive(Clone, Default, Serialize)]
pub struct Stats {
    /// Stable protocol identifier for this CONFIG generation; zero is the legacy/unknown value.
    pub config_id: u64,
    pub width: usize,
    pub height: usize,
    pub target_fps: u32,
    /// Frames sent per second.
    pub fps: f32,
    /// Sent frames backed by a new capture, versus FLUSH repetitions.
    pub unique_fps: f32,
    pub repeated_fps: f32,
    pub mbps: f32,
    pub encode_ms: f32,
    pub encode_p50_ms: f32,
    pub encode_p95_ms: f32,
    pub encode_p99_ms: f32,
    pub capture_ms: f32,
    pub send_ms: f32,
    /// "GPU" or "CPU": which encoder produced the current stream.
    pub encoder_kind: &'static str,
    /// The codec negotiated by this protocol generation.
    pub codec: &'static str,
    /// USB or Wi-Fi, without an address or device identifier.
    pub transport: &'static str,
    /// Round trip PC -> tablet -> PC.
    pub rtt_ms: u32,
    /// Frames submitted to MediaCodec and frames observed by the output callback.
    pub decode_fps: u32,
    pub tablet_fps: u32,
    pub decode_drops: u32,
    pub render_failures: u32,
    /// No new capture arrived in this window. Repeated FLUSH frames do not make it busy.
    pub idle: bool,
    /// Number of samples discarded after the bounded per-window sample cap.
    pub samples_lost: u32,
    /// The Auto profile is running below Balanced because the network is struggling.
    pub reduced: bool,
}

/// Bounded local timing samples. It deliberately keeps no frame log or payload.
#[derive(Default)]
struct MetricWindow {
    capture_ms: Vec<f32>,
    encode_ms: Vec<f32>,
    send_ms: Vec<f32>,
    lost: u32,
}

impl MetricWindow {
    const CAP: usize = 128;

    fn add(slot: &mut Vec<f32>, value: f32, lost: &mut u32) {
        if slot.len() < Self::CAP {
            slot.push(value);
        } else {
            // Deterministic bounded sampling keeps the cost predictable and avoids retaining a log.
            let index = (*lost as usize) % Self::CAP;
            slot[index] = value;
            *lost = lost.saturating_add(1);
        }
    }

    fn percentile(values: &[f32], percentile: f32) -> f32 {
        if values.is_empty() {
            return 0.0;
        }
        let mut sorted = values.to_vec();
        sorted.sort_unstable_by(f32::total_cmp);
        let index = ((sorted.len() - 1) as f32 * percentile).round() as usize;
        sorted[index.min(sorted.len() - 1)]
    }

    fn avg(values: &[f32]) -> f32 {
        if values.is_empty() {
            0.0
        } else {
            values.iter().sum::<f32>() / values.len() as f32
        }
    }
}

/// What a session's reader thread (tablet -> PC) shares with its streaming loop.
struct Shared {
    /// Key of this tablet's entry in `SESSIONS`.
    id: u64,
    start: Instant,
    alive: AtomicBool,
    target: Mutex<Target>,
    /// The tablet's decodable size; changes when it rotates (RESIZE).
    caps: Mutex<TabletCaps>,
    /// Set by RESIZE: rebuild the pipeline for the new size.
    rebuild: AtomicBool,
    paused: AtomicBool,
    rtt_ms: AtomicU32,
    decode_fps: AtomicU32,
    tablet_fps: AtomicU32,
    decode_drops: AtomicU32,
    render_failures: AtomicU32,
    tablet_config_id: AtomicU64,
    next_config_id: AtomicU64,
    transport: &'static str,
    /// Monotonic milliseconds since session start of the last valid PONG.
    last_pong_ms: AtomicU64,
    /// State of the Auto quality profile; outlives rebuilds so a step change sticks.
    adapt: Mutex<Adapt>,
    /// Tablet-local quality choice. PROFILE from this authenticated session never edits global Settings.
    profile: Mutex<Profile>,
    /// New clients opt into the versioned AUDIO payload; legacy clients keep raw Opus packets.
    audio_timestamps: bool,
    /// Whether this authenticated tablet supports explicit one-shot text transfer.
    text_transfer: bool,
    /// Whether this authenticated tablet supports the bounded logical keyboard channel.
    keyboard: bool,
}

/// Auto quality: the presets from best (0) to lightest. Steps down when the link struggles for a few seconds,
/// back up after a long calm stretch.
const LADDER: [Profile; 3] = [Profile::Quality, Profile::Balanced, Profile::Performance];
const ADAPT_START: usize = 1;
/// The loop owns at most one raw frame and one encoded frame at a time.
/// These explicit budgets prevent a slow socket from becoming an unbounded producer.
const RAW_FRAME_SLOTS: usize = 2;
const MAX_ENCODED_IN_FLIGHT: usize = MAX_VIDEO;
const MAX_FRAME_AGE: Duration = Duration::from_millis(250);
const AUDIO_PACKETS_PER_TURN: usize = 2;
const AUDIO_BYTES_PER_TURN: usize = MAX_AUDIO;
const BOTTLENECK_MS: f32 = 40.0;

struct Adapt {
    level: usize,
    bad: u32,
    good: u32,
    /// Seconds left during which stepping back up is not allowed (after a step down).
    hold: u32,
    reason: &'static str,
    last_decode_drops: u32,
    last_render_failures: u32,
}

impl Adapt {
    fn new() -> Self {
        Self {
            level: ADAPT_START,
            bad: 0,
            good: 0,
            hold: 0,
            reason: "nenhum",
            last_decode_drops: 0,
            last_render_failures: 0,
        }
    }

    /// Feeds one second of stats; true if the level changed. `sent_fps` is what the PC sent, `shown_fps` what
    /// the tablet displayed (a static desktop sends few frames, so only compare them when the PC is busy).
    fn tick(
        &mut self,
        rtt_ms: u32,
        sent_fps: f32,
        shown_fps: u32,
        encode_ms: f32,
        capture_ms: f32,
        send_ms: f32,
        decode_drops: u32,
        render_failures: u32,
        idle: bool,
    ) -> bool {
        let new_tablet_loss =
            decode_drops > self.last_decode_drops || render_failures > self.last_render_failures;
        self.last_decode_drops = decode_drops;
        self.last_render_failures = render_failures;
        self.reason = if idle {
            "tela"
        } else if rtt_ms > 150 || send_ms > BOTTLENECK_MS {
            "rede"
        } else if encode_ms > BOTTLENECK_MS {
            "encoder"
        } else if capture_ms > BOTTLENECK_MS {
            "captura"
        } else if new_tablet_loss || (sent_fps >= 10.0 && (shown_fps as f32) < sent_fps * 0.6) {
            "tablet"
        } else {
            "nenhum"
        };
        let struggling = !idle && self.reason != "nenhum";
        (self.bad, self.good) = if struggling {
            (self.bad + 1, 0)
        } else {
            (0, self.good + 1)
        };
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
    let (kind, payload) = read_msg_deadline(&mut stream, HELLO_DEADLINE)?;
    stream.set_read_timeout(None)?;
    if kind != HELLO {
        return Err(refuse(
            &mut stream,
            "A primeira mensagem precisa ser HELLO; atualize o app do tablet.",
        ));
    }
    let hello = serde_json::from_slice::<Hello>(&payload)
        .map_err(|_| refuse(&mut stream, "HELLO inválido; atualize o app do tablet."))?;
    if hello.v != PROTOCOL {
        return Err(refuse(
            &mut stream,
            "Versões diferentes do TabDisplay no PC e no tablet: atualize os dois.",
        ));
    }
    validate_hello(&hello).map_err(|message| refuse(&mut stream, message))?;
    // Loopback is only an endpoint. ADB reverse/wireless debugging is not cryptographic proof of a
    // particular cable or tablet, so it follows the same token/pairing path as Wi-Fi.
    let usb = stream.peer_addr()?.ip().is_loopback();
    if !pairing::is_paired(&hello.device_id, &hello.token) {
        let Ok(_pairing) = PAIRING.try_lock() else {
            return Err(refuse(
                &mut stream,
                "Outro tablet está sendo pareado agora. Tente de novo em instantes.",
            ));
        };
        pair(&mut stream, &hello).inspect_err(|_| pairing::cancel())?;
    }
    let session = format!(
        "{} · {}",
        hello.device_name,
        if usb { "USB" } else { "Wi‑Fi" }
    );
    let registered = match register(
        &hello.device_id,
        session,
        stream.try_clone()?,
        hello.text_transfer,
        hello.keyboard,
    ) {
        Ok(r) => r,
        Err(message) => return Err(refuse(&mut stream, &message)),
    };
    let id = registered.0;
    crate::telemetry::crumb(
        "session",
        if usb {
            "connected over usb"
        } else {
            "connected over wifi"
        },
    );

    let shared = Arc::new(Shared {
        id,
        start: Instant::now(),
        alive: AtomicBool::new(true),
        target: Mutex::new(None),
        caps: Mutex::new(TabletCaps::from_hello(&hello)),
        rebuild: AtomicBool::new(false),
        paused: AtomicBool::new(false),
        rtt_ms: AtomicU32::new(0),
        decode_fps: AtomicU32::new(0),
        tablet_fps: AtomicU32::new(0),
        decode_drops: AtomicU32::new(0),
        render_failures: AtomicU32::new(0),
        tablet_config_id: AtomicU64::new(0),
        next_config_id: AtomicU64::new(1),
        transport: if usb { "USB" } else { "Wi-Fi" },
        last_pong_ms: AtomicU64::new(0),
        adapt: Mutex::new(Adapt::new()),
        profile: Mutex::new(settings::get().profile),
        audio_timestamps: hello.audio_timestamps,
        text_transfer: hello.text_transfer,
        keyboard: hello.keyboard,
    });
    // System audio: captured and encoded on its own thread; `stream_once` is the only socket writer.
    let (alive, audio_on) = (shared.clone(), || settings::get().audio);
    let audio = crate::audio::spawn(move || alive.alive.load(Ordering::Relaxed), audio_on);
    let (mut rd, reader) = (stream.try_clone()?, shared.clone());
    thread::spawn(move || {
        let mut injector = Injector::default();
        while let Ok((kind, p)) = read_msg(&mut rd) {
            match kind {
                INPUT => {
                    let target = *reader.target.lock().unwrap();
                    if let (Some((rect, as_mouse)), Some(frame)) = (target, crate::input::parse(&p))
                    {
                        injector.inject(&frame, rect, as_mouse);
                    }
                }
                PROFILE => {
                    // The tablet picked a quality preset: rebuild this session only.
                    if let Some(profile) = preset(&p) {
                        let mut current = reader.profile.lock().unwrap();
                        if *current != profile {
                            *current = profile;
                            with_session(reader.id, |info| info.profile = profile);
                            reader.rebuild.store(true, Ordering::Relaxed);
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
                    if let Some(size) = parse_dimensions(&p, "decodable") {
                        reader.caps.lock().unwrap().decodable = size;
                        reader.rebuild.store(true, Ordering::Relaxed);
                    }
                }
                PONG => {
                    if let Some(t) = serde_json::from_slice::<Value>(&p)
                        .ok()
                        .and_then(|v| v["t"].as_u64())
                    {
                        let now = reader.start.elapsed().as_millis() as u64;
                        if t <= now {
                            reader
                                .rtt_ms
                                .store((now - t).min(u32::MAX as u64) as u32, Ordering::Relaxed);
                            reader.last_pong_ms.store(now, Ordering::Relaxed);
                        }
                    }
                }
                STATS_MSG => {
                    if let Ok(v) = serde_json::from_slice::<Value>(&p) {
                        if let Some(fps) = v["fps"].as_u64().filter(|fps| *fps <= MAX_FPS as u64) {
                            reader.tablet_fps.store(fps as u32, Ordering::Relaxed);
                        }
                        if let Some(fps) = v["decode_fps"]
                            .as_u64()
                            .filter(|fps| *fps <= MAX_FPS as u64)
                        {
                            reader.decode_fps.store(fps as u32, Ordering::Relaxed);
                        }
                        if let Some(drops) = v["decode_drops"]
                            .as_u64()
                            .filter(|drops| *drops <= u32::MAX as u64)
                        {
                            reader.decode_drops.store(drops as u32, Ordering::Relaxed);
                        }
                        if let Some(failures) = v["render_failures"]
                            .as_u64()
                            .filter(|failures| *failures <= u32::MAX as u64)
                        {
                            reader
                                .render_failures
                                .store(failures as u32, Ordering::Relaxed);
                        }
                        if let Some(id) = v["config_id"].as_u64() {
                            reader.tablet_config_id.store(id, Ordering::Relaxed);
                        }
                    }
                }
                KEYFRAME => {
                    reader.rebuild.store(true, Ordering::Relaxed);
                }
                PAUSE => {
                    reader.paused.store(true, Ordering::Relaxed);
                }
                RESUME => {
                    reader.paused.store(false, Ordering::Relaxed);
                    reader.rebuild.store(true, Ordering::Relaxed);
                }
                TEXT => {
                    if reader.text_transfer {
                        if let Some(text) = parse_text(&p, "tablet") {
                            let sender = sessions()
                                .into_iter()
                                .find(|session| session.id == reader.id)
                                .map(|session| session.name)
                                .unwrap_or_else(|| "Tablet".into());
                            *TEXT_INBOX.lock().unwrap() = Some(ReceivedText {
                                session_id: reader.id,
                                sender,
                                text,
                            });
                        }
                    }
                }
                KEY => {
                    if reader.keyboard && settings::get().touch {
                        if let Some(event) = crate::input::parse_key(&p) {
                            injector.key(&event);
                        }
                    }
                }
                _ => {
                    reader.alive.store(false, Ordering::Relaxed);
                    break;
                }
            }
        }
        reader.alive.store(false, Ordering::Relaxed);
    });

    // The virtual monitor lives for the whole session; rebuilds only change its mode.
    let mut virtual_display = None;
    let mut result = Ok(());
    while shared.alive.load(Ordering::Relaxed) && result.is_ok() {
        if shared.paused.load(Ordering::Relaxed) {
            audio.clear();
            thread::sleep(Duration::from_millis(100));
            continue;
        }
        result = stream_once(&mut stream, &shared, &mut virtual_display, &audio);
    }
    drop(virtual_display);
    finish_recording_for_session(shared.id);
    drop(registered);
    let _ = stream.shutdown(Shutdown::Both);
    result
}

fn is_keyframe(data: &[u8]) -> bool {
    let mut index = 0;
    while index + 3 <= data.len() {
        let prefix = if index + 4 <= data.len() && data[index..index + 4] == [0, 0, 0, 1] {
            4
        } else if data[index..index + 3] == [0, 0, 1] {
            3
        } else {
            index += 1;
            continue;
        };
        if index + prefix < data.len() && data[index + prefix] & 0x1f == 5 {
            return true;
        }
        index += prefix;
    }
    false
}

/// The preset in a PROFILE message from the tablet; Custom is only settable from the PC.
fn preset(payload: &[u8]) -> Option<Profile> {
    let v: Value = serde_json::from_slice(payload).ok()?;
    match serde_json::from_value(v["profile"].clone()).ok()? {
        Profile::Custom => None,
        p => Some(p),
    }
}

fn parse_dimensions(payload: &[u8], key: &str) -> Option<(u32, u32)> {
    let value: Value = serde_json::from_slice(payload).ok()?;
    let values = value.get(key)?.as_array()?;
    if values.len() != 2 {
        return None;
    }
    let width = values[0].as_u64()?.try_into().ok()?;
    let height = values[1].as_u64()?.try_into().ok()?;
    valid_dimensions((width, height)).then_some((width, height))
}

/// Size, fps and Mbps for the chosen quality profile.
fn plan(s: &Settings, caps: &TabletCaps, auto_level: usize) -> ((u32, u32), u32, u32) {
    let tablet = caps.decodable;
    let (requested, fps, bitrate) = match s.profile {
        Profile::Auto => {
            return plan(
                &Settings {
                    profile: LADDER[auto_level],
                    ..s.clone()
                },
                caps,
                auto_level,
            )
        }
        Profile::Performance => ((tablet.0 / 2 & !15, tablet.1 / 2 & !15), 60, 10),
        Profile::Balanced => (tablet, 60, 20),
        Profile::Quality => (tablet, 60, 40),
        Profile::Custom => (
            fit(s.resolution.unwrap_or(tablet), tablet),
            s.fps,
            s.bitrate_mbps,
        ),
    };
    let (effective, effective_fps) = caps.effective(requested, fps);
    (effective, effective_fps, bitrate)
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
    loop {
        let (kind, p) = read_msg_deadline(stream, PAIR_DEADLINE)?;
        if kind != PAIR {
            continue;
        }
        let code = serde_json::from_slice::<Value>(&p)
            .ok()
            .and_then(|v| v["code"].as_str().map(String::from))
            .unwrap_or_default();
        match pairing::check(&code) {
            Check::Ok => {
                let token = pairing::complete(&hello.device_id, &hello.device_name)
                    .map_err(io::Error::other)?;
                send_json(
                    stream,
                    PAIRED,
                    &json!({ "pc_id": pairing::pc_id(), "token": token }),
                )?;
                return stream.set_read_timeout(None);
            }
            Check::Wrong => send_json(stream, PAIR_REQUIRED, &ask(true))?,
            Check::Over => {
                return Err(refuse(
                    stream,
                    "O código expirou ou teve tentativas demais. Conecte de novo para gerar outro.",
                ))
            }
        }
    }
}

/// Streams with the current settings until they change, the tablet rotates, capture is lost, or the
/// tablet leaves.
fn take_audio_turn(audio: &crate::audio::Queue, timestamps: bool) -> Vec<crate::audio::Packet> {
    const FRAME_OVERHEAD: usize = 5; // type + length in protocol v3
    const AUDIO_V1_OVERHEAD: usize = 17; // version + sequence + PTS
    let overhead = FRAME_OVERHEAD + if timestamps { AUDIO_V1_OVERHEAD } else { 0 };
    let mut packets = Vec::with_capacity(AUDIO_PACKETS_PER_TURN);
    let mut bytes = 0usize;
    while packets.len() < AUDIO_PACKETS_PER_TURN {
        let used_with_next_overhead = bytes.saturating_add(overhead * (packets.len() + 1));
        let Some(packet) = audio.try_pop_with_budget(used_with_next_overhead, AUDIO_BYTES_PER_TURN)
        else {
            break;
        };
        bytes = bytes.saturating_add(packet.opus.len() + overhead);
        packets.push(packet);
    }
    packets
}

fn stream_once(
    stream: &mut Conn,
    shared: &Shared,
    virtual_display: &mut Option<display::VirtualDisplay>,
    audio: &crate::audio::Queue,
) -> io::Result<()> {
    let version = settings::VERSION.load(Ordering::Relaxed);
    let mut s = settings::get();
    s.profile = *shared.profile.lock().unwrap();
    let caps = shared.caps.lock().unwrap().clone();
    let level = shared.adapt.lock().unwrap().level;
    let ((w, h), fps, mbps) = plan(&s, &caps, level);

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
                    return Err(io::Error::other(format!(
                        "monitor virtual indisponível; sessão pausada: {e}"
                    )));
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
    // Capture is constrained by the effective video output, not only by the tablet maximum.
    // The source monitor and input rect remain physical; Capture performs aspect-preserving scale.
    let mut cap = open_capture(device.as_deref(), fps, (w, h))?;
    *shared.target.lock().unwrap() = s
        .touch
        .then_some((cap.rect, s.touch_mode == TouchMode::Mouse));
    let (cw, ch) = (cap.width, cap.height);
    let (mut encode, kind) = encoder(cw, ch, fps, mbps, s.encoder)?;
    label += &format!(" {cw}x{ch} · {fps} fps · {mbps} Mbps · {kind}");
    if let Some((mw, mh, mhz, _)) = virtual_display
        .as_ref()
        .and_then(display::VirtualDisplay::effective_mode)
    {
        label += &format!(" · monitor {mw}x{mh}@{mhz}Hz");
    }
    set_session_status(shared.id, label.clone());

    let config_id = shared.next_config_id.fetch_add(1, Ordering::Relaxed);
    send_json(
        stream,
        CONFIG,
        &json!({ "width": cw, "height": ch, "fps": fps, "config_id": config_id }),
    )?;
    // Tell the tablet which preset is active (a rebuild after either side changed it lands here).
    send_json(stream, PROFILE, &json!({ "profile": s.profile }))?;
    record_config(
        shared.id,
        crate::recording::StreamConfig {
            generation: config_id,
            width: cw as u32,
            height: ch as u32,
            fps,
            audio: s.audio,
        },
    );

    // Frame pacing: capture as fast as the desktop updates, send at most `fps`, and never
    // drop the last update of a burst (it's sent once the interval has passed).
    let interval = Duration::from_secs(1) / fps.max(1);
    let (mut frame, mut nal) = (Vec::new(), Vec::new());
    debug_assert!(RAW_FRAME_SLOTS >= 1); // serial capture owns one of the two allowed raw-frame slots
    let mut next_deadline = Instant::now();
    let mut pending = false;
    // Hardware decoders (the tablet's MediaTek one) hold a few frames before showing them, so a lone
    // update (cursor move, a typed letter) would sit in the decoder until the screen changes again.
    // Repeating the last frame for a moment pushes it out; identical frames encode to a few bytes.
    let mut last_change = Instant::now() - MAX_FRAME_AGE;
    // Once a second: ping for the round trip, and publish stats for the UI.
    let mut tick = Instant::now();
    let (mut frames, mut unique_frames, mut repeated_frames, mut bytes) =
        (0u32, 0u32, 0u32, 0usize);
    let (mut captured_updates, mut unique_pending) = (0u32, false);
    let mut metrics = MetricWindow::default();
    while shared.alive.load(Ordering::Relaxed)
        && !shared.paused.load(Ordering::Relaxed)
        && settings::VERSION.load(Ordering::Relaxed) == version
        && !shared.rebuild.swap(false, Ordering::Relaxed)
    {
        if tick.elapsed() >= Duration::from_secs(1) {
            let now_ms = shared.start.elapsed().as_millis() as u64;
            let last_pong = shared.last_pong_ms.load(Ordering::Relaxed);
            if now_ms > HEARTBEAT_TIMEOUT.as_millis() as u64
                && now_ms.saturating_sub(last_pong) > HEARTBEAT_TIMEOUT.as_millis() as u64
            {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "tablet heartbeat timed out",
                ));
            }
            let secs = tick.elapsed().as_secs_f32();
            let stats = Stats {
                config_id,
                width: cw,
                height: ch,
                target_fps: fps,
                fps: frames as f32 / secs,
                unique_fps: unique_frames as f32 / secs,
                repeated_fps: repeated_frames as f32 / secs,
                mbps: bytes as f32 * 8.0 / secs / 1e6,
                encode_ms: MetricWindow::avg(&metrics.encode_ms),
                encode_p50_ms: MetricWindow::percentile(&metrics.encode_ms, 0.50),
                encode_p95_ms: MetricWindow::percentile(&metrics.encode_ms, 0.95),
                encode_p99_ms: MetricWindow::percentile(&metrics.encode_ms, 0.99),
                capture_ms: MetricWindow::avg(&metrics.capture_ms),
                send_ms: MetricWindow::avg(&metrics.send_ms),
                encoder_kind: kind,
                codec: "H.264/AVC",
                transport: shared.transport,
                rtt_ms: shared.rtt_ms.load(Ordering::Relaxed),
                decode_fps: shared.decode_fps.load(Ordering::Relaxed),
                tablet_fps: shared.tablet_fps.load(Ordering::Relaxed),
                decode_drops: shared.decode_drops.load(Ordering::Relaxed),
                render_failures: shared.render_failures.load(Ordering::Relaxed),
                idle: captured_updates == 0 && unique_frames == 0,
                samples_lost: metrics.lost,
                reduced: s.profile == Profile::Auto && level > ADAPT_START,
            };
            if s.profile == Profile::Auto {
                let (changed, reason) = {
                    let mut adapt = shared.adapt.lock().unwrap();
                    let changed = adapt.tick(
                        stats.rtt_ms,
                        stats.fps,
                        stats.tablet_fps,
                        stats.encode_ms,
                        stats.capture_ms,
                        stats.send_ms,
                        stats.decode_drops,
                        stats.render_failures,
                        stats.idle,
                    );
                    (changed, adapt.reason)
                };
                if changed {
                    set_session_status(shared.id, format!("{label} · motivo: {reason}"));
                    shared.rebuild.store(true, Ordering::Relaxed);
                }
            }
            // The tablet echoes `t` (round trip) and may show the rest in its stats overlay.
            let ping = json!({ "t": shared.start.elapsed().as_millis() as u64, "rtt_ms": stats.rtt_ms, "fps": stats.fps.round(), "mbps": stats.mbps });
            with_session(shared.id, |info| info.stats = Some(stats.clone()));
            send_json(stream, PING, &ping)?;
            (
                tick,
                frames,
                unique_frames,
                repeated_frames,
                bytes,
                captured_updates,
                unique_pending,
            ) = (Instant::now(), 0, 0, 0, 0, 0, false);
            metrics = MetricWindow::default();
        }
        if !settings::get().audio {
            audio.clear();
        }
        for packet in take_audio_turn(audio, shared.audio_timestamps) {
            record_audio(shared.id, packet.pts_samples, &packet.opus);
            if shared.audio_timestamps {
                let mut payload = Vec::with_capacity(17 + packet.opus.len());
                payload.push(1); // AUDIO payload v1: sequence:u64, pts_samples:u64, Opus bytes.
                payload.extend_from_slice(&packet.sequence.to_be_bytes());
                payload.extend_from_slice(&packet.pts_samples.to_be_bytes());
                payload.extend_from_slice(&packet.opus);
                write_msg(stream, AUDIO, &payload)?;
            } else {
                write_msg(stream, AUDIO, &packet.opus)?;
            }
        }
        let now = Instant::now();
        let wait = if pending {
            next_deadline
                .saturating_duration_since(now)
                .min(Duration::from_millis(50))
                .as_millis()
                .max(1) as u32
        } else {
            interval.min(Duration::from_millis(50)).as_millis().max(1) as u32
        };
        let capture_started = Instant::now();
        match cap.next(&mut frame, wait) {
            Ok(true) => {
                MetricWindow::add(
                    &mut metrics.capture_ms,
                    capture_started.elapsed().as_secs_f32() * 1000.0,
                    &mut metrics.lost,
                );
                captured_updates = captured_updates.saturating_add(1);
                unique_pending = true;
                (pending, last_change) = (true, Instant::now());
            }
            Ok(false) => pending |= last_change.elapsed() < MAX_FRAME_AGE,
            Err(e) => {
                // e.g. ACCESS_LOST on a mode change, UAC prompt or lock screen: rebuild.
                crate::telemetry::crumb("capture", &format!("capture lost: {e}"));
                thread::sleep(Duration::from_millis(300));
                break;
            }
        }
        if pending && Instant::now() >= next_deadline {
            let t = Instant::now();
            encode(&frame, &mut nal)?;
            if nal.len() > MAX_ENCODED_IN_FLIGHT {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "quadro codificado excede o orçamento da sessão",
                ));
            }
            let encode_ms = t.elapsed().as_secs_f32() * 1000.0;
            if !nal.is_empty() {
                record_video(
                    shared.id,
                    shared.start.elapsed().as_millis() as u64,
                    is_keyframe(&nal),
                    &nal,
                );
                let send_started = Instant::now();
                write_msg(stream, VIDEO, &nal)?;
                let send_ms = send_started.elapsed().as_secs_f32() * 1000.0;
                MetricWindow::add(&mut metrics.encode_ms, encode_ms, &mut metrics.lost);
                MetricWindow::add(&mut metrics.send_ms, send_ms, &mut metrics.lost);
                (frames, bytes) = (frames + 1, bytes + nal.len());
                if unique_pending {
                    unique_frames = unique_frames.saturating_add(1);
                } else {
                    repeated_frames = repeated_frames.saturating_add(1);
                }
                unique_pending = false;
            }
            let completed = Instant::now();
            next_deadline = next_deadline_after(next_deadline, completed, interval);
            pending = false;
        }
    }
    *shared.target.lock().unwrap() = None;
    Ok(())
}

/// Shrinks `size` (keeping its aspect ratio) until the tablet can decode it. The tablet reports its
/// largest decodable size in HELLO; either orientation of that fits.
fn fit((w, h): (u32, u32), (tw, th): (u32, u32)) -> (u32, u32) {
    let (long, short) = (tw.max(th) as f64, tw.min(th) as f64);
    let scale = (long / w.max(h) as f64)
        .min(short / w.min(h) as f64)
        .min(1.0);
    let round = |v: u32| (v as f64 * scale) as u32 & !15;
    // H.264/MediaCodec requires dimensions aligned to 16 even when the native
    // monitor already fits the tablet; e.g. 1920x1080 must become 1920x1072.
    if scale >= 1.0 {
        (w & !15, h & !15)
    } else {
        (round(w), round(h))
    }
}

/// Advances from the prior deadline and skips overdue slots; encoding/writing cost is never added to the period.
fn next_deadline_after(previous: Instant, completed: Instant, interval: Duration) -> Instant {
    let mut next = previous + interval;
    while next <= completed {
        next += interval;
    }
    next
}

/// A just-attached monitor takes a moment to show up in DXGI. An explicitly
/// selected monitor is retried, then the session fails; it is never replaced
/// silently by the primary monitor.
fn open_capture(device: Option<&str>, fps: u32, output: (u32, u32)) -> io::Result<Capture> {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        match Capture::open(device, fps, |size| fit(size, output)) {
            Ok(c) => return Ok(c),
            Err(_) if Instant::now() < deadline => thread::sleep(Duration::from_millis(100)),
            Err(e) if device.is_some() => {
                return Err(io::Error::other(format!(
                    "monitor selecionado indisponível ({device:?}): {e}"
                )))
            }
            Err(e) => {
                return Err(io::Error::other(format!(
                    "monitor principal indisponível: {e}"
                )))
            }
        }
    }
}

type EncodeFn = Box<dyn FnMut(&[u8], &mut Vec<u8>) -> io::Result<()>>;

/// Per the settings: GPU (Media Foundation), CPU (openh264), or GPU falling back to CPU.
fn encoder(
    w: usize,
    h: usize,
    fps: u32,
    mbps: u32,
    choice: Encoder,
) -> io::Result<(EncodeFn, &'static str)> {
    let bitrate = mbps * 1_000_000;
    if choice != Encoder::Cpu {
        match HwEncoder::new(w, h, fps, bitrate) {
            Ok(mut e) => {
                return Ok((
                    Box::new(move |f, out| e.encode(f, out).map_err(io::Error::other)),
                    "GPU",
                ))
            }
            Err(err) if choice == Encoder::Gpu => return Err(io::Error::other(err)),
            Err(err) => crate::telemetry::warn(format!(
                "hardware encoder unavailable ({err}), using openh264"
            )),
        }
    }
    let mut e = H264::new(w, h, fps, bitrate).map_err(io::Error::other)?;
    Ok((
        Box::new(move |f, out| e.encode(f, out).map_err(io::Error::other)),
        "CPU",
    ))
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
    fn shared_control_fixtures_and_truncated_headers() {
        let hello = crate::test_fixtures::text("hello.json").as_bytes();
        let config = crate::test_fixtures::text("config.json").as_bytes();
        let mut buf = Vec::new();
        write_msg(&mut buf, HELLO, hello).unwrap();
        write_msg(&mut buf, CONFIG, config).unwrap();
        let mut reader = &buf[..];
        assert_eq!(read_msg(&mut reader).unwrap(), (HELLO, hello.to_vec()));
        assert_eq!(read_msg(&mut reader).unwrap(), (CONFIG, config.to_vec()));
        let parsed: Value = serde_json::from_slice(hello).unwrap();
        assert_eq!(parsed["v"], 3);
        assert!(serde_json::from_slice::<Value>(b"{bad json").is_err());
        assert!(read_msg(&mut &[HELLO, 0, 0, 0][..]).is_err());
        assert!(read_msg(&mut &[HELLO, 0, 0, 0, 2, b'{'][..]).is_err());
        assert!(write_msg(&mut Vec::new(), 99, &[]).is_err());
        assert!(write_msg(&mut Vec::new(), CONFIG, &vec![0; MAX_CONTROL + 1]).is_err());
        let hello: Hello = serde_json::from_str(crate::test_fixtures::text("hello.json")).unwrap();
        assert!(validate_hello(&hello).is_ok());
        assert!(parse_dimensions(br#"{"decodable":[0,800]}"#, "decodable").is_none());
        assert!(parse_dimensions(br#"{"decodable":[1280,800]}"#, "decodable").is_some());
    }

    #[test]
    fn text_transfer_is_bounded_and_explicit() {
        let payload =
            serde_json::to_vec(&json!({ "version": 1, "source": "tablet", "text": "Olá" }))
                .unwrap();
        assert_eq!(parse_text(&payload, "tablet").as_deref(), Some("Olá"));
        assert!(parse_text(
            &serde_json::to_vec(&json!({ "version": 1, "source": "pc", "text": "Olá" })).unwrap(),
            "tablet"
        )
        .is_none());
        assert!(validate_text(&"x".repeat(MAX_TEXT)).is_ok());
        assert!(validate_text(&"x".repeat(MAX_TEXT + 1)).is_err());
        assert!(write_msg(&mut Vec::new(), TEXT, &payload).is_ok());
    }

    #[test]
    fn text_inbox_is_discarded_with_the_session() {
        *TEXT_INBOX.lock().unwrap() = Some(ReceivedText {
            session_id: 77,
            sender: "Tablet de teste".into(),
            text: "conteúdo fictício".into(),
        });
        clear_text_for_session(76);
        assert!(TEXT_INBOX.lock().unwrap().is_some());
        clear_text_for_session(77);
        assert!(TEXT_INBOX.lock().unwrap().is_none());
    }

    #[test]
    fn quality_profiles() {
        let tablet = (2304, 1440);
        let caps = TabletCaps {
            decodable: tablet,
            modes: vec![
                VideoMode {
                    width: 2304,
                    height: 1440,
                    fps: 60,
                },
                VideoMode {
                    width: 2304,
                    height: 1440,
                    fps: 90,
                },
            ],
        };
        let mut s = Settings::default();
        assert_eq!(plan(&s, &caps, 1), ((2304, 1440), 60, 20)); // Balanced is the default
        s.profile = Profile::Performance;
        assert_eq!(plan(&s, &caps, 1), ((1152, 720), 60, 10));
        s.profile = Profile::Quality;
        assert_eq!(plan(&s, &caps, 1).2, 40);
        s = Settings {
            profile: Profile::Custom,
            resolution: Some((2560, 1600)),
            fps: 90,
            bitrate_mbps: 30,
            ..s
        };
        assert_eq!(plan(&s, &caps, 1), ((2304, 1440), 90, 30)); // custom still fits the decoder
        assert_eq!(
            plan(
                &s,
                &TabletCaps {
                    decodable: (1440, 2304),
                    modes: caps.modes.clone()
                },
                1
            )
            .0,
            (2304, 1440)
        ); // a portrait tablet decodes the landscape size too
    }

    #[test]
    fn metric_window_is_bounded_and_reports_percentiles() {
        let mut window = MetricWindow::default();
        for value in 0..(MetricWindow::CAP + 20) {
            MetricWindow::add(&mut window.encode_ms, value as f32, &mut window.lost);
        }
        assert_eq!(window.encode_ms.len(), MetricWindow::CAP);
        assert_eq!(window.lost, 20);
        assert!(MetricWindow::percentile(&window.encode_ms, 0.50) >= 0.0);
        assert!(MetricWindow::percentile(&window.encode_ms, 0.99) <= 147.0);
    }

    #[test]
    fn capability_modes_negotiate_fps_without_overpromising() {
        let caps = TabletCaps {
            decodable: (1920, 1080),
            modes: vec![VideoMode {
                width: 1920,
                height: 1080,
                fps: 60,
            }],
        };
        let custom = Settings {
            profile: Profile::Custom,
            resolution: Some((1920, 1080)),
            fps: 120,
            ..Settings::default()
        };
        assert_eq!(plan(&custom, &caps, 1), ((1920, 1072), 60, 20));
    }

    #[test]
    fn pacing_deadline_does_not_add_encode_cost_or_replay_overdue_slots() {
        let base = Instant::now();
        let interval = Duration::from_millis(16);
        assert_eq!(
            next_deadline_after(base, base + Duration::from_millis(8), interval),
            base + interval
        );
        assert_eq!(
            next_deadline_after(base, base + Duration::from_millis(25), interval),
            base + interval * 2
        );
        assert_eq!(
            next_deadline_after(base, base + Duration::from_millis(80), interval),
            base + interval * 6
        );
    }

    #[test]
    fn auto_steps_down_on_bad_link_and_back_up_when_calm() {
        let mut a = Adapt::new();
        let bad = |a: &mut Adapt| a.tick(200, 0.0, 0, 0.0, 0.0, 0.0, 0, 0, false);
        let calm = |a: &mut Adapt| a.tick(20, 30.0, 30, 0.0, 0.0, 0.0, 0, 0, false);
        assert!(!bad(&mut a) && !bad(&mut a)); // two bad seconds: not yet
        assert!(bad(&mut a) && a.level == 2); // third: Balanced -> Performance
        assert!(!bad(&mut a) && a.level == 2); // already at the lightest
        for _ in 0..50 {
            calm(&mut a); // calm, but still held
        }
        assert_eq!(a.level, 2);
        let mut up = false;
        for _ in 0..40 {
            up |= calm(&mut a);
        }
        assert!(up && a.level < 2);
        // The tablet showing far fewer frames than were sent counts as struggling too.
        let mut b = Adapt::new();
        let tablet = |b: &mut Adapt| b.tick(10, 60.0, 20, 0.0, 0.0, 0.0, 1, 0, false);
        assert!(!tablet(&mut b) && !tablet(&mut b) && tablet(&mut b));
        assert_eq!(b.reason, "tablet");
    }

    #[test]
    fn adaptation_ignores_static_scene_and_classifies_cost() {
        let mut a = Adapt::new();
        for _ in 0..10 {
            assert!(!a.tick(500, 0.0, 0, 100.0, 100.0, 100.0, 10, 10, true));
        }
        assert_eq!(a.level, ADAPT_START);
        assert_eq!(a.reason, "tela");

        let mut b = Adapt::new();
        assert!(!b.tick(10, 60.0, 60, 80.0, 0.0, 0.0, 0, 0, false));
        assert_eq!(b.reason, "encoder");
        assert!(RAW_FRAME_SLOTS <= 2);
        assert_eq!(MAX_ENCODED_IN_FLIGHT, MAX_VIDEO);
    }

    #[test]
    fn audio_turn_limits_packets_and_wire_bytes_without_dropping_next_packet() {
        let queue = crate::audio::Queue::default();
        for sequence in 0..3 {
            queue.push(crate::audio::Packet {
                sequence,
                pts_samples: sequence * 960,
                opus: vec![sequence as u8; 10],
            });
        }
        let first = take_audio_turn(&queue, false);
        assert_eq!(first.len(), AUDIO_PACKETS_PER_TURN);
        assert_eq!(first[0].sequence, 0);
        assert_eq!(take_audio_turn(&queue, false).len(), 1);

        let bounded = crate::audio::Queue::default();
        bounded.push(crate::audio::Packet {
            sequence: 9,
            pts_samples: 0,
            opus: vec![0; MAX_AUDIO - 22 - 1],
        });
        bounded.push(crate::audio::Packet {
            sequence: 10,
            pts_samples: 960,
            opus: vec![1; 2],
        });
        let timestamped = take_audio_turn(&bounded, true);
        assert_eq!(
            timestamped.len(),
            1,
            "AUDIO v1 overhead must count toward the turn budget"
        );
        assert_eq!(timestamped[0].sequence, 9);
        assert_eq!(take_audio_turn(&bounded, true)[0].sequence, 10);
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
        assert_eq!(fit((1920, 1080), tablet), (1920, 1072));
        assert_eq!(fit((1200, 1920), tablet), (1200, 1920)); // portrait fits the swapped limit
        assert_eq!(fit((2560, 1600), tablet), (2304, 1440));
        assert_eq!(fit((2560, 1440), tablet), (2304, 1296));
    }

    #[test]
    fn recording_controller_rotates_complete_segments_without_concatenating_them() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("tabdisplay-server-recording-{nonce}"));
        std::fs::create_dir(&dir).unwrap();
        let base = dir.join("session.mkv");
        let config = |generation| crate::recording::StreamConfig {
            generation,
            width: 1152,
            height: 640,
            fps: 60,
            audio: true,
        };
        let keyframe = vec![
            0, 0, 0, 1, 0x67, 0x42, 0, 0x1f, 0, 0, 1, 0x68, 0xce, 6, 0xe2, 0, 0, 1, 0x65, 0x88,
        ];
        assert!(is_keyframe(&keyframe));
        let mut recording = ActiveRecording::new(1, base.clone()).unwrap();
        recording.configure(config(1));
        recording.video(0, true, keyframe.clone());
        recording.configure(config(2));
        recording.video(33, true, keyframe);
        let paths = recording.finish().unwrap();
        assert_eq!(paths.len(), 2);
        assert!(base.exists());
        assert!(crate::recording::Destination::rotated_path(&base, 1).exists());
        for path in paths {
            let bytes = std::fs::read(&path).unwrap();
            assert_eq!(&bytes[..4], &[0x1a, 0x45, 0xdf, 0xa3]);
            std::fs::remove_file(path).unwrap();
        }
        std::fs::remove_dir(dir).unwrap();
    }
}
