use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Popup,
    Pinned,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub mode: Mode,
    pub saved_position: Option<(i32, i32)>,
    pub mini_position: Option<(i32, i32)>,
    pub shown: [bool; 3],
    pub animations_enabled: bool,
    pub opacity_percent: u8,
    // 0 = compact (former standard), 1 = standard (new midpoint), 2 = spacious.
    pub capsule_size: u8,
    // Old files without a version used index 1 for the 88px size.
    #[serde(default = "legacy_size_preset_version")]
    pub size_preset_version: u8,
    pub edge_hide: bool,
    pub dock_left: bool,
}

const CURRENT_SIZE_PRESET_VERSION: u8 = 2;

fn legacy_size_preset_version() -> u8 {
    1
}

impl Preferences {
    fn migrate_size_preset(&mut self) {
        if self.size_preset_version < CURRENT_SIZE_PRESET_VERSION {
            // The old 88px standard is the new compact; old spacious remains
            // spacious. Retired 72px compact has no identical preset.
            self.capsule_size = match self.capsule_size {
                0 | 1 => 0,
                _ => 2,
            };
            self.size_preset_version = CURRENT_SIZE_PRESET_VERSION;
        }
        self.capsule_size = self.capsule_size.min(2);
    }
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            mode: Mode::Popup,
            saved_position: None,
            mini_position: None,
            shown: [true; 3],
            animations_enabled: true,
            opacity_percent: 95,
            capsule_size: 1,
            size_preset_version: CURRENT_SIZE_PRESET_VERSION,
            edge_hide: false,
            dock_left: false,
        }
    }
}

fn config_path() -> PathBuf {
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("LOCALAPPDATA").map(PathBuf::from))
        .unwrap_or_else(std::env::temp_dir);
    base.join("Wacu").join("preferences.json")
}

pub fn load() -> Preferences {
    let Ok(data) = fs::read(config_path()) else {
        return Preferences::default();
    };
    let mut prefs = serde_json::from_slice::<Preferences>(&data).unwrap_or_default();
    prefs.migrate_size_preset();
    prefs
}

pub fn save(prefs: &Preferences) -> Result<(), String> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let contents = serde_json::to_vec_pretty(prefs).map_err(|e| e.to_string())?;
    let temp = path.with_extension("tmp");
    fs::write(&temp, contents).map_err(|e| e.to_string())?;
    fs::rename(&temp, path).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preferences_backward_compatible() {
        let prefs: Preferences = serde_json::from_str(r#"{"mode":"popup"}"#).unwrap();
        assert_eq!(prefs.mode, Mode::Popup);
        assert_eq!(prefs.shown, [true; 3]);
        assert!(prefs.animations_enabled);
        assert_eq!(prefs.opacity_percent, 95);
        assert_eq!(prefs.capsule_size, 1);
        assert_eq!(prefs.size_preset_version, 1);
        assert!(!prefs.edge_hide);
        assert!(!prefs.dock_left);
    }

    #[test]
    fn old_standard_preset_preserves_exact_display_size() {
        let mut legacy: Preferences =
            serde_json::from_str(r#"{"capsule_size":1,"mode":"pinned"}"#).unwrap();
        assert_eq!(legacy.size_preset_version, 1);
        legacy.migrate_size_preset();
        assert_eq!(legacy.capsule_size, 0); // old standard 88px is now compact
        assert_eq!(legacy.size_preset_version, CURRENT_SIZE_PRESET_VERSION);
        let mut reloaded: Preferences =
            serde_json::from_str(&serde_json::to_string(&legacy).unwrap()).unwrap();
        reloaded.migrate_size_preset();
        assert_eq!(reloaded.capsule_size, 0); // idempotent
    }

    #[test]
    fn old_spacious_stays_spacious_and_new_default_is_midpoint() {
        let mut legacy: Preferences = serde_json::from_str(r#"{"capsule_size":2}"#).unwrap();
        legacy.migrate_size_preset();
        assert_eq!(legacy.capsule_size, 2);
        let fresh = Preferences::default();
        assert_eq!(fresh.capsule_size, 1);
        assert_eq!(fresh.size_preset_version, CURRENT_SIZE_PRESET_VERSION);
    }

    #[test]
    fn visual_preferences_round_trip() {
        let mut prefs = Preferences::default();
        prefs.animations_enabled = false;
        prefs.opacity_percent = 72;
        prefs.capsule_size = 2;
        prefs.edge_hide = true;
        prefs.dock_left = true;
        let json = serde_json::to_string(&prefs).unwrap();
        let restored: Preferences = serde_json::from_str(&json).unwrap();
        assert!(!restored.animations_enabled);
        assert_eq!(restored.opacity_percent, 72);
        assert_eq!(restored.capsule_size, 2);
        assert_eq!(restored.size_preset_version, CURRENT_SIZE_PRESET_VERSION);
        assert!(restored.edge_hide);
        assert!(restored.dock_left);
        assert_eq!(restored.mode, Mode::Popup);
    }
}
