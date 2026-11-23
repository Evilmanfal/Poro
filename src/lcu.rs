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
    /// My pick is locked in.
    pub my_locked: bool,
    pub my_lane: Option<Lane>,
    /// The enemy in my lane, if the client says enemy roles (most queues don't).
    pub lane_opponent: Option<u32>,
    /// Enemy champions locked in so far.
    pub enemies: Vec<u32>,
    pub my_team: Vec<CsPlayer>,
    pub their_team: Vec<CsPlayer>,
    /// Banned champions: (mine, theirs).
    pub bans: (Vec<u32>, Vec<u32>),
    /// "PLANNING", "BAN_PICK", "FINALIZATION", …
    pub phase: String,
    pub queue_id: i64,
    pub game_id: u64,
}

/// One player in champ select.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CsPlayer {
    pub cell: i64,
    /// Locked (or, for my team, picked) champion; 0 = none.
    pub champion: u32,
    /// Hovered champion (my team only).
    pub intent: u32,
    pub locked: bool,
    pub lane: Option<Lane>,
    pub spells: (u32, u32),
    /// "Name#TAG" when the client shows it (ranked hides other players).
    pub name: Option<String>,
    pub is_me: bool,
}

/// Who's signed in: Riot ID and the U.GG region id ("na1").
#[derive(Clone, Debug, PartialEq)]
pub struct Me {
    pub game_name: String,
    pub tag_line: String,
    pub region: String,
    pub summoner_id: u64,
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
    ///
    /// Every field is read on its own with a default, so a field the client
    /// sends as null, -1 or not at all can't make the whole session unreadable
    /// (a typed parse did that once pick phase started).
    pub fn champ_select(&self) -> Option<ChampSelect> {
        let s = self.get("/lol-champ-select/v1/session").ok()?;
        let parsed = parse_champ_select(&s);
        if parsed.is_none() && s.get("myTeam").is_some() {
            // There is a session but it couldn't be read: keep it for a bug report.
            let path = crate::data::cache_dir().parent().map(|p| p.join("champselect-debug.json"));
            if let Some(path) = path {
                let _ = std::fs::write(path, serde_json::to_vec_pretty(&s).unwrap_or_default());
            }
        }
        parsed
    }

    /// "None", "Lobby", "Matchmaking", "ChampSelect", "InProgress", "EndOfGame", …
    pub fn phase(&self) -> String {
        self.get("/lol-gameflow/v1/gameflow-phase")
            .ok()
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap_or_default()
    }

    /// The signed-in player's Riot ID and region.
    pub fn me(&self) -> Result<Me> {
        let s = self.get("/lol-summoner/v1/current-summoner")?;
        let region = self
            .get("/riotclient/region-locale")
            .ok()
            .and_then(|v| v.get("region").and_then(Value::as_str).map(str::to_string))
            .unwrap_or_default();
        Ok(Me {
            game_name: s.get("gameName").and_then(Value::as_str).unwrap_or_default().to_string(),
            tag_line: s.get("tagLine").and_then(Value::as_str).unwrap_or_default().to_string(),
            region: ugg_region(&region).to_string(),
            summoner_id: s.get("summonerId").and_then(Value::as_u64).unwrap_or(0),
        })
    }

    /// Writes an item set for `champion` (shown in the in-game shop). Poro's
    /// sets are titled "Poro: …" and replace each other; the player's own
    /// sets are left alone.
    pub fn apply_item_set(&self, title: &str, champion: u32, blocks: &[(String, Vec<u32>)]) -> Result<()> {
        let me = self.me()?;
        let path = format!("/lol-item-sets/v1/item-sets/{}/sets", me.summoner_id);
        let current = self.get(&path).unwrap_or(json!({}));
        let mut sets: Vec<Value> = current
            .get("itemSets")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|s| !s.get("title").and_then(Value::as_str).is_some_and(|t| t.starts_with(PAGE_PREFIX)))
            .collect();
        let uid = format!("poro-{champion}");
        sets.push(json!({
            "title": format!("{PAGE_PREFIX}{title}"),
            "associatedChampions": [champion],
            "associatedMaps": [11],
            "blocks": blocks.iter().filter(|(_, items)| !items.is_empty()).map(|(name, items)| json!({
                "type": name,
                "items": items.iter().map(|i| json!({ "id": i.to_string(), "count": 1 })).collect::<Vec<_>>(),
            })).collect::<Vec<_>>(),
            "map": "any",
            "mode": "any",
            "preferredItemSlots": [],
            "sortrank": 0,
            "startedFrom": "blank",
            "type": "custom",
            "uid": uid,
        }));
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0);
        self.request(
            reqwest::Method::PUT,
            &path,
            Some(json!({ "accountId": me.summoner_id, "itemSets": sets, "timestamp": now })),
        )?;
        Ok(())
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

/// A number that may come as a float, a negative "none" (-1) or null: 0 then.
fn num(v: &Value, key: &str) -> u64 {
    match v.get(key) {
        Some(Value::Number(n)) => n.as_u64().or_else(|| n.as_f64().filter(|f| *f >= 0.0).map(|f| f as u64)).unwrap_or(0),
        _ => 0,
    }
}

fn text(v: &Value, key: &str) -> String {
    v.get(key).and_then(Value::as_str).unwrap_or("").to_string()
}

/// Reads `/lol-champ-select/v1/session`. None if it has no local player.
fn parse_champ_select(s: &Value) -> Option<ChampSelect> {
    let local_cell = s.get("localPlayerCellId").and_then(Value::as_i64)?;
    let list = |key: &str| s.get(key).and_then(Value::as_array).cloned().unwrap_or_default();
    let actions: Vec<Value> = list("actions").into_iter().flat_map(|a| a.as_array().cloned().unwrap_or_default()).collect();
    let cell_of = |v: &Value, key: &str| v.get(key).and_then(Value::as_i64).unwrap_or(-1);
    let locked = |cell: i64| {
        actions.iter().any(|a| cell_of(a, "actorCellId") == cell && text(a, "type") == "pick" && a.get("completed").and_then(Value::as_bool) == Some(true))
    };
    let player = |m: &Value, mine: bool| {
        let cell = cell_of(m, "cellId");
        let champion = num(m, "championId") as u32;
        let (game_name, tag_line) = (text(m, "gameName"), text(m, "tagLine"));
        let visible = !game_name.is_empty() && text(m, "nameVisibilityType") != "HIDDEN";
        // Unset spells can come as huge numbers: keep only real spell ids.
        let spell = |k: &str| u32::try_from(num(m, k)).ok().filter(|&id| id < 10_000).unwrap_or(0);
        CsPlayer {
            cell,
            champion,
            intent: num(m, "championPickIntent") as u32,
            // Enemies only show champions once they're locked in.
            locked: if mine { locked(cell) } else { champion != 0 },
            lane: Lane::from_client(&text(m, "assignedPosition")),
            spells: (spell("spell1Id"), spell("spell2Id")),
            name: visible.then(|| if tag_line.is_empty() { game_name.clone() } else { format!("{game_name}#{tag_line}") }),
            is_me: mine && cell == local_cell,
        }
    };
    let my_team: Vec<CsPlayer> = list("myTeam").iter().map(|m| player(m, true)).collect();
    let their_team: Vec<CsPlayer> = list("theirTeam").iter().map(|m| player(m, false)).collect();
    let me = my_team.iter().find(|p| p.is_me)?.clone();
    let my_champion = if me.champion != 0 { me.champion } else { me.intent };
    let lane_opponent = me.lane.and_then(|lane| their_team.iter().find(|p| p.lane == Some(lane) && p.champion != 0).map(|p| p.champion));
    let enemies = their_team.iter().map(|p| p.champion).filter(|&c| c != 0).collect();
    // Bans: from the session's lists, or from completed ban actions.
    let ban_list = |key: &str| -> Vec<u32> {
        s.pointer(&format!("/bans/{key}"))
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(|x| x.as_i64()).filter(|&x| x > 0).map(|x| x as u32).collect())
            .unwrap_or_default()
    };
    let mut bans = (ban_list("myTeamBans"), ban_list("theirTeamBans"));
    if bans.0.is_empty() && bans.1.is_empty() {
        for a in actions.iter().filter(|a| text(a, "type") == "ban" && a.get("completed").and_then(Value::as_bool) == Some(true)) {
            let champ = num(a, "championId") as u32;
            if champ == 0 {
                continue;
            }
            if a.get("isAllyAction").and_then(Value::as_bool) == Some(true) {
                bans.0.push(champ);
            } else {
                bans.1.push(champ);
            }
        }
    }
    Some(ChampSelect {
        my_champion,
        my_locked: me.locked,
        my_lane: me.lane,
        lane_opponent,
        enemies,
        my_team,
        their_team,
        bans,
        phase: s.pointer("/timer/phase").and_then(Value::as_str).unwrap_or("").to_string(),
        queue_id: s.get("queueId").and_then(Value::as_i64).unwrap_or(0),
        game_id: num(s, "gameId"),
    })
}

/// The client's region ("NA", "EUW", …) as U.GG's region id ("na1", …).
pub fn ugg_region(region: &str) -> &'static str {
    match region.to_uppercase().as_str() {
        "NA" | "NA1" => "na1",
        "EUW" | "EUW1" => "euw1",
        "EUNE" | "EUN1" => "eun1",
        "KR" => "kr",
        "BR" | "BR1" => "br1",
        "LAN" | "LA1" => "la1",
        "LAS" | "LA2" => "la2",
        "OCE" | "OC1" => "oc1",
        "RU" => "ru",
        "TR" | "TR1" => "tr1",
        "JP" | "JP1" => "jp1",
        "PH" | "PH2" => "ph2",
        "SG" | "SG2" => "sg2",
        "TH" | "TH2" => "th2",
        "TW" | "TW2" => "tw2",
        "VN" | "VN2" => "vn2",
        "ME" | "ME1" => "me1",
        _ => "na1",
    }
}

/// Rune pages made by this app start with this, so the next import reuses
/// their slot instead of touching the player's own pages.
const PAGE_PREFIX: &str = "Poro: ";
/// What pages were called before the app was renamed.
const OLD_PAGE_PREFIX: &str = "MU: ";
const MAX_PAGE_NAME: usize = 30;

/// (port, password) of the running client, from the lockfile the client
/// writes in its install folder while it runs.
///
/// This used to fall back to reading LeagueClientUx.exe's command line with a
/// hidden PowerShell every couple of seconds; antivirus (Avast) treats that as
/// credential theft and sandboxes Poro. Now the running client's own folder is
/// found from its process, which needs no helper process.
fn find_credentials() -> Option<(u16, String)> {
    let running = client_dir();
    // Not running (and no folder to look in): don't bother with stale lockfiles.
    if running == Some(None) {
        return None;
    }
    let mut dirs: Vec<PathBuf> = running.flatten().into_iter().collect();
    dirs.extend(install_dirs());
    for dir in dirs {
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
    None
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

/// The League client's folder if it's running: Some(Some(dir)) when found,
/// Some(None) when it isn't running, None if the process list couldn't be read.
#[cfg(windows)]
fn client_dir() -> Option<Option<PathBuf>> {
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
    };
    use windows_sys::Win32::System::Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION};

    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap == INVALID_HANDLE_VALUE {
            return None;
        }
        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut pid = None;
        let mut ok = Process32FirstW(snap, &mut entry) != 0;
        while ok {
            let len = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(entry.szExeFile.len());
            let name = String::from_utf16_lossy(&entry.szExeFile[..len]);
            if name.eq_ignore_ascii_case("LeagueClientUx.exe") || name.eq_ignore_ascii_case("LeagueClient.exe") {
                pid = Some(entry.th32ProcessID);
                break;
            }
            ok = Process32NextW(snap, &mut entry) != 0;
        }
        CloseHandle(snap);
        let Some(pid) = pid else { return Some(None) };
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            // Running, but its folder can't be read: the usual folders are tried.
            return None;
        }
        let mut buf = [0u16; 1024];
        let mut size = buf.len() as u32;
        let got = QueryFullProcessImageNameW(handle, 0, buf.as_mut_ptr(), &mut size) != 0;
        CloseHandle(handle);
        if !got {
            return None;
        }
        let exe = PathBuf::from(String::from_utf16_lossy(&buf[..size as usize]));
        Some(exe.parent().map(|d| d.to_path_buf()))
    }
}

#[cfg(not(windows))]
fn client_dir() -> Option<Option<PathBuf>> {
    None
}

/// A made-up champ select for testing without a League client
/// (PORO_DEMO_CS=1): picks lock in over the first ~20 seconds.
pub fn demo_champ_select(secs: u64) -> ChampSelect {
    let lane = |s: &str| Lane::from_client(s);
    let ally = |cell: i64, champ: u32, at: u64, pos: &str, name: Option<&str>| CsPlayer {
        cell,
        champion: if secs >= at { champ } else { 0 },
        intent: champ,
        locked: secs >= at,
        lane: lane(pos),
        spells: (4, 14),
        name: name.map(str::to_string),
        is_me: cell == 2,
    };
    let enemy = |cell: i64, champ: u32, at: u64| CsPlayer {
        cell,
        champion: if secs >= at { champ } else { 0 },
        locked: secs >= at,
        ..Default::default()
    };
    let my_team = vec![
        ally(0, 122, 4, "top", None),
        ally(1, 64, 8, "jungle", None),
        ally(2, 103, 10, "middle", None),
        ally(3, 222, 14, "bottom", None),
        ally(4, 412, 16, "utility", None),
    ];
    let their_team = vec![enemy(5, 111, 6), enemy(6, 238, 12), enemy(7, 86, 18), enemy(8, 81, 20), enemy(9, 60, 22)];
    let me = my_team[2].clone();
    ChampSelect {
        my_champion: me.intent,
        my_locked: me.locked,
        my_lane: me.lane,
        lane_opponent: None,
        enemies: their_team.iter().map(|p| p.champion).filter(|&c| c != 0).collect(),
        my_team,
        their_team,
        bans: (vec![157, 91, 0, 0, 0], vec![11, 84, 0, 0, 0]),
        phase: "BAN_PICK".into(),
        queue_id: 420,
        game_id: 1,
    }
}

#[cfg(test)]
mod tests {
    /// Finds a running "LeagueClientUx.exe" (a renamed ping.exe in a temp
    /// folder with a fake lockfile) without any helper process.
    #[test]
    fn finds_client_folder_from_process() {
        assert_eq!(super::client_dir(), Some(None), "no client should be running for this test");
        let dir = std::env::temp_dir().join("poro-fake-league");
        let _ = std::fs::create_dir_all(&dir);
        let exe = dir.join("LeagueClientUx.exe");
        std::fs::copy(r"C:\Windows\System32\PING.EXE", &exe).unwrap();
        std::fs::write(dir.join("lockfile"), "LeagueClient:1234:54321:secretpw:https").unwrap();
        let mut child = std::process::Command::new(&exe).args(["-n", "20", "127.0.0.1"]).stdout(std::process::Stdio::null()).spawn().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(500));
        let found = super::client_dir();
        let creds = super::find_credentials();
        let _ = child.kill();
        let _ = child.wait();
        println!("{found:?} {creds:?}");
        assert_eq!(found, Some(Some(dir.clone())));
        assert_eq!(creds, Some((54321, "secretpw".to_string())));
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod parse_tests {
    use serde_json::json;

    /// Pick phase as the client may send it: null names, -1 "none" ids,
    /// unset spells as u64::MAX, missing fields. It must still parse.
    #[test]
    fn pick_phase_with_nulls_parses() {
        let s = json!({
            "localPlayerCellId": 2,
            "gameId": 7123456789u64,
            "queueId": 420,
            "timer": { "phase": "BAN_PICK" },
            "bans": { "myTeamBans": [157, 238], "theirTeamBans": [11, -1] },
            "actions": [
                [ { "actorCellId": 2, "championId": 103, "completed": true, "type": "pick", "isAllyAction": true } ],
                [ { "actorCellId": 7, "championId": -1, "completed": false, "type": "pick", "isAllyAction": false } ]
            ],
            "myTeam": [
                { "cellId": 0, "championId": 0, "championPickIntent": -1, "assignedPosition": "top",
                  "gameName": null, "tagLine": null, "nameVisibilityType": null, "spell1Id": 18446744073709551615u64, "spell2Id": 4 },
                { "cellId": 2, "championId": 103, "championPickIntent": 103, "assignedPosition": "middle",
                  "gameName": "Me", "tagLine": "NA1", "nameVisibilityType": "VISIBLE", "spell1Id": 4.0, "spell2Id": 14 },
                { "cellId": 1 }
            ],
            "theirTeam": [
                { "cellId": 5, "championId": 238, "assignedPosition": null, "gameName": "" },
                { "cellId": 7, "championId": -1 }
            ]
        });
        let cs = super::parse_champ_select(&s).expect("parses");
        assert_eq!(cs.my_champion, 103);
        assert!(cs.my_locked);
        assert_eq!(cs.my_lane, Some(crate::data::ddragon::Lane::Middle));
        assert_eq!(cs.enemies, vec![238]);
        assert_eq!(cs.bans, (vec![157, 238], vec![11]));
        assert_eq!(cs.my_team[0].spells, (0, 4));
        assert_eq!(cs.my_team[0].name, None);
        assert_eq!(cs.my_team[1].spells, (4, 14));
        assert_eq!(cs.my_team[1].name.as_deref(), Some("Me#NA1"));
        assert_eq!(cs.game_id, 7123456789);
        assert_eq!(cs.phase, "BAN_PICK");
    }

    /// The old typed parse failed on exactly this kind of session.
    #[test]
    fn old_strict_parse_would_have_failed() {
        #[derive(serde::Deserialize)]
        #[allow(dead_code)]
        #[serde(rename_all = "camelCase")]
        struct Member {
            #[serde(default)]
            game_name: String,
            #[serde(default)]
            champion_pick_intent: u32,
        }
        assert!(serde_json::from_value::<Member>(json!({ "gameName": null })).is_err());
        assert!(serde_json::from_value::<Member>(json!({ "championPickIntent": -1 })).is_err());
    }
}
