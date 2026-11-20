//! The pieces every full view is made of (DESIGN.md §8): the page with its one header line, a quiet
//! group heading, a setting row (label at the left, its control at the right, help in a tooltip),
//! switches, selects and text fields. The pages compose these and draw no chrome of their own.

use rune::{El, ScrollSpec, TextEdit, col, el, row, text};
use rune_kit::{Select, TextArea};

use super::{COLUMN, band};
use crate::app::App;
use crate::theme::Neko;

/// A full view: the band that drags the window, then one centred column (880 wide, 32 at the
/// sides) that scrolls. The header is the title (20, semibold) with a faint count beside it and, at
/// the right, at most the page's own controls. No subtitle, no sentence under it.
pub fn page(t: &Neko, key: &str, title: &str, count: Option<usize>, trailing: Vec<El>, body: El) -> El {
    col().w_full().h_full().child(band(t)).child(scrolled(page_column(t, key, title, count, trailing, body)))
}

/// [`page`] with a count in words ("Week 47") and its own column width (the Calendar's 1400).
pub fn page_wide(t: &Neko, key: &str, title: &str, count: Option<String>, trailing: Vec<El>, body: El, max_w: f32) -> El {
    col().w_full().h_full().child(band(t)).child(scrolled(page_column_with(t, key, title, count, trailing, body, max_w)))
}

/// A page column, centred and scrolling inside the free space.
pub fn scrolled(column: El) -> El {
    col().key("page-scroll").w_full().grow(1.0).min_h(0.0).clip_rounded(0.0).scroll(ScrollSpec { stick_bottom: false }).child(row().w_full().justify_center().child(column))
}

/// The section rail of Settings and Setup: 200 wide (150 when narrow), 12 of padding (8 narrow), an optional field
/// above, then the rows in a list that scrolls when the window is short.
pub fn rail(prefix: &str, narrow: bool, search: Option<El>, rows: Vec<El>) -> El {
    let mut c = col().key(format!("{prefix}-rail")).w(if narrow { 150.0 } else { 200.0 }).h_full().px(if narrow { 8.0 } else { 12.0 }).pt(4.0).gap(0.0);
    if let Some(search) = search {
        c = c.child(search.h(32.0)).child(el().h(8.0));
    }
    let list = col().key(format!("{prefix}-rail-list")).w_full().gap(4.0).children(rows);
    c.child(col().key(format!("{prefix}-rail-scroll")).w_full().grow(1.0).min_h(0.0).clip_rounded(0.0).scroll(ScrollSpec { stick_bottom: false }).child(list))
}

/// The column of a full view: its header line (title, count, the page's own controls), then the body.
pub fn page_column(t: &Neko, key: &str, title: &str, count: Option<usize>, trailing: Vec<El>, body: El) -> El {
    page_column_with(t, key, title, count.map(|n| n.to_string()), trailing, body, COLUMN)
}

fn page_column_with(t: &Neko, key: &str, title: &str, count: Option<String>, trailing: Vec<El>, body: El, max_w: f32) -> El {
    // `PageContainer`: 24 at the sides (40 from 768), 48 above (64 from 768), 96 below; the header and the page 28 apart.
    let wide = super::webkit::page_width() >= 768.0;
    let mut head = row().w_full().gap(16.0).center_y().flex_wrap().min_h(40.0);
    let mut name = row().gap(10.0).center_y().child(text(title.to_owned()).key("title").font(rune::TextStyle { tracking: -0.41, ..super::webkit::font(t, 20.0, 600.0, 30.0) }).color(t.text.strong).role("heading"));
    if let Some(n) = count {
        name = name.child(text(n).key("page-count").font(super::webkit::tnum(super::webkit::font(t, 14.0, 400.0, 21.0))).color(t.text.muted).mt(3.0));
    }
    head = head.child(name).child(el().grow(1.0));
    for e in trailing {
        head = head.child(e);
    }
    col().key(format!("full-{key}")).w_full().max_w(max_w).px(if wide { 40.0 } else { 24.0 }).pt(if wide { 64.0 } else { 48.0 }).pb(96.0).gap(28.0).child(head).child(body)
}

/// A quiet group heading ("Standing orders", "Recent runs").
pub fn group(t: &Neko, label: &str) -> El {
    text(label.to_owned()).font(t.ty.group).color(t.text.muted).px(6.0)
}

/// A one-line muted sentence, for a page or section with nothing to show (DESIGN.md §8 rule 11).
pub fn quiet_line(t: &Neko, key: &str, line: &str) -> El {
    text(line.to_owned()).key(key.to_owned()).font(t.ty.meta).color(t.text.muted).px(6.0).wrap()
}

/// A setting row: the label at the left, its control at the right. The help is a tooltip only.
pub fn setting_row(t: &Neko, key: &str, label: &str, help: Option<&str>, control: El) -> El {
    let mut r = row()
        .key(key.to_owned())
        .w_full()
        .min_h(40.0)
        .px(6.0)
        .py(4.0)
        .gap(4.0)
        .center_y()
        // In a narrow page the control drops under its label instead of squeezing it.
        .flex_wrap()
        .radius(t.radius.row)
        .hover(t.surface.hover)
        .child(text(label.to_owned()).font(t.ty.row).color(t.text.base).grow(1.0).min_w(140.0).wrap())
        .child(control);
    if let Some(h) = help {
        r = r.help(h);
    }
    r
}

/// The note under a control that was refused, in the daemon's words.
pub fn note(s: &App, t: &Neko, key: &str) -> Option<El> {
    s.pages.notes.get(key).map(|n| text(n.clone()).key(format!("note-{key}")).font(t.ty.meta).color(t.meaning.amber).wrap().px(6.0).pb(4.0).live_region(rune::LiveRegion::Polite))
}

/// The web's switch (`ui/switch.tsx`): 46 x 28, off is the well, on is the ink. It says what it does to assistive technology and
/// shows only its state.
fn web_switch(t: &Neko, key: &str, label: &str, on: bool, enabled: bool, f: impl Fn(&mut App) + 'static) -> El {
    super::webkit::toggle_when(t, key, label, on, enabled, f)
}

/// A switch that says what it does to assistive technology and shows only its state.
pub fn switch(t: &Neko, key: &str, label: &str, on: bool, f: impl Fn(&mut App) + 'static) -> El {
    web_switch(t, key, label, on, true, f)
}

/// A switch that cannot be pressed yet (what it changes has not loaded).
pub fn switch_when(t: &Neko, key: &str, label: &str, on: bool, enabled: bool, f: impl Fn(&mut App) + 'static) -> El {
    web_switch(t, key, label, on, enabled, f)
}

/// A choice from a short list: the current option shows, a list opens under it. Options are shown
/// by their words; `choose` hears the words picked.
#[allow(clippy::too_many_arguments)]
pub fn pick(s: &App, t: &Neko, layer: &mut Vec<El>, key: &str, label: &str, options: &[String], current: &str, width: f32, choose: impl Fn(&mut App, &str) + Clone + 'static) -> El {
    let open_key = key.to_owned();
    let close_key = key.to_owned();
    let k = key.to_owned();
    Select::new(key, options)
        .current(Some(current))
        .label(label)
        .width(width)
        .open(s.pages.open_select.as_deref() == Some(key))
        .on_toggle(move |s: &mut App| s.toggle_select(&open_key))
        .on_close(move |s: &mut App| {
            if s.pages.open_select.as_deref() == Some(close_key.as_str()) {
                s.pages.open_select = None;
            }
        })
        .on_choose(move |s: &mut App, chosen: &str, _| {
            s.pages.open_select = None;
            choose(s, chosen);
        })
        .build(&t.kit, layer)
        .key(format!("pick-{k}"))
}

/// The text of a page field, or an empty box when nothing was typed yet.
pub fn edit_of<'a>(s: &'a App, key: &str) -> &'a TextEdit {
    s.pages.fields.get(key).unwrap_or(&s.pages.blank)
}

/// The accessor a text field edits through: the page's field with this key.
pub fn editing(key: String) -> impl for<'a> Fn(&'a mut App) -> &'a mut TextEdit + 'static {
    move |s: &mut App| s.pages.field(&key)
}

/// A multi-line text field of a few rows.
pub fn area(s: &App, t: &Neko, key: &str, label: &str, placeholder: &str, rows: usize) -> El {
    TextArea::new(edit_of(s, key)).label(label).placeholder(placeholder).rows(rows).on_edit(editing(key.to_owned())).build(&t.kit).key(key.to_owned()).w_full().min_w(0.0)
}

/// One entry of a row's right-click menu.
pub type MenuEntry = (String, std::rc::Rc<dyn Fn(&mut App)>);

/// A row with a right-click menu (every action of a row is also in its menu). `key` names the row for
/// the menu's open state.
pub fn with_menu(s: &App, t: &Neko, layer: &mut Vec<El>, key: &str, row: El, entries: Vec<MenuEntry>) -> El {
    if entries.is_empty() {
        return row;
    }
    let owner = key.to_owned();
    let row = row.on_secondary_press(move |s: &mut App, p, _| s.page_menu = Some((owner.clone(), p)));
    match s.page_menu.as_ref().filter(|(open, _)| open == key) {
        Some((_, at)) => {
            let mut menu = rune_kit::context_menu(format!("menu-{key}"), row, Some(*at)).on_close(|s: &mut App| s.page_menu = None);
            for (label, act) in entries {
                menu = menu.item(rune_kit::MenuItem::new(label).on_select(move |s: &mut App| {
                    s.page_menu = None;
                    act(s);
                }));
            }
            menu.build(&t.kit, layer)
        }
        None => row,
    }
}
