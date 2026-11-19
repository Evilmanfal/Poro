//! The Tier List tab (Poro's main page): U.GG's tier list with role
//! filters, and a champion page with deeplol's builds when a champion is
//! clicked.

use std::collections::HashMap;

use gpui::{div, prelude::*, px, AnyElement, Context, FontWeight, SharedString};

use super::theme::*;
use super::tips;
use super::{spawn_job, AppView, Loadable};
use crate::data::ddragon::{Lane, StaticData};
use crate::data::deeplol::{self, Build, LaneStats, SHARD_ROWS};
use crate::data::matchup_builds::build_style;
use crate::data::ugg::{self, TierEntry, TierList};
use crate::images;

/// U.GG's tier colours.
fn tier_color(tier: &str) -> u32 {
    match tier {
        "S+" => 0xff9b00,
        "S" => 0x3273fa,
        "A" => 0x7ea4f4,
        "B" => 0xffffff,
        "C" => 0xfcb1b2,
        _ => 0xff4e50,
    }
}

/// U.GG colours win rates by how good they are.
fn wr_tier_color(wr: f64) -> u32 {
    if wr < 45.0 {
        0xff4e50
    } else if wr < 48.5 {
        0xfcb1b2
    } else if wr < 51.5 {
        0xffffff
    } else if wr < 53.0 {
        0x7ea4f4
    } else if wr < 55.0 {
        0x3273fa
    } else {
        0xff9b00
    }
}

fn short_games(n: u64) -> String {
    if n >= 1000 {
        format!("{:.1}K", n as f64 / 1000.0)
    } else {
        n.to_string()
    }
}

#[derive(Default)]
pub struct TierState {
    pub list: Option<Loadable<TierList>>,
    loaded_for: String,
    /// None = all roles.
    pub role: Option<Lane>,
    /// The champion page being shown: (champion, lane).
    pub page: Option<(u32, Lane)>,
    pub page_stats: Option<Loadable<HashMap<Lane, LaneStats>>>,
    pub build: usize,
}

impl TierState {
    /// Forget the loaded list so it's fetched again (the old one stays on
    /// screen until the new one arrives).
    pub fn reset_for_refresh(&mut self) {
        self.loaded_for.clear();
    }
}

impl AppView {
    /// Loads U.GG's tier list for the selected patch (once per patch).
    pub(super) fn ensure_tier_list(&mut self, cx: &mut Context<Self>) {
        let patch = self.patch_label().to_string();
        if patch.is_empty() || self.tiers.loaded_for == patch {
            return;
        }
        self.tiers.loaded_for = patch.clone();
        // On a background refresh keep showing the current list until the new one is in.
        if !matches!(self.tiers.list, Some(Loadable::Ready(_))) {
            self.tiers.list = Some(Loadable::Loading);
        }
        let live = self.is_latest_patch();
        spawn_job(cx, move || ugg::tier_list(&patch, live), move |this, r, _| {
            if this.tiers.loaded_for == this.patch_label() {
                // A failed refresh keeps the list that's already showing.
                let keep = r.is_err() && matches!(this.tiers.list, Some(Loadable::Ready(_)));
                if !keep {
                    this.tiers.list = Some(Loadable::from(r));
                }
            }
        });
    }

    fn open_champion_page(&mut self, champ: u32, lane: Lane, cx: &mut Context<Self>) {
        self.tiers.page = Some((champ, lane));
        self.tiers.build = 0;
        self.load_skills(champ, cx);
        self.reload_champion_page(cx);
    }

    pub(super) fn reload_champion_page(&mut self, cx: &mut Context<Self>) {
        let Some((champ, _)) = self.tiers.page else { return };
        let (patch, _) = self.deeplol_patch();
        let live = self.is_latest_patch();
        self.tiers.page_stats = Some(Loadable::Loading);
        spawn_job(cx, move || deeplol::champion(champ, &patch, live), move |this, r, _| {
            if this.tiers.page.map(|p| p.0) == Some(champ) {
                this.tiers.page_stats = Some(Loadable::from(r));
            }
        });
        cx.notify();
    }

    pub(super) fn render_tier_list(&mut self, cx: &mut Context<Self>) -> AnyElement {
        self.ensure_tier_list(cx);
        let data = self.data.clone().unwrap();
        if self.tiers.page.is_some() {
            return self.render_champion_page(&data, cx);
        }
        let role = self.tiers.role;
        let role_name = match role {
            None => "All Roles",
            Some(Lane::Top) => "Top",
            Some(Lane::Jungle) => "Jungle",
            Some(Lane::Middle) => "Mid",
            Some(Lane::Bottom) => "ADC",
            Some(Lane::Support) => "Support",
        };
        let chip = |id: SharedString, icon: Option<String>, label: &'static str, active: bool, cx: &mut Context<Self>| {
            let icon = icon.map(|u| images::icon(Some(&u), 16.0, cx));
            div()
                .id(id)
                .flex()
                .items_center()
                .gap(px(6.))
                .h(px(32.))
                .px(px(14.))
                .rounded(px(6.))
                .border_1()
                .text_size(px(13.))
                .font_weight(FontWeight::SEMIBOLD)
                .cursor_pointer()
                .when(active, |d| d.border_color(rgb(ACCENT)).text_color(rgb(ACCENT)).bg(rgba(0x37c8ab1a)))
                .when(!active, |d| d.border_color(rgb(BORDER)).text_color(rgb(MUTED)).hover(|s| s.text_color(rgb(TEXT))))
                .children(icon)
                .child(label)
        };
        let mut chips: Vec<AnyElement> = vec![chip("tl-all".into(), None, "All", role.is_none(), cx)
            .on_click(cx.listener(|this, _, _, cx| {
                this.tiers.role = None;
                cx.notify();
            }))
            .into_any_element()];
        for (lane, label) in
            [(Lane::Top, "Top"), (Lane::Jungle, "Jungle"), (Lane::Middle, "Mid"), (Lane::Bottom, "ADC"), (Lane::Support, "Support")]
        {
            chips.push(
                chip(format!("tl-{label}").into(), Some(lane.icon()), label, role == Some(lane), cx)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.tiers.role = Some(lane);
                        cx.notify();
                    }))
                    .into_any_element(),
            );
        }

        let body = match &self.tiers.list {
            None | Some(Loadable::Idle) | Some(Loadable::Loading) => {
                div().p(px(20.)).text_color(rgb(MUTED)).child("Loading U.GG's tier list…").into_any_element()
            }
            Some(Loadable::Failed(e)) => div().p(px(20.)).text_color(rgb(RED)).child(format!("Couldn't load the tier list: {e}")).into_any_element(),
            Some(Loadable::Ready(list)) => {
                let entries: Vec<TierEntry> = list
                    .entries
                    .iter()
                    .filter(|e| e.pick_rate >= ugg::MIN_PICK_RATE && role.is_none_or(|r| r == e.lane))
                    .cloned()
                    .collect();
                let columns: Vec<AnyElement> = ["S+", "S", "A"]
                    .iter()
                    .map(|tier| {
                        let cards: Vec<AnyElement> =
                            entries.iter().filter(|e| e.tier() == *tier).map(|e| self.tier_card(&data, e, cx)).collect();
                        let color = tier_color(tier);
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .gap(px(10.))
                            .p(px(10.))
                            .rounded(px(8.))
                            .bg(rgb(PANEL))
                            .border_t_2()
                            .border_color(rgb(color))
                            .child(
                                div().flex().justify_center().child(
                                    div()
                                        .px(px(12.))
                                        .py(px(2.))
                                        .rounded(px(4.))
                                        .border_1()
                                        .border_color(rgb(color))
                                        .text_color(rgb(color))
                                        .font_weight(FontWeight::BOLD)
                                        .child(tier.to_string()),
                                ),
                            )
                            .child(div().flex().flex_wrap().gap(px(8.)).children(cards))
                            .into_any_element()
                    })
                    .collect();
                div().flex().items_start().gap(px(12.)).children(columns).into_any_element()
            }
        };
        let patch = self.tiers.list.as_ref().and_then(|l| l.ready()).map(|l| l.patch.clone()).unwrap_or_default();
        div()
            .flex()
            .flex_col()
            .gap(px(14.))
            .p(px(18.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .child(div().w(px(3.)).h(px(18.)).bg(rgb(ACCENT)))
                    .child(div().text_size(px(18.)).font_weight(FontWeight::BOLD).child("Tier List"))
                    .child(div().text_size(px(14.)).text_color(rgb(MUTED)).child(format!(
                        "The best champions for {role_name} in patch {patch}"
                    ))),
            )
            .child(div().flex().flex_wrap().gap(px(8.)).children(chips))
            .child(body)
            .into_any_element()
    }

    fn tier_card(&self, data: &StaticData, e: &TierEntry, cx: &mut Context<Self>) -> AnyElement {
        let (champ, lane) = (e.champion, e.lane);
        let portrait = images::round(data.champion_icon(champ).as_deref(), 52., cx);
        let role_icon = images::icon(Some(&lane.icon()), 14., cx);
        let tier_ring = tier_color(e.tier());
        div()
            .id(SharedString::from(format!("tier-{champ}-{}", lane.short())))
            .w(px(112.))
            .flex()
            .flex_col()
            .items_center()
            .gap(px(4.))
            .py(px(10.))
            .rounded(px(8.))
            .bg(rgb(PANEL_2))
            .cursor_pointer()
            .hover(|s| s.bg(rgb(BORDER)))
            .on_click(cx.listener(move |this, _, _, cx| this.open_champion_page(champ, lane, cx)))
            .child(
                div()
                    .relative()
                    .child(div().p(px(2.)).rounded_full().border_2().border_color(rgb(tier_ring)).child(portrait))
                    .child(div().absolute().bottom_0().right_0().rounded(px(3.)).bg(rgb(0x111115)).p(px(1.)).child(role_icon)),
            )
            .child(div().text_size(px(13.)).font_weight(FontWeight::BOLD).truncate().max_w(px(104.)).child(data.champion_name(champ)))
            .child(
                div()
                    .flex()
                    .gap(px(4.))
                    .text_size(px(12.))
                    .child(div().font_weight(FontWeight::BOLD).text_color(rgb(wr_tier_color(e.win_rate))).child(format!("{:.1}%", e.win_rate)))
                    .child(div().text_color(rgb(MUTED)).child("WR")),
            )
            .child(div().text_size(px(11.)).text_color(rgb(MUTED)).child(format!("{} games", short_games(e.games))))
            .into_any_element()
    }

    // ---- champion page ----

    fn render_champion_page(&mut self, data: &StaticData, cx: &mut Context<Self>) -> AnyElement {
        let (champ, lane) = self.tiers.page.unwrap();
        let name = data.champion_name(champ);
        let (dl_patch, _) = self.deeplol_patch();
        let stats: Option<LaneStats> = self.tiers.page_stats.as_ref().and_then(|s| s.ready()).and_then(|m| m.get(&lane).cloned());
        let loading = matches!(self.tiers.page_stats, Some(Loadable::Loading));
        let ugg_tier = self
            .tiers
            .list
            .as_ref()
            .and_then(|l| l.ready())
            .and_then(|l| l.entries.iter().find(|e| e.champion == champ && e.lane == lane).map(|e| e.tier()));

        let back = button("tl-back", "← Tier List").on_click(cx.listener(|this, _, _, cx| {
            this.tiers.page = None;
            cx.notify();
        }));
        let skills: Vec<AnyElement> = self
            .skills
            .get(&champ)
            .cloned()
            .unwrap_or_default()
            .iter()
            .enumerate()
            .map(|(i, sk)| {
                div()
                    .relative()
                    .child(tips::skill(sk, 28., cx))
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
        let stat_row = match &stats {
            Some(s) => div()
                .flex()
                .gap(px(24.))
                .p(px(12.))
                .rounded(px(8.))
                .border_1()
                .border_color(rgb(BORDER))
                .child(stat("Tier", s.champion_tier.map(|t| t.to_string()).unwrap_or("–".into())))
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
                .children(ugg_tier.map(|t| stat("U.GG Tier", t.to_string())))
                .into_any_element(),
            None => div()
                .text_color(rgb(MUTED))
                .child(if loading { "Loading deeplol's builds…".to_string() } else { format!("deeplol has no {} data for {name}.", lane.label()) })
                .into_any_element(),
        };
        let header = card()
            .flex()
            .gap(px(16.))
            .child(images::icon(data.champion_icon(champ).as_deref(), 96., cx))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .child(
                        div()
                            .flex()
                            .items_baseline()
                            .gap(px(10.))
                            .child(div().text_size(px(24.)).font_weight(FontWeight::BOLD).child(name.clone()))
                            .child(div().text_size(px(13.)).text_color(rgb(MUTED)).child(format!(
                                "{} Build, Emerald+, Patch {dl_patch}",
                                lane.label()
                            ))),
                    )
                    .child(div().flex().gap(px(4.)).children(skills))
                    .child(stat_row),
            );

        let builds: Vec<Build> = stats.as_ref().map(|s| s.build_lst.clone()).unwrap_or_default();
        let selected = self.tiers.build.min(builds.len().saturating_sub(1));
        let tabs: Vec<AnyElement> = builds
            .iter()
            .enumerate()
            .take(4)
            .map(|(i, b)| {
                let items: Vec<u32> = b.item.build.iter().copied().filter(|i| data.items.get(i).is_some_and(|x| x.legendary)).collect();
                let style = build_style(&items, &data.items);
                let keystone = b.rune.main_build.get(1).copied().unwrap_or(0);
                let first_item = items.first().copied();
                let active = i == selected;
                let wr = b.win_rate * 100.0;
                div()
                    .id(SharedString::from(format!("tl-build-{i}")))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .px(px(12.))
                    .py(px(8.))
                    .rounded_t(px(8.))
                    .border_1()
                    .border_color(rgb(if active { ACCENT } else { BORDER }))
                    .bg(rgb(if active { PANEL_2 } else { PANEL }))
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.tiers.build = i;
                        cx.notify();
                    }))
                    .child(tips::rune(data, keystone, 26., cx))
                    .children(first_item.map(|it| tips::item(data, it, 26., cx)))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .child(div().text_size(px(13.)).font_weight(FontWeight::BOLD).child(style))
                            .child(div().text_size(px(11.)).text_color(rgb(MUTED)).child(format!("{} games", thousands(b.games)))),
                    )
                    .child(
                        div()
                            .px(px(6.))
                            .rounded(px(4.))
                            .border_1()
                            .border_color(rgb(if wr >= 50.0 { BLUE } else { RED }))
                            .text_color(rgb(if wr >= 50.0 { BLUE } else { RED }))
                            .text_size(px(12.))
                            .font_weight(FontWeight::BOLD)
                            .child(format!("{wr:.1} %")),
                    )
                    .into_any_element()
            })
            .collect();
        let panel = builds.get(selected).map(|b| self.render_build_panel(data, &name, lane, b, cx));

        div()
            .flex()
            .flex_col()
            .gap(px(14.))
            .p(px(18.))
            .child(div().flex().child(back))
            .child(header)
            .child(div().flex().gap(px(6.)).children(tabs))
            .children(panel)
            .into_any_element()
    }

    fn render_build_panel(&mut self, data: &StaticData, name: &str, lane: Lane, b: &Build, cx: &mut Context<Self>) -> AnyElement {
        let items: Vec<u32> = b.item.build.iter().copied().filter(|i| data.items.get(i).is_some_and(|x| x.legendary)).collect();
        let style = build_style(&items, &data.items);
        let runes = self.rune_trees_view(data, b, cx);

        let spells: Vec<AnyElement> = b.spell.build.iter().map(|&s| tips::spell(data, s, 36., cx)).collect();
        let core: Vec<AnyElement> = b
            .item
            .build
            .iter()
            .take(5)
            .enumerate()
            .map(|(i, &it)| {
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .when(i > 0, |d| d.child(div().text_color(rgb(MUTED)).child("›")))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .items_center()
                            .gap(px(2.))
                            .child(tips::item(data, it, 36., cx))
                            .child(div().text_size(px(11.)).text_color(rgb(MUTED)).child(format!("{} Core", i + 1))),
                    )
                    .into_any_element()
            })
            .collect();
        let path: Vec<AnyElement> = b
            .item
            .detail
            .iter()
            .enumerate()
            .map(|(i, &it)| {
                let price = b.item.detail_price.get(i).copied();
                let is_core = b.item.build.contains(&it);
                div()
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .when(i > 0, |d| d.child(div().text_color(rgb(MUTED)).child("›")))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .items_center()
                            .gap(px(2.))
                            .child(
                                div()
                                    .rounded(px(5.))
                                    .border_2()
                                    .border_color(rgb(if is_core { ACCENT } else { 0x00000000 }))
                                    .child(tips::item(data, it, 30., cx)),
                            )
                            .children(price.map(|p| {
                                div()
                                    .text_size(px(11.))
                                    .when(is_core, |d| d.font_weight(FontWeight::BOLD))
                                    .text_color(rgb(if is_core { TEXT } else { MUTED }))
                                    .child(p.to_string())
                            })),
                    )
                    .into_any_element()
            })
            .collect();
        let max_order: Vec<AnyElement> = b
            .skill
            .build
            .iter()
            .enumerate()
            .map(|(i, &sk)| {
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .when(i > 0, |d| d.child(div().text_color(rgb(MUTED)).child("›")))
                    .child(div().text_size(px(13.)).font_weight(FontWeight::BOLD).text_color(rgb(ACCENT)).child(skill_letter(sk)))
                    .into_any_element()
            })
            .collect();
        let grid = skill_grid(&b.skill.detail);
        let page = b.runes();
        let spell_pair = match b.spell.build.as_slice() {
            [a, c, ..] => Some((*a, *c)),
            _ => None,
        };
        let label = format!("{name} {style}");
        let apply = page.map(|page| {
            primary_button("tl-apply", "Add runes")
                .on_click(cx.listener(move |this, _, _, cx| this.apply(label.clone(), page.clone(), spell_pair, cx)))
        });
        let tag = |t: &str| {
            div()
                .px(px(10.))
                .py(px(3.))
                .rounded(px(5.))
                .bg(rgba(0x37c8ab26))
                .text_color(rgb(ACCENT))
                .text_size(px(13.))
                .font_weight(FontWeight::BOLD)
                .child(t.to_string())
        };
        card()
            .flex()
            .gap(px(28.))
            .border_color(rgb(ACCENT))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(8.))
                    .child(tag(&format!("[{style}] {name} Build")))
                    .child(div().text_size(px(12.)).text_color(rgb(MUTED)).child(format!(
                        "Pick Rate {:.1}% ({}) | WR {:.1}% · {}",
                        b.pick_rate * 100.0,
                        thousands(b.games),
                        b.win_rate * 100.0,
                        lane.label()
                    )))
                    .child(runes)
                    .children(apply),
            )
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap(px(14.))
                    .child(
                        div()
                            .flex()
                            .gap(px(28.))
                            .child(div().flex().flex_col().gap(px(8.)).child(tag("Spells")).child(div().flex().gap(px(6.)).children(spells)))
                            .child(div().flex().flex_col().gap(px(8.)).child(tag("Core Item Build Order")).child(div().flex().items_center().children(core))),
                    )
                    .child(div().flex().flex_col().gap(px(8.)).child(tag("Core Item Build Path")).child(div().flex().flex_wrap().items_center().gap(px(2.)).children(path)))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(8.))
                            .child(div().flex().items_center().gap(px(12.)).child(tag("Skill Order")).children(max_order))
                            .child(grid),
                    ),
            )
            .into_any_element()
    }

    /// Both rune trees with every rune shown and the picked ones lit, like
    /// the game's rune page, plus the stat shards.
    fn rune_trees_view(&mut self, data: &StaticData, b: &Build, cx: &mut Context<Self>) -> AnyElement {
        let Some(page) = b.runes() else { return div().into_any_element() };
        let picked = |id: u32| page.perks.contains(&id) || id == page.primary_style || id == page.sub_style;
        let rune = |id: u32, size: f32, cx: &mut Context<Self>| {
            if picked(id) {
                div().rounded_full().border_2().border_color(rgb(GOLD)).child(tips::rune(data, id, size, cx)).into_any_element()
            } else {
                tips::rune_dim(data, id, size, cx)
            }
        };
        let tree_row = |exclude: Option<u32>, chosen: u32, cx: &mut Context<Self>| {
            let icons: Vec<AnyElement> = data
                .rune_trees
                .iter()
                .filter(|t| Some(t.id) != exclude)
                .map(|t| if t.id == chosen { tips::rune(data, t.id, 22., cx) } else { tips::rune_dim(data, t.id, 22., cx) })
                .collect();
            div().flex().justify_center().gap(px(6.)).children(icons)
        };
        let primary = data.rune_trees.iter().find(|t| t.id == page.primary_style).cloned();
        let secondary = data.rune_trees.iter().find(|t| t.id == page.sub_style).cloned();
        let mut left = div().flex().flex_col().items_center().gap(px(10.)).child(tree_row(None, page.primary_style, cx));
        if let Some(t) = primary {
            for (i, slot) in t.slots.iter().enumerate() {
                let size = if i == 0 { 40. } else { 34. };
                let icons: Vec<AnyElement> = slot.iter().map(|&id| rune(id, size, cx)).collect();
                left = left.child(div().flex().gap(px(10.)).children(icons));
            }
        }
        let mut right = div().flex().flex_col().items_center().gap(px(10.)).child(tree_row(Some(page.primary_style), page.sub_style, cx));
        if let Some(t) = secondary {
            for slot in t.slots.iter().skip(1) {
                let icons: Vec<AnyElement> = slot.iter().map(|&id| rune(id, 30., cx)).collect();
                right = right.child(div().flex().gap(px(10.)).children(icons));
            }
        }
        // Stat shards, one row per slot, the picked one lit.
        for (row, choices) in SHARD_ROWS.iter().enumerate() {
            let picked_shard = page.perks[6 + row];
            let icons: Vec<AnyElement> = choices
                .iter()
                .map(|&id| {
                    if id == picked_shard {
                        div().rounded_full().border_1().border_color(rgb(ACCENT)).child(tips::rune(data, id, 20., cx)).into_any_element()
                    } else {
                        tips::rune_dim(data, id, 20., cx)
                    }
                })
                .collect();
            right = right.child(div().flex().gap(px(12.)).children(icons));
        }
        div().flex().gap(px(24.)).child(left).child(right).into_any_element()
    }
}

/// Q/W/E/R rows × levels, with the level number where each point goes.
fn skill_grid(levels: &[u32]) -> AnyElement {
    let rows: Vec<AnyElement> = (1..=4u32)
        .map(|skill| {
            let cells: Vec<AnyElement> = (0..levels.len().max(15))
                .map(|lv| {
                    let here = levels.get(lv) == Some(&skill);
                    div()
                        .size(px(24.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(4.))
                        .text_size(px(11.))
                        .font_weight(FontWeight::BOLD)
                        .bg(rgb(if here { ACCENT } else { PANEL_2 }))
                        .text_color(rgb(0x0f1f1c))
                        .when(here, |d| d.child(format!("{}", lv + 1)))
                        .into_any_element()
                })
                .collect();
            div()
                .flex()
                .items_center()
                .gap(px(3.))
                .child(div().w(px(18.)).text_size(px(12.)).font_weight(FontWeight::BOLD).text_color(rgb(ACCENT)).child(skill_letter(skill)))
                .children(cells)
                .into_any_element()
        })
        .collect();
    div().flex().flex_col().gap(px(3.)).children(rows).into_any_element()
}
