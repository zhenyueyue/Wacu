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
    pub shown: [bool; 3],
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            mode: Mode::Popup,
            saved_position: None,
            shown: [true; 3],
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
    serde_json::from_slice::<Preferences>(&data).unwrap_or_default()
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
    }
}
