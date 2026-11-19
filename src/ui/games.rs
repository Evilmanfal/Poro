//! deeplol's list of real games of the matchup (pros and high-elo players),
//! each with its full build and an "Add Build" button.

use gpui::{div, prelude::*, px, AnyElement, App, Context, FontWeight, SharedString};

use super::matchup::labeled;
use super::theme::*;
use super::tips;
use super::{AppView, Loadable, PlayerFilter};
use crate::data::ddragon::Lane;
use crate::images;
use crate::data::ddragon::StaticData;
use crate::data::deeplol::{Game, RunePage};

/// Primary tree + keystone + 3 runes, secondary tree + 2 runes, 3 shards.
pub fn rune_page_row(data: &StaticData, page: &RunePage, size: f32, cx: &mut App) -> AnyElement {
    let rune = |id: u32, s: f32, cx: &mut App| {
        div().rounded_full().bg(rgb(0x15151a)).child(tips::rune(data, id, s, cx)).into_any_element()
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
        let patch_games = self.patch_games();
        let lane_filter = self.games_lane;
        let player_filter = self.games_player;
        let all: Vec<Game> = patch_games
            .iter()
            .filter(|g| lane_filter.is_none() || g.lane() == lane_filter)
            .filter(|g| match player_filter {
                PlayerFilter::All => true,
                PlayerFilter::Otp => g.is_otp(),
                PlayerFilter::Pro => g.is_pro(),
            })
            .cloned()
            .collect();
        let shown = self.games_shown.min(all.len());
        let rows: Vec<AnyElement> = all[..shown].iter().map(|g| self.render_game(data, g, cx)).collect();
        let footer = match &self.history {
            Loadable::Loading => div().text_color(rgb(MUTED)).text_size(px(13.)).child("Loading games…").into_any_element(),
            Loadable::Failed(e) => div().text_color(rgb(RED)).text_size(px(13.)).child(format!("Couldn't load games: {e}")).into_any_element(),
            _ if all.is_empty() && !patch_games.is_empty() => div()
                .text_color(rgb(MUTED))
                .text_size(px(13.))
                .child("No games match these filters.")
                .into_any_element(),
            _ if all.is_empty() => div()
                .text_color(rgb(MUTED))
                .text_size(px(13.))
                .child(if self.is_latest_patch() {
                    "No recorded games of this matchup yet.".to_string()
                } else {
                    format!("deeplol has no games of this matchup on patch {}.", self.patch_label())
                })
                .into_any_element(),
            _ if shown >= all.len() => div()
                .text_color(rgb(MUTED))
                .text_size(px(13.))
                .child(format!("That's all {} games.", all.len()))
                .into_any_element(),
            _ => button("more-games", format!("Show more ({} more)", all.len() - shown))
                .on_click(cx.listener(|this, _, _, cx| {
                    this.games_shown += super::GAMES_PAGE;
                    cx.notify();
                }))
                .into_any_element(),
        };
        // deeplol's filter bar: ALL / TOP / JUG / MID / BOT / SUP, then ALL / OTP / PRO.
        let chip = |id: SharedString, icon: Option<AnyElement>, label: &'static str, active: bool| {
            div()
                .id(id)
                .flex()
                .items_center()
                .gap(px(6.))
                .h(px(30.))
                .px(px(12.))
                .rounded(px(5.))
                .text_size(px(12.))
                .font_weight(FontWeight::BOLD)
                .cursor_pointer()
                .border_1()
                .when(active, |d| d.border_color(rgb(ACCENT)).text_color(rgb(ACCENT)).bg(rgba(0x37c8ab1a)))
                .when(!active, |d| d.border_color(rgba(0x00000000)).text_color(rgb(ACCENT)).hover(|s| s.bg(rgb(PANEL_2))))
                .children(icon)
                .child(label)
        };
        let mut lane_chips: Vec<AnyElement> = vec![chip("glane-all".into(), None, "ALL", lane_filter.is_none())
            .on_click(cx.listener(|this, _, _, cx| this.set_games_filter(None, this.games_player, cx)))
            .into_any_element()];
        for lane in Lane::ALL {
            let icon = images::icon(Some(&lane.icon()), 16., cx);
            lane_chips.push(
                chip(format!("glane-{}", lane.short()).into(), Some(icon), lane.short(), lane_filter == Some(lane))
                    .on_click(cx.listener(move |this, _, _, cx| this.set_games_filter(Some(lane), this.games_player, cx)))
                    .into_any_element(),
            );
        }
        let player_chips: Vec<AnyElement> = [(PlayerFilter::All, "ALL"), (PlayerFilter::Otp, "OTP"), (PlayerFilter::Pro, "PRO")]
            .into_iter()
            .map(|(f, label)| {
                chip(format!("gplayer-{label}").into(), None, label, player_filter == f)
                    .on_click(cx.listener(move |this, _, _, cx| this.set_games_filter(this.games_lane, f, cx)))
                    .into_any_element()
            })
            .collect();
        let filter_bar = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(4.))
            .children(lane_chips)
            .child(div().mx(px(10.)).text_color(rgb(MUTED)).child("/"))
            .children(player_chips);
        let scope = if self.history.ready().is_some() { self.patch_games_label(&patch_games) } else { String::new() };
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
                    .child(div().text_size(px(12.)).text_color(rgb(MUTED)).child(format!("{} shown of {}", all.len(), patch_games.len()))),
            )
            .child(filter_bar)
            .child(div().text_size(px(11.)).text_color(rgb(MUTED)).child(if self.is_latest_patch() {
                format!("{scope} · every recent game deeplol has (pick an older patch to see only its games). Click a game for its full build.")
            } else {
                format!("{scope}. Click a game for its full build.")
            }))
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

        let me = tips::champ(data, g.champion_id, 40., true, cx);
        let them = tips::champ(data, g.enemy_champion_id, 34., true, cx);
        let keystone = tips::rune(data, page.perks[0], 26., cx);
        let sub = tips::rune(data, page.sub_style, 20., cx);
        let spells: Vec<AnyElement> =
            [g.spell.spell_1, g.spell.spell_2].iter().map(|&s| tips::spell(data, s, 22., cx)).collect();
        let items: Vec<AnyElement> =
            g.item_final.iter().take(7).map(|&it| tips::item(data, it, 26., cx)).collect();

        let name = format!("{} {} vs {}", g.display_name(), data.champion_name(g.champion_id), data.champion_name(g.enemy_champion_id));
        let import = super::BuildImport {
            name,
            champion: g.champion_id,
            page: Some(page.clone()),
            spells: Some((g.spell.spell_1, g.spell.spell_2)),
            blocks: vec![("Items from this game".into(), g.item_final.iter().copied().filter(|&i| i != 0).collect())],
        };
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
                    .when(g.is_pro() || g.is_streamer(), |d| {
                        let (label, color) = if g.is_pro() { ("PRO", GOLD) } else { ("STREAMER", 0xb48cff) };
                        d.child(
                            div()
                                .px(px(5.))
                                .rounded(px(3.))
                                .bg(rgb(color))
                                .text_color(rgb(0x1a1a1a))
                                .text_size(px(10.))
                                .font_weight(FontWeight::BOLD)
                                .child(label),
                        )
                    })
                    .child(div().text_size(px(13.)).font_weight(FontWeight::SEMIBOLD).truncate().child(g.display_name())),
            )
            .child(div().text_size(px(11.)).text_color(rgb(MUTED)).child({
                let tier = if g.tier.is_empty() { "Ranked".to_string() } else { g.tier.clone() };
                if g.is_otp() { format!("{tier} · One-trick") } else { tier }
            }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .children(g.patch.clone().map(|p| {
                        div()
                            .px(px(4.))
                            .rounded(px(3.))
                            .border_1()
                            .border_color(rgb(BORDER))
                            .text_size(px(10.))
                            .text_color(rgb(TEXT))
                            .child(p)
                    }))
                    .child(div().text_size(px(11.)).text_color(rgb(MUTED)).child(format!(
                        "{} | {}",
                        time_ago(g.game_end_time),
                        duration(g.game_duration)
                    ))),
            );

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
                primary_button(SharedString::from(format!("apply-{}", g.match_id)), "Add Build").on_click(cx.listener(
                    move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.apply_build(import.clone(), cx);
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
            [g.spell.spell_1, g.spell.spell_2].iter().map(|&s| tips::spell(data, s, 28., cx)).collect();
        let start: Vec<AnyElement> = g.item_start.iter().map(|&it| tips::item(data, it, 28., cx)).collect();
        let core: Vec<AnyElement> = g.item_core.iter().map(|&it| tips::item(data, it, 28., cx)).collect();
        let skills = skill_order_row(&g.skill_order(), 18);
        let timeline: Vec<AnyElement> = g
            .purchases()
            .into_iter()
            .map(|(minute, items)| {
                let icons: Vec<AnyElement> = items.iter().map(|&it| tips::item(data, it, 26., cx)).collect();
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
