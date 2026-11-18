//! Downloads icons (champions, runes, items, spells, skills) once, keeps
//! them on disk and in memory, and draws a placeholder until they arrive.

use std::collections::HashMap;
use std::sync::Arc;

use gpui::{div, img, prelude::*, px, rgb, AnyElement, App, Global, Image, ImageFormat, Pixels};

use crate::data::{get_bytes, STATIC};

#[derive(Default)]
pub struct Images {
    /// None = still loading or failed.
    loaded: HashMap<String, Option<Arc<Image>>>,
}

impl Global for Images {}

fn format_of(bytes: &[u8]) -> ImageFormat {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        ImageFormat::Png
    } else if bytes.starts_with(b"RIFF") {
        ImageFormat::Webp
    } else if bytes.starts_with(&[0xFF, 0xD8]) {
        ImageFormat::Jpeg
    } else {
        ImageFormat::Png
    }
}

fn get(url: &str, cx: &mut App) -> Option<Arc<Image>> {
    if !cx.has_global::<Images>() {
        cx.set_global(Images::default());
    }
    if let Some(state) = cx.global::<Images>().loaded.get(url) {
        return state.clone();
    }
    cx.global_mut::<Images>().loaded.insert(url.to_string(), None);
    let url = url.to_string();
    cx.spawn(async move |cx| {
        let fetch_url = url.clone();
        let bytes = cx.background_executor().spawn(async move { get_bytes(&fetch_url, STATIC) }).await;
        let _ = cx.update(|cx| {
            if let Ok(bytes) = bytes {
                let image = Arc::new(Image::from_bytes(format_of(&bytes), bytes));
                cx.global_mut::<Images>().loaded.insert(url, Some(image));
                cx.refresh_windows();
            }
        });
    })
    .detach();
    None
}

/// A square icon; a dark tile while it loads.
pub fn icon(url: Option<&str>, size: f32, cx: &mut App) -> AnyElement {
    icon_rounded(url, size, 4.0, cx)
}

pub fn icon_rounded(url: Option<&str>, size: f32, radius: f32, cx: &mut App) -> AnyElement {
    let size: Pixels = px(size);
    match url.and_then(|u| get(u, cx)) {
        Some(image) => img(image).size(size).flex_none().rounded(px(radius)).into_any_element(),
        None => div().size(size).flex_none().rounded(px(radius)).bg(rgb(0x2c2c34)).into_any_element(),
    }
}

/// A round icon (champion portraits).
pub fn round(url: Option<&str>, size: f32, cx: &mut App) -> AnyElement {
    icon_rounded(url, size, size / 2.0, cx)
}
