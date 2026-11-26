//! rune-ui registry: sidebar 0.10.0 Copied into your app by `rune-ui add sidebar`; yours to change.
//!
//! An app's side column: a header, groups of rows under quiet labels, a footer. A row is an icon, its words and an optional count;
//! it washes on hover and takes the selected wash and the first ink when it is the current page. The column sits on the slab colour.
//! A row can lead with a status mark instead of its icon (`lead`: a spinner, a coloured dot, a warning), on the icon's own box, and can
//! say it is one row of a list (`list_row`: an `option` with its place in the list and whether it is selected; put the rows in a
//! `listbox` with `focus_group`, and Return opens a row as a press does).
//!
//!
//! **A two-line row** leads with an image or element of your own (`image`: a portrait, a file icon) and reads as a title over a quiet
//! second line (`subtitle`), a count and a trailing element (`trail`: a pill, a button), and can sit raised when it is the current
//! one (`selected_style(Selected::Raised)`: the raised material, with its own focus ring). A row can be a button instead of a link
//! (`button`), and a count is read out ("Count 3", or the words given to `count_label`).
//!
//! ```ignore
//! SidebarItem::new("t1", "Fix the login test").image(portrait).subtitle("Working · 2 min").count(3).trail(pill)
//!     .selected_style(Selected::Raised).selected(true).list_row(1, 4)
//! Sidebar::new("nav").header(brand).group(Some("Workspace"), vec![SidebarItem::new("home", "Home").icon(&icons::HOME).selected(true)])
//!     .footer(account).build(t)
//! ```

use std::rc::Rc;

use rune::{El, Icon, col, row, text};
use rune_ui_style::{tabular, weight};

use super::base::{Press, Shape, State, States, Theme, glyph, paint, paint_state};

/// How the current row looks.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Selected {
    /// The selected wash and the first ink.
    #[default]
    Wash,
    /// The raised material, a pebble on the slab, with the material's own focus ring.
    Raised,
}

/// One row.
pub struct SidebarItem<S: 'static> {
    key: String,
    label: String,
    icon: Option<&'static Icon>,
    lead: Option<El>,
    image: Option<El>,
    subtitle: Option<String>,
    trail: Option<El>,
    selected_style: Selected,
    button: bool,
    count_label: Option<String>,
    count: Option<usize>,
    list: Option<(usize, usize)>,
    description: Option<String>,
    selected: bool,
    pinned: Option<State>,
    press: Option<Press<S>>,
}

impl<S: 'static> SidebarItem<S> {
    pub fn new(key: impl Into<String>, label: impl Into<String>) -> SidebarItem<S> {
        SidebarItem { key: key.into(), label: label.into(), icon: None, lead: None, image: None, subtitle: None, trail: None, selected_style: Selected::Wash, button: false, count_label: None, count: None, list: None, description: None, selected: false, pinned: None, press: None }
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

    /// A leading image or element (a portrait, a file icon) in place of the icon, vertically centred, at the size you built it. It is
    /// hidden from assistive technology: the row's name says what it shows. With an image the row is two lines high.
    pub fn image(mut self, e: El) -> Self {
        self.image = Some(e);
        self
    }

    /// A quiet second line under the title (a state, a time, a path), truncated. The row becomes two lines high, and assistive
    /// technology reads it after the name.
    pub fn subtitle(mut self, words: impl Into<String>) -> Self {
        self.subtitle = Some(words.into());
        self
    }

    /// An element at the row's end after the count (a pill, a small button). It keeps its own role, name and Tab stop.
    pub fn trail(mut self, e: El) -> Self {
        self.trail = Some(e);
        self
    }

    /// How the row looks while it is the current one: the wash (the default) or raised.
    pub fn selected_style(mut self, s: Selected) -> Self {
        self.selected_style = s;
        self
    }

    /// The row acts instead of going somewhere: role `button` in place of `link` ("New task"). A list row stays an option.
    pub fn button(mut self, on: bool) -> Self {
        self.button = on;
        self
    }

    /// What assistive technology reads for the count ("3 unread"); without it, "Count 3".
    pub fn count_label(mut self, words: impl Into<String>) -> Self {
        self.count_label = Some(words.into());
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
        let raised = self.selected && self.selected_style == Selected::Raised;
        if raised {
            st.selected = false;
        }
        let two_lines = self.image.is_some() || self.subtitle.is_some();
        let ink = if self.selected { t.ink() } else { t.ink_muted() };
        let role = if self.list.is_some() { "option" } else if self.button { "button" } else { "link" };
        let height = if two_lines { t.metrics.row + t.space.at(4.0) } else { t.metrics.row };
        let mut r = row().key(self.key).role(role).label(self.label.clone()).focusable(true).w_full().h(height).px(t.metrics.gap).gap(t.metrics.gap).center_y().radius(t.radius.md).round_as(t);
        match self.list {
            Some((position, size)) => r = r.position_in_set(position, size).focus_selected(self.selected).press_on_return(),
            None if self.selected && !self.button => r = r.checked(true),
            None => {}
        }
        // What assistive technology reads after the name: the app's words, the second line, the count.
        let mut said: Vec<String> = Vec::new();
        said.extend(self.description);
        said.extend(self.subtitle.clone());
        if let Some(n) = self.count {
            said.push(self.count_label.clone().unwrap_or_else(|| format!("Count {n}")));
        }
        if !said.is_empty() {
            r = r.description(said.join(", "));
        }
        r = paint_state(r, if raised { &t.mat.raised } else { &t.mat.row }, st);
        let icon = t.control.icon_lg;
        if let Some(e) = self.image {
            r = r.child(row().center().a11y_hidden().child(e));
        } else if let Some(e) = self.lead {
            r = r.child(row().size(icon, icon).center().a11y_hidden().child(e));
        } else if let Some(i) = self.icon {
            r = r.child(glyph(i, icon, ink));
        }
        let title = text(self.label).font(weight(t.ty.base, if self.selected { if two_lines { t.ty.semibold } else { t.ty.medium } } else { t.ty.regular })).color(ink).truncate().min_w(0.0).a11y_hidden();
        r = match self.subtitle {
            Some(sub) => r.child(col().grow(1.0).shrink().min_w(0.0).child(title).child(text(sub).font(t.ty.sm).color(t.ink_faint()).truncate().min_w(0.0).a11y_hidden())),
            None => r.child(title.grow(1.0)),
        };
        if let Some(n) = self.count {
            r = r.child(text(n.to_string()).font(tabular(t.ty.sm)).color(t.ink_faint()).a11y_hidden());
        }
        if let Some(e) = self.trail {
            r = r.child(e);
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
