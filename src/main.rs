#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! Poro: League of Legends champion matchups (deeplol.gg), counters
//! (lolalytics.com) and one-click rune pages for the League client.

mod data;
mod images;
mod lcu;
mod live;
mod settings;
mod text_input;
mod ui;
mod update;
mod win;

use gpui::{px, size, App, AppContext, Application, Bounds, TitlebarOptions, WindowBounds, WindowOptions};

fn main() {
    if let Ok(exe) = std::env::current_exe() {
        // Just updated: give the old copy a moment to quit, then tidy up after it.
        if std::env::args().any(|a| a == "--updated") {
            std::thread::sleep(std::time::Duration::from_millis(300));
        }
        update::cleanup(&exe);
    }
    Application::new().run(|cx: &mut App| {
        text_input::bind_keys(cx);
        let bounds = Bounds::centered(None, size(px(1280.), px(900.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions { title: Some("Poro".into()), ..Default::default() }),
                window_min_size: Some(size(px(1100.), px(640.))),
                ..Default::default()
            },
            |window, cx| cx.new(|cx| ui::AppView::new(window, cx)),
        )
        .expect("open window");
        cx.on_window_closed(|cx| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
        cx.activate(true);
    });
}
