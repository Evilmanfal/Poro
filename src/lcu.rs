//! Talks to the League client running on this PC through its local API
//! (the "LCU" API that rune importers use). Only used in the client, never
//! in a game.
//!
//! The client writes its port and password to a `lockfile` in the League
//! install folder while it runs. The API uses a self-signed certificate on
//! 127.0.0.1, so that one connection skips certificate checks.

use std::path::PathBuf;
use std::time::Duration;

use anyhow::{anyhow, bail, Context as _, Result};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::data::ddragon::Lane;
use crate::data::deeplol::RunePage;

pub struct Lcu {
    base: String,
    password: String,
    http: reqwest::blocking::Client,
}

/// What the app needs from champ select.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ChampSelect {
    /// Hovered or locked champion (0 = none yet).
    pub my_champion: u32,
    pub my_lane: Option<Lane>,
    /// The enemy in my lane, once they're visible.
    pub lane_opponent: Option<u32>,
    pub enemies: Vec<u32>,
}

impl Lcu {
    /// Connects to the running League client, if there is one.
    pub fn connect() -> Result<Lcu> {
        let (port, password) = find_credentials().ok_or_else(|| anyhow!("League client isn't running"))?;
        let http = reqwest::blocking::Client::builder()
            .danger_accept_invalid_certs(true)
            .timeout(Duration::from_secs(5))
            .build()?;
        let lcu = Lcu { base: format!("https://127.0.0.1:{port}"), password, http };
        lcu.get("/lol-summoner/v1/current-summoner").context("League client isn't signed in")?;
        Ok(lcu)
    }

    fn request(&self, method: reqwest::Method, path: &str, body: Option<Value>) -> Result<Option<Value>> {
        let mut req = self.http.request(method, format!("{}{path}", self.base)).basic_auth("riot", Some(&self.password));
        if let Some(b) = body {
            req = req.json(&b);
        }
        let resp = req.send().with_context(|| format!("League client: {path}"))?;
        let status = resp.status();
        let text = resp.text().unwrap_or_default();
        if !status.is_success() {
            let msg = serde_json::from_str::<Value>(&text)
                .ok()
                .and_then(|v| v.get("message").and_then(Value::as_str).map(str::to_string))
                .unwrap_or(text);
            bail!("League client said {status}: {msg}");
        }
        Ok(if text.is_empty() { None } else { serde_json::from_str(&text).ok() })
    }

    /// Whether the client is still there and signed in.
    pub fn alive(&self) -> bool {
        self.get("/lol-summoner/v1/current-summoner").is_ok()
    }

    fn get(&self, path: &str) -> Result<Value> {
        self.request(reqwest::Method::GET, path, None)?.ok_or_else(|| anyhow!("empty reply from {path}"))
    }

    /// The current champ select, or None outside of one.
    pub fn champ_select(&self) -> Option<ChampSelect> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Session {
            local_player_cell_id: i64,
            #[serde(default)]
            my_team: Vec<Member>,
            #[serde(default)]
            their_team: Vec<Member>,
        }
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Member {
            cell_id: i64,
            #[serde(default)]
            champion_id: u32,
            #[serde(default)]
            champion_pick_intent: u32,
            #[serde(default)]
            assigned_position: String,
        }
        let s: Session = serde_json::from_value(self.get("/lol-champ-select/v1/session").ok()?).ok()?;
        let me = s.my_team.iter().find(|m| m.cell_id == s.local_player_cell_id)?;
        let my_lane = Lane::from_client(&me.assigned_position);
        let my_champion = if me.champion_id != 0 { me.champion_id } else { me.champion_pick_intent };
        let lane_opponent = my_lane.and_then(|lane| {
            s.their_team
                .iter()
                .find(|m| Lane::from_client(&m.assigned_position) == Some(lane) && m.champion_id != 0)
                .map(|m| m.champion_id)
        });
        let enemies = s.their_team.iter().map(|m| m.champion_id).filter(|&c| c != 0).collect();
        Some(ChampSelect { my_champion, my_lane, lane_opponent, enemies })
    }

    /// Creates `page` as the active rune page, named `name`. Reuses the
    /// slot of an earlier page from this app; if every slot is taken,
    /// replaces the current (editable) page.
    pub fn apply_runes(&self, name: &str, page: &RunePage) -> Result<()> {
        let pages = self.get("/lol-perks/v1/pages")?;
        let pages = pages.as_array().cloned().unwrap_or_default();
        let deletable = |p: &Value| p.get("isDeletable").and_then(Value::as_bool).unwrap_or(false);
        let id_of = |p: &Value| p.get("id").and_then(Value::as_i64);

        let mut editable = pages.iter().filter(|p| deletable(p)).count();
        for p in pages.iter().filter(|p| deletable(p)) {
            let ours = p
                .get("name")
                .and_then(Value::as_str)
                .is_some_and(|n| n.starts_with(PAGE_PREFIX) || n.starts_with(OLD_PAGE_PREFIX));
            if let (true, Some(id)) = (ours, id_of(p)) {
                self.request(reqwest::Method::DELETE, &format!("/lol-perks/v1/pages/{id}"), None)?;
                editable -= 1;
            }
        }

        let owned = self
            .get("/lol-perks/v1/inventory")
            .ok()
            .and_then(|v| v.get("ownedPageCount").and_then(Value::as_u64))
            .unwrap_or(2) as usize;
        if editable >= owned {
            let current = self.get("/lol-perks/v1/currentpage").ok();
            let victim = current
                .filter(|p| deletable(p))
                .or_else(|| pages.iter().find(|p| deletable(p)).cloned())
                .and_then(|p| id_of(&p))
                .ok_or_else(|| anyhow!("no rune page slot is free"))?;
            self.request(reqwest::Method::DELETE, &format!("/lol-perks/v1/pages/{victim}"), None)?;
        }

        let name: String = format!("{PAGE_PREFIX}{name}").chars().take(MAX_PAGE_NAME).collect();
        self.request(
            reqwest::Method::POST,
            "/lol-perks/v1/pages",
            Some(json!({
                "name": name,
                "primaryStyleId": page.primary_style,
                "subStyleId": page.sub_style,
                "selectedPerkIds": page.perks,
                "current": true,
            })),
        )?;
        Ok(())
    }

    /// Sets summoner spells (champ select only).
    pub fn apply_spells(&self, spell1: u32, spell2: u32) -> Result<()> {
        self.request(
            reqwest::Method::PATCH,
            "/lol-champ-select/v1/session/my-selection",
            Some(json!({ "spell1Id": spell1, "spell2Id": spell2 })),
        )?;
        Ok(())
    }
}

/// Rune pages made by this app start with this, so the next import reuses
/// their slot instead of touching the player's own pages.
const PAGE_PREFIX: &str = "Poro: ";
/// What pages were called before the app was renamed.
const OLD_PAGE_PREFIX: &str = "MU: ";
const MAX_PAGE_NAME: usize = 30;

/// (port, password) of the running client.
fn find_credentials() -> Option<(u16, String)> {
    for dir in install_dirs() {
        if let Ok(text) = std::fs::read_to_string(dir.join("lockfile")) {
            // LeagueClient:<pid>:<port>:<password>:https
            let parts: Vec<&str> = text.trim().split(':').collect();
            if parts.len() >= 5 {
                if let Ok(port) = parts[2].parse() {
                    return Some((port, parts[3].to_string()));
                }
            }
        }
    }
    from_process_command_line()
}

fn install_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    // Riot's record of where League is installed.
    let settings = PathBuf::from(r"C:\ProgramData\Riot Games\Metadata\league_of_legends.live\league_of_legends.live.product_settings.yaml");
    if let Ok(text) = std::fs::read_to_string(settings) {
        for line in text.lines() {
            if let Some(v) = line.trim().strip_prefix("product_install_full_path:") {
                dirs.push(PathBuf::from(v.trim().trim_matches('"').replace("\\\\", "\\")));
            }
        }
    }
    for drive in ["C", "D", "E", "F"] {
        dirs.push(PathBuf::from(format!(r"{drive}:\Riot Games\League of Legends")));
    }
    dirs
}

/// Fallback: read the port and password from LeagueClientUx.exe's arguments.
#[cfg(windows)]
fn from_process_command_line() -> Option<(u16, String)> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let out = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            "(Get-CimInstance Win32_Process -Filter \"name='LeagueClientUx.exe'\").CommandLine",
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    let cmd = String::from_utf8_lossy(&out.stdout);
    let arg = |key: &str| {
        let start = cmd.find(key)? + key.len();
        Some(cmd[start..].split(['"', ' ']).next()?.to_string())
    };
    Some((arg("--app-port=")?.parse().ok()?, arg("--remoting-auth-token=")?))
}

#[cfg(not(windows))]
fn from_process_command_line() -> Option<(u16, String)> {
    None
}
