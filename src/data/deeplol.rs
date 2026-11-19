//! deeplol.gg's own API (what deeplol.gg/llm, the Champion Matchup page, calls).
//! Champion stats are from Korean ranked, Emerald and above.

use std::collections::HashMap;

use anyhow::Result;
use serde::Deserialize;

use super::ddragon::Lane;
use super::{get_json, LIVE, STATIC};

const API: &str = "https://b2c-api-cdn.deeplol.gg";

#[derive(Clone, Debug, Deserialize)]
#[allow(dead_code)]
pub struct Versions {
    /// The live patch, e.g. "16.23" (usually has no stats yet).
    pub recent_version: String,
    /// Patches with stats, newest first.
    pub game_version_list: Vec<String>,
}

pub fn versions() -> Result<Versions> {
    get_json(&format!("{API}/champion/version?cnt=8"), LIVE)
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct LaneStats {
    #[serde(default, deserialize_with = "opt_int")]
    pub champion_tier: Option<u64>,
    #[serde(default, deserialize_with = "opt_int")]
    pub rank: Option<u64>,
    #[serde(default, deserialize_with = "opt_int")]
    pub lane_cnt_all: Option<u64>,
    pub win_rate: Option<f64>,
    pub pick_rate: Option<f64>,
    pub ban_rate: Option<f64>,
    #[serde(default, deserialize_with = "opt_int")]
    pub games: Option<u64>,
    #[serde(default)]
    pub match_up: MatchUps,
    #[serde(default)]
    pub build_lst: Vec<Build>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct MatchUps {
    #[serde(default)]
    pub strong_against: Vec<MatchUp>,
    #[serde(default)]
    pub weak_against: Vec<MatchUp>,
}

impl MatchUps {
    pub fn find(&self, enemy: u32) -> Option<&MatchUp> {
        self.strong_against.iter().chain(&self.weak_against).find(|m| m.enemy_champion_id == enemy)
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct MatchUp {
    pub enemy_champion_id: u32,
    #[serde(deserialize_with = "int")]
    pub games: u64,
    pub win_rate: f64,
}

/// deeplol sometimes sends whole numbers as floats ("rank": 29.0).
fn opt_int<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<u64>, D::Error> {
    Ok(Option::<f64>::deserialize(d)?.map(|v| v.max(0.0).round() as u64))
}

fn int<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u64, D::Error> {
    Ok(opt_int(d)?.unwrap_or(0))
}

/// One of the champion's popular builds.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct Build {
    #[serde(default)]
    pub rune: BuildRune,
    #[serde(default)]
    pub item: BuildItems,
    #[serde(default)]
    pub spell: BuildList,
    #[serde(default)]
    pub start_item: BuildList,
    #[serde(default)]
    pub skill: BuildSkill,
    #[serde(default)]
    pub win_rate: f64,
    #[serde(default)]
    pub pick_rate: f64,
    #[serde(default, deserialize_with = "int")]
    pub games: u64,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct BuildRune {
    #[serde(default)]
    pub main_build: Vec<u32>,
    #[serde(default)]
    pub sub_build: Vec<u32>,
    #[serde(default)]
    pub stat_build: Vec<u32>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct BuildItems {
    #[serde(default)]
    pub build: Vec<u32>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct BuildList {
    #[serde(default)]
    pub build: Vec<u32>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[allow(dead_code)]
pub struct BuildSkill {
    /// Max order, e.g. [1, 2, 3] = Q > W > E.
    #[serde(default)]
    pub build: Vec<u32>,
    /// Level 1..15 skill ups (1 = Q .. 4 = R).
    #[serde(default)]
    pub detail: Vec<u32>,
}

impl Build {
    /// The rune page: [primary tree, keystone + 3, secondary tree, 2, 3 shards].
    pub fn runes(&self) -> Option<RunePage> {
        let main = &self.rune.main_build;
        let sub = &self.rune.sub_build;
        let stat = &self.rune.stat_build;
        if main.len() != 5 || sub.len() != 3 || stat.len() != 3 {
            return None;
        }
        Some(RunePage {
            primary_style: main[0],
            sub_style: sub[0],
            perks: with_shards([main[1], main[2], main[3], main[4], sub[1], sub[2]], [stat[0], stat[1], stat[2]]),
        })
    }
}

/// The League client's stat shard rows, top to bottom (offense, flex,
/// defense), as `/lol-perks/v1/styles` reports them.
const SHARD_ROWS: [&[u32]; 3] = [&[5008, 5005, 5007], &[5008, 5010, 5001], &[5011, 5013, 5001]];

/// Puts deeplol's stat shards in the order the League client wants.
/// deeplol lists them bottom-up (defense, flex, offense, like Riot's match
/// data), while the client wants top-down; sending them as-is put e.g.
/// Health in the offense row and made the page invalid. Tries the reversed
/// order first, then any order that fits every row.
pub fn order_shards(shards: [u32; 3]) -> [u32; 3] {
    let [a, b, c] = shards;
    let fits = |o: &[u32; 3]| o.iter().zip(SHARD_ROWS).all(|(s, row)| row.contains(s));
    let candidates = [[c, b, a], [a, b, c], [a, c, b], [b, a, c], [b, c, a], [c, a, b]];
    candidates.into_iter().find(|o| fits(o)).unwrap_or([c, b, a])
}

fn with_shards(runes: [u32; 6], shards: [u32; 3]) -> [u32; 9] {
    let s = order_shards(shards);
    [runes[0], runes[1], runes[2], runes[3], runes[4], runes[5], s[0], s[1], s[2]]
}

/// A complete rune page, in the order the League client wants it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct RunePage {
    pub primary_style: u32,
    pub sub_style: u32,
    /// Keystone, 3 primary runes, 2 secondary runes, 3 stat shards.
    pub perks: [u32; 9],
}

#[derive(Deserialize)]
struct BuildFile {
    build_by_lane: HashMap<String, LaneStats>,
}

/// A champion's stats on `patch` ("16.22"), by lane.
pub fn champion(champion: u32, patch: &str, live: bool) -> Result<HashMap<Lane, LaneStats>> {
    let url = format!("{API}/champion/build?platform_id=KR&champion_id={champion}&game_version={patch}&tier=Emerald%2B");
    let file: BuildFile = get_json(&url, if live { LIVE } else { STATIC })?;
    Ok(file.build_by_lane.into_iter().filter_map(|(k, v)| Some((Lane::from_deeplol(&k)?, v))).collect())
}

#[derive(Clone, Debug, Deserialize)]
pub struct HeadToHead {
    #[serde(deserialize_with = "int")]
    pub games: u64,
    pub my_win_rate: f64,
    pub enemy_win_rate: f64,
}

/// Head-to-head stats for `champion` vs `enemy`, by lane. deeplol only has
/// these for its current data window, not per patch.
pub fn head_to_head(champion: u32, enemy: u32) -> Result<HashMap<Lane, HeadToHead>> {
    #[derive(Deserialize)]
    struct File {
        stats_by_position: HashMap<String, HeadToHead>,
    }
    let f: File = get_json(&format!("{API}/matchup/matchup_stats?champion_id={champion}&enemy_champion_id={enemy}"), LIVE)?;
    Ok(f.stats_by_position.into_iter().filter_map(|(k, v)| Some((Lane::from_deeplol(&k)?, v))).collect())
}

#[derive(Clone, Debug, Default)]
pub struct Tips {
    /// Levels 1–6: Some(true) = ahead, Some(false) = behind, None = even.
    pub levels: Vec<Option<bool>>,
    pub tips: Vec<(String, String)>,
}

pub fn tips(champion: u32, enemy: u32) -> Result<Tips> {
    #[derive(Deserialize)]
    struct File {
        // deeplol sends null for both when it has no tips for a matchup.
        #[serde(rename = "levelAdvantage", default)]
        level_advantage: Option<HashMap<String, String>>,
        #[serde(default)]
        tips: Option<Vec<Tip>>,
    }
    #[derive(Deserialize)]
    struct Tip {
        title: String,
        tip: String,
    }
    let f: File = get_json(
        &format!("{API}/matchup/matchup_tips?champion_id={champion}&enemy_champion_id={enemy}&language=en_US"),
        LIVE,
    )?;
    let level_advantage = f.level_advantage.unwrap_or_default();
    if level_advantage.is_empty() && f.tips.as_ref().is_none_or(|t| t.is_empty()) {
        anyhow::bail!("deeplol has no tips for this matchup yet");
    }
    let levels = (1..=6)
        .map(|lv| match level_advantage.get(&format!("lv{lv}")).map(String::as_str) {
            // deeplol sends these in Korean: 유리 = favoured, 불리 = unfavoured.
            Some("유리") => Some(true),
            Some("불리") => Some(false),
            _ => None,
        })
        .collect();
    Ok(Tips { levels, tips: f.tips.unwrap_or_default().into_iter().map(|t| (t.title, t.tip)).collect() })
}

/// A real game of the matchup by a pro or high-elo player.
#[derive(Clone, Debug, Deserialize)]
pub struct Game {
    pub match_id: String,
    pub champion_id: u32,
    pub enemy_champion_id: u32,
    #[serde(default)]
    pub ai_score: f64,
    #[serde(default)]
    pub game_end_time: i64,
    /// The pro's name when this is a pro player's game.
    #[serde(default)]
    pub championship_name: String,
    #[serde(default)]
    pub player_name: String,
    /// "pro", "streamer" or empty.
    #[serde(default)]
    pub pro_streamer_status: String,
    #[serde(default)]
    pub player_type: String,
    #[serde(default)]
    pub riot_id_name: String,
    #[serde(default)]
    pub riot_id_tag_line: String,
    #[serde(default)]
    pub tier: String,
    /// The lane played, e.g. "Middle".
    #[serde(default)]
    pub position: String,
    #[serde(default)]
    pub game_duration: u64,
    #[serde(default)]
    pub win: u8,
    #[serde(default)]
    pub kills: u32,
    #[serde(default)]
    pub deaths: u32,
    #[serde(default)]
    pub assists: u32,
    /// The patch the game was played on ("16.22"); filled in separately.
    #[serde(skip)]
    pub patch: Option<String>,
    pub rune: GameRune,
    pub spell: GameSpell,
    #[serde(default)]
    pub item_start: Vec<u32>,
    #[serde(default)]
    pub item_core: Vec<u32>,
    #[serde(default)]
    pub item_final: Vec<u32>,
    #[serde(default)]
    pub skill: Vec<SkillUp>,
    #[serde(default)]
    pub item_seq: Vec<ItemEvent>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct GameRune {
    pub perk_0: u32,
    pub perk_1: u32,
    pub perk_2: u32,
    pub perk_3: u32,
    pub perk_4: u32,
    pub perk_5: u32,
    pub perk_primary_style: u32,
    pub perk_sub_style: u32,
    pub stat_perk_0: u32,
    pub stat_perk_1: u32,
    pub stat_perk_2: u32,
}

#[derive(Clone, Debug, Deserialize)]
pub struct GameSpell {
    pub spell_1: u32,
    pub spell_2: u32,
}

#[derive(Clone, Debug, Deserialize)]
#[allow(dead_code)]
pub struct SkillUp {
    pub minute: u32,
    pub skill_id: u32,
    #[serde(rename = "type", default)]
    pub kind: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ItemEvent {
    pub minute: u32,
    pub item_id: u32,
    #[serde(rename = "type", default)]
    pub kind: String,
}

impl Game {
    pub fn lane(&self) -> Option<Lane> {
        Lane::from_deeplol(self.position.rsplit('_').next().unwrap_or(""))
    }

    pub fn is_otp(&self) -> bool {
        self.player_type == "OTP"
    }

    pub fn is_streamer(&self) -> bool {
        self.pro_streamer_status == "streamer"
    }

    pub fn is_pro(&self) -> bool {
        self.pro_streamer_status == "pro"
            || (self.pro_streamer_status.is_empty() && (named(&self.championship_name) || named(&self.player_name)))
    }

    pub fn display_name(&self) -> String {
        if named(&self.championship_name) {
            self.championship_name.clone()
        } else if named(&self.player_name) {
            self.player_name.clone()
        } else {
            format!("{}#{}", self.riot_id_name, self.riot_id_tag_line)
        }
    }

    pub fn runes(&self) -> RunePage {
        let r = &self.rune;
        RunePage {
            primary_style: r.perk_primary_style,
            sub_style: r.perk_sub_style,
            perks: with_shards([r.perk_0, r.perk_1, r.perk_2, r.perk_3, r.perk_4, r.perk_5], [r.stat_perk_0, r.stat_perk_1, r.stat_perk_2]),
        }
    }

    /// Skill order letters for each level (evolves and the like are left out).
    pub fn skill_order(&self) -> Vec<u32> {
        self.skill.iter().filter(|s| s.kind == "NORMAL" || s.kind.is_empty()).map(|s| s.skill_id).collect()
    }

    /// Items bought (not sold or undone), grouped by minute.
    pub fn purchases(&self) -> Vec<(u32, Vec<u32>)> {
        let mut out: Vec<(u32, Vec<u32>)> = Vec::new();
        for e in self.item_seq.iter().filter(|e| e.kind == "ITEM_PURCHASED") {
            match out.last_mut() {
                Some((m, items)) if *m == e.minute => items.push(e.item_id),
                _ => out.push((e.minute, vec![e.item_id])),
            }
        }
        out
    }
}

/// deeplol uses "" or "-" for "no name".
fn named(s: &str) -> bool {
    !s.trim().is_empty() && s.trim() != "-"
}

/// Recent pro and high-elo games of the matchup, 10 per page (deeplol's
/// latest data; it has no patch filter).
/// The patch a game was played on ("16.22"). The game list doesn't say, so
/// this reads the game's match details (about 150 KB) and keeps only the
/// patch number on disk; a game's patch never changes.
pub fn game_patch(match_id: &str) -> Result<String> {
    let dir = super::cache_dir().join("patches");
    let path = dir.join(match_id.replace(|c: char| !c.is_ascii_alphanumeric() && c != '_', "_"));
    if let Ok(v) = std::fs::read_to_string(&path) {
        if !v.trim().is_empty() {
            return Ok(v.trim().to_string());
        }
    }
    let platform = match_id.split('_').next().unwrap_or("KR");
    let bytes = super::fetch(&format!("{API}/match/match-cached?match_id={match_id}&platform_id={platform}"))?;
    let v: serde_json::Value = serde_json::from_slice(&bytes)?;
    let version = v
        .pointer("/match_basic_dict/game_version")
        .and_then(|x| x.as_str())
        .ok_or_else(|| anyhow::anyhow!("no patch in match {match_id}"))?;
    // "16.22.123.4567" -> "16.22"
    let patch: String = version.split('.').take(2).collect::<Vec<_>>().join(".");
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::write(&path, &patch);
    Ok(patch)
}

/// Which of deeplol's game lists: "ALL" (high-elo players and one-tricks)
/// or "Pro" (pro players' games, a separate list).
pub fn games(champion: u32, enemy: u32, page: u32, player_type: &str) -> Result<Vec<Game>> {
    #[derive(Deserialize)]
    struct File {
        #[serde(default)]
        match_up_list: Vec<Game>,
    }
    let f: File = get_json(
        &format!("{API}/matchup/OTP_match?champion_id={champion}&enemy_champion_id={enemy}&page={page}&player_type={player_type}"),
        LIVE,
    )?;
    Ok(f.match_up_list)
}

#[cfg(test)]
mod tests {
    #[test]
    fn shards_go_top_down() {
        // The broken import: deeplol's [Health, Adaptive, Adaptive].
        assert_eq!(super::order_shards([5011, 5008, 5008]), [5008, 5008, 5011]);
        // Orianna's general build [Scaling Health, Adaptive, Attack Speed].
        assert_eq!(super::order_shards([5001, 5008, 5005]), [5005, 5008, 5001]);
        // Already top-down stays as it is.
        assert_eq!(super::order_shards([5005, 5010, 5013]), [5005, 5010, 5013]);
    }

    #[test]
    fn live_champion() {
        let r = super::champion(61, "16.22", true);
        if let Err(e) = &r { println!("ERR {e:#}"); }
        r.unwrap();
    }
}
