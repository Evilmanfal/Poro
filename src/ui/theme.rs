//! Colors and small shared widgets.

pub use gpui::{rgb, rgba};
use gpui::{div, prelude::*, px, AnyElement, Div};

pub const BG: u32 = 0x1c1c22;
pub const TOP_BAR: u32 = 0x18181d;
pub const PANEL: u32 = 0x25252d;
pub const PANEL_2: u32 = 0x2f2f39;
pub const BORDER: u32 = 0x3a3a45;
pub const TEXT: u32 = 0xe9eaee;
pub const MUTED: u32 = 0x8b8d98;
pub const ACCENT: u32 = 0x37c8ab;
pub const RED: u32 = 0xe8605e;
pub const BLUE: u32 = 0x5b8cf5;
pub const GOLD: u32 = 0xf2c14e;

/// A rounded panel.
pub fn card() -> Div {
    div().bg(rgb(PANEL)).rounded(px(10.)).border_1().border_color(rgb(BORDER)).p(px(16.))
}

pub fn section_title(text: impl Into<gpui::SharedString>) -> Div {
    div().text_size(px(13.)).font_weight(gpui::FontWeight::SEMIBOLD).text_color(rgb(MUTED)).child(text.into())
}

/// A small "label / value" stat.
pub fn stat(label: &str, value: String) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap(px(4.))
        .child(div().text_size(px(12.)).text_color(rgb(MUTED)).child(label.to_string()))
        .child(div().text_size(px(16.)).font_weight(gpui::FontWeight::SEMIBOLD).child(value))
        .into_any_element()
}

pub fn pct(v: f64) -> String {
    format!("{:.1}%", v)
}

/// deeplol sends rates as fractions (0.485).
pub fn pct_frac(v: Option<f64>) -> String {
    v.map(|v| pct(v * 100.0)).unwrap_or_else(|| "–".into())
}

pub fn thousands(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// Green above 50%, red below.
pub fn wr_color(wr: f64) -> u32 {
    if wr >= 52.0 {
        ACCENT
    } else if wr <= 48.0 {
        RED
    } else {
        TEXT
    }
}

pub fn delta_color(d: f64) -> u32 {
    if d > 0.5 {
        ACCENT
    } else if d < -0.5 {
        RED
    } else {
        MUTED
    }
}

pub fn button(id: impl Into<gpui::ElementId>, label: impl Into<gpui::SharedString>) -> gpui::Stateful<Div> {
    div()
        .id(id)
        .px(px(14.))
        .py(px(7.))
        .rounded(px(6.))
        .border_1()
        .border_color(rgb(BORDER))
        .text_size(px(13.))
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .cursor_pointer()
        .hover(|s| s.bg(rgb(PANEL_2)))
        .child(label.into())
}

pub fn primary_button(id: impl Into<gpui::ElementId>, label: impl Into<gpui::SharedString>) -> gpui::Stateful<Div> {
    div()
        .id(id)
        .px(px(12.))
        .py(px(6.))
        .rounded(px(6.))
        .bg(rgba(0x37c8ab26))
        .border_1()
        .border_color(rgb(ACCENT))
        .text_color(rgb(ACCENT))
        .text_size(px(12.))
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .cursor_pointer()
        .hover(|s| s.bg(rgba(0x37c8ab44)))
        .child(label.into())
}

pub fn time_ago(unix: i64) -> String {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    let s = (now - unix).max(0);
    match s {
        0..=3599 => format!("{} min ago", (s / 60).max(1)),
        3600..=86399 => format!("{} hours ago", s / 3600),
        _ => match s / 86400 {
            1 => "1 day ago".into(),
            d => format!("{d} days ago"),
        },
    }
}

pub fn duration(secs: u64) -> String {
    format!("{}:{:02}", secs / 60, secs % 60)
}

pub fn skill_letter(id: u32) -> &'static str {
    match id {
        1 => "Q",
        2 => "W",
        3 => "E",
        4 => "R",
        _ => "?",
    }
}
