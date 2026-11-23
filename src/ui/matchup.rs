//! The Champion Matchup tab (deeplol.gg/llm).

use gpui::{
    deferred, div, prelude::*, px, AnyElement, Context, Entity, Focusable, FontWeight, MouseButton, SharedString, Window,
};

use super::games::{rune_page_row, skill_order_row};
use super::theme::*;
use super::tips;
use super::{AppView, Loadable, Slot};
use crate::data::ddragon::{Lane, StaticData};
use crate::data::matchup_builds::{self, GeneralBuild};
use crate::data::deeplol::LaneStats;
use crate::images;
use crate::text_input::TextInput;

impl AppView {
    pub fn render_matchup(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let data = self.data.clone().unwrap();
        let header = self.render_selected_card(&data, cx);
        let controls = self.render_controls(&data, window, cx);
        let vs = self.render_vs(&data, cx);
        let tips = self.render_tips(&data);
        let builds = self.render_builds(&data, cx);
        let games = self.render_games(&data, cx);
        div()
            .flex()
            .flex_col()
            .gap(px(14.))
            .p(px(18.))
            .child(header)
            .child(controls)
            .child(vs)
            .child(tips)
            .child(builds)
            .child(games)
            .into_any_element()
    }

    fn lane_stats<'a>(&self, stats: &'a Loadable<std::collections::HashMap<Lane, LaneStats>>) -> Option<&'a LaneStats> {
        stats.ready().and_then(|m| m.get(&self.lane))
    }

    /// Which patch the champion stats are from.
    fn patch_note(&self) -> String {
        let (dl, fell_back) = self.deeplol_patch();
        if fell_back {
            format!("patch {dl} stats (deeplol hasn't published {} stats yet)", self.patch_label())
        } else {
            format!("patch {dl} stats")
        }
    }

    fn render_selected_card(&mut self, data: &StaticData, cx: &mut Context<Self>) -> AnyElement {
        let explain = div()
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(div().text_size(px(22.)).font_weight(FontWeight::BOLD).child("Champion Matchup"))
            .child(div().text_size(px(13.)).text_color(rgb(MUTED)).child(
                "Check matchup information quickly and pick a counter for your lane. Based on real ranked games \
                 (deeplol.gg, Korea, Emerald+), including pro players' games.",
            ));
        let Some(a) = self.a else {
            return card().child(explain).into_any_element();
        };
        let champ = data.champion(a).cloned();
        let s = self.lane_stats(&self.a_stats).cloned();
        let loading = self.a_stats.is_loading();
        let portrait = tips::champ(data, a, 72., false, cx);
        let tier = s.as_ref().and_then(|s| s.champion_tier);
        let stats_row = match &s {
            Some(s) => div()
                .flex()
                .gap(px(22.))
                .child(stat("Tier", tier.map(|t| t.to_string()).unwrap_or("–".into())))
                .child(stat(
                    "Ranking",
                    match (s.rank, s.lane_cnt_all) {
                        (Some(r), Some(n)) => format!("{r} / {n}"),
                        _ => "–".into(),
                    },
                ))
                .child(stat("Win Rate", pct_frac(s.win_rate)))
                .child(stat("Pick Rate", pct_frac(s.pick_rate)))
                .child(stat("Ban Rate", pct_frac(s.ban_rate)))
                .child(stat("Games", s.games.map(thousands).unwrap_or("–".into())))
                .into_any_element(),
            None => div()
                .text_size(px(13.))
                .text_color(rgb(MUTED))
                .child(match &self.a_stats {
                    _ if loading => "Loading…".to_string(),
                    Loadable::Failed(e) => format!("Couldn't load deeplol data: {e}"),
                    _ => format!("No deeplol data for {} · {}", self.lane.label(), self.patch_note()),
                })
                .into_any_element(),
        };
        card()
            .flex()
            .flex_col()
            .gap(px(14.))
            .child(explain)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(16.))
                    .p(px(12.))
                    .rounded(px(8.))
                    .bg(rgb(PANEL_2))
                    .child(portrait)
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(8.))
                            .child(
                                div()
                                    .flex()
                                    .items_baseline()
                                    .gap(px(8.))
                                    .child(div().text_size(px(18.)).font_weight(FontWeight::BOLD).child(champ.map(|c| c.name).unwrap_or_default()))
                                    .child(div().text_size(px(12.)).text_color(rgb(MUTED)).child(format!(
                                        "Selected champion · {} · {}",
                                        self.lane.label(),
                                        self.patch_note()
                                    ))),
                            )
                            .child(stats_row),
                    )
                    .child(div().flex_1())
                    .child(self.ban_suggestions(data, a, cx)),
            )
            .into_any_element()
    }

    /// The selected champion's five worst matchups in this lane (lolalytics,
    /// like the Counters tab), shown before an opponent is picked so you know
    /// whom to ban. Clicking one makes it the opponent.
    fn ban_suggestions(&mut self, data: &StaticData, champ: u32, cx: &mut Context<Self>) -> AnyElement {
        let lane = self.lane;
        let counters = self.counters_for(champ, lane, cx);
        let name = data.champion_name(champ);
        let body: AnyElement = match &counters {
            Loadable::Ready(list) if !list.is_empty() => {
                let cells: Vec<AnyElement> = list
                    .iter()
                    .take(5)
                    .map(|m| {
                        let enemy = m.cid;
                        let tip = crate::data::ddragon::Tip {
                            title: data.champion_name(enemy),
                            subtitle: Some(format!("{name} wins {:.2}% of {} games vs them", m.vs_wr, thousands(m.n))),
                            body: format!("Δ1 {:+.2} · Δ2 {:+.2} · click to see this matchup", m.d1, m.d2),
                        };
                        let icon = tips::tipped(images::round(data.champion_icon(enemy).as_deref(), 38., cx), Some(tip));
                        div()
                            .id(SharedString::from(format!("ban-{enemy}")))
                            .flex()
                            .flex_col()
                            .items_center()
                            .gap(px(3.))
                            .cursor_pointer()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.set_champion(Slot::B, enemy, cx);
                                this.lane = lane;
                                this.lane_auto = false;
                            }))
                            .child(div().rounded_full().border_2().border_color(rgb(RED)).child(icon))
                            .child(div().text_size(px(11.)).font_weight(FontWeight::BOLD).text_color(rgb(wr_color(m.vs_wr))).child(format!("{:.1}%", m.vs_wr)))
                            .into_any_element()
                    })
                    .collect();
                div().flex().gap(px(10.)).children(cells).into_any_element()
            }
            Loadable::Loading | Loadable::Idle => {
                div().text_size(px(12.)).text_color(rgb(MUTED)).child("Loading…").into_any_element()
            }
            _ => div()
                .text_size(px(12.))
                .text_color(rgb(MUTED))
                .child(format!("No counter data for {name} {}", lane.label()))
                .into_any_element(),
        };
        div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(
                div()
                    .flex()
                    .items_baseline()
                    .gap(px(8.))
                    .child(div().text_size(px(13.)).font_weight(FontWeight::BOLD).child("Weak Against"))
                    .child(div().text_size(px(11.)).text_color(rgb(MUTED)).child(format!("ban candidates · {}", lane.label()))),
            )
            .child(body)
            .into_any_element()
    }

    pub(super) fn champ_field(
        &self,
        id: &'static str,
        slot: Slot,
        input: &Entity<TextInput>,
        selected: Option<u32>,
        data: &StaticData,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let focused = input.focus_handle(cx).is_focused(window);
        let query = input.read(cx).text().to_string();
        let selected_name = selected.map(|k| data.champion_name(k));
        let show = focused && !query.trim().is_empty() && selected_name.as_deref() != Some(query.as_str());
        let mut rows = Vec::new();
        if show {
            for c in data.search(&query).into_iter().take(8) {
                let key = c.key;
                let icon = tips::champ(data, key, 24., false, cx);
                rows.push(
                    div()
                        .id(SharedString::from(format!("{id}-{key}")))
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .px(px(8.))
                        .py(px(5.))
                        .rounded(px(5.))
                        .cursor_pointer()
                        .hover(|s| s.bg(rgb(BORDER)))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _, _, cx| {
                                this.set_champion(slot, key, cx);
                                cx.stop_propagation();
                            }),
                        )
                        .child(icon)
                        .child(div().text_size(px(13.)).child(c.name.clone())),
                );
            }
        }
        let icon = images::icon(selected.and_then(|k| data.champion_icon(k)).as_deref(), 26., cx);
        div()
            .relative()
            .w(px(250.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .px(px(6.))
                    .h(px(38.))
                    .rounded(px(6.))
                    .bg(rgb(0x1b1e25))
                    .border_1()
                    .border_color(rgb(if focused { ACCENT } else { BORDER }))
                    .child(icon)
                    .child(div().flex_1().child(input.clone())),
            )
            .when(!rows.is_empty(), |d| {
                d.child(
                    deferred(
                        div()
                            .absolute()
                            .top(px(42.))
                            .left_0()
                            .w(px(250.))
                            .p(px(4.))
                            .rounded(px(8.))
                            .bg(rgb(PANEL_2))
                            .border_1()
                            .border_color(rgb(BORDER))
                            .shadow_lg()
                            .occlude()
                            .flex()
                            .flex_col()
                            .children(rows),
                    )
                    .with_priority(1),
                )
            })
            .into_any_element()
    }

    fn render_controls(&mut self, data: &StaticData, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let input_a = self.input_a.clone();
        let input_b = self.input_b.clone();
        let field_a = self.champ_field("sug-a", Slot::A, &input_a, self.a, data, window, cx);
        let field_b = self.champ_field("sug-b", Slot::B, &input_b, self.b, data, window, cx);
        let lanes = lane_chips("lane", self.lane, cx, |this, lane, cx| this.set_lane(lane, cx));
        div()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(10.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .child(button("reset", "Reset").on_click(cx.listener(|this, _, _, cx| this.reset(cx))))
                    .child(field_a)
                    .child(
                        button("swap", "⇄  Swap")
                            .text_color(rgb(ACCENT))
                            .border_color(rgb(ACCENT))
                            .on_click(cx.listener(|this, _, _, cx| this.swap(cx))),
                    )
                    .child(field_b),
            )
            .child(lanes)
            .into_any_element()
    }

    fn render_vs(&mut self, data: &StaticData, cx: &mut Context<Self>) -> AnyElement {
        let (Some(a), Some(b)) = (self.a, self.b) else {
            return card()
                .text_color(rgb(MUTED))
                .child("Pick two champions to see the matchup.")
                .into_any_element();
        };
        let side_a = self.render_side(data, a, true, cx);
        let side_b = self.render_side(data, b, false, cx);
        let portrait_a = ring(tips::champ(data, a, 104., true, cx), RED);
        let portrait_b = ring(tips::champ(data, b, 104., true, cx), BLUE);

        // The head-to-head numbers: deeplol's live matchup stats on the live
        // patch, otherwise the selected patch's matchup list.
        let (h2h, source) = if self.is_latest_patch() {
            (
                self.h2h.ready().and_then(|m| m.get(&self.lane)).map(|h| (h.games, h.my_win_rate, h.enemy_win_rate)),
                "deeplol's latest matchup data (all recent games)".to_string(),
            )
        } else {
            let (dl, _) = self.deeplol_patch();
            (
                self.lane_stats(&self.a_stats)
                    .and_then(|s| s.match_up.find(b))
                    .map(|m| (m.games, m.win_rate * 100.0, 100.0 - m.win_rate * 100.0)),
                format!("patch {dl} matchup list"),
            )
        };
        let loading = self.h2h.is_loading() || self.a_stats.is_loading();
        let center = match h2h {
            Some((games, my, enemy)) => {
                let bar = div()
                    .flex()
                    .w(px(380.))
                    .h(px(5.))
                    .rounded(px(3.))
                    .overflow_hidden()
                    .child(div().h_full().w(px((380.0 * my / 100.0) as f32)).bg(rgb(RED)))
                    .child(div().h_full().flex_1().bg(rgb(BLUE)));
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(6.))
                    .child(
                        div()
                            .flex()
                            .w(px(380.))
                            .justify_between()
                            .items_end()
                            .child(rate_block("Match Win Rate", my, RED))
                            .child(
                                div()
                                    .flex()
                                    .gap(px(6.))
                                    .pb(px(4.))
                                    .child(div().text_size(px(13.)).text_color(rgb(ACCENT)).child("Sample Size"))
                                    .child(div().text_size(px(13.)).font_weight(FontWeight::BOLD).child(thousands(games))),
                            )
                            .child(rate_block("Match Win Rate", enemy, BLUE)),
                    )
                    .child(bar)
                    .child(div().text_size(px(11.)).text_color(rgb(MUTED)).child(format!("{} · {source}", self.lane.label())))
                    .into_any_element()
            }
            None => div()
                .text_size(px(13.))
                .text_color(rgb(MUTED))
                .child(if loading {
                    "Loading matchup…".to_string()
                } else {
                    format!("Not enough {} games of this matchup ({source})", self.lane.label())
                })
                .into_any_element(),
        };

        card()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(12.))
            .child(side_a)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(14.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(56.))
                            .child(portrait_a)
                            .child(div().text_size(px(30.)).text_color(rgb(MUTED)).child("VS"))
                            .child(portrait_b),
                    )
                    .child(center),
            )
            .child(side_b)
            .into_any_element()
    }

    /// Name, "Weak Against" and skills for one side of the VS card.
    fn render_side(&mut self, data: &StaticData, key: u32, left: bool, cx: &mut Context<Self>) -> AnyElement {
        // The same data as the Counters tab: lolalytics, this lane, this patch,
        // lowest win rate first.
        let lane = self.lane;
        let counters = self.counters_for(key, lane, cx);
        let weak_note = match &counters {
            Loadable::Loading => Some("Loading…".to_string()),
            Loadable::Failed(_) => Some(format!("No lolalytics data for {} {}", data.champion_name(key), lane.label())),
            Loadable::Ready(list) if list.is_empty() => Some(format!("No lolalytics data for {} {}", data.champion_name(key), lane.label())),
            _ => None,
        };
        let weak: Vec<crate::data::lolalytics::Counter> = counters.ready().map(|l| l.iter().take(5).cloned().collect()).unwrap_or_default();
        let name = data.champion_name(key);
        let weak_icons: Vec<AnyElement> = weak
            .iter()
            .map(|m| {
                let enemy = m.cid;
                let tip = crate::data::ddragon::Tip {
                    title: data.champion_name(enemy),
                    subtitle: Some(format!("{name} wins {:.2}% of {} games vs them", m.vs_wr, thousands(m.n))),
                    body: format!("Δ1 {:+.2} · Δ2 {:+.2} · their win rate overall {:.2}% (lolalytics, patch {})", m.d1, m.d2, m.all_wr, self.patch_label()),
                };
                let icon = tips::tipped(images::round(data.champion_icon(enemy).as_deref(), 30., cx), Some(tip));
                let lane = self.lane;
                div()
                    .id(SharedString::from(format!("weak-{left}-{enemy}")))
                    .rounded_full()
                    .border_1()
                    .border_color(rgb(ACCENT))
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| this.open_matchup(key, enemy, lane, cx)))
                    .child(icon)
                    .into_any_element()
            })
            .collect();
        let skills: Vec<AnyElement> = self
            .skills
            .get(&key)
            .cloned()
            .unwrap_or_default()
            .iter()
            .enumerate()
            .map(|(i, skill)| {
                let icon = tips::skill(skill, 30., cx);
                div()
                    .relative()
                    .child(icon)
                    .when(i > 0, |d| {
                        d.child(
                            div()
                                .absolute()
                                .bottom_0()
                                .right_0()
                                .px(px(2.))
                                .bg(rgb(0x000000))
                                .text_size(px(9.))
                                .font_weight(FontWeight::BOLD)
                                .child(["", "Q", "W", "E", "R"][i.min(4)]),
                        )
                    })
                    .into_any_element()
            })
            .collect();
        let champ = data.champion(key).cloned();
        let align = |d: gpui::Div| if left { d.items_start() } else { d.items_end() };
        align(div().flex().flex_col().gap(px(6.)).w(px(220.)))
            .child(div().text_size(px(22.)).font_weight(FontWeight::BOLD).child(champ.as_ref().map(|c| c.name.clone()).unwrap_or_default()))
            .child(div().text_size(px(13.)).text_color(rgb(MUTED)).child(champ.map(|c| c.id).unwrap_or_default()))
            .child(div().mt(px(6.)).text_size(px(12.)).text_color(rgb(MUTED)).child("Weak Against"))
            .child(div().flex().gap(px(5.)).children(weak_icons))
            .children(weak_note.map(|n| div().text_size(px(11.)).text_color(rgb(MUTED)).child(n)))
            .child(div().mt(px(6.)).text_size(px(12.)).text_color(rgb(MUTED)).child("Skills"))
            .child(div().flex().gap(px(5.)).children(skills))
            .into_any_element()
    }

    fn render_tips(&self, data: &StaticData) -> AnyElement {
        let (Some(a), Some(b)) = (self.a, self.b) else { return div().into_any_element() };
        let body = match &self.tips {
            Loadable::Ready(t) => {
                let levels = t.levels.iter().enumerate().map(|(i, adv)| {
                    let (label, color) = match adv {
                        Some(true) => ("Ahead", ACCENT),
                        Some(false) => ("Behind", RED),
                        None => ("Even", MUTED),
                    };
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap(px(4.))
                        .px(px(12.))
                        .py(px(8.))
                        .rounded(px(6.))
                        .bg(rgb(PANEL_2))
                        .child(div().text_size(px(11.)).text_color(rgb(MUTED)).child(format!("Level {}", i + 1)))
                        .child(div().text_size(px(13.)).font_weight(FontWeight::SEMIBOLD).text_color(rgb(color)).child(label))
                });
                let tips = t.tips.iter().map(|(title, tip)| {
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(4.))
                        .child(div().text_size(px(14.)).font_weight(FontWeight::SEMIBOLD).child(title.clone()))
                        .child(div().text_size(px(13.)).text_color(rgb(0xc4c6ce)).child(tip.clone()))
                });
                div()
                    .flex()
                    .flex_col()
                    .gap(px(14.))
                    .child(div().flex().gap(px(8.)).children(levels))
                    .child(div().flex().flex_col().gap(px(12.)).children(tips))
                    .into_any_element()
            }
            Loadable::Loading => div().text_color(rgb(MUTED)).child("Loading tips…").into_any_element(),
            Loadable::Failed(e) => div().text_color(rgb(MUTED)).text_size(px(13.)).child(if e.contains("no tips") { "deeplol has no tips for this matchup yet.".to_string() } else { format!("Couldn't load tips: {e}") }).into_any_element(),
            Loadable::Idle => div().into_any_element(),
        };
        card()
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(section_title(format!(
                "Level advantage & tips · {} vs {}",
                data.champion_name(a),
                data.champion_name(b)
            )))
            .child(body)
            .into_any_element()
    }

    /// Up to 3 builds for this exact matchup, from deeplol's matchup history
    /// (filled up with deeplol's general builds when the matchup has fewer).
    /// U.GG's builds for a champion and lane on the selected patch, loading them once.
    fn ugg_builds_for(&mut self, champ: u32, lane: Lane, cx: &mut Context<Self>) -> Option<Vec<crate::data::ugg::UggBuild>> {
        let patch = self.patch_label().to_string();
        let key = (champ, lane, patch.clone());
        if let Some(l) = self.ugg_general.get(&key) {
            return l.ready().cloned();
        }
        self.ugg_general.insert(key.clone(), Loadable::Loading);
        let live = self.is_latest_patch();
        super::spawn_job(cx, move || crate::data::ugg::champion_builds(champ, lane, &patch, live), move |this, r, _| {
            // No U.GG data for that lane just means no extra builds.
            this.ugg_general.insert(key, Loadable::Ready(r.unwrap_or_default()));
        });
        None
    }

    fn render_builds(&mut self, data: &StaticData, cx: &mut Context<Self>) -> AnyElement {
        let Some(a) = self.a else { return div().into_any_element() };
        let name = data.champion_name(a);
        let (dl_patch, _) = self.deeplol_patch();
        // General builds for this lane: deeplol's (it only has a champion's
        // main lanes), then U.GG's (any lane with enough games).
        let mut general: Vec<GeneralBuild> = self
            .lane_stats(&self.a_stats)
            .map(|s| s.build_lst.iter().filter_map(|b| GeneralBuild::from_deeplol(b, &dl_patch)).collect())
            .unwrap_or_default();
        let ugg_patch = self.patch_label().to_string();
        let ugg = self.ugg_builds_for(a, self.lane, cx);
        if let Some(ugg) = &ugg {
            general.extend(ugg.iter().filter_map(|b| super::tierlist::ugg_general(data, b, &ugg_patch)));
        }
        // Show the builds only once every source is in (the matchup's games,
        // deeplol's and U.GG's general builds); otherwise they'd show from
        // whatever came first, then change a few seconds later.
        let loading = |l: &Loadable<_>| matches!(l, Loadable::Idle | Loadable::Loading);
        // (Each of these ends Ready or Failed, so this can't wait forever.)
        let pending = ugg.is_none() || self.a_stats.is_loading() || (self.b.is_some() && loading(&self.history));
        if pending {
            let title = match self.b {
                Some(b) => format!("{name} {} builds vs {}", self.lane.label(), data.champion_name(b)),
                None => format!("{name} {} builds", self.lane.label()),
            };
            return card()
                .flex()
                .flex_col()
                .gap(px(8.))
                .child(section_title(title))
                .child(div().text_size(px(13.)).text_color(rgb(MUTED)).child(
                    "Gathering builds from this matchup's games, deeplol and U.GG…",
                ))
                .into_any_element();
        }
        let (games, note): (Vec<crate::data::deeplol::Game>, Option<String>) = match (self.b, &self.history) {
            (None, _) => (Vec::new(), None),
            (Some(_), Loadable::Ready(_)) => {
                // Only games played in the selected lane.
                let games = matchup_builds::in_lane(&self.patch_games(), self.lane);
                let note = (games.is_empty()).then(|| {
                    format!(
                        "deeplol has no games of this matchup in {} on patch {}; showing general builds.",
                        self.lane.label(),
                        self.patch_label()
                    )
                });
                (games, note)
            }
            (Some(_), Loadable::Failed(e)) => (Vec::new(), Some(format!("Couldn't load the matchup history ({e}); showing general builds."))),
            (Some(_), _) => (Vec::new(), Some("Gathering this matchup's games from deeplol…".to_string())),
        };
        let summary = matchup_builds::summarize(&games, data, &general, 3);
        let enemy = self.b.map(|b| data.champion_name(b));
        let title = match &enemy {
            Some(e) if !games.is_empty() => {
                format!("{name} {} builds vs {e} · from {}", self.lane.label(), self.patch_games_label(&games))
            }
            Some(e) => format!("{name} {} builds vs {e}", self.lane.label()),
            None => format!("{name} {} builds · general builds", self.lane.label()),
        };
        if summary.builds.is_empty() {
            return card()
                .flex()
                .flex_col()
                .gap(px(8.))
                .child(section_title(title))
                .child(div().text_size(px(13.)).text_color(rgb(MUTED)).child(note.unwrap_or_else(|| "No builds yet.".into())))
                .into_any_element();
        }
        let rows: Vec<AnyElement> = summary
            .builds
            .iter()
            .enumerate()
            .map(|(i, b)| {
                let runes = rune_page_row(data, &b.runes, 28., cx);
                let spells: Vec<AnyElement> =
                    [b.spells.0, b.spells.1].iter().map(|&s| tips::spell(data, s, 28., cx)).collect();
                let start: Vec<AnyElement> = b.starter.iter().map(|&it| tips::item(data, it, 28., cx)).collect();
                // Items from the games, then the ones Poro suggests to finish the build.
                let bought_boots = b.boots.filter(|_| !b.suggested_boots);
                let items: Vec<AnyElement> =
                    b.items.iter().chain(bought_boots.iter()).map(|&it| tips::item(data, it, 28., cx)).collect();
                let suggested_boots = b.boots.filter(|_| b.suggested_boots);
                let suggested: Vec<AnyElement> = b
                    .suggested
                    .iter()
                    .chain(suggested_boots.iter())
                    .map(|&it| {
                        div()
                            .p(px(1.))
                            .rounded(px(5.))
                            .border_1()
                            .border_color(rgb(GOLD))
                            .child(tips::item(data, it, 26., cx))
                            .into_any_element()
                    })
                    .collect();
                let from = match &b.general {
                    Some(g) => format!("Items ({} build)", g.source),
                    None => "Items (from games, buy order)".to_string(),
                };
                let skills = skill_order_row(&b.skills, 15);
                let keystone = data.rune_name(b.keystone);
                let stats = match &b.general {
                    Some(g) => {
                        let pick = g.pick_rate.map(|p| format!(" · {} pick rate", pct(p * 100.0))).unwrap_or_default();
                        format!(
                            "{} (not matchup-specific) · {} win rate{pick} · {} games",
                            g.detail,
                            pct(g.win_rate * 100.0),
                            thousands(g.games)
                        )
                    }
                    None => format!(
                        "Used in {} of these {} games vs {} · {} won ({})",
                        b.games,
                        summary.sample,
                        enemy.clone().unwrap_or_default(),
                        b.wins,
                        pct(b.wins as f64 * 100.0 / b.games.max(1) as f64)
                    ),
                };
                let mut core = b.items.clone();
                core.extend(bought_boots);
                let mut finish = b.suggested.clone();
                finish.extend(suggested_boots);
                let import = super::BuildImport {
                    name: format!("{name} {} {keystone}", b.style),
                    champion: self.a.unwrap_or(0),
                    page: Some(b.runes.clone()),
                    spells: Some(b.spells),
                    blocks: vec![
                        ("Starting items".into(), b.starter.clone()),
                        ("Core items".into(), core),
                        ("Suggested to finish the build".into(), finish),
                    ],
                };
                let style_color = style_color(&b.style);
                div()
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .p(px(12.))
                    .rounded(px(8.))
                    .bg(rgb(PANEL_2))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(10.))
                            .child(div().text_size(px(14.)).font_weight(FontWeight::SEMIBOLD).child(format!("Build {}", i + 1)))
                            .child(
                                div()
                                    .px(px(8.))
                                    .py(px(2.))
                                    .rounded(px(4.))
                                    .bg(rgb(style_color))
                                    .text_color(rgb(0x111114))
                                    .text_size(px(12.))
                                    .font_weight(FontWeight::BOLD)
                                    .child(b.style.clone()),
                            )
                            .child(div().text_size(px(13.)).font_weight(FontWeight::SEMIBOLD).child(keystone))
                            .child(div().text_size(px(12.)).text_color(rgb(MUTED)).child(stats))
                            .child(div().flex_1())
                            .child(
                                primary_button(SharedString::from(format!("apply-build-{i}")), "Add Build").on_click(cx.listener(
                                    move |this, _, _, cx| this.apply_build(import.clone(), cx),
                                )),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .gap(px(22.))
                            .child(runes)
                            .child(labeled("Spells", div().flex().gap(px(4.)).children(spells)))
                            .child(labeled("Starter", div().flex().gap(px(4.)).children(start)))
                            .child(labeled(&from, div().flex().gap(px(4.)).children(items)))
                            .when(!suggested.is_empty(), |d| {
                                d.child(labeled(
                                    "Suggested to finish (Poro)",
                                    div().flex().gap(px(4.)).children(suggested),
                                ))
                            }),
                    )
                    .child(skills)
                    .into_any_element()
            })
            .collect();
        card()
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(section_title(title))
            .children(note.map(|n| div().text_size(px(12.)).text_color(rgb(MUTED)).child(n)))
            .children(rows)
            .into_any_element()
    }
}

/// A color per build style.
fn style_color(style: &str) -> u32 {
    match style {
        "AP" | "AP Bruiser" => 0x9b8cff,
        "Lethality" => 0xf2c14e,
        "Crit" | "On-hit" => 0xf29b4e,
        "Bruiser" | "AD" => 0xe8605e,
        "Tank" => 0x5b8cf5,
        "Hybrid" => 0x37c8ab,
        _ => 0x8b8d98,
    }
}

pub fn labeled(label: &str, child: impl IntoElement) -> gpui::Div {
    div()
        .flex()
        .flex_col()
        .gap(px(4.))
        .child(div().text_size(px(11.)).text_color(rgb(MUTED)).child(label.to_string()))
        .child(child)
}

fn rate_block(label: &str, rate: f64, color: u32) -> gpui::Div {
    div()
        .flex()
        .flex_col()
        .child(div().text_size(px(12.)).text_color(rgb(MUTED)).child(label.to_string()))
        .child(div().text_size(px(30.)).font_weight(FontWeight::BOLD).text_color(rgb(color)).child(format!("{rate:.2}%")))
}

fn ring(child: AnyElement, color: u32) -> AnyElement {
    div().p(px(3.)).rounded_full().border_2().border_color(rgb(color)).child(child).into_any_element()
}

/// TOP / JUNGLE / MID / BOT / SUPPORT chips.
pub fn lane_chips(
    id: &'static str,
    selected: Lane,
    cx: &mut Context<AppView>,
    on_pick: impl Fn(&mut AppView, Lane, &mut Context<AppView>) + Clone + 'static,
) -> AnyElement {
    div()
        .flex()
        .gap(px(6.))
        .children(Lane::ALL.into_iter().map(|lane| {
            let active = lane == selected;
            let on_pick = on_pick.clone();
            div()
                .id(SharedString::from(format!("{id}-{}", lane.label())))
                .px(px(12.))
                .py(px(6.))
                .rounded(px(6.))
                .border_1()
                .text_size(px(12.))
                .font_weight(FontWeight::SEMIBOLD)
                .cursor_pointer()
                .when(active, |d| d.border_color(rgb(ACCENT)).text_color(rgb(ACCENT)).bg(rgba(0x37c8ab22)))
                .when(!active, |d| d.border_color(rgb(BORDER)).text_color(rgb(MUTED)).hover(|s| s.text_color(rgb(TEXT))))
                .on_click(cx.listener(move |this, _, _, cx| on_pick(this, lane, cx)))
                .child(lane.label())
        }))
        .into_any_element()
}
