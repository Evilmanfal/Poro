//! Poro's settings, saved as JSON in %LOCALAPPDATA%\Poro\settings.json.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub overlays: Overlays,
    /// Closing the window keeps Poro running in the system tray.
    pub exit_to_tray: bool,
}

/// In-game overlays. All off until the player turns them on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Overlays {
    pub quick_stats: bool,
    pub skill_order: bool,
    pub item_build: bool,
    pub lane_gold: bool,
}

impl Overlays {
    pub fn any(&self) -> bool {
        self.quick_stats || self.skill_order || self.item_build || self.lane_gold
    }
}

fn path() -> std::path::PathBuf {
    crate::data::cache_dir().parent().map(|p| p.join("settings.json")).unwrap_or_else(|| "settings.json".into())
}

pub fn load() -> Settings {
    std::fs::read(path()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

pub fn save(s: &Settings) {
    let p = path();
    if let Some(dir) = p.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(json) = serde_json::to_vec_pretty(s) {
        let tmp = p.with_extension("tmp");
        if std::fs::write(&tmp, json).is_ok() {
            let _ = std::fs::rename(tmp, p);
        }
    }
}
