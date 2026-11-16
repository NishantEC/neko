//! The shortcut overlay (`?`): the web's list in two columns, each group under its heading, each line a label and its keys.

use rune::{El, TextStyle, col, row, text};
use rune_kit::Dialog;

use crate::app::App;
use crate::shortcuts::{Group, columns};
use crate::theme::Neko;

fn key_cap(t: &Neko, key: &str) -> El {
    row()
        .h(20.0)
        .min_w(20.0)
        .px(5.0)
        .center()
        .radius(5.0)
        .fill(t.text.base.alpha(0.06))
        .border(1.0, t.surface.field_border)
        .child(text(key.to_owned()).font(TextStyle { weight: 500.0, ..t.ty.hint }).color(t.text.muted).a11y_hidden())
}

fn group(t: &Neko, g: &Group) -> El {
    let mut c = col().key(format!("shortcuts-{}", g.title)).role("group").label(g.title).w_full().gap(8.0).child(text(g.title.to_owned()).font(TextStyle { weight: 500.0, ..t.ty.hint }).color(t.text.faint));
    for r in g.rows {
        let mut keys = row().gap(4.0).center_y();
        for k in r.keys {
            keys = keys.child(key_cap(t, k));
        }
        c = c.child(row().w_full().gap(12.0).center_y().min_h(24.0).child(text(r.label).font(t.ty.row).color(t.text.base).wrap().grow(1.0).min_w(0.0)).child(keys));
    }
    c
}

pub fn mount(s: &App, t: &Neko, layer: &mut Vec<El>) {
    if !s.shortcuts_open {
        return;
    }
    let [left, right] = columns();
    let column = |list: Vec<&Group>, key: &str| {
        let mut c = col().key(key.to_owned()).grow(1.0).min_w(0.0).gap(24.0);
        for g in list {
            c = c.child(group(t, g));
        }
        c
    };
    let body = row().key("shortcuts-columns").w_full().gap(40.0).items_start().child(column(left, "shortcuts-left")).child(column(right, "shortcuts-right"));
    Dialog::new("shortcuts", "Keyboard shortcuts")
        .open(true)
        .description("Everything here works without the mouse. Press ? again to close.")
        .width(720.0)
        .fit()
        .child(body)
        .on_close(|s: &mut App| s.shortcuts_open = false)
        .build(&t.kit, layer);
}
