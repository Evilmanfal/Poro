//! The update prompt: "Poro vX is available" with Update now / Later, a small
//! "Update" button in the header after Later, download progress, then swap
//! the exe and restart (see `crate::update`).

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use gpui::{div, prelude::*, px, relative, AnyElement, Context, FontWeight};

use super::theme::*;
use super::AppView;
use crate::update::{self, Release};

/// Where the update stands in the UI.
#[derive(Default)]
pub struct UpdateState {
    pub release: Option<Release>,
    /// The full prompt is showing.
    pub prompt: bool,
    /// "Later" was picked for this version (until the next start).
    pub postponed: Option<String>,
    /// (done, total) bytes while downloading.
    pub progress: Option<(u64, u64)>,
    pub error: Option<String>,
}

impl AppView {
    /// Checks GitHub for a newer release now and every 12 hours.
    pub(super) fn start_update_checks(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| loop {
            let found = cx.background_executor().spawn(async { update::check(&update::endpoint()) }).await;
            let alive = this.update(cx, |this, cx| {
                if let Ok(Some(rel)) = found {
                    // A version newer than the one put off brings the prompt back.
                    let put_off = this.up.postponed.as_deref().is_some_and(|v| !update::newer(&rel.version, v));
                    if this.up.release.as_ref() != Some(&rel) {
                        this.up.prompt = !put_off;
                        this.up.release = Some(rel);
                        cx.notify();
                    }
                }
            });
            if alive.is_err() {
                break;
            }
            cx.background_executor().timer(update::EVERY).await;
        })
        .detach();
    }

    fn start_update(&mut self, cx: &mut Context<Self>) {
        let Some(rel) = self.up.release.clone() else { return };
        if self.up.progress.is_some() {
            return;
        }
        self.up.progress = Some((0, 0));
        self.up.error = None;
        let done = Arc::new(AtomicU64::new(0));
        let total = Arc::new(AtomicU64::new(0));
        let (d, t) = (done.clone(), total.clone());
        let job = cx.background_executor().spawn(async move {
            let exe = std::env::current_exe()?;
            update::download(&rel, &exe, &update::release_key(), |a, b| {
                d.store(a, Ordering::Relaxed);
                t.store(b, Ordering::Relaxed);
            })
        });
        // Show progress while the download runs.
        cx.spawn(async move |this, cx| {
            let mut job = std::pin::pin!(job);
            loop {
                let tick = cx.background_executor().timer(Duration::from_millis(200));
                match futures_lite_select(&mut job, tick).await {
                    Some(result) => {
                        let _ = this.update(cx, |this, cx| match result {
                            Ok(path) => this.install_and_restart(path, cx),
                            Err(e) => {
                                this.up.progress = None;
                                this.up.error = Some(format!("{e:#}"));
                                cx.notify();
                            }
                        });
                        break;
                    }
                    None => {
                        let p = (done.load(Ordering::Relaxed), total.load(Ordering::Relaxed));
                        if this
                            .update(cx, |this, cx| {
                                this.up.progress = Some(p);
                                cx.notify();
                            })
                            .is_err()
                        {
                            break;
                        }
                    }
                }
            }
        })
        .detach();
        cx.notify();
    }

    fn install_and_restart(&mut self, path: std::path::PathBuf, cx: &mut Context<Self>) {
        let result = std::env::current_exe().map_err(anyhow::Error::from).and_then(|exe| {
            update::install(&path, &exe)?;
            let mut args: Vec<String> = std::env::args().skip(1).filter(|a| a != "--updated").collect();
            args.push("--updated".into());
            std::process::Command::new(&exe).args(args).spawn()?;
            Ok(())
        });
        match result {
            Ok(()) => cx.quit(),
            Err(e) => {
                self.up.progress = None;
                self.up.error = Some(format!("Couldn't install the update: {e:#}"));
                cx.notify();
            }
        }
    }

    /// The small header button once the prompt was put off.
    pub(super) fn update_pill(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let rel = self.up.release.as_ref()?;
        if self.up.prompt {
            return None;
        }
        let label = if self.up.progress.is_some() { "Updating…".to_string() } else { format!("Update to v{}", rel.version) };
        Some(
            primary_button("update-pill", label)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.up.prompt = true;
                    cx.notify();
                }))
                .into_any_element(),
        )
    }

    /// The prompt card above the page.
    pub(super) fn update_card(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let rel = self.up.release.clone()?;
        if !self.up.prompt {
            return None;
        }
        let version = rel.version.clone();
        let card = div()
            .mx(px(18.))
            .mt(px(14.))
            .p(px(14.))
            .rounded(px(10.))
            .bg(rgb(PANEL))
            .border_1()
            .border_color(rgb(ACCENT))
            .flex()
            .flex_col()
            .gap(px(8.));
        let title = |t: String| div().text_size(px(15.)).font_weight(FontWeight::SEMIBOLD).child(t);
        let body = |t: String| div().text_size(px(12.)).text_color(rgb(MUTED)).child(t);
        let card = if let Some((done, total)) = self.up.progress {
            let frac = if total == 0 { 0.0 } else { (done as f64 / total as f64).clamp(0.0, 1.0) };
            let what = if total == 0 {
                "Starting…".to_string()
            } else {
                format!("{:.1} of {:.1} MB", done as f64 / 1e6, total as f64 / 1e6)
            };
            card.child(title(format!("Downloading Poro v{version}…")))
                .child(body(format!("{what} · Poro restarts by itself when it's ready.")))
                .child(
                    div()
                        .h(px(4.))
                        .w_full()
                        .rounded_full()
                        .bg(rgb(PANEL_2))
                        .child(div().h_full().rounded_full().bg(rgb(ACCENT)).w(relative(frac as f32))),
                )
        } else {
            let mut c = card.child(title(format!("Poro v{version} is available (you have v{})", update::current())));
            if !rel.notes.is_empty() {
                c = c.child(body(rel.notes.clone()));
            }
            if let Some(e) = &self.up.error {
                c = c.child(div().text_size(px(12.)).text_color(rgb(RED)).child(format!("Update failed: {e}")));
            }
            c.child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.))
                    .child(button("update-later", "Later").on_click(cx.listener(|this, _, _, cx| {
                        this.up.prompt = false;
                        this.up.postponed = this.up.release.as_ref().map(|r| r.version.clone());
                        cx.notify();
                    })))
                    .child(
                        primary_button("update-now", if self.up.error.is_some() { "Try again" } else { "Update now" })
                            .on_click(cx.listener(|this, _, _, cx| this.start_update(cx))),
                    ),
            )
        };
        Some(card.into_any_element())
    }
}

/// Waits for whichever finishes first: the job (Some(result)) or the tick (None).
async fn futures_lite_select<T>(
    job: &mut std::pin::Pin<&mut gpui::Task<T>>,
    tick: gpui::Task<()>,
) -> Option<T> {
    use std::future::Future;
    use std::task::Poll;
    let mut tick = std::pin::pin!(tick);
    std::future::poll_fn(|cx| {
        if let Poll::Ready(v) = job.as_mut().poll(cx) {
            return Poll::Ready(Some(v));
        }
        if tick.as_mut().poll(cx).is_ready() {
            return Poll::Ready(None);
        }
        Poll::Pending
    })
    .await
}
