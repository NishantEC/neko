//! rune-ui registry: tabs 0.11.0. Copied into your app by `rune-ui add tabs`; yours to change. Needs: popup.
//!
//! Views of one place. **Line**: words over a hairline, the chosen tab in the first ink with a 2 point ink line under it. **Contained**:
//! a well track holding the chosen tab raised (like a toggle group). **Pane**: the Line look made compact for the header of a side pane
//! (one control-height row, smaller words, no hairline: the pane draws its own edge). A tab may lead with an icon and end with a count.
//!
//! **Keys.** The chosen tab is the strip's one Tab stop (roving). Left and Right move to the next and previous tab and wrap, Home and
//! End go to the ends. With *automatic* activation (the default) moving also chooses; with *manual* activation (`Activation::Manual`)
//! moving only moves focus, and Return or Space chooses. Roles: `tablist`, each `tab` says whether it is selected, and the panel is a
//! `tabpanel` labelled by the chosen tab.
//!
//! **Closable.** `on_close` gives every tab a close button (named "Close <tab>", shown on the chosen tab and while the pointer or focus
//! is on a tab) and makes Cmd+W on a focused tab close it. `permanent()` keeps the last tab you added from closing.
//!
//! **Overflow.** Give the tabs a [`TabsState`] (`overflow`) and build with `build_in`: the strip measures itself and its tabs, the
//! tabs that do not fit move into a "More tabs" menu (the chosen tab always stays in the strip), and the menu's rows choose or close
//! them. Without it the tabs share the row and cut their words.
//!
//! ```ignore
//! Tabs::new("project", "Project views").tab("Overview", None).permanent().tab("Activity", Some(3)).selected(s.tab)
//!     .on_change(|s: &mut App, i| s.tab = i).on_close(|s: &mut App, i| s.close_tab(i)).build(t)
//! ```

use std::rc::Rc;

use rune::{Color, El, Icon, Key, KeyEvent, Side, Tx, col, el, row, text};
use rune_ui_style::{Delta, Radius, tabular};

use super::base::{Choose, Press, Shape, State, States, Theme, glyph, hit_area, paint, paint_state, press};
use super::icons;
use super::popup::{Entry, Mount, Popup, Row, mount};

/// The looks.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TabsVariant {
    #[default]
    Line,
    Contained,
    /// The Line look, compact, for a side pane's header.
    Pane,
}

/// How a tab is chosen from the keys.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Activation {
    /// Moving to a tab chooses it.
    #[default]
    Automatic,
    /// Moving only moves focus; Return or Space chooses.
    Manual,
}

/// What the tabs measured, and whether the More menu is open. Keep one per strip in the app's state.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TabsState {
    /// The strip's own width (0 before it was laid out: every tab shows).
    pub width: f32,
    /// Each tab's width by index (0 until it was drawn).
    pub widths: Vec<f32>,
    pub more_open: bool,
}

/// Which of the tabs (by index) stay in a strip `avail` points wide: as many from the start as fit with `gap` between and room for
/// a More button of `more` when any are left out, and the chosen tab always, taking the place of the last that fits.
pub fn fitting(widths: &[f32], avail: f32, gap: f32, more: f32, selected: usize) -> Vec<usize> {
    let n = widths.len();
    if avail <= 0.0 || n == 0 {
        return (0..n).collect();
    }
    let total: f32 = widths.iter().sum::<f32>() + gap * (n as f32 - 1.0);
    if total <= avail + 0.5 {
        return (0..n).collect();
    }
    let room = avail - more - gap;
    let mut shown: Vec<usize> = Vec::new();
    let mut used = 0.0;
    for (i, w) in widths.iter().enumerate() {
        let next = used + w + if shown.is_empty() { 0.0 } else { gap };
        if next > room + 0.5 {
            break;
        }
        used = next;
        shown.push(i);
    }
    let sel = selected.min(n - 1);
    if !shown.contains(&sel) {
        // The chosen tab takes the last place; if even that does not fit with the one before it, it stands alone.
        let w = widths[sel];
        while !shown.is_empty() && shown.iter().map(|&i| widths[i]).sum::<f32>() + gap * shown.len() as f32 + w > room + 0.5 {
            shown.pop();
        }
        shown.push(sel);
    }
    shown
}

/// How the strip reaches its [`TabsState`] in the app's state.
type StateAccess<S> = Rc<dyn for<'a> Fn(&'a mut S) -> &'a mut TabsState>;

struct Tab {
    label: String,
    icon: Option<&'static Icon>,
    count: Option<usize>,
    permanent: bool,
}

pub struct Tabs<S: 'static> {
    key: String,
    name: String,
    tabs: Vec<Tab>,
    selected: usize,
    variant: TabsVariant,
    activation: Activation,
    pinned: Option<(usize, State)>,
    content: Option<El>,
    change: Option<Choose<S>>,
    close: Option<Choose<S>>,
    overflow: Option<(TabsState, StateAccess<S>)>,
}

/// The key of tab `i`.
pub fn tab_key(key: &str, i: usize) -> String {
    format!("{key}-{i}")
}

impl<S: 'static> Tabs<S> {
    pub fn new(key: impl Into<String>, name: impl Into<String>) -> Tabs<S> {
        Tabs { key: key.into(), name: name.into(), tabs: Vec::new(), selected: 0, variant: TabsVariant::Line, activation: Activation::Automatic, pinned: None, content: None, change: None, close: None, overflow: None }
    }

    /// A tab and an optional count after its words.
    pub fn tab(mut self, label: impl Into<String>, count: Option<usize>) -> Self {
        self.tabs.push(Tab { label: label.into(), icon: None, count, permanent: false });
        self
    }

    /// An icon before the last tab's words.
    pub fn icon(mut self, i: &'static Icon) -> Self {
        if let Some(t) = self.tabs.last_mut() {
            t.icon = Some(i);
        }
        self
    }

    /// The last tab you added cannot be closed (an overview): no close button, and Cmd+W leaves it.
    pub fn permanent(mut self) -> Self {
        if let Some(t) = self.tabs.last_mut() {
            t.permanent = true;
        }
        self
    }

    pub fn selected(mut self, i: usize) -> Self {
        self.selected = i;
        self
    }

    pub fn variant(mut self, v: TabsVariant) -> Self {
        self.variant = v;
        self
    }

    /// Whether moving with the arrows chooses (the default) or only moves focus.
    pub fn activation(mut self, a: Activation) -> Self {
        self.activation = a;
        self
    }

    pub fn state(mut self, i: usize, s: State) -> Self {
        self.pinned = Some((i, s));
        self
    }

    /// What the chosen tab shows, under the tabs.
    pub fn content(mut self, c: El) -> Self {
        self.content = Some(c);
        self
    }

    pub fn on_change(mut self, f: impl Fn(&mut S, usize) + 'static) -> Self {
        self.change = Some(Rc::new(f));
        self
    }

    /// Every tab (except a `permanent` one) gets a close button, and Cmd+W closes the focused one. You remove the tab and fix the
    /// chosen index.
    pub fn on_close(mut self, f: impl Fn(&mut S, usize) + 'static) -> Self {
        self.close = Some(Rc::new(f));
        self
    }

    /// Fold the tabs that do not fit into a More menu. `build_in` mounts the menu.
    pub fn overflow(mut self, state: &TabsState, access: impl for<'a> Fn(&'a mut S) -> &'a mut TabsState + 'static) -> Self {
        self.overflow = Some((state.clone(), Rc::new(access)));
        self
    }

    pub fn build(self, t: &Theme) -> El {
        self.build_in(t, &mut Vec::new())
    }

    pub fn build_in(self, t: &Theme, layer: &mut Vec<El>) -> El {
        let variant = self.variant;
        let line = variant != TabsVariant::Contained;
        let pane = variant == TabsVariant::Pane;
        let h = if pane { t.control.sm } else { t.control.md };
        let n = self.tabs.len();
        let k = self.key.clone();
        let gap = if line { t.metrics.inset - 4.0 } else { 2.0 };
        let more_w = t.control.md;
        let widths: Vec<f32> = (0..n).map(|i| {
            let m = self.overflow.as_ref().and_then(|(st, _)| st.widths.get(i).copied()).unwrap_or(0.0);
            if m > 0.0 { m } else { t.control.md + self.tabs[i].label.chars().count() as f32 * t.ty.base.size * 0.55 }
        }).collect();
        let shown: Vec<usize> = match (&self.overflow, line) {
            (Some((st, _)), true) => fitting(&widths, st.width, gap, more_w, self.selected),
            _ => (0..n).collect(),
        };
        let mut strip = row().key(k.clone()).role("tablist").label(self.name.clone());
        strip = if line {
            let mut s = strip.w_full().min_w(0.0).gap(gap).center_y();
            if !pane {
                s = s.border_sides([0.0, 0.0, 1.0, 0.0], t.c.line);
            }
            s
        } else {
            paint(strip.h(h).padding(3.0).gap(2.0).center_y().radius(t.radius.md).round_as(t), &t.mat.well.base)
        };
        let chosen = t.mat.raised.clone().on(State::Pressed, Delta::new());
        let close_all = self.close.clone();
        let change = self.change.clone();
        let activation = self.activation;
        let selected = self.selected;
        let mut hidden: Vec<Hidden> = Vec::new();
        for (i, tab) in self.tabs.into_iter().enumerate() {
            if !shown.contains(&i) {
                hidden.push((i, tab.label, tab.icon));
                continue;
            }
            let on = i == selected;
            let mut st = States::default();
            if let Some((at, s)) = self.pinned
                && at == i
            {
                st.set(s, true);
            }
            let size = if pane { t.ty.sm } else { t.ty.base };
            let ink = if on { t.ink() } else { t.ink_muted() };
            let mut face = row().gap(t.space.at(1.5)).center_y();
            if let Some(ic) = tab.icon {
                face = face.child(glyph(ic, t.control.icon, ink));
            }
            let w = if pane { t.ty.semibold } else { t.ty.medium };
            face = face.child(text(tab.label.clone()).font(rune_ui_style::weight(size, if on { w } else { t.ty.regular })).color(ink).truncate().min_w(0.0).a11y_hidden());
            if let Some(c) = tab.count {
                face = face.child(text(c.to_string()).font(tabular(t.ty.xs)).color(t.ink_faint()).a11y_hidden());
            }
            let closable = close_all.is_some() && !tab.permanent;
            if closable && let Some(f) = close_all.clone() {
                face = face.child(close_button(t, &k, i, &tab.label, on || st.hover || st.focus_visible, f));
            }
            let measure = self.overflow.as_ref().map(|(_, a)| a.clone());
            let mut cell = if line {
                let px = t.space.at(1.5);
                let mut face = paint_state(face.h(h).px(px).radius(t.radius.sm).round_as(t), &t.mat.row, States { focus_visible: false, ..st });
                if closable {
                    face = face.hover_group();
                }
                let bar = el().w_full().h(2.0).radius(1.0).fill(if on { t.c.primary.bg } else { Color::TRANSPARENT }).a11y_hidden();
                let mut c = col().gap(t.space.at(0.5)).min_w(0.0).child(face).child(bar);
                if st.focus_visible {
                    c = paint(c, &t.mat.ghost.state(State::FocusVisible));
                }
                c.focus_ring(t.control.ring, t.control.ring_offset, t.c.ring)
            } else {
                paint_state(face.h(h - 6.0).px(t.space.at(2.5)).radius(Radius::inner(t.radius.md, 3.0)).round_as(t), if on { &chosen } else { &t.mat.row }, st)
            };
            cell = cell.key(tab_key(&k, i)).role("tab").label(tab.label.clone()).focus_selected(on).focusable(true).tab_index(if on { 0 } else { -1 });
            if self.content.is_some() && on {
                cell = cell.controls(format!("{k}-panel"));
            }
            if let Some(a) = measure {
                cell = cell.on_layout(move |s: &mut S, r: [f32; 4]| {
                    let st = a(s);
                    if st.widths.len() < n {
                        st.widths.resize(n, 0.0);
                    }
                    if (st.widths[i] - r[2]).abs() > 0.5 {
                        st.widths[i] = r[2];
                    }
                });
            }
            if let Some(f) = change.clone() {
                let g = f.clone();
                cell = cell.on_press(move |s: &mut S| g(s, i));
            }
            let choose = change.clone();
            let close = if closable { close_all.clone() } else { None };
            let (shown, k2) = (shown.clone(), k.clone());
            cell = cell.on_key(move |s: &mut S, ev: &KeyEvent, tx: &mut Tx| {
                if ev.mods.cmd && !ev.mods.alt && matches!(&ev.key, Key::Text(w) if w.eq_ignore_ascii_case("w")) {
                    return match &close {
                        Some(c) => {
                            c(s, i);
                            true
                        }
                        None => false,
                    };
                }
                if ev.mods.cmd || ev.mods.alt || ev.mods.ctrl {
                    return false;
                }
                let at = shown.iter().position(|&x| x == i).unwrap_or(0);
                let m = shown.len();
                let to = match ev.key {
                    Key::Right => shown[(at + 1) % m],
                    Key::Left => shown[(at + m - 1) % m],
                    Key::Home => shown[0],
                    Key::End => shown[m - 1],
                    Key::Enter if activation == Activation::Manual => {
                        if let Some(f) = &choose {
                            f(s, i);
                        }
                        return true;
                    }
                    Key::Text(ref w) if w == " " && activation == Activation::Manual => {
                        if let Some(f) = &choose {
                            f(s, i);
                        }
                        return true;
                    }
                    _ => return false,
                };
                if activation == Activation::Automatic
                    && let Some(f) = &choose
                {
                    f(s, to);
                }
                tx.focus(tab_key(&k2, to), true);
                true
            });
            strip = strip.child(cell);
        }
        if !hidden.is_empty()
            && let Some((state, access)) = self.overflow.clone()
        {
            strip = strip.child(more_menu(t, &k, &hidden, state.more_open, access, change, layer));
        }
        let strip = match self.overflow {
            Some((_, access)) if line => strip.on_layout(move |s: &mut S, r: [f32; 4]| {
                let st = access(s);
                if (st.width - r[2]).abs() > 0.5 {
                    st.width = r[2];
                }
            }),
            _ => strip,
        };
        match self.content {
            Some(c) => {
                let panel = col().key(format!("{k}-panel")).role("tabpanel").labelled_by(tab_key(&k, selected)).w_full().child(c);
                col().w_full().gap(t.space.at(4.0)).child(strip).child(panel)
            }
            None => strip,
        }
    }
}

/// A tab's close button: its own 18 point target, a faint close mark, named "Close <tab>". Out of the Tab order (Cmd+W on the tab
/// closes it from the keys); drawn only while the tab is chosen or under the pointer or focus.
fn close_button<S: 'static>(t: &Theme, key: &str, i: usize, label: &str, shown: bool, close: Choose<S>) -> El {
    let ckey = format!("{}-close", tab_key(key, i));
    let f: Press<S> = press(move |s: &mut S| close(s, i));
    let g = f.clone();
    let mut b = row().key(ckey.clone()).role("button").label(format!("Close {label}")).help(format!("Close {label}")).focusable(true).tab_index(-1).size(18.0, 18.0).center().radius(t.radius.sm).round_as(t);
    b = paint_state(b, &t.mat.ghost, States::default()).child(glyph(&icons::CLOSE, t.control.icon_sm, t.ink_muted()));
    if !shown {
        b = b.group_opacity(0.0, 1.0);
    }
    let b = b.on_press(move |s: &mut S| g(s));
    hit_area(&ckey, b, 3.0, 0.0, Some(f))
}

type Hidden = (usize, String, Option<&'static Icon>);

/// The "More tabs" button at the strip's end and, while open, its menu: a row per hidden tab that chooses it.
fn more_menu<S: 'static>(t: &Theme, key: &str, hidden: &[Hidden], open: bool, access: StateAccess<S>, change: Option<Choose<S>>, layer: &mut Vec<El>) -> El {
    let more = format!("{key}-more");
    let h = t.control.md;
    let acc = access.clone();
    let toggle = press(move |s: &mut S| {
        let st = acc(s);
        st.more_open = !st.more_open;
    });
    let mut face = row().key(more.clone()).role("button").label(format!("More tabs, {} hidden", hidden.len())).help("More tabs").expanded(open).focusable(true).size(h, h).center().radius(Radius::fit(t.radius.md, h)).round_as(t);
    face = paint_state(face, &t.mat.ghost, States { selected: open, ..States::default() }).child(glyph(&icons::CHEVRONS_RIGHT, t.control.icon, t.ink_muted()));
    let f = toggle.clone();
    face = face.on_press(move |s: &mut S| f(s));
    let slop = t.control.slop(h);
    let anchor = row().key(format!("{more}-anchor")).child(hit_area(&more, face, slop.min(t.space.at(0.5)), slop, Some(toggle)));
    if open {
        let acc = access.clone();
        let shut = press(move |s: &mut S| acc(s).more_open = false);
        let mut popup = Popup::<S>::new(more.clone(), "menu", "More tabs");
        for (i, label, icon) in hidden {
            let mut r = Row::new(label.clone()).key(format!("{more}-{i}"));
            if let Some(ic) = icon {
                r = r.icon(ic);
            }
            if let Some(f) = change.clone() {
                let (i, acc) = (*i, access.clone());
                r = r.on_select(move |s: &mut S| {
                    acc(s).more_open = false;
                    f(s, i);
                });
            }
            popup.entries.push(Entry::Row(r));
        }
        let surface = popup.surface(t, shut.clone());
        let at = format!("{more}-anchor");
        mount(t, layer, surface, Mount { key: &more, anchor: &at, side: Side::Bottom, return_to: &more }, shut);
    }
    anchor
}
