//! User settings, persisted as JSON. Changing them bumps VERSION, which makes the running
//! session rebuild its display/capture/encoder without dropping the tablet connection.
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

#[derive(Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Extend,
    Mirror,
}

#[derive(Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Position {
    Right,
    Left,
    Above,
    Below,
}

#[derive(Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Encoder {
    Auto,
    Gpu,
    Cpu,
}

/// How finger touches reach the PC. The pen is always a pen.
#[derive(Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TouchMode {
    /// Real Windows touch: scroll, pinch and press-and-hold come from Windows itself.
    Native,
    /// First finger drives the mouse, for apps that ignore touch.
    Mouse,
}

/// Quality presets; Custom uses `resolution`, `fps` and `bitrate_mbps`.
#[derive(Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Profile {
    /// Half the tablet's resolution, 10 Mbps: smooth on weak Wi-Fi.
    Performance,
    /// Native resolution, 20 Mbps.
    Balanced,
    /// Native resolution, 40 Mbps: sharpest, needs a good link.
    Quality,
    /// Starts at Balanced and steps between the presets by itself as the network gets worse or better.
    Auto,
    Custom,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub mode: Mode,
    pub profile: Profile,
    /// Extend: virtual monitor size; None = tablet's native resolution.
    pub resolution: Option<(u32, u32)>,
    pub position: Position,
    /// Mirror: GDI name of the monitor to mirror; None = primary.
    pub mirror_monitor: Option<String>,
    pub fps: u32,
    pub bitrate_mbps: u32,
    pub encoder: Encoder,
    pub touch: bool,
    pub touch_mode: TouchMode,
    /// Send the PC's audio to the tablet.
    pub audio: bool,
    /// The first-run guide was shown (or skipped).
    pub onboarded: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            mode: Mode::Extend,
            profile: Profile::Balanced,
            resolution: None,
            position: Position::Right,
            mirror_monitor: None,
            fps: 60,
            bitrate_mbps: 20,
            encoder: Encoder::Auto,
            touch: true,
            touch_mode: TouchMode::Native,
            audio: true,
            onboarded: false,
        }
    }
}

static CURRENT: Mutex<Option<Settings>> = Mutex::new(None);
static PATH: OnceLock<PathBuf> = OnceLock::new();
pub static VERSION: AtomicU64 = AtomicU64::new(0);

/// Loads settings from `dir/settings.json` (defaults if missing or invalid).
pub fn init(dir: PathBuf) {
    let path = dir.join("settings.json");
    // 0.1 used the identifier com.tabdisplay.app; carry its settings over once.
    let old = dir.with_file_name("com.tabdisplay.app").join("settings.json");
    let text = std::fs::read_to_string(&path).or_else(|_| std::fs::read_to_string(old));
    // Settings from before the first-run guide existed belong to someone who already knows the app.
    let known_user = text.as_ref().is_ok_and(|t| !t.contains("\"onboarded\""));
    let mut loaded: Option<Settings> = text.ok().and_then(|s| serde_json::from_str(&s).ok());
    if let (Some(s), true) = (&mut loaded, known_user) {
        s.onboarded = true;
    }
    *CURRENT.lock().unwrap() = Some(loaded.unwrap_or_default());
    let _ = PATH.set(path);
}

pub fn get() -> Settings {
    CURRENT.lock().unwrap().clone().unwrap_or_default()
}

pub fn set(s: Settings) {
    if let Some(path) = PATH.get() {
        let _ = std::fs::create_dir_all(path.parent().unwrap());
        let _ = std::fs::write(path, serde_json::to_string_pretty(&s).unwrap());
    }
    *CURRENT.lock().unwrap() = Some(s);
    VERSION.fetch_add(1, Ordering::Relaxed);
}
