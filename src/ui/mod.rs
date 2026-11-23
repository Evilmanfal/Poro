//! The app window: a Champion Matchup tab (deeplol) and a Counters tab
//! (lolalytics), a patch picker and the League client connection.

mod counters;
mod games;
mod live_game;
mod matchup;
mod theme;
mod tierlist;
mod overlay;
mod settings_page;
mod tips;
mod update;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use gpui::{div, prelude::*, px, AnyElement, Context, Entity, FocusHandle, Focusable, MouseButton, SharedString, Window};

use crate::data::ddragon::{self, Lane, StaticData};
use crate::data::deeplol::{self, Game, HeadToHead, LaneStats, RunePage, Tips, Versions};
use crate::data::lolalytics::{self, Counters};
use crate::data::matchup_builds;
use crate::lcu::{ChampSelect, Lcu};
use crate::text_input::{Submitted, TextInput};
use theme::*;

pub enum Loadable<T> {
    Idle,
    Loading,
    Ready(T),
    Failed(String),
}

impl<T> Loadable<T> {
    pub fn ready(&self) -> Option<&T> {
        match self {
            Loadable::Ready(t) => Some(t),
            _ => None,
        }
    }
    pub fn is_loading(&self) -> bool {
        matches!(self, Loadable::Loading)
    }
    fn from(r: anyhow::Result<T>) -> Self {
        match r {
            Ok(t) => Loadable::Ready(t),
            Err(e) => Loadable::Failed(format!("{e:#}")),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    TierList,
    Matchup,
    Counters,
    LiveGame,
    Settings,
}

/// A build for "Add Build".
#[derive(Clone)]
pub struct BuildImport {
    pub name: String,
    pub champion: u32,
    pub page: Option<RunePage>,
    pub spells: Option<(u32, u32)>,
    /// Item set blocks: (title, items).
    pub blocks: Vec<(String, Vec<u32>)>,
}

fn join_and(parts: &[&str]) -> String {
    match parts {
        [] => String::new(),
        [a] => a.to_string(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// What auto-follow already took from this champ select.
#[derive(Default)]
struct Followed {
    game_id: u64,
    champion: u32,
    opponent: u32,
    lane: Option<Lane>,
    /// Switched to the Champion Matchup tab already.
    switched: bool,
}

#[derive(Clone, PartialEq)]
pub enum ClientStatus {
    NotRunning,
    Connected,
    ChampSelect(ChampSelect),
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SortCol {
    Champion,
    VsWr,
    AllWr,
    D1,
    D2,
    Games,
}

pub struct AppView {
    focus: FocusHandle,
    logo: Arc<gpui::Image>,
    pub data: Option<Arc<StaticData>>,
    pub startup_error: Option<String>,
    pub dl_versions: Option<Versions>,
    /// The last 6 patches, newest first ("16.23").
    pub patches: Vec<String>,
    pub patch: usize,
    /// The patch dropdown is open.
    patch_menu: bool,
    patch_menu_closed: Option<std::time::Instant>,
    pub tab: Tab,

    // Champion Matchup
    pub a: Option<u32>,
    pub b: Option<u32>,
    pub lane: Lane,
    lane_auto: bool,
    pub input_a: Entity<TextInput>,
    pub input_b: Entity<TextInput>,
    pub a_stats: Loadable<HashMap<Lane, LaneStats>>,
    pub b_stats: Loadable<HashMap<Lane, LaneStats>>,
    pub h2h: Loadable<HashMap<Lane, HeadToHead>>,
    pub tips: Loadable<Tips>,
    /// Every game deeplol has of this matchup (for the matchup builds).
    pub history: Loadable<Vec<Game>>,
    /// Each game in `history` has its patch looked up.
    pub history_patched: bool,
    /// How many of the matchup's games the list shows.
    pub games_shown: usize,
    /// Games list filters (deeplol's lane and ALL/OTP/PRO buttons).
    pub games_lane: Option<Lane>,
    pub games_player: PlayerFilter,
    pub expanded: Option<String>,
    pub skills: HashMap<u32, Vec<ddragon::Skill>>,
    gen: u64,
    a_gen: u64,

    // Counters
    pub c_champ: Option<u32>,
    pub c_lane: Lane,
    c_lane_auto: bool,
    pub input_c: Entity<TextInput>,
    /// Tier List search.
    pub input_t: Entity<TextInput>,
    pub counters: Loadable<Counters>,
    /// lolalytics counters for the VS card's "Weak Against", by
    /// (champion, lane, patch); hardest matchups first.
    counter_cache: HashMap<(u32, Lane, String), Loadable<Vec<lolalytics::Counter>>>,
    /// U.GG's builds by (champion, lane, patch), for Champion Matchup's
    /// general builds when deeplol has none for that lane.
    ugg_general: HashMap<(u32, Lane, String), Loadable<Vec<crate::data::ugg::UggBuild>>>,
    pub sort: (SortCol, bool),
    c_gen: u64,

    // League client
    pub client: ClientStatus,
    pub follow_client: bool,
    last_followed: Option<Followed>,
    /// The client's gameflow phase ("ChampSelect", "InProgress", …).
    pub phase: String,
    pub live: live_game::LiveState,
    /// Spells from an Add Build outside champ select: (champion, spells).
    pending_spells: Option<(u32, (u32, u32))>,
    lcu: Arc<Mutex<Option<Lcu>>>,
    pub toast: Option<(String, bool)>,
    toast_gen: u64,
    up: update::UpdateState,
    tiers: tierlist::TierState,
    /// The tray icon, while "Exit into tray" is on.
    tray: Option<crate::tray::Tray>,
    main_hwnd: Option<isize>,
    tray_hint_shown: bool,
    pub settings: crate::settings::Settings,
    ov: overlay::OverlayState,
}

/// Runs `job` on a background thread, then `done` on the UI thread.
fn spawn_job<T: Send + 'static>(
    cx: &mut Context<AppView>,
    job: impl FnOnce() -> anyhow::Result<T> + Send + 'static,
    done: impl FnOnce(&mut AppView, anyhow::Result<T>, &mut Context<AppView>) + 'static,
) {
    cx.spawn(async move |this, cx| {
        let r = cx.background_executor().spawn(async move { job() }).await;
        let _ = this.update(cx, |this, cx| {
            done(this, r, cx);
            cx.notify();
        });
    })
    .detach();
}

impl AppView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input_a = cx.new(|cx| TextInput::new(cx, "Search a champion", ""));
        let input_b = cx.new(|cx| TextInput::new(cx, "Search an opponent", ""));
        let input_c = cx.new(|cx| TextInput::new(cx, "Search a champion", ""));
        let input_t = cx.new(|cx| TextInput::new(cx, "Search any champion", ""));
        for input in [&input_a, &input_b, &input_c, &input_t] {
            cx.observe(input, |_, _, cx| cx.notify()).detach();
        }
        cx.subscribe(&input_a, |this, _, _: &Submitted, cx| this.pick_first(Slot::A, cx)).detach();
        cx.subscribe(&input_b, |this, _, _: &Submitted, cx| this.pick_first(Slot::B, cx)).detach();
        cx.subscribe(&input_c, |this, _, _: &Submitted, cx| this.pick_first(Slot::C, cx)).detach();
        cx.subscribe(&input_t, |this, _, _: &Submitted, cx| this.open_first_search_result(cx)).detach();
        window.focus(&input_a.focus_handle(cx));

        let mut view = AppView {
            focus: cx.focus_handle(),
            logo: Arc::new(gpui::Image::from_bytes(gpui::ImageFormat::Png, include_bytes!("../../assets/poro.png").to_vec())),
            data: None,
            startup_error: None,
            dl_versions: None,
            patches: Vec::new(),
            patch: 0,
            patch_menu: false,
            patch_menu_closed: None,
            tab: Tab::TierList,
            a: None,
            b: None,
            lane: Lane::Middle,
            lane_auto: true,
            input_a,
            input_b,
            a_stats: Loadable::Idle,
            b_stats: Loadable::Idle,
            h2h: Loadable::Idle,
            tips: Loadable::Idle,
            history: Loadable::Idle,
            history_patched: false,
            games_shown: GAMES_PAGE,
            games_lane: None,
            games_player: PlayerFilter::All,
            expanded: None,
            skills: HashMap::new(),
            gen: 0,
            a_gen: 0,
            c_champ: None,
            c_lane: Lane::Middle,
            c_lane_auto: true,
            input_c,
            input_t,
            counters: Loadable::Idle,
            counter_cache: HashMap::new(),
            ugg_general: HashMap::new(),
            sort: (SortCol::VsWr, true),
            c_gen: 0,
            client: ClientStatus::NotRunning,
            follow_client: true,
            last_followed: None,
            phase: String::new(),
            live: Default::default(),
            pending_spells: None,
            lcu: Arc::new(Mutex::new(None)),
            toast: None,
            toast_gen: 0,
            up: update::UpdateState::default(),
            tiers: tierlist::TierState::default(),
            tray: None,
            main_hwnd: crate::tray::hwnd_of(window),
            tray_hint_shown: false,
            settings: crate::settings::load(),
            ov: overlay::OverlayState::default(),
        };
        view.start_update_checks(cx);
        view.start_auto_refresh(cx);
        view.start_overlay_loop(cx);
        view.start_tab_watch(cx);
        view.sync_tray(cx);
        // Closing hides to the tray when "Exit into tray" is on; otherwise it
        // quits (the overlay is a window too, so quit explicitly).
        let entity = cx.entity();
        window.on_window_should_close(cx, move |_, cx| entity.update(cx, |this, cx| this.on_close_requested(cx)));
        view.load_static(cx);
        view.poll_client(cx);
        view
    }

    fn load_static(&mut self, cx: &mut Context<Self>) {
        spawn_job(
            cx,
            || Ok((ddragon::load()?, deeplol::versions().ok())),
            |this, r, cx| match r {
                Ok((data, dl)) => {
                    this.patches = patch_list(&data);
                    this.data = Some(Arc::new(data));
                    this.dl_versions = dl;
                    // Testing aid: `poro.exe --preview-overlays` starts the overlay preview.
                    if std::env::args().any(|a| a == "--preview-overlays") {
                        this.preview_overlays(cx);
                    }
                    // Start on a matchup so the window isn't empty.
                    if this.a.is_none() {
                        this.set_champion(Slot::A, 61, cx);
                        this.set_champion(Slot::B, 7, cx);
                        this.set_champion(Slot::C, 61, cx);
                    }
                }
                Err(e) => this.startup_error = Some(format!("{e:#}")),
            },
        );
    }

    /// Every 4 hours: pick up new patches and reload whatever is on screen,
    /// so batches the sites publish while Poro is open show up by themselves.
    /// (`PORO_REFRESH_SECS` shortens the wait for tests.)
    fn start_auto_refresh(&mut self, cx: &mut Context<Self>) {
        let every = std::env::var("PORO_REFRESH_SECS")
            .ok()
            .and_then(|v| v.parse().ok())
            .map(Duration::from_secs)
            .unwrap_or(AUTO_REFRESH);
        cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(every).await;
            if this.update(cx, |this, cx| this.refresh_all(cx)).is_err() {
                break;
            }
        })
        .detach();
    }

    fn refresh_all(&mut self, cx: &mut Context<Self>) {
        if std::env::var_os("PORO_DEBUG").is_some() {
            eprintln!("auto-refresh: reloading patches and pages");
        }
        spawn_job(
            cx,
            || Ok((ddragon::load()?, deeplol::versions().ok())),
            |this, r, cx| {
                let Ok((data, dl)) = r else { return };
                // Keep the selected patch; stay on the live one if that's where we were.
                let was_live = this.is_latest_patch();
                let label = this.patch_label().to_string();
                this.patches = patch_list(&data);
                this.patch = if was_live { 0 } else { this.patches.iter().position(|p| *p == label).unwrap_or(0) };
                this.data = Some(Arc::new(data));
                if dl.is_some() {
                    this.dl_versions = dl;
                }
                // Reload what's on screen with the same selections.
                this.tiers.reset_for_refresh();
                this.counter_cache.clear();
                if this.a.is_some() {
                    this.reload_matchup(false, cx);
                }
                this.reload_counters(cx);
                this.reload_champion_page(cx);
                this.ov.refresh_build();
                cx.notify();
            },
        );
    }

    pub fn patch_label(&self) -> &str {
        self.patches.get(self.patch).map(String::as_str).unwrap_or("")
    }

    /// The patch to ask deeplol for, and whether it had to fall back because
    /// deeplol hasn't published the selected one yet.
    pub fn deeplol_patch(&self) -> (String, bool) {
        let wanted = self.patch_label().to_string();
        match &self.dl_versions {
            Some(v) if v.game_version_list.contains(&wanted) => (wanted, false),
            Some(v) if !v.game_version_list.is_empty() => {
                // Selected patch is newer than deeplol's data: use its newest.
                let newest = v.game_version_list[0].clone();
                if patch_num(&wanted) > patch_num(&newest) {
                    (newest, true)
                } else {
                    (wanted, false)
                }
            }
            _ => (wanted, false),
        }
    }

    pub fn is_latest_patch(&self) -> bool {
        self.patch == 0
    }

    pub fn set_patch(&mut self, i: usize, cx: &mut Context<Self>) {
        if i != self.patch {
            self.patch = i;
            self.reload_matchup(false, cx);
            self.reload_counters(cx);
            self.reload_champion_page(cx);
        }
    }

    pub fn set_champion(&mut self, slot: Slot, key: u32, cx: &mut Context<Self>) {
        let Some(data) = self.data.clone() else { return };
        let name = data.champion_name(key);
        match slot {
            Slot::A => {
                self.a = Some(key);
                self.input_a.update(cx, |i, cx| i.set_text(&name, cx));
                self.lane_auto = true;
                self.reload_matchup(false, cx);
            }
            Slot::B => {
                self.b = Some(key);
                self.input_b.update(cx, |i, cx| i.set_text(&name, cx));
                self.reload_matchup(true, cx);
            }
            Slot::C => {
                self.c_champ = Some(key);
                self.input_c.update(cx, |i, cx| i.set_text(&name, cx));
                self.c_lane_auto = true;
                self.reload_counters(cx);
            }
        }
        self.load_skills(key, cx);
    }

    fn pick_first(&mut self, slot: Slot, cx: &mut Context<Self>) {
        let Some(data) = self.data.clone() else { return };
        let input = match slot {
            Slot::A => &self.input_a,
            Slot::B => &self.input_b,
            Slot::C => &self.input_c,
        };
        let query = input.read(cx).text().to_string();
        if let Some(c) = data.search(&query).first() {
            self.set_champion(slot, c.key, cx);
        }
    }

    pub fn swap(&mut self, cx: &mut Context<Self>) {
        if let (Some(a), Some(b)) = (self.a, self.b) {
            let lane = self.lane;
            self.set_champion(Slot::A, b, cx);
            self.set_champion(Slot::B, a, cx);
            self.lane = lane;
            self.lane_auto = false;
            self.reload_matchup(false, cx);
        }
    }

    pub fn reset(&mut self, cx: &mut Context<Self>) {
        self.a = None;
        self.b = None;
        self.input_a.update(cx, |i, cx| i.clear(cx));
        self.input_b.update(cx, |i, cx| i.clear(cx));
        self.reload_matchup(false, cx);
    }

    pub fn set_lane(&mut self, lane: Lane, cx: &mut Context<Self>) {
        self.lane = lane;
        self.lane_auto = false;
        cx.notify();
    }

    pub fn set_counter_lane(&mut self, lane: Lane, cx: &mut Context<Self>) {
        self.c_lane = lane;
        self.c_lane_auto = false;
        self.reload_counters(cx);
    }

    fn load_skills(&mut self, key: u32, cx: &mut Context<Self>) {
        let Some(data) = self.data.clone() else { return };
        if self.skills.contains_key(&key) {
            return;
        }
        self.skills.insert(key, Vec::new());
        spawn_job(
            cx,
            move || {
                let champ = data.champion(key).ok_or_else(|| anyhow::anyhow!("unknown champion"))?;
                ddragon::skills(&data, champ)
            },
            move |this, r, _| {
                if let Ok(icons) = r {
                    this.skills.insert(key, icons);
                }
            },
        );
    }

    /// Reloads the matchup. `only_enemy`: just the opponent changed.
    pub fn reload_matchup(&mut self, only_enemy: bool, cx: &mut Context<Self>) {
        self.gen += 1;
        let gen = self.gen;
        let (patch, _) = self.deeplol_patch();
        let live = self.is_latest_patch();

        if !only_enemy {
            self.a_gen += 1;
            let a_gen = self.a_gen;
            self.a_stats = Loadable::Idle;
            if let Some(a) = self.a {
                self.a_stats = Loadable::Loading;
                let p = patch.clone();
                spawn_job(cx, move || deeplol::champion(a, &p, live), move |this, r, _| {
                    if this.a_gen != a_gen {
                        return;
                    }
                    if this.lane_auto {
                        if let Ok(lanes) = &r {
                            if let Some(lane) = main_lane(lanes) {
                                this.lane = lane;
                            }
                        }
                    }
                    this.a_stats = Loadable::from(r);
                });
            }
        }

        self.b_stats = Loadable::Idle;
        self.h2h = Loadable::Idle;
        self.tips = Loadable::Idle;
        self.history = Loadable::Idle;
        self.games_shown = GAMES_PAGE;
        self.expanded = None;
        if let Some(b) = self.b {
            self.b_stats = Loadable::Loading;
            let p = patch.clone();
            spawn_job(cx, move || deeplol::champion(b, &p, live), move |this, r, _| {
                if this.gen == gen {
                    this.b_stats = Loadable::from(r);
                }
            });
        }
        if let (Some(a), Some(b)) = (self.a, self.b) {
            self.h2h = Loadable::Loading;
            spawn_job(cx, move || deeplol::head_to_head(a, b), move |this, r, _| {
                if this.gen == gen {
                    this.h2h = Loadable::from(r);
                }
            });
            self.tips = Loadable::Loading;
            spawn_job(cx, move || deeplol::tips(a, b), move |this, r, _| {
                if this.gen == gen {
                    this.tips = Loadable::from(r);
                }
            });
            // The games first (enough for the builds on the live patch), then
            // each game's patch, which takes longer the first time.
            self.history = Loadable::Loading;
            self.history_patched = false;
            spawn_job(cx, move || matchup_builds::game_lists(a, b), move |this, r, cx| {
                if this.gen != gen {
                    return;
                }
                let games = r.as_ref().ok().cloned();
                this.history = Loadable::from(r);
                let Some(mut games) = games else {
                    this.history_patched = true;
                    return;
                };
                spawn_job(
                    cx,
                    move || {
                        matchup_builds::fill_patches(&mut games);
                        Ok(games)
                    },
                    move |this, r, _| {
                        if this.gen == gen {
                            if let Ok(games) = r {
                                this.history = Loadable::Ready(games);
                            }
                            this.history_patched = true;
                        }
                    },
                );
            });
        }
        cx.notify();
    }

    pub fn set_games_filter(&mut self, lane: Option<Lane>, player: PlayerFilter, cx: &mut Context<Self>) {
        self.games_lane = lane;
        self.games_player = player;
        self.games_shown = GAMES_PAGE;
        self.expanded = None;
        cx.notify();
    }

    /// lolalytics' counters for `champ` in `lane` on the selected patch,
    /// lowest win rate first (loads them the first time).
    pub fn counters_for(&mut self, champ: u32, lane: Lane, cx: &mut Context<Self>) -> Loadable<Vec<lolalytics::Counter>> {
        let patch = self.patch_label().to_string();
        let key = (champ, lane, patch.clone());
        if let Some(state) = self.counter_cache.get(&key) {
            return match state {
                Loadable::Ready(v) => Loadable::Ready(v.clone()),
                Loadable::Failed(e) => Loadable::Failed(e.clone()),
                Loadable::Loading => Loadable::Loading,
                Loadable::Idle => Loadable::Idle,
            };
        }
        let Some(data) = self.data.clone() else { return Loadable::Idle };
        self.counter_cache.insert(key.clone(), Loadable::Loading);
        let live = self.is_latest_patch();
        spawn_job(
            cx,
            move || {
                let c = data.champion(champ).ok_or_else(|| anyhow::anyhow!("unknown champion"))?.clone();
                let mut list = lolalytics::counters(&c, lane, &patch, live)?.counters;
                list.sort_by(|a, b| a.vs_wr.total_cmp(&b.vs_wr));
                Ok(list)
            },
            move |this, r, _| {
                this.counter_cache.insert(key, Loadable::from(r));
            },
        );
        Loadable::Loading
    }

    /// The matchup's games for the selected patch. On the live patch that's
    /// every recent game (the live patch alone has too few); on an older
    /// patch, only games played on it.
    pub fn patch_games(&self) -> Vec<Game> {
        let Some(all) = self.history.ready() else { return Vec::new() };
        if self.is_latest_patch() {
            return all.clone();
        }
        let patch = self.patch_label();
        all.iter().filter(|g| g.patch.as_deref() == Some(patch)).cloned().collect()
    }

    /// What `patch_games` covers, for headings.
    pub fn patch_games_label(&self, games: &[Game]) -> String {
        if self.is_latest_patch() {
            let breakdown = matchup_builds::patch_breakdown(games);
            if breakdown.is_empty() {
                format!("{} recent games", games.len())
            } else {
                format!("{} recent games ({breakdown})", games.len())
            }
        } else {
            format!("{} games on patch {}", games.len(), self.patch_label())
        }
    }

    pub fn reload_counters(&mut self, cx: &mut Context<Self>) {
        let (Some(key), Some(data)) = (self.c_champ, self.data.clone()) else { return };
        self.c_gen += 1;
        let gen = self.c_gen;
        let patch = self.patch_label().to_string();
        let live = self.is_latest_patch();
        let auto = self.c_lane_auto;
        let lane = self.c_lane;
        let (dl_patch, _) = self.deeplol_patch();
        self.counters = Loadable::Loading;
        spawn_job(
            cx,
            move || {
                let champ = data.champion(key).ok_or_else(|| anyhow::anyhow!("unknown champion"))?.clone();
                // Pick the champion's main lane (from deeplol's lane split) when the user hasn't chosen one.
                let lane = if auto {
                    deeplol::champion(key, &dl_patch, live).ok().and_then(|l| main_lane(&l)).unwrap_or(lane)
                } else {
                    lane
                };
                Ok((lane, lolalytics::counters(&champ, lane, &patch, live)?))
            },
            move |this, r, _| {
                if this.c_gen != gen {
                    return;
                }
                match r {
                    Ok((lane, c)) => {
                        this.c_lane = lane;
                        this.counters = Loadable::Ready(c);
                    }
                    Err(e) => this.counters = Loadable::Failed(format!("{e:#}")),
                }
            },
        );
        cx.notify();
    }

    pub fn open_matchup(&mut self, a: u32, b: u32, lane: Lane, cx: &mut Context<Self>) {
        self.tab = Tab::Matchup;
        self.set_champion(Slot::A, a, cx);
        self.set_champion(Slot::B, b, cx);
        self.lane = lane;
        self.lane_auto = false;
        cx.notify();
    }

    // ---- League client ----

    fn poll_client(&mut self, cx: &mut Context<Self>) {
        let lcu = self.lcu.clone();
        // Testing without a League client: PORO_DEMO_CS=1 plays a made-up champ select.
        let demo_start = std::env::var_os("PORO_DEMO_CS").map(|_| std::time::Instant::now());
        cx.spawn(async move |this, cx| loop {
            let lcu = lcu.clone();
            let status = cx
                .background_executor()
                .spawn(async move {
                    if let Some(t) = demo_start {
                        return (ClientStatus::ChampSelect(crate::lcu::demo_champ_select(t.elapsed().as_secs())), "ChampSelect".to_string());
                    }
                    let mut guard = lcu.lock().unwrap();
                    // A slow reply (the client is busy switching phases) isn't a
                    // closed client: only give up after two failed checks.
                    if guard.as_ref().is_some_and(|l| !l.alive() && !l.alive()) {
                        *guard = None;
                    }
                    if guard.is_none() {
                        *guard = Lcu::connect().ok();
                    }
                    match guard.as_ref() {
                        None => (ClientStatus::NotRunning, String::new()),
                        Some(l) => {
                            let status = match l.champ_select() {
                                Some(cs) => ClientStatus::ChampSelect(cs),
                                None => ClientStatus::Connected,
                            };
                            (status, l.phase())
                        }
                    }
                })
                .await;
            let (status, phase) = status;
            let alive = this.update(cx, |this, cx| {
                if this.phase != phase {
                    this.phase = phase;
                    cx.notify();
                }
                this.on_client_status(status, cx);
                // Live Game keeps its data coming even while another tab is open.
                this.ensure_live(cx);
            });
            if alive.is_err() {
                break;
            }
            cx.background_executor().timer(Duration::from_secs(2)).await;
        })
        .detach();
    }

    fn on_client_status(&mut self, status: ClientStatus, cx: &mut Context<Self>) {
        if status == self.client {
            return;
        }
        // The client still says champ select: one unreadable poll doesn't end it.
        if !matches!(status, ClientStatus::ChampSelect(_))
            && self.phase == "ChampSelect"
            && matches!(self.client, ClientStatus::ChampSelect(_))
        {
            return;
        }
        if matches!(status, ClientStatus::ChampSelect(_)) && !matches!(self.client, ClientStatus::ChampSelect(_)) {
            self.reset_live_game();
        }
        if matches!(status, ClientStatus::NotRunning) {
            self.live.me = None;
        }
        if let ClientStatus::ChampSelect(cs) = &status {
            if self.follow_client {
                self.follow_champ_select(cs, cx);
            }
            self.apply_pending_spells(cs, cx);
        }
        self.client = status;
        cx.notify();
    }

    /// Fills in the Champion Matchup from champ select. Each thing is set only
    /// when it changes, and the tab switches once per champ select, when my
    /// pick locks in, so browsing other tabs isn't interrupted.
    fn follow_champ_select(&mut self, cs: &ChampSelect, cx: &mut Context<Self>) {
        // A new champ select starts over; a brief hiccup reading the same one doesn't.
        let mut f = self.last_followed.take().filter(|f| f.game_id == cs.game_id).unwrap_or_default();
        f.game_id = cs.game_id;
        if let Some(lane) = cs.my_lane {
            if f.lane != Some(lane) {
                f.lane = Some(lane);
                self.lane = lane;
                self.lane_auto = false;
            }
        }
        if cs.my_locked && cs.my_champion != 0 && f.champion != cs.my_champion {
            f.champion = cs.my_champion;
            if self.a != Some(cs.my_champion) {
                self.set_champion(Slot::A, cs.my_champion, cx);
            }
            if !f.switched {
                f.switched = true;
                self.tab = Tab::Matchup;
            }
        }
        if let Some(opp) = self.lane_opponent(cs) {
            if f.opponent != opp {
                f.opponent = opp;
                if self.b != Some(opp) {
                    self.set_champion(Slot::B, opp, cx);
                }
            }
        }
        self.last_followed = Some(f);
    }

    /// The enemy in my lane: from the client when it says enemy roles,
    /// otherwise predicted from the enemies' locked champions.
    pub fn lane_opponent(&self, cs: &ChampSelect) -> Option<u32> {
        if let Some(o) = cs.lane_opponent {
            return Some(o);
        }
        let lane = cs.my_lane.or(if self.lane_auto { None } else { Some(self.lane) })?;
        self.enemy_roles(cs).into_iter().find(|(_, l)| *l == Some(lane)).map(|(c, _)| c)
    }

    /// Locked enemy champions with their roles (given by the client or predicted).
    pub fn enemy_roles(&self, cs: &ChampSelect) -> Vec<(u32, Option<Lane>)> {
        let locked: Vec<&crate::lcu::CsPlayer> = cs.their_team.iter().filter(|p| p.champion != 0).collect();
        if !locked.is_empty() && locked.iter().all(|p| p.lane.is_some()) {
            return locked.iter().map(|p| (p.champion, p.lane)).collect();
        }
        let champs: Vec<u32> = locked.iter().map(|p| p.champion).collect();
        let roles = match &self.tiers.list {
            Some(Loadable::Ready(list)) => crate::data::ugg::predict_roles(list, &champs),
            _ => vec![None; champs.len()],
        };
        champs.into_iter().zip(roles).collect()
    }

    /// Adds or removes the tray icon to match the "Exit into tray" setting.
    pub fn sync_tray(&mut self, cx: &mut Context<Self>) {
        if !self.settings.exit_to_tray {
            self.tray = None;
            return;
        }
        if self.tray.is_some() {
            return;
        }
        let Some((tray, events)) = crate::tray::Tray::new() else { return };
        self.tray = Some(tray);
        cx.spawn(async move |this, cx| {
            while let Ok(ev) = events.recv().await {
                if this.update(cx, |this, cx| this.on_tray(ev, cx)).is_err() {
                    break;
                }
            }
        })
        .detach();
    }

    fn on_tray(&mut self, ev: crate::tray::TrayEvent, cx: &mut Context<Self>) {
        use crate::tray::TrayEvent;
        match ev {
            TrayEvent::Open => {
                if let Some(h) = self.main_hwnd {
                    crate::tray::show_window(h);
                }
                cx.notify();
            }
            TrayEvent::Quit => {
                self.tray = None; // removes the icon right away
                cx.quit();
            }
            TrayEvent::Readd => {
                if let Some(tray) = &self.tray {
                    tray.add();
                }
            }
        }
    }

    /// Closing the window: hide to the tray if that's on, otherwise quit.
    fn on_close_requested(&mut self, cx: &mut Context<Self>) -> bool {
        match (&self.tray, self.main_hwnd) {
            (Some(tray), Some(h)) if self.settings.exit_to_tray => {
                crate::tray::hide_window(h);
                if !self.tray_hint_shown {
                    self.tray_hint_shown = true;
                    tray.notify(
                        "Poro is still running",
                        "Your in-game overlays keep working. Click the tray icon to open Poro, or right-click it to quit.",
                    );
                }
                false
            }
            _ => {
                self.tray = None;
                cx.quit();
                true
            }
        }
    }

    /// "Add Build": puts a build's rune page, summoner spells and items into
    /// the League client. Spells can only be set in champ select, so outside
    /// of one they're kept for the next champ select on that champion. Items
    /// become an item set, shown in the in-game shop.
    pub fn apply_build(&mut self, build: BuildImport, cx: &mut Context<Self>) {
        let lcu = self.lcu.clone();
        let in_champ_select = matches!(self.client, ClientStatus::ChampSelect(_));
        let spells = build.spells.map(|s| self.settings.flash_key.order(s));
        if !in_champ_select {
            self.pending_spells = spells.map(|s| (build.champion, s));
        }
        self.show_toast("Sending the build to the League client…".into(), true, cx);
        spawn_job(
            cx,
            move || {
                let mut guard = lcu.lock().unwrap();
                if guard.is_none() {
                    *guard = Some(Lcu::connect()?);
                }
                let l = guard.as_ref().unwrap();
                let mut done = Vec::new();
                let mut failed = Vec::new();
                if let Some(page) = &build.page {
                    match l.apply_runes(&build.name, page) {
                        Ok(()) => done.push("runes"),
                        Err(e) => failed.push(format!("runes: {e}")),
                    }
                }
                let mut spells_later = false;
                if let Some((s1, s2)) = spells {
                    if in_champ_select {
                        match l.apply_spells(s1, s2) {
                            Ok(()) => done.push("summoner spells"),
                            Err(e) => failed.push(format!("spells: {e}")),
                        }
                    } else {
                        spells_later = true;
                    }
                }
                if build.champion != 0 && build.blocks.iter().any(|(_, items)| !items.is_empty()) {
                    match l.apply_item_set(&build.name, build.champion, &build.blocks) {
                        Ok(()) => done.push("item set"),
                        Err(e) => failed.push(format!("items: {e}")),
                    }
                }
                if done.is_empty() && !failed.is_empty() {
                    anyhow::bail!("{}", failed.join("; "));
                }
                let mut msg = format!("\"{}\": {} added", build.name, join_and(&done));
                if spells_later {
                    msg.push_str(". Summoner spells will be set in your next champ select");
                }
                if !failed.is_empty() {
                    msg.push_str(&format!(" (not added: {})", failed.join("; ")));
                }
                Ok(msg)
            },
            |this, r, cx| match r {
                Ok(msg) => this.show_toast(msg, true, cx),
                Err(e) => this.show_toast(format!("Couldn't add the build: {e:#}"), false, cx),
            },
        );
    }

    /// Sets summoner spells kept from an Add Build outside champ select, once
    /// I'm on that champion.
    fn apply_pending_spells(&mut self, cs: &ChampSelect, cx: &mut Context<Self>) {
        let Some((champ, (s1, s2))) = self.pending_spells else { return };
        if cs.my_champion == 0 || (champ != 0 && champ != cs.my_champion) {
            return;
        }
        self.pending_spells = None;
        let lcu = self.lcu.clone();
        spawn_job(
            cx,
            move || {
                let guard = lcu.lock().unwrap();
                guard.as_ref().ok_or_else(|| anyhow::anyhow!("not connected"))?.apply_spells(s1, s2)
            },
            |this, r, cx| {
                if r.is_ok() {
                    this.show_toast("Summoner spells set from your build".into(), true, cx);
                }
            },
        );
    }

    pub fn show_toast(&mut self, msg: String, ok: bool, cx: &mut Context<Self>) {
        self.toast = Some((msg, ok));
        self.toast_gen += 1;
        let gen = self.toast_gen;
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_secs(5)).await;
            let _ = this.update(cx, |this, cx| {
                if this.toast_gen == gen {
                    this.toast = None;
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }

    // ---- rendering ----

    fn render_top_bar(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let tab = |label: &'static str, t: Tab, this: &Self, cx: &mut Context<Self>| {
            let active = this.tab == t;
            div()
                .id(label)
                .px(px(14.))
                .py(px(7.))
                .rounded(px(6.))
                .text_size(px(14.))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .cursor_pointer()
                .when(active, |d| d.bg(rgb(PANEL_2)).text_color(rgb(TEXT)))
                .when(!active, |d| d.text_color(rgb(MUTED)).hover(|s| s.text_color(rgb(TEXT))))
                .on_click(cx.listener(move |this, _, _, cx| {
                    // Clicking Tier List while on a champion page goes back to the list.
                    if t == Tab::TierList && this.tab == Tab::TierList {
                        this.tiers.page = None;
                    }
                    this.tab = t;
                    cx.notify();
                }))
                .child(label)
        };
        // Patch picker: one button that opens a list.
        let label_of = |i: usize, p: &str| if i == 0 { format!("{p} (live)") } else { p.to_string() };
        let current = label_of(self.patch, self.patch_label());
        let open = self.patch_menu;
        let items: Vec<AnyElement> = if open {
            self.patches
                .iter()
                .enumerate()
                .map(|(i, p)| {
                    let active = i == self.patch;
                    div()
                        .id(SharedString::from(format!("patch-{p}")))
                        .px(px(10.))
                        .py(px(6.))
                        .rounded(px(5.))
                        .text_size(px(13.))
                        .cursor_pointer()
                        .when(active, |d| d.text_color(rgb(ACCENT)).bg(rgba(0x37c8ab22)))
                        .when(!active, |d| d.text_color(rgb(TEXT)).hover(|s| s.bg(rgb(BORDER))))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.patch_menu = false;
                            this.set_patch(i, cx);
                            cx.notify();
                        }))
                        .child(label_of(i, p))
                        .into_any_element()
                })
                .collect()
        } else {
            Vec::new()
        };
        let patch_picker = div()
            .relative()
            .child(
                div()
                    .id("patch-picker")
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .px(px(10.))
                    .py(px(5.))
                    .rounded(px(6.))
                    .border_1()
                    .border_color(rgb(if open { ACCENT } else { BORDER }))
                    .text_size(px(12.))
                    .cursor_pointer()
                    .hover(|s| s.bg(rgb(PANEL_2)))
                    .on_click(cx.listener(|this, _, _, cx| {
                        // The same click already closed the open menu.
                        let just_closed = this.patch_menu_closed.is_some_and(|t| t.elapsed() < Duration::from_millis(400));
                        this.patch_menu = !this.patch_menu && !just_closed;
                        cx.notify();
                    }))
                    .child(div().text_color(rgb(MUTED)).child("Patch"))
                    .child(div().text_color(rgb(ACCENT)).font_weight(gpui::FontWeight::SEMIBOLD).child(current))
                    .child(div().text_color(rgb(MUTED)).child(if open { "▴" } else { "▾" })),
            )
            .when(open, |d| {
                d.child(
                    gpui::deferred(
                        div()
                            .id("patch-menu")
                            .absolute()
                            .top(px(34.))
                            .right_0()
                            .w(px(150.))
                            .p(px(4.))
                            .rounded(px(8.))
                            .bg(rgb(PANEL_2))
                            .border_1()
                            .border_color(rgb(BORDER))
                            .shadow_lg()
                            .occlude()
                            .flex()
                            .flex_col()
                            // Clicking anywhere else closes it.
                            .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                                this.patch_menu = false;
                                this.patch_menu_closed = Some(std::time::Instant::now());
                                cx.notify();
                            }))
                            .children(items),
                    )
                    .with_priority(2),
                )
            });

        let (dot, label) = match &self.client {
            ClientStatus::NotRunning => (MUTED, "League client not running".to_string()),
            ClientStatus::Connected => (ACCENT, "League client connected".to_string()),
            ClientStatus::ChampSelect(cs) => {
                let data = self.data.clone();
                let name = |k: u32| data.as_ref().map(|d| d.champion_name(k)).unwrap_or_default();
                let mut s = "In champ select".to_string();
                if cs.my_champion != 0 {
                    s = format!("{s}: {}", name(cs.my_champion));
                    if let Some(l) = cs.my_lane {
                        s = format!("{s} {}", l.label());
                    }
                    if let Some(o) = self.lane_opponent(cs) {
                        s = format!("{s} vs {}", name(o));
                    }
                }
                (GOLD, s)
            }
        };
        let follow = self.follow_client;
        let update_pill = self.update_pill(cx);

        div()
            .flex()
            .items_center()
            .gap(px(8.))
            .px(px(16.))
            .py(px(10.))
            .bg(rgb(TOP_BAR))
            .border_b_1()
            .border_color(rgb(BORDER))
            .child(gpui::img(self.logo.clone()).size(px(30.)).flex_none())
            .child(div().text_size(px(17.)).font_weight(gpui::FontWeight::BOLD).text_color(rgb(TEXT)).child("Poro"))
            .child(div().text_size(px(11.)).text_color(rgb(MUTED)).mr(px(10.)).child(format!("v{}", crate::update::current())))
            .child(tab("Tier List", Tab::TierList, self, cx))
            .child(tab("Champion Matchup", Tab::Matchup, self, cx))
            .child(tab("Counters", Tab::Counters, self, cx))
            .child(tab("Live Game", Tab::LiveGame, self, cx))
            .child(tab("Settings", Tab::Settings, self, cx))
            .child(div().flex_1())
            .child(patch_picker)
            .child(div().w(px(10.)))
            .children(update_pill)
            .child(
                div()
                    .id("client")
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .px(px(10.))
                    .py(px(5.))
                    .rounded(px(12.))
                    .bg(rgb(PANEL))
                    .text_size(px(12.))
                    .text_color(rgb(TEXT))
                    .cursor_pointer()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.follow_client = !this.follow_client;
                        this.last_followed = None;
                        cx.notify();
                    }))
                    .child(div().size(px(8.)).rounded_full().bg(rgb(dot)))
                    .child(label)
                    .child(div().text_color(rgb(MUTED)).child(if follow { "· auto-follow on" } else { "· auto-follow off" })),
            )
            .into_any_element()
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PlayerFilter {
    All,
    Otp,
    Pro,
}

/// How often the background refresh runs.
const AUTO_REFRESH: Duration = Duration::from_secs(4 * 3600);

/// The newest 6 patches ("16.23") from Data Dragon's versions.
fn patch_list(data: &StaticData) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for v in &data.versions {
        let p = v.split('.').take(2).collect::<Vec<_>>().join(".");
        if !out.contains(&p) {
            out.push(p);
        }
        if out.len() == 6 {
            break;
        }
    }
    out
}

/// Games shown at a time in the games list.
pub const GAMES_PAGE: usize = 10;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    A,
    B,
    C,
}

/// The lane a champion is played in most (ARAM is never in the lane map).
pub fn main_lane(lanes: &HashMap<Lane, LaneStats>) -> Option<Lane> {
    lanes.iter().max_by_key(|(_, s)| s.games.unwrap_or(0)).map(|(l, _)| *l)
}

fn patch_num(p: &str) -> (u32, u32) {
    let mut it = p.split('.').map(|x| x.parse().unwrap_or(0));
    (it.next().unwrap_or(0), it.next().unwrap_or(0))
}

impl Focusable for AppView {
    fn focus_handle(&self, _: &gpui::App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for AppView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        tips::reset_ids();
        let top = self.render_top_bar(cx);
        let update_card = self.update_card(cx);
        let body = if let Some(err) = &self.startup_error {
            div().p(px(24.)).text_color(rgb(RED)).child(format!("Couldn't load game data: {err}")).into_any_element()
        } else if self.data.is_none() {
            div().p(px(24.)).text_color(rgb(MUTED)).child("Loading champion data…").into_any_element()
        } else {
            match self.tab {
                Tab::TierList => self.render_tier_list(window, cx),
                Tab::Matchup => self.render_matchup(window, cx),
                Tab::Counters => self.render_counters(window, cx),
                Tab::LiveGame => self.render_live_game(cx),
                Tab::Settings => self.render_settings(cx),
            }
        };
        let toast = self.toast.clone().map(|(msg, ok)| {
            div()
                .absolute()
                .bottom(px(18.))
                .left(px(18.))
                .max_w(px(560.))
                .px(px(14.))
                .py(px(10.))
                .rounded(px(8.))
                .bg(rgb(PANEL_2))
                .border_1()
                .border_color(rgb(if ok { ACCENT } else { RED }))
                .text_size(px(13.))
                .text_color(rgb(TEXT))
                .child(msg)
        });
        div()
            .size_full()
            .relative()
            .flex()
            .flex_col()
            .bg(rgb(BG))
            .text_color(rgb(TEXT))
            .font_family("Segoe UI")
            .track_focus(&self.focus)
            .on_mouse_down(MouseButton::Left, |_, _, _| {})
            .child(top)
            .children(update_card)
            // One scroll position per tab.
            .child(
                div()
                    .id(match self.tab {
                        Tab::TierList => "body-tierlist",
                        Tab::Matchup => "body-matchup",
                        Tab::Counters => "body-counters",
                        Tab::LiveGame => "body-livegame",
                        Tab::Settings => "body-settings",
                    })
                    .flex_1()
                    .overflow_y_scroll()
                    .child(body),
            )
            .children(toast)
    }
}
