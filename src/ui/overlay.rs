//! In-game overlays (toggled in Settings): Quick Stats, Skill Order, Item
//! Build, Jungle Timers and Lane Gold Difference. They're drawn on one
//! transparent, click-through window laid over the League game window, and
//! only while the game is in front. Data comes from the game's Live Client
//! Data API (see `crate::live`) and Poro's matchup builds.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui::{
    div, prelude::*, px, AnyElement, App, Bounds, Context, Div, FontWeight, Point, Size, Window,
    WindowBackgroundAppearance, WindowBounds, WindowHandle, WindowKind, WindowOptions,
};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

use super::theme::*;
use super::AppView;
use crate::data::ddragon::{Lane, Skill, StaticData};
use crate::data::{deeplol, matchup_builds};
use crate::hud::{Hud, BARON_PIT, DRAGON_PIT};
use crate::live::{self, GameData, Player};
use crate::settings::Overlays;
use crate::{images, win};

#[derive(Default)]
pub struct OverlayState {
    pub live: Option<GameData>,
    /// Showing demo data from Settings until then.
    pub preview_until: Option<Instant>,
    /// The recommended build for (my champion, lane opponent).
    pub build: Option<matchup_builds::MatchupBuild>,
    build_for: Option<(u32, Option<u32>)>,
    window: Option<(WindowHandle<OverlayView>, win::Hwnd)>,
    placed: Option<(i32, i32, i32, i32)>,
    /// Tab is held in the game.
    pub tab: bool,
    /// The size GPUI was last told about.
    sized_for: Option<(i32, i32)>,
}

impl OverlayState {
    pub fn previewing(&self) -> bool {
        self.preview_until.is_some_and(|t| Instant::now() < t)
    }
}

impl AppView {
    /// Polls the game once a second while any overlay is on.
    pub(super) fn start_overlay_loop(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| loop {
            let Ok((want, preview)) =
                this.update(cx, |this, _| (this.settings.overlays.any(), this.ov.previewing()))
            else {
                break;
            };
            let live = if want && !preview {
                cx.background_executor().spawn(async { live::fetch().ok() }).await
            } else {
                None
            };
            if this.update(cx, |this, cx| this.on_live(live, cx)).is_err() {
                break;
            }
            let wait = if want || preview { 1000 } else { 3000 };
            cx.background_executor().timer(Duration::from_millis(wait)).await;
        })
        .detach();
    }

    /// Watches the Tab key (the scoreboard) while the gold overlay is showing.
    pub(super) fn start_tab_watch(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(Duration::from_millis(50)).await;
            let alive = this.update(cx, |this, cx| {
                this.sync_overlay(cx);
                let watching = this.settings.overlays.lane_gold && this.ov.placed.is_some() && !this.ov.previewing();
                let down = watching && win::key_down(win::VK_TAB_KEY);
                if down != this.ov.tab {
                    this.ov.tab = down;
                    this.push_overlay(cx);
                }
            });
            if alive.is_err() {
                break;
            }
        })
        .detach();
    }

    fn push_overlay(&mut self, cx: &mut Context<Self>) {
        if let Some((handle, _)) = self.ov.window {
            let snap = if self.ov.placed.is_some() { self.overlay_snapshot() } else { None };
            let _ = handle.update(cx, |view, _, cx| {
                view.snap = snap;
                cx.notify();
            });
        }
    }

    pub(super) fn preview_overlays(&mut self, cx: &mut Context<Self>) {
        self.ov.preview_until = Some(Instant::now() + Duration::from_secs(20));
        self.on_live(None, cx);
    }

    fn on_live(&mut self, live: Option<GameData>, cx: &mut Context<Self>) {
        let preview = self.ov.previewing();
        if !preview {
            self.ov.preview_until = None;
        }
        self.ov.live = if preview { Some(demo_game()) } else { live };

        if let Some(game) = self.ov.live.clone() {
            self.update_overlay_build(&game, cx);
        }
        self.sync_overlay(cx);
        self.push_overlay(cx);
    }

    /// Shows the overlay exactly over the League game while the game is the
    /// window in front, and hides it as soon as it isn't (alt-tab, minimise).
    /// Runs every 50 ms so it follows the game window right away.
    fn sync_overlay(&mut self, cx: &mut Context<Self>) {
        let rect = if self.ov.previewing() {
            Some(win::primary_screen())
        } else if self.ov.live.is_some() && self.settings.overlays.any() {
            win::game_window().and_then(|g| {
                let fg = win::foreground();
                let ours = self.ov.window.map(|(_, h)| h);
                (fg == g || Some(fg) == ours).then(|| win::client_rect(g)).flatten()
            })
        } else {
            None
        };
        if rect.is_some() && self.ov.window.is_none() {
            self.open_overlay_window(cx);
        }
        let Some((handle, hwnd)) = self.ov.window else { return };
        if rect != self.ov.placed {
            win::place(hwnd, rect);
            let was_hidden = self.ov.placed.is_none();
            self.ov.placed = rect;
            if let Some((_, _, w, h)) = rect {
                if self.ov.sized_for != Some((w, h)) {
                    // GPUI needs telling about the new size; its resize also
                    // adds a frame, so the window is put back in place next tick.
                    self.ov.sized_for = Some((w, h));
                    let _ = handle.update(cx, |_, window, _| {
                        let sf = window.scale_factor();
                        window.resize(Size::new(px(w as f32 / sf), px(h as f32 / sf)));
                    });
                }
            }
            if was_hidden {
                self.push_overlay(cx);
            }
        } else if rect.is_some() && win::window_rect(hwnd) != rect {
            // Something (GPUI's own resize) moved it: put it back over the game.
            win::place(hwnd, rect);
        }
    }

    /// Loads the build for my champion vs my lane opponent when they change.
    fn update_overlay_build(&mut self, game: &GameData, cx: &mut Context<Self>) {
        let Some(data) = self.data.clone() else { return };
        let Some(me) = game.me() else { return };
        let Some(my_key) = data.champion_by_id(me.champion_id()).map(|c| c.key) else { return };
        let opp_key = game.opponent().and_then(|o| data.champion_by_id(o.champion_id())).map(|c| c.key);
        if self.ov.build_for == Some((my_key, opp_key)) {
            return;
        }
        self.ov.build_for = Some((my_key, opp_key));
        self.ov.build = None;
        self.load_skills(my_key, cx);
        let lane = Lane::from_client(&me.position.to_lowercase());
        let (patch, _) = self.deeplol_patch();
        super::spawn_job(
            cx,
            move || {
                let lanes = deeplol::champion(my_key, &patch, true).unwrap_or_default();
                let lane = lane.or_else(|| super::main_lane(&lanes)).unwrap_or(Lane::Middle);
                let general = lanes.get(&lane).map(|s| s.build_lst.clone()).unwrap_or_default();
                let games = match opp_key {
                    Some(o) => matchup_builds::all_games(my_key, o).unwrap_or_default(),
                    None => Vec::new(),
                };
                Ok(matchup_builds::summarize(&games, &data, &general, 1).builds.into_iter().next())
            },
            move |this, r, _| {
                if this.ov.build_for == Some((my_key, opp_key)) {
                    this.ov.build = r.ok().flatten();
                }
            },
        );
    }

    fn overlay_snapshot(&self) -> Option<Snapshot> {
        let data = self.data.clone()?;
        let game = self.ov.live.clone()?;
        let mut on = self.settings.overlays;
        if self.ov.previewing() {
            on = Overlays { quick_stats: true, skill_order: true, item_build: true, jungle_timers: true, lane_gold: true };
        }
        let my_key = game.me().and_then(|m| data.champion_by_id(m.champion_id())).map(|c| c.key);
        let skills = my_key.and_then(|k| self.skills.get(&k).cloned()).unwrap_or_default();
        let tab = self.ov.tab || self.ov.previewing();
        Some(Snapshot { data, game, on, build: self.ov.build.clone(), skills, tab, placed: self.ov.placed })
    }

    fn open_overlay_window(&mut self, cx: &mut Context<Self>) {
        let hwnd = Rc::new(Cell::new(0isize));
        let hwnd_out = hwnd.clone();
        let (x, y, w, h) = win::primary_screen();
        let opened = cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: Point::new(px(x as f32), px(y as f32)),
                    size: Size::new(px(w as f32), px(h as f32)),
                })),
                titlebar: None,
                focus: false,
                show: false,
                kind: WindowKind::PopUp,
                is_movable: false,
                is_resizable: false,
                is_minimizable: false,
                window_background: WindowBackgroundAppearance::Transparent,
                ..Default::default()
            },
            move |window, cx| {
                if let Ok(handle) = HasWindowHandle::window_handle(window) {
                    if let RawWindowHandle::Win32(h) = handle.as_raw() {
                        hwnd_out.set(h.hwnd.get());
                    }
                }
                cx.new(|_| OverlayView { snap: None })
            },
        );
        if let Ok(handle) = opened {
            let h = hwnd.get();
            win::make_overlay(h);
            self.ov.window = Some((handle, h));
            self.ov.placed = None;
        }
    }
}

pub struct OverlayView {
    snap: Option<Snapshot>,
}

/// What the overlay needs from the app, copied out each frame.
struct Snapshot {
    data: Arc<StaticData>,
    game: GameData,
    on: Overlays,
    build: Option<matchup_builds::MatchupBuild>,
    skills: Vec<Skill>,
    /// Tab (the scoreboard) is held.
    tab: bool,
    /// The overlay window's size in screen pixels.
    placed: Option<(i32, i32, i32, i32)>,
}

impl Render for OverlayView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let root = div().size_full().relative().font_family("Segoe UI");
        let Some(snap) = self.snap.take() else { return root };
        let size = window.viewport_size();
        // HUD positions are in game pixels; GPUI lays out in logical pixels.
        let sf = window.scale_factor();
        let (w, h) = (f32::from(size.width), f32::from(size.height));
        let hud = Hud::read((w * sf, h * sf));
        if std::env::var_os("PORO_DEBUG").is_some() {
            eprintln!("overlay viewport {w}x{h} scale {sf} placed {:?}", snap.placed);
        }
        let l = |v: f32| px(v / sf);
        let s = hud.s() / sf;
        let mut root = root;

        if snap.on.skill_order {
            // Bottom left, beside the stats panel next to the champion portrait.
            let (left_edge, bottom) = hud.hud_left();
            let width = 5.0 * 44.0 * s;
            root = root.child(
                div()
                    .absolute()
                    .left(px(left_edge / sf - width - 10.0 * s))
                    .bottom(px(h - bottom / sf))
                    .w(px(width))
                    .child(skill_order(&snap, s, cx)),
            );
        }
        if snap.on.item_build {
            // Just above the minimap.
            let m = hud.minimap();
            root = root.child(
                div()
                    .absolute()
                    .left(l(m.x))
                    .w(l(m.w))
                    .bottom(px(h - (m.y / sf) + 6.0 * s))
                    .child(item_build(&snap, s, cx)),
            );
        }
        if snap.on.jungle_timers {
            for (label, (x, y)) in jungle_timers(&snap, &hud) {
                root = root.child(
                    div()
                        .absolute()
                        .left(px(x / sf - 26.0 * s))
                        .top(px(y / sf - 9.0 * s))
                        .w(px(52.0 * s))
                        .flex()
                        .justify_center()
                        .child(outlined(label, 13.0 * s, GOLD)),
                );
            }
        }
        if snap.on.quick_stats {
            root = root.child(div().absolute().top(px(h * 0.16)).left(px(w - 236.0)).w(px(220.0)).child(quick_stats(&snap)));
        }
        if snap.on.lane_gold && snap.tab {
            root = root.child(div().absolute().top(px(h * 0.055)).left(px(w / 2. - 210.)).w(px(420.)).child(lane_gold(&snap)));
        }
        self.snap = Some(snap);
        root
    }
}

/// Text that stays readable on any background without a box behind it:
/// a dark copy offset in four directions, then the text on top.
fn outlined(text: String, size: f32, color: u32) -> AnyElement {
    let shadow = |dx: f32, dy: f32| {
        div()
            .absolute()
            .left(px(dx))
            .top(px(dy))
            .text_size(px(size))
            .font_weight(FontWeight::BOLD)
            .text_color(rgba(0x000000e6))
            .child(text.clone())
    };
    div()
        .relative()
        .child(shadow(-1.0, 0.0))
        .child(shadow(1.0, 0.0))
        .child(shadow(0.0, -1.0))
        .child(shadow(0.0, 1.0))
        .child(div().relative().text_size(px(size)).font_weight(FontWeight::BOLD).text_color(rgb(color)).child(text))
        .into_any_element()
}

fn panel(title: &str) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(6.))
        .text_color(rgb(TEXT))
        .child(outlined(title.to_string(), 12.0, ACCENT))
}

fn row(label: &str, value: String, color: u32) -> Div {
    div()
        .flex()
        .justify_between()
        .gap(px(14.))
        .child(outlined(label.to_string(), 12.0, 0xd0d2da))
        .child(outlined(value, 12.0, color))
}

fn mmss(secs: f64) -> String {
    let s = secs.max(0.0) as u64;
    format!("{}:{:02}", s / 60, s % 60)
}

/// Your own live numbers.
fn quick_stats(s: &Snapshot) -> AnyElement {
    let Some(me) = s.game.me() else { return div().into_any_element() };
    let minutes = (s.game.game_data.game_time / 60.0).max(1.0 / 60.0);
    let sc = &me.scores;
    let cs_min = sc.creep_score as f64 / minutes;
    let kda = (sc.kills + sc.assists) as f64 / sc.deaths.max(1) as f64;
    let team_kills = s.game.team_kills();
    let kp = if team_kills == 0 { 0.0 } else { (sc.kills + sc.assists) as f64 * 100.0 / team_kills as f64 };
    // Rough targets: 7+ CS/min is strong, under 5 is low (not for supports/junglers).
    let cs_color = if cs_min >= 7.0 { ACCENT } else if cs_min < 5.0 { RED } else { TEXT };
    panel("Quick Stats")
        .w_full()
        .child(row("Game time", mmss(s.game.game_data.game_time), TEXT))
        .child(row("CS", format!("{} ({cs_min:.1}/min)", sc.creep_score), cs_color))
        .child(row("KDA", format!("{}/{}/{} ({kda:.2})", sc.kills, sc.deaths, sc.assists), if kda >= 3.0 { ACCENT } else { TEXT }))
        .child(row("Kill participation", format!("{kp:.0}%"), if kp >= 50.0 { ACCENT } else { TEXT }))
        .child(row("Vision score", format!("{:.0}", sc.ward_score), TEXT))
        .child(row("Gold", format!("{:.0}", s.game.active_player.current_gold), GOLD))
        .into_any_element()
}

/// The next five skill points from the recommended build.
fn skill_order(s: &Snapshot, k: f32, cx: &mut App) -> AnyElement {
    let Some(b) = &s.build else { return outlined("Loading build…".into(), 11.0 * k, MUTED) };
    let level = s.game.active_player.level as usize;
    let next: Vec<(usize, u32)> = b.skills.iter().copied().enumerate().skip(level).take(5).collect();
    if next.is_empty() {
        return div().into_any_element();
    }
    let cells: Vec<AnyElement> = next
        .iter()
        .enumerate()
        .map(|(i, (lv, skill))| {
            let icon = s.skills.get(*skill as usize).map(|sk| sk.icon.clone());
            let first = i == 0;
            div()
                .flex()
                .flex_col()
                .items_center()
                .gap(px(2.0 * k))
                .child(outlined(skill_letter(*skill).to_string(), 13.0 * k, if first { ACCENT } else { 0xd0d2da }))
                .child(
                    div()
                        .rounded(px(4.0 * k))
                        .border_2()
                        .border_color(rgb(if first { ACCENT } else { 0x000000 }))
                        .when(!first, |d| d.opacity(0.6))
                        .child(images::icon(icon.as_deref(), 32.0 * k, cx)),
                )
                .child(outlined(format!("{}", lv + 1), 10.0 * k, 0xd0d2da))
                .into_any_element()
        })
        .collect();
    div().flex().justify_between().children(cells).into_any_element()
}

/// Starting items in the first minutes, then the core build; owned ones are ticked.
fn item_build(s: &Snapshot, k: f32, cx: &mut App) -> AnyElement {
    let Some(b) = &s.build else { return outlined("Loading build…".into(), 11.0 * k, MUTED) };
    let owned: Vec<u32> = s.game.me().map(|m| m.items.iter().map(|i| i.item_id).collect()).unwrap_or_default();
    let icon = |id: u32, cx: &mut App| {
        let have = owned.contains(&id);
        div()
            .rounded(px(4.0 * k))
            .border_2()
            .border_color(rgb(if have { ACCENT } else { 0x000000 }))
            .when(have, |d| d.opacity(0.5))
            .child(images::icon(Some(&s.data.item_icon(id)), 28.0 * k, cx))
            .into_any_element()
    };
    let early = s.game.game_data.game_time < 180.0;
    let ids: Vec<u32> = if early { b.starter.clone() } else { b.items.iter().chain(b.boots.iter()).copied().collect() };
    let title = if early { format!("Start · {}", b.style) } else { format!("Core · {}", b.style) };
    let icons: Vec<AnyElement> = ids.into_iter().map(|i| icon(i, cx)).collect();
    div()
        .flex()
        .flex_col()
        .gap(px(3.0 * k))
        .child(outlined(title, 11.0 * k, ACCENT))
        .child(div().flex().flex_wrap().gap(px(3.0 * k)).children(icons))
        .into_any_element()
}

/// Dragon and Baron countdowns from the game's own kill announcements,
/// placed on their pits in the minimap (nothing once they're up).
fn jungle_timers(s: &Snapshot, hud: &Hud) -> Vec<(String, (f32, f32))> {
    let t = s.game.game_data.game_time;
    let (mine, theirs) = s.game.dragons();
    let soul = mine >= 4 || theirs >= 4;
    let dragon_next = match s.game.last_event(&["DragonKill"]) {
        Some(e) => e.time + if soul { 360.0 } else { 300.0 },
        None => 300.0,
    };
    let baron_next = match s.game.last_event(&["BaronKill"]) {
        Some(e) => e.time + 360.0,
        None => 1200.0,
    };
    let mut out = Vec::new();
    for (next, pit) in [(dragon_next, DRAGON_PIT), (baron_next, BARON_PIT)] {
        if t < next {
            out.push((mmss(next - t), hud.map_point(pit.0, pit.1)));
        }
    }
    out
}

/// Gold in each player's items ("realized gold"), lane by lane.
fn lane_gold(s: &Snapshot) -> AnyElement {
    let gold = |p: Option<&Player>| -> i64 {
        p.map(|p| {
            p.items
                .iter()
                .map(|i| s.data.items.get(&i.item_id).map(|x| x.gold as i64).unwrap_or(0) * i.count.max(1) as i64)
                .sum()
        })
        .unwrap_or(0)
    };
    let lanes = s.game.lanes();
    let mut team = 0i64;
    let cells: Vec<AnyElement> = lanes
        .iter()
        .map(|(label, a, e)| {
            let diff = gold(*a) - gold(*e);
            team += diff;
            div()
                .flex()
                .flex_col()
                .items_center()
                .child(div().text_size(px(10.)).text_color(rgb(MUTED)).child(label.to_string()))
                .child(div().text_size(px(13.)).font_weight(FontWeight::BOLD).text_color(rgb(gold_color(diff))).child(signed_gold(diff)))
                .into_any_element()
        })
        .collect();
    panel("Lane Gold Difference")
        .child(div().flex().justify_between().children(cells))
        .child(
            div()
                .flex()
                .justify_center()
                .gap(px(6.))
                .text_size(px(12.))
                .child(div().text_color(rgb(MUTED)).child("Team"))
                .child(div().font_weight(FontWeight::BOLD).text_color(rgb(gold_color(team))).child(signed_gold(team))),
        )
        .into_any_element()
}

fn gold_color(diff: i64) -> u32 {
    if diff > 300 {
        ACCENT
    } else if diff < -300 {
        RED
    } else {
        TEXT
    }
}

fn signed_gold(v: i64) -> String {
    let sign = if v > 0 { "+" } else if v < 0 { "−" } else { "" };
    format!("{sign}{}", thousands(v.unsigned_abs()))
}

/// Sample data for the Settings preview.
fn demo_game() -> GameData {
    use live::*;
    let p = |name: &str, champ: &str, team: &str, pos: &str, items: &[u32], k: u32, d: u32, a: u32, cs: u32| Player {
        riot_id: format!("{name}#DEMO"),
        summoner_name: name.into(),
        raw_champion_name: format!("game_character_displayname_{champ}"),
        champion_name: champ.into(),
        team: team.into(),
        position: pos.into(),
        level: 9,
        items: items.iter().map(|&i| Item { item_id: i, count: 1 }).collect(),
        scores: Scores { kills: k, deaths: d, assists: a, creep_score: cs, ward_score: 11.0 },
    };
    let all_players = vec![
        p("You", "Orianna", "ORDER", "MIDDLE", &[6655, 3020, 1052], 3, 1, 5, 121),
        p("Ally1", "Ornn", "ORDER", "TOP", &[3068, 3047], 1, 2, 3, 104),
        p("Ally2", "LeeSin", "ORDER", "JUNGLE", &[6692, 3111], 4, 2, 6, 78),
        p("Ally3", "Jinx", "ORDER", "BOTTOM", &[3031, 3006, 1036], 2, 1, 4, 130),
        p("Ally4", "Thresh", "ORDER", "UTILITY", &[3190], 0, 2, 9, 18),
        p("Enemy1", "Darius", "CHAOS", "TOP", &[6631, 3047, 1037], 3, 1, 1, 112),
        p("Enemy2", "Nocturne", "CHAOS", "JUNGLE", &[3142], 2, 3, 2, 70),
        p("Enemy3", "Leblanc", "CHAOS", "MIDDLE", &[6655, 3020], 2, 3, 2, 115),
        p("Enemy4", "Kaisa", "CHAOS", "BOTTOM", &[3124, 3006], 1, 3, 2, 122),
        p("Enemy5", "Nautilus", "CHAOS", "UTILITY", &[3190, 3047], 0, 2, 4, 20),
    ];
    GameData {
        active_player: ActivePlayer { riot_id: "You#DEMO".into(), summoner_name: "You".into(), level: 9, current_gold: 1240.0 },
        all_players,
        events: Events {
            events: vec![Event { name: "DragonKill".into(), time: 610.0, killer: "Ally2#DEMO".into(), dragon_type: "Fire".into() }],
        },
        game_data: GameInfo { game_mode: "CLASSIC".into(), game_time: 840.0, map_number: 11 },
    }
}
