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
//! critical path. Instead the window height is one fixed size (448px) and
//! the footer is pinned to its true bottom with a flex-grow content area,
//! so short result lists just leave quiet space above the footer rather
//! than the window itself growing/shrinking. Correct, on-brief, and zero
//! risk to the summon-latency budget; real dynamic resizing is a
//! follow-up.
//!
//! The real `NSWindow`'s own *width* is fixed too, at
//! `theme::PANEL_WIDTH_WITH_DETAIL_PX` (760px), for the whole process
//! lifetime — not a v1 scope cut like the height above, but the fix for a
//! real defect (`AGENTS.md`, "Mode view resize seam"): resizing the real
//! window for a mode transition left `gpui`'s own paint viewport silently
//! out of sync with it once the window had been shown/hidden a few times.
//! `render`'s own stage element centers the narrower root-list panel
//! inside that fixed window instead; `Root::update_background_bounds`
//! keeps the native material backdrop in lockstep via a direct `NSView`
//! frame set, never a window resize.

use std::path::PathBuf;
use std::rc::Rc;

use gpui::{
    Anchor, AnyElement, App, ClickEvent, Context, CursorStyle, Entity, FocusHandle, Focusable,
    MouseButton, MouseDownEvent, Render, ScrollHandle, SharedString, Window, actions, anchored, deferred,
    div, img, point, prelude::*, px,
};
use neko_client::NekoClient;
use neko_protocol::{Glyph, Icon, ItemAction, Request, Response, SearchItem};

use crate::accessibility::AccessibilityChecker;
use crate::edge_fade::scroll_edge_fade;
use crate::menu_frost::sync_menu_frost;
use crate::modes::{self, ModeChrome};
use crate::motion;
use crate::text_field::{ContentChanged, DEFAULT_PLACEHOLDER, TextField};
use crate::theme;

actions!(panel, [SelectNext, SelectPrevious, Confirm, OpenActionsMenu]);

const RESULT_LIMIT: usize = 8;
/// How long a query has to stay in flight before the "still searching" tell
/// (`render_searching_tell`) appears — long enough that the common fast
/// case (apps/clipboard/settings, all answering well under this) never sees
/// it, short enough to give a real signal well before `files::QUERY_TIMEOUT`
/// (1.5s, `AGENTS.md`) — the one provider whose worst case actually reaches
/// this window.
const SEARCHING_TELL_DELAY_MS: u64 = 150;
/// A mode's own list wants "as many of this one provider's matches as it
/// can consider," not the shared, multi-provider root-list budget — the
/// mode list scrolls (`render_mode_list`, `edge_fade::scroll_edge_fade`)
/// rather than being budget-fit to a fixed content area the way
/// `RESULT_LIMIT`/`fit_within_budget` bound the root list.
const MODE_RESULT_LIMIT: usize = 50;
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
    /// Whether `material::install_menu_overlay` put a *second*, menu-scoped
    /// native background view behind the window — see that function's own
    /// doc comment. `false` whenever `translucent` is `false` (no ambient
    /// glass for a menu-scoped patch of it to read as distinct against) or
    /// the overlay install itself errored; either way the actions menu
    /// falls back to its original fully-opaque `SURFACE_RAISED` fill and
    /// never attempts to sync a native view that isn't there — the same
    /// "honest fallback" shape `translucent` already establishes for the
    /// panel's own background.
    menu_frost: bool,
    /// Mirrors `NekoClient::is_connected()` — pushed by `main.rs`'s summon
    /// loop, which already polls something else on a fixed short interval
    /// (see that method's own doc comment). Defaults optimistic (`true`):
    /// the supervisor's very first dial-in race (tens to a couple hundred
    /// ms, per `neko-client`'s own test) only ever happens while this
    /// window is still hidden pre-first-summon, so there's nothing for a
    /// captain to see either way — biasing toward *not* flashing a false
    /// "can't reach neko-daemon" banner on a normal, fast launch matches
    /// this codebase's existing "fail toward not showing a false state"
    /// calls (see `main.rs`'s `fetch_onboarding_state`).
    connected: bool,
    /// The bounded `ImageCache` every row's `img(path)` element loads
    /// through — installed once on the content-area container
    /// (`render_content_area`, `.image_cache(...)`), not per-row, so every
    /// `Icon::Image` in the results list shares one cache instance. See
    /// `row_icon_cache.rs`'s own module doc comment for why this exists
    /// (GPUI's sprite atlas never reclaims a tile without it) and why it's
    /// a bounded LRU rather than a full clear on every summon.
    row_icon_cache: Entity<crate::row_icon_cache::RowIconCache>,
    /// `Some` while a command's mode is active (`SearchItem::enters_mode`) —
    /// see `crate::modes`'s module doc comment for the full concept. `None`
    /// is the ordinary root list.
    active_mode: Option<ActiveMode>,
    /// `Some` while the `⌘K` actions menu is open for the currently
    /// selected row.
    actions_menu: Option<ActionsMenuState>,
    /// Snapshot of `actions_menu.is_some()` taken at the very start of the
    /// current mouse-down gesture, by a capture-phase listener on `render`'s
    /// own outer panel div — see `handle_actions_menu_trigger_click`'s doc
    /// comment for the click race this exists to resolve. Consumed (read
    /// and reset to `false`) by that same handler; a stale `true` can never
    /// leak into a later, unrelated click because it's overwritten by the
    /// capture-phase listener on *every* mouse-down, not just ones that hit
    /// the trigger.
    menu_open_before_this_press: bool,
    /// Set once a search has been in flight for `SEARCHING_TELL_DELAY_MS`
    /// without a response landing for it — see `run_search`'s own doc
    /// comment. Cleared the instant a new search starts or the in-flight one
    /// resolves, so it never outlives the query it describes.
    searching: bool,
    /// `Some(generation)` from the moment `run_search` dispatches a request
    /// for that generation until its response (success or error) lands —
    /// `None` once resolved. The delayed-reveal task that flips `searching`
    /// on checks this, not just `generation` alone: `generation` only
    /// changes on the *next* search, so without this a response that
    /// resolves well within `SEARCHING_TELL_DELAY_MS` (the common case)
    /// would still see the delayed task fire later for the same,
    /// already-answered generation and incorrectly flip the tell on.
    pending_search_generation: Option<u64>,
    /// Tracks the mode list's own scroll position (`render_mode_list`) —
    /// the root list never scrolls (still budget-fit, `fit_within_budget`,
    /// per this module's "v1 simplification" doc comment above), so this is
    /// only ever read/written while a mode is active. One persistent handle
    /// reused across mode entries rather than a fresh one each time, so
    /// `edge_fade::scroll_edge_fade` and `select_next`/`select_previous`'s
    /// own scroll-into-view calls are always looking at the same state.
    mode_scroll: ScrollHandle,
}

/// The one piece of state a mode transition actually carries, beyond the
/// static `ModeChrome` — the query the root list had before entering, so
/// exiting can restore it exactly. `chrome` is `&'static` (looked up once
/// from `crate::modes::MODES` on entry), so this whole struct is `Copy`
/// apart from the owned `String`.
#[derive(Clone)]
struct ActiveMode {
    chrome: &'static ModeChrome,
    saved_query: String,
}

/// The `⌘K` actions menu's own state — which row it's for (so a stray
/// keystroke race can't apply an action to whatever's newly selected
/// instead), which action is highlighted, and whether a destructive action
/// is "armed" (one Enter selects it, a second confirms — see
/// `confirm_menu_action`'s doc comment for why this exists).
#[derive(Clone)]
struct ActionsMenuState {
    kind: String,
    id: String,
    actions: Vec<ItemAction>,
    selected: usize,
    confirm_armed: bool,
}

impl Root {
    pub fn new(
        client: NekoClient,
        accessibility: Rc<dyn AccessibilityChecker>,
        translucent: bool,
        menu_frost: bool,
        cx: &mut App,
    ) -> Entity<Self> {
        cx.new(|cx| Self::build(client, accessibility, translucent, menu_frost, cx))
    }

    /// The real construction logic, factored out of [`new`](Self::new) so a
    /// test can build a `Root` directly inside `TestAppContext::add_window`'s
    /// own closure (which needs a plain `V`, not an `Entity<V>` — `new`
    /// itself wraps this in `cx.new(...)`) without duplicating any of it.
    fn build(
        client: NekoClient,
        accessibility: Rc<dyn AccessibilityChecker>,
        translucent: bool,
        menu_frost: bool,
        cx: &mut Context<Self>,
    ) -> Self {
        let text_field = TextField::new(cx);
        let row_icon_cache = crate::row_icon_cache::RowIconCache::new(cx);
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
            menu_frost,
            connected: true,
            row_icon_cache,
            active_mode: None,
            actions_menu: None,
            menu_open_before_this_press: false,
            searching: false,
            pending_search_generation: None,
            mode_scroll: ScrollHandle::new(),
        };
        root.run_search(cx);
        root.fetch_accessibility_banner_state(cx);
        root
    }

    /// Called right before the window is activated on a summon, so every
    /// summon starts from a clean query rather than whatever was last
    /// typed (matches Raycast's own behavior, confirmed live per the
    /// design report's evidence log).
    ///
    /// **Deliberately does not clear `row_icon_cache` here.** This is one
    /// of the two boundaries named for cache eviction — a full clear on
    /// every fresh summon would drop icons that are about to be shown
    /// again immediately (the same apps a captain summons repeatedly),
    /// forcing a redundant disk reload and a visible blank-then-appear
    /// flash on almost every summon — the exact "evicted too eagerly"
    /// regression the icon-cache task's own brief warns against. The
    /// bounded LRU in `row_icon_cache.rs` evicts continuously instead,
    /// only when a genuinely new icon identity is requested while already
    /// at capacity, which a fresh summon's new query naturally can (and
    /// does) trigger without any extra call here. See that module's doc
    /// comment for the full reasoning.
    ///
    /// **Also force-exits any active mode.** A mode is treated as
    /// per-summon-session state, not something that survives the panel
    /// being hidden and re-shown — matching "every summon starts from a
    /// clean query" above: if a captain hid the panel while inside
    /// clipboard-history mode (Escape, or clicking outside), the *next*
    /// hotkey press should re-summon the ordinary root list, not silently
    /// resume the mode they were in. This is also what keeps the native
    /// background material correctly narrowed before the panel is shown
    /// again — see `update_background_bounds`'s own doc comment for why
    /// this no longer resizes the real `NSWindow` at all.
    pub fn reset_for_summon(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Unconditional, not just inside the mode branch below: the actions
        // menu is per-summon-session state exactly like a mode is (this
        // method's own module-level reasoning), and it can be left open
        // independent of any mode — e.g. the window lost activation
        // (`main.rs`'s `cx.observe_window_activation`) while `⌘K` was open
        // on a root-list row. That path hides the whole window without ever
        // routing through `handle_dismiss`'s own "close the menu first"
        // logic, so without this a stale open menu would silently reappear
        // on the next summon.
        self.close_actions_menu(window);
        if let Some(mode) = self.active_mode.take() {
            self.text_field.update(cx, |field, cx| field.set_placeholder(DEFAULT_PLACEHOLDER, cx));
            self.mode_scroll.set_offset(point(px(0.), px(0.)));
            if mode.chrome.has_detail {
                self.update_background_bounds(window, theme::PANEL_WIDTH_PX);
            }
        }
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

    /// Updates the native background material view to match a mode
    /// transition — see `AGENTS.md`, "Mode view resize seam", for why this
    /// is a direct `NSView` frame set (`material::set_background_frame`)
    /// rather than a native `NSWindow` resize: the real window is now
    /// always `theme::PANEL_WIDTH_WITH_DETAIL_PX` wide (`main.rs`'s own
    /// window creation), so this only ever moves/resizes the background
    /// view *within* that fixed window, centering it at `new_width` — the
    /// same centering `Render::render`'s own `justify_center()` stage
    /// element produces for the panel `div` itself (flexbox centering a
    /// `new_width`-wide child inside a `PANEL_WIDTH_WITH_DETAIL_PX`-wide
    /// row lands on this exact same `x`), so the two always agree without
    /// either one hard-coding the other's formula. Best-effort: an
    /// error (no raw window handle, material not installed) is logged and
    /// otherwise ignored, never a panic and never a blocked mode transition
    /// — the panel's own `div` width/centering still changes either way, so
    /// the *content* is always internally consistent even on the rare path
    /// where the native backdrop fails to follow it.
    fn update_background_bounds(&self, window: &Window, new_width: f32) {
        let x = (theme::PANEL_WIDTH_WITH_DETAIL_PX - new_width) / 2.0;
        if let Err(e) = crate::material::set_background_frame(window, x, new_width, PANEL_HEIGHT_PX) {
            eprintln!("neko: could not update the native background frame for a mode transition: {e}");
        }
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
    ///
    /// **Also deliberately does not clear `row_icon_cache` here**, for the
    /// same reason `reset_for_summon` doesn't — see that method's doc
    /// comment. An icon that just finished extracting is, by construction,
    /// a resource identity the cache has never loaded before (it was
    /// `Icon::Placeholder`, a painted glyph, not an `img()` at all, until
    /// this event fired), so it's always a normal cache miss here, never a
    /// stale hit that needs invalidating first.
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

    /// Evidence/verification-only — drives the exact same `confirm()` path
    /// a real Enter keystroke takes (activate the selected result, surface
    /// `Response::Error` inline on failure) without a synthetic OS
    /// keystroke, for `evidence.rs`'s `NEKO_SHOW_CONFIRM` hook. Real
    /// keystroke synthesis was already ruled out for this panel — see
    /// `set_query_for_evidence`'s own doc comment.
    pub fn confirm_for_evidence(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.confirm(&Confirm, window, cx);
    }

    /// Evidence/verification-only — drives the same `Escape`/back-arrow
    /// path a real dismiss takes (`handle_dismiss`, exiting the active mode
    /// if one is open), for verification hooks that need to cycle a mode
    /// exit/re-entry without synthetic OS input — same reasoning as
    /// `confirm_for_evidence` above.
    pub fn dismiss_for_evidence(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.handle_dismiss(&crate::DismissWindow, window, cx);
    }

    /// Evidence/verification-only — drives the exact same `⌘K` path a real
    /// keypress or a real click on the footer's "Actions ⌘K" trigger takes
    /// (`open_actions_menu_for_selected_row`), for `evidence.rs`'s
    /// `NEKO_SHOW_ACTIONS_MENU` hook. Same reasoning as
    /// `confirm_for_evidence`/`dismiss_for_evidence` above: real synthetic
    /// input was already ruled out for this panel.
    pub fn open_actions_menu_for_evidence(&mut self, cx: &mut Context<Self>) {
        self.open_actions_menu_for_selected_row(cx);
    }

    /// Evidence/verification-only — scrolls the active mode's list to its
    /// own bottom (`ScrollHandle::scroll_to_bottom`, gpui's own public API,
    /// not a synthetic scroll-wheel event), for capturing the top-edge fade
    /// (`evidence.rs`'s `NEKO_SCROLL_MODE_LIST_TO_BOTTOM` hook) without
    /// synthetic OS input — same reasoning as `confirm_for_evidence` above.
    /// A no-op outside an active mode.
    pub fn scroll_mode_list_to_bottom_for_evidence(&mut self, cx: &mut Context<Self>) {
        if self.active_mode.is_some() {
            self.mode_scroll.scroll_to_bottom();
            cx.notify();
        }
    }

    /// Pushed by `main.rs`'s summon loop whenever `NekoClient::is_connected()`
    /// changes — see that method's doc comment for why this is a poll, not
    /// an event subscription. This is the fix for `data/neko-audit/report.md`
    /// Part 4 item 8: a dead daemon used to be a completely silent no-op
    /// (`run_search`'s response handling discarded every error, including a
    /// dropped connection); now the panel has a real, live-updating signal
    /// for it, independent of whether anything is being typed.
    pub fn set_connected(&mut self, connected: bool, cx: &mut Context<Self>) {
        if self.connected != connected {
            self.connected = connected;
            cx.notify();
        }
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
        // Reset immediately, not after a delay — a query that resolves fast
        // (the common case) must never flash the tell on before the delayed
        // reveal below even gets a chance to check whether it's still
        // needed.
        self.searching = false;
        self.generation += 1;
        let generation = self.generation;
        // Marks this generation's request outstanding *before* it's even
        // sent — the delayed-reveal task below checks this, not just
        // `generation` alone, precisely so a fast response (the common
        // case, resolving well under `SEARCHING_TELL_DELAY_MS`) can clear
        // it before that task ever fires. See this field's own doc comment.
        self.pending_search_generation = Some(generation);
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
        // The mode seam: while a mode is active, every keystroke scopes to
        // its own provider (`Request::Search`'s `provider` field) with a
        // generous limit — the merged root-list budget/reservation logic
        // (`fit_within_budget`) doesn't apply at all here; the mode list
        // renders every returned item and scrolls instead (`render_mode_list`,
        // `edge_fade::scroll_edge_fade`).
        let mode_provider = self.active_mode.as_ref().map(|m| m.chrome.provider_id.to_string());
        let limit = if mode_provider.is_some() { MODE_RESULT_LIMIT } else { RESULT_LIMIT };
        cx.spawn(async move |this, cx| {
            let response = client.request(Request::Search { query, limit, provider: mode_provider }).await;
            // A request error (including a dead connection) is deliberately
            // *not* surfaced here — `results`/`selected` just stay exactly
            // as they were, per the design intent below. The live
            // "can't reach neko-daemon" signal itself is
            // `main.rs`'s poll of `NekoClient::is_connected()`
            // (`Root::set_connected`), which doesn't depend on a search
            // having been attempted at all — see that method's doc comment
            // for why a request failing here is the wrong place to decide
            // connection state.
            let Ok(Response::SearchResults { items }) = response else {
                // Still clears `searching`/`pending_search_generation` for
                // this generation on the way out — an errored/malformed
                // response is a resolution too; without this, a query that
                // started slow and then failed would leave the tell showing
                // forever, since nothing else ever turns it back off for a
                // generation that never produces a `SearchResults`.
                let _ = this.update(cx, |root, cx| {
                    if root.generation == generation {
                        root.searching = false;
                        root.pending_search_generation = None;
                        cx.notify();
                    }
                });
                return;
            };
            let _ = this.update(cx, |root, cx| {
                if root.generation == generation {
                    root.searching = false;
                    root.pending_search_generation = None;
                    // The mode list scrolls (`edge_fade::scroll_edge_fade`
                    // in `render_mode_list`) rather than being budget-fit
                    // like the root list — `items` is already capped at
                    // `MODE_RESULT_LIMIT` by the request above, and
                    // rendering all of it, letting overflow scroll, is what
                    // makes the edge fade honest (see `edge_fade.rs`'s own
                    // module doc comment: a fade over content the captain
                    // has no way to actually reach would be decorative, not
                    // correct). The root list is untouched: still budget-fit
                    // before selection resolves, exactly as before this
                    // change, since it never scrolls at all.
                    root.results = if root.active_mode.is_some() {
                        items
                    } else {
                        fit_within_budget(items, CONTENT_AREA_MIN_HEIGHT_PX)
                    };
                    let previous = previous_selection
                        .as_ref()
                        .map(|(kind, id)| (kind.as_str(), id.as_str()));
                    root.selected = resolve_selection(previous, &root.results);
                    root.sync_mode_scroll_to_selection();
                    cx.notify();
                }
            });
        })
        .detach();

        // The "still searching" tell (`render_searching_tell`): a separate,
        // delayed task rather than a timeout race bolted onto the request
        // spawn above, so the fast common case (a response well under
        // `SEARCHING_TELL_DELAY_MS`) is completely unaffected. The actual
        // decision is `reveal_searching_tell_if_still_pending`, its own
        // method rather than inlined here, so a test can drive it directly
        // against a deliberately-still-pending generation without needing a
        // request that hangs for real wall-clock time.
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(std::time::Duration::from_millis(SEARCHING_TELL_DELAY_MS)).await;
            let _ = this.update(cx, |root, cx| root.reveal_searching_tell_if_still_pending(generation, cx));
        })
        .detach();
    }

    /// Flips the "still searching" tell on for `generation`, but only if
    /// that generation's request is still genuinely outstanding
    /// (`pending_search_generation`) — not just still current
    /// (`self.generation == generation` alone can't distinguish "resolved"
    /// from "still pending": `generation` only changes on the *next*
    /// search, so a response that already landed well within the delay
    /// (the common case) would otherwise still see this fire later and
    /// incorrectly show the tell for an already-answered query).
    fn reveal_searching_tell_if_still_pending(&mut self, generation: u64, cx: &mut Context<Self>) {
        if self.pending_search_generation == Some(generation) {
            self.searching = true;
            cx.notify();
        }
    }

    fn select_next(&mut self, _: &SelectNext, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(menu) = &mut self.actions_menu {
            if !menu.actions.is_empty() {
                menu.selected = (menu.selected + 1).min(menu.actions.len() - 1);
                menu.confirm_armed = false;
                cx.notify();
            }
            return;
        }
        if !self.results.is_empty() {
            self.selected = (self.selected + 1).min(self.results.len() - 1);
            self.sync_mode_scroll_to_selection();
            cx.notify();
        }
    }

    fn select_previous(&mut self, _: &SelectPrevious, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(menu) = &mut self.actions_menu {
            menu.selected = menu.selected.saturating_sub(1);
            menu.confirm_armed = false;
            cx.notify();
            return;
        }
        self.selected = self.selected.saturating_sub(1);
        self.sync_mode_scroll_to_selection();
        cx.notify();
    }

    /// Keeps the mode list's own scroll position following keyboard
    /// selection — called after every `self.selected` change while a mode
    /// is active (`select_next`/`select_previous`, and `run_search`'s
    /// mode-scoped response handler). A no-op for the root list, which
    /// never scrolls at all (`fit_within_budget` still guarantees it always
    /// fits — see this module's own "v1 simplification" doc comment).
    ///
    /// `ScrollHandle::scroll_to_item` takes an index into the tracked
    /// container's own DIRECT children, which is `self.results`' index
    /// *plus* one slot for every day-bucket header (`SearchItem::group_label`)
    /// rendered ahead of it (`render_mode_list` interleaves header divs with
    /// row divs) — `mode_list_child_index` below computes that offset. Its
    /// `FirstVisible` scroll strategy only moves the offset if the target
    /// isn't already visible, so this is safe to call on every selection
    /// change without fighting a manual scroll the captain did in between
    /// (nothing here runs on a bare scroll-wheel tick — only on an actual
    /// `self.selected` change, which mouse-wheel scrolling alone never
    /// causes).
    fn sync_mode_scroll_to_selection(&self) {
        if self.active_mode.is_none() {
            return;
        }
        self.mode_scroll.scroll_to_item(mode_list_child_index(&self.results, self.selected));
    }

    fn confirm(&mut self, _: &Confirm, window: &mut Window, cx: &mut Context<Self>) {
        if self.actions_menu.is_some() {
            self.confirm_menu_action(window, cx);
            return;
        }
        let Some(item) = self.results.get(self.selected).cloned() else {
            return;
        };
        // A command row never reaches `Request::Activate` at all —
        // confirming it is a client-side UI transition, not a daemon
        // action (see `crate::modes`'s module doc comment).
        if let Some(mode_id) = item.enters_mode {
            self.enter_mode(&mode_id, window, cx);
            return;
        }
        // A single generic action, routed by `kind` back to whichever
        // provider produced this row — see `Request::Activate`'s doc
        // comment. The panel never needs to know what "activating" an app
        // vs. a clipboard entry vs. a file actually does. Matches every
        // pre-existing Enter behavior exactly (hides the panel on success)
        // whether or not a mode happens to be active — Enter on a
        // clipboard-history row still pastes-and-dismisses, the same as
        // Enter on one in the root list always has.
        let request = Request::Activate { kind: item.kind, id: item.id, action: None };
        self.perform_activation(request, true, cx);
    }

    /// Confirming a row's primary action (`confirm`, above) and confirming
    /// a `⌘K` menu selection (`confirm_menu_action`, below) both end in the
    /// same "send `Request::Activate`, surface any error inline" shape —
    /// pulled out once rather than duplicated. `hide_on_success` is the one
    /// real difference: a primary Enter finishes the interaction (matching
    /// every pre-existing Activate call site), while a menu action is a
    /// management operation on the current list (copy, delete, ...) the
    /// captain very plausibly wants to keep working from — the actions menu
    /// has no mockup at all (the launch brief's own note), so "the menu
    /// never closes the panel, Enter on a row always can" is this task's
    /// own deliberate, stated design choice, not a frozen spec's.
    fn perform_activation(&mut self, request: Request, hide_on_success: bool, cx: &mut Context<Self>) {
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
                // A menu action that changed the underlying data (delete,
                // ...) and isn't about to hide the panel needs the current
                // list re-fetched to reflect it — a deleted row must not
                // keep rendering until the next keystroke happens to
                // re-search.
                if !failed && !hide_on_success {
                    root.run_search(cx);
                }
                cx.notify();
            });
            if !failed && hide_on_success {
                cx.update(|cx| cx.hide());
            }
        })
        .detach();
    }

    /// Enters `mode_id`'s mode: saves the current query so `exit_mode` can
    /// restore it, clears the field to start the mode's own list fresh,
    /// swaps the placeholder, and — if the mode wants a detail pane —
    /// widens the *visible* panel (`Render::render`'s own centering, driven
    /// by `active_mode`, plus `update_background_bounds`'s matching native
    /// backdrop — the real `NSWindow` itself never resizes, see that
    /// method's doc comment). A no-op if `mode_id` doesn't name a
    /// registered mode (a stale/corrupted value) or a mode is already
    /// active (confirming a command row is only ever possible from the
    /// root list, since commands never appear inside a mode's own scoped
    /// search — but this guards the invariant rather than assuming it).
    fn enter_mode(&mut self, mode_id: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self.active_mode.is_some() {
            return;
        }
        let Some(chrome) = modes::chrome_for(mode_id) else {
            return;
        };
        let saved_query = self.text_field.read(cx).content().to_string();
        self.active_mode = Some(ActiveMode { chrome, saved_query });
        self.selected = 0;
        self.mode_scroll.set_offset(point(px(0.), px(0.)));
        self.close_actions_menu(window);
        if chrome.has_detail {
            self.update_background_bounds(window, theme::PANEL_WIDTH_WITH_DETAIL_PX);
        }
        self.text_field.update(cx, |field, cx| {
            field.set_placeholder(chrome.placeholder, cx);
            // A real edit (emits `ContentChanged`), which is what actually
            // runs the mode-scoped search above — `active_mode` is already
            // `Some` by the time this synchronously fires, so `run_search`
            // takes the mode branch immediately, not the root-list one.
            field.set_content("", cx);
        });
        cx.notify();
    }

    /// Leaves the active mode, if any: restores the pre-entry query
    /// (triggering a real root-list search, same reasoning as
    /// `enter_mode`'s own `set_content` call), restores the default
    /// placeholder, closes any open actions menu, and narrows the visible
    /// panel back if it had widened.
    fn exit_mode(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(mode) = self.active_mode.take() else {
            return;
        };
        self.close_actions_menu(window);
        self.selected = 0;
        self.mode_scroll.set_offset(point(px(0.), px(0.)));
        if mode.chrome.has_detail {
            self.update_background_bounds(window, theme::PANEL_WIDTH_PX);
        }
        self.text_field.update(cx, |field, cx| {
            field.set_placeholder(DEFAULT_PLACEHOLDER, cx);
            field.set_content(&mode.saved_query, cx);
        });
        cx.notify();
    }

    /// `⌘K` — opens the actions menu for the currently selected row. Thin
    /// wrapper over `open_actions_menu_for_selected_row` so the keybinding
    /// (`OpenActionsMenu`) and the footer trigger's own click handler
    /// (`handle_actions_menu_trigger_click`, below) share one real
    /// implementation.
    fn open_actions_menu(&mut self, _: &OpenActionsMenu, _window: &mut Window, cx: &mut Context<Self>) {
        self.open_actions_menu_for_selected_row(cx);
    }

    /// Opens the actions menu for the currently selected row, if it has any
    /// (`SearchItem::actions`); a no-op for a row with none (apps, files,
    /// settings, commands today), which is why the footer's "Actions ⌘K"
    /// label is always shown rather than conditionally hidden — matching
    /// Raycast's own convention of a menu that's simply empty (here: inert)
    /// rather than a control that disappears depending on selection.
    fn open_actions_menu_for_selected_row(&mut self, cx: &mut Context<Self>) {
        let Some(item) = self.results.get(self.selected) else {
            return;
        };
        if item.actions.is_empty() {
            return;
        }
        self.actions_menu = Some(ActionsMenuState {
            kind: item.kind.clone(),
            id: item.id.clone(),
            actions: item.actions.clone(),
            selected: 0,
            confirm_armed: false,
        });
        cx.notify();
    }

    /// A capture-phase listener on `render`'s own outer panel div — see that
    /// call site's own comment for why it must be capture, not bubble.
    /// Snapshots whether the actions menu is mounted at the very start of
    /// every mouse-down gesture, *before* any capture-phase handler
    /// downstream (the menu card's own `on_mouse_down_out`, below) has a
    /// chance to mutate it. `gpui` dispatches every capture-phase listener
    /// across the whole window for one event before any bubble-phase
    /// listener runs (confirmed by reading `gpui-0.2.2/src/window.rs`'s
    /// `dispatch_mouse_event`: two full passes, capture forward then bubble
    /// reversed, never interleaved) — registering this on an ancestor of
    /// both the menu card and the footer trigger, rather than on the trigger
    /// alone, is what makes the snapshot correct regardless of where either
    /// of those sits in paint order (the menu card is `deferred`, painted
    /// after the ordinary tree, but capture already visited this ancestor
    /// before recursing into any child either way).
    fn note_actions_menu_mouse_down(&mut self, _event: &MouseDownEvent, _window: &mut Window, _cx: &mut Context<Self>) {
        self.menu_open_before_this_press = self.actions_menu.is_some();
    }

    /// The menu card's own `on_mouse_down_out` — a click anywhere outside
    /// the card (but still inside the window; the trigger itself counts as
    /// "outside" the card, since the two don't overlap) closes the menu
    /// without touching anything else. Deliberately does *not* check whether
    /// the click was inside the panel at all vs. the transparent margin —
    /// `on_mouse_down_out` only fires for `MouseDownEvent`s the window
    /// itself received, and a click on the margin already has its own
    /// dismiss handler (`render_dismiss_margin`) that hides the whole
    /// window, menu included, before this would ever matter.
    fn close_actions_menu_from_outside_click(&mut self, _event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.actions_menu.is_some() {
            self.close_actions_menu(window);
            cx.notify();
        }
    }

    /// The footer's "Actions ⌘K" trigger — `on_click` (a full press+release
    /// on the trigger itself), not a raw `on_mouse_down`, so a press that
    /// drags off the trigger before releasing doesn't open it, matching
    /// ordinary button semantics.
    ///
    /// **The race this guards against** (comet's own finding,
    /// `data/neko-comet-design/report.md` §2.1, reimplemented here in
    /// neko's own terms — no lingering "closing" state, since neko's menu
    /// close is an instant cut, see `motion.rs`'s own doc comment on why):
    /// the trigger sits outside the menu card, so clicking it while the menu
    /// is open *always* fires `close_actions_menu_from_outside_click` too,
    /// on the very same physical mouse-down (capture phase, before this
    /// handler's own bubble-phase click even fires). A naive toggle —
    /// "closed ⇒ open, open ⇒ close" — reads `actions_menu` fresh right
    /// here and finds it already `None` (the outside handler beat it to the
    /// close), so it would open a *fresh* menu instead of leaving the
    /// captain's dismiss click alone: the menu would flicker closed-then-
    /// reopened on a single click, never actually dismissible by clicking
    /// the trigger again. `menu_open_before_this_press`
    /// (`note_actions_menu_mouse_down`) is the fix: it's a snapshot of
    /// whether the menu was mounted *before* this gesture's capture phase
    /// ran at all, so this handler can tell "the outside click just closed
    /// what was open a moment ago" (consume the note, stay closed) apart
    /// from "the menu was already closed, this is a genuine open" (open it).
    fn handle_actions_menu_trigger_click(&mut self, _event: &ClickEvent, _window: &mut Window, cx: &mut Context<Self>) {
        if std::mem::take(&mut self.menu_open_before_this_press) {
            return;
        }
        self.open_actions_menu_for_selected_row(cx);
    }

    /// Enter, while the actions menu is open. A destructive action
    /// (`ItemAction::destructive`) needs a *second* Enter to actually run —
    /// the first just arms it (re-rendered with a "press again to confirm"
    /// label, `render_actions_menu`) — so a single mis-keyed Enter on
    /// "Delete" can never silently destroy an entry; moving the menu
    /// selection at all (`select_next`/`select_previous`) disarms it again,
    /// so the confirmation can't survive being scrolled past and back.
    fn confirm_menu_action(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(menu) = &mut self.actions_menu else { return };
        let Some(action) = menu.actions.get(menu.selected).cloned() else {
            self.close_actions_menu(window);
            cx.notify();
            return;
        };
        if action.destructive && !menu.confirm_armed {
            menu.confirm_armed = true;
            cx.notify();
            return;
        }
        let kind = menu.kind.clone();
        let id = menu.id.clone();
        self.close_actions_menu(window);
        cx.notify();
        let request = Request::Activate { kind, id, action: Some(action.id) };
        self.perform_activation(request, false, cx);
    }

    /// Closes the `⌘K` actions menu, if one is open — the single place that
    /// ever clears `actions_menu`, so the native menu-overlay material (when
    /// installed — `menu_frost`) is always hidden in lockstep. Necessary
    /// because closing the menu unmounts `menu_frost::MenuFrostSync`
    /// entirely (it simply stops being painted), and that element has no
    /// "hide" branch of its own to run when it disappears — see its own
    /// module doc comment. A no-op, including no native call, when no menu
    /// was open.
    fn close_actions_menu(&mut self, window: &Window) {
        if self.actions_menu.take().is_none() {
            return;
        }
        if self.menu_frost
            && let Err(e) = crate::material::hide_menu_overlay(window)
        {
            eprintln!("neko: could not hide the menu frost overlay: {e}");
        }
    }

    /// `Escape` — closes the actions menu if it's open, else exits the
    /// active mode if one is, else hides the whole panel (the ordinary,
    /// pre-existing behavior). Registered as a window-level `on_action`
    /// listener on `Root`'s own div (`Render::render`, below), which — per
    /// GPUI's own action-dispatch order (window listeners run in the bubble
    /// phase *before* global ones, and a handled action stops propagating
    /// there by default) — intercepts `DismissWindow` before `main.rs`'s
    /// global `cx.on_action(|_, cx| cx.hide())` fallback ever sees it. Only
    /// the "hide the whole panel" branch reaches that fallback's own
    /// behavior, and it does so explicitly (`cx.hide()`), not by
    /// re-propagating — the two are equivalent for that one case, and
    /// keeping this one call site self-contained is clearer than routing
    /// back through the global handler.
    fn handle_dismiss(&mut self, _: &crate::DismissWindow, window: &mut Window, cx: &mut Context<Self>) {
        if self.actions_menu.is_some() {
            self.close_actions_menu(window);
            cx.notify();
            return;
        }
        if self.active_mode.is_some() {
            self.exit_mode(window, cx);
            return;
        }
        cx.hide();
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
        let panel_width = match &self.active_mode {
            Some(mode) if mode.chrome.has_detail => theme::PANEL_WIDTH_WITH_DETAIL_PX,
            _ => theme::PANEL_WIDTH_PX,
        };
        let root = div()
            .key_context("Panel")
            .on_action(cx.listener(Self::select_next))
            .on_action(cx.listener(Self::select_previous))
            .on_action(cx.listener(Self::confirm))
            .on_action(cx.listener(Self::open_actions_menu))
            .on_action(cx.listener(Self::handle_dismiss))
            // Capture phase, not bubble — see `note_actions_menu_mouse_down`'s
            // own doc comment for why this has to run before any
            // capture-phase handler further down the tree (the actions
            // menu card's own `on_mouse_down_out`) has a chance to mutate
            // `actions_menu` first.
            .capture_any_mouse_down(cx.listener(Self::note_actions_menu_mouse_down))
            .relative()
            .flex()
            .flex_col()
            .w(px(panel_width))
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
            // **Deliberately no drawn shadow here — this used to be
            // `.shadow_lg()`.** That was the real, second cause of the
            // "black tent" halo the captain kept reporting even after the
            // native window shadow was disabled ("The double-panel shadow
            // defect" in `AGENTS.md`): `shadow_lg`'s blur/spread paints a
            // few px of soft, low-alpha black *outside* this div's own
            // bounds, and since `235bf88` there's `theme::PANEL_ROOT_INSET_PX`
            // of real, otherwise-empty transparent window on each side of
            // the root-list panel for it to bleed into
            // (`render_dismiss_margin`'s own margin `div`s, next to this
            // one, paint nothing at all). Confirmed with a single-variable
            // test, not assumed: alpha-channel analysis of a window-scoped
            // capture showed a soft 0→~17/255 gradient in the margin with
            // this line present, and a hard, exact 0 with it removed. Every
            // other paint source in that margin was already deliberately
            // eliminated for the same reason (the native auto-shadow fix
            // above; the native backdrop material is intentionally
            // narrowed to the panel's own width, not the window's) — this
            // was the one holdout. The translucent Glass material
            // (`self.translucent`) already reads as an elevated surface on
            // its own via real vibrancy; the opaque fallback keeps its
            // `.border_1()` above for edge definition, which never had any
            // spill risk. See `docs/evidence/panel-shadow-tent-fix-report.md`.
            .overflow_hidden()
            .child(self.render_input_row(cx))
            .child(match &self.active_mode {
                Some(mode) => self.render_mode_content(mode),
                None => self.render_content_area(cx, query_is_empty).into_any_element(),
            })
            .child(self.render_footer(cx));
        // The real `NSWindow` is always `PANEL_WIDTH_WITH_DETAIL_PX` wide
        // now, never resized at runtime for a mode transition — see
        // `AGENTS.md`, "Mode view resize seam", and
        // `update_background_bounds`'s own doc comment. This stage element
        // is what gpui actually lays out against the window's own (now
        // fixed, always-correct) viewport; centering the narrower
        // root-list `root` div inside it, rather than ever asking gpui to
        // resize the window itself, is what the fix trades on — matched
        // pixel-for-pixel by `update_background_bounds`'s identical
        // centering of the native backdrop, so the two always agree.
        //
        // **Explicit margin children, not `justify_center()`'s implicit
        // flex gap** — the margin is still real, clickable window area (the
        // real `NSWindow` frame extends past `root`'s own edge whenever
        // `panel_width < PANEL_WIDTH_WITH_DETAIL_PX`), and with no gpui
        // element covering it, a click there used to be silently swallowed:
        // this window is already key/frontmost (`WindowKind::PopUp`, no
        // `ignoresMouseEvents`, no custom hit-testing anywhere in `gpui`),
        // so it neither reaches whatever's behind it on the desktop nor
        // triggers `main.rs`'s click-outside-dismiss (`cx.
        // observe_window_activation`, which only fires on an actual
        // activation *change* — clicking inside this window's own frame,
        // margin included, is never that). Giving each margin div its own
        // `on_mouse_down` restores the click-outside *behavior* (dismiss)
        // for a region that can no longer be true click-through — see
        // `AGENTS.md`, "Mode view resize seam", for why real click-through
        // isn't available without a second overlay window or patching
        // `gpui`'s own hit-testing, neither undertaken here.
        let margin_width = (theme::PANEL_WIDTH_WITH_DETAIL_PX - panel_width) / 2.0;
        div()
            .w(px(theme::PANEL_WIDTH_WITH_DETAIL_PX))
            .h(px(PANEL_HEIGHT_PX))
            .flex()
            .child(self.render_dismiss_margin(margin_width, "left", cx))
            .child(root)
            .child(self.render_dismiss_margin(margin_width, "right", cx))
    }
}

impl Root {
    /// One of `render`'s own two margin children — see its call site's doc
    /// comment for why this exists at all. `width` is `0` whenever the
    /// panel is already the full `PANEL_WIDTH_WITH_DETAIL_PX` (detail
    /// mode), which renders an empty, harmless, unclickable sliver rather
    /// than needing a separate conditional.
    fn render_dismiss_margin(&self, width: f32, side: &'static str, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id(SharedString::from(format!("panel-margin-{side}")))
            .w(px(width))
            .h_full()
            .flex_shrink_0()
            .on_mouse_down(MouseButton::Left, cx.listener(|_root, _event, _window, cx| cx.hide()))
    }

    fn render_input_row(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .items_center()
            .flex_shrink_0()
            .h(px(theme::INPUT_ROW_HEIGHT_PX))
            .px_5()
            .gap_3()
            .text_color(theme::TEXT_PRIMARY)
            .text_size(px(18.))
            .child(match &self.active_mode {
                // The back affordance the launch brief asks for: "a back
                // arrow in place of the search glyph." Clickable — exits
                // the mode the same way Escape does, sharing `exit_mode`
                // rather than duplicating its logic.
                Some(_) => div()
                    .id("mode-back")
                    .cursor(CursorStyle::PointingHand)
                    .on_click(cx.listener(|root, _: &ClickEvent, window, cx| root.exit_mode(window, cx)))
                    .child(back_glyph())
                    .into_any_element(),
                None => search_glyph().into_any_element(),
            })
            .child(div().flex_1().child(self.text_field.clone()))
            .children(self.render_searching_tell())
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(theme::TEXT_TERTIARY)
                    .child("esc"),
            )
    }

    /// The "still searching" tell for a query that hasn't returned yet —
    /// `self.searching`, set by `run_search`'s own delayed-reveal task once
    /// `SEARCHING_TELL_DELAY_MS` has passed with no response for the current
    /// generation. `None` (nothing rendered, not an invisible placeholder)
    /// for the common fast case, which is the whole point: apps/clipboard/
    /// settings all answer well under the delay, so this never appears for
    /// them, and `run_search`'s own "keep the previous results on screen
    /// while a new request is in flight" behavior is otherwise silent —
    /// `AGENTS.md`'s own "Search and ranking" section records why that's the
    /// right call for the fast case, and why `files.rs`'s up-to-1.5s worst
    /// case needed a real signal instead of leaving the captain looking at
    /// stale results with no indication a new answer is coming.
    fn render_searching_tell(&self) -> Option<AnyElement> {
        if !self.searching {
            return None;
        }
        let reduced = motion::system_reduce_motion();
        let tell = div().text_size(px(11.)).text_color(theme::TEXT_TERTIARY).child("Searching…");
        Some(motion::fade_in("searching-tell-fade", reduced, tell))
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
            .px_2()
            // Every `img(path)` row icon under this container loads through
            // one bounded cache instance, not GPUI's default never-evicted
            // per-`App` asset cache — see `row_icon_cache.rs`.
            .image_cache(self.row_icon_cache.clone());

        if !self.connected {
            container = container.child(self.render_connection_banner());
        }

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
            container = container.child(self.render_row(idx, item, false));
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

    /// The fix for `data/neko-audit/report.md` Part 4 item 8 — same strip
    /// treatment as `render_accessibility_banner` just above (frozen design,
    /// no new chrome), but with no dismiss control: unlike the accessibility
    /// banner (a persisted setting the captain explicitly closes), this one
    /// tracks a live signal and clears itself the moment `set_connected`
    /// reports the daemon is reachable again — there's nothing to dismiss.
    fn render_connection_banner(&self) -> impl IntoElement {
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
                    .child("Can't reach neko-daemon. Results may be out of date."),
            )
    }

    /// `compact` drops `subtitle`/`accessory` from the row — the mode list's
    /// own anatomy (`data/neko-design/mockups/12-first-clipboard-use.html`'s
    /// `.row`: icon, title, type-tag, nothing else). The root list still
    /// wants both (`11-first-search.html`'s rows carry a subtitle and a
    /// trailing accessory) — that's the wider 680px row, with room for them.
    /// Left in at `MODE_LIST_COLUMN_WIDTH_PX` (264px), `item.subtitle` (e.g.
    /// clipboard's own `"Copied from {app}"`) and `item.accessory` (the
    /// relative-time stamp) had nowhere near enough room next to the title,
    /// badge, and icon — a real, captain-reported defect
    /// (`"remove con  Co   TEXT   now"`, title truncated mid-word,
    /// overlapping the subtitle's own truncated remainder before the badge
    /// and timestamp), not a taste call: the mockup's mode-list row simply
    /// never carries them, since `Application`/`Copied` already have a
    /// dedicated, unhurried home in the detail pane
    /// (`render_mode_detail`).
    fn render_row(&self, idx: usize, item: &SearchItem, compact: bool) -> impl IntoElement {
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
                    .children(item.subtitle.clone().filter(|_| !compact).map(|subtitle| {
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
            .children(item.accessory.clone().filter(|_| !compact).map(|accessory| {
                div()
                    .flex_shrink_0()
                    .text_size(px(11.))
                    .text_color(subtitle_color)
                    .child(SharedString::from(accessory))
            }))
    }

    fn render_footer(&self, cx: &mut Context<Self>) -> impl IntoElement {
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
        // The footer's left side is the mode's own name while a mode is
        // active (`data/neko-design/mockups/12-first-clipboard-use.html`'s
        // `footer-source`: "Clipboard History", constant regardless of
        // selection) rather than the selected row's title — the mode *is*
        // the context now, not whatever happens to be highlighted.
        let left_label: Option<SharedString> = match &self.active_mode {
            Some(mode) => Some(mode.chrome.title.into()),
            None => selected_item.map(|item| SharedString::from(item.title.clone())),
        };
        base.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_size(px(12.))
                    .text_color(theme::TEXT_TERTIARY)
                    .children(left_label),
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
                    .child(self.render_actions_trigger(cx)),
            )
    }

    /// The "Actions ⌘K" footer label, now a real clickable trigger for the
    /// menu it names, not just a static hint — Raycast's own footer actions
    /// are clickable the same way. `.relative()` establishes the positioned
    /// ancestor `render_actions_menu`'s own zero-size pin div needs (see
    /// that function's doc comment) — mounted here, as the trigger's own
    /// child, rather than as a `render()`-level sibling, is what anchors the
    /// floating menu to the trigger's actual on-screen position instead of a
    /// hand-tuned fixed offset from the panel's corner.
    fn render_actions_trigger(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let mut trigger = div()
            .id("actions-trigger")
            .relative()
            .cursor(CursorStyle::PointingHand)
            .on_click(cx.listener(Self::handle_actions_menu_trigger_click))
            .child("Actions  ⌘K");
        if let Some(menu) = self.actions_menu.clone() {
            trigger = trigger.child(self.render_actions_menu(&menu, cx));
        }
        trigger
    }

    /// The two-column mode view (`data/neko-design/mockups/
    /// 12-first-clipboard-use.html`): a fixed-width filtered list on the
    /// left, a preview + info pane on the right when
    /// `ModeChrome::has_detail` — otherwise just the list, full width. Sits
    /// where `render_content_area` sits for the root list; same content-area
    /// height budget (`CONTENT_AREA_MIN_HEIGHT_PX`), only ever a width
    /// change between the two.
    fn render_mode_content(&self, mode: &ActiveMode) -> AnyElement {
        let content = div()
            .flex()
            .flex_1()
            .min_h(px(0.))
            .overflow_hidden()
            .child(self.render_mode_list())
            .when(mode.chrome.has_detail, |el| el.child(self.render_mode_detail()));
        // A one-shot opacity reveal on entry, not the width/geometry
        // transition itself — the real `NSWindow` still never resizes at
        // runtime (`AGENTS.md`, "Mode view resize seam" — "cost two days"),
        // and this element doesn't touch `update_background_bounds` or the
        // panel `div`'s own width/centering at all, only the content painted
        // inside whatever width `render`'s own stage element already
        // resolved to this frame. `AnimationElement` is layout-transparent
        // (its own `request_layout` forwards the wrapped `Div`'s layout id
        // directly — confirmed by reading `gpui-0.2.2/src/elements/
        // animation.rs`), so this doesn't disturb `root`'s own `flex_1()`
        // expectations of whatever fills this slot. Keyed by a fixed
        // element id: since this whole subtree only exists while a mode is
        // active, each fresh `enter_mode` is a genuinely new mount (no
        // element state survives an exit), so the fade replays on every
        // entry rather than only the first.
        let reduced = motion::system_reduce_motion();
        motion::fade_in("mode-content-fade", reduced, content)
    }

    /// The mode's own filtered, time-grouped list — reuses `render_row`
    /// verbatim (it already renders purely from `SearchItem` data, with no
    /// per-provider knowledge), grouping by `SearchItem::group_label`
    /// instead of `kind`/`section_label` the way the root list's
    /// `render_content_area` does. Same image cache as the root list, so a
    /// clipboard-mode session doesn't get its own separate, redundant icon
    /// cache (moot today — clipboard rows are always painted glyphs, never
    /// `img()` — but correct if a future mode's provider ever has real
    /// per-row icons).
    /// Scrolls, unlike the root list's own fixed/budget-fit
    /// `render_content_area` (see this module's "v1 simplification" doc
    /// comment) — a mode's list can genuinely hold more entries
    /// (`MODE_RESULT_LIMIT`, 50) than the fixed content-area height ever
    /// fits, and letting the captain scroll to the rest is strictly better
    /// than the old `fit_mode_list` behavior of silently dropping whatever
    /// didn't fit. `edge_fade::scroll_edge_fade` wraps the scrollable
    /// container so the fade only ever shows where there's real overflow to
    /// scroll to — see that module's own doc comment.
    fn render_mode_list(&self) -> impl IntoElement {
        let mut container = div()
            .flex()
            .flex_col()
            .flex_shrink_0()
            .w(px(theme::MODE_LIST_COLUMN_WIDTH_PX))
            .h_full()
            .px_2()
            .border_r_1()
            .border_color(theme::BORDER_HAIRLINE)
            // `image_cache` is a `Div`-only method (not on the `Stateful<Div>`
            // `.id(...)` below produces), so it has to come first.
            .image_cache(self.row_icon_cache.clone())
            .id("mode-list-scroll")
            .overflow_y_scroll()
            .track_scroll(&self.mode_scroll);

        if self.results.is_empty() {
            container = container.child(render_empty_state_message("No matching entries."));
        } else {
            let mut current_group: Option<&Option<String>> = None;
            for (idx, item) in self.results.iter().enumerate() {
                if current_group != Some(&item.group_label) {
                    if let Some(label) = &item.group_label {
                        container = container.child(section_header(label.clone()));
                    }
                    current_group = Some(&item.group_label);
                }
                container = container.child(self.render_row(idx, item, true));
            }
        }

        let fade_color = if self.translucent { theme::SURFACE_PANEL_TRANSLUCENT } else { theme::SURFACE_PANEL };
        scroll_edge_fade(self.mode_scroll.clone(), fade_color.into(), theme::EDGE_FADE_BAND_PX, container)
    }

    /// The mode's own preview + info pane — deliberately *not* a generic
    /// "detail pane renderer": what fields are honest to show is entirely
    /// mode-specific (see `crate::modes`'s module doc comment, "what a
    /// second command has to implement" — this function is exactly the
    /// cost a second command with its own detail view pays). Three fields,
    /// matching `data/neko-design/mockups/12-first-clipboard-use.html`
    /// exactly: Application (`SearchItem::source`), Content Type
    /// (`SearchItem::badge`, title-cased), Copied (`SearchItem::accessory`
    /// — this app's existing relative-time label, not an absolute
    /// local-clock timestamp like the mockup's literal "Today, 9:50 AM":
    /// no date/time-formatting dependency exists anywhere in this codebase,
    /// and adding one for one label wasn't judged worth it — a disclosed,
    /// deliberate deviation, not an oversight). The preview box itself
    /// shows the entry's raw stored content (`SearchItem::id`, clipboard's
    /// own dedup key) rather than the list row's own truncated/quoted
    /// `title` — the whole point of a detail pane is showing what the list
    /// row had to compress.
    fn render_mode_detail(&self) -> impl IntoElement {
        let col = div().flex().flex_col().flex_1().min_w(px(0.)).min_h(px(0.)).gap_4().px_5().py_5();
        let Some(item) = self.results.get(self.selected) else {
            return col.child(
                div()
                    .text_size(px(12.5))
                    .text_color(theme::TEXT_TERTIARY)
                    .child("Select an entry to preview it."),
            );
        };

        let preview = div()
            .flex_1()
            .min_h(px(0.))
            .overflow_hidden()
            .p_3()
            .rounded(px(theme::ROW_RADIUS_PX))
            .bg(theme::SURFACE_INPUT)
            .border_1()
            .border_color(theme::BORDER_HAIRLINE)
            .text_size(px(13.))
            .text_color(theme::TEXT_PRIMARY)
            .child(SharedString::from(item.id.clone()));

        let mut info = div().flex().flex_col().gap_2();
        if let Some(source) = &item.source {
            info = info.child(detail_info_row("Application", source.clone()));
        }
        if let Some(badge) = &item.badge {
            info = info.child(detail_info_row("Content Type", title_case_badge(badge)));
        }
        if let Some(accessory) = &item.accessory {
            info = info.child(detail_info_row("Copied", accessory.clone()));
        }

        col.child(preview).child(info)
    }

    /// `⌘K`'s own popup — no mockup exists for this (the launch brief's own
    /// note), so this reuses the existing visual language (raised surface,
    /// hairline border, the same selected-row fill and danger color every
    /// other surface in this panel already uses) rather than inventing new
    /// tokens. Given real floating-layer discipline per
    /// `data/neko-comet-design/report.md` recommendation 1 (comet's
    /// `crates/ui/src/popover.rs:395-416`, `anchored_menu` — read for the
    /// pattern, reimplemented here against neko's own geometry and, unlike
    /// comet's fork, without `frost.rs`'s backdrop blur — `window.
    /// paint_backdrop_blur` is available as of the `wingleeio/zed` fork
    /// migration (`AGENTS.md`, "The GPUI dependency decision"), but using it
    /// is deliberately out of scope for that migration and left to a
    /// follow-up, not attempted here):
    ///
    /// - **`deferred(...)`** gives the card its own floating paint layer,
    ///   painted after (so visually above) everything else already painted
    ///   this frame — it can't be occluded by content painted later, the way
    ///   a plain `.child()` sitting earlier in paint order could be.
    /// - **`anchored().anchor(Anchor::BottomRight)
    ///   .snap_to_window_with_margin(px(8.0))`** positions the card relative
    ///   to the trigger's own on-screen point (the zero-size pin div in
    ///   `render_actions_trigger`, at the trigger's top-right corner —
    ///   `BottomRight` anchoring means the *card's* bottom-right corner sits
    ///   there, so it grows up and to the left, above the footer) and clamps
    ///   it to stay inside the real window if the trigger sits close to an
    ///   edge — genuinely reachable here: the clipboard mode's own detail
    ///   view runs the panel at the full `PANEL_WIDTH_WITH_DETAIL_PX` with no
    ///   side margin at all, putting the trigger right at the window's own
    ///   edge.
    /// - **`.occlude()`** on the card means a click on the card's own dead
    ///   space (padding, the gap between rows) can't fall through to
    ///   whatever sits underneath the floating layer.
    /// - **`.on_mouse_down_out(...)`** dismisses on a click anywhere outside
    ///   the card — not "any click anywhere" (which would double-fire with
    ///   the trigger's own click and reopen the menu; see
    ///   `handle_actions_menu_trigger_click`'s doc comment for that race and
    ///   its fix).
    ///
    /// Wrapped in `motion::menu_fade_in` for the open transition — see that
    /// module's own doc comment for why the close path stays an instant cut
    /// instead of a matching fade-out.
    fn render_actions_menu(&self, menu: &ActionsMenuState, cx: &mut Context<Self>) -> AnyElement {
        let card = div()
            .occlude()
            .on_mouse_down_out(cx.listener(Self::close_actions_menu_from_outside_click))
            .w(px(200.))
            .flex()
            .flex_col()
            .p_1()
            .gap(px(1.))
            .rounded(px(theme::ROW_RADIUS_PX))
            // Translucent, letting the native menu-overlay material
            // genuinely show through, only when that material actually
            // installed (`Root::menu_frost`) — otherwise the original
            // fully-opaque fill, the same honest-fallback shape
            // `Render::render`'s own panel background already uses for
            // `translucent`. See `theme::MENU_GLASS_TINT`'s own doc comment.
            .bg(if self.menu_frost { theme::MENU_GLASS_TINT } else { theme::SURFACE_RAISED })
            .border_1()
            .border_color(theme::BORDER_HAIRLINE_STRONG)
            .shadow_lg()
            .children(menu.actions.iter().enumerate().map(|(idx, action)| {
                let selected = idx == menu.selected;
                let armed = selected && menu.confirm_armed && action.destructive;
                let label: SharedString = if armed {
                    format!("Confirm {} — ↵ again", action.label).into()
                } else {
                    action.label.clone().into()
                };
                let color = if action.destructive { theme::STATE_DANGER } else { theme::TEXT_PRIMARY };
                div()
                    .id(("actions-menu-row", idx))
                    .flex()
                    .items_center()
                    .h(px(30.))
                    .px_2()
                    .rounded(px(theme::ROW_RADIUS_PX))
                    .when(selected, |el| el.bg(theme::SURFACE_SELECTED))
                    .text_size(px(12.5))
                    .text_color(color)
                    .child(label)
            }));
        let reduced = motion::system_reduce_motion();
        let card = motion::menu_fade_in("actions-menu-fade", reduced, card);
        // `paint_layer` discipline (`components::layered`, recommendation 3
        // in `data/neko-comet-design/report.md`) — the card's background,
        // border, and rows paint as one atomic scene layer, so a hover
        // repaint elsewhere in the panel this same frame can't reassign any
        // of this card's own quads to the wrong relative paint order. This
        // task's first real call site; see that module's own doc comment
        // for the rule going forward.
        let card = crate::components::layered::layered(card);
        // Syncs the native menu-overlay material's frame to this card's own
        // real, finished screen position every frame it paints — layout-
        // transparent like `layered`/`motion::menu_fade_in` above it, so it
        // sees exactly the position `anchored()` resolves below (including
        // its own edge-clamping), never a value computed independently. See
        // `menu_frost::sync_menu_frost`'s own doc comment. Only meaningful
        // when the overlay material actually installed (`Root::menu_frost`)
        // — otherwise this card already fell back to the plain opaque fill
        // above and there's no native view to keep in sync.
        let card = if self.menu_frost {
            sync_menu_frost(card).into_any_element()
        } else {
            card.into_any_element()
        };
        div()
            .absolute()
            .top_0()
            .right_0()
            .size_0()
            .child(
                deferred(
                    anchored()
                        .anchor(Anchor::BottomRight)
                        .snap_to_window_with_margin(px(8.0))
                        .child(card),
                )
                .priority(1),
            )
            .into_any_element()
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
/// section gets whatever's left of `budget_px` after every *other* section
/// has one header-plus-one-row set aside for it. If an earlier section uses
/// less than its capped share, later sections split the difference too —
/// `fit_section` below just spends whatever budget is actually left after
/// the previous section, in order, same as before this task.
///
/// **This intentionally reuses the daemon's own section order rather than
/// deciding one independently.** `results` arrives from the wire already
/// ordered by `neko_core::search::allocate`'s own final pass — sections by
/// content strength (their best candidate's score), registration order only
/// as the tiebreak (see that function's doc comment) — not by provider
/// registration order the way it used to be. `group_into_sections` merely
/// splits that order into contiguous runs; it never re-sorts. Giving "the
/// first section" the most generous pixel budget therefore now means "the
/// section the daemon judged most relevant to this query gets the most
/// rows," which is the same intent as the daemon's own reservation-then-
/// greedy budget — the screen and the wire have to agree on what "primary"
/// means, and this is how they stay in sync without duplicating the
/// ordering logic client-side.
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

/// Maps a `results` index to its position among `render_mode_list`'s own
/// DIRECT children — the index space `ScrollHandle::scroll_to_item`
/// operates in, which is *not* the same as `results`' own index once any
/// day-bucket header divs (`SearchItem::group_label`, rendered as their own
/// sibling child whenever it changes — `render_mode_list`'s own loop) are
/// interleaved ahead of a given row. Pulled out as a pure function so the
/// mapping is unit-testable without a live `Window`/`ScrollHandle`.
fn mode_list_child_index(results: &[SearchItem], target: usize) -> usize {
    let mut child_index = 0;
    let mut current_group: Option<&Option<String>> = None;
    for (idx, item) in results.iter().enumerate() {
        if current_group != Some(&item.group_label) {
            if item.group_label.is_some() {
                child_index += 1;
            }
            current_group = Some(&item.group_label);
        }
        if idx == target {
            return child_index;
        }
        child_index += 1;
    }
    child_index
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
    render_empty_state_message(message)
}

/// The plain one-line-tip shape `render_empty_state` uses, parameterized on
/// the message — shared with the mode list's own empty state
/// (`Root::render_mode_list`), which has different copy but the identical
/// layout.
fn render_empty_state_message(message: impl Into<SharedString>) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .h(px(theme::RESULT_ROW_HEIGHT_PX))
        .px_3()
        .text_size(px(13.))
        .text_color(theme::TEXT_TERTIARY)
        .child(message.into())
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
        // A clipboard board with a small clip tab along the top edge —
        // traces `data/neko-design/mockups/12-first-clipboard-use.html`'s
        // own clipboard-mode input-row glyph (`<rect x="5" y="4" width="10"
        // height="14" rx="2"/><rect x="7.5" y="2.5" width="5" height="3"
        // rx="1" fill/>`, from a 20×20 viewBox), reused here for the
        // root-list "Clipboard History" command row's own icon.
        Glyph::Clipboard => slot
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .relative()
                    .w(px(14.))
                    .h(px(16.))
                    .child(
                        div()
                            .absolute()
                            .top_0()
                            .left(px(3.5))
                            .w(px(7.))
                            .h(px(3.))
                            .rounded(px(1.))
                            .bg(theme::TEXT_TERTIARY),
                    )
                    .child(
                        div()
                            .absolute()
                            .top(px(1.5))
                            .left_0()
                            .w(px(14.))
                            .h(px(14.5))
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

/// The mode input row's back affordance — "a back arrow in place of the
/// search glyph," per the launch brief. Traced with `gpui::PathBuilder`,
/// the same mechanism `components::glyphs::opt_glyph`/`neko_wordmark_glyph`
/// already use for a shape a plain axis-aligned `div()` border can't draw
/// (a diagonal chevron) — GPUI's `div()` styling API has no rotation
/// primitive, and per the design report's §6 finding, a Unicode `←`
/// character isn't a reliable substitute either.
fn back_glyph() -> impl IntoElement {
    gpui::canvas(
        move |_bounds, _window, _cx| (),
        move |bounds, (), window, _cx| {
            let scale = f32::from(bounds.size.width) / 20.0;
            let ox = f32::from(bounds.origin.x);
            let oy = f32::from(bounds.origin.y);
            let pt = |x: f32, y: f32| gpui::point(px(ox + x * scale), px(oy + y * scale));

            let mut builder = gpui::PathBuilder::stroke(px((1.6f32 * scale).max(1.0)));
            builder.move_to(pt(12.0, 4.0));
            builder.line_to(pt(6.0, 10.0));
            builder.line_to(pt(12.0, 16.0));
            if let Ok(path) = builder.build() {
                window.paint_path(path, theme::TEXT_TERTIARY);
            }
        },
    )
    .w(px(14.))
    .h(px(14.))
}

/// The detail pane's one repeated row shape: a label on the left, the
/// value on the right — matches `data/neko-design/mockups/
/// 12-first-clipboard-use.html`'s plain `display:flex; justify-content:
/// space-between` info rows exactly (no card/border chrome of its own).
fn detail_info_row(label: &str, value: String) -> impl IntoElement {
    div()
        .flex()
        .justify_between()
        .gap_3()
        .text_size(px(12.))
        .child(div().flex_shrink_0().text_color(theme::TEXT_TERTIARY).child(SharedString::from(label.to_string())))
        .child(div().overflow_hidden().truncate().text_color(theme::TEXT_SECONDARY).child(SharedString::from(value)))
}

/// Translates a row badge's already-uppercase wire value (`"TEXT"`,
/// `"LINK"` — see `SearchItem::badge`'s own doc comment on why the wire
/// value is pre-uppercased rather than CSS-transformed) into the
/// title-cased form the detail pane's "Content Type" field shows in the
/// frozen mockup (`"Text"`, `"Link"`) — a plain, closed two-value
/// translation of data the *same* clipboard row already carries, not a
/// match on which provider produced the row.
fn title_case_badge(badge: &str) -> String {
    match badge {
        "TEXT" => "Text".to_string(),
        "LINK" => "Link".to_string(),
        other => other.to_string(),
    }
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
            enters_mode: None,
            group_label: None,
            actions: Vec::new(),
            source: None,
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

    #[test]
    fn the_reservation_holds_regardless_of_which_kind_the_daemon_put_first() {
        // `neko_core::search::allocate` now orders sections by content
        // strength, not provider registration order (see that function's
        // own doc comment) — "command" or "settings" can legitimately lead
        // a response now, not just "app". `fit_within_budget` never re-sorts
        // (`group_into_sections` only groups contiguous runs, preserving
        // whatever order `results` already arrived in), so this only proves
        // what matters here: the reservation-then-crowd-out guarantee holds
        // for *whichever* section happens to be first, not just "app".
        let mut results = vec![item("command")];
        results.extend((0..6).map(|_| item("clipboard")));
        results.push(item("settings"));

        let fitted = fit_within_budget(results, CONTENT_AREA_MIN_HEIGHT_PX);

        assert!(fitted.iter().any(|i| i.kind == "command"), "the leading section's own reservation must survive");
        assert!(fitted.iter().any(|i| i.kind == "settings"), "the trailing section must never be crowded out either");
        let kinds: Vec<&str> = fitted.iter().map(|i| i.kind.as_str()).collect();
        assert_eq!(kinds[0], "command", "the daemon's own section order must be preserved verbatim, not re-sorted here");
    }

    // --- Commands and modes: mode_list_child_index (the mode list now
    // scrolls — `edge_fade.rs` — rather than being budget-fit like the root
    // list, so this is the one piece of client-side logic that still has to
    // reason about `SearchItem::group_label` boundaries: mapping a
    // `results` index to `render_mode_list`'s own direct-child index, the
    // space `ScrollHandle::scroll_to_item` operates in.) ---

    fn mode_item(group: Option<&str>, id: &str) -> SearchItem {
        SearchItem { group_label: group.map(str::to_string), ..item_with_id("clipboard", id) }
    }

    #[test]
    fn mode_list_child_index_with_no_groups_is_the_identity() {
        let items = vec![mode_item(None, "a"), mode_item(None, "b"), mode_item(None, "c")];
        assert_eq!(mode_list_child_index(&items, 0), 0);
        assert_eq!(mode_list_child_index(&items, 2), 2);
    }

    #[test]
    fn mode_list_child_index_accounts_for_one_header_before_the_first_group() {
        let items = vec![mode_item(Some("Today"), "a"), mode_item(Some("Today"), "b")];
        // child 0 = the "Today" header, child 1 = row "a", child 2 = row "b"
        assert_eq!(mode_list_child_index(&items, 0), 1);
        assert_eq!(mode_list_child_index(&items, 1), 2);
    }

    #[test]
    fn mode_list_child_index_accounts_for_every_header_crossed_so_far() {
        let items = vec![
            mode_item(Some("Today"), "a"),
            mode_item(Some("Today"), "b"),
            mode_item(Some("Yesterday"), "c"),
            mode_item(Some("Yesterday"), "d"),
        ];
        // 0: Today header, 1: a, 2: b, 3: Yesterday header, 4: c, 5: d
        assert_eq!(mode_list_child_index(&items, 2), 4, "row c comes after both Today rows and the Yesterday header");
        assert_eq!(mode_list_child_index(&items, 3), 5);
    }

    #[test]
    fn mode_list_child_index_on_an_empty_list_is_zero() {
        assert_eq!(mode_list_child_index(&[], 0), 0);
    }

    #[test]
    fn title_case_badge_translates_the_two_known_wire_values_and_passes_through_anything_else() {
        assert_eq!(title_case_badge("TEXT"), "Text");
        assert_eq!(title_case_badge("LINK"), "Link");
        assert_eq!(title_case_badge("COMMAND"), "COMMAND");
    }

    // --- Commands and modes: Root-level state transitions, headless via
    // TestAppContext (no OS window, no screen pixels — see this module's
    // own report on what this can and can't prove without real rendering).

    use gpui::TestAppContext;

    use crate::accessibility::FakeAccessibilityChecker;

    fn test_root(cx: &mut TestAppContext) -> gpui::WindowHandle<Root> {
        let (client, _events) = NekoClient::connect(std::path::PathBuf::from(format!(
            "/tmp/neko-panel-test-{}-{}.sock",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        )));
        let accessibility: Rc<dyn AccessibilityChecker> = Rc::new(FakeAccessibilityChecker::new(true));
        cx.add_window(|_window, cx| Root::build(client, accessibility, true, true, cx))
    }

    /// A command row's `id`, `kind: "command"` — everything else is
    /// deliberately minimal, since only `enters_mode` and `action_label`
    /// matter to `confirm`'s own routing.
    fn command_item(mode: &str) -> SearchItem {
        SearchItem { enters_mode: Some(mode.to_string()), ..item_with_id("command", "clipboard-history") }
    }

    #[gpui::test]
    fn confirming_a_command_row_enters_its_mode_and_saves_the_prior_query(cx: &mut TestAppContext) {
        let window = test_root(cx);
        cx.run_until_parked();

        window
            .update(cx, |root, _window, cx| {
                root.text_field.update(cx, |field, cx| field.set_content("safari", cx));
            })
            .unwrap();
        cx.run_until_parked();

        window
            .update(cx, |root, _window, _cx| {
                root.results = vec![command_item("clipboard")];
                root.selected = 0;
            })
            .unwrap();

        window
            .update(cx, |root, window, cx| root.confirm(&Confirm, window, cx))
            .unwrap();
        cx.run_until_parked();

        window
            .update(cx, |root, _window, _cx| {
                let mode = root.active_mode.as_ref().expect("confirming a command row must enter a mode");
                assert_eq!(mode.chrome.id, "clipboard");
                assert_eq!(mode.saved_query, "safari", "the query typed before entering the mode must be saved");
            })
            .unwrap();
    }

    #[gpui::test]
    fn exiting_a_mode_restores_the_saved_query_and_clears_active_mode(cx: &mut TestAppContext) {
        let window = test_root(cx);
        cx.run_until_parked();

        window
            .update(cx, |root, _window, cx| {
                root.text_field.update(cx, |field, cx| field.set_content("safari", cx));
                root.results = vec![command_item("clipboard")];
                root.selected = 0;
            })
            .unwrap();
        window
            .update(cx, |root, window, cx| root.confirm(&Confirm, window, cx))
            .unwrap();
        cx.run_until_parked();
        window
            .update(cx, |root, _window, _cx| assert!(root.active_mode.is_some(), "must be in the mode before exiting it"))
            .unwrap();

        window
            .update(cx, |root, window, cx| root.exit_mode(window, cx))
            .unwrap();
        cx.run_until_parked();

        window
            .update(cx, |root, _window, cx| {
                assert!(root.active_mode.is_none());
                assert_eq!(root.text_field.read(cx).content(), "safari", "exiting must restore the pre-entry query");
            })
            .unwrap();
    }

    #[gpui::test]
    fn escape_exits_the_mode_instead_of_hiding_when_one_is_active(cx: &mut TestAppContext) {
        // `handle_dismiss` (bound to the same `escape` key `DismissWindow`
        // already uses globally) must intercept and exit the mode rather
        // than falling through to `cx.hide()` — this is the "back
        // affordance" half of the brief's "Escape and back leave it"
        // requirement; `exiting_a_mode_...` above already covers the other
        // half (the actual UI back-arrow, which calls the same
        // `exit_mode`).
        let window = test_root(cx);
        cx.run_until_parked();
        window
            .update(cx, |root, _window, cx| {
                root.results = vec![command_item("clipboard")];
                root.selected = 0;
                let _ = cx;
            })
            .unwrap();
        window
            .update(cx, |root, window, cx| root.confirm(&Confirm, window, cx))
            .unwrap();
        cx.run_until_parked();

        window
            .update(cx, |root, window, cx| root.handle_dismiss(&crate::DismissWindow, window, cx))
            .unwrap();

        window.update(cx, |root, _window, _cx| assert!(root.active_mode.is_none())).unwrap();
    }

    #[gpui::test]
    fn opening_the_actions_menu_on_a_row_with_no_actions_does_nothing(cx: &mut TestAppContext) {
        let window = test_root(cx);
        cx.run_until_parked();
        window
            .update(cx, |root, _window, _cx| {
                root.results = vec![item_with_id("app", "safari")]; // apps carry no actions
                root.selected = 0;
            })
            .unwrap();

        window
            .update(cx, |root, window, cx| root.open_actions_menu(&OpenActionsMenu, window, cx))
            .unwrap();

        window.update(cx, |root, _window, _cx| assert!(root.actions_menu.is_none())).unwrap();
    }

    #[gpui::test]
    fn opening_the_actions_menu_on_a_row_with_actions_populates_it(cx: &mut TestAppContext) {
        let window = test_root(cx);
        cx.run_until_parked();
        let mut clipboard_row = item_with_id("clipboard", "hello");
        clipboard_row.actions = vec![
            ItemAction { id: "paste".into(), label: "Paste".into(), destructive: false },
            ItemAction { id: "delete".into(), label: "Delete".into(), destructive: true },
        ];
        window
            .update(cx, |root, _window, _cx| {
                root.results = vec![clipboard_row];
                root.selected = 0;
            })
            .unwrap();

        window
            .update(cx, |root, window, cx| root.open_actions_menu(&OpenActionsMenu, window, cx))
            .unwrap();

        window
            .update(cx, |root, _window, _cx| {
                let menu = root.actions_menu.as_ref().expect("a row with actions must open the menu");
                assert_eq!(menu.actions.len(), 2);
                assert_eq!(menu.id, "hello");
            })
            .unwrap();
    }

    #[gpui::test]
    fn a_destructive_menu_action_requires_a_second_confirm_before_it_ever_sends_a_request(cx: &mut TestAppContext) {
        // The mis-keyed-delete guard: the very first Enter on "Delete" must
        // only arm it, never perform it — proven here by checking the menu
        // is still open (and therefore no request was dispatched to close
        // it) after exactly one `confirm`.
        let window = test_root(cx);
        cx.run_until_parked();
        let mut clipboard_row = item_with_id("clipboard", "hello");
        clipboard_row.actions = vec![ItemAction { id: "delete".into(), label: "Delete".into(), destructive: true }];
        window
            .update(cx, |root, _window, _cx| {
                root.results = vec![clipboard_row];
                root.selected = 0;
            })
            .unwrap();
        window
            .update(cx, |root, window, cx| root.open_actions_menu(&OpenActionsMenu, window, cx))
            .unwrap();

        window.update(cx, |root, window, cx| root.confirm(&Confirm, window, cx)).unwrap();
        cx.run_until_parked();

        window
            .update(cx, |root, _window, _cx| {
                let menu = root.actions_menu.as_ref().expect("one Enter on a destructive action must only arm it");
                assert!(menu.confirm_armed);
            })
            .unwrap();

        // A second Enter actually performs it, which closes the menu (the
        // request itself fails against the disconnected test client, but
        // `perform_activation` closes the menu synchronously before it
        // even sends the request — see that method's own body).
        window.update(cx, |root, window, cx| root.confirm(&Confirm, window, cx)).unwrap();
        window.update(cx, |root, _window, _cx| assert!(root.actions_menu.is_none())).unwrap();
    }

    #[gpui::test]
    fn moving_the_menu_selection_disarms_a_pending_destructive_confirmation(cx: &mut TestAppContext) {
        let window = test_root(cx);
        cx.run_until_parked();
        let mut clipboard_row = item_with_id("clipboard", "hello");
        clipboard_row.actions = vec![
            ItemAction { id: "delete".into(), label: "Delete".into(), destructive: true },
            ItemAction { id: "paste".into(), label: "Paste".into(), destructive: false },
        ];
        window
            .update(cx, |root, _window, _cx| {
                root.results = vec![clipboard_row];
                root.selected = 0;
            })
            .unwrap();
        window
            .update(cx, |root, window, cx| root.open_actions_menu(&OpenActionsMenu, window, cx))
            .unwrap();
        window.update(cx, |root, window, cx| root.confirm(&Confirm, window, cx)).unwrap();
        window
            .update(cx, |root, _window, _cx| assert!(root.actions_menu.as_ref().unwrap().confirm_armed))
            .unwrap();

        window
            .update(cx, |root, window, cx| root.select_next(&SelectNext, window, cx))
            .unwrap();

        window
            .update(cx, |root, _window, _cx| {
                assert!(!root.actions_menu.as_ref().unwrap().confirm_armed, "moving off the armed action must disarm it");
            })
            .unwrap();
    }

    // --- Motion/floating-layer craft pass: the actions menu's trigger-click
    // race, Escape's menu-before-mode-or-panel ordering, the stale-menu
    // reset fix, and the "still searching" tell's own timing.

    fn clipboard_row_with_a_paste_action(id: &str) -> SearchItem {
        let mut row = item_with_id("clipboard", id);
        row.actions = vec![ItemAction { id: "paste".into(), label: "Paste".into(), destructive: false }];
        row
    }

    #[gpui::test]
    fn clicking_the_trigger_while_the_menu_is_open_does_not_reopen_it(cx: &mut TestAppContext) {
        // The race `data/neko-comet-design/report.md` recommendation 1
        // describes (comet's `popover.rs:67-180`, reimplemented here in
        // neko's own terms — see `handle_actions_menu_trigger_click`'s doc
        // comment for the full mechanism): the footer trigger sits outside
        // the menu card, so a click on it while the menu is open fires the
        // card's own outside-close handler (capture phase) on the very same
        // physical mouse-down, strictly before the trigger's own click
        // handler (bubble phase, on mouse-up) ever runs — `gpui` completes
        // every capture-phase listener across the whole window before any
        // bubble-phase listener starts (confirmed by reading
        // `gpui-0.2.2/src/window.rs`'s `dispatch_mouse_event`). Exercised
        // here by calling the three real handler methods directly, in
        // exactly that dispatch order, rather than via a simulated window
        // click — matching this suite's own established convention
        // (`test_root`'s doc comment) of proving state-machine correctness
        // headlessly, the same way comet's own equivalent test
        // (`trigger_press_note_distinguishes_dismiss_from_open`) is a pure
        // state test with no simulated mouse event either.
        let window = test_root(cx);
        cx.run_until_parked();
        window
            .update(cx, |root, _window, _cx| {
                root.results = vec![clipboard_row_with_a_paste_action("hello")];
                root.selected = 0;
            })
            .unwrap();
        window
            .update(cx, |root, window, cx| root.open_actions_menu(&OpenActionsMenu, window, cx))
            .unwrap();
        window
            .update(cx, |root, _window, _cx| assert!(root.actions_menu.is_some(), "setup: the menu must be open"))
            .unwrap();

        window
            .update(cx, |root, window, cx| {
                // Capture phase: the note fires before anything mutates
                // `actions_menu` for this gesture.
                root.note_actions_menu_mouse_down(&MouseDownEvent::default(), window, cx);
                // Still capture phase: the card's own outside-close handler
                // fires next (the trigger is outside the card), closing the
                // menu.
                root.close_actions_menu_from_outside_click(&MouseDownEvent::default(), window, cx);
                // Bubble phase, on mouse-up: the trigger's own click.
                root.handle_actions_menu_trigger_click(&ClickEvent::default(), window, cx);
            })
            .unwrap();

        window
            .update(cx, |root, _window, _cx| {
                assert!(
                    root.actions_menu.is_none(),
                    "the trigger's own click must not reopen what the outside click in the same gesture just closed"
                );
            })
            .unwrap();
    }

    #[gpui::test]
    fn clicking_the_trigger_when_the_menu_is_already_closed_opens_it(cx: &mut TestAppContext) {
        // The normal-path counterpart to the race test above — the guard
        // must not suppress a genuine open when nothing closed it first.
        let window = test_root(cx);
        cx.run_until_parked();
        window
            .update(cx, |root, _window, _cx| {
                root.results = vec![clipboard_row_with_a_paste_action("hello")];
                root.selected = 0;
            })
            .unwrap();
        window
            .update(cx, |root, _window, _cx| assert!(root.actions_menu.is_none(), "setup: the menu must start closed"))
            .unwrap();

        window
            .update(cx, |root, window, cx| {
                root.note_actions_menu_mouse_down(&MouseDownEvent::default(), window, cx);
                // No outside-close handler fires this time — the menu was
                // already closed, so there was nothing for it to dismiss.
                root.handle_actions_menu_trigger_click(&ClickEvent::default(), window, cx);
            })
            .unwrap();

        window
            .update(cx, |root, _window, _cx| {
                assert!(root.actions_menu.is_some(), "a click on the trigger with the menu closed must open it");
            })
            .unwrap();
    }

    #[gpui::test]
    fn escape_closes_the_actions_menu_before_exiting_an_active_mode(cx: &mut TestAppContext) {
        // "Escape closes the menu first, panel second" — the mode-active
        // case is the one this headless suite can observe directly (window
        // hide isn't visible from here, same limitation the pre-existing
        // `escape_exits_the_mode_instead_of_hiding_when_one_is_active` test
        // already has for the panel-level case). `handle_dismiss`'s own
        // early-return structure makes the two mutually exclusive per call,
        // so proving "the mode is still active after the first Escape"
        // proves the menu branch, not the mode branch, actually ran.
        let window = test_root(cx);
        cx.run_until_parked();

        window
            .update(cx, |root, _window, _cx| {
                root.results = vec![command_item("clipboard")];
                root.selected = 0;
            })
            .unwrap();
        window
            .update(cx, |root, window, cx| root.confirm(&Confirm, window, cx))
            .unwrap();
        cx.run_until_parked();

        window
            .update(cx, |root, _window, _cx| {
                root.results = vec![clipboard_row_with_a_paste_action("hello")];
                root.selected = 0;
            })
            .unwrap();
        window
            .update(cx, |root, window, cx| root.open_actions_menu(&OpenActionsMenu, window, cx))
            .unwrap();
        window
            .update(cx, |root, _window, _cx| {
                assert!(root.actions_menu.is_some(), "setup: the menu must be open before Escape");
                assert!(root.active_mode.is_some(), "setup: still inside the mode before Escape");
            })
            .unwrap();

        window
            .update(cx, |root, window, cx| root.handle_dismiss(&crate::DismissWindow, window, cx))
            .unwrap();
        window
            .update(cx, |root, _window, _cx| {
                assert!(root.actions_menu.is_none(), "the first Escape must close the menu");
                assert!(root.active_mode.is_some(), "the first Escape must not also exit the mode in the same press");
            })
            .unwrap();

        window
            .update(cx, |root, window, cx| root.handle_dismiss(&crate::DismissWindow, window, cx))
            .unwrap();
        window
            .update(cx, |root, _window, _cx| {
                assert!(root.active_mode.is_none(), "the second Escape, with the menu already closed, must exit the mode");
            })
            .unwrap();
    }

    #[gpui::test]
    fn reset_for_summon_closes_a_stale_open_actions_menu_even_with_no_mode_active(cx: &mut TestAppContext) {
        // The gap this task closed in `reset_for_summon`: the window losing
        // activation (`main.rs`'s `cx.observe_window_activation`) hides the
        // whole panel without ever routing through `handle_dismiss`'s own
        // "close the menu first" logic, so a menu left open on a *root-list*
        // row (no mode involved at all) used to survive into the next
        // summon.
        let window = test_root(cx);
        cx.run_until_parked();
        window
            .update(cx, |root, _window, _cx| {
                root.results = vec![clipboard_row_with_a_paste_action("hello")];
                root.selected = 0;
            })
            .unwrap();
        window
            .update(cx, |root, window, cx| root.open_actions_menu(&OpenActionsMenu, window, cx))
            .unwrap();
        window
            .update(cx, |root, _window, _cx| assert!(root.actions_menu.is_some(), "setup"))
            .unwrap();

        window.update(cx, |root, window, cx| root.reset_for_summon(window, cx)).unwrap();

        window
            .update(cx, |root, _window, _cx| {
                assert!(root.actions_menu.is_none(), "a fresh summon must never resume a stale open actions menu");
            })
            .unwrap();
    }

    #[gpui::test]
    fn a_still_pending_generation_shows_the_searching_tell(cx: &mut TestAppContext) {
        // Driven directly against `reveal_searching_tell_if_still_pending`
        // (rather than through `run_search`'s own real request and a real
        // `advance_clock`) so this is deterministic regardless of how fast
        // the test client's own connection resolves — see that client's
        // "resolves immediately with `NotConnected` when disconnected" doc
        // comment (`neko-client/src/lib.rs`), which makes a genuinely
        // still-in-flight request unreproducible through the real path in
        // this headless harness.
        let window = test_root(cx);
        cx.run_until_parked();

        window
            .update(cx, |root, _window, cx| {
                root.generation = 7;
                root.pending_search_generation = Some(7);
                root.reveal_searching_tell_if_still_pending(7, cx);
            })
            .unwrap();

        window
            .update(cx, |root, _window, _cx| {
                assert!(root.searching, "a generation still marked pending must show the tell");
            })
            .unwrap();
    }

    #[gpui::test]
    fn an_already_resolved_generation_never_shows_the_searching_tell_later(cx: &mut TestAppContext) {
        // The bug `pending_search_generation` exists to prevent: without
        // it, a response that lands well within the delay (the common,
        // fast case) would still see the delayed-reveal task fire later for
        // the same generation and incorrectly flip the tell on for an
        // already-answered query. `self.generation == generation` alone
        // can't tell the difference — only `pending_search_generation`,
        // cleared the instant a response lands, can.
        let window = test_root(cx);
        cx.run_until_parked();

        window
            .update(cx, |root, _window, cx| {
                root.generation = 7;
                root.pending_search_generation = None; // already resolved
                root.reveal_searching_tell_if_still_pending(7, cx);
            })
            .unwrap();

        window
            .update(cx, |root, _window, _cx| {
                assert!(!root.searching, "an already-resolved generation must not have the tell flip on later");
            })
            .unwrap();
    }
}
