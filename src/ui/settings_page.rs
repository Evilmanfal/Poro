//! The Settings tab: in-game overlay toggles (saved between runs) and a
//! preview of the overlays with sample data.

use gpui::{div, prelude::*, px, AnyElement, Context, FontWeight, SharedString};

use super::theme::*;
use super::AppView;
use crate::settings::Overlays;

type Getter = fn(&mut Overlays) -> &mut bool;

const OVERLAYS: [(&str, &str, Getter); 5] = [
    (
        "Quick Stats",
        "Display settings for live stats overlay. Your CS, CS per minute, KDA, kill participation, vision score and gold, updated live through the game.",
        |o| &mut o.quick_stats,
    ),
    (
        "Skill Order",
        "Display settings for skill order overlay. Shows the next abilities to level from the recommended build for your matchup, highlighting the next one.",
        |o| &mut o.skill_order,
    ),
    (
        "Item Build",
        "Display settings for item build overlay. Starting and core items from the recommended build for your matchup; items you own are ticked off.",
        |o| &mut o.item_build,
    ),
    (
        "Jungle Timers",
        "Display settings for Jungle Timers overlay. Dragon (and Elder) and Baron Nashor respawn timers from the game's kill announcements.",
        |o| &mut o.jungle_timers,
    ),
    (
        "Lane Gold Difference",
        "Displays \u{201c}realized\u{201d} gold difference (GD) between each lane (same row on scoreboard), plus teamwide GD below it. \u{201c}Realized\u{201d} here means GD in champion inventory.",
        |o| &mut o.lane_gold,
    ),
];

impl AppView {
    pub(super) fn render_settings(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let on = self.settings.overlays;
        let rows: Vec<AnyElement> = OVERLAYS
            .iter()
            .enumerate()
            .map(|(i, (title, desc, get))| {
                let get = *get;
                let mut copy = on;
                let enabled = *get(&mut copy);
                div()
                    .id(SharedString::from(format!("overlay-{i}")))
                    .flex()
                    .items_center()
                    .gap(px(16.))
                    .p(px(14.))
                    .rounded(px(8.))
                    .bg(rgb(PANEL_2))
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        let v = get(&mut this.settings.overlays);
                        *v = !*v;
                        crate::settings::save(&this.settings);
                        cx.notify();
                    }))
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .gap(px(4.))
                            .child(div().text_size(px(15.)).font_weight(FontWeight::SEMIBOLD).child(title.to_string()))
                            .child(div().text_size(px(13.)).text_color(rgb(0xb4b6c0)).child(desc.to_string())),
                    )
                    .child(switch(enabled))
                    .into_any_element()
            })
            .collect();
        let previewing = self.ov.previewing();
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
                    .child(div().text_size(px(22.)).font_weight(FontWeight::BOLD).child("Settings"))
                    .child(div().text_size(px(13.)).text_color(rgb(MUTED)).child(
                        "In-game overlays show on top of League while a game is running and the game is in front. \
                         Play in Borderless or Windowed mode (League Settings › Video › Window Mode): overlays can't \
                         draw over Fullscreen. Overlays let your clicks through to the game.",
                    )),
            )
            .child(
                card()
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .child(section_title("In-game overlays"))
                            .child(div().flex_1())
                            .child(
                                primary_button("overlay-preview", if previewing { "Previewing…" } else { "Preview overlays" })
                                    .on_click(cx.listener(|this, _, _, cx| this.preview_overlays(cx))),
                            ),
                    )
                    .child(div().text_size(px(12.)).text_color(rgb(MUTED)).child(
                        "Preview shows every overlay over your screen for 20 seconds with sample data, so you can see where they sit.",
                    ))
                    .children(rows),
            )
            .into_any_element()
    }
}

fn switch(on: bool) -> AnyElement {
    div()
        .flex_none()
        .w(px(42.))
        .h(px(24.))
        .p(px(3.))
        .rounded_full()
        .bg(rgb(if on { ACCENT } else { BORDER }))
        .flex()
        .when(on, |d| d.justify_end())
        .child(div().size(px(18.)).rounded_full().bg(rgb(if on { 0xffffff } else { 0x8b8d98 })))
        .into_any_element()
}
