# Neo's patch to GPUI 0.2.2

This is `gpui` 0.2.2 from crates.io (Apache-2.0, Zed Industries), vendored
with one behavioral change. The examples and tests are left out.

**Problem:** on Windows, GPUI's `VSyncProvider` thread invalidates every
window on every display refresh, 60–144 times a second, even when nothing
changed. An idle window used about 1.4% of a CPU core just waking up to find
nothing to draw.

**Change:**
- `src/window.rs` adds `FRAME_WAKER` / `wake_frames()`. It is called when a
  view is invalidated, the window is marked dirty, a next-frame callback is
  queued, or input arrives (1.1 s, matching GPUI's "keep presenting for 1 s
  after input").
- `src/platform/windows/platform.rs`: the vsync thread only ticks while it
  has been woken recently, plus a 250 ms grace period. Otherwise it parks,
  waking once a second as a safety net.

Every change is marked `NEO PATCH`. To upgrade GPUI, re-apply those hunks or
drop the `[patch.crates-io]` entry in the workspace `Cargo.toml`.

## Poro patch

`src/elements/div.rs` (marked `POROPATCH`): moving the mouse off an element
whose tooltip is showing now requests a frame. Tooltips are hidden during
prepaint, and with the idle vsync change above an idle window doesn't
repaint on a plain mouse move, so tooltips stayed up until something else
redrew the window.
