//! rune-ui registry: popup 0.8.0 The floating list that Select, DropdownMenu and ContextMenu open (and Combobox's rows): drawn by
//! rune-ui on Rune's core behaviour, not rune-kit's list. `rune-ui add` brings it with them.
//!
//! * **Placement**: anchored under its trigger with a 4 point gap, sharing the trigger's left edge and at least its width; it flips
//!   above when there is no room below and is never taller than the window leaves it, scrolling inside (`El::anchored_fit`).
//! * **Look**: the floating material at the large radius; rows inset 4 points from its edge at the concentric radius; the highlight is
//!   a filled row that follows both the pointer and the keys (`El::focus_highlight`); the current choice has a check and is focused
//!   (so scrolled into view) on open; disabled rows are faint and skipped; sections have labels and separators; shortcuts sit right;
//!   destructive rows are tinted.
//! * **Keys**: Up and Down move and wrap, Home and End jump (`El::focus_group` on a listbox or menu), Return and Space choose, a typed
//!   letter jumps to the next row starting with it, Escape closes (`El::modal`) and Tab closes, focus going back to the trigger
//!   (`El::focus_return`). A press outside closes it (`rune_kit::scrim`).
//! * **Motion**: in from 0.97 and transparent at the trigger's side in about 140 ms, out in about 90 ms; in place, with no motion,
//!   when the theme says Reduce Motion (`Theme::reduced_motion`).

use std::rc::Rc;

use rune::{Align, El, Icon, Key, KeyEvent, Side, Spring, SpringPair, Transition, Tx, Vec2, col, el, row, text};
use rune_ui_style::{Radius, tabular, weight};

use super::base::{Press, Shape, Theme, glyph, paint, press};
use super::icons;

/// One row.
pub struct Row<S: 'static> {
    pub key: Option<String>,
    pub label: String,
    pub icon: Option<&'static Icon>,
    pub shortcut: Option<String>,
    pub description: Option<String>,
    pub checked: Option<bool>,
    pub disabled: bool,
    pub danger: bool,
    pub press: Option<Press<S>>,
}

impl<S: 'static> Row<S> {
    pub fn new(label: impl Into<String>) -> Row<S> {
        Row { key: None, label: label.into(), icon: None, shortcut: None, description: None, checked: None, disabled: false, danger: false, press: None }
    }

    /// Its own key (otherwise `<list>-item<N>`).
    pub fn key(mut self, k: impl Into<String>) -> Self {
        self.key = Some(k.into());
        self
    }

    pub fn icon(mut self, i: &'static Icon) -> Self {
        self.icon = Some(i);
        self
    }

    /// The keys that do the same, at the row's right.
    pub fn shortcut(mut self, k: impl Into<String>) -> Self {
        self.shortcut = Some(k.into());
        self
    }

    /// A quiet second line.
    pub fn description(mut self, d: impl Into<String>) -> Self {
        self.description = Some(d.into());
        self
    }

    /// A check at the right while on (a menu's checkbox item, a list's current choice).
    pub fn checked(mut self, on: bool) -> Self {
        self.checked = Some(on);
        self
    }

    pub fn disabled(mut self, on: bool) -> Self {
        self.disabled = on;
        self
    }

    /// Tinted: for what cannot be taken back.
    pub fn danger(mut self, on: bool) -> Self {
        self.danger = on;
        self
    }

    /// What choosing it does (the list closes after).
    pub fn on_select(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.press = Some(press(f));
        self
    }
}

/// A row, a separator, or a section label.
pub enum Entry<S: 'static> {
    Row(Row<S>),
    Separator,
    Label(String),
}

/// The list's inner padding: rows sit this far in, at the list radius minus it.
pub const INSET: f32 = 4.0;

/// Paints a row: `highlight` pins the highlight (a combobox's keyboard row, a picture); live rows highlight under the pointer and keys.
pub fn paint_row<S: 'static>(t: &Theme, r: &Row<S>, key: String, highlight: Option<bool>) -> El {
    let radius = Radius::inner(t.radius.lg, INSET);
    let ink = if r.disabled { t.ink_faint() } else if r.danger { t.c.danger.fg } else { t.ink() };
    let wash = if r.danger { t.c.danger.bg } else { t.c.selected };
    let mut e = row().key(key).label(r.label.clone()).min_h(t.metrics.row).px(t.metrics.pad_md - INSET).gap(t.metrics.gap).center_y().radius(radius).round_as(t);
    e = match highlight {
        Some(true) => e.fill(wash),
        Some(false) => e.hover(wash),
        None if !r.disabled => e.focus_highlight(wash),
        None => e,
    };
    if let Some(i) = r.icon {
        e = e.child(glyph(i, t.control.icon, if r.disabled || r.danger { ink } else { t.ink_muted() }));
    }
    let mut words = col().grow(1.0).shrink().min_w(0.0).gap(1.0).child(text(r.label.clone()).font(t.ty.base).color(ink).truncate().a11y_hidden());
    if let Some(d) = &r.description {
        words = words.child(text(d.clone()).font(t.ty.sm).color(t.ink_faint()).truncate().a11y_hidden());
        e = e.py(t.space.at(1.5)).description(d.clone());
    }
    e = e.child(words);
    if let Some(k) = &r.shortcut {
        e = e.child(text(k.clone()).font(tabular(t.ty.sm)).color(t.ink_faint()).a11y_hidden()).shortcut_hint(k.clone());
    }
    match r.checked {
        Some(true) => e = e.child(glyph(&icons::CHECK, t.control.icon, ink)),
        Some(false) => e = e.child(el().size(t.control.icon, t.control.icon)),
        None => {}
    }
    if r.disabled {
        e = e.disabled(true);
    }
    e
}

/// A floating list.
pub struct Popup<S: 'static> {
    pub key: String,
    /// `"listbox"` (rows are options) or `"menu"` (rows are menu items).
    pub role: &'static str,
    pub name: String,
    pub entries: Vec<Entry<S>>,
    pub min_w: f32,
    pub max_h: f32,
}

impl<S: 'static> Popup<S> {
    pub fn new(key: impl Into<String>, role: &'static str, name: impl Into<String>) -> Popup<S> {
        Popup { key: key.into(), role, name: name.into(), entries: Vec::new(), min_w: 160.0, max_h: 320.0 }
    }

    /// The surface alone (`close` runs after a row is chosen and on Tab).
    pub fn surface(self, t: &Theme, close: Press<S>) -> El {
        let option = self.role == "listbox";
        let mut rows = col().key("rows").w_full().gap(1.0);
        let mut targets: Vec<(String, String)> = Vec::new();
        let mut i = 0;
        for entry in self.entries {
            i += 1;
            match entry {
                Entry::Separator => rows = rows.child(el().key(format!("sep{i}")).h(1.0).my(INSET).mx(-INSET).fill(t.c.line).a11y_hidden()),
                Entry::Label(l) => rows = rows.child(row().key(format!("label{i}")).h(t.metrics.row - 8.0).px(t.metrics.pad_md - INSET).center_y().child(text(l).font(weight(t.ty.xs, t.ty.medium)).color(t.ink_faint()))),
                Entry::Row(r) => {
                    let key = r.key.clone().unwrap_or_else(|| format!("{}-item{i}", self.key));
                    let current = r.checked == Some(true);
                    let mut e = paint_row(t, &r, key.clone(), None).role(if option { "option" } else if r.checked.is_some() { "menuitemcheckbox" } else { "menuitem" });
                    if let Some(c) = r.checked {
                        e = e.checked(c);
                    }
                    if !r.disabled {
                        targets.push((key, r.label.to_lowercase()));
                        e = e.focusable(true).focus_selected(current);
                        if current {
                            e = e.autofocus();
                        }
                        let close = close.clone();
                        let f = r.press.clone();
                        e = e.on_press(move |s: &mut S| {
                            if let Some(f) = &f {
                                f(s);
                            }
                            close(s);
                        });
                    }
                    rows = rows.child(e);
                }
            }
        }
        let list = col().key(format!("{}-list", self.key)).role(self.role).label(self.name.clone()).focus_group().w_full().max_h(self.max_h).shrink().min_h(0.0).child(rune_kit::popover::fit_scroll(rows));
        let keys = close.clone();
        // `shrink` also zeroes the least width, so the trigger's width goes after it.
        let surface = col().key(format!("{}-content", self.key)).shrink().min_h(0.0).min_w(self.min_w).max_w(self.min_w.max(360.0)).padding(INSET).radius(t.radius.lg).round_as(t);
        paint(surface, &t.mat.floating.base).child(list).on_key(move |s: &mut S, k: &KeyEvent, tx: &mut Tx| match &k.key {
            Key::Tab => {
                keys(s);
                true
            }
            // Typeahead: the next row (after the focused one) whose words start with the letter typed.
            Key::Text(ch) if !k.mods.cmd && !k.mods.ctrl && ch.chars().count() == 1 => {
                let ch = ch.to_lowercase();
                let at = tx.focused().and_then(|f| targets.iter().position(|(key, _)| key == f)).map_or(0, |p| p + 1);
                let n = targets.len();
                match (0..n).map(|d| &targets[(at + d) % n.max(1)]).find(|(_, l)| l.starts_with(&ch)) {
                    Some((key, _)) => {
                        tx.focus(key.clone(), true);
                        true
                    }
                    None => false,
                }
            }
            _ => false,
        })
    }
}

/// Where a floating surface goes and what it gives focus back to: the arguments of [`mount`].
pub struct Mount<'a> {
    /// The surface's key (its scrim is keyed `<key>-scrim`).
    pub key: &'a str,
    /// The element it opens beside.
    pub anchor: &'a str,
    pub side: Side,
    /// What takes focus when it closes.
    pub return_to: &'a str,
}

/// Mounts `surface` into `layer` beside the element keyed `at.anchor` (on `at.side`, aligned to its start), with the scrim that closes
/// it on a press outside, a modal that closes it on Escape, focus going back to `at.return_to`, and the open and close motion.
pub fn mount<S: 'static>(t: &Theme, layer: &mut Vec<El>, surface: El, at: Mount, close: Press<S>) {
    let outside = close.clone();
    let modal = surface.focus_return(at.return_to).modal(true, move |s: &mut S| close(s));
    layer.push(rune_kit::scrim(format!("{}-scrim", at.key), move |s: &mut S| outside(s)));
    let from = Vec2 { x: 0.0, y: if at.side == Side::Top { 1.0 } else { 0.0 } };
    let motion = Transition::scale_anchor(0.97, from).springs(SpringPair::new(Spring::new(0.14, 0.0), Spring::new(0.09, 0.0)));
    let gap = t.space.at(1.0);
    let placed = modal.anchored_fit(at.anchor, at.side, Align::Start, gap);
    layer.push(if t.reduced_motion { placed } else { placed.transition(motion) });
}

/// A one-point anchor at a window point (a context menu opens where the pointer pressed); mount it before the surface.
pub fn point_anchor(key: &str, x: f32, y: f32) -> El {
    el().key(key.to_owned()).absolute(x, y).size(1.0, 1.0).a11y_hidden()
}

/// The shared handler type, for components building rows from strings.
pub type Choose<S> = Rc<dyn Fn(&mut S, &str)>;
