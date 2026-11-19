//! Hover descriptions for every icon: champions, abilities, items, runes,
//! stat shards and summoner spells (text from Riot's Data Dragon, the same
//! data deeplol shows).

use std::cell::Cell;

use gpui::{div, prelude::*, px, AnyElement, App, Context, ElementId, FontWeight, Window};

use super::theme::*;
use crate::data::ddragon::{Skill, StaticData, Tip};
use crate::images;

thread_local! {
    static NEXT_ID: Cell<u64> = const { Cell::new(0) };
}

/// Called at the start of each frame so tooltip ids stay the same between
/// frames (the hover state is kept by id).
pub fn reset_ids() {
    NEXT_ID.with(|n| n.set(0));
}

fn next_id() -> ElementId {
    let n = NEXT_ID.with(|c| {
        let v = c.get();
        c.set(v + 1);
        v
    });
    ElementId::Name(format!("tip-{n}").into())
}

/// Wraps `el` so hovering it shows `tip`.
pub fn tipped(el: AnyElement, tip: Option<Tip>) -> AnyElement {
    let Some(tip) = tip else { return el };
    div()
        .id(next_id())
        .flex_none()
        .child(el)
        .tooltip(move |_, cx| cx.new(|_| TipView(tip.clone())).into())
        .into_any_element()
}

pub fn item(data: &StaticData, id: u32, size: f32, cx: &mut App) -> AnyElement {
    tipped(images::icon(Some(&data.item_icon(id)), size, cx), data.item_tip(id))
}

pub fn spell(data: &StaticData, key: u32, size: f32, cx: &mut App) -> AnyElement {
    tipped(images::icon(data.spell_icon(key).as_deref(), size, cx), data.spell_tip(key))
}

pub fn rune(data: &StaticData, id: u32, size: f32, cx: &mut App) -> AnyElement {
    tipped(images::round(data.rune_icon(id).as_deref(), size, cx), data.rune_tip(id))
}

/// A rune that wasn't picked: greyed out, still with its description.
pub fn rune_dim(data: &StaticData, id: u32, size: f32, cx: &mut App) -> AnyElement {
    tipped(images::dim(data.rune_icon(id).as_deref(), size, true, cx), data.rune_tip(id))
}

pub fn champ(data: &StaticData, key: u32, size: f32, round: bool, cx: &mut App) -> AnyElement {
    let url = data.champion_icon(key);
    let icon = if round { images::round(url.as_deref(), size, cx) } else { images::icon(url.as_deref(), size, cx) };
    tipped(icon, data.champion_tip(key))
}

pub fn skill(s: &Skill, size: f32, cx: &mut App) -> AnyElement {
    tipped(images::icon(Some(&s.icon), size, cx), Some(s.tip()))
}

struct TipView(Tip);

impl Render for TipView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let tip = &self.0;
        // A fixed width so wrapped lines grow the box (with only a max width
        // the box is sized for unwrapped text).
        let long = tip.body.len() > 60 || tip.title.len() > 30;
        div()
            .w(px(if long { 340. } else { 220. }))
            .px(px(12.))
            .py(px(10.))
            .rounded(px(8.))
            .bg(rgb(0x121216))
            .border_1()
            .border_color(rgb(BORDER))
            .shadow_lg()
            .flex()
            .flex_col()
            .gap(px(4.))
            .child(div().text_size(px(14.)).font_weight(FontWeight::BOLD).text_color(rgb(GOLD)).child(tip.title.clone()))
            .children(tip.subtitle.clone().filter(|s| !s.is_empty()).map(|s| div().text_size(px(12.)).text_color(rgb(MUTED)).child(s)))
            .children(
                tip.body
                    .lines()
                    .filter(|l| !l.trim().is_empty())
                    .map(|l| div().text_size(px(12.)).text_color(rgb(0xd4d6dd)).child(l.to_string())),
            )
    }
}
