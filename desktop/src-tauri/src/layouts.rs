//! Versioned per-tablet layout presets and presentation planning.
//!
//! This module computes an explicit plan. It does not apply a preset to the
//! display driver or move other applications' windows.

use crate::settings::{Mode, Position, Profile, Settings, TouchMode};
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const CURRENT_SCHEMA: u32 = 1;
const MAX_PRESETS: usize = 16;
const MAX_DEVICE_ID_BYTES: usize = 128;
const MAX_MONITOR_NAME_BYTES: usize = 512;

#[derive(Clone, Serialize, Deserialize, PartialEq)]
pub struct PresentationOptions {
    pub allow_input: bool,
    pub show_stats: bool,
    pub keep_awake: bool,
}

impl Default for PresentationOptions {
    fn default() -> Self {
        Self {
            allow_input: false,
            show_stats: false,
            keep_awake: true,
        }
    }
}

#[derive(Clone, Serialize, Deserialize, PartialEq)]
pub struct LayoutPreset {
    pub schema: u32,
    pub device_id: String,
    pub mode: Mode,
    pub position: Position,
    pub mirror_monitor: Option<String>,
    pub profile: Profile,
    pub audio: bool,
    pub input: bool,
    pub touch_mode: TouchMode,
    pub presentation: PresentationOptions,
}

impl LayoutPreset {
    pub fn from_settings(device_id: &str, settings: &Settings) -> Result<Self, PresetError> {
        validate_device_id(device_id)?;
        let preset = Self {
            schema: CURRENT_SCHEMA,
            device_id: device_id.to_owned(),
            mode: settings.mode,
            position: settings.position,
            mirror_monitor: settings.mirror_monitor.clone(),
            profile: settings.profile,
            audio: settings.audio,
            input: settings.touch,
            touch_mode: settings.touch_mode,
            presentation: PresentationOptions::default(),
        };
        preset.validate()?;
        Ok(preset)
    }

    pub fn validate(&self) -> Result<(), PresetError> {
        if self.schema != CURRENT_SCHEMA {
            return Err(PresetError::UnsupportedSchema(self.schema));
        }
        validate_device_id(&self.device_id)?;
        if self
            .mirror_monitor
            .as_ref()
            .is_some_and(|name| name.as_bytes().len() > MAX_MONITOR_NAME_BYTES)
        {
            return Err(PresetError::MonitorNameTooLong);
        }
        Ok(())
    }

    pub fn to_settings(&self) -> Settings {
        self.apply_to(Settings::default())
    }

    /// Applies only fields owned by a layout preset, preserving custom video
    /// limits and onboarding state from the current settings.
    pub fn apply_to(&self, mut settings: Settings) -> Settings {
        settings.mode = self.mode;
        settings.position = self.position;
        settings.mirror_monitor = self.mirror_monitor.clone();
        settings.profile = self.profile;
        settings.audio = self.audio;
        settings.touch = self.input;
        settings.touch_mode = self.touch_mode;
        settings
    }
}

#[derive(Clone, Serialize, Deserialize, PartialEq)]
pub struct PresetDocument {
    pub schema: u32,
    pub presets: Vec<LayoutPreset>,
}

impl Default for PresetDocument {
    fn default() -> Self {
        Self {
            schema: CURRENT_SCHEMA,
            presets: Vec::new(),
        }
    }
}

impl PresetDocument {
    pub fn validate(&self) -> Result<(), PresetError> {
        if self.schema != CURRENT_SCHEMA {
            return Err(PresetError::UnsupportedSchema(self.schema));
        }
        if self.presets.len() > MAX_PRESETS {
            return Err(PresetError::TooManyPresets);
        }
        for (index, preset) in self.presets.iter().enumerate() {
            preset
                .validate()
                .map_err(|error| PresetError::InvalidPreset {
                    index,
                    error: Box::new(error),
                })?;
        }
        for (index, preset) in self.presets.iter().enumerate() {
            if self.presets[..index]
                .iter()
                .any(|other| other.device_id == preset.device_id)
            {
                return Err(PresetError::DuplicateDevice(preset.device_id.clone()));
            }
        }
        Ok(())
    }

    pub fn from_json(bytes: &[u8]) -> Result<Self, PresetError> {
        let value: serde_json::Value =
            serde_json::from_slice(bytes).map_err(|error| PresetError::Json(error.to_string()))?;
        if value.get("schema").is_some() {
            let document: Self = serde_json::from_value(value)
                .map_err(|error| PresetError::Json(error.to_string()))?;
            document.validate()?;
            return Ok(document);
        }

        // Version zero was a single preset without a document wrapper. Keep
        // its preferences, but never silently accept a future/unknown shape.
        let legacy: LegacyPreset =
            serde_json::from_value(value).map_err(|error| PresetError::Json(error.to_string()))?;
        let preset = LayoutPreset {
            schema: CURRENT_SCHEMA,
            device_id: legacy.device_id,
            mode: legacy.mode,
            position: legacy.position,
            mirror_monitor: legacy.mirror_monitor,
            profile: legacy.profile,
            audio: legacy.audio,
            input: legacy.input,
            touch_mode: legacy.touch_mode,
            presentation: PresentationOptions::default(),
        };
        let document = Self {
            schema: CURRENT_SCHEMA,
            presets: vec![preset],
        };
        document.validate()?;
        Ok(document)
    }

    pub fn to_json(&self) -> Result<Vec<u8>, PresetError> {
        self.validate()?;
        serde_json::to_vec_pretty(self).map_err(|error| PresetError::Json(error.to_string()))
    }

    pub fn upsert(&mut self, preset: LayoutPreset) -> Result<(), PresetError> {
        preset.validate()?;
        if let Some(existing) = self
            .presets
            .iter_mut()
            .find(|existing| existing.device_id == preset.device_id)
        {
            *existing = preset;
        } else {
            if self.presets.len() == MAX_PRESETS {
                return Err(PresetError::TooManyPresets);
            }
            self.presets.push(preset);
        }
        self.validate()
    }
}

#[derive(Debug, PartialEq)]
pub enum PresetError {
    Json(String),
    Io(String),
    UnsupportedSchema(u32),
    InvalidDeviceId,
    MonitorNameTooLong,
    TooManyPresets,
    DuplicateDevice(String),
    InvalidPreset {
        index: usize,
        error: Box<PresetError>,
    },
}

#[derive(Deserialize)]
struct LegacyPreset {
    device_id: String,
    mode: Mode,
    position: Position,
    mirror_monitor: Option<String>,
    profile: Profile,
    audio: bool,
    #[serde(alias = "touch")]
    input: bool,
    touch_mode: TouchMode,
}

fn validate_device_id(device_id: &str) -> Result<(), PresetError> {
    if device_id.is_empty()
        || device_id.as_bytes().len() > MAX_DEVICE_ID_BYTES
        || device_id.chars().any(char::is_control)
    {
        return Err(PresetError::InvalidDeviceId);
    }
    Ok(())
}

pub fn load(path: &Path) -> Result<PresetDocument, PresetError> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(PresetDocument::default())
        }
        Err(error) => return Err(PresetError::Io(error.to_string())),
    };
    PresetDocument::from_json(&bytes)
}

pub fn save(path: &Path, document: &PresetDocument) -> Result<(), PresetError> {
    let bytes = document.to_json()?;
    crate::settings::persist(&path.to_path_buf(), &bytes).map_err(PresetError::Io)
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct ApplyAdjustment {
    pub field: &'static str,
    pub reason: &'static str,
}

#[derive(Clone, Serialize, PartialEq)]
pub struct ApplyPlan {
    pub requested: LayoutPreset,
    pub effective: LayoutPreset,
    pub adjustments: Vec<ApplyAdjustment>,
    pub requires_choice: bool,
}

pub fn plan_apply(
    preset: LayoutPreset,
    has_license: bool,
    available_monitors: &[String],
) -> ApplyPlan {
    let mut effective = preset.clone();
    let mut adjustments = Vec::new();
    if effective.mode == Mode::Extend && !has_license {
        effective.mode = Mode::Mirror;
        adjustments.push(ApplyAdjustment {
            field: "mode",
            reason: "license_required_for_extend",
        });
    }
    let missing_monitor = effective.mode == Mode::Mirror
        && effective
            .mirror_monitor
            .as_ref()
            .is_some_and(|name| !available_monitors.iter().any(|available| available == name));
    if missing_monitor {
        adjustments.push(ApplyAdjustment {
            field: "mirror_monitor",
            reason: "selected_monitor_unavailable",
        });
    }
    ApplyPlan {
        requested: preset,
        effective,
        requires_choice: missing_monitor,
        adjustments,
    }
}

#[derive(Clone, PartialEq)]
pub struct PresentationSession {
    previous: LayoutPreset,
    current: LayoutPreset,
}

impl PresentationSession {
    pub fn enter(previous: LayoutPreset, options: PresentationOptions) -> Self {
        let mut current = previous.clone();
        current.input = previous.input && options.allow_input;
        current.presentation = options;
        Self { previous, current }
    }

    pub fn current(&self) -> &LayoutPreset {
        &self.current
    }

    pub fn exit(self) -> LayoutPreset {
        self.previous
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn preset(device_id: &str) -> LayoutPreset {
        LayoutPreset::from_settings(device_id, &Settings::default()).unwrap()
    }

    #[test]
    fn saves_and_reloads_multiple_tablet_presets_atomically() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir()
            .join(format!("tabdisplay-layouts-{nonce}"))
            .join("presets.json");
        let mut document = PresetDocument::default();
        document.upsert(preset("tablet-a")).unwrap();
        document.upsert(preset("tablet-b")).unwrap();
        save(&path, &document).unwrap();
        let loaded = load(&path).unwrap();
        assert_eq!(loaded.presets.len(), 2);
        assert!(!path.with_extension("json.tmp").exists());
        assert!(!path.with_extension("json.bak").exists());
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn migrates_legacy_single_preset_without_losing_preferences() {
        let legacy = br#"{"device_id":"tablet-a","mode":"mirror","position":"left","mirror_monitor":"HDMI-1","profile":"quality","audio":false,"touch":true,"touch_mode":"mouse"}"#;
        let document = PresetDocument::from_json(legacy).unwrap();
        assert_eq!(document.schema, CURRENT_SCHEMA);
        let migrated = &document.presets[0];
        assert_eq!(migrated.device_id, "tablet-a");
        assert_eq!(migrated.mirror_monitor.as_deref(), Some("HDMI-1"));
        assert!(!migrated.audio);
        assert!(migrated.input);
    }

    #[test]
    fn rejects_duplicate_devices_and_invalid_ids() {
        let mut document = PresetDocument::default();
        document.upsert(preset("tablet-a")).unwrap();
        assert_eq!(document.upsert(preset("tablet-a")), Ok(()));
        assert!(LayoutPreset::from_settings("", &Settings::default()).is_err());
        let duplicate = PresetDocument {
            schema: CURRENT_SCHEMA,
            presets: vec![preset("a"), preset("a")],
        };
        assert!(matches!(
            duplicate.validate(),
            Err(PresetError::DuplicateDevice(_))
        ));
    }

    #[test]
    fn plans_explicit_license_and_missing_monitor_adjustments() {
        let mut requested = preset("tablet-a");
        requested.mode = Mode::Extend;
        requested.mirror_monitor = Some("Gone".into());
        let plan = plan_apply(requested.clone(), false, &[]);
        assert!(plan.effective.mode == Mode::Mirror);
        assert!(plan.requires_choice);
        assert!(plan.requested == requested);
        assert_eq!(plan.adjustments.len(), 2);
    }

    #[test]
    fn presentation_disables_input_and_restores_previous_preset() {
        let previous = preset("tablet-a");
        let session = PresentationSession::enter(previous.clone(), PresentationOptions::default());
        assert!(!session.current().input);
        assert!(session.exit() == previous);
    }

    #[test]
    fn applying_a_preset_preserves_unowned_video_and_onboarding_fields() {
        let mut current = Settings::default();
        current.fps = 90;
        current.bitrate_mbps = 55;
        current.onboarded = false;
        let mut saved = preset("tablet-a");
        saved.audio = false;
        saved.input = false;
        let applied = saved.apply_to(current);
        assert_eq!(applied.fps, 90);
        assert_eq!(applied.bitrate_mbps, 55);
        assert!(!applied.onboarded);
        assert!(!applied.audio);
        assert!(!applied.touch);
    }
}
