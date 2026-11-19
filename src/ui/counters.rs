//! The Counters tab (lolalytics.com/lol/<champion>/counters/).

use gpui::{div, prelude::*, px, AnyElement, Context, FontWeight, SharedString, Window};

use super::matchup::lane_chips;
use super::theme::*;
use super::tips;
use super::{AppView, Loadable, Slot, SortCol};
use crate::data::lolalytics::Counter;

impl AppView {
    pub fn render_counters(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let data = self.data.clone().unwrap();
        let input = self.input_c.clone();
        let field = self.champ_field("sug-c", Slot::C, &input, self.c_champ, &data, window, cx);
        let lanes = lane_chips("clane", self.c_lane, cx, |this, lane, cx| this.set_counter_lane(lane, cx));
        let name = self.c_champ.map(|k| data.champion_name(k)).unwrap_or_default();
        let patch = self.patch_label().to_string();

        let body = match &self.counters {
            Loadable::Idle => div().text_color(rgb(MUTED)).child("Pick a champion.").into_any_element(),
            Loadable::Loading => div().text_color(rgb(MUTED)).child("Loading counters…").into_any_element(),
            Loadable::Failed(e) => div().text_color(rgb(RED)).text_size(px(13.)).child(e.clone()).into_any_element(),
            Loadable::Ready(c) => {
                let c = c.clone();
                let mut list: Vec<Counter> = c.counters.clone();
                let (col, asc) = self.sort;
                list.sort_by(|x, y| {
                    let o = match col {
                        SortCol::Champion => data.champion_name(x.cid).cmp(&data.champion_name(y.cid)),
                        SortCol::VsWr => x.vs_wr.total_cmp(&y.vs_wr),
                        SortCol::AllWr => x.all_wr.total_cmp(&y.all_wr),
                        SortCol::D1 => x.d1.total_cmp(&y.d1),
                        SortCol::D2 => x.d2.total_cmp(&y.d2),
                        SortCol::Games => x.n.cmp(&y.n),
                    };
                    if asc { o } else { o.reverse() }
                });
                let hardest: Vec<String> = {
                    let mut v = c.counters.clone();
                    v.sort_by(|x, y| x.vs_wr.total_cmp(&y.vs_wr));
                    v.iter().take(3).map(|x| data.champion_name(x.cid)).collect()
                };
                let easiest: Vec<String> = {
                    let mut v = c.counters.clone();
                    v.sort_by(|x, y| y.vs_wr.total_cmp(&x.vs_wr));
                    v.iter().take(3).map(|x| data.champion_name(x.cid)).collect()
                };
                let me = self.c_champ.unwrap_or(0);
                let lane = self.c_lane;
                let rows: Vec<AnyElement> = list
                    .iter()
                    .map(|x| {
                        let icon = tips::champ(&data, x.cid, 30., false, cx);
                        let enemy = x.cid;
                        div()
                            .id(SharedString::from(format!("counter-{}", x.cid)))
                            .flex()
                            .items_center()
                            .px(px(10.))
                            .py(px(6.))
                            .rounded(px(6.))
                            .cursor_pointer()
                            .hover(|s| s.bg(rgb(PANEL_2)))
                            .on_click(cx.listener(move |this, _, _, cx| this.open_matchup(me, enemy, lane, cx)))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(10.))
                                    .w(px(220.))
                                    .child(icon)
                                    .child(div().text_size(px(14.)).font_weight(FontWeight::SEMIBOLD).child(data.champion_name(x.cid))),
                            )
                            .child(cell(pct_2(x.vs_wr), wr_color(x.vs_wr), true))
                            .child(cell(pct_2(x.all_wr), TEXT, false))
                            .child(cell(signed(x.d1), delta_color(x.d1), false))
                            .child(cell(signed(x.d2), delta_color(x.d2), false))
                            .child(cell(thousands(x.n), MUTED, false))
                            .into_any_element()
                    })
                    .collect();
                let header = |label: &'static str, sc: SortCol, w: f32, this: &Self, cx: &mut Context<Self>| {
                    let (cur, asc) = this.sort;
                    let arrow = if cur == sc { if asc { " ▲" } else { " ▼" } } else { "" };
                    div()
                        .id(label)
                        .w(px(w))
                        .text_size(px(12.))
                        .text_color(rgb(if cur == sc { ACCENT } else { MUTED }))
                        .cursor_pointer()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.sort = if this.sort.0 == sc { (sc, !this.sort.1) } else { (sc, sc == SortCol::VsWr || sc == SortCol::Champion) };
                            cx.notify();
                        }))
                        .child(format!("{label}{arrow}"))
                };
                div()
                    .flex()
                    .flex_col()
                    .gap(px(14.))
                    .child(
                        div()
                            .flex()
                            .gap(px(26.))
                            .child(stat("Win Rate", pct(c.win_rate)))
                            .child(stat("Pick Rate", pct(c.pick_rate)))
                            .child(stat("Ban Rate", pct(c.ban_rate)))
                            .child(stat("Average Emerald+ Win Rate", pct(c.avg_wr)))
                            .child(stat("Champions Analysed", thousands(c.analysed))),
                    )
                    .child(div().text_size(px(13.)).text_color(rgb(0xc4c6ce)).child(format!(
                        "{name} {} on patch {patch} counters {} best, and is countered most by {}. \
                         Matchups with at least ~100 games. Click a champion to open the matchup.",
                        lane.label(),
                        easiest.join(", "),
                        hardest.join(", ")
                    )))
                    .child(
                        div()
                            .flex()
                            .px(px(10.))
                            .pb(px(4.))
                            .border_b_1()
                            .border_color(rgb(BORDER))
                            .child(header("Name", SortCol::Champion, 220., self, cx))
                            .child(header("WR vs", SortCol::VsWr, 110., self, cx))
                            .child(header("All Champs WR", SortCol::AllWr, 110., self, cx))
                            .child(header("Δ1", SortCol::D1, 110., self, cx))
                            .child(header("Δ2", SortCol::D2, 110., self, cx))
                            .child(header("Games", SortCol::Games, 110., self, cx)),
                    )
                    .child(div().flex().flex_col().children(rows))
                    .into_any_element()
            }
        };

        div()
            .flex()
            .flex_col()
            .gap(px(14.))
            .p(px(18.))
            .child(
                card()
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .child(div().text_size(px(22.)).font_weight(FontWeight::BOLD).child(if name.is_empty() {
                        "Counters".to_string()
                    } else {
                        format!("{name} Counters")
                    }))
                    .child(div().text_size(px(13.)).text_color(rgb(MUTED)).child(format!(
                        "{name} {} vs {} counters on patch {patch}. \
                         Δ1 = win rate difference vs normal; Δ2 = also adjusted for the enemy's own win rate.",
                        self.c_lane.label(),
                        self.c_lane.label()
                    ))),
            )
            .child(div().flex().items_center().gap(px(14.)).child(field).child(lanes))
            .child(card().child(body))
            .into_any_element()
    }
}

fn cell(text: String, color: u32, bold: bool) -> gpui::Div {
    div()
        .w(px(110.))
        .text_size(px(14.))
        .text_color(rgb(color))
        .when(bold, |d| d.font_weight(FontWeight::BOLD))
        .child(text)
}

fn pct_2(v: f64) -> String {
    format!("{v:.2}%")
}

fn signed(v: f64) -> String {
    format!("{}{v:.2}", if v > 0.0 { "+" } else { "" })
}
