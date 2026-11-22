//! The Live Game tab, after U.GG's: the whole lobby during champ select
//! (your team's picks, spells, positions and ranks where the client shows
//! names; the enemy's locked picks with predicted roles), then both teams
//! with ranks, champion stats and tags once the loading screen starts.
//!
//! Only names the League client or U.GG show are used: ranked champ select
//! hides other players, and anonymous players stay anonymous.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use gpui::{div, prelude::*, px, AnyElement, Context, FontWeight, SharedString};

use super::theme::*;
use super::tips;
use super::{spawn_job, AppView, ClientStatus, Loadable};
use crate::data::ddragon::{Lane, StaticData};
use crate::data::ugg_live::{self, LiveGame, LivePlayer, RankScore};
use crate::images;
use crate::lcu::{ChampSelect, CsPlayer, Me};

/// U.GG only sees a game from the loading screen on; until then, look again this often.
const RETRY: Duration = Duration::from_secs(15);

#[derive(Default)]
pub struct LiveState {
    pub me: Option<Me>,
    me_loading: bool,
    /// U.GG's view of my current game (Ready(None) = it doesn't see one yet).
    pub game: Option<Loadable<Option<LiveGame>>>,
    fetched_at: Option<Instant>,
    /// Solo/duo ranks by lowercase "name#tag".
    ranks: HashMap<String, Loadable<Option<RankScore>>>,
}

fn in_game(phase: &str) -> bool {
    matches!(phase, "InProgress" | "Reconnect" | "GameStart")
}

fn queue_name(id: i64, fallback: &str) -> String {
    match id {
        420 => "Ranked Solo/Duo".into(),
        440 => "Ranked Flex".into(),
        400 => "Normal Draft".into(),
        430 => "Normal Blind".into(),
        490 => "Quickplay".into(),
        450 => "ARAM".into(),
        1700 => "Arena".into(),
        0 => fallback.replace('_', " "),
        _ => format!("Queue {id}"),
    }
}

fn lane_order(l: Option<Lane>) -> u8 {
    match l {
        Some(Lane::Top) => 0,
        Some(Lane::Jungle) => 1,
        Some(Lane::Middle) => 2,
        Some(Lane::Bottom) => 3,
        Some(Lane::Support) => 4,
        None => 5,
    }
}

fn role_name(l: Lane) -> &'static str {
    match l {
        Lane::Top => "Top",
        Lane::Jungle => "Jungle",
        Lane::Middle => "Mid",
        Lane::Bottom => "ADC",
        Lane::Support => "Support",
    }
}

fn tier_text_color(tier: &str) -> u32 {
    match tier {
        "iron" => 0x8c7b74,
        "bronze" => 0xb5795a,
        "silver" => 0xa3b1be,
        "gold" => 0xe3b35a,
        "platinum" => 0x4fb8b0,
        "emerald" => 0x3bc47c,
        "diamond" => 0x7a8cf5,
        "master" => 0xc76df0,
        "grandmaster" => 0xef5e5e,
        "challenger" => 0xf4c874,
        _ => MUTED,
    }
}

const PARTY_COLORS: [u32; 4] = [0xf2c14e, 0xb48cff, 0x5bd1f5, 0xf58fb4];

impl AppView {
    /// Keeps the data this tab needs coming: who I am, ranks of visible
    /// players, and U.GG's live game once I'm in one.
    pub(super) fn ensure_live(&mut self, cx: &mut Context<Self>) {
        // Testing: PORO_LIVE_AS="Name#TAG@kr" looks up that player's game instead.
        let live_as = std::env::var("PORO_LIVE_AS").ok().and_then(|v| {
            let (id, region) = v.rsplit_once('@')?;
            let (name, tag) = id.split_once('#')?;
            Some(Me { game_name: name.into(), tag_line: tag.into(), region: region.into(), summoner_id: 0 })
        });
        if live_as.is_some() && self.live.me.is_none() {
            self.live.me = live_as.clone();
        }
        let connected = !matches!(self.client, ClientStatus::NotRunning);
        if connected && self.live.me.is_none() && !self.live.me_loading {
            self.live.me_loading = true;
            let lcu = self.lcu.clone();
            spawn_job(
                cx,
                move || lcu.lock().unwrap().as_ref().ok_or_else(|| anyhow::anyhow!("not connected"))?.me(),
                |this, r, _| {
                    this.live.me_loading = false;
                    if let Ok(me) = r {
                        if !me.game_name.is_empty() {
                            this.live.me = Some(me);
                        }
                    }
                },
            );
        }
        let Some(me) = self.live.me.clone() else { return };

        // Ranks for the players champ select shows names for.
        if let ClientStatus::ChampSelect(cs) = &self.client {
            let ids: Vec<String> = cs
                .my_team
                .iter()
                .filter_map(|p| if p.is_me { Some(format!("{}#{}", me.game_name, me.tag_line)) } else { p.name.clone() })
                .collect();
            for id in ids {
                self.ensure_rank(&me.region, &id, cx);
            }
        }

        // U.GG's live game: look it up when the game starts, and again until U.GG sees it.
        if in_game(&self.phase) || live_as.is_some() {
            let stale = match (&self.live.game, self.live.fetched_at) {
                (None, _) => true,
                (Some(Loadable::Ready(None)) | Some(Loadable::Failed(_)), Some(t)) => t.elapsed() >= RETRY,
                _ => false,
            };
            if stale {
                self.fetch_live_game(cx);
            }
        }
    }

    fn ensure_rank(&mut self, region: &str, riot_id: &str, cx: &mut Context<Self>) {
        let key = riot_id.to_lowercase();
        if self.live.ranks.contains_key(&key) {
            return;
        }
        let Some((name, tag)) = riot_id.split_once('#') else { return };
        self.live.ranks.insert(key.clone(), Loadable::Loading);
        let (region, name, tag) = (region.to_string(), name.to_string(), tag.to_string());
        spawn_job(cx, move || ugg_live::profile_rank(&region, &name, &tag), move |this, r, _| {
            this.live.ranks.insert(key, Loadable::from(r));
        });
    }

    pub(super) fn fetch_live_game(&mut self, cx: &mut Context<Self>) {
        let Some(me) = self.live.me.clone() else { return };
        if self.live.game.as_ref().is_some_and(|g| g.is_loading()) {
            return;
        }
        // Keep showing what's there while looking again.
        if !matches!(self.live.game, Some(Loadable::Ready(Some(_)))) {
            self.live.game = Some(Loadable::Loading);
        }
        self.live.fetched_at = Some(Instant::now());
        spawn_job(
            cx,
            move || ugg_live::live_game(&me.region, &me.game_name, &me.tag_line),
            |this, r, _| {
                this.live.fetched_at = Some(Instant::now());
                let keep = r.is_err() && matches!(this.live.game, Some(Loadable::Ready(Some(_))));
                if !keep {
                    this.live.game = Some(Loadable::from(r));
                }
            },
        );
    }

    /// A new champ select: the last game's lookup no longer applies.
    pub(super) fn reset_live_game(&mut self) {
        self.live.game = None;
        self.live.fetched_at = None;
    }

    pub(super) fn render_live_game(&mut self, cx: &mut Context<Self>) -> AnyElement {
        self.ensure_live(cx);
        let data = self.data.clone().unwrap();
        let header = |title: String, sub: String| {
            div()
                .flex()
                .items_center()
                .gap(px(10.))
                .child(div().w(px(3.)).h(px(18.)).bg(rgb(ACCENT)))
                .child(div().text_size(px(18.)).font_weight(FontWeight::BOLD).child(title))
                .child(div().text_size(px(14.)).text_color(rgb(MUTED)).child(sub))
        };
        let body = match (&self.client, &self.live.game) {
            (ClientStatus::ChampSelect(cs), _) => {
                let cs = cs.clone();
                self.render_champ_select_lobby(&data, &cs, cx)
            }
            (_, Some(Loadable::Ready(Some(game)))) => {
                let game = game.clone();
                self.render_live_teams(&data, &game, cx)
            }
            (status, game) => {
                let (msg, sub) = match (status, game) {
                    (ClientStatus::NotRunning, _) => (
                        "Open the League client.".to_string(),
                        "Live Game follows your lobby from champ select through the game.".to_string(),
                    ),
                    (_, Some(Loadable::Loading)) => ("Looking up your game on U.GG…".into(), String::new()),
                    (_, Some(Loadable::Failed(e))) => (format!("Couldn't load the live game: {e}"), String::new()),
                    _ if in_game(&self.phase) => (
                        "Waiting for the loading screen…".into(),
                        "U.GG sees a game once the loading screen appears; Poro checks again every 15 seconds.".into(),
                    ),
                    _ => (
                        "Not in a game.".into(),
                        "Start a game: Live Game shows your lobby during champ select and both teams from the loading screen on."
                            .into(),
                    ),
                };
                card()
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .child(div().text_size(px(15.)).font_weight(FontWeight::SEMIBOLD).child(msg))
                    .when(!sub.is_empty(), |d| d.child(div().text_size(px(13.)).text_color(rgb(MUTED)).child(sub)))
                    .into_any_element()
            }
        };
        let sub = match (&self.client, &self.live.game) {
            (ClientStatus::ChampSelect(cs), _) => {
                let phase = match cs.phase.as_str() {
                    "PLANNING" => "Declaring picks",
                    "BAN_PICK" => "Bans and picks",
                    "FINALIZATION" => "Finalizing",
                    _ => "In progress",
                };
                format!("Champ select · {} · {phase}", queue_name(cs.queue_id, ""))
            }
            (_, Some(Loadable::Ready(Some(g)))) => {
                let secs = g.seconds.max(0) as u64
                    + self.live.fetched_at.map(|t| t.elapsed().as_secs()).unwrap_or(0) * in_game(&self.phase) as u64;
                format!("{} · {}", queue_name(g.queue_id, &g.game_type), duration(secs))
            }
            _ => String::new(),
        };
        let refresh = matches!(self.live.game, Some(Loadable::Ready(Some(_))))
            .then(|| button("live-refresh", "Refresh").on_click(cx.listener(|this, _, _, cx| this.fetch_live_game(cx))));
        div()
            .flex()
            .flex_col()
            .gap(px(14.))
            .p(px(18.))
            .child(header("Live Game".into(), sub).child(div().flex_1()).children(refresh))
            .child(body)
            .into_any_element()
    }

    // ---- champ select ----

    fn render_champ_select_lobby(&mut self, data: &StaticData, cs: &ChampSelect, cx: &mut Context<Self>) -> AnyElement {
        let me = self.live.me.clone();
        let mut mine: Vec<CsPlayer> = cs.my_team.clone();
        mine.sort_by_key(|p| (lane_order(p.lane), p.cell));
        let rows: Vec<AnyElement> = mine.iter().enumerate().map(|(i, p)| self.ally_row(data, p, i, me.as_ref(), cx)).collect();

        // Enemies: locked picks with their roles (from the client, or predicted), then the ones still picking.
        let mut enemies = self.enemy_roles(cs);
        enemies.sort_by_key(|(_, l)| lane_order(*l));
        let predicted = !cs.their_team.iter().any(|p| p.lane.is_some());
        let picking = cs.their_team.len().saturating_sub(enemies.len());
        let my_lane = cs.my_lane;
        let mut enemy_rows: Vec<AnyElement> = enemies
            .iter()
            .map(|&(champ, lane)| enemy_row(data, champ, lane, predicted, lane.is_some() && lane == my_lane, cx))
            .collect();
        for i in 0..picking {
            enemy_rows.push(
                lobby_row(SharedString::from(format!("enemy-picking-{i}")))
                    .child(div().size(px(44.)).rounded_full().bg(rgb(PANEL_2)))
                    .child(div().text_size(px(13.)).text_color(rgb(MUTED)).child("Picking…"))
                    .into_any_element(),
            );
        }
        let bans = |list: &[u32], cx: &mut Context<Self>| -> AnyElement {
            div()
                .flex()
                .items_center()
                .gap(px(4.))
                .child(div().text_size(px(11.)).text_color(rgb(MUTED)).mr(px(4.)).child("Bans"))
                .children(list.iter().filter(|&&c| c != 0).map(|&c| images::dim(data.champion_icon(c).as_deref(), 22., true, cx)))
                .when(list.iter().all(|&c| c == 0), |d| d.child(div().text_size(px(11.)).text_color(rgb(MUTED)).child("–")))
                .into_any_element()
        };
        let ally_bans = bans(&cs.bans.0, cx);
        let enemy_bans = bans(&cs.bans.1, cx);
        let column = |title: &'static str, color: u32, bans: AnyElement, rows: Vec<AnyElement>| {
            card()
                .flex_1()
                .flex()
                .flex_col()
                .gap(px(8.))
                .border_t_2()
                .border_color(rgb(color))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .child(div().text_size(px(15.)).font_weight(FontWeight::BOLD).text_color(rgb(color)).child(title))
                        .child(div().flex_1())
                        .child(bans),
                )
                .children(rows)
        };
        let note = if predicted && !enemies.is_empty() {
            "Enemy roles are predicted from how often U.GG sees each champion in each role. "
        } else {
            ""
        };
        div()
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(
                div()
                    .flex()
                    .items_start()
                    .gap(px(12.))
                    .child(column("Your Team", BLUE, ally_bans, rows))
                    .child(column("Enemy Team", RED, enemy_bans, enemy_rows)),
            )
            .child(div().text_size(px(12.)).text_color(rgb(MUTED)).child(format!(
                "{note}The League client hides other players' names in ranked champ select, and enemy names until the game \
                 loads, so their ranks show here once the loading screen appears."
            )))
            .into_any_element()
    }

    fn ally_row(&self, data: &StaticData, p: &CsPlayer, i: usize, me: Option<&Me>, cx: &mut Context<Self>) -> AnyElement {
        let champ = if p.champion != 0 { p.champion } else { p.intent };
        let portrait = if champ == 0 {
            div().size(px(44.)).rounded_full().bg(rgb(PANEL_2)).into_any_element()
        } else if p.locked {
            tips::champ(data, champ, 44., true, cx)
        } else {
            images::dim(data.champion_icon(champ).as_deref(), 44., true, cx)
        };
        let lane_icon = p.lane.map(|l| images::icon(Some(&l.icon()), 18., cx));
        let spells: Vec<AnyElement> = [p.spells.0, p.spells.1]
            .iter()
            .filter(|&&s| s != 0)
            .map(|&s| tips::spell(data, s, 20., cx))
            .collect();
        let riot_id = if p.is_me { me.map(|m| format!("{}#{}", m.game_name, m.tag_line)) } else { p.name.clone() };
        let name = riot_id.clone().map(|r| r.split('#').next().unwrap_or("").to_string());
        let rank = riot_id.as_ref().and_then(|r| self.live.ranks.get(&r.to_lowercase()));
        let champ_label = if champ == 0 {
            "Picking…".to_string()
        } else if p.locked {
            data.champion_name(champ)
        } else {
            format!("{} (hovering)", data.champion_name(champ))
        };
        lobby_row(SharedString::from(format!("ally-{i}")))
            .when(p.is_me, |d| d.border_1().border_color(rgb(ACCENT)))
            .child(div().w(px(20.)).flex().justify_center().children(lane_icon))
            .child(portrait)
            .child(div().flex().flex_col().gap(px(2.)).children(spells))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .child(
                                div()
                                    .text_size(px(14.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .truncate()
                                    .child(name.unwrap_or_else(|| "Hidden player".into())),
                            )
                            .when(p.is_me, |d| d.child(pill("You", ACCENT))),
                    )
                    .child(div().text_size(px(12.)).text_color(rgb(MUTED)).child(champ_label)),
            )
            .child(rank_block(rank, riot_id.is_some(), cx))
            .into_any_element()
    }

    // ---- in game ----

    fn render_live_teams(&mut self, data: &StaticData, game: &LiveGame, cx: &mut Context<Self>) -> AnyElement {
        let me = self.live.me.as_ref().map(|m| format!("{}#{}", m.game_name, m.tag_line)).unwrap_or_default();
        let (mine, theirs, found) = game.teams_for(&me);
        let (t1, t2) = if found { ("Your Team", "Enemy Team") } else { ("Team 1", "Team 2") };
        let a = self.live_team(data, t1, BLUE, mine, &me, cx);
        let b = self.live_team(data, t2, RED, theirs, &me, cx);
        div().flex().flex_col().gap(px(12.)).child(a).child(b).into_any_element()
    }

    fn live_team(
        &self,
        data: &StaticData,
        title: &str,
        color: u32,
        team: &[LivePlayer],
        me: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        // Parties: numbers shared by two or more players on this team.
        let mut parties: Vec<i64> = Vec::new();
        for p in team {
            if let Some(n) = p.party {
                if team.iter().filter(|q| q.party == Some(n)).count() > 1 && !parties.contains(&n) {
                    parties.push(n);
                }
            }
        }
        let bans: Vec<AnyElement> =
            team.iter().filter(|p| p.ban != 0).map(|p| images::dim(data.champion_icon(p.ban).as_deref(), 22., true, cx)).collect();
        let head = |label: &'static str, w: f32| div().w(px(w)).text_size(px(11.)).text_color(rgb(MUTED)).child(label);
        let rows: Vec<AnyElement> = team
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let party = p.party.and_then(|n| parties.iter().position(|&x| x == n)).map(|k| PARTY_COLORS[k % PARTY_COLORS.len()]);
                live_row(data, title, i, p, party, p.riot_id.as_deref().is_some_and(|r| r.eq_ignore_ascii_case(me)), cx)
            })
            .collect();
        card()
            .flex()
            .flex_col()
            .gap(px(6.))
            .border_t_2()
            .border_color(rgb(color))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .child(div().text_size(px(15.)).font_weight(FontWeight::BOLD).text_color(rgb(color)).mr(px(10.)).child(title.to_string()))
                    .child(div().text_size(px(11.)).text_color(rgb(MUTED)).mr(px(4.)).child("Bans"))
                    .children(bans),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .px(px(10.))
                    .child(head("Champion", 150.))
                    .child(div().flex_1().text_size(px(11.)).text_color(rgb(MUTED)).child("Summoner"))
                    .child(head("Rank", 170.))
                    .child(head("Champion WR", 110.))
                    .child(head("KDA on champion", 120.))
                    .child(head("Role", 140.)),
            )
            .children(rows)
            .into_any_element()
    }
}

fn lobby_row(id: SharedString) -> gpui::Stateful<gpui::Div> {
    div().id(id).flex().items_center().gap(px(10.)).px(px(10.)).py(px(7.)).rounded(px(8.)).bg(rgb(PANEL_2))
}

fn pill(label: &str, color: u32) -> AnyElement {
    div()
        .px(px(6.))
        .rounded(px(3.))
        .bg(rgb(color))
        .text_color(rgb(0x15151a))
        .text_size(px(10.))
        .font_weight(FontWeight::BOLD)
        .child(label.to_string())
        .into_any_element()
}

fn rank_block(rank: Option<&Loadable<Option<RankScore>>>, named: bool, cx: &mut Context<AppView>) -> AnyElement {
    let col = div().w(px(170.)).flex().items_center().gap(px(8.));
    match rank {
        Some(Loadable::Ready(Some(r))) => {
            let wr = r.win_rate().unwrap_or(0.0);
            col.children(r.crest().map(|u| images::icon(Some(&u), 26., cx)))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .child(div().text_size(px(13.)).font_weight(FontWeight::BOLD).text_color(rgb(tier_text_color(&r.tier))).child(r.label()))
                        .child(
                            div()
                                .flex()
                                .gap(px(4.))
                                .text_size(px(11.))
                                .child(div().text_color(rgb(wr_color(wr))).child(format!("{wr:.0}%")))
                                .child(div().text_color(rgb(MUTED)).child(format!("{}W {}L", r.wins, r.losses))),
                        ),
                )
                .into_any_element()
        }
        Some(Loadable::Ready(None)) => col
            .child(div().text_size(px(13.)).text_color(rgb(MUTED)).child(if named { "Unranked" } else { "Hidden" }))
            .into_any_element(),
        Some(Loadable::Loading) => col.child(div().text_size(px(12.)).text_color(rgb(MUTED)).child("Loading rank…")).into_any_element(),
        Some(Loadable::Failed(_)) => col.child(div().text_size(px(12.)).text_color(rgb(MUTED)).child("Rank unavailable")).into_any_element(),
        _ => col
            .child(div().text_size(px(12.)).text_color(rgb(MUTED)).child(if named { "" } else { "Shown in loading screen" }))
            .into_any_element(),
    }
}

fn enemy_row(data: &StaticData, champ: u32, lane: Option<Lane>, predicted: bool, vs_me: bool, cx: &mut Context<AppView>) -> AnyElement {
    let lane_icon = lane.map(|l| images::icon(Some(&l.icon()), 18., cx));
    let role = match lane {
        Some(l) if predicted => format!("Predicted {}", role_name(l)),
        Some(l) => role_name(l).to_string(),
        None => "Role unknown".into(),
    };
    lobby_row(SharedString::from(format!("enemy-{champ}")))
        .when(vs_me, |d| d.border_1().border_color(rgb(RED)))
        .child(div().w(px(20.)).flex().justify_center().children(lane_icon))
        .child(tips::champ(data, champ, 44., true, cx))
        .child(
            div()
                .flex_1()
                .flex()
                .flex_col()
                .gap(px(2.))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .child(div().text_size(px(14.)).font_weight(FontWeight::SEMIBOLD).child(data.champion_name(champ)))
                        .when(vs_me, |d| d.child(pill("Your lane", RED))),
                )
                .child(div().text_size(px(12.)).text_color(rgb(MUTED)).child(role)),
        )
        .into_any_element()
}

fn live_row(data: &StaticData, team: &str, i: usize, p: &LivePlayer, party: Option<u32>, is_me: bool, cx: &mut Context<AppView>) -> AnyElement {
    let spells: Vec<AnyElement> =
        [p.spells.0, p.spells.1].iter().filter(|&&s| s != 0).map(|&s| tips::spell(data, s, 20., cx)).collect();
    let runes: Vec<AnyElement> = [(p.keystone, 22.), (p.sub_style, 16.)]
        .iter()
        .filter(|(r, _)| *r != 0)
        .map(|&(r, size)| tips::rune(data, r, size, cx))
        .collect();
    let lane_icon = p.role.map(|l| images::icon(Some(&l.icon()), 16., cx));
    let tags: Vec<AnyElement> = p
        .tags
        .iter()
        .take(3)
        .map(|t| {
            let color = match t.kind.as_str() {
                "good" => 0x3bc47c,
                "negative" => RED,
                _ => 0xb4b6c0,
            };
            div()
                .px(px(5.))
                .rounded(px(3.))
                .border_1()
                .border_color(rgb(color))
                .text_color(rgb(color))
                .text_size(px(10.))
                .child(t.tag.clone())
                .into_any_element()
        })
        .collect();
    let (name, tag) = match &p.riot_id {
        Some(r) => {
            let (n, t) = r.split_once('#').unwrap_or((r.as_str(), ""));
            (n.to_string(), format!("#{t}"))
        }
        None => ("Anonymous".to_string(), String::new()),
    };
    let rank = p.rank.clone().map(|r| Loadable::Ready(Some(r))).unwrap_or(Loadable::Ready(None));
    let games = p.champ_games();
    let champ_wr = (games > 0).then(|| p.champ_wins as f64 * 100.0 / games as f64);
    let kda = p.avg_kda();
    // Main role = the role they've played most this season. (U.GG's own
    // `onRole` flag disagreed with its role stats, e.g. "off role" for a
    // jungle main playing jungle, so it isn't used.)
    let main = p.roles.first().map(|r| r.0);
    let off_role = matches!((p.role, main), (Some(r), Some(m)) if r != m);
    let role_line = match (p.role, main) {
        (Some(_), Some(m)) if off_role => format!("Off role (main {})", role_name(m)),
        (Some(_), Some(_)) => "Main role".to_string(),
        _ => String::new(),
    };
    div()
        .id(SharedString::from(format!("live-{team}-{i}")))
        .flex()
        .items_center()
        .gap(px(12.))
        .px(px(10.))
        .py(px(7.))
        .rounded(px(8.))
        .bg(rgb(PANEL_2))
        .when(is_me, |d| d.border_1().border_color(rgb(ACCENT)))
        .child(
            div()
                .w(px(150.))
                .flex()
                .items_center()
                .gap(px(6.))
                .child(div().w(px(16.)).children(lane_icon))
                .child(tips::champ(data, p.champion, 42., true, cx))
                .child(div().flex().flex_col().gap(px(2.)).children(spells))
                .child(div().flex().flex_col().items_center().gap(px(2.)).children(runes)),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(3.))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(4.))
                        .when_some(party, |d, c| d.child(div().size(px(8.)).rounded_full().bg(rgb(c))))
                        .child(div().text_size(px(14.)).font_weight(FontWeight::SEMIBOLD).truncate().child(name))
                        .child(div().text_size(px(12.)).text_color(rgb(MUTED)).child(tag))
                        .when(is_me, |d| d.child(pill("You", ACCENT))),
                )
                .child(div().flex().flex_wrap().gap(px(4.)).children(tags)),
        )
        .child(rank_block(Some(&rank), p.riot_id.is_some(), cx))
        .child(
            div()
                .w(px(110.))
                .flex()
                .flex_col()
                .child(match champ_wr {
                    Some(wr) => div().text_size(px(13.)).font_weight(FontWeight::BOLD).text_color(rgb(wr_color(wr))).child(format!("{wr:.0}%")),
                    None => div().text_size(px(13.)).text_color(rgb(MUTED)).child("–"),
                })
                .child(div().text_size(px(11.)).text_color(rgb(MUTED)).child(if games == 0 {
                    "First game this season".to_string()
                } else {
                    format!("{games} game{}", if games == 1 { "" } else { "s" })
                })),
        )
        .child(
            div()
                .w(px(120.))
                .flex()
                .flex_col()
                .child(match p.kda_ratio() {
                    Some(r) => div().text_size(px(13.)).font_weight(FontWeight::BOLD).child(format!("{r:.2} KDA")),
                    None => div().text_size(px(13.)).text_color(rgb(MUTED)).child("–"),
                })
                .child(div().text_size(px(11.)).text_color(rgb(MUTED)).child(match kda {
                    Some((k, d, a)) => format!("{k:.1} / {d:.1} / {a:.1}"),
                    None => String::new(),
                })),
        )
        .child(
            div()
                .w(px(140.))
                .flex()
                .flex_col()
                .child(div().text_size(px(13.)).child(p.role.map(role_name).unwrap_or("–").to_string()))
                .child(div().text_size(px(11.)).whitespace_nowrap().text_color(rgb(if off_role { GOLD } else { MUTED })).child(role_line)),
        )
        .into_any_element()
}
