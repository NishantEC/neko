//! The summoned panel: input row, ranked result rows, footer — built to
//! `data/neko-design/report.md` §2's frozen anatomy and §1's token table.
//!
//! One v1 simplification worth flagging explicitly (not answered by the
//! plan or the design report, so decided per the brief's "keep the summon
//! path fast and the scope small" instruction): the window does **not**
//! resize per keystroke the way Raycast's own does. `Window::resize`
//! exists in GPUI, but its resize-anchor behavior on a borderless
//! always-on-top popup window was untested territory this task had no safe
//! way to verify visually before committing to it on the hotkey-latency
//! critical path. Instead the window is one fixed size (680×448) and the
//! footer is pinned to its true bottom with a flex-grow content area, so
//! short result lists just leave quiet space above the footer rather than
//! the window itself growing/shrinking. Correct, on-brief, and zero risk to
//! the summon-latency budget; real dynamic resizing is a follow-up.

use std::path::PathBuf;

use gpui::{
    AnyElement, App, Context, Entity, FocusHandle, Focusable, Render, SharedString, Window,
    actions, div, img, prelude::*, px, rgba,
};
use neko_client::NekoClient;
use neko_protocol::{ClipboardContentKind, Request, ResultKind, Response, SearchItem};

use crate::text_field::TextField;
use crate::theme;

actions!(panel, [SelectNext, SelectPrevious, Confirm]);

const RESULT_LIMIT: usize = 8;
pub const CONTENT_AREA_MIN_HEIGHT_PX: f32 = theme::RESULT_ROW_HEIGHT_PX * RESULT_LIMIT as f32;
pub const PANEL_HEIGHT_PX: f32 =
    theme::INPUT_ROW_HEIGHT_PX + CONTENT_AREA_MIN_HEIGHT_PX + theme::FOOTER_HEIGHT_PX;

pub struct Root {
    text_field: Entity<TextField>,
    client: NekoClient,
    results: Vec<SearchItem>,
    selected: usize,
    generation: u64,
}

impl Root {
    pub fn new(client: NekoClient, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| {
            let text_field = TextField::new(cx);
            cx.observe(&text_field, |root: &mut Root, _field, cx| {
                root.run_search(cx);
            })
            .detach();
            let mut root = Self {
                text_field,
                client,
                results: Vec::new(),
                selected: 0,
                generation: 0,
            };
            root.run_search(cx);
            root
        })
    }

    /// Called right before the window is activated on a summon, so every
    /// summon starts from a clean query rather than whatever was last
    /// typed (matches Raycast's own behavior, confirmed live per the
    /// design report's evidence log).
    pub fn reset_for_summon(&mut self, cx: &mut Context<Self>) {
        self.text_field.update(cx, |field, cx| field.clear(cx));
        self.results.clear();
        self.selected = 0;
        self.run_search(cx);
    }

    fn run_search(&mut self, cx: &mut Context<Self>) {
        self.generation += 1;
        let generation = self.generation;
        let query = self.text_field.read(cx).content().to_string();
        let client = self.client.clone();
        cx.spawn(async move |this, cx| {
            let response = client
                .request(Request::Search {
                    query,
                    limit: RESULT_LIMIT,
                })
                .await;
            let Ok(Response::SearchResults { items }) = response else {
                return;
            };
            let _ = this.update(cx, |root, cx| {
                if root.generation == generation {
                    root.results = fit_within_budget(items, CONTENT_AREA_MIN_HEIGHT_PX);
                    root.selected = 0;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn select_next(&mut self, _: &SelectNext, _window: &mut Window, cx: &mut Context<Self>) {
        if !self.results.is_empty() {
            self.selected = (self.selected + 1).min(self.results.len() - 1);
            cx.notify();
        }
    }

    fn select_previous(&mut self, _: &SelectPrevious, _window: &mut Window, cx: &mut Context<Self>) {
        self.selected = self.selected.saturating_sub(1);
        cx.notify();
    }

    fn confirm(&mut self, _: &Confirm, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(item) = self.results.get(self.selected).cloned() else {
            return;
        };
        // "Paste" here means "make this the system pasteboard's contents
        // again" — not a synthesized ⌘V into whatever regains focus after
        // neko hides. See `neko_protocol::Request::Paste`'s doc comment.
        let request = match item.kind {
            ResultKind::App => Request::Launch { id: item.id },
            ResultKind::Clipboard => Request::Paste { id: item.id },
        };
        let client = self.client.clone();
        cx.spawn(async move |_this, cx| {
            let _ = client.request(request).await;
            let _ = cx.update(|cx| cx.hide());
        })
        .detach();
    }
}

impl Focusable for Root {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.text_field.focus_handle(cx)
    }
}

impl Render for Root {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let query_is_empty = self.text_field.read(cx).content().is_empty();
        div()
            .key_context("Panel")
            .on_action(cx.listener(Self::select_next))
            .on_action(cx.listener(Self::select_previous))
            .on_action(cx.listener(Self::confirm))
            .flex()
            .flex_col()
            .w(px(theme::PANEL_WIDTH_PX))
            .h(px(PANEL_HEIGHT_PX))
            .bg(theme::SURFACE_PANEL)
            .rounded(px(theme::PANEL_RADIUS_PX))
            .shadow_lg()
            .overflow_hidden()
            .child(self.render_input_row())
            .child(self.render_content_area(query_is_empty))
            .child(self.render_footer())
    }
}

impl Root {
    fn render_input_row(&self) -> impl IntoElement {
        div()
            .flex()
            .items_center()
            .flex_shrink_0()
            .h(px(theme::INPUT_ROW_HEIGHT_PX))
            .px_4()
            .gap_3()
            .text_color(theme::TEXT_PRIMARY)
            .text_size(px(18.))
            .child(search_glyph())
            .child(div().flex_1().child(self.text_field.clone()))
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(theme::TEXT_TERTIARY)
                    .child("esc"),
            )
    }

    fn render_content_area(&self, query_is_empty: bool) -> impl IntoElement {
        let mut container = div().flex().flex_col().flex_1().min_h(px(0.)).overflow_hidden();

        if self.results.is_empty() {
            return container.child(render_empty_state(query_is_empty));
        }

        // A header per contiguous run of the same `kind` — apps and
        // clipboard entries each get their own section (design report
        // screen 11: "one query, two result types, same list"). The daemon
        // already emits apps before clipboard, so this walks the list once
        // rather than sorting or grouping client-side.
        let mut current_section: Option<ResultKind> = None;
        for (idx, item) in self.results.iter().enumerate() {
            if current_section != Some(item.kind) {
                container = container.child(section_header(section_label(item.kind)));
                current_section = Some(item.kind);
            }
            container = container.child(self.render_row(idx, item));
        }
        container
    }

    fn render_row(&self, idx: usize, item: &SearchItem) -> impl IntoElement {
        let selected = idx == self.selected;
        let title_color = theme::TEXT_PRIMARY;
        let subtitle_color = if selected {
            theme::TEXT_TERTIARY_ON_SELECTED
        } else {
            theme::TEXT_TERTIARY
        };

        let icon: AnyElement = match (item.kind, item.icon_path.as_deref()) {
            // Clipboard rows have no per-entry icon (no favicon fetching in
            // this slice) — the content-type glyph fills the slot instead
            // of an empty placeholder square, matching design screen 12's
            // per-row type glyph.
            (ResultKind::Clipboard, _) => {
                content_kind_glyph(item.content_kind.unwrap_or(ClipboardContentKind::Text))
            }
            (_, Some(path)) => img(PathBuf::from(path))
                .w(px(theme::ROW_ICON_PX))
                .h(px(theme::ROW_ICON_PX))
                .rounded(px(4.))
                .into_any_element(),
            (_, None) => div()
                .w(px(theme::ROW_ICON_PX))
                .h(px(theme::ROW_ICON_PX))
                .rounded(px(4.))
                .bg(rgba(0xffffff14))
                .into_any_element(),
        };

        div()
            .id(("result-row", idx))
            .flex()
            .items_center()
            .flex_shrink_0()
            .h(px(theme::RESULT_ROW_HEIGHT_PX))
            .px_2()
            .gap_3()
            .when(selected, |row| {
                row.bg(theme::SURFACE_SELECTED).rounded(px(theme::ROW_RADIUS_PX))
            })
            .child(icon)
            .child(
                div()
                    .flex_1()
                    .flex()
                    .items_baseline()
                    .gap_2()
                    .overflow_hidden()
                    .child(
                        div()
                            .text_size(px(14.))
                            .text_color(title_color)
                            .truncate()
                            .child(SharedString::from(item.title.clone())),
                    )
                    .children(item.subtitle.clone().map(|subtitle| {
                        div()
                            .text_size(px(12.))
                            .text_color(subtitle_color)
                            .truncate()
                            .child(SharedString::from(subtitle))
                    })),
            )
            .children(item.content_kind.map(|kind| {
                div()
                    .flex_shrink_0()
                    .px(px(6.))
                    .py(px(2.))
                    .rounded(px(4.))
                    .bg(rgba(0xffffff0f))
                    .text_size(px(10.))
                    .text_color(theme::TEXT_TERTIARY)
                    .child(content_kind_tag(kind))
            }))
            .children(item.accessory.clone().map(|accessory| {
                div()
                    .flex_shrink_0()
                    .text_size(px(11.))
                    .text_color(subtitle_color)
                    .child(SharedString::from(accessory))
            }))
    }

    fn render_footer(&self) -> impl IntoElement {
        let selected_item = self.results.get(self.selected);
        // The primary action's verb matches what enter actually does — the
        // design's dedicated clipboard screen (screen 12) uses "Paste" for
        // exactly this reason.
        let primary_action = match selected_item.map(|item| item.kind) {
            Some(ResultKind::Clipboard) => "Paste  ↵",
            _ => "Open  ↵",
        };
        div()
            .flex()
            .items_center()
            .justify_between()
            .flex_shrink_0()
            .h(px(theme::FOOTER_HEIGHT_PX))
            .px_4()
            .border_t_1()
            .border_color(rgba(0xffffff0f))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_size(px(12.))
                    .text_color(theme::TEXT_TERTIARY)
                    .children(selected_item.map(|item| SharedString::from(item.title.clone()))),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .text_size(px(12.))
                    .text_color(theme::TEXT_SECONDARY)
                    .child(primary_action)
                    .child(div().w(px(1.)).h(px(14.)).bg(rgba(0xffffff1f)))
                    .child("Actions  ⌘K"),
            )
    }
}

/// Trims `results` to the prefix that renders within `budget_px` without
/// ever showing a partial row or a section header with no row beneath it.
///
/// The panel is a fixed-size window (see this module's own doc comment) —
/// there's no scroll machinery and dynamic resize is an explicit non-goal —
/// so unlike a scrollable list, anything that doesn't fit has to be dropped
/// here rather than merely clipped by `overflow_hidden()` on the content
/// container, which would otherwise render the last row half-visible right
/// against the footer. `results` is assumed already grouped by `kind`
/// (the daemon emits apps before clipboard) — a header's cost is only
/// charged on the first row of each contiguous run.
fn fit_within_budget(results: Vec<SearchItem>, budget_px: f32) -> Vec<SearchItem> {
    let mut used_px = 0.0f32;
    let mut kept = 0usize;
    let mut current_kind = None;
    for item in &results {
        let header_cost = if current_kind != Some(item.kind) {
            theme::SECTION_HEADER_HEIGHT_PX
        } else {
            0.0
        };
        let cost = header_cost + theme::RESULT_ROW_HEIGHT_PX;
        if used_px + cost > budget_px {
            break;
        }
        used_px += cost;
        current_kind = Some(item.kind);
        kept += 1;
    }
    results.into_iter().take(kept).collect()
}

fn render_empty_state(query_is_empty: bool) -> impl IntoElement {
    // Step 10 of onboarding's own sequence: "a one-line tip stands in for a
    // blank list" — the same principle applies to steady-state empty
    // results, not just first run. Copy matches that screen's own tip, now
    // that the empty state covers both result types: "Type an app name, or
    // paste history from your clipboard."
    let message: SharedString = if query_is_empty {
        "Type an app name, or paste history from your clipboard.".into()
    } else {
        "No matching results".into()
    };
    div()
        .flex()
        .items_center()
        .h(px(theme::RESULT_ROW_HEIGHT_PX))
        .px_4()
        .text_size(px(13.))
        .text_color(theme::TEXT_TERTIARY)
        .child(message)
}

fn section_header(label: &'static str) -> impl IntoElement {
    div()
        .flex_shrink_0()
        .h(px(theme::SECTION_HEADER_HEIGHT_PX))
        .flex()
        .items_center()
        .px_2()
        .text_size(px(11.))
        .text_color(theme::TEXT_TERTIARY)
        .child(label)
}

fn section_label(kind: ResultKind) -> &'static str {
    match kind {
        ResultKind::App => "Applications",
        ResultKind::Clipboard => "Clipboard",
    }
}

/// GPUI has no CSS `text-transform`, so the design's uppercase type-tag
/// badge (`LINK`, `TEXT`) is upper-cased here rather than at the source.
fn content_kind_tag(kind: ClipboardContentKind) -> &'static str {
    match kind {
        ClipboardContentKind::Text => "TEXT",
        ClipboardContentKind::Link => "LINK",
    }
}

/// A small hand-painted glyph for the clipboard row-icon slot, in the same
/// spirit as `search_glyph` below (a painted shape composed from plain
/// divs, not a font glyph or an SVG asset — this codebase has no bundled
/// icon-asset pipeline, and a Unicode symbol is exactly what the design
/// report's §6 finding on unreliable glyph rendering in GPUI already ruled
/// out for the search icon).
fn content_kind_glyph(kind: ClipboardContentKind) -> AnyElement {
    let slot = div().w(px(theme::ROW_ICON_PX)).h(px(theme::ROW_ICON_PX)).flex_shrink_0();
    match kind {
        // Three stacked bars of decreasing width — a plain "lines of text"
        // mark.
        ClipboardContentKind::Text => slot
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(2.))
            .child(div().w(px(12.)).h(px(1.5)).rounded(px(1.)).bg(theme::TEXT_TERTIARY))
            .child(div().w(px(9.)).h(px(1.5)).rounded(px(1.)).bg(theme::TEXT_TERTIARY))
            .child(div().w(px(12.)).h(px(1.5)).rounded(px(1.)).bg(theme::TEXT_TERTIARY))
            .into_any_element(),
        // Two overlapping rounded-square rings on a diagonal — a chain-link
        // mark.
        ClipboardContentKind::Link => slot
            .relative()
            .child(
                div()
                    .absolute()
                    .top(px(3.))
                    .left(px(2.))
                    .w(px(11.))
                    .h(px(11.))
                    .rounded(px(3.))
                    .border_2()
                    .border_color(theme::TEXT_TERTIARY),
            )
            .child(
                div()
                    .absolute()
                    .bottom(px(3.))
                    .right(px(2.))
                    .w(px(11.))
                    .h(px(11.))
                    .rounded(px(3.))
                    .border_2()
                    .border_color(theme::TEXT_TERTIARY),
            )
            .into_any_element(),
    }
}

fn search_glyph() -> impl IntoElement {
    // A hand-drawn glyph rather than a font character: the design report's
    // §6 finding that the ⌥ modifier glyph has no reliable font rendering
    // in GPUI applies just as much to a search icon, so this is a small
    // painted shape (a circle + a diagonal stroke), not a Unicode symbol
    // trusted to be in the system font.
    div()
        .w(px(14.))
        .h(px(14.))
        .rounded_full()
        .border_2()
        .border_color(theme::TEXT_TERTIARY)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(kind: ResultKind) -> SearchItem {
        SearchItem {
            id: "x".into(),
            kind,
            title: "x".into(),
            subtitle: None,
            icon_path: None,
            content_kind: None,
            accessory: None,
        }
    }

    #[test]
    fn six_apps_and_a_clipboard_row_fit_within_budget_without_a_partial_row() {
        // The exact shape that produced a half-clipped row: a query
        // matching 6 apps plus 1 clipboard entry needs 2 headers + 7 rows
        // (56 + 280 = 336px) against the fixed 320px content budget.
        let mut results: Vec<SearchItem> = (0..6).map(|_| item(ResultKind::App)).collect();
        results.push(item(ResultKind::Clipboard));

        let fitted = fit_within_budget(results, CONTENT_AREA_MIN_HEIGHT_PX);

        // The clipboard section's header would push total height past the
        // budget, so it — and its one row — are dropped entirely rather
        // than rendering a header with no row, or a partially visible row.
        assert_eq!(fitted.len(), 6);
        assert!(fitted.iter().all(|i| i.kind == ResultKind::App));
    }

    #[test]
    fn a_full_page_of_a_single_section_never_exceeds_the_budget() {
        let results: Vec<SearchItem> = (0..RESULT_LIMIT).map(|_| item(ResultKind::App)).collect();
        let fitted = fit_within_budget(results, CONTENT_AREA_MIN_HEIGHT_PX);
        let total_height = theme::SECTION_HEADER_HEIGHT_PX + fitted.len() as f32 * theme::RESULT_ROW_HEIGHT_PX;
        assert!(
            total_height <= CONTENT_AREA_MIN_HEIGHT_PX,
            "{} rows + a header ({total_height}px) must fit in {CONTENT_AREA_MIN_HEIGHT_PX}px",
            fitted.len()
        );
    }

    #[test]
    fn results_that_already_fit_are_returned_unchanged() {
        let results = vec![item(ResultKind::App), item(ResultKind::Clipboard)];
        let fitted = fit_within_budget(results.clone(), CONTENT_AREA_MIN_HEIGHT_PX);
        assert_eq!(fitted, results);
    }

    #[test]
    fn empty_results_stay_empty() {
        assert_eq!(fit_within_budget(Vec::new(), CONTENT_AREA_MIN_HEIGHT_PX), Vec::new());
    }
}
