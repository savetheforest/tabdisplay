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

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub mode: Mode,
    /// Extend: virtual monitor size; None = tablet's native resolution.
    pub resolution: Option<(u32, u32)>,
    pub position: Position,
    /// Mirror: GDI name of the monitor to mirror; None = primary.
    pub mirror_monitor: Option<String>,
    pub fps: u32,
    pub bitrate_mbps: u32,
    pub encoder: Encoder,
    pub touch: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            mode: Mode::Extend,
            resolution: None,
            position: Position::Right,
            mirror_monitor: None,
            fps: 60,
            bitrate_mbps: 20,
            encoder: Encoder::Auto,
            touch: true,
        }
    }
}

static CURRENT: Mutex<Option<Settings>> = Mutex::new(None);
static PATH: OnceLock<PathBuf> = OnceLock::new();
pub static VERSION: AtomicU64 = AtomicU64::new(0);

/// Loads settings from `dir/settings.json` (defaults if missing or invalid).
pub fn init(dir: PathBuf) {
    let path = dir.join("settings.json");
    let loaded = std::fs::read_to_string(&path).ok().and_then(|s| serde_json::from_str(&s).ok());
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
