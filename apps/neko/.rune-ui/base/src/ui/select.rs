//! rune-ui registry: select 0.6.0. Copied into your app by `rune-ui add select`; yours to change. Needs: popup.
//!
//! One choice of a few. The trigger is a **well** (it holds a value) with the choice and a chevron that turns while open; the list is
//! rune-ui's popup (the **floating** material) under the trigger, sharing its left edge and at least its width, flipping above near the
//! window's bottom, the current choice checked and focused on open. Keys: Up/Down (wrapping), Home/End, a typed letter, Return or Space
//! to choose, Escape or Tab to close, focus back on the trigger. The app owns whether it is open.
//!
//! ```ignore
//! Select::new("level", "Starting level", &s.levels).current(&s.level).open(s.open == Some("level"))
//!     .on_toggle(|s: &mut App| s.toggle("level")).on_close(|s: &mut App| s.open = None)
//!     .on_choose(|s: &mut App, v: &str| s.level = v.to_owned()).build(t, &mut layer)
//! ```

use rune::{El, Side, row, text};

use super::base::{Pick, Press, Shape, Size, SizeExt, State, Theme, glyph, paint, press, recipes, resolve, states};
use super::icons;
use super::popup::{Entry, Popup, Row, Mount, mount};

pub struct Select<'a, S: 'static> {
    key: String,
    name: String,
    options: &'a [String],
    current: Option<String>,
    placeholder: String,
    width: f32,
    size: Size,
    disabled: bool,
    invalid: bool,
    pinned: Option<State>,
    open: bool,
    toggle: Option<Press<S>>,
    close: Option<Press<S>>,
    choose: Option<Pick<S>>,
}

impl<'a, S: 'static> Select<'a, S> {
    /// `name` names it for assistive technology.
    pub fn new(key: impl Into<String>, name: impl Into<String>, options: &'a [String]) -> Select<'a, S> {
        Select { key: key.into(), name: name.into(), options, current: None, placeholder: "Choose…".to_owned(), width: 200.0, size: Size::Md, disabled: false, invalid: false, pinned: None, open: false, toggle: None, close: None, choose: None }
    }

    pub fn current(mut self, c: &str) -> Self {
        self.current = Some(c.to_owned());
        self
    }

    pub fn placeholder(mut self, p: impl Into<String>) -> Self {
        self.placeholder = p.into();
        self
    }

    pub fn width(mut self, w: f32) -> Self {
        self.width = w;
        self
    }

    pub fn size(mut self, s: Size) -> Self {
        self.size = s;
        self
    }

    pub fn disabled(mut self, on: bool) -> Self {
        self.disabled = on;
        self
    }

    pub fn invalid(mut self, on: bool) -> Self {
        self.invalid = on;
        self
    }

    pub fn state(mut self, s: State) -> Self {
        self.pinned = Some(s);
        self
    }

    /// Whether the list is showing (the app keeps this).
    pub fn open(mut self, open: bool) -> Self {
        self.open = open;
        self
    }

    pub fn on_toggle(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.toggle = Some(press(f));
        self
    }

    /// Escape, Tab, a press outside, or a choice.
    pub fn on_close(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.close = Some(press(f));
        self
    }

    pub fn on_choose(mut self, f: impl Fn(&mut S, &str) + 'static) -> Self {
        self.choose = Some(std::rc::Rc::new(f));
        self
    }

    /// The trigger in place; the open list goes into `layer` (mounted at the view's root: `base::overlay_root`).
    pub fn build(self, t: &Theme, layer: &mut Vec<El>) -> El {
        let anchor = format!("{}-trigger", self.key);
        let open = self.open && !self.disabled;
        let parts = recipes::field(t);
        let mut st = states(self.pinned, self.disabled, false);
        st.invalid = self.invalid;
        st.open = open;
        let (words, ink) = match &self.current {
            Some(c) => (c.clone(), parts.label.resolve(st)),
            None => (self.placeholder.clone(), t.ink_faint()),
        };
        let h = self.size.height(t);
        let mut trigger = row().key(anchor.clone()).role("combobox").label(self.name.clone()).expanded(open).focusable(!self.disabled).w(self.width).h(h).pl(self.size.pad(t) - 2.0).pr(t.metrics.gap).gap(t.metrics.gap).center_y().radius(self.size.radius(t)).round_as(t);
        if let Some(c) = &self.current {
            trigger = trigger.a11y_value(c.clone());
        }
        trigger = paint(trigger, &resolve(&parts.root, st));
        if !self.disabled
            && let Some((w, o, c)) = parts.root.live().focus_ring
        {
            trigger = trigger.focus_ring(w, o, c);
        }
        let chevron = glyph(&icons::CHEVRON_DOWN, self.size.icon(t), t.ink_muted()).rotate(if open { std::f32::consts::PI } else { 0.0 });
        trigger = trigger.child(text(words).font(self.size.text(t, t.ty.regular)).color(ink).grow(1.0).truncate().min_w(0.0).a11y_hidden()).child(chevron);
        if self.disabled {
            trigger = trigger.disabled(true);
        } else if let Some(f) = self.toggle.clone() {
            // Opening puts the keyboard on the current choice, showing it, so the first Down moves on from it (as Radix does).
            let at = self.current.as_deref().and_then(|c| self.options.iter().position(|o| o == c)).unwrap_or(0);
            let row_key = format!("{}-item{}", self.key, at + 1);
            trigger = trigger.on_press_tx(move |s: &mut S, tx: &mut rune::Tx| {
                f(s);
                tx.focus_when_shown(row_key.clone(), true);
            });
        }
        if open {
            let close = self.close.clone().unwrap_or_else(|| press(|_: &mut S| {}));
            let mut popup = Popup::<S>::new(self.key.clone(), "listbox", self.name.clone());
            popup.min_w = self.width;
            for o in self.options {
                let mut r = Row::new(o.clone()).checked(self.current.as_deref() == Some(o.as_str()));
                if let Some(f) = self.choose.clone() {
                    let v = o.clone();
                    r = r.on_select(move |s: &mut S| f(s, &v));
                }
                popup.entries.push(Entry::Row(r));
            }
            let surface = popup.surface(t, close.clone());
            mount(t, layer, surface, Mount { key: &self.key, anchor: &anchor, side: Side::Bottom, return_to: &anchor }, close);
        }
        row().key(self.key).child(trigger)
    }
}
