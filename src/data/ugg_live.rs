//! U.GG's live game lookup: the same GraphQL queries u.gg's Live Game page
//! sends (u.gg/api). A game shows up there once the loading screen starts.
//! Players who turned on Riot's anonymity come back without a name, and
//! Poro shows them that way too.

use anyhow::{anyhow, Result};
use serde::Deserialize;
use serde_json::{json, Value};

use super::client;
use super::ddragon::Lane;

/// U.GG's id for the current ranked season.
const SEASON: i64 = 26;

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RankScore {
    pub queue_type: String,
    /// "master", "gold", … (lowercase).
    pub tier: String,
    /// "i" … "iv".
    pub rank: String,
    pub lp: i64,
    pub wins: u64,
    pub losses: u64,
}

impl RankScore {
    pub fn is_ranked(&self) -> bool {
        !self.tier.is_empty() && self.tier != "unranked"
    }

    /// "Master 655 LP", "Gold II 40 LP".
    pub fn label(&self) -> String {
        if !self.is_ranked() {
            return "Unranked".into();
        }
        let tier = capitalize(&self.tier);
        if matches!(self.tier.as_str(), "master" | "grandmaster" | "challenger") {
            format!("{tier} {} LP", self.lp)
        } else {
            format!("{tier} {} {} LP", self.rank.to_uppercase(), self.lp)
        }
    }

    pub fn win_rate(&self) -> Option<f64> {
        let games = self.wins + self.losses;
        (games > 0).then(|| self.wins as f64 * 100.0 / games as f64)
    }

    /// Small rank crest from CommunityDragon (SVG: Emerald has no PNG there).
    pub fn crest(&self) -> Option<String> {
        self.is_ranked().then(|| {
            format!(
                "https://raw.communitydragon.org/latest/plugins/rcp-fe-lol-static-assets/global/default/images/ranked-mini-crests/{}.svg",
                self.tier
            )
        })
    }
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Tag {
    pub tag: String,
    /// "good", "negative" or "neutral".
    pub kind: String,
    /// "Top 20% in CS"; may contain "%champion".
    pub meta: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LivePlayer {
    /// "Name#TAG"; None for anonymous players.
    pub riot_id: Option<String>,
    pub champion: u32,
    pub ban: u32,
    pub spells: (u32, u32),
    pub primary_style: u32,
    pub sub_style: u32,
    pub keystone: u32,
    pub icon: u32,
    /// Ranked games on this champion this season.
    pub champ_wins: u64,
    pub champ_losses: u64,
    /// Totals over those games.
    pub kda: (u64, u64, u64),
    pub role: Option<Lane>,
    /// Playing their main role.
    pub on_role: bool,
    /// Players queued together share a party number.
    pub party: Option<i64>,
    pub rank: Option<RankScore>,
    pub prev_rank: Option<RankScore>,
    /// Ranked games per role: (role, games, wins), most played first.
    pub roles: Vec<(Lane, u64, u64)>,
    pub tags: Vec<Tag>,
}

impl LivePlayer {
    pub fn champ_games(&self) -> u64 {
        self.champ_wins + self.champ_losses
    }

    /// Average (K + A) / D on this champion.
    pub fn kda_ratio(&self) -> Option<f64> {
        let (k, d, a) = self.kda;
        (self.champ_games() > 0).then(|| (k + a) as f64 / d.max(1) as f64)
    }

    pub fn avg_kda(&self) -> Option<(f64, f64, f64)> {
        let g = self.champ_games() as f64;
        (g > 0.0).then(|| (self.kda.0 as f64 / g, self.kda.1 as f64 / g, self.kda.2 as f64 / g))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct LiveGame {
    pub seconds: i64,
    pub queue_id: i64,
    pub game_type: String,
    pub team_a: Vec<LivePlayer>,
    pub team_b: Vec<LivePlayer>,
}

impl LiveGame {
    /// The team with `riot_id` in it first.
    pub fn teams_for(&self, riot_id: &str) -> (&[LivePlayer], &[LivePlayer], bool) {
        let is = |p: &LivePlayer| p.riot_id.as_deref().is_some_and(|r| r.eq_ignore_ascii_case(riot_id));
        if self.team_b.iter().any(is) {
            (&self.team_b, &self.team_a, true)
        } else {
            (&self.team_a, &self.team_b, self.team_a.iter().any(is))
        }
    }
}

fn post(op: &str, variables: Value, query: &str) -> Result<Value> {
    let r: Value = client()
        .post("https://u.gg/api")
        .json(&json!({ "operationName": op, "variables": variables, "query": query }))
        .send()?
        .error_for_status()?
        .json()?;
    Ok(r)
}

fn errors(r: &Value) -> String {
    r.get("errors")
        .and_then(Value::as_array)
        .and_then(|e| e.first())
        .and_then(|e| e.get("message"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

pub fn lane_of(role: &str) -> Option<Lane> {
    match role {
        "top" => Some(Lane::Top),
        "jungle" => Some(Lane::Jungle),
        "mid" => Some(Lane::Middle),
        "adc" => Some(Lane::Bottom),
        "supp" | "support" => Some(Lane::Support),
        _ => None,
    }
}

const RANK_FIELDS: &str = "lastUpdatedAt losses lp queueType rank role seasonId tier wins";

/// A player's solo/duo rank this season (None if unranked or not found).
pub fn profile_rank(region: &str, name: &str, tag: &str) -> Result<Option<RankScore>> {
    let query = format!(
        "query fetchProfileRanks($riotUserName: String!, $riotTagLine: String!, $regionId: String!, $seasonId: Int!) {{
  fetchProfileRanks(riotUserName: $riotUserName, riotTagLine: $riotTagLine, regionId: $regionId, seasonId: $seasonId) {{
    rankScores {{ {RANK_FIELDS} }}
  }}
}}"
    );
    let r = post(
        "fetchProfileRanks",
        json!({ "riotUserName": name, "riotTagLine": tag, "regionId": region, "seasonId": SEASON }),
        &query,
    )?;
    let scores: Vec<RankScore> = r
        .pointer("/data/fetchProfileRanks/rankScores")
        .cloned()
        .map(serde_json::from_value)
        .transpose()?
        .unwrap_or_default();
    Ok(scores.into_iter().find(|s| s.queue_type == "ranked_solo_5x5" && s.is_ranked()))
}

/// The game `name#tag` is in right now, or None if U.GG doesn't see one
/// (not in a game, or it hasn't reached the loading screen yet).
pub fn live_game(region: &str, name: &str, tag: &str) -> Result<Option<LiveGame>> {
    let player = format!(
        "banId championId championLosses championWins championStats {{ kills deaths assists }}
      currentRole onRole partyNumber currentSeasonRankScore {{ {RANK_FIELDS} }} previousSeasonRankScore {{ {RANK_FIELDS} }}
      roleDatas {{ games roleName wins }} isAnonEnabled summonerIconId riotUserName riotTagLine
      summonerRuneA summonerRuneB summonerRuneData summonerSpellA summonerSpellB threatLevel"
    );
    let query = format!(
        "query getLiveGame($regionId: String!, $riotUserName: String!, $riotTagLine: String!) {{
  getLiveGame(regionId: $regionId, riotUserName: $riotUserName, riotTagLine: $riotTagLine) {{
    gameLengthSeconds gameType queueId teamA {{ {player} }} teamB {{ {player} }}
  }}
}}"
    );
    let r = post("getLiveGame", json!({ "regionId": region, "riotUserName": name, "riotTagLine": tag }), &query)?;
    let Some(g) = r.pointer("/data/getLiveGame").filter(|g| !g.is_null()) else {
        let e = errors(&r);
        if e.is_empty() || e.contains("No live game") {
            return Ok(None);
        }
        return Err(anyhow!("{e}"));
    };
    let mut game = LiveGame {
        seconds: g.get("gameLengthSeconds").and_then(Value::as_i64).unwrap_or(0),
        queue_id: g.get("queueId").and_then(Value::as_i64).unwrap_or(0),
        game_type: g.get("gameType").and_then(Value::as_str).unwrap_or("").to_string(),
        team_a: team(g.get("teamA")),
        team_b: team(g.get("teamB")),
    };
    // Tags ("Good CS", "First Time", …) are a second query; the game shows without them.
    if let Ok(mut tags) = live_tags(region, game.queue_id, &game) {
        for p in game.team_a.iter_mut().chain(game.team_b.iter_mut()) {
            if let Some(id) = &p.riot_id {
                if let Some(t) = tags.remove(&id.to_lowercase()) {
                    p.tags = t;
                }
            }
        }
    }
    Ok(Some(game))
}

fn team(v: Option<&Value>) -> Vec<LivePlayer> {
    let mut out: Vec<LivePlayer> = v.and_then(Value::as_array).into_iter().flatten().map(player).collect();
    // U.GG's order: top, jungle, mid, adc, support.
    let order = |p: &LivePlayer| match p.role {
        Some(Lane::Top) => 0,
        Some(Lane::Jungle) => 1,
        Some(Lane::Middle) => 2,
        Some(Lane::Bottom) => 3,
        Some(Lane::Support) => 4,
        None => 5,
    };
    out.sort_by_key(order);
    out
}

fn player(p: &Value) -> LivePlayer {
    let u = |k: &str| p.get(k).and_then(Value::as_u64).unwrap_or(0);
    let s = |k: &str| p.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    let rank = |k: &str| p.get(k).cloned().and_then(|v| serde_json::from_value::<RankScore>(v).ok()).filter(RankScore::is_ranked);
    let (name, tag) = (s("riotUserName"), s("riotTagLine"));
    let anon = p.get("isAnonEnabled").and_then(Value::as_bool).unwrap_or(false);
    let stats = p.get("championStats");
    let stat = |k: &str| stats.and_then(|s| s.get(k)).and_then(Value::as_u64).unwrap_or(0);
    let mut roles: Vec<(Lane, u64, u64)> = p
        .get("roleDatas")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|r| {
            let lane = lane_of(r.get("roleName")?.as_str()?)?;
            Some((lane, r.get("games")?.as_u64()?, r.get("wins").and_then(Value::as_u64).unwrap_or(0)))
        })
        .collect();
    roles.sort_by(|a, b| b.1.cmp(&a.1));
    LivePlayer {
        riot_id: (!anon && !name.is_empty()).then(|| format!("{name}#{tag}")),
        champion: u("championId") as u32,
        ban: u("banId") as u32,
        spells: (u("summonerSpellA") as u32, u("summonerSpellB") as u32),
        primary_style: u("summonerRuneA") as u32,
        sub_style: u("summonerRuneB") as u32,
        keystone: p.pointer("/summonerRuneData/0").and_then(Value::as_u64).unwrap_or(0) as u32,
        icon: u("summonerIconId") as u32,
        champ_wins: u("championWins"),
        champ_losses: u("championLosses"),
        kda: (stat("kills"), stat("deaths"), stat("assists")),
        role: lane_of(&s("currentRole")),
        on_role: p.get("onRole").and_then(Value::as_bool).unwrap_or(false),
        party: p.get("partyNumber").and_then(Value::as_i64),
        rank: rank("currentSeasonRankScore"),
        prev_rank: rank("previousSeasonRankScore"),
        roles,
        tags: Vec::new(),
    }
}

fn live_tags(region: &str, queue: i64, game: &LiveGame) -> Result<std::collections::HashMap<String, Vec<Tag>>> {
    let summoners: Vec<Value> = game
        .team_a
        .iter()
        .chain(game.team_b.iter())
        .filter_map(|p| {
            let (name, tag) = p.riot_id.as_deref()?.split_once('#')?;
            Some(json!({ "riotUserName": name, "riotTagLine": tag, "championId": p.champion }))
        })
        .collect();
    let query = "query liveGameTags($queueId: Int!, $regionId: String!, $summoners: [LiveGameSummoner]) {
  liveGameTags(queueId: $queueId, regionId: $regionId, summoners: $summoners) { riotUserName riotTagLine tagList { tag type category meta } }
}";
    let r = post("liveGameTags", json!({ "queueId": queue, "regionId": region, "summoners": summoners }), query)?;
    let mut out = std::collections::HashMap::new();
    for t in r.pointer("/data/liveGameTags").and_then(Value::as_array).into_iter().flatten() {
        let name = t.get("riotUserName").and_then(Value::as_str).unwrap_or("");
        let tag = t.get("riotTagLine").and_then(Value::as_str).unwrap_or("");
        let list: Vec<Tag> = t
            .get("tagList")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(|x| Tag {
                tag: x.get("tag").and_then(Value::as_str).unwrap_or("").to_string(),
                kind: x.get("type").and_then(Value::as_str).unwrap_or("neutral").to_string(),
                meta: x.get("meta").and_then(Value::as_str).unwrap_or("").to_string(),
            })
            .filter(|t| !t.tag.is_empty())
            .collect();
        out.insert(format!("{name}#{tag}").to_lowercase(), list);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    #[test]
    fn live_profile_rank() {
        let r = super::profile_rank("na1", "Doublelift", "NA1").unwrap();
        println!("{r:?}");
    }
}
