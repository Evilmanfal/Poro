//! The Tier List tab (Poro's main page): U.GG's tier list with role
//! filters, and a champion page with U.GG's builds when a champion is
//! clicked.

use gpui::{div, prelude::*, px, AnyElement, Context, FontWeight, SharedString};

use super::theme::*;
use super::tips;
use super::{spawn_job, AppView, Loadable};
use crate::data::ddragon::{Lane, StaticData};
use crate::data::deeplol::{order_shards, RunePage, SHARD_ROWS};
use crate::data::matchup_builds::build_style;
use crate::data::ugg::{self, TierEntry, TierList, UggBuild};
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
    /// U.GG's builds for that champion and lane.
    pub page_builds: Option<Loadable<Vec<UggBuild>>>,
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
        self.tiers.page_builds = None;
        self.load_skills(champ, cx);
        self.reload_champion_page(cx);
    }

    pub(super) fn reload_champion_page(&mut self, cx: &mut Context<Self>) {
        let Some((champ, lane)) = self.tiers.page else { return };
        let patch = self.patch_label().to_string();
        let live = self.is_latest_patch();
        // On a background refresh keep the builds on screen until the new ones arrive.
        if !matches!(self.tiers.page_builds, Some(Loadable::Ready(_))) {
            self.tiers.page_builds = Some(Loadable::Loading);
        }
        spawn_job(cx, move || ugg::champion_builds(champ, lane, &patch, live), move |this, r, _| {
            if this.tiers.page == Some((champ, lane)) {
                let keep = r.is_err() && matches!(this.tiers.page_builds, Some(Loadable::Ready(_)));
                if !keep {
                    this.tiers.page_builds = Some(Loadable::from(r));
                }
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
        let list = self.tiers.list.as_ref().and_then(|l| l.ready());
        let patch = list.map(|l| l.patch.clone()).unwrap_or_else(|| self.patch_label().to_string());
        // The champion's U.GG stats and its rank in the role (as on the tier list).
        let entry = list.and_then(|l| l.entries.iter().find(|e| e.champion == champ && e.lane == lane).cloned());
        let rank = list.and_then(|l| {
            let mut in_role: Vec<&TierEntry> =
                l.entries.iter().filter(|e| e.lane == lane && e.pick_rate >= ugg::MIN_PICK_RATE).collect();
            in_role.sort_by(|a, b| b.stdevs.total_cmp(&a.stdevs));
            let pos = in_role.iter().position(|e| e.champion == champ)?;
            Some((pos + 1, in_role.len()))
        });
        let builds: Vec<UggBuild> = self.tiers.page_builds.as_ref().and_then(|b| b.ready()).cloned().unwrap_or_default();
        let loading = matches!(self.tiers.page_builds, Some(Loadable::Loading) | None);
        let failed = match &self.tiers.page_builds {
            Some(Loadable::Failed(e)) => Some(e.clone()),
            _ => None,
        };

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
        let stat_row = match &entry {
            Some(e) => div()
                .flex()
                .gap(px(24.))
                .p(px(12.))
                .rounded(px(8.))
                .border_1()
                .border_color(rgb(BORDER))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(4.))
                        .child(div().text_size(px(12.)).text_color(rgb(MUTED)).child("Tier"))
                        .child(div().text_size(px(16.)).font_weight(FontWeight::BOLD).text_color(rgb(tier_color(e.tier()))).child(e.tier())),
                )
                .child(stat("Ranking", rank.map(|(r, n)| format!("{r} / {n}")).unwrap_or("–".into())))
                .child(stat("Win Rate", pct(e.win_rate)))
                .child(stat("Pick Rate", pct(e.pick_rate)))
                .child(stat("Ban Rate", pct(e.ban_rate)))
                .child(stat("Games", thousands(e.games)))
                .into_any_element(),
            None => div().text_color(rgb(MUTED)).child(format!("No {} stats for {name} on this patch.", lane.label())).into_any_element(),
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
                                "{} Build, Emerald+, Patch {patch}",
                                lane.label()
                            ))),
                    )
                    .child(div().flex().gap(px(4.)).children(skills))
                    .child(stat_row),
            );

        let selected = self.tiers.build.min(builds.len().saturating_sub(1));
        let tabs: Vec<AnyElement> = builds
            .iter()
            .enumerate()
            .map(|(i, b)| {
                let style = build_name(data, b);
                let page = ugg_page(data, b);
                let keystone = page.as_ref().map(|p| p.perks[0]).unwrap_or(0);
                let first_item = b.core.iter().copied().find(|i| data.items.get(i).is_some_and(|x| x.legendary));
                let active = i == selected;
                let wr = b.wins as f64 * 100.0 / b.games.max(1) as f64;
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
                            .child(div().text_size(px(11.)).text_color(rgb(MUTED)).child(format!(
                                "{}{} games",
                                if i == 0 { "Recommended · " } else { "" },
                                thousands(b.games)
                            ))),
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
        let status = if builds.is_empty() {
            Some(
                div()
                    .text_color(rgb(MUTED))
                    .child(match failed {
                        Some(e) => format!("Couldn't load builds: {e}"),
                        None if loading => "Loading builds…".to_string(),
                        None => format!("No {} builds for {name}.", lane.label()),
                    })
                    .into_any_element(),
            )
        } else {
            None
        };
        let total_games = builds.first().map(|b| b.games).unwrap_or(0);
        let panel = builds.get(selected).map(|b| self.render_build_panel(data, &name, lane, b, total_games, cx));

        div()
            .flex()
            .flex_col()
            .gap(px(14.))
            .p(px(18.))
            .child(div().flex().child(back))
            .child(header)
            .child(div().flex().gap(px(6.)).children(tabs))
            .children(status)
            .children(panel)
            .into_any_element()
    }

    fn render_build_panel(
        &mut self,
        data: &StaticData,
        name: &str,
        lane: Lane,
        b: &UggBuild,
        total_games: u64,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let style = build_name(data, b);
        let page = ugg_page(data, b);
        let runes = page.as_ref().map(|p| self.rune_trees_view(data, p, cx)).unwrap_or_else(|| div().into_any_element());

        let spells: Vec<AnyElement> = b.spells.iter().map(|&s| tips::spell(data, s, 36., cx)).collect();
        let starter: Vec<AnyElement> = b.starter.iter().map(|&it| tips::item(data, it, 36., cx)).collect();
        let core: Vec<AnyElement> = b
            .core
            .iter()
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
        // The 4th, 5th and 6th item: each option with its win rate and games.
        let options: Vec<AnyElement> = ["4th Item", "5th Item", "6th Item"]
            .iter()
            .zip(b.item_options.iter())
            .map(|(title, opts)| {
                let rows: Vec<AnyElement> = opts
                    .iter()
                    .take(3)
                    .map(|o| {
                        let wr = o.wins as f64 * 100.0 / o.games.max(1) as f64;
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(tips::item(data, o.item, 32., cx))
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .child(div().text_size(px(12.)).font_weight(FontWeight::BOLD).text_color(rgb(wr_tier_color(wr))).child(format!("{wr:.1}% WR")))
                                    .child(div().text_size(px(11.)).text_color(rgb(MUTED)).child(format!("{} games", thousands(o.games)))),
                            )
                            .into_any_element()
                    })
                    .collect();
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .min_w(px(120.))
                    .child(div().text_size(px(12.)).text_color(rgb(MUTED)).font_weight(FontWeight::SEMIBOLD).child(title.to_string()))
                    .children(rows)
                    .when(opts.is_empty(), |d| d.child(div().text_size(px(11.)).text_color(rgb(MUTED)).child("–")))
                    .into_any_element()
            })
            .collect();
        let max_order: Vec<AnyElement> = b
            .max_order
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
        let grid = skill_grid(&b.skills);
        let spell_pair = match b.spells.as_slice() {
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
        let wr = b.wins as f64 * 100.0 / b.games.max(1) as f64;
        let share = b.games as f64 * 100.0 / total_games.max(1) as f64;
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
                    .child(div().text_size(px(12.)).text_color(rgb(MUTED)).child(if b.label == "Recommended" {
                        format!("{} games | WR {wr:.1}% · {}", thousands(b.games), lane.label())
                    } else {
                        format!("{} games ({share:.0}% of the recommended build's) | WR {wr:.1}%", thousands(b.games))
                    }))
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
                            .flex_wrap()
                            .gap(px(28.))
                            .child(div().flex().flex_col().gap(px(8.)).child(tag("Spells")).child(div().flex().gap(px(6.)).children(spells)))
                            .child(div().flex().flex_col().gap(px(8.)).child(tag("Starting Items")).child(div().flex().gap(px(6.)).children(starter)))
                            .child(div().flex().flex_col().gap(px(8.)).child(tag("Core Item Build Order")).child(div().flex().items_center().children(core))),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(8.))
                            .child(tag("4th / 5th / 6th Item Options"))
                            .child(div().flex().gap(px(28.)).children(options)),
                    )
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
    fn rune_trees_view(&mut self, data: &StaticData, page: &RunePage, cx: &mut Context<Self>) -> AnyElement {
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

/// The build's name: "AP", "Lethality", … (from its core items) or U.GG's style.
fn build_name(data: &StaticData, b: &UggBuild) -> String {
    if b.label != "Recommended" {
        return b.label.clone();
    }
    let items: Vec<u32> = b.core.iter().copied().filter(|i| data.items.get(i).is_some_and(|x| x.legendary)).collect();
    build_style(&items, &data.items)
}

/// U.GG lists a build's six runes in no particular order; the League client
/// (and the rune tree view) wants keystone, the primary tree's rows, then
/// the secondary tree's rows, then the shards top to bottom.
fn ugg_page(data: &StaticData, b: &UggBuild) -> Option<RunePage> {
    let row_of = |tree: u32, id: u32| -> Option<usize> {
        data.rune_trees.iter().find(|t| t.id == tree)?.slots.iter().position(|slot| slot.contains(&id))
    };
    let mut primary: Vec<(usize, u32)> = b.perks.iter().filter_map(|&id| row_of(b.primary_style, id).map(|r| (r, id))).collect();
    let mut secondary: Vec<(usize, u32)> = b.perks.iter().filter_map(|&id| row_of(b.sub_style, id).map(|r| (r, id))).collect();
    primary.sort();
    secondary.sort();
    if primary.len() != 4 || secondary.len() != 2 || b.shards.len() != 3 {
        return None;
    }
    let shards = order_shards([b.shards[0], b.shards[1], b.shards[2]]);
    Some(RunePage {
        primary_style: b.primary_style,
        sub_style: b.sub_style,
        perks: [primary[0].1, primary[1].1, primary[2].1, primary[3].1, secondary[0].1, secondary[1].1, shards[0], shards[1], shards[2]],
    })
}
