//! rune-ui registry: dropdown_menu 0.10.0 Copied into your app by `rune-ui add dropdown_menu`; yours to change. Needs: popup.
//!
//! A menu under its trigger: rune-ui's popup with role `menu`. It shares the trigger's left edge with a 4 point gap, flips above near
//! the window's bottom and stays in the window; rows highlight under the pointer and the keys; groups have quiet labels and separators;
//! shortcuts sit right; checkable rows show a check; destructive rows are tinted; disabled rows are faint and skipped. Keys: Up/Down
//! (wrapping), Home/End, a typed letter, Return or Space, Escape or Tab to close with focus back on the trigger.
//!
//! ```ignore
//! DropdownMenu::new("account").trigger(button).open(s.open == Some("account")).on_close(|s: &mut App| s.open = None)
//!     .label("Account").item(DropdownMenuItem::new("Settings").shortcut("⌘,")).separator().danger_item(DropdownMenuItem::new("Sign out"))
//!     .build(t, &mut layer)
//! ```

use rune::{El, Side, row};

use super::base::{Press, Theme, press};
use super::popup::{Entry, Popup, Row, Mount, mount};

/// A menu row (see `popup::Row`): `new(label)`, `.icon`, `.shortcut`, `.checked`, `.disabled`, `.danger`, `.on_select`.
pub type DropdownMenuItem<S> = Row<S>;

pub struct DropdownMenu<S: 'static> {
    key: String,
    trigger: Option<El>,
    entries: Vec<Entry<S>>,
    open: bool,
    min_w: f32,
    close: Option<Press<S>>,
}

impl<S: 'static> DropdownMenu<S> {
    /// Rows are keyed `<key>-item<N>` unless they bring their own key.
    pub fn new(key: impl Into<String>) -> DropdownMenu<S> {
        DropdownMenu { key: key.into(), trigger: None, entries: Vec::new(), open: false, min_w: 200.0, close: None }
    }

    pub fn trigger(mut self, t: El) -> Self {
        self.trigger = Some(t);
        self
    }

    pub fn item(mut self, i: DropdownMenuItem<S>) -> Self {
        self.entries.push(Entry::Row(i));
        self
    }

    /// A destructive row, set apart by a separator.
    pub fn danger_item(mut self, i: DropdownMenuItem<S>) -> Self {
        self.entries.push(Entry::Separator);
        self.entries.push(Entry::Row(i.danger(true)));
        self
    }

    pub fn separator(mut self) -> Self {
        self.entries.push(Entry::Separator);
        self
    }

    /// A quiet group label.
    pub fn label(mut self, words: impl Into<String>) -> Self {
        self.entries.push(Entry::Label(words.into()));
        self
    }

    pub fn open(mut self, open: bool) -> Self {
        self.open = open;
        self
    }

    /// The least width (it is never narrower than this, whatever the trigger).
    pub fn min_width(mut self, w: f32) -> Self {
        self.min_w = w;
        self
    }

    /// Escape, Tab, a press outside, or any row chosen.
    pub fn on_close(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.close = Some(press(f));
        self
    }

    /// The trigger in place; the open menu goes into `layer` (mounted at the view's root: `base::overlay_root`).
    pub fn build(self, t: &Theme, layer: &mut Vec<El>) -> El {
        let anchor = format!("{}-trigger", self.key);
        let trigger = row().key(anchor.clone()).expanded(self.open).children(self.trigger);
        if self.open {
            let close = self.close.unwrap_or_else(|| press(|_: &mut S| {}));
            let mut popup = Popup::<S>::new(self.key.clone(), "menu", "Menu");
            popup.min_w = self.min_w;
            popup.entries = self.entries;
            let surface = popup.surface(t, close.clone());
            mount(t, layer, surface, Mount { key: &self.key, anchor: &anchor, side: Side::Bottom, return_to: &anchor }, close);
        }
        trigger
    }
}
