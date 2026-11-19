//! Poro's settings, saved as JSON in %LOCALAPPDATA%\Poro\settings.json.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub overlays: Overlays,
    /// Closing the window keeps Poro running in the system tray.
    pub exit_to_tray: bool,
    /// Which key Flash goes on when Poro sets summoner spells.
    pub flash_key: FlashKey,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum FlashKey {
    /// Keep the build's order.
    #[default]
    Build,
    D,
    F,
}

impl FlashKey {
    /// Puts Flash (id 4) on the chosen key; spell 1 is D, spell 2 is F.
    pub fn order(self, (a, b): (u32, u32)) -> (u32, u32) {
        const FLASH: u32 = 4;
        match self {
            FlashKey::D if b == FLASH => (b, a),
            FlashKey::F if a == FLASH => (b, a),
            _ => (a, b),
        }
    }
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
