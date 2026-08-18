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
use std::rc::Rc;

use gpui::{
    AnyElement, App, ClickEvent, Context, CursorStyle, Entity, FocusHandle, Focusable, Render,
    SharedString, Window, actions, div, img, prelude::*, px,
};
use neko_client::NekoClient;
use neko_protocol::{Glyph, Icon, Request, Response, SearchItem};

use crate::accessibility::AccessibilityChecker;
use crate::text_field::{ContentChanged, TextField};
use crate::theme;

actions!(panel, [SelectNext, SelectPrevious, Confirm]);

const RESULT_LIMIT: usize = 8;
pub const CONTENT_AREA_MIN_HEIGHT_PX: f32 = theme::RESULT_ROW_HEIGHT_PX * RESULT_LIMIT as f32;
pub const PANEL_HEIGHT_PX: f32 =
    theme::INPUT_ROW_HEIGHT_PX + CONTENT_AREA_MIN_HEIGHT_PX + theme::FOOTER_HEIGHT_PX;

pub struct Root {
    text_field: Entity<TextField>,
    client: NekoClient,
    accessibility: Rc<dyn AccessibilityChecker>,
    results: Vec<SearchItem>,
    selected: usize,
    generation: u64,
    /// Design report §3, step 08: shown whenever Accessibility isn't
    /// granted and the captain hasn't dismissed the notice — never a dead
    /// end, never nagging once dismissed. `None` until the daemon's
    /// persisted dismissal flag has been fetched, so the banner doesn't
    /// flash on for one frame before that first response lands.
    accessibility_banner_dismissed: Option<bool>,
    /// Set when `Request::Activate` comes back as `Response::Error` (or the
    /// request fails to reach the daemon at all) — the underlying app/file
    /// moved or was deleted since it was indexed, or the daemon is
    /// unreachable. Rendered in place of the footer's normal title/verb
    /// (`render_footer`) rather than a new toast surface: same fixed
    /// geometry, no layout change, just different content for one strip
    /// that's already always on screen. Cleared by the next query change or
    /// summon so it can never outlive the state that produced it.
    activation_error: Option<String>,
    /// Whether `material::install` put a native background view behind the
    /// window. When `true`, the panel fills with `theme::SURFACE_PANEL_TRANSLUCENT`
    /// so that material actually shows through; when `false` (the material
    /// install errored — see `main.rs`), it fills fully opaque and grows a
    /// hairline border instead, per the design report's own explicit
    /// fallback (§1: "opaque-plus-shadow... as the fallback").
    translucent: bool,
}

impl Root {
    pub fn new(
        client: NekoClient,
        accessibility: Rc<dyn AccessibilityChecker>,
        translucent: bool,
        cx: &mut App,
    ) -> Entity<Self> {
        cx.new(|cx| {
            let text_field = TextField::new(cx);
            // Subscribed to `ContentChanged` specifically, not observed via
            // `cx.observe` — `TextField` also notifies on every cursor
            // blink (a render concern), and `cx.observe` cannot
            // distinguish that from a real edit. Search must only re-run on
            // an actual query change. See `ContentChanged`'s doc comment.
            cx.subscribe(&text_field, |root: &mut Root, _field, _event: &ContentChanged, cx| {
                root.run_search(cx);
            })
            .detach();
            let mut root = Self {
                text_field,
                client,
                accessibility,
                results: Vec::new(),
                selected: 0,
                generation: 0,
                accessibility_banner_dismissed: None,
                activation_error: None,
                translucent,
            };
            root.run_search(cx);
            root.fetch_accessibility_banner_state(cx);
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
        // Re-checked on every summon, not just once at process start:
        // accessibility may have been granted from System Settings, or the
        // banner dismissed from a previous summon, since this window was
        // last shown.
        self.fetch_accessibility_banner_state(cx);
    }

    /// Pushed by `Event::IconsUpdated` (`main.rs`'s daemon-event loop) once
    /// a batch of background icon extraction finishes. Re-runs the current
    /// query rather than clearing it (unlike `reset_for_summon`) — this can
    /// fire while the panel is open and mid-search, and the point is to let
    /// an icon that just became available actually show up in what's
    /// already on screen, not to reset it. `run_search` re-derives each
    /// item's `icon` fresh from the daemon on every call (see
    /// `AppsProvider::search`), so results already fitting the same query
    /// naturally pick up any icon that finished since the last response. No
    /// filesystem check happens here or anywhere on this client-side path —
    /// the daemon is the one place that stats icon files, once per search
    /// it already has to run.
    pub fn refresh_icons(&mut self, cx: &mut Context<Self>) {
        self.run_search(cx);
    }

    /// Evidence/verification-only — see `TextField::set_content_for_evidence`'s
    /// doc comment. Drives a real query into the field (which re-runs
    /// search through the same `ContentChanged` path a keystroke would),
    /// for `evidence.rs`'s `NEKO_SHOW_QUERY` hook.
    pub fn set_query_for_evidence(&mut self, query: &str, cx: &mut Context<Self>) {
        self.text_field.update(cx, |field, cx| field.set_content_for_evidence(query, cx));
    }

    fn fetch_accessibility_banner_state(&mut self, cx: &mut Context<Self>) {
        let client = self.client.clone();
        cx.spawn(async move |this, cx| {
            let response = client.request(Request::GetOnboardingState).await;
            let Ok(Response::OnboardingState { accessibility_banner_dismissed, .. }) = response else {
                return;
            };
            let _ = this.update(cx, |root, cx| {
                root.accessibility_banner_dismissed = Some(accessibility_banner_dismissed);
                cx.notify();
            });
        })
        .detach();
    }

    fn show_accessibility_banner(&self) -> bool {
        self.accessibility_banner_dismissed == Some(false) && !self.accessibility.is_trusted()
    }

    fn open_accessibility_settings(&mut self, _: &ClickEvent, _window: &mut Window, _cx: &mut Context<Self>) {
        crate::accessibility::open_accessibility_settings();
    }

    fn dismiss_accessibility_banner(&mut self, _: &ClickEvent, _window: &mut Window, cx: &mut Context<Self>) {
        self.accessibility_banner_dismissed = Some(true);
        let client = self.client.clone();
        cx.spawn(async move |_this, _cx| {
            let _ = client.request(Request::DismissAccessibilityBanner).await;
        })
        .detach();
        cx.notify();
    }

    fn run_search(&mut self, cx: &mut Context<Self>) {
        // A stale activation-failure message from a previous result no
        // longer applies once the query changes underneath it.
        self.activation_error = None;
        self.generation += 1;
        let generation = self.generation;
        let query = self.text_field.read(cx).content().to_string();
        // Snapshot *before* the request goes out, not when the response
        // lands: this is "what was highlighted going into this search",
        // which `resolve_selection` uses to decide whether to keep the
        // highlight in place or the result set is genuinely new.
        let previous_selection = self
            .results
            .get(self.selected)
            .map(|item| (item.kind.clone(), item.id.clone()));
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
                    let previous = previous_selection
                        .as_ref()
                        .map(|(kind, id)| (kind.as_str(), id.as_str()));
                    root.selected = resolve_selection(previous, &root.results);
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
        // A single generic action, routed by `kind` back to whichever
        // provider produced this row — see `Request::Activate`'s doc
        // comment. The panel never needs to know what "activating" an app
        // vs. a clipboard entry vs. a file actually does.
        let request = Request::Activate { kind: item.kind, id: item.id };
        let client = self.client.clone();
        cx.spawn(async move |this, cx| {
            // `Request::Activate` really can come back `Response::Error`
            // (the underlying app/file moved or was deleted since it was
            // indexed — `server.rs`'s own unit tests cover both "no such
            // app" and "no such provider") — discarding it here used to
            // hide the panel exactly as if the activation had worked, with
            // no indication anything failed. A transport-level `Err` (the
            // daemon unreachable) is folded into the same inline-error path
            // rather than a second, silent no-op, since from the captain's
            // seat "nothing happened" and "it errored" both need the same
            // visible result: the panel stays open and says why.
            let outcome = client.request(request).await;
            let error_message = match outcome {
                Ok(Response::Error { message }) => Some(message),
                Ok(_) => None,
                Err(_) => Some("couldn't reach neko".to_string()),
            };
            let failed = error_message.is_some();
            let _ = this.update(cx, |root, cx| {
                root.activation_error = error_message;
                cx.notify();
            });
            if !failed {
                let _ = cx.update(|cx| cx.hide());
            }
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
            .bg(if self.translucent {
                theme::SURFACE_PANEL_TRANSLUCENT
            } else {
                theme::SURFACE_PANEL
            })
            .when(!self.translucent, |el| {
                el.border_1().border_color(theme::BORDER_HAIRLINE_STRONG)
            })
            .rounded(px(theme::PANEL_RADIUS_PX))
            .shadow_lg()
            .overflow_hidden()
            .child(self.render_input_row())
            .child(self.render_content_area(cx, query_is_empty))
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
            .px_5()
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

    fn render_content_area(&self, cx: &mut Context<Self>, query_is_empty: bool) -> impl IntoElement {
        // Horizontal only, deliberately: `design.css`'s `.panel-list` also
        // takes `padding-top`/`padding-bottom`, but this container's
        // available height is a tuned, tested budget (`fit_within_budget`
        // below) that assumes zero vertical inset — spending any of it here
        // would need re-deriving that budget, a materially bigger change
        // than what this pass is fixing. The horizontal 8px is still real:
        // it's what makes the panel's 16px corner radius and the selected
        // row's 8px pill radius concentric (16 = 8 + 8), and what pairs with
        // each row's own 12px padding below to match the input row's 20px
        // inset — before this, the container had no padding at all, so rows
        // sat flush against the panel's rounded corner and noticeably
        // closer to the edge than the search glyph above them.
        let mut container = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h(px(0.))
            .overflow_hidden()
            .px_2();

        if self.show_accessibility_banner() {
            container = container.child(self.render_accessibility_banner(cx));
        }

        if self.results.is_empty() {
            return container.child(render_empty_state(query_is_empty));
        }

        // A header per contiguous run of the same `kind` — every provider's
        // results get their own section (design report screen 11: "one
        // query, two result types, same list", now generalized to however
        // many providers are registered). The daemon already emits each
        // provider's results contiguously (`search::allocate`), so this
        // walks the list once rather than sorting or grouping client-side.
        // The header text itself is `item.section_label`, provider data —
        // this function has no per-kind knowledge at all.
        let mut current_section: Option<&str> = None;
        for (idx, item) in self.results.iter().enumerate() {
            if current_section != Some(item.kind.as_str()) {
                container = container.child(section_header(item.section_label.clone()));
                current_section = Some(item.kind.as_str());
            }
            container = container.child(self.render_row(idx, item));
        }
        container
    }

    /// Design report §3, step 08 — content/copy matches the mockup, but not
    /// its outer position/size: that mockup repositions and narrows the
    /// whole panel to sit near a menu-bar icon, which this build doesn't
    /// have (see `AGENTS.md`, "Menu bar icon: not built"). Rendered instead
    /// as a strip inside the normal upper-third 680px panel, so the
    /// already-shipped fixed-size/positioning decision (this file's own
    /// module doc comment) doesn't get reopened for one banner.
    fn render_accessibility_banner(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap_3()
            .px_3()
            .py_2()
            .mb_1()
            .rounded(px(theme::ROW_RADIUS_PX))
            .bg(theme::BANNER_DANGER_BG)
            .child(
                div()
                    .flex_1()
                    .text_size(px(12.5))
                    .line_height(px(18.))
                    .text_color(theme::TEXT_SECONDARY)
                    .child("⌥Space is off. Accessibility access was skipped, so the hotkey won't open neko. Reopen neko from the Dock to search anytime."),
            )
            .child(
                div()
                    .id("banner-open-settings")
                    .flex_shrink_0()
                    .text_size(px(12.))
                    .text_color(theme::TEXT_SECONDARY)
                    .cursor(CursorStyle::PointingHand)
                    .hover(|s| s.text_color(theme::TEXT_PRIMARY))
                    .on_click(cx.listener(Self::open_accessibility_settings))
                    .child("Open System Settings"),
            )
            .child(
                div()
                    .id("banner-dismiss")
                    .flex_shrink_0()
                    .text_size(px(12.))
                    .text_color(theme::TEXT_TERTIARY)
                    .cursor(CursorStyle::PointingHand)
                    .hover(|s| s.text_color(theme::TEXT_PRIMARY))
                    .on_click(cx.listener(Self::dismiss_accessibility_banner))
                    .child("Dismiss"),
            )
    }

    fn render_row(&self, idx: usize, item: &SearchItem) -> impl IntoElement {
        let selected = idx == self.selected;
        let title_color = theme::TEXT_PRIMARY;
        let subtitle_color = if selected {
            theme::TEXT_TERTIARY_ON_SELECTED
        } else {
            theme::TEXT_TERTIARY
        };

        // What fills the icon slot is entirely provider data now (`item.icon`)
        // — this match is over the closed, rendering-only `Icon` enum, not
        // over which provider produced the row. A new provider that just
        // wants a cached raster or the placeholder square needs zero
        // changes here; one that wants a genuinely new painted shape adds a
        // `Glyph` variant and a case in `glyph_element` below, nothing else
        // in this file.
        let icon: AnyElement = match &item.icon {
            Icon::Image(path) => img(PathBuf::from(path))
                .w(px(theme::ROW_ICON_PX))
                .h(px(theme::ROW_ICON_PX))
                .rounded(px(theme::ROW_ICON_RADIUS_PX))
                .bg(theme::ROW_ICON_SOCKET_BG)
                .into_any_element(),
            Icon::Glyph(glyph) => glyph_element(*glyph),
            // An icon the daemon hasn't finished extracting yet (a fresh
            // install, or right after a daemon restart — see
            // `Event::IconsUpdated`'s doc comment) — a neutral glyph in the
            // socket rather than an empty hole, self-healing to the real
            // icon on the next `refresh_icons` without a layout change.
            Icon::Placeholder => app_icon_placeholder_glyph(),
        };

        div()
            .id(("result-row", idx))
            .flex()
            .items_center()
            .flex_shrink_0()
            .h(px(theme::RESULT_ROW_HEIGHT_PX))
            .px_3()
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
            .children(item.badge.clone().map(|badge| {
                div()
                    .flex_shrink_0()
                    .px(px(6.))
                    .py(px(2.))
                    .rounded(px(4.))
                    .bg(theme::ROW_ICON_SOCKET_BG)
                    .text_size(px(10.))
                    .text_color(theme::TEXT_TERTIARY)
                    .child(SharedString::from(badge))
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
        let base = div()
            .flex()
            .items_center()
            .justify_between()
            .flex_shrink_0()
            .h(px(theme::FOOTER_HEIGHT_PX))
            .px_5()
            .border_t_1()
            .border_color(theme::BORDER_HAIRLINE);

        // An activation failure takes over the footer's own fixed strip
        // instead of opening a new toast surface — same geometry, same
        // always-on-screen location, just different content until the next
        // query or summon clears it (`run_search`). Reuses the danger
        // tokens the accessibility banner already established
        // (`render_accessibility_banner`) rather than inventing a new color.
        if let Some(message) = &self.activation_error {
            return base.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_size(px(12.))
                    .text_color(theme::STATE_DANGER)
                    .child(format!("Couldn't open — {message}")),
            );
        }

        let selected_item = self.results.get(self.selected);
        // The primary action's verb matches what enter actually does — data
        // straight from the selected row's own provider (`item.action_label`),
        // not a client-side match on which provider produced it. Falls back
        // to the app provider's own verb when nothing is selected, matching
        // this footer's pre-existing behavior on an empty result list.
        let primary_action: SharedString = selected_item
            .map(|item| SharedString::from(item.action_label.clone()))
            .unwrap_or_else(|| "Open  ↵".into());
        base.child(
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
                    .child(div().w(px(1.)).h(px(16.)).bg(theme::BORDER_HAIRLINE_STRONG))
                    .child("Actions  ⌘K"),
            )
    }
}

/// The selection-preservation rule for a fresh search response: if the item
/// that was highlighted going into this search (identified by its stable
/// `(kind, id)`, not by index — index shifts as sections grow/shrink) is
/// still present in the new result set, the highlight follows it to its new
/// position. Otherwise the result set is genuinely new for this query and
/// the highlight resets to the top. This only ever runs when the query
/// actually changed (`run_search` is now gated on `TextField`'s
/// `ContentChanged` event, not blink's render-only notify — see
/// `text_field::ContentChanged`), so a "genuinely new" result set is the
/// normal case; this rule matters for the query-changes-but-the-top-match-
/// is-still-there case, and for a keyboard adjustment made while the
/// request for the *next* keystroke was still in flight.
fn resolve_selection(previous: Option<(&str, &str)>, results: &[SearchItem]) -> usize {
    previous
        .and_then(|(kind, id)| results.iter().position(|item| item.kind == kind && item.id == id))
        .unwrap_or(0)
}

/// Trims `results` to what renders within `budget_px` without ever showing
/// a partial row or a section header with no row beneath it — and, when a
/// secondary provider (anything after the first section) has a match,
/// without ever letting a long run of primary-section matches crowd it out
/// of the panel entirely.
///
/// The panel is a fixed-size window (see this module's own doc comment) —
/// there's no scroll machinery and dynamic resize is an explicit non-goal —
/// so unlike a scrollable list, anything that doesn't fit has to be dropped
/// here rather than merely clipped by `overflow_hidden()` on the content
/// container, which would otherwise render the last row half-visible right
/// against the footer.
///
/// The reservation, generalized from a hard-coded apps-vs-clipboard-only
/// rule to however many contiguous provider sections `results` actually
/// contains (a fourth provider needs no changes here at all): the first
/// section is primary and gets whatever's left of `budget_px` after every
/// *other* section has one header-plus-one-row set aside for it. If an
/// earlier section uses less than its capped share, later sections split
/// the difference too — `fit_section` below just spends whatever budget is
/// actually left after the previous section, in order, same as before this
/// task.
fn fit_within_budget(results: Vec<SearchItem>, budget_px: f32) -> Vec<SearchItem> {
    let sections = group_into_sections(results);
    if sections.is_empty() {
        return Vec::new();
    }

    let header_and_one_row = theme::SECTION_HEADER_HEIGHT_PX + theme::RESULT_ROW_HEIGHT_PX;
    // Each section's own budget is the *original* `budget_px`, minus what
    // every earlier section actually used (not its capped share — an
    // earlier section using less than its cap must roll the difference
    // forward), minus a floor reserved for every section still to come
    // (not just the very next one) — that's what stops a middle section
    // from crowding out the *last* section the same way a first section
    // could crowd out a second.
    let mut used_so_far = 0.0;
    let mut kept_counts = Vec::with_capacity(sections.len());
    for (i, section) in sections.iter().enumerate() {
        let reserved_for_later_sections = (sections.len() - 1 - i) as f32 * header_and_one_row;
        let available = budget_px - used_so_far - reserved_for_later_sections;
        let (kept, used_px) = fit_section(section, available);
        kept_counts.push(kept);
        used_so_far += used_px;
    }

    sections
        .into_iter()
        .zip(kept_counts)
        .flat_map(|(section, kept)| section.into_iter().take(kept))
        .collect()
}

/// Splits `results` into contiguous same-`kind` runs, preserving order —
/// the same grouping `render_content_area` uses to decide where a section
/// header goes, pulled out so `fit_within_budget` can reason about "the
/// first section" vs. "every other section" generically.
fn group_into_sections(results: Vec<SearchItem>) -> Vec<Vec<SearchItem>> {
    let mut sections: Vec<Vec<SearchItem>> = Vec::new();
    for item in results {
        match sections.last_mut() {
            Some(section) if section.last().is_some_and(|last| last.kind == item.kind) => section.push(item),
            _ => sections.push(vec![item]),
        }
    }
    sections
}

/// How many leading items of one same-kind, contiguous section fit within
/// `budget_px` (a header, charged once if anything is kept, plus one row
/// per item), and the pixel height they use.
fn fit_section(items: &[SearchItem], budget_px: f32) -> (usize, f32) {
    let header_and_one_row = theme::SECTION_HEADER_HEIGHT_PX + theme::RESULT_ROW_HEIGHT_PX;
    if items.is_empty() || budget_px < header_and_one_row {
        return (0, 0.0);
    }
    let rows_that_fit =
        ((budget_px - theme::SECTION_HEADER_HEIGHT_PX) / theme::RESULT_ROW_HEIGHT_PX).floor() as usize;
    let kept = rows_that_fit.min(items.len());
    (kept, theme::SECTION_HEADER_HEIGHT_PX + kept as f32 * theme::RESULT_ROW_HEIGHT_PX)
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
        .px_3()
        .text_size(px(13.))
        .text_color(theme::TEXT_TERTIARY)
        .child(message)
}

fn section_header(label: impl Into<SharedString>) -> impl IntoElement {
    div()
        .flex_shrink_0()
        .h(px(theme::SECTION_HEADER_HEIGHT_PX))
        .flex()
        .items_center()
        .px_3()
        .text_size(px(11.))
        .text_color(theme::TEXT_TERTIARY)
        .child(label.into())
}

/// The app row-icon slot before the daemon has finished extracting a real
/// icon — a small centered rounded-square outline on the same
/// `ROW_ICON_SOCKET_BG` plate every other row icon sits on, so a still-
/// loading row reads as "generic app, not loaded yet" rather than a hole in
/// the list. Deliberately a different shape from `glyph_element`'s marks
/// (bars for text, rings for a link, ...) — this socket will very shortly
/// hold a real per-app icon, unlike a clipboard or file row's, which never
/// will in this slice.
fn app_icon_placeholder_glyph() -> AnyElement {
    div()
        .w(px(theme::ROW_ICON_PX))
        .h(px(theme::ROW_ICON_PX))
        .flex_shrink_0()
        .rounded(px(theme::ROW_ICON_RADIUS_PX))
        .bg(theme::ROW_ICON_SOCKET_BG)
        .flex()
        .items_center()
        .justify_center()
        .child(
            div()
                .w(px(10.))
                .h(px(10.))
                .rounded(px(3.))
                .border_2()
                .border_color(theme::TEXT_TERTIARY),
        )
        .into_any_element()
}

/// A small hand-painted glyph for the row-icon slot, in the same spirit as
/// `search_glyph` below (a painted shape composed from plain divs, not a
/// font glyph or an SVG asset — this codebase has no bundled icon-asset
/// pipeline, and a Unicode symbol is exactly what the design report's §6
/// finding on unreliable glyph rendering in GPUI already ruled out for the
/// search icon). `Text`/`Link` predate this task (clipboard rows have no
/// per-entry icon); `File`/`Folder` are this task's own addition for file
/// search results that haven't gotten a real icon.
fn glyph_element(glyph: Glyph) -> AnyElement {
    let slot = div().w(px(theme::ROW_ICON_PX)).h(px(theme::ROW_ICON_PX)).flex_shrink_0();
    match glyph {
        // Three stacked bars of decreasing width — a plain "lines of text"
        // mark.
        Glyph::Text => slot
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
        Glyph::Link => slot
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
        // A plain document outline (a portrait rounded-rect, border only —
        // no fill, so it composes correctly whether the row is selected or
        // the window is translucent, unlike a shape that would need to fake
        // a cutout against the background color) with one short bar
        // standing in for a line of text, same weight as `Glyph::Text`'s
        // bars.
        Glyph::File => slot
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .w(px(12.))
                    .h(px(15.))
                    .rounded(px(1.))
                    .border_2()
                    .border_color(theme::TEXT_TERTIARY)
                    .child(div().w(px(6.)).h(px(1.5)).rounded(px(1.)).bg(theme::TEXT_TERTIARY)),
            )
            .into_any_element(),
        // A folder shape: a wide rounded rectangle with a small tab along
        // its top edge.
        Glyph::Folder => slot
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .relative()
                    .w(px(15.))
                    .h(px(12.))
                    .child(
                        div()
                            .absolute()
                            .top_0()
                            .left(px(1.))
                            .w(px(6.))
                            .h(px(2.))
                            .rounded_t(px(1.))
                            .bg(theme::TEXT_TERTIARY),
                    )
                    .child(
                        div()
                            .absolute()
                            .top(px(2.))
                            .left_0()
                            .w(px(15.))
                            .h(px(10.))
                            .rounded(px(2.))
                            .border_2()
                            .border_color(theme::TEXT_TERTIARY),
                    ),
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

    fn item(kind: &str) -> SearchItem {
        item_with_id(kind, "x")
    }

    fn item_with_id(kind: &str, id: &str) -> SearchItem {
        SearchItem {
            id: id.into(),
            kind: kind.into(),
            title: id.into(),
            subtitle: None,
            icon: Icon::Placeholder,
            section_label: kind.into(),
            action_label: "Open  ↵".into(),
            badge: None,
            accessory: None,
        }
    }

    #[test]
    fn resolve_selection_follows_the_previously_selected_item_to_its_new_index() {
        // "notes" was selected (via Down) before this search; the new
        // result set still contains it, just at a different index — the
        // highlight must follow it there, not snap back to the top.
        let new_results = vec![
            item_with_id("app", "safari"),
            item_with_id("app", "notes"),
            item_with_id("app", "mail"),
        ];
        let selected = resolve_selection(Some(("app", "notes")), &new_results);
        assert_eq!(selected, 1);
    }

    #[test]
    fn resolve_selection_resets_to_top_when_the_previous_item_is_gone() {
        // The previously selected item didn't match this query at all —
        // the result set is genuinely new, so the highlight resets.
        let new_results = vec![item_with_id("app", "safari")];
        let selected = resolve_selection(Some(("app", "notes")), &new_results);
        assert_eq!(selected, 0);
    }

    #[test]
    fn resolve_selection_with_no_previous_selection_defaults_to_top() {
        let new_results = vec![item_with_id("app", "safari")];
        assert_eq!(resolve_selection(None, &new_results), 0);
    }

    #[test]
    fn resolve_selection_does_not_match_across_kinds() {
        // Same `id` string, different `kind` (an app path vs. a clipboard
        // entry's own content-as-id) must not be treated as the same item.
        let new_results = vec![item_with_id("clipboard", "notes")];
        let selected = resolve_selection(Some(("app", "notes")), &new_results);
        assert_eq!(selected, 0);
    }

    #[test]
    fn six_apps_and_a_clipboard_row_still_show_the_clipboard_row() {
        // The exact shape that first produced a half-clipped row, and then
        // (after fixing that) produced a dropped-entirely clipboard
        // section: 6 apps plus 1 clipboard entry, against the fixed 320px
        // content budget. The clipboard reservation means the app section
        // gives up a row rather than the clipboard section losing its only
        // one.
        let mut results: Vec<SearchItem> = (0..6).map(|_| item("app")).collect();
        results.push(item("clipboard"));

        let fitted = fit_within_budget(results, CONTENT_AREA_MIN_HEIGHT_PX);

        let apps_kept = fitted.iter().filter(|i| i.kind == "app").count();
        let clipboard_kept = fitted.iter().filter(|i| i.kind == "clipboard").count();
        assert_eq!(clipboard_kept, 1, "the top clipboard result must always be visible when one matched");
        assert_eq!(apps_kept, 5, "apps give up one row to make room, not zero clipboard rows");
        // Still no partial row and no dangling header: total height fits.
        let total_height = theme::SECTION_HEADER_HEIGHT_PX * 2.0
            + (apps_kept + clipboard_kept) as f32 * theme::RESULT_ROW_HEIGHT_PX;
        assert!(total_height <= CONTENT_AREA_MIN_HEIGHT_PX);
        // Order is preserved: apps first, clipboard after — matches how
        // `render_content_area` detects section boundaries.
        assert_eq!(fitted.last().unwrap().kind, "clipboard");
    }

    #[test]
    fn many_more_app_matches_still_cannot_crowd_clipboard_out_entirely() {
        let mut results: Vec<SearchItem> = (0..20).map(|_| item("app")).collect();
        results.push(item("clipboard"));
        let fitted = fit_within_budget(results, CONTENT_AREA_MIN_HEIGHT_PX);
        assert!(fitted.iter().any(|i| i.kind == "clipboard"));
    }

    #[test]
    fn clipboard_gets_the_app_sections_unused_budget_too() {
        // Only 1 app matched, so it can't use its whole reserved-against
        // share — the rest of the budget (not just the 1-row reservation)
        // should go to clipboard.
        let mut results = vec![item("app")];
        results.extend((0..3).map(|_| item("clipboard")));
        let fitted = fit_within_budget(results, CONTENT_AREA_MIN_HEIGHT_PX);
        let clipboard_kept = fitted.iter().filter(|i| i.kind == "clipboard").count();
        assert_eq!(clipboard_kept, 3);
    }

    #[test]
    fn a_pure_app_query_is_unaffected_by_the_clipboard_reservation() {
        // No clipboard entries at all -> no reservation taken -> same
        // count a pure app-only search produced before clipboard existed
        // (7 of 8 requested fit once the one header is charged).
        let results: Vec<SearchItem> = (0..RESULT_LIMIT).map(|_| item("app")).collect();
        let fitted = fit_within_budget(results, CONTENT_AREA_MIN_HEIGHT_PX);
        assert_eq!(fitted.len(), 7);
    }

    #[test]
    fn results_that_already_fit_are_returned_unchanged() {
        let results = vec![item("app"), item("clipboard")];
        let fitted = fit_within_budget(results.clone(), CONTENT_AREA_MIN_HEIGHT_PX);
        assert_eq!(fitted, results);
    }

    #[test]
    fn empty_results_stay_empty() {
        assert_eq!(fit_within_budget(Vec::new(), CONTENT_AREA_MIN_HEIGHT_PX), Vec::new());
    }

    #[test]
    fn a_third_provider_gets_the_same_reservation_with_zero_special_casing() {
        // Proves `fit_within_budget` generalized cleanly to N sections: a
        // "file" section behaves exactly like "clipboard" did on its own —
        // reserved a floor, never crowded out by a long run of app
        // matches — without this function knowing "file" exists as a
        // concept anywhere.
        let mut results: Vec<SearchItem> = (0..6).map(|_| item("app")).collect();
        results.push(item("file"));
        results.push(item("clipboard"));

        let fitted = fit_within_budget(results, CONTENT_AREA_MIN_HEIGHT_PX);

        assert!(fitted.iter().any(|i| i.kind == "file"), "file's reservation must survive");
        assert!(fitted.iter().any(|i| i.kind == "clipboard"), "clipboard's reservation must survive");
        // Section order preserved: app, then file, then clipboard.
        let kinds: Vec<&str> = fitted.iter().map(|i| i.kind.as_str()).collect();
        let first_file = kinds.iter().position(|&k| k == "file").unwrap();
        let first_clipboard = kinds.iter().position(|&k| k == "clipboard").unwrap();
        assert!(kinds[..first_file].iter().all(|&k| k == "app"));
        assert!(first_file < first_clipboard);
    }
}
