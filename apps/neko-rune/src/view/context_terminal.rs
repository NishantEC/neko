//! The Terminal tab: a read-only log of the session's shell commands. Each is a header line (`$ command`,
//! 12.5 mono) and its output drawn through `vt100` (12 mono), in the theme's own colours. Newest at the
//! bottom; the log follows new output unless the person has scrolled up. Only the rows on screen are built.

use std::borrow::Cow;

use rune::{El, GrowthSpec, ScrollSpec, col, row, text};

use super::placeholder;
use crate::app::App;
use crate::panel::TermRow;
use crate::terminal::spans;
use crate::theme::Neko;

pub fn tab(s: &App, t: &Neko) -> El {
    let Some((entries, rows, dropped)) = s.terminal_rows() else { return placeholder(&["Nothing to show yet."], t) };
    if rows.is_empty() {
        return placeholder(&["No commands yet."], t);
    }
    let version = (entries.len(), rows.len(), s.detail().map(|d| d.task.events.len()));
    let theme = crate::theme::theme(t.dark);
    let list = {
        let rows = rows.clone();
        col()
            .key("terminal-rows")
            .w_full()
            .growth(GrowthSpec::default())
            .virtual_rows(
                rows.len(),
                17.0,
                |_: &App, i| Cow::Owned(format!("t{i}")),
                move |_: &App, i| {
                    let t = &theme;
                    match rows[i] {
                        TermRow::Dropped => text(format!("…{dropped} earlier commands hidden")).font(t.ty.id).color(t.text.faint).pb(4.0),
                        TermRow::Header(e) => {
                            let entry = &entries[e];
                            let mut line = row().w_full().gap(8.0).pt(if i == 0 { 0.0 } else { 10.0 }).pb(2.0).center_y().child(text(format!("$ {}", entry.command)).font(t.ty.mono_meta).color(t.text.base).truncate().grow(1.0).min_w(0.0).selectable(true));
                            if let Some(code) = entry.exit.filter(|c| *c != 0) {
                                line = line.child(text(format!("exit {code}")).font(t.ty.mono).color(t.meaning.coral));
                            }
                            line
                        }
                        TermRow::Line(e, l) => {
                            let (content, marks) = spans(&entries[e].lines[l], t);
                            text(if content.is_empty() { " ".to_owned() } else { content }).font(t.ty.mono).color(t.terminal_fg()).spans(marks).wrap().selectable(true)
                        }
                        TermRow::Gap(e) => text(format!("…{} lines hidden", entries[e].hidden)).font(t.ty.id).color(t.text.faint).py(2.0),
                    }
                },
            )
            .virtual_rows_version(version)
    };
    // The scroll area sticks to its end while the person has not scrolled away from it.
    col().key("terminal").w_full().grow(1.0).min_h(0.0).clip_rounded(0.0).fill(t.terminal_bg()).scroll(ScrollSpec { stick_bottom: true }).child(col().w_full().px(12.0).py(8.0).child(list))
}
