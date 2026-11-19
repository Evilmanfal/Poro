//! Where League draws its HUD, so overlays can sit next to it. Reads the
//! game's own settings (Config\game.cfg: resolution, HUD scale, minimap
//! scale and side) and scales positions measured on a 1920×1080 screen with
//! HUD scale 0.21 and minimap scale 0.94.
//!
//! All results are in game-window pixels (physical), origin top-left.

use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hud {
    /// The game window's size in pixels.
    pub width: f32,
    pub height: f32,
    /// HUD scale, 0–1 (League › Settings › Interface › HUD Scale / 100).
    pub global_scale: f32,
    /// Minimap scale (League › Settings › Interface › Minimap Scale / 100... ×3).
    pub minimap_scale: f32,
    /// Minimap on the left.
    pub flip_minimap: bool,
    /// The scoreboard's teams are swapped (League › Interface).
    pub mirrored_scoreboard: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// The reference the positions were measured at.
const REF_H: f32 = 1080.0;
const REF_HUD: f32 = 0.21;
const REF_MINIMAP: f32 = 0.94;

impl Hud {
    /// Reads game.cfg; `size` is the game window's size in pixels.
    pub fn read(size: (f32, f32)) -> Hud {
        let mut hud = Hud {
            width: size.0,
            height: size.1,
            global_scale: REF_HUD,
            minimap_scale: REF_MINIMAP,
            flip_minimap: false,
            mirrored_scoreboard: false,
        };
        if let Some(text) = config_path().and_then(|p| std::fs::read_to_string(p).ok()) {
            let mut section = String::new();
            for line in text.lines().map(str::trim) {
                if line.starts_with('[') {
                    section = line.to_string();
                    continue;
                }
                let Some((k, v)) = line.split_once('=') else { continue };
                if section != "[HUD]" {
                    continue;
                }
                match k {
                    "GlobalScale" => hud.global_scale = v.parse().unwrap_or(REF_HUD),
                    "MinimapScale" => hud.minimap_scale = v.parse().unwrap_or(REF_MINIMAP),
                    "FlipMiniMap" => hud.flip_minimap = v == "1",
                    "MirroredScoreboard" => hud.mirrored_scoreboard = v == "1",
                    _ => {}
                }
            }
        }
        hud
    }

    /// Size factor for HUD elements: screen height, then HUD scale. League's
    /// HUD grows roughly half again as big from scale 0 to 1.
    pub fn s(&self) -> f32 {
        (self.height / REF_H) * (1.0 + 0.5 * self.global_scale) / (1.0 + 0.5 * REF_HUD)
    }

    /// The minimap's square, including its frame.
    pub fn minimap(&self) -> Rect {
        let k = self.height / REF_H;
        let side = 250.0 * k * (self.minimap_scale / REF_MINIMAP);
        let (margin_x, margin_y) = (15.0 * k, 12.0 * k);
        let x = if self.flip_minimap { margin_x } else { self.width - margin_x - side };
        Rect { x, y: self.height - margin_y - side, w: side, h: side }
    }

    /// The minimap's map area (inside the frame).
    pub fn map_area(&self) -> Rect {
        let m = self.minimap();
        let pad = m.w * 0.03;
        Rect { x: m.x + pad, y: m.y + pad, w: m.w - 2.0 * pad, h: m.h - 2.0 * pad }
    }

    /// Screen position of a Summoner's Rift map coordinate on the minimap.
    pub fn map_point(&self, world_x: f32, world_y: f32) -> (f32, f32) {
        const MAP: f32 = 14870.0;
        let a = self.map_area();
        (a.x + world_x / MAP * a.w, a.y + a.h - world_y / MAP * a.h)
    }

    /// The left edge of the bottom-center HUD (the stats panel left of the
    /// champion portrait) and the HUD's bottom.
    pub fn hud_left(&self) -> (f32, f32) {
        let s = self.s();
        (self.width / 2.0 - 445.0 * s, self.height - 8.0 * s)
    }
}

impl Hud {
    /// Where to put a number just under a player's K/D/A on the Tab
    /// scoreboard: the centre of the spot, for the team shown on the `left`
    /// half, `row` 0–4 (top, jungle, mid, bot, support).
    pub fn scoreboard_kda(&self, left: bool, row: usize) -> (f32, f32) {
        let s = self.s();
        let g = SCOREBOARD;
        let x = self.width / 2.0 + if left { -g.kda_x } else { g.kda_x } * s;
        let y = self.height / 2.0 + (g.first_row_y + row as f32 * g.row_h + g.below_kda) * s;
        (x, y)
    }
}

/// The Tab scoreboard's geometry at the reference size, relative to the
/// screen centre (in pixels at 1920×1080, HUD scale 0.21).
#[derive(Clone, Copy)]
pub struct ScoreboardGeometry {
    /// Horizontal distance from the centre to each team's K/D/A column.
    pub kda_x: f32,
    /// The first row's K/D/A line, relative to the screen's vertical centre.
    pub first_row_y: f32,
    /// Distance between rows.
    pub row_h: f32,
    /// How far under the K/D/A text the number goes.
    pub below_kda: f32,
}

pub const SCOREBOARD: ScoreboardGeometry = ScoreboardGeometry { kda_x: 300.0, first_row_y: -175.0, row_h: 52.0, below_kda: 16.0 };

/// League's game.cfg.
fn config_path() -> Option<PathBuf> {
    let settings = std::fs::read_to_string(
        r"C:\ProgramData\Riot Games\Metadata\league_of_legends.live\league_of_legends.live.product_settings.yaml",
    )
    .ok();
    let dir = settings
        .and_then(|t| {
            t.lines().find_map(|l| l.trim().strip_prefix("product_install_full_path:").map(|v| v.trim().trim_matches('"').replace("\\\\", "\\")))
        })
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Riot Games\League of Legends"));
    let p = dir.join("Config").join("game.cfg");
    p.exists().then_some(p)
}

/// Summoner's Rift objective pits (map coordinates).
pub const DRAGON_PIT: (f32, f32) = (9866.0, 4414.0);
pub const BARON_PIT: (f32, f32) = (5007.0, 10471.0);
