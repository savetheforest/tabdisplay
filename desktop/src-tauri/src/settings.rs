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

impl Settings {
    /// Rejects values before they reach capture, the encoder or the virtual display.
    pub fn validate(&self) -> Result<(), &'static str> {
        if let Some((width, height)) = self.resolution {
            if width < 16
                || height < 16
                || width > 7680
                || height > 7680
                || width % 16 != 0
                || height % 16 != 0
                || (width as u64) * (height as u64) > 16_777_216
            {
                return Err("resolution out of bounds");
            }
        }
        if !(1..=240).contains(&self.fps) {
            return Err("fps out of bounds");
        }
        if !(1..=200).contains(&self.bitrate_mbps) {
            return Err("bitrate out of bounds");
        }
        if self
            .mirror_monitor
            .as_ref()
            .is_some_and(|name| name.len() > 512)
        {
            return Err("monitor name too long");
        }
        Ok(())
    }
}

static CURRENT: Mutex<Option<Settings>> = Mutex::new(None);
static PATH: OnceLock<PathBuf> = OnceLock::new();
pub static VERSION: AtomicU64 = AtomicU64::new(0);

/// Loads settings from `dir/settings.json` (defaults if missing or invalid).
pub fn init(dir: PathBuf) {
    let path = dir.join("settings.json");
    // 0.1 used the identifier com.tabdisplay.app; carry its settings over once.
    let old = dir
        .with_file_name("com.tabdisplay.app")
        .join("settings.json");
    let text = std::fs::read_to_string(&path).or_else(|_| std::fs::read_to_string(old));
    // Settings from before the first-run guide existed belong to someone who already knows the app.
    let known_user = text.as_ref().is_ok_and(|t| !t.contains("\"onboarded\""));
    let mut loaded: Option<Settings> = text.ok().and_then(|s| {
        serde_json::from_str(&s)
            .ok()
            .filter(|settings: &Settings| settings.validate().is_ok())
    });
    if let (Some(s), true) = (&mut loaded, known_user) {
        s.onboarded = true;
    }
    let mut settings = loaded.unwrap_or_default();
    require_licence_for_extend(&mut settings);
    *CURRENT.lock().unwrap() = Some(settings);
    let _ = PATH.set(path);
}

pub fn get() -> Settings {
    CURRENT.lock().unwrap().clone().unwrap_or_default()
}

/// Extending the desktop needs a licence; without one the mode stays Mirror.
fn require_licence_for_extend(s: &mut Settings) {
    if s.mode == Mode::Extend && !crate::license::valid() {
        s.mode = Mode::Mirror;
    }
}

fn video_changed(before: &Settings, after: &Settings) -> bool {
    before.mode != after.mode
        || before.profile != after.profile
        || before.resolution != after.resolution
        || before.position != after.position
        || before.mirror_monitor != after.mirror_monitor
        || before.fps != after.fps
        || before.bitrate_mbps != after.bitrate_mbps
        || before.encoder != after.encoder
}

pub(crate) fn persist(path: &PathBuf, text: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "pasta de configurações inválida".to_string())?;
    std::fs::create_dir_all(parent)
        .map_err(|e| format!("não foi possível criar a pasta de configurações: {e}"))?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, text)
        .map_err(|e| format!("não foi possível escrever configurações temporárias: {e}"))?;
    #[cfg(windows)]
    {
        let backup = path.with_extension("json.bak");
        if path.exists() {
            let _ = std::fs::remove_file(&backup);
            std::fs::rename(path, &backup)
                .map_err(|e| format!("não foi possível preparar substituição atômica: {e}"))?;
            if let Err(e) = std::fs::rename(&tmp, path) {
                let _ = std::fs::rename(&backup, path);
                let _ = std::fs::remove_file(&tmp);
                return Err(format!("não foi possível substituir configurações: {e}"));
            }
            let _ = std::fs::remove_file(backup);
        } else {
            std::fs::rename(&tmp, path)
                .map_err(|e| format!("não foi possível ativar configurações: {e}"))?;
        }
    }
    #[cfg(not(windows))]
    std::fs::rename(&tmp, path)
        .map_err(|e| format!("não foi possível ativar configurações: {e}"))?;
    Ok(())
}

pub fn set(mut s: Settings) -> Result<(), String> {
    if let Err(reason) = s.validate() {
        crate::telemetry::warn(format!("invalid settings ignored: {reason}"));
        return Err(reason.into());
    }
    require_licence_for_extend(&mut s);
    let before = get();
    if let Some(path) = PATH.get() {
        let text = serde_json::to_vec_pretty(&s)
            .map_err(|e| format!("não foi possível serializar configurações: {e}"))?;
        persist(path, &text)?;
    }
    let changed = video_changed(&before, &s);
    *CURRENT.lock().unwrap() = Some(s);
    if changed {
        VERSION.fetch_add(1, Ordering::Relaxed);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn extend_needs_a_licence() {
        // No licence is loaded in tests: asking for Extend leaves the mode at Mirror.
        set(Settings {
            mode: Mode::Extend,
            ..Default::default()
        })
        .unwrap();
        assert!(get().mode == Mode::Mirror);
    }

    #[test]
    fn rejects_unbounded_video_settings() {
        let mut invalid = Settings::default();
        invalid.fps = 0;
        assert!(invalid.validate().is_err());
        invalid = Settings {
            resolution: Some((0, 4096)),
            ..Settings::default()
        };
        assert!(invalid.validate().is_err());
        invalid = Settings {
            resolution: Some((4096, 4096)),
            ..Settings::default()
        };
        assert!(invalid.validate().is_ok());
    }

    #[test]
    fn only_video_fields_bump_the_stream_revision() {
        let before = Settings::default();
        let audio_only = Settings {
            audio: false,
            ..before.clone()
        };
        assert!(!video_changed(&before, &audio_only));
        let profile = Settings {
            profile: Profile::Quality,
            ..before.clone()
        };
        assert!(video_changed(&before, &profile));
        let onboarding = Settings {
            onboarded: true,
            ..before
        };
        assert!(!video_changed(&Settings::default(), &onboarding));
    }

    #[test]
    fn persistence_replaces_existing_file_without_leftover_staging_files() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("tabdisplay-settings-{nonce}"));
        let path = dir.join("settings.json");
        let first = serde_json::to_vec(&Settings {
            bitrate_mbps: 10,
            ..Settings::default()
        })
        .unwrap();
        let second = serde_json::to_vec(&Settings {
            bitrate_mbps: 40,
            ..Settings::default()
        })
        .unwrap();

        persist(&path, &first).unwrap();
        assert_eq!(
            serde_json::from_slice::<Settings>(&std::fs::read(&path).unwrap())
                .unwrap()
                .bitrate_mbps,
            10
        );
        persist(&path, &second).unwrap();
        let loaded: Settings = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(loaded.bitrate_mbps, 40);
        assert!(!path.with_extension("json.tmp").exists());
        assert!(!path.with_extension("json.bak").exists());

        std::fs::remove_dir_all(dir).unwrap();
    }
}
