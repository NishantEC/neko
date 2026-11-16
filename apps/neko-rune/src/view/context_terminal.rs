//! The Terminal tab: a read-only log of the session's shell commands. Each is a 32 point header row (`$` and the
//! command in 12.5 mono, cut in the middle when it is long, and an exit chip at the right: "0" muted, any other coral)
//! and its output drawn through `vt100` (12 mono), in the theme's own colours, 12 lines of it until "Show all N lines".
//! The person's home folder reads `~`. Newest at the
//! bottom; the log follows new output unless the person has scrolled up. Only the rows on screen are built.

use std::borrow::Cow;

use rune::{El, GrowthSpec, ScrollSpec, col, row, text};
use rune::Color;

use super::placeholder;
use crate::app::App;
use crate::panel::TermRow;
use crate::terminal::{middle, spans};
use crate::theme::Neko;

pub fn tab(s: &App, t: &Neko) -> El {
    let Some((entries, rows, dropped)) = s.terminal_rows() else { return placeholder(&["Nothing to show yet."], t) };
    if rows.is_empty() {
        return placeholder(&["No commands yet."], t);
    }
    // How many characters of a command fit: the panel's width less the padding, the `$` and the chip, at the mono
    // face's advance (about 0.6 em, 7.5 points at 12.5).
    let width = if s.panel.expanded { 880.0 } else { s.layout.context.shown_width() };
    let fit = (((width - 24.0 - 14.0 - 8.0 - 44.0) / 7.5).floor() as usize).max(16);
    let version = (entries.len(), rows.len(), s.detail().map(|d| d.task.events.len()), fit, s.panel.term_rev);
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
                            let shown = middle(&entry.command, fit);
                            let mut line = row()
                                .key(format!("term-head-{e}"))
                                .w_full()
                                .h(32.0)
                                .mt(if i == 0 { 0.0 } else { 8.0 })
                                .gap(8.0)
                                .center_y()
                                .child(text("$").font(t.ty.mono_meta).color(t.text.faint).a11y_hidden())
                                .child(text(shown).font(t.ty.mono_meta).color(t.text.base).truncate().grow(1.0).min_w(0.0).help(entry.command.clone()));
                            if let Some(code) = entry.exit {
                                let ink = if code == 0 { t.text.muted } else { t.meaning.coral };
                                let fill = if code == 0 { t.text.base.alpha(0.08) } else { t.meaning.coral.alpha(0.12) };
                                line = line.child(row().key(format!("term-exit-{e}")).h(20.0).min_w(20.0).px(6.0).center().radius(6.0).fill(fill).child(text(code.to_string()).font(t.ty.mono).color(ink).a11y_hidden()));
                            }
                            line
                        }
                        TermRow::Line(e, l) => {
                            let (content, marks) = spans(&entries[e].lines[l], t);
                            text(if content.is_empty() { " ".to_owned() } else { content }).font(t.ty.mono).color(t.terminal_fg()).spans(marks).wrap().selectable(true)
                        }
                        TermRow::Gap(e) => text(format!("…{} lines hidden", entries[e].hidden)).font(t.ty.id).color(t.text.faint).py(2.0),
                        TermRow::More(e) => {
                            let entry = &entries[e];
                            let id = entry.id.clone();
                            let open = entry.lines.len() > crate::terminal::COLLAPSED_LINES && rows.iter().any(|r| matches!(r, TermRow::Line(x, l) if *x == e && *l >= crate::terminal::COLLAPSED_LINES));
                            let words = if open { "Show fewer".to_owned() } else { format!("Show all {} lines", entry.total()) };
                            row()
                                .key(format!("term-more-{e}"))
                                .role("button")
                                .label(words.clone())
                                .focusable(true)
                                .h(24.0)
                                .px(6.0)
                                .ml(-6.0)
                                .mt(2.0)
                                .center_y()
                                .radius(t.radius.control)
                                .hover(Color::WHITE.alpha(0.06))
                                .focus_ring(2.0, 2.0, t.kit.c.border_focus)
                                .on_press(move |s: &mut App| s.toggle_term(&id))
                                .child(text(words).font(t.ty.id).color(t.text.muted).a11y_hidden())
                        }
                    }
                },
            )
            .virtual_rows_version(version)
    };
    // The scroll area sticks to its end while the person has not scrolled away from it.
    col().key("terminal").w_full().grow(1.0).min_h(0.0).clip_rounded(0.0).fill(t.terminal_bg()).scroll(ScrollSpec { stick_bottom: true }).child(col().w_full().px(12.0).py(8.0).child(list))
}
