//! The window's side of the Browser tab: what is shown (the latest frame as `Pixels`), when the thread is asked to watch, and the
//! four things the person can do (pick a page, follow the newest, look again, open the page in their own browser).

use std::cell::Cell;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use rune::{Pixels, Tx};

use super::cdp::{Client, Cmd, Config};
use super::{BrowserView, CLOSED_NOTE_MS, Follow, PageInfo, Status, screencast_side};
use crate::app::App;
use crate::panel::Tab;

/// What a build of the tab reads, copied out of the lock.
#[derive(Clone, Debug, Default)]
pub struct Seen {
    pub status: Status,
    pub pages: Vec<PageInfo>,
    pub follow: Follow,
    pub shown: Option<PageInfo>,
    pub closed_at_ms: Option<i64>,
}

#[derive(Default)]
pub struct BrowserTab {
    /// Shared with the thread.
    pub view: Arc<Mutex<BrowserView>>,
    /// The frame on screen; its size once there is one.
    pub pixels: Pixels,
    pub size: Option<(u32, u32)>,
    seen_seq: u64,
    client: Option<Client>,
    /// Set in a real window: the call that has the window build its view (coalesced). Without it (tests, snapshots) no thread starts.
    wake: Option<Arc<dyn Fn() + Send + Sync>>,
    /// What the thread was last told.
    sent: Option<(bool, (u32, u32))>,
    /// The picker's menu is open.
    pub menu_open: bool,
    /// The window's scale (set by the view each build).
    pub scale: Cell<f32>,
}

impl BrowserTab {
    /// A real window: `wake` has it build its view again. Until this is called no connection is ever made.
    pub fn use_wake(&mut self, wake: Arc<dyn Fn() + Send + Sync>) {
        self.wake = Some(wake);
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, BrowserView> {
        self.view.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn snapshot(&self) -> Seen {
        let v = self.lock();
        Seen { status: v.status.clone(), pages: v.pages.clone(), follow: v.follow.clone(), shown: v.shown_page().cloned(), closed_at_ms: v.closed_at_ms }
    }

    /// Moves a newer frame (or the end of the frame) from the thread's view into the pixels on screen. True when something changed.
    pub fn pull(&mut self) -> bool {
        let (seq, frame) = {
            let mut v = self.lock();
            if v.frame_seq == self.seen_seq {
                return false;
            }
            (v.frame_seq, v.frame.take())
        };
        self.seen_seq = seq;
        match frame {
            Some(f) => {
                self.size = Some((f.w, f.h));
                self.pixels.set(f.w, f.h, f.rgba);
            }
            None => self.drop_frame(),
        }
        true
    }

    /// Lets go of the picture on screen.
    pub fn drop_frame(&mut self) {
        self.size = None;
        self.pixels = Pixels::new();
    }

    /// The window shows (or stops showing) the tab, wanting a screencast of at most `max` pixels.
    pub fn want(&mut self, on: bool, max: (u32, u32)) {
        if self.sent == Some((on, max)) {
            return;
        }
        if self.client.is_none() {
            let Some(wake) = self.wake.clone().filter(|_| on) else { return };
            self.client = Some(Client::start(Config::standard(wake), self.view.clone()));
        }
        if let Some(client) = &self.client {
            client.send(Cmd::Watch { on, max });
        }
        self.sent = Some((on, max));
    }

    pub fn look_again(&mut self) {
        self.lock().status = Status::Looking;
        if let Some(client) = &self.client {
            client.send(Cmd::Look);
        }
    }

    pub fn set_follow(&mut self, follow: Follow) {
        self.lock().follow = follow;
        self.menu_open = false;
    }

    /// A tab for scenes and tests: made-up pages and a frame, no thread.
    pub fn staged(status: Status, pages: Vec<PageInfo>, shown: Option<&str>, frame: Option<(u32, u32, Vec<u8>)>) -> BrowserTab {
        let mut tab = BrowserTab::default();
        {
            let mut v = tab.lock();
            v.status = status;
            v.pages = pages;
            v.shown = shown.map(str::to_owned);
        }
        if let Some((w, h, rgba)) = frame {
            tab.size = Some((w, h));
            tab.pixels.set(w, h, rgba);
        }
        tab
    }
}

impl App {
    /// The tab is the panel's visible tab and the window is active: the only time anything is connected.
    pub fn browser_on(&self) -> bool {
        self.focused && self.panel_visible() && self.panel_tab() == Some(Tab::Browser)
    }

    /// Once a frame: tells the thread whether to watch (and at what size), shows a newer frame, and asks to be woken when the
    /// "That page closed." line is due to go. True when the window has something new to draw.
    pub fn browser_tick(&mut self, _t: f64, tx: &mut Tx) -> bool {
        let on = self.browser_on();
        let scale = self.browser.scale.get().max(1.0);
        let (w, h) = if self.panel.expanded { self.window_size.get().map_or((880.0, 700.0), |s| (s[0] as f32, s[1] as f32)) } else { (self.layout.context.shown_width(), self.window_size.get().map_or(700.0, |s| s[1] as f32)) };
        self.browser.want(on, (screencast_side(w, scale), screencast_side((h - 120.0).max(64.0), scale)));
        let mut changed = self.browser.pull();
        if !on && self.browser.size.is_some() {
            self.browser.drop_frame();
            changed = true;
        }
        if let Some(at) = self.browser.snapshot().closed_at_ms {
            let left = at + CLOSED_NOTE_MS - (self.clock)();
            if left > 0 {
                tx.wake_after(Duration::from_millis(left as u64 + 20));
            } else if left > -200 {
                changed = true;
            }
        }
        changed
    }

    /// The picker: show this page, or follow the newest.
    pub fn browser_follow(&mut self, follow: Follow) {
        self.browser.set_follow(follow);
    }

    /// "Open in your browser": the shown page's address, http or https only, in the person's own browser.
    pub fn browser_open_current(&mut self) {
        let Some(url) = self.browser.snapshot().shown.and_then(|p| p.openable()) else { return };
        self.open_link(&url);
    }
}
