//! The Champion Matchup tab (deeplol.gg/llm).

use gpui::{
    deferred, div, prelude::*, px, AnyElement, Context, Entity, Focusable, FontWeight, MouseButton, SharedString, Window,
};

use super::games::{rune_page_row, skill_order_row};
use super::theme::*;
use super::{AppView, Loadable, Slot};
use crate::data::ddragon::{Lane, StaticData};
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

    fn patch_note(&self) -> String {
        let (dl, fell_back) = self.deeplol_patch();
        if fell_back {
            format!("deeplol hasn't published {} yet; showing {dl}", self.patch_label())
        } else {
            format!("Patch {dl}")
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
        let portrait = images::icon(data.champion_icon(a).as_deref(), 72., cx);
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
                    ),
            )
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
                let icon = images::icon(data.champion_icon(key).as_deref(), 24., cx);
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
        let portrait_a = ring(images::round(data.champion_icon(a).as_deref(), 104., cx), RED);
        let portrait_b = ring(images::round(data.champion_icon(b).as_deref(), 104., cx), BLUE);

        // The head-to-head numbers: deeplol's live matchup stats on the live
        // patch, otherwise the selected patch's matchup list.
        let (h2h, source) = if self.is_latest_patch() {
            (
                self.h2h.ready().and_then(|m| m.get(&self.lane)).map(|h| (h.games, h.my_win_rate, h.enemy_win_rate)),
                "deeplol live matchup data".to_string(),
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
        let stats = if left { &self.a_stats } else { &self.b_stats };
        let mut weak: Vec<_> = self.lane_stats(stats).map(|s| s.match_up.weak_against.clone()).unwrap_or_default();
        weak.sort_by(|x, y| x.win_rate.total_cmp(&y.win_rate));
        let weak_icons: Vec<AnyElement> = weak
            .iter()
            .take(5)
            .map(|m| {
                let icon = images::round(data.champion_icon(m.enemy_champion_id).as_deref(), 30., cx);
                let enemy = m.enemy_champion_id;
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
            .map(|(i, url)| {
                let icon = images::icon(Some(url), 30., cx);
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
            Loadable::Failed(e) => div().text_color(rgb(MUTED)).text_size(px(13.)).child(format!("No tips: {e}")).into_any_element(),
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

    fn render_builds(&mut self, data: &StaticData, cx: &mut Context<Self>) -> AnyElement {
        let Some(a) = self.a else { return div().into_any_element() };
        let name = data.champion_name(a);
        let builds = self.lane_stats(&self.a_stats).map(|s| s.build_lst.clone()).unwrap_or_default();
        if builds.is_empty() {
            return div().into_any_element();
        }
        let rows: Vec<AnyElement> = builds
            .iter()
            .enumerate()
            .take(2)
            .map(|(i, b)| {
                let runes = b.runes().map(|p| rune_page_row(data, &p, 28., cx));
                let spells: Vec<AnyElement> =
                    b.spell.build.iter().map(|&s| images::icon(data.spell_icon(s).as_deref(), 28., cx)).collect();
                let start: Vec<AnyElement> =
                    b.start_item.build.iter().map(|&it| images::icon(Some(&data.item_icon(it)), 28., cx)).collect();
                let items: Vec<AnyElement> =
                    b.item.build.iter().map(|&it| images::icon(Some(&data.item_icon(it)), 28., cx)).collect();
                let skills = skill_order_row(&b.skill.detail, 15);
                let page = b.runes();
                let spell_pair = match b.spell.build.as_slice() {
                    [s1, s2, ..] => Some((*s1, *s2)),
                    _ => None,
                };
                let label = format!("{name} {}", self.lane.label());
                let apply = page.map(|page| {
                    primary_button(SharedString::from(format!("apply-build-{i}")), "Add runes").on_click(cx.listener(
                        move |this, _, _, cx| this.apply(label.clone(), page.clone(), spell_pair, cx),
                    ))
                });
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
                            .child(div().text_size(px(12.)).text_color(rgb(MUTED)).child(format!(
                                "{} win rate · {} pick rate · {} games",
                                pct(b.win_rate * 100.0),
                                pct(b.pick_rate * 100.0),
                                thousands(b.games)
                            )))
                            .child(div().flex_1())
                            .children(apply),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .gap(px(22.))
                            .children(runes)
                            .child(labeled("Spells", div().flex().gap(px(4.)).children(spells)))
                            .child(labeled("Starter", div().flex().gap(px(4.)).children(start)))
                            .child(labeled("Core", div().flex().gap(px(4.)).children(items))),
                    )
                    .child(skills)
                    .into_any_element()
            })
            .collect();
        card()
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(section_title(format!("{name} {} builds · {}", self.lane.label(), self.patch_note())))
            .children(rows)
            .into_any_element()
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
