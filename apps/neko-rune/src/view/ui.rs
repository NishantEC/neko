//! The pieces every full view is made of (DESIGN.md §8): the page with its one header line, a quiet
//! group heading, a setting row (label at the left, its control at the right, help in a tooltip),
//! switches, selects and text fields. The pages compose these and draw no chrome of their own.

use rune::{El, ScrollSpec, TextEdit, col, el, row, text};
use rune_kit::{Select, Switch, TextArea, TextInput};

use super::{COLUMN, band};
use crate::app::App;
use crate::theme::Neko;

/// A full view: the band that drags the window, then one centred column (880 wide, 32 at the
/// sides) that scrolls. The header is the title (20, semibold) with a faint count beside it and, at
/// the right, at most the page's own controls. No subtitle, no sentence under it.
pub fn page(t: &Neko, key: &str, title: &str, count: Option<usize>, trailing: Vec<El>, body: El) -> El {
    let mut head = row().w_full().gap(8.0).center_y().min_h(32.0).child(text(title.to_owned()).key("title").font(t.ty.title).color(t.text.base).role("heading"));
    if let Some(n) = count {
        head = head.child(text(n.to_string()).key("page-count").font(t.ty.row).color(t.text.faint));
    }
    head = head.child(el().grow(1.0));
    for e in trailing {
        head = head.child(e);
    }
    let column = col().key(format!("full-{key}")).w_full().max_w(COLUMN).px(32.0).pt(8.0).pb(40.0).gap(20.0).child(head).child(body);
    col().w_full().h_full().child(band(t)).child(col().key("page-scroll").w_full().grow(1.0).min_h(0.0).clip_rounded(0.0).scroll(ScrollSpec { stick_bottom: false }).child(row().w_full().justify_center().child(column)))
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
        .wrap()
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

/// A switch that says what it does to assistive technology and shows only its state.
pub fn switch(t: &Neko, key: &str, label: &str, on: bool, f: impl Fn(&mut App) + 'static) -> El {
    Switch::new(on).label(label).hide_label().on_toggle(f).build(&t.kit).key(key.to_owned())
}

/// A switch that cannot be pressed yet (what it changes has not loaded).
pub fn switch_when(t: &Neko, key: &str, label: &str, on: bool, enabled: bool, f: impl Fn(&mut App) + 'static) -> El {
    Switch::new(on).label(label).hide_label().disabled(!enabled).on_toggle(f).build(&t.kit).key(key.to_owned())
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

/// A one-line text field.
pub fn input(s: &App, t: &Neko, key: &str, label: &str, placeholder: &str, width: f32, submit: impl Fn(&mut App) + 'static) -> El {
    TextInput::new(edit_of(s, key))
        .label(label)
        .placeholder(placeholder)
        .width(Some(width))
        .on_edit(editing(key.to_owned()))
        .on_submit(move |s: &mut App, _| submit(s))
        .build(&t.kit)
        .key(key.to_owned())
        .shrink()
        .max_w(320.0)
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
