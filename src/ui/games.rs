//! deeplol's list of real games of the matchup (pros and high-elo players),
//! each with its full build and an "Add runes" button.

use gpui::{div, prelude::*, px, AnyElement, App, Context, FontWeight, SharedString};

use super::matchup::labeled;
use super::theme::*;
use super::{AppView, Loadable};
use crate::data::ddragon::StaticData;
use crate::data::deeplol::{Game, RunePage};
use crate::images;

/// Primary tree + keystone + 3 runes, secondary tree + 2 runes, 3 shards.
pub fn rune_page_row(data: &StaticData, page: &RunePage, size: f32, cx: &mut App) -> AnyElement {
    let rune = |id: u32, s: f32, cx: &mut App| {
        div()
            .id(SharedString::from(format!("rune-{id}-{s}")))
            .rounded_full()
            .bg(rgb(0x15151a))
            .child(images::round(data.rune_icon(id).as_deref(), s, cx))
            .tooltip(tooltip_text(data.rune_name(id)))
            .into_any_element()
    };
    let p = page.perks;
    let primary = div()
        .flex()
        .items_center()
        .gap(px(4.))
        .child(rune(page.primary_style, size * 0.75, cx))
        .child(rune(p[0], size * 1.35, cx))
        .child(rune(p[1], size, cx))
        .child(rune(p[2], size, cx))
        .child(rune(p[3], size, cx));
    let secondary = div()
        .flex()
        .items_center()
        .gap(px(4.))
        .child(rune(page.sub_style, size * 0.75, cx))
        .child(rune(p[4], size, cx))
        .child(rune(p[5], size, cx));
    let shards = div()
        .flex()
        .items_center()
        .gap(px(3.))
        .child(rune(p[6], size * 0.7, cx))
        .child(rune(p[7], size * 0.7, cx))
        .child(rune(p[8], size * 0.7, cx));
    labeled(
        "Runes",
        div().flex().items_center().gap(px(14.)).child(primary).child(secondary).child(shards),
    )
    .into_any_element()
}

fn tooltip_text(text: String) -> impl Fn(&mut gpui::Window, &mut App) -> gpui::AnyView + 'static {
    move |_, cx| cx.new(|_| Tooltip(text.clone())).into()
}

struct Tooltip(String);

impl Render for Tooltip {
    fn render(&mut self, _: &mut gpui::Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .px(px(8.))
            .py(px(4.))
            .rounded(px(5.))
            .bg(rgb(0x111115))
            .border_1()
            .border_color(rgb(BORDER))
            .text_size(px(12.))
            .text_color(rgb(TEXT))
            .child(self.0.clone())
    }
}

/// Q W E R boxes for each level, R highlighted.
pub fn skill_order_row(levels: &[u32], max: usize) -> AnyElement {
    let boxes = levels.iter().take(max).map(|&s| {
        let is_r = s == 4;
        div()
            .w(px(22.))
            .h(px(22.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(4.))
            .text_size(px(11.))
            .font_weight(FontWeight::BOLD)
            .when(is_r, |d| d.bg(rgb(ACCENT)).text_color(rgb(0x10201c)))
            .when(!is_r, |d| d.bg(rgb(0x1f3a36)).text_color(rgb(ACCENT)))
            .child(skill_letter(s))
    });
    labeled("Skill order", div().flex().gap(px(3.)).children(boxes)).into_any_element()
}

impl AppView {
    pub fn render_games(&mut self, data: &StaticData, cx: &mut Context<Self>) -> AnyElement {
        let (Some(a), Some(b)) = (self.a, self.b) else { return div().into_any_element() };
        let pros_only = self.pros_only;
        let games: Vec<Game> = self.games.iter().filter(|g| !pros_only || g.is_pro()).cloned().collect();
        let rows: Vec<AnyElement> = games.iter().map(|g| self.render_game(data, g, cx)).collect();
        let footer = match &self.games_state {
            Loadable::Loading => div().text_color(rgb(MUTED)).text_size(px(13.)).child("Loading games…").into_any_element(),
            Loadable::Failed(e) => div().text_color(rgb(RED)).text_size(px(13.)).child(format!("Couldn't load games: {e}")).into_any_element(),
            _ if self.games_done => div()
                .text_color(rgb(MUTED))
                .text_size(px(13.))
                .child(if self.games.is_empty() { "No recorded games of this matchup yet." } else { "That's every game deeplol has." })
                .into_any_element(),
            _ => button("more-games", "Load more games")
                .on_click(cx.listener(|this, _, _, cx| this.load_more_games(cx)))
                .into_any_element(),
        };
        let filter = |id: &'static str, label: &'static str, active: bool, value: bool, cx: &mut Context<Self>| {
            div()
                .id(id)
                .px(px(10.))
                .py(px(4.))
                .rounded(px(5.))
                .text_size(px(12.))
                .cursor_pointer()
                .border_1()
                .when(active, |d| d.border_color(rgb(ACCENT)).text_color(rgb(ACCENT)))
                .when(!active, |d| d.border_color(rgb(BORDER)).text_color(rgb(MUTED)))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.pros_only = value;
                    cx.notify();
                }))
                .child(label)
        };
        let pro_count = self.games.iter().filter(|g| g.is_pro()).count();
        card()
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(section_title(format!(
                        "Pro & high-elo games · {} vs {}",
                        data.champion_name(a),
                        data.champion_name(b)
                    )))
                    .child(div().flex_1())
                    .child(filter("all-games", "All", !pros_only, false, cx))
                    .child(filter("pro-games", if pro_count > 0 { "Pros only" } else { "Pros only (none loaded yet)" }, pros_only, true, cx)),
            )
            .child(div().text_size(px(11.)).text_color(rgb(MUTED)).child(
                "deeplol's latest games (not filtered by patch). Click a game for its full build.",
            ))
            .children(rows)
            .child(div().flex().justify_center().child(footer))
            .into_any_element()
    }

    fn render_game(&mut self, data: &StaticData, g: &Game, cx: &mut Context<Self>) -> AnyElement {
        let expanded = self.expanded.as_deref() == Some(g.match_id.as_str());
        let win = g.win == 1;
        let accent = if win { BLUE } else { RED };
        let page = g.runes();
        let kda = if g.deaths == 0 { (g.kills + g.assists) as f64 } else { (g.kills + g.assists) as f64 / g.deaths as f64 };

        let me = images::round(data.champion_icon(g.champion_id).as_deref(), 40., cx);
        let them = images::round(data.champion_icon(g.enemy_champion_id).as_deref(), 34., cx);
        let keystone = images::round(data.rune_icon(page.perks[0]).as_deref(), 26., cx);
        let sub = images::round(data.rune_icon(page.sub_style).as_deref(), 20., cx);
        let spells: Vec<AnyElement> =
            [g.spell.spell_1, g.spell.spell_2].iter().map(|&s| images::icon(data.spell_icon(s).as_deref(), 22., cx)).collect();
        let items: Vec<AnyElement> =
            g.item_final.iter().take(7).map(|&it| images::icon(Some(&data.item_icon(it)), 26., cx)).collect();

        let name = format!("{} {} vs {}", g.display_name(), data.champion_name(g.champion_id), data.champion_name(g.enemy_champion_id));
        let spell_pair = Some((g.spell.spell_1, g.spell.spell_2));
        let page_for_click = page.clone();
        let id = g.match_id.clone();
        let player = div()
            .flex()
            .flex_col()
            .gap(px(2.))
            .w(px(170.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .when(g.is_pro(), |d| {
                        d.child(
                            div()
                                .px(px(5.))
                                .rounded(px(3.))
                                .bg(rgb(GOLD))
                                .text_color(rgb(0x1a1a1a))
                                .text_size(px(10.))
                                .font_weight(FontWeight::BOLD)
                                .child("PRO"),
                        )
                    })
                    .child(div().text_size(px(13.)).font_weight(FontWeight::SEMIBOLD).truncate().child(g.display_name())),
            )
            .child(div().text_size(px(11.)).text_color(rgb(MUTED)).child(format!(
                "{} · {}",
                if g.tier.is_empty() { "Ranked".to_string() } else { g.tier.clone() },
                if g.player_type == "OTP" { "One-trick" } else { "" }
            )))
            .child(div().text_size(px(11.)).text_color(rgb(MUTED)).child(format!(
                "{} | {}",
                time_ago(g.game_end_time),
                duration(g.game_duration)
            )));

        let summary = div()
            .id(SharedString::from(format!("game-{}", g.match_id)))
            .flex()
            .items_center()
            .gap(px(16.))
            .px(px(12.))
            .py(px(10.))
            .cursor_pointer()
            .on_click(cx.listener(move |this, _, _, cx| {
                this.expanded = if this.expanded.as_deref() == Some(id.as_str()) { None } else { Some(id.clone()) };
                cx.notify();
            }))
            .child(div().w(px(4.)).h(px(44.)).rounded(px(2.)).bg(rgb(accent)))
            .child(player)
            .child(div().flex().items_center().gap(px(6.)).child(me).child(div().text_size(px(11.)).text_color(rgb(MUTED)).child("vs")).child(them))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .w(px(80.))
                    .child(div().text_size(px(14.)).font_weight(FontWeight::BOLD).child(format!("{} / {} / {}", g.kills, g.deaths, g.assists)))
                    .child(div().text_size(px(11.)).text_color(rgb(MUTED)).child(format!("{kda:.2} KDA"))),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .child(div().text_size(px(10.)).text_color(rgb(MUTED)).child("AI-Score"))
                    .child(
                        div()
                            .px(px(8.))
                            .rounded(px(4.))
                            .bg(rgb(if win { 0x2a4fa0 } else { 0x8a3434 }))
                            .text_size(px(14.))
                            .font_weight(FontWeight::BOLD)
                            .child(format!("{:.0}", g.ai_score)),
                    ),
            )
            .child(div().flex().items_center().gap(px(3.)).child(keystone).child(sub))
            .child(div().flex().flex_col().gap(px(2.)).children(spells))
            .child(div().flex().gap(px(3.)).children(items))
            .child(div().flex_1())
            .child(
                primary_button(SharedString::from(format!("apply-{}", g.match_id)), "Add runes").on_click(cx.listener(
                    move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.apply(name.clone(), page_for_click.clone(), spell_pair, cx);
                    },
                )),
            )
            .child(div().text_color(rgb(MUTED)).child(if expanded { "▾" } else { "▸" }));

        let detail = expanded.then(|| self.render_game_detail(data, g, cx));
        div()
            .flex()
            .flex_col()
            .rounded(px(8.))
            .bg(rgb(if win { 0x232a3a } else { 0x33262a }))
            .child(summary)
            .children(detail)
            .into_any_element()
    }

    fn render_game_detail(&mut self, data: &StaticData, g: &Game, cx: &mut Context<Self>) -> AnyElement {
        let runes = rune_page_row(data, &g.runes(), 28., cx);
        let shards: Vec<String> = g.runes().perks[6..].iter().map(|&s| data.rune_name(s)).collect();
        let spells: Vec<AnyElement> =
            [g.spell.spell_1, g.spell.spell_2].iter().map(|&s| images::icon(data.spell_icon(s).as_deref(), 28., cx)).collect();
        let start: Vec<AnyElement> = g.item_start.iter().map(|&it| images::icon(Some(&data.item_icon(it)), 28., cx)).collect();
        let core: Vec<AnyElement> = g.item_core.iter().map(|&it| images::icon(Some(&data.item_icon(it)), 28., cx)).collect();
        let skills = skill_order_row(&g.skill_order(), 18);
        let timeline: Vec<AnyElement> = g
            .purchases()
            .into_iter()
            .map(|(minute, items)| {
                let icons: Vec<AnyElement> = items.iter().map(|&it| images::icon(Some(&data.item_icon(it)), 26., cx)).collect();
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(2.))
                    .child(div().flex().gap(px(2.)).children(icons))
                    .child(div().text_size(px(10.)).text_color(rgb(MUTED)).child(format!("{minute}m")))
                    .into_any_element()
            })
            .collect();
        div()
            .flex()
            .flex_col()
            .gap(px(14.))
            .px(px(16.))
            .pb(px(14.))
            .pt(px(4.))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(24.))
                    .child(runes)
                    .child(labeled("Spells", div().flex().gap(px(4.)).children(spells)))
                    .child(labeled("Starter", div().flex().gap(px(4.)).children(start)))
                    .child(labeled("Core", div().flex().gap(px(4.)).children(core))),
            )
            .child(div().text_size(px(11.)).text_color(rgb(MUTED)).child(format!("Shards: {}", shards.join(" · "))))
            .child(skills)
            .child(labeled("Item build", div().flex().flex_wrap().gap(px(10.)).children(timeline)))
            .into_any_element()
    }
}
