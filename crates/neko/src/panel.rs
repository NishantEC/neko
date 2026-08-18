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
    SharedString, Window, actions, div, img, prelude::*, px, size,
};
use neko_client::NekoClient;
use neko_protocol::{Glyph, Icon, ItemAction, Request, Response, SearchItem};

use crate::accessibility::AccessibilityChecker;
use crate::modes::{self, ModeChrome};
use crate::text_field::{ContentChanged, DEFAULT_PLACEHOLDER, TextField};
use crate::theme;

actions!(panel, [SelectNext, SelectPrevious, Confirm, OpenActionsMenu]);

const RESULT_LIMIT: usize = 8;
/// A mode's own list wants "as many of this one provider's matches as it
/// can consider," not the shared, multi-provider root-list budget — a
/// generous cap since the daemon does the real trimming to what actually
/// fits on screen (`fit_mode_list`, this file), same as `RESULT_LIMIT`
/// does for the root list's own `fit_within_budget`.
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
        cx: &mut App,
    ) -> Entity<Self> {
        cx.new(|cx| Self::build(client, accessibility, translucent, cx))
    }

    /// The real construction logic, factored out of [`new`](Self::new) so a
    /// test can build a `Root` directly inside `TestAppContext::add_window`'s
    /// own closure (which needs a plain `V`, not an `Entity<V>` — `new`
    /// itself wraps this in `cx.new(...)`) without duplicating any of it.
    fn build(
        client: NekoClient,
        accessibility: Rc<dyn AccessibilityChecker>,
        translucent: bool,
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
            connected: true,
            row_icon_cache,
            active_mode: None,
            actions_menu: None,
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
    /// resume the mode they were in. This is also what keeps the window's
    /// own width correct before it's shown again — narrowing back to
    /// `PANEL_WIDTH_PX` happens here, synchronously, before `main.rs`
    /// activates the window, so there's no visible wide-then-narrow flash.
    pub fn reset_for_summon(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(mode) = self.active_mode.take() {
            self.text_field.update(cx, |field, cx| field.set_placeholder(DEFAULT_PLACEHOLDER, cx));
            self.actions_menu = None;
            if mode.chrome.has_detail {
                self.resize_panel(window, theme::PANEL_WIDTH_PX);
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

    /// The one call site that actually touches the real `NSWindow` — see
    /// `display_placement::resize_and_recenter`'s own doc comment for why
    /// this is a synchronous raw AppKit call rather than `gpui::Window::
    /// resize` plus a separate reposition. Best-effort: an error (no raw
    /// window handle — the same conditions `main.rs`'s own
    /// `reposition_to_cursor_display` already tolerates, e.g. under a
    /// headless test window) is logged and otherwise ignored, never a panic
    /// and never a blocked mode transition — the panel's own `div` width
    /// (`Render::render`, below) still changes either way, so the *content*
    /// is always internally consistent even on the rare path where the real
    /// window fails to follow it.
    fn resize_panel(&self, window: &mut Window, new_width: f32) {
        let target = size(px(new_width), px(PANEL_HEIGHT_PX));
        if let Err(e) = crate::display_placement::resize_and_recenter(window, target) {
            eprintln!("neko: could not resize/recenter the panel for a mode transition: {e}");
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
        // The mode seam: while a mode is active, every keystroke scopes to
        // its own provider (`Request::Search`'s `provider` field) with a
        // generous limit — the merged root-list budget/reservation logic
        // (`fit_within_budget`) doesn't apply at all here, `fit_mode_list`
        // does instead (see that function's own doc comment for why it's a
        // different, simpler shape).
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
                return;
            };
            let _ = this.update(cx, |root, cx| {
                if root.generation == generation {
                    root.results = if root.active_mode.is_some() {
                        fit_mode_list(items, CONTENT_AREA_MIN_HEIGHT_PX)
                    } else {
                        fit_within_budget(items, CONTENT_AREA_MIN_HEIGHT_PX)
                    };
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
        cx.notify();
    }

    fn confirm(&mut self, _: &Confirm, window: &mut Window, cx: &mut Context<Self>) {
        if self.actions_menu.is_some() {
            self.confirm_menu_action(cx);
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
                let _ = cx.update(|cx| cx.hide());
            }
        })
        .detach();
    }

    /// Enters `mode_id`'s mode: saves the current query so `exit_mode` can
    /// restore it, clears the field to start the mode's own list fresh,
    /// swaps the placeholder, and — if the mode wants a detail pane —
    /// widens the real window. A no-op if `mode_id` doesn't name a
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
        self.actions_menu = None;
        if chrome.has_detail {
            self.resize_panel(window, theme::PANEL_WIDTH_WITH_DETAIL_PX);
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
    /// placeholder, closes any open actions menu, and narrows the window
    /// back if it had widened.
    fn exit_mode(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(mode) = self.active_mode.take() else {
            return;
        };
        self.actions_menu = None;
        self.selected = 0;
        if mode.chrome.has_detail {
            self.resize_panel(window, theme::PANEL_WIDTH_PX);
        }
        self.text_field.update(cx, |field, cx| {
            field.set_placeholder(DEFAULT_PLACEHOLDER, cx);
            field.set_content(&mode.saved_query, cx);
        });
        cx.notify();
    }

    /// `⌘K` — opens the actions menu for the currently selected row, if it
    /// has any (`SearchItem::actions`); a no-op for a row with none (apps,
    /// files, settings, commands today), which is why the footer's
    /// "Actions ⌘K" label is always shown rather than conditionally hidden
    /// — matching Raycast's own convention of a menu that's simply empty
    /// (here: inert) rather than a control that disappears depending on
    /// selection.
    fn open_actions_menu(&mut self, _: &OpenActionsMenu, _window: &mut Window, cx: &mut Context<Self>) {
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

    /// Enter, while the actions menu is open. A destructive action
    /// (`ItemAction::destructive`) needs a *second* Enter to actually run —
    /// the first just arms it (re-rendered with a "press again to confirm"
    /// label, `render_actions_menu`) — so a single mis-keyed Enter on
    /// "Delete" can never silently destroy an entry; moving the menu
    /// selection at all (`select_next`/`select_previous`) disarms it again,
    /// so the confirmation can't survive being scrolled past and back.
    fn confirm_menu_action(&mut self, cx: &mut Context<Self>) {
        let Some(menu) = &mut self.actions_menu else { return };
        let Some(action) = menu.actions.get(menu.selected).cloned() else {
            self.actions_menu = None;
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
        self.actions_menu = None;
        cx.notify();
        let request = Request::Activate { kind, id, action: Some(action.id) };
        self.perform_activation(request, false, cx);
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
        if self.actions_menu.take().is_some() {
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
        let mut root = div()
            .key_context("Panel")
            .on_action(cx.listener(Self::select_next))
            .on_action(cx.listener(Self::select_previous))
            .on_action(cx.listener(Self::confirm))
            .on_action(cx.listener(Self::open_actions_menu))
            .on_action(cx.listener(Self::handle_dismiss))
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
            .shadow_lg()
            .overflow_hidden()
            .child(self.render_input_row(cx))
            .child(match &self.active_mode {
                Some(mode) => self.render_mode_content(mode).into_any_element(),
                None => self.render_content_area(cx, query_is_empty).into_any_element(),
            })
            .child(self.render_footer());
        if let Some(menu) = self.actions_menu.clone() {
            root = root.child(self.render_actions_menu(&menu));
        }
        root
    }
}

impl Root {
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
                    .child("Actions  ⌘K"),
            )
    }

    /// The two-column mode view (`data/neko-design/mockups/
    /// 12-first-clipboard-use.html`): a fixed-width filtered list on the
    /// left, a preview + info pane on the right when
    /// `ModeChrome::has_detail` — otherwise just the list, full width. Sits
    /// where `render_content_area` sits for the root list; same content-area
    /// height budget (`CONTENT_AREA_MIN_HEIGHT_PX`), only ever a width
    /// change between the two.
    fn render_mode_content(&self, mode: &ActiveMode) -> impl IntoElement {
        div()
            .flex()
            .flex_1()
            .min_h(px(0.))
            .overflow_hidden()
            .child(self.render_mode_list())
            .when(mode.chrome.has_detail, |el| el.child(self.render_mode_detail()))
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
    fn render_mode_list(&self) -> impl IntoElement {
        let mut container = div()
            .flex()
            .flex_col()
            .flex_shrink_0()
            .w(px(theme::MODE_LIST_COLUMN_WIDTH_PX))
            .h_full()
            .overflow_hidden()
            .px_2()
            .border_r_1()
            .border_color(theme::BORDER_HAIRLINE)
            .image_cache(self.row_icon_cache.clone());

        if self.results.is_empty() {
            return container.child(render_empty_state_message("No matching entries."));
        }

        let mut current_group: Option<&Option<String>> = None;
        for (idx, item) in self.results.iter().enumerate() {
            if current_group != Some(&item.group_label) {
                if let Some(label) = &item.group_label {
                    container = container.child(section_header(label.clone()));
                }
                current_group = Some(&item.group_label);
            }
            container = container.child(self.render_row(idx, item));
        }
        container
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
    /// tokens. Anchored above the footer's own "Actions ⌘K" label, the
    /// control that opens it.
    fn render_actions_menu(&self, menu: &ActionsMenuState) -> impl IntoElement {
        div()
            .absolute()
            .bottom(px(theme::FOOTER_HEIGHT_PX + 8.))
            .right(px(theme::PANEL_RADIUS_PX))
            .w(px(200.))
            .flex()
            .flex_col()
            .p_1()
            .gap(px(1.))
            .rounded(px(theme::ROW_RADIUS_PX))
            .bg(theme::SURFACE_RAISED)
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
            }))
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

/// The mode list's own budget-fitting pass — same "never a partial row,
/// never a dangling header" invariant `fit_within_budget` enforces for the
/// root list, but a simpler shape: a mode's list is always exactly one
/// provider's results (no cross-provider crowd-out to guard against), just
/// grouped by `SearchItem::group_label` (time buckets) instead of `kind`
/// (provider identity). Groups are spent front-to-back — the most recent
/// group can't be crowded out because there's nothing recency-ranked ahead
/// of it to crowd it, matching "most recent copies first" being exactly
/// what a captain wants visible when the budget is tight.
fn fit_mode_list(results: Vec<SearchItem>, budget_px: f32) -> Vec<SearchItem> {
    let groups = group_by_group_label(results);
    let mut used_so_far = 0.0;
    let mut out = Vec::new();
    for group in groups {
        let (kept, used_px) = fit_section(&group, budget_px - used_so_far);
        used_so_far += used_px;
        let group_len = group.len();
        out.extend(group.into_iter().take(kept));
        if kept < group_len {
            break;
        }
    }
    out
}

/// Splits `results` into contiguous same-`group_label` runs, preserving
/// order — `fit_mode_list`'s own grouping, and `Root::render_mode_list`'s
/// (which renders one `section_header` per group boundary, mirroring
/// `group_into_sections`/`render_content_area`'s identical shape for
/// `kind` above).
fn group_by_group_label(results: Vec<SearchItem>) -> Vec<Vec<SearchItem>> {
    let mut groups: Vec<Vec<SearchItem>> = Vec::new();
    for item in results {
        match groups.last_mut() {
            Some(group) if group.last().is_some_and(|last| last.group_label == item.group_label) => group.push(item),
            _ => groups.push(vec![item]),
        }
    }
    groups
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

    // --- Commands and modes: fit_mode_list / group_by_group_label ---

    fn mode_item(group: Option<&str>, id: &str) -> SearchItem {
        SearchItem { group_label: group.map(str::to_string), ..item_with_id("clipboard", id) }
    }

    #[test]
    fn group_by_group_label_splits_contiguous_runs_by_group() {
        let items = vec![
            mode_item(Some("Today"), "a"),
            mode_item(Some("Today"), "b"),
            mode_item(Some("Yesterday"), "c"),
        ];
        let groups = group_by_group_label(items);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].len(), 2);
        assert_eq!(groups[1].len(), 1);
    }

    #[test]
    fn fit_mode_list_never_shows_a_dangling_header_or_a_partial_row() {
        // 3 "Today" entries + 3 "Yesterday" entries against a budget that
        // only fits one full group plus a partial second one.
        let budget = theme::SECTION_HEADER_HEIGHT_PX * 2.0 + theme::RESULT_ROW_HEIGHT_PX * 4.0;
        let mut items: Vec<SearchItem> = (0..3).map(|i| mode_item(Some("Today"), &format!("t{i}"))).collect();
        items.extend((0..3).map(|i| mode_item(Some("Yesterday"), &format!("y{i}"))));

        let fitted = fit_mode_list(items, budget);

        let today_kept = fitted.iter().filter(|i| i.group_label.as_deref() == Some("Today")).count();
        let yesterday_kept = fitted.iter().filter(|i| i.group_label.as_deref() == Some("Yesterday")).count();
        assert_eq!(today_kept, 3, "the first (most recent) group must fit in full before any budget goes elsewhere");
        assert_eq!(yesterday_kept, 1, "leftover budget after the full first group is exactly one more row");
        // No provider-crowd-out reservation exists for mode lists (unlike
        // `fit_within_budget`) — a later group can be starved entirely by
        // an earlier, larger one, which is the correct, simpler behavior
        // for "most recent first" (see `fit_mode_list`'s own doc comment).
    }

    #[test]
    fn fit_mode_list_keeps_everything_that_already_fits() {
        let items = vec![mode_item(Some("Today"), "a"), mode_item(Some("Yesterday"), "b")];
        let fitted = fit_mode_list(items.clone(), CONTENT_AREA_MIN_HEIGHT_PX);
        assert_eq!(fitted, items);
    }

    #[test]
    fn fit_mode_list_on_an_empty_list_stays_empty() {
        assert_eq!(fit_mode_list(Vec::new(), CONTENT_AREA_MIN_HEIGHT_PX), Vec::new());
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
        cx.add_window(|_window, cx| Root::build(client, accessibility, true, cx))
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
}
