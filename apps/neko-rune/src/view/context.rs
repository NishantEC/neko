//! The context panel: what the session produces, one viewer at a time (plugin slot (a), viewers
//! by kind). The header is a segmented control over Diff, Files, Terminal, Browser (live), Screen
//! (live) and Canvas. In P0 each is a placeholder; the panel is closed until a session has
//! something to show, and the toggle in the session's top line opens it.

use rune::{El, col, row, text};

use super::{band, icon_button, placeholder};
use crate::app::{App, ContextKind};
use crate::icons;
use crate::theme::Neko;

pub fn panel(s: &App, t: &Neko) -> El {
    // The kinds as small text buttons (the kit's segmented control is too wide for a 360-point
    // panel); the open one has the soft fill.
    let mut picker = row().key("context-picker").role("tablist").label("Context").gap(2.0).center_y().min_w(0.0);
    for kind in ContextKind::ALL {
        let active = s.context == kind;
        picker = picker.child(
            row()
                .key(format!("context-{}", kind.title().to_lowercase()))
                .role("tab")
                .label(kind.title())
                .checked(active)
                .focusable(true)
                .h(26.0)
                .px(6.0)
                .center()
                .radius(t.radius.control)
                .fill(if active { t.surface.active } else { rune::Color::TRANSPARENT })
                .hover(t.surface.hover)
                .focus_ring(2.0, 0.0, t.kit.c.border_focus)
                .on_press(move |s: &mut App| s.context = kind)
                .child(text(kind.title()).font(if active { rune::TextStyle { weight: 500.0, ..t.ty.meta } } else { t.ty.meta }).color(if active { t.text.base } else { t.text.muted }).a11y_hidden()),
        );
    }
    let lines: Vec<String> = match s.context {
        ContextKind::Browser => vec!["The agent's browser shows here, live.".to_owned(), "Live views arrive in P3.".to_owned()],
        ContextKind::Screen => vec!["The agent's screen shows here, live.".to_owned(), "Live views arrive in P3.".to_owned()],
        kind => vec![format!("{} appears here when the session produces one.", kind.title())],
    };
    let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
    col().key("context").h_full().min_w(0.0).my(8.0).mx(2.0).radius(t.radius.window).fill(t.surface.card).child(
        col().w_full().h_full().child(
            band(t).px(8.0).gap(4.0).child(row().grow(1.0).min_w(0.0).child(picker)).child(icon_button(&icons::CLOSE_SMALL, "Hide context", "context-close", t, |s: &mut App| s.layout.context.set_open(false))),
        ).child(placeholder(&refs, t)),
    )
}
