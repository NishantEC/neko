// Adapted from `gpui-component` 0.5.1 (https://github.com/longbridge/gpui-component),
// Copyright the gpui-component contributors, licensed Apache-2.0.
// A full copy of that license is at `THIRD_PARTY_LICENSES/gpui-component-APACHE-2.0.txt`.
// See `crates/neko/src/components/vendor/MANIFEST.md` for what was vendored and why.
//
// Changes from upstream `src/input/blink_cursor.rs` (Apache-2.0 §4(b) notice):
// the type is renamed `BlinkCursor` -> `CursorBlink` to match this crate's naming, doc
// comments are trimmed, and the pause-delay/interval constants are pulled to the top as
// `pub` so `text_field.rs` can reference them in tests. The blink/pause/epoch state machine
// itself — the actual hard mechanics this file was vendored for — is unchanged.

use std::time::Duration;

use gpui::{Context, Task, Timer};

pub const BLINK_INTERVAL: Duration = Duration::from_millis(500);
pub const PAUSE_DELAY: Duration = Duration::from_millis(300);

/// Manages a text cursor's blink timing: blinks on a fixed interval, and
/// pauses (cursor solid) for a short delay after every edit or move so the
/// cursor doesn't visually vanish mid-keystroke.
pub(crate) struct CursorBlink {
    visible: bool,
    paused: bool,
    epoch: usize,
    _task: Task<()>,
}

impl CursorBlink {
    pub fn new() -> Self {
        Self {
            // Starts `false` deliberately: `start()`'s first `blink()` call
            // flips this to `true` (cursor visible) before anything has
            // rendered, matching a cursor that appears solid the instant a
            // field is focused rather than starting mid-blink.
            visible: false,
            paused: false,
            epoch: 0,
            _task: Task::ready(()),
        }
    }

    pub fn start(&mut self, cx: &mut Context<Self>) {
        self.blink(self.epoch, cx);
    }

    fn next_epoch(&mut self) -> usize {
        self.epoch += 1;
        self.epoch
    }

    fn blink(&mut self, epoch: usize, cx: &mut Context<Self>) {
        if self.paused || epoch != self.epoch {
            self.visible = true;
            return;
        }

        self.visible = !self.visible;
        cx.notify();

        let epoch = self.next_epoch();
        self._task = cx.spawn(async move |this, cx| {
            Timer::after(BLINK_INTERVAL).await;
            if let Some(this) = this.upgrade() {
                this.update(cx, |this, cx| this.blink(epoch, cx)).ok();
            }
        });
    }

    pub fn visible(&self) -> bool {
        self.paused || self.visible
    }

    /// Keep the cursor solid for `PAUSE_DELAY`, then resume blinking. Call
    /// this on every keystroke so the cursor never disappears while typing.
    pub fn pause(&mut self, cx: &mut Context<Self>) {
        self.paused = true;
        self.visible = true;
        cx.notify();

        let epoch = self.next_epoch();
        self._task = cx.spawn(async move |this, cx| {
            Timer::after(PAUSE_DELAY).await;
            if let Some(this) = this.upgrade() {
                this.update(cx, |this, cx| {
                    this.paused = false;
                    this.blink(epoch, cx);
                })
                .ok();
            }
        });
    }
}
