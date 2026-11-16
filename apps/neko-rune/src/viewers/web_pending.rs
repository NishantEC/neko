//! The placeholder for kinds that need a web view (HTML pages, Excalidraw sketches): they open in
//! the panel in the next update (P3b replaces this viewer). Until then the web UI shows them.

use rune::{El, col, row, text};

use super::{ViewerInput, ViewerState};
use crate::app::App;
use crate::theme::theme;

pub fn build(input: &ViewerInput, state: &mut ViewerState) -> El {
    let t = theme(input.theme_dark);
    let route = state.route.clone();
    let open = row()
        .key("web-pending-open")
        .role("button")
        .label("Open in web UI")
        .focusable(true)
        .h(28.0)
        .px(12.0)
        .radius(t.radius.control)
        .center()
        .fill(t.surface.active)
        .hover(t.surface.selected)
        .focus_ring(2.0, 2.0, t.kit.c.border_focus)
        .on_press(move |s: &mut App| s.open_in_web(route.as_deref()))
        .child(text("Open in web UI").font(t.ty.row).color(t.text.base).a11y_hidden());
    col().key("viewer-web-pending").w_full().grow(1.0).min_h(0.0).center().gap(12.0).px(24.0)
        .child(text(input.title.to_owned()).font(rune::TextStyle { weight: 500.0, ..t.ty.row }).color(t.text.base).truncate())
        .child(text("Opens here in the next update").font(t.ty.meta).color(t.text.muted))
        .child(open)
}
