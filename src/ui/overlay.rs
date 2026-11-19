//! In-game overlays (toggled in Settings): Quick Stats, Skill Order, Item
//! Build and Lane Gold Difference. They're drawn on one
//! transparent, click-through window laid over the League game window, and
//! only while the game is in front. Data comes from the game's Live Client
//! Data API (see `crate::live`) and Poro's matchup builds.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui::{
    div, prelude::*, px, AnyElement, App, Bounds, Context, FontWeight, Point, Size, Window,
    WindowBackgroundAppearance, WindowBounds, WindowHandle, WindowKind, WindowOptions,
};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

use super::theme::*;
use super::AppView;
use crate::data::ddragon::{Lane, Skill, StaticData};
use crate::data::{deeplol, matchup_builds};
use crate::hud::Hud;
use crate::live::{self, GameData, Player};
use crate::settings::Overlays;
use crate::{images, win};

/// Polls (one a second) without an answer before a game counts as over.
const GAME_END_MISSES: u32 = 4;

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
    /// A game is running (the game's live data answers).
    pub in_game: bool,
    /// Polls in a row without an answer from the game.
    misses: u32,
}

impl OverlayState {
    /// Reload the in-game build on the next poll (background refresh).
    pub fn refresh_build(&mut self) {
        self.build_for = None;
    }

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

        // Game start / end. The game answers once it has loaded; a few
        // missed polls in a row (not just one hiccup) mean it has ended.
        if !preview {
            match (&live, self.ov.in_game) {
                (Some(_), false) => self.on_game_start(),
                (Some(_), true) => self.ov.misses = 0,
                (None, true) => {
                    self.ov.misses += 1;
                    if self.ov.misses >= GAME_END_MISSES {
                        self.on_game_end(cx);
                    }
                }
                (None, false) => {}
            }
        }
        // Keep the last data through a short hiccup, so nothing flickers.
        self.ov.live = if preview {
            Some(demo_game())
        } else if live.is_some() || !self.ov.in_game {
            live
        } else {
            self.ov.live.take()
        };

        if let Some(game) = self.ov.live.clone() {
            self.update_overlay_build(&game, cx);
        }
        self.sync_overlay(cx);
        self.push_overlay(cx);
    }

    fn on_game_start(&mut self) {
        if std::env::var_os("PORO_DEBUG").is_some() {
            eprintln!("overlay: game started");
        }
        self.ov.in_game = true;
        self.ov.misses = 0;
        // Fresh state for the new game.
        self.ov.build = None;
        self.ov.build_for = None;
        self.ov.tab = false;
    }

    /// The game is over: close the overlay window and forget the game.
    fn on_game_end(&mut self, cx: &mut Context<Self>) {
        if std::env::var_os("PORO_DEBUG").is_some() {
            eprintln!("overlay: game ended");
        }
        self.ov.in_game = false;
        self.ov.misses = 0;
        self.ov.live = None;
        self.ov.build = None;
        self.ov.build_for = None;
        self.ov.tab = false;
        if let Some((handle, _)) = self.ov.window.take() {
            let _ = handle.update(cx, |_, window, _| window.remove_window());
        }
        self.ov.placed = None;
        self.ov.sized_for = None;
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
        if rect != self.ov.placed && std::env::var_os("PORO_DEBUG").is_some() {
            eprintln!("overlay: place {:?} -> {:?} (preview {}, live {})", self.ov.placed, rect, self.ov.previewing(), self.ov.live.is_some());
        }
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
            on = Overlays { quick_stats: true, skill_order: true, item_build: true, lane_gold: true };
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
        if snap.on.quick_stats {
            let qw = 214.0 * s;
            root = root.child(
                div().absolute().top(px(34.0 * s)).left(px(w - qw - 16.0 * s)).w(px(qw)).child(quick_stats(&snap, s, cx)),
            );
        }
        if snap.on.lane_gold && snap.tab {
            // Under each player's K/D/A: coin, difference, arrow (like U.GG).
            let gold = lane_gold(&snap, &hud);
            for &(x, y, diff) in &gold.players {
                root = root.child(
                    div()
                        .absolute()
                        .left(px(x / sf - 60.0 * s))
                        .top(px(y / sf))
                        .w(px(120.0 * s))
                        .flex()
                        .justify_center()
                        .child(gold_tag(diff, s)),
                );
            }
            // The team totals bar under the scoreboard.
            let (bx, by) = hud.scoreboard_bar();
            let bw = 290.0 * s;
            root = root.child(
                div().absolute().left(px(bx / sf - bw / 2.0)).top(px(by / sf)).w(px(bw)).child(team_gold_bar(&gold, s)),
            );
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

fn mmss(secs: f64) -> String {
    let s = secs.max(0.0) as u64;
    format!("{}:{:02}", s / 60, s % 60)
}

/// Item gold ("realized gold") a player is carrying.
fn item_gold(data: &StaticData, p: &Player) -> i64 {
    p.items.iter().map(|i| data.items.get(&i.item_id).map(|x| x.gold as i64).unwrap_or(0) * i.count.max(1) as i64).sum()
}

/// Your numbers next to your lane opponent's, like U.GG's quick stats:
/// label, yours, then an arrow (blue: you're ahead, red: behind) and theirs.
fn quick_stats(s: &Snapshot, k: f32, cx: &mut App) -> AnyElement {
    let Some(me) = s.game.me() else { return div().into_any_element() };
    let opp = s.game.opponent();
    let minutes = (s.game.game_data.game_time / 60.0).max(1.0);
    let team_kills = |p: &Player| -> u32 {
        s.game.all_players.iter().filter(|q| q.team == p.team).map(|q| q.scores.kills).sum()
    };
    let kp = |p: &Player| -> f64 {
        let t = team_kills(p);
        if t == 0 { 0.0 } else { (p.scores.kills + p.scores.assists) as f64 * 100.0 / t as f64 }
    };
    struct Stat {
        label: &'static str,
        mine: f64,
        theirs: Option<f64>,
        fmt: fn(f64) -> String,
    }
    let int = |v: f64| format!("{v:.0}");
    let one = |v: f64| format!("{v:.1}");
    let pct = |v: f64| format!("{v:.0}%");
    let stats = [
        Stat {
            label: "Gold / min",
            mine: item_gold(&s.data, me) as f64 / minutes,
            theirs: opp.map(|o| item_gold(&s.data, o) as f64 / minutes),
            fmt: int,
        },
        Stat { label: "CS / min", mine: me.scores.creep_score as f64 / minutes, theirs: opp.map(|o| o.scores.creep_score as f64 / minutes), fmt: one },
        Stat { label: "Kill Par.", mine: kp(me), theirs: opp.map(kp), fmt: pct },
        Stat { label: "Vision / min", mine: me.scores.ward_score / minutes, theirs: opp.map(|o| o.scores.ward_score / minutes), fmt: one },
        Stat { label: "Level", mine: me.level as f64, theirs: opp.map(|o| o.level as f64), fmt: int },
    ];
    let rows: Vec<AnyElement> = stats
        .iter()
        .map(|st| {
            let (arrow, color) = match st.theirs {
                Some(t) if (st.fmt)(st.mine) != (st.fmt)(t) && st.mine > t => ("▲", BLUE_TEAM),
                Some(t) if (st.fmt)(st.mine) != (st.fmt)(t) => ("▼", RED_TEAM),
                _ => ("-", MUTED),
            };
            let theirs = st.theirs.map(st.fmt).unwrap_or_else(|| "–".into());
            div()
                .flex()
                .items_center()
                .h(px(20.0 * k))
                .child(div().flex_1().child(outlined(st.label.to_string(), 12.0 * k, 0xffffff)))
                .child(div().w(px(48.0 * k)).flex().justify_end().child(outlined((st.fmt)(st.mine), 12.0 * k, 0xc9ccd4)))
                .child(div().w(px(18.0 * k)).flex().justify_center().child(arrow_glyph(arrow, 10.0 * k, color)))
                .child(div().w(px(48.0 * k)).flex().justify_end().child(outlined(theirs, 12.0 * k, 0xc9ccd4)))
                .into_any_element()
        })
        .collect();
    let opp_icon = opp
        .and_then(|o| s.data.champion_by_id(o.champion_id()))
        .and_then(|c| s.data.champion_icon(c.key));
    let header = div()
        .flex()
        .items_center()
        .gap(px(6.0 * k))
        .px(px(6.0 * k))
        .h(px(24.0 * k))
        .rounded(px(4.0 * k))
        .bg(rgba(0x0b0f17b3))
        .child(gpui::img(poro_logo()).size(px(16.0 * k)))
        .child(outlined("Poro".into(), 12.0 * k, 0xffffff))
        .child(div().flex_1())
        .child(outlined("vs".into(), 10.0 * k, MUTED))
        .child(images::round(opp_icon.as_deref(), 16.0 * k, cx))
        .child(outlined(mmss(s.game.game_data.game_time), 11.0 * k, 0xffffff));
    div().flex().flex_col().gap(px(4.0 * k)).child(header).child(div().px(px(4.0 * k)).children(rows)).into_any_element()
}

/// A small ▲/▼ without an outline (an outline smears glyphs this small).
fn arrow_glyph(glyph: &str, size: f32, color: u32) -> AnyElement {
    div().font_family("Segoe UI Symbol").text_size(px(size)).text_color(rgb(color)).child(glyph.to_string()).into_any_element()
}

/// Poro's logo, loaded once.
fn poro_logo() -> std::sync::Arc<gpui::Image> {
    static LOGO: std::sync::OnceLock<std::sync::Arc<gpui::Image>> = std::sync::OnceLock::new();
    LOGO.get_or_init(|| {
        std::sync::Arc::new(gpui::Image::from_bytes(gpui::ImageFormat::Png, include_bytes!("../../assets/poro.png").to_vec()))
    })
    .clone()
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

/// Realized gold (the value of a player's items) for the scoreboard.
struct GoldView {
    /// (x, y, difference to the lane opponent) per player, at their K/D/A.
    players: Vec<(f32, f32, i64)>,
    /// Item gold of the team on the left and right halves of the scoreboard.
    left_total: i64,
    right_total: i64,
    /// My team is the left half.
    allies_left: bool,
}

/// Gold in each player's items ("realized gold"): for every player, the
/// difference to their lane opponent, at their row on the scoreboard.
fn lane_gold(s: &Snapshot, hud: &Hud) -> GoldView {
    let gold = |p: Option<&Player>| -> i64 {
        p.map(|p| {
            p.items
                .iter()
                .map(|i| s.data.items.get(&i.item_id).map(|x| x.gold as i64).unwrap_or(0) * i.count.max(1) as i64)
                .sum()
        })
        .unwrap_or(0)
    };
    let mut view = GoldView { players: Vec::new(), left_total: 0, right_total: 0, allies_left: true };
    let Some(me) = s.game.me() else { return view };
    // Blue (ORDER) is on the left unless the scoreboard is mirrored.
    let allies_left = (me.team == "ORDER") != hud.mirrored_scoreboard;
    view.allies_left = allies_left;
    let (mut ally_total, mut enemy_total) = (0, 0);
    for (row, (_, ally, enemy)) in s.game.lanes().into_iter().enumerate() {
        let (ga, ge) = (gold(ally), gold(enemy));
        ally_total += ga;
        enemy_total += ge;
        if ally.is_some() {
            let (x, y) = hud.scoreboard_kda(allies_left, row);
            view.players.push((x, y, ga - ge));
        }
        if enemy.is_some() {
            let (x, y) = hud.scoreboard_kda(!allies_left, row);
            view.players.push((x, y, ge - ga));
        }
    }
    (view.left_total, view.right_total) = if allies_left { (ally_total, enemy_total) } else { (enemy_total, ally_total) };
    view
}

const BLUE_TEAM: u32 = 0x3b82f6;
const RED_TEAM: u32 = 0xe5484d;

/// A small gold coin.
fn coin(k: f32) -> AnyElement {
    div()
        .w(px(12.0 * k))
        .h(px(9.0 * k))
        .rounded_full()
        .bg(rgb(0xf3c873))
        .border_1()
        .border_color(rgb(0xb8862f))
        .into_any_element()
}

/// Coin, difference and an up/down arrow, e.g. "+1150 ▲".
fn gold_tag(diff: i64, k: f32) -> AnyElement {
    let text = if diff > 0 { format!("+{diff}") } else { format!("{diff}") };
    let arrow = match diff.signum() {
        1 => Some(("▲", BLUE_TEAM)),
        -1 => Some(("▼", RED_TEAM)),
        _ => None,
    };
    div()
        .flex()
        .items_center()
        .gap(px(4.0 * k))
        .child(coin(k))
        .child(outlined(text, 11.0 * k, 0xffffff))
        .children(arrow.map(|(a, c)| arrow_glyph(a, 10.0 * k, c)))
        .into_any_element()
}

/// Team totals under the scoreboard: left total, the lead in the middle
/// (coloured for the team ahead, pointing at it), right total.
fn team_gold_bar(g: &GoldView, k: f32) -> AnyElement {
    let (left_color, right_color) = if g.allies_left { (BLUE_TEAM, RED_TEAM) } else { (RED_TEAM, BLUE_TEAM) };
    let lead = g.left_total - g.right_total;
    let total = |amount: i64, color: u32| {
        div()
            .flex_1()
            .h_full()
            .flex()
            .items_center()
            .justify_center()
            .gap(px(5.0 * k))
            .bg(rgba(0x10141ccc))
            .border_b_2()
            .border_color(rgb(color))
            .child(coin(k))
            .child(outlined(amount.to_string(), 11.0 * k, 0xffffff))
    };
    let (badge_color, label) = match lead.signum() {
        1 => (left_color, format!("◀ {}", lead)),
        -1 => (right_color, format!("{} ▶", -lead)),
        _ => (0x3a3f4b, "Even".to_string()),
    };
    div()
        .flex()
        .h(px(20.0 * k))
        .child(total(g.left_total, left_color))
        .child(
            div()
                .flex_1()
                .h_full()
                .flex()
                .items_center()
                .justify_center()
                .bg(rgb(badge_color))
                .child(outlined(label, 11.0 * k, 0xffffff)),
        )
        .child(total(g.right_total, right_color))
        .into_any_element()
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
        game_data: GameInfo { game_mode: "CLASSIC".into(), game_time: 840.0, map_number: 11 },
    }
}
