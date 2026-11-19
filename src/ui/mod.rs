//! The app window: a Champion Matchup tab (deeplol) and a Counters tab
//! (lolalytics), a patch picker and the League client connection.

mod counters;
mod games;
mod matchup;
mod theme;
mod tips;

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
    Matchup,
    Counters,
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
    pub counters: Loadable<Counters>,
    pub sort: (SortCol, bool),
    c_gen: u64,

    // League client
    pub client: ClientStatus,
    pub follow_client: bool,
    last_followed: Option<ChampSelect>,
    lcu: Arc<Mutex<Option<Lcu>>>,
    pub toast: Option<(String, bool)>,
    toast_gen: u64,
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
        for input in [&input_a, &input_b, &input_c] {
            cx.observe(input, |_, _, cx| cx.notify()).detach();
        }
        cx.subscribe(&input_a, |this, _, _: &Submitted, cx| this.pick_first(Slot::A, cx)).detach();
        cx.subscribe(&input_b, |this, _, _: &Submitted, cx| this.pick_first(Slot::B, cx)).detach();
        cx.subscribe(&input_c, |this, _, _: &Submitted, cx| this.pick_first(Slot::C, cx)).detach();
        window.focus(&input_a.focus_handle(cx));

        let mut view = AppView {
            focus: cx.focus_handle(),
            logo: Arc::new(gpui::Image::from_bytes(gpui::ImageFormat::Png, include_bytes!("../../assets/poro.png").to_vec())),
            data: None,
            startup_error: None,
            dl_versions: None,
            patches: Vec::new(),
            patch: 0,
            tab: Tab::Matchup,
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
            counters: Loadable::Idle,
            sort: (SortCol::VsWr, true),
            c_gen: 0,
            client: ClientStatus::NotRunning,
            follow_client: true,
            last_followed: None,
            lcu: Arc::new(Mutex::new(None)),
            toast: None,
            toast_gen: 0,
        };
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
                    this.patches = data
                        .versions
                        .iter()
                        .map(|v| v.split('.').take(2).collect::<Vec<_>>().join("."))
                        .fold(Vec::new(), |mut acc: Vec<String>, v| {
                            if !acc.contains(&v) {
                                acc.push(v);
                            }
                            acc
                        })
                        .into_iter()
                        .take(6)
                        .collect();
                    this.data = Some(Arc::new(data));
                    this.dl_versions = dl;
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
            self.history = Loadable::Loading;
            spawn_job(cx, move || matchup_builds::all_games(a, b), move |this, r, _| {
                if this.gen == gen {
                    this.history = Loadable::from(r);
                }
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
            format!("{} recent games ({})", games.len(), matchup_builds::patch_breakdown(games))
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
        cx.spawn(async move |this, cx| loop {
            let lcu = lcu.clone();
            let status = cx
                .background_executor()
                .spawn(async move {
                    let mut guard = lcu.lock().unwrap();
                    if guard.as_ref().is_some_and(|l| !l.alive()) {
                        *guard = None;
                    }
                    if guard.is_none() {
                        *guard = Lcu::connect().ok();
                    }
                    match guard.as_ref() {
                        None => ClientStatus::NotRunning,
                        Some(l) => match l.champ_select() {
                            Some(cs) => ClientStatus::ChampSelect(cs),
                            None => ClientStatus::Connected,
                        },
                    }
                })
                .await;
            let alive = this.update(cx, |this, cx| this.on_client_status(status, cx));
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
        if let ClientStatus::ChampSelect(cs) = &status {
            if self.follow_client && cs.my_champion != 0 && self.last_followed.as_ref() != Some(cs) {
                self.last_followed = Some(cs.clone());
                if self.a != Some(cs.my_champion) {
                    self.set_champion(Slot::A, cs.my_champion, cx);
                }
                if let Some(opp) = cs.lane_opponent {
                    if self.b != Some(opp) {
                        self.set_champion(Slot::B, opp, cx);
                    }
                }
                if let Some(lane) = cs.my_lane {
                    self.lane = lane;
                    self.lane_auto = false;
                }
                self.tab = Tab::Matchup;
            }
        } else {
            self.last_followed = None;
        }
        self.client = status;
        cx.notify();
    }

    /// Puts `page` (and optionally summoner spells) into the League client.
    pub fn apply(&mut self, name: String, page: RunePage, spells: Option<(u32, u32)>, cx: &mut Context<Self>) {
        let lcu = self.lcu.clone();
        let in_champ_select = matches!(self.client, ClientStatus::ChampSelect(_));
        self.show_toast("Sending runes to the League client…".into(), true, cx);
        spawn_job(
            cx,
            move || {
                let mut guard = lcu.lock().unwrap();
                if guard.is_none() {
                    *guard = Some(Lcu::connect()?);
                }
                let l = guard.as_ref().unwrap();
                l.apply_runes(&name, &page)?;
                let mut msg = format!("Rune page \"{name}\" is now active");
                if let (Some((s1, s2)), true) = (spells, in_champ_select) {
                    match l.apply_spells(s1, s2) {
                        Ok(()) => msg.push_str(", summoner spells set"),
                        Err(e) => msg.push_str(&format!(" (spells not set: {e})")),
                    }
                }
                Ok(msg)
            },
            |this, r, cx| match r {
                Ok(msg) => this.show_toast(msg, true, cx),
                Err(e) => this.show_toast(format!("Couldn't apply runes: {e:#}"), false, cx),
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
                    this.tab = t;
                    cx.notify();
                }))
                .child(label)
        };
        let patches = self
            .patches
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let active = i == self.patch;
                div()
                    .id(SharedString::from(format!("patch-{p}")))
                    .px(px(9.))
                    .py(px(4.))
                    .rounded(px(5.))
                    .text_size(px(12.))
                    .cursor_pointer()
                    .border_1()
                    .when(active, |d| d.border_color(rgb(ACCENT)).text_color(rgb(ACCENT)).bg(rgba(0x37c8ab22)))
                    .when(!active, |d| d.border_color(rgb(BORDER)).text_color(rgb(MUTED)).hover(|s| s.text_color(rgb(TEXT))))
                    .on_click(cx.listener(move |this, _, _, cx| this.set_patch(i, cx)))
                    .child(if i == 0 { format!("{p} (live)") } else { p.clone() })
            })
            .collect::<Vec<_>>();

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
                    if let Some(o) = cs.lane_opponent {
                        s = format!("{s} vs {}", name(o));
                    }
                }
                (GOLD, s)
            }
        };
        let follow = self.follow_client;

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
            .child(div().text_size(px(17.)).font_weight(gpui::FontWeight::BOLD).text_color(rgb(TEXT)).mr(px(10.)).child("Poro"))
            .child(tab("Champion Matchup", Tab::Matchup, self, cx))
            .child(tab("Counters", Tab::Counters, self, cx))
            .child(div().flex_1())
            .child(div().text_size(px(12.)).text_color(rgb(MUTED)).child("Patch"))
            .children(patches)
            .child(div().w(px(10.)))
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
        let body = if let Some(err) = &self.startup_error {
            div().p(px(24.)).text_color(rgb(RED)).child(format!("Couldn't load game data: {err}")).into_any_element()
        } else if self.data.is_none() {
            div().p(px(24.)).text_color(rgb(MUTED)).child("Loading champion data…").into_any_element()
        } else {
            match self.tab {
                Tab::Matchup => self.render_matchup(window, cx),
                Tab::Counters => self.render_counters(window, cx),
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
            // One scroll position per tab.
            .child(
                div()
                    .id(if self.tab == Tab::Matchup { "body-matchup" } else { "body-counters" })
                    .flex_1()
                    .overflow_y_scroll()
                    .child(body),
            )
            .children(toast)
    }
}
