//! The running game's live data, from League's official Live Client Data API
//! (https://127.0.0.1:2999/liveclientdata/allgamedata, served by the game
//! itself while a match is on). Only things the player can already see in
//! the game: their own stats, the scoreboard (champions, items, KDA, CS).

use std::time::Duration;

use anyhow::Result;
use serde::Deserialize;

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameData {
    #[serde(default)]
    pub active_player: ActivePlayer,
    #[serde(default)]
    pub all_players: Vec<Player>,
    #[serde(default)]
    pub game_data: GameInfo,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivePlayer {
    #[serde(default)]
    pub riot_id: String,
    #[serde(default)]
    pub summoner_name: String,
    #[serde(default)]
    pub level: u32,
    #[serde(default)]
    pub current_gold: f64,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub struct Player {
    #[serde(default)]
    pub riot_id: String,
    #[serde(default)]
    pub summoner_name: String,
    /// "game_character_displayname_MonkeyKing": the Data Dragon id follows the prefix.
    #[serde(default)]
    pub raw_champion_name: String,
    #[serde(default)]
    pub champion_name: String,
    /// "ORDER" (blue) or "CHAOS" (red).
    #[serde(default)]
    pub team: String,
    /// "TOP", "JUNGLE", "MIDDLE", "BOTTOM", "UTILITY" (empty in some modes).
    #[serde(default)]
    pub position: String,
    #[serde(default)]
    pub level: u32,
    #[serde(default)]
    pub items: Vec<Item>,
    #[serde(default)]
    pub scores: Scores,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    #[serde(rename = "itemID", default)]
    pub item_id: u32,
    #[serde(default)]
    pub count: u32,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Scores {
    #[serde(default)]
    pub kills: u32,
    #[serde(default)]
    pub deaths: u32,
    #[serde(default)]
    pub assists: u32,
    #[serde(default)]
    pub creep_score: u32,
    #[serde(default)]
    pub ward_score: f64,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub struct GameInfo {
    #[serde(default)]
    pub game_mode: String,
    #[serde(default)]
    pub game_time: f64,
    #[serde(default)]
    pub map_number: u32,
}

fn client() -> &'static reqwest::blocking::Client {
    static C: std::sync::OnceLock<reqwest::blocking::Client> = std::sync::OnceLock::new();
    C.get_or_init(|| {
        // The game serves this on 127.0.0.1 with Riot's self-signed certificate.
        reqwest::blocking::Client::builder()
            .danger_accept_invalid_certs(true)
            .timeout(Duration::from_millis(1500))
            .build()
            .expect("http client")
    })
}

/// The current game's data, or an error when no game is running.
pub fn fetch() -> Result<GameData> {
    let resp = client().get("https://127.0.0.1:2999/liveclientdata/allgamedata").send()?.error_for_status()?;
    Ok(resp.json()?)
}

impl Player {
    /// Data Dragon id ("MonkeyKing").
    pub fn champion_id(&self) -> &str {
        self.raw_champion_name.rsplit('_').next().unwrap_or(&self.champion_name)
    }

    fn is(&self, me: &ActivePlayer) -> bool {
        (!me.riot_id.is_empty() && self.riot_id == me.riot_id)
            || (!me.summoner_name.is_empty() && self.summoner_name == me.summoner_name)
    }
}

impl GameData {
    pub fn me(&self) -> Option<&Player> {
        self.all_players.iter().find(|p| p.is(&self.active_player))
    }

    /// The five lanes paired up: (lane label, ally, enemy). Uses positions
    /// when the game reports them, otherwise scoreboard order.
    pub fn lanes(&self) -> Vec<(&'static str, Option<&Player>, Option<&Player>)> {
        let Some(me) = self.me() else { return Vec::new() };
        let mine: Vec<&Player> = self.all_players.iter().filter(|p| p.team == me.team).collect();
        let theirs: Vec<&Player> = self.all_players.iter().filter(|p| p.team != me.team).collect();
        const LANES: [(&str, &str); 5] =
            [("TOP", "TOP"), ("JUNGLE", "JNG"), ("MIDDLE", "MID"), ("BOTTOM", "BOT"), ("UTILITY", "SUP")];
        // Practice Tool and some modes report "NONE": use scoreboard order then.
        let known = |p: &Player| !p.position.is_empty() && p.position != "NONE";
        let have_positions = self.all_players.iter().all(known);
        LANES
            .iter()
            .enumerate()
            .map(|(i, (pos, label))| {
                if have_positions {
                    (
                        *label,
                        mine.iter().find(|p| p.position == *pos).copied(),
                        theirs.iter().find(|p| p.position == *pos).copied(),
                    )
                } else {
                    (*label, mine.get(i).copied(), theirs.get(i).copied())
                }
            })
            .collect()
    }

    /// My lane opponent.
    pub fn opponent(&self) -> Option<&Player> {
        let me = self.me()?;
        self.lanes().into_iter().find(|(_, a, _)| a.is_some_and(|a| std::ptr::eq(a, me))).and_then(|(_, _, e)| e)
    }

    /// Kills, deaths, assists summed for my team (for kill participation).
    pub fn team_kills(&self) -> u32 {
        let Some(me) = self.me() else { return 0 };
        self.all_players.iter().filter(|p| p.team == me.team).map(|p| p.scores.kills).sum()
    }
}
