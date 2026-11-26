//! rune-ui registry: sidebar 0.9.0. Copied into your app by `rune-ui add sidebar`; yours to change.
//!
//! An app's side column: a header, groups of rows under quiet labels, a footer. A row is an icon, its words and an optional count;
//! it washes on hover and takes the selected wash and the first ink when it is the current page. The column sits on the slab colour.
//! A row can lead with a status mark instead of its icon (`lead`: a spinner, a coloured dot, a warning), on the icon's own box, and can
//! say it is one row of a list (`list_row`: an `option` with its place in the list and whether it is selected; put the rows in a
//! `listbox` with `focus_group`, and Return opens a row as a press does).
//!
//! ```ignore
//! Sidebar::new("nav").header(brand).group(Some("Workspace"), vec![SidebarItem::new("home", "Home").icon(&icons::HOME).selected(true)])
//!     .footer(account).build(t)
//! ```

use std::rc::Rc;

use rune::{El, Icon, col, row, text};
use rune_ui_style::{tabular, weight};

use super::base::{Press, Shape, State, States, Theme, glyph, paint, paint_state};

/// One row.
pub struct SidebarItem<S: 'static> {
    key: String,
    label: String,
    icon: Option<&'static Icon>,
    lead: Option<El>,
    count: Option<usize>,
    list: Option<(usize, usize)>,
    description: Option<String>,
    selected: bool,
    pinned: Option<State>,
    press: Option<Press<S>>,
}

impl<S: 'static> SidebarItem<S> {
    pub fn new(key: impl Into<String>, label: impl Into<String>) -> SidebarItem<S> {
        SidebarItem { key: key.into(), label: label.into(), icon: None, lead: None, count: None, list: None, description: None, selected: false, pinned: None, press: None }
    }

    pub fn icon(mut self, i: &'static Icon) -> Self {
        self.icon = Some(i);
        self
    }

    /// A status mark in the icon's place (a spinner, a coloured dot, a warning), centred on the icon's box. It replaces the icon.
    pub fn lead(mut self, e: El) -> Self {
        self.lead = Some(e);
        self
    }

    /// One row of a list, for assistive technology: an `option` at `position` (1-based) of `size`, selected while `selected`, and
    /// the list's Tab stop when selected. Return presses it, as a click does. The rows' parent should be a `listbox` with
    /// `focus_group`.
    pub fn list_row(mut self, position: usize, size: usize) -> Self {
        self.list = Some((position, size));
        self
    }

    /// Words assistive technology reads after the name ("Running", "Needs you"): what a lead mark shows.
    pub fn description(mut self, words: impl Into<String>) -> Self {
        self.description = Some(words.into());
        self
    }

    pub fn count(mut self, n: usize) -> Self {
        self.count = Some(n);
        self
    }

    pub fn selected(mut self, on: bool) -> Self {
        self.selected = on;
        self
    }

    pub fn state(mut self, s: State) -> Self {
        self.pinned = Some(s);
        self
    }

    pub fn on_press(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.press = Some(Rc::new(f));
        self
    }

    pub fn build(self, t: &Theme) -> El {
        let mut st = States { selected: self.selected, ..States::default() };
        if let Some(p) = self.pinned {
            st.set(p, true);
        }
        let ink = if self.selected { t.ink() } else { t.ink_muted() };
        let mut r = row().key(self.key).role(if self.list.is_some() { "option" } else { "link" }).label(self.label.clone()).focusable(true).w_full().h(t.metrics.row).px(t.metrics.gap).gap(t.metrics.gap).center_y().radius(t.radius.md).round_as(t);
        match self.list {
            Some((position, size)) => r = r.position_in_set(position, size).focus_selected(self.selected).press_on_return(),
            None if self.selected => r = r.checked(true),
            None => {}
        }
        if let Some(d) = self.description {
            r = r.description(d);
        }
        r = paint_state(r, &t.mat.row, st);
        let icon = t.control.icon_lg;
        if let Some(e) = self.lead {
            r = r.child(row().size(icon, icon).center().a11y_hidden().child(e));
        } else if let Some(i) = self.icon {
            r = r.child(glyph(i, icon, ink));
        }
        r = r.child(text(self.label).font(weight(t.ty.base, if self.selected { t.ty.medium } else { t.ty.regular })).color(ink).grow(1.0).truncate().min_w(0.0).a11y_hidden());
        if let Some(n) = self.count {
            r = r.child(text(n.to_string()).font(tabular(t.ty.sm)).color(t.ink_faint()).a11y_hidden());
        }
        if let Some(f) = self.press {
            r = r.on_press(move |s: &mut S| f(s));
        }
        r
    }
}

/// A group: its quiet label, its list name (a `listbox` when set), its rows.
type Group<S> = (Option<String>, Option<String>, Vec<SidebarItem<S>>);

pub struct Sidebar<S: 'static> {
    key: String,
    width: f32,
    header: Option<El>,
    groups: Vec<Group<S>>,
    footer: Option<El>,
}

impl<S: 'static> Sidebar<S> {
    pub fn new(key: impl Into<String>) -> Sidebar<S> {
        Sidebar { key: key.into(), width: 240.0, header: None, groups: Vec::new(), footer: None }
    }

    pub fn width(mut self, w: f32) -> Self {
        self.width = w;
        self
    }

    pub fn header(mut self, h: El) -> Self {
        self.header = Some(h);
        self
    }

    /// A group of rows, under a quiet label if it has one.
    pub fn group(mut self, label: Option<&str>, items: Vec<SidebarItem<S>>) -> Self {
        self.groups.push((label.map(str::to_owned), None, items));
        self
    }

    /// A group that is one list for assistive technology: a `listbox` named `name` (one Tab stop, Up and Down move inside), each row
    /// an `option` with its place in the list ([`SidebarItem::list_row`]).
    pub fn list(mut self, label: Option<&str>, name: &str, items: Vec<SidebarItem<S>>) -> Self {
        self.groups.push((label.map(str::to_owned), Some(name.to_owned()), items));
        self
    }

    pub fn footer(mut self, f: El) -> Self {
        self.footer = Some(f);
        self
    }

    pub fn build(self, t: &Theme) -> El {
        let mut c = paint(col().key(self.key.clone()).role("navigation").label("Sidebar").w(self.width).h_full().padding(t.space.at(2.0)).gap(t.space.at(4.0)), &t.mat.slab.base);
        if let Some(h) = self.header {
            c = c.child(row().w_full().px(t.space.at(2.0)).h(t.control.lg).center_y().child(h));
        }
        for (gi, (label, list, items)) in self.groups.into_iter().enumerate() {
            let mut g = col().w_full().gap(1.0);
            if let Some(l) = label {
                g = g.child(row().h(24.0).px(t.space.at(2.0)).center_y().child(text(l).font(weight(t.ty.xs, t.ty.medium)).color(t.ink_faint())));
            }
            match list {
                Some(name) => {
                    let n = items.len();
                    let rows = items.into_iter().enumerate().map(|(i, it)| (if it.list.is_some() { it } else { it.list_row(i + 1, n) }).build(t));
                    g = g.child(col().key(format!("{}-list-{gi}", self.key)).w_full().gap(1.0).role("listbox").label(name).focus_group().children(rows));
                }
                None => {
                    for i in items {
                        g = g.child(i.build(t));
                    }
                }
            }
            c = c.child(g);
        }
        if let Some(f) = self.footer {
            c = c.child(rune::spacer()).child(row().w_full().px(t.space.at(2.0)).child(f));
        }
        c
    }
}
