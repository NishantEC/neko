//! rune-ui registry: split 0.11.0. Copied into your app by `rune-ui add split`; yours to change.
//!
//! The regions of a window: an optional **left** region (a sidebar), the **centre** that takes what is left, an optional **right**
//! region (a pane), side by side, with a drag handle on each edge between a region and the centre. The regions have a width in
//! points (not a share, as `Resizable` has): the app keeps each width and hears every change, so it can save and restore them.
//!
//! * **Bounds.** Each region has a `min` and a `max` in points, and an optional `max_fraction` of the whole (a right pane never wider
//!   than half the window). The centre keeps `center_min` points: a region stops growing before it takes them.
//! * **Drag** the handle (a hairline with a 9 point grab area); **double-click** it to go back to the region's `default`.
//! * **Keys.** A focused handle is a `separator` named "Resize <region>" with its width as its value. Left and Right move it by
//!   `step` points (Shift: four steps), Home and End go to the least and most, Return collapses or opens the region.
//! * **Collapse.** `collapsed(true)` removes the region (its handle stays at the edge); `collapsible(true)` lets a drag past the
//!   least collapse it, and a drag from a collapsed edge open it again. `on_collapse` hears it. The app draws its own button.
//! * Works for a sidebar and a pane at once (three columns), or either alone (two).
//!
//! ```ignore
//! Split::new("window", "Window regions")
//!     .left(Region::new(sidebar).name("sidebar").width(s.side).min(208.0).max(360.0).default(256.0).collapsed(s.side_hidden))
//!     .center(conversation)
//!     .right(Region::new(pane).name("pane").width(s.pane).min(320.0).max(560.0).max_fraction(0.5).default(380.0))
//!     .on_resize(|s: &mut App, edge, w| match edge { Edge::Left => s.side = w, Edge::Right => s.pane = w })
//!     .on_collapse(|s: &mut App, edge, hidden| s.hide(edge, hidden)).build(t)
//! ```

use std::rc::Rc;

use rune::{Cursor, Drag, El, Key, KeyEvent, Phase, Tx, col, el, row};

use super::base::{State, Theme};

thread_local! {
    static LENGTHS: std::cell::RefCell<std::collections::HashMap<String, f32>> = std::cell::RefCell::new(std::collections::HashMap::new());
}

/// A handle's grab area. Layout gives it one point (the hairline): the rest overlaps the regions beside it.
pub const HANDLE: f32 = 9.0;

/// How far past the least width a drag goes before a collapsible region collapses, and how far from a collapsed edge before it opens.
pub const SNAP: f32 = 48.0;

/// Which region.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    Left,
    Right,
}

/// One side region: its content and its bounds.
pub struct Region {
    content: El,
    name: String,
    width: f32,
    min: f32,
    max: f32,
    max_fraction: f32,
    default: f32,
    collapsed: bool,
    collapsible: bool,
}

impl Region {
    pub fn new(content: El) -> Region {
        Region { content, name: String::new(), width: 256.0, min: 120.0, max: 600.0, max_fraction: 1.0, default: 256.0, collapsed: false, collapsible: false }
    }

    /// What the handle says it resizes ("sidebar", "pane").
    pub fn name(mut self, n: impl Into<String>) -> Self {
        self.name = n.into();
        self
    }

    /// Its width now, in points: the app's saved value.
    pub fn width(mut self, w: f32) -> Self {
        self.width = w;
        self
    }

    pub fn min(mut self, w: f32) -> Self {
        self.min = w;
        self
    }

    pub fn max(mut self, w: f32) -> Self {
        self.max = w;
        self
    }

    /// The most it may take of the whole width (0 to 1).
    pub fn max_fraction(mut self, f: f32) -> Self {
        self.max_fraction = f;
        self
    }

    /// The width a double-click on the handle goes back to.
    pub fn default(mut self, w: f32) -> Self {
        self.default = w;
        self
    }

    pub fn collapsed(mut self, on: bool) -> Self {
        self.collapsed = on;
        self
    }

    /// A drag past the least width collapses it, and a drag from the collapsed edge opens it.
    pub fn collapsible(mut self, on: bool) -> Self {
        self.collapsible = on;
        self
    }
}

/// What a width change does: which region, the new width.
pub type OnResize<S> = Rc<dyn Fn(&mut S, Edge, f32)>;
/// What a collapse does: which region, whether it is now collapsed.
pub type OnCollapse<S> = Rc<dyn Fn(&mut S, Edge, bool)>;

/// `w` kept inside the region's least and most, and inside what the whole leaves: `total` is the split's width, `other` what the other
/// region (and its handle) take (its width and one point), this region's own handle is one more point, `center_min` what the centre keeps. Without a measured `total` (0) only min and max apply.
pub fn clamped(w: f32, min: f32, max: f32, max_fraction: f32, total: f32, other: f32, center_min: f32) -> f32 {
    let mut hi = max;
    if total > 0.0 {
        hi = hi.min(total * max_fraction).min(total - other - center_min - 1.0);
    }
    w.clamp(min, hi.max(min))
}

pub struct Split<S: 'static> {
    key: String,
    name: String,
    left: Option<Region>,
    center: Option<El>,
    right: Option<Region>,
    center_min: f32,
    step: f32,
    pinned: Option<(Edge, State)>,
    on_resize: Option<OnResize<S>>,
    on_collapse: Option<OnCollapse<S>>,
}

impl<S: 'static> Split<S> {
    pub fn new(key: impl Into<String>, name: impl Into<String>) -> Split<S> {
        Split { key: key.into(), name: name.into(), left: None, center: None, right: None, center_min: 320.0, step: 16.0, pinned: None, on_resize: None, on_collapse: None }
    }

    pub fn left(mut self, r: Region) -> Self {
        self.left = Some(r);
        self
    }

    pub fn center(mut self, c: El) -> Self {
        self.center = Some(c);
        self
    }

    pub fn right(mut self, r: Region) -> Self {
        self.right = Some(r);
        self
    }

    /// What the centre keeps (default 320 points).
    pub fn center_min(mut self, w: f32) -> Self {
        self.center_min = w;
        self
    }

    /// How far an arrow key moves a focused handle (default 16 points; Shift four times that).
    pub fn step(mut self, s: f32) -> Self {
        self.step = s;
        self
    }

    /// Shows a handle as if hovered or focused (for a gallery).
    pub fn state(mut self, e: Edge, s: State) -> Self {
        self.pinned = Some((e, s));
        self
    }

    pub fn on_resize(mut self, f: impl Fn(&mut S, Edge, f32) + 'static) -> Self {
        self.on_resize = Some(Rc::new(f));
        self
    }

    pub fn on_collapse(mut self, f: impl Fn(&mut S, Edge, bool) + 'static) -> Self {
        self.on_collapse = Some(Rc::new(f));
        self
    }

    /// The key of a region's handle.
    pub fn handle_key(key: &str, e: Edge) -> String {
        format!("{key}-handle-{}", if e == Edge::Left { "left" } else { "right" })
    }

    /// The key of a region (`left`, `right`) or the centre.
    pub fn region_key(key: &str, e: Option<Edge>) -> String {
        format!("{key}-{}", match e { Some(Edge::Left) => "left", Some(Edge::Right) => "right", None => "center" })
    }

    fn handle(&self, t: &Theme, e: Edge, me: &Region, other: f32) -> El {
        let k = self.key.clone();
        let hk = Self::handle_key(&k, e);
        let shown = if me.collapsed { 0.0 } else { me.width };
        let mid = (HANDLE - 1.0) / 2.0;
        let line = el().absolute(mid, 0.0).h_full().w(1.0).fill(t.c.line).a11y_hidden();
        let name = if me.name.is_empty() { if e == Edge::Left { "left region".to_owned() } else { "right region".to_owned() } } else { me.name.clone() };
        let mut h = col().key(hk).role("separator").label(format!("Resize {name}")).range_value(f64::from(shown), 0.0, f64::from(me.max)).description(if me.collapsed { "Collapsed" } else { "" }).focusable(true).h_full().w(HANDLE).mx(-mid).cursor(Cursor::ResizeColumn).radius(HANDLE / 2.0).focus_ring(t.control.ring, 0.0, t.c.ring).raise().child(line);
        h = match self.pinned {
            Some((pe, State::Hover)) if pe == e => h.fill(t.c.hover),
            Some((pe, State::FocusVisible)) if pe == e => h.border(t.control.ring, t.c.ring),
            _ => h.hover(t.c.hover),
        };
        let (min, max, frac, default, collapsible, collapsed, width) = (me.min, me.max, me.max_fraction, me.default, me.collapsible, me.collapsed, me.width);
        let (center_min, step) = (self.center_min, self.step);
        let sign = if e == Edge::Left { 1.0 } else { -1.0 };
        if let Some(resize) = self.on_resize.clone() {
            let (lk, collapse, r2) = (k.clone(), self.on_collapse.clone(), resize.clone());
            h = h.on_drag(move |s: &mut S, d: &mut Drag, tx: &mut Tx| {
                tx.instant();
                if d.phase == Phase::Down {
                    d.data = if collapsed { 0.0 } else { width };
                    if d.clicks >= 2 {
                        r2(s, e, default);
                        if collapsed && let Some(c) = &collapse {
                            c(s, e, false);
                        }
                        return;
                    }
                }
                if d.cancelled() || d.clicks >= 2 {
                    return;
                }
                let total = LENGTHS.with(|m| m.borrow().get(&lk).copied()).unwrap_or(0.0);
                let want = d.data + sign * d.dx;
                if collapsed {
                    if want >= SNAP.min(min) && let Some(c) = &collapse {
                        c(s, e, false);
                        r2(s, e, clamped(want.max(min), min, max, frac, total, other, center_min));
                    }
                    return;
                }
                if collapsible && want < min - SNAP && let Some(c) = &collapse {
                    c(s, e, true);
                    return;
                }
                r2(s, e, clamped(want, min, max, frac, total, other, center_min));
            });
            let collapse = self.on_collapse.clone();
            let lk = k.clone();
            h = h.on_key(move |s: &mut S, ev: &KeyEvent, _: &mut Tx| {
                let total = LENGTHS.with(|m| m.borrow().get(&lk).copied()).unwrap_or(0.0);
                let by = if ev.mods.shift { step * 4.0 } else { step };
                let now = if collapsed { min } else { width };
                let to = match ev.key {
                    Key::Right => now + sign * by,
                    Key::Left => now - sign * by,
                    Key::Home => min,
                    Key::End => max,
                    Key::Enter => {
                        return match &collapse {
                            Some(c) => {
                                c(s, e, !collapsed);
                                true
                            }
                            None => false,
                        };
                    }
                    _ => return false,
                };
                if collapsed && let Some(c) = &collapse {
                    c(s, e, false);
                }
                resize(s, e, clamped(to, min, max, frac, total, other, center_min));
                true
            });
        }
        h
    }

    pub fn build(self, t: &Theme) -> El {
        let k = self.key.clone();
        let mut group = row().key(k.clone()).role("group").label(self.name.clone()).w_full().h_full().min_w(0.0);
        let left = self.left.as_ref().map(|r| if r.collapsed { 0.0 } else { r.width + 1.0 }).unwrap_or(0.0);
        let right = self.right.as_ref().map(|r| if r.collapsed { 0.0 } else { r.width + 1.0 }).unwrap_or(0.0);
        let mut handles: Vec<(Edge, El)> = Vec::new();
        if let Some(r) = &self.left {
            handles.push((Edge::Left, self.handle(t, Edge::Left, r, right)));
        }
        if let Some(r) = &self.right {
            handles.push((Edge::Right, self.handle(t, Edge::Right, r, left)));
        }
        let mut handles = handles.into_iter();
        let (l, c, r) = (self.left, self.center, self.right);
        if let Some(region) = l {
            if !region.collapsed {
                group = group.child(col().key(Self::region_key(&k, Some(Edge::Left))).w(region.width).h_full().min_w(0.0).child(region.content));
            }
            if let Some((_, h)) = handles.next() {
                group = group.child(h);
            }
        }
        group = group.child(col().key(Self::region_key(&k, None)).h_full().grow(1.0).shrink().min_w(0.0).child(c.unwrap_or_else(el)));
        if let Some(region) = r {
            if let Some((_, h)) = handles.next() {
                group = group.child(h);
            }
            if !region.collapsed {
                group = group.child(col().key(Self::region_key(&k, Some(Edge::Right))).w(region.width).h_full().min_w(0.0).child(region.content));
            }
        }
        let lk = k;
        group.on_layout_tx(move |_: &mut S, rect: [f32; 4], tx: &mut Tx| {
            LENGTHS.with(|m| m.borrow_mut().insert(lk.clone(), rect[2]));
            tx.live_only();
        })
    }
}
