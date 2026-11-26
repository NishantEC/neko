//! rune-ui registry: sidebar 0.5.0. Copied into your app by `rune-ui add sidebar`; yours to change.
//!
//! An app's side column: a header, groups of rows under quiet labels, a footer. A row is an icon, its words and an optional count;
//! it washes on hover and takes the selected wash and the first ink when it is the current page. The column sits on the slab colour.
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
    count: Option<usize>,
    selected: bool,
    pinned: Option<State>,
    press: Option<Press<S>>,
}

impl<S: 'static> SidebarItem<S> {
    pub fn new(key: impl Into<String>, label: impl Into<String>) -> SidebarItem<S> {
        SidebarItem { key: key.into(), label: label.into(), icon: None, count: None, selected: false, pinned: None, press: None }
    }

    pub fn icon(mut self, i: &'static Icon) -> Self {
        self.icon = Some(i);
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
        let mut r = row().key(self.key).role("link").label(self.label.clone()).focusable(true).w_full().h(t.metrics.row).px(t.metrics.gap).gap(t.metrics.gap).center_y().radius(t.radius.md).round_as(t);
        if self.selected {
            r = r.checked(true);
        }
        r = paint_state(r, &t.mat.row, st);
        if let Some(i) = self.icon {
            r = r.child(glyph(i, t.control.icon_lg, ink));
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

pub struct Sidebar<S: 'static> {
    key: String,
    width: f32,
    header: Option<El>,
    groups: Vec<(Option<String>, Vec<SidebarItem<S>>)>,
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
        self.groups.push((label.map(str::to_owned), items));
        self
    }

    pub fn footer(mut self, f: El) -> Self {
        self.footer = Some(f);
        self
    }

    pub fn build(self, t: &Theme) -> El {
        let mut c = paint(col().key(self.key).role("navigation").label("Sidebar").w(self.width).h_full().padding(t.space.at(2.0)).gap(t.space.at(4.0)), &t.mat.slab.base);
        if let Some(h) = self.header {
            c = c.child(row().w_full().px(t.space.at(2.0)).h(t.control.lg).center_y().child(h));
        }
        for (label, items) in self.groups {
            let mut g = col().w_full().gap(1.0);
            if let Some(l) = label {
                g = g.child(row().h(24.0).px(t.space.at(2.0)).center_y().child(text(l).font(weight(t.ty.xs, t.ty.medium)).color(t.ink_faint())));
            }
            for i in items {
                g = g.child(i.build(t));
            }
            c = c.child(g);
        }
        if let Some(f) = self.footer {
            c = c.child(rune::spacer()).child(row().w_full().px(t.space.at(2.0)).child(f));
        }
        c
    }
}
