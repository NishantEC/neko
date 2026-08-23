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
//! **The panel `div` itself is now always exactly this same width too, root
//! list and clipboard mode alike** (`AGENTS.md`, "One constant panel
//! width," a later captain override of the frozen design's original
//! two-width rule) — `render` no longer varies `panel_width` by mode, and
//! there is no longer a centering stage element, margin divs, or a
//! `Root::update_background_bounds` call: the panel always fills the
//! window exactly, so the native material backdrop `install` puts in place
//! (sized to the window's own full `contentView` bounds, once, at startup)
//! never needs repositioning for a mode transition either.

use std::path::PathBuf;
use std::rc::Rc;

use gpui::{
    Anchor, AnyElement, App, ClickEvent, Context, CursorStyle, Entity, FocusHandle, Focusable,
    MouseDownEvent, Render, ScrollHandle, SharedString, Window, actions, anchored, deferred,
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
/// The panel is a fixed height for the process's whole lifetime (the real
/// `NSWindow` is never resized — `AGENTS.md`, "Mode view resize seam"), so
/// the agent grid can only ever take space *from* the rows. A grid taller
/// than the content area would leave no rows at all, which is a build error
/// rather than something to discover at runtime.
const _: () = assert!(theme::AGENT_GRID_HEIGHT_PX < CONTENT_AREA_MIN_HEIGHT_PX);

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
    /// window. When `true`, the panel fills with `theme::active().surface_panel_translucent`
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
    /// see `crate::modes`'s module doc comment for the full concept. Empty
    /// is the ordinary root list.
    ///
    /// A single `Option`: no mode nests. Preferences is a real window
    /// (`crate::preferences`), not a mode, so nothing here ever needed to.
    active_mode: Option<ActiveMode>,
    /// Running agents, lifted out of `results` into the grid above the
    /// search field. They are moved rather than copied: the same agent in
    /// both places would be two rows for one thing, and Enter would have to
    /// pick one of them.
    agent_tiles: Vec<SearchItem>,
    /// Which grid tile has the keyboard, if the selection is up in the grid
    /// rather than down in the list.
    ///
    /// **The grid is navigated with the same Up/Down as the list, not with
    /// Left/Right**, and that is forced rather than chosen: Left and Right
    /// are bound to the search field's own cursor movement (`main.rs`'s
    /// `cx.bind_keys`, `"TextField"` context), so a grid that claimed them
    /// would break typing to reach it. Treating the tiles as rows that
    /// happen to sit above the input costs no new keys at all: Up from the
    /// first result walks into the grid, Down off the last tile walks back
    /// into the list.
    grid_selected: Option<usize>,
    /// The shared pulse clock (`motion::PulseClock`), observed so a tick
    /// repaints this panel. Held as an entity rather than read per frame so
    /// the subscription can exist at all.
    pulse: Entity<motion::PulseClock>,
    /// Opens the Preferences window. Injected for the same two reasons
    /// [`AppearanceSetter`] is: GPUI's test-platform window panics rather
    /// than erroring on real window operations, so an unconditional call
    /// here would take every panel test down with it — and the pieces the
    /// window needs (the client, the live hotkey registrar, the
    /// single-window slot) belong to `main.rs`, not to a list of results.
    open_preferences: PreferencesOpener,
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
    /// `Some(generation)` once that generation's *partial* frame has been
    /// applied — i.e. the fast providers' results are on screen and a
    /// deferred one is still running. Two things read it: the complete
    /// frame, to know it must merge rather than replace
    /// (`apply_search_results`), and the "still searching" tell, which is
    /// this state made visible rather than a timer's guess about it.
    /// Cleared at the top of every `run_search`, so it can never describe
    /// a generation other than the current one.
    partial_generation: Option<u64>,
    /// Verification-only (`evidence::bench_search_query`,
    /// `NEKO_BENCH_SEARCH`): when the current generation's request was
    /// dispatched, so `apply_search_results` can report keystroke-to-render
    /// latency for the frame that actually lands. Always recorded — an
    /// `Instant::now()` per keystroke is far below the noise floor of the
    /// thing being measured — but only ever *read* when the hook is on, so
    /// normal operation prints nothing.
    search_dispatched_at: Option<(u64, std::time::Instant)>,
    /// Tracks the mode list's own scroll position (`render_mode_list`) —
    /// the root list never scrolls (still budget-fit, `fit_within_budget`,
    /// per this module's "v1 simplification" doc comment above), so this is
    /// only ever read/written while a mode is active. One persistent handle
    /// reused across mode entries rather than a fresh one each time, so
    /// `edge_fade::scroll_edge_fade` and `select_next`/`select_previous`'s
    /// own scroll-into-view calls are always looking at the same state.
    mode_scroll: ScrollHandle,
    /// The appearance last pushed to the real `NSWindow` — see
    /// [`Root::sync_window_appearance`]. `None` until the first frame, so a
    /// process that starts on a light theme sets it before anything is ever
    /// painted rather than one frame late.
    applied_appearance: Option<theme::Appearance>,
    /// How this panel reaches AppKit to keep the window's `NSAppearance` in
    /// step with the active theme — injected rather than called directly, the
    /// same shape `accessibility` already uses for `AXIsProcessTrusted`.
    ///
    /// Two reasons, and the second is not optional: it lets a headless
    /// `#[gpui::test]` assert *what appearance was asked for* without a real
    /// window, and GPUI's own test-platform window `unimplemented!()`s
    /// (panics, rather than returning `Err`) on `window_handle()`, so any
    /// unconditional native call from `render` would take every panel test
    /// down with it. Every other native call in this crate lives in
    /// `main.rs`, which tests never run; this is the first one on the render
    /// path.
    appearance_setter: AppearanceSetter,
}

/// See [`Root::appearance_setter`]. The real one is
/// `material::set_window_appearance`; tests inject a recorder.
/// Opens the Preferences window and orders the summon panel out — see
/// [`Root::open_preferences`]. Takes the panel's own `Window` because
/// hiding *just that window* is a native call, and every native call on this
/// path has to sit behind the injection for tests to survive it.
pub type PreferencesOpener = Rc<dyn Fn(&Window, &mut App)>;

pub type AppearanceSetter = Rc<dyn Fn(&Window, theme::Appearance) -> Result<(), String>>;

/// The one piece of state a mode transition actually carries, beyond the
/// static `ModeChrome` — the query the root list had before entering, so
/// exiting can restore it exactly. `chrome` is `&'static` (looked up once
/// from `crate::modes::MODES` on entry), so this whole struct is `Copy`
/// apart from the owned `String`.
#[derive(Clone)]
struct ActiveMode {
    chrome: &'static ModeChrome,
    saved_query: String,
    /// The theme that was active when this mode was entered, so leaving can
    /// put it back. **This is what makes live preview safe to be live**: the
    /// panel really does become each theme as the selection moves, and
    /// Escape really does undo all of it.
    ///
    /// `None` for a mode that does not preview themes at all (clipboard
    /// history), and `None` again once a theme has been *confirmed* — from
    /// that moment there is nothing to revert to, because the previewed
    /// palette is the chosen one.
    ///
    /// Deliberately not "every mode records the theme and restores it on
    /// exit, which is a harmless no-op for modes that never changed it": that
    /// version turns leaving *any* mode into a global palette write, which is
    /// a real cross-effect for something that should have none.
    restore_theme: Option<&'static str>,
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
        appearance_setter: AppearanceSetter,
        open_preferences: PreferencesOpener,
        cx: &mut App,
    ) -> Entity<Self> {
        cx.new(|cx| {
            Self::build(client, accessibility, translucent, menu_frost, appearance_setter, open_preferences, cx)
        })
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
        appearance_setter: AppearanceSetter,
        open_preferences: PreferencesOpener,
        cx: &mut Context<Self>,
    ) -> Self {
        let pulse = motion::PulseClock::global(cx);
        // A tick is only worth anything if it repaints — see
        // `sync_pulse` for why it is the render pass, not this
        // subscription, that decides whether the clock runs at all.
        cx.observe(&pulse, |_root, _clock, cx| cx.notify()).detach();
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
            open_preferences,
            agent_tiles: Vec::new(),
            grid_selected: None,
            pulse,
            actions_menu: None,
            menu_open_before_this_press: false,
            searching: false,
            pending_search_generation: None,
            partial_generation: None,
            search_dispatched_at: None,
            mode_scroll: ScrollHandle::new(),
            applied_appearance: None,
            appearance_setter,
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
    /// resume the mode they were in. Since "One constant panel width" this
    /// no longer touches the native background material at all — the panel
    /// is the same width in and out of a mode, so there is nothing to
    /// narrow back.
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
        // Per-summon state, exactly like the mode and the menu above it: a
        // tile focused in one session must not still be focused in the next.
        self.grid_selected = None;
        if self.active_mode.take().is_some() {
            self.text_field.update(cx, |field, cx| field.set_placeholder(DEFAULT_PLACEHOLDER, cx));
            self.mode_scroll.set_offset(point(px(0.), px(0.)));
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

    /// Evidence/verification-only — reads the search field's current
    /// content back, for `evidence.rs`'s `NEKO_PROVE_TYPING` hook. Read-only:
    /// this is how that hook proves a character delivered into AppKit's
    /// responder chain actually landed in the field, rather than asserting it.
    pub fn query_for_evidence(&self, cx: &App) -> String {
        self.text_field.read(cx).content().to_string()
    }

    /// Evidence/verification-only — selects the whole current query
    /// (`TextField::select_all_for_evidence`, the exact logic ⌘A's real
    /// handler uses) so the rendered selection highlight
    /// (`theme::active().surface_selected`) shows up in a window-scoped screenshot,
    /// for `evidence.rs`'s `NEKO_SHOW_SELECTION` hook. Same "no synthetic OS
    /// input" reasoning as `set_query_for_evidence` above.
    pub fn select_query_for_evidence(&mut self, cx: &mut Context<Self>) {
        self.text_field.update(cx, |field, cx| field.select_all_for_evidence(cx));
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
        if self.active_mode().is_some() {
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
        self.partial_generation = None;
        self.search_dispatched_at = Some((generation, std::time::Instant::now()));
        let query = self.text_field.read(cx).content().to_string();
        let client = self.client.clone();
        // The mode seam: while a mode is active, every keystroke scopes to
        // its own provider (`Request::Search`'s `provider` field) with a
        // generous limit — the merged root-list budget/reservation logic
        // (`fit_within_budget`) doesn't apply at all here; the mode list
        // renders every returned item and scrolls instead (`render_mode_list`,
        // `edge_fade::scroll_edge_fade`).
        let mode_provider = self.active_mode().map(|m| m.chrome.provider_id.to_string());
        let limit = if mode_provider.is_some() { MODE_RESULT_LIMIT } else { RESULT_LIMIT };
        cx.spawn(async move |this, cx| {
            // Streaming, not a single `request`: the daemon answers a root
            // search in two frames whenever a slow provider is involved —
            // the fast providers' own results first (`complete: false`),
            // the full merged set once file search finishes
            // (`complete: true`). See `NekoClient::request_streaming` and
            // `AGENTS.md`'s "Two-phase search" section. A mode's own
            // provider-scoped search is always a single complete frame; the
            // loop below handles both shapes without branching on which.
            let mut stream = client.request_streaming(Request::Search { query, limit, provider: mode_provider });
            while let Some(response) = stream.next().await {
                // A request error (including a dead connection) is
                // deliberately *not* surfaced here — `results`/`selected`
                // just stay exactly as they were, per the design intent
                // below. The live "can't reach neko-daemon" signal itself
                // is `main.rs`'s poll of `NekoClient::is_connected()`
                // (`Root::set_connected`), which doesn't depend on a search
                // having been attempted at all — see that method's doc
                // comment for why a request failing here is the wrong place
                // to decide connection state.
                let Response::SearchResults { items, complete } = response else { continue };
                let applied = this.update(cx, |root, cx| {
                    if root.generation != generation {
                        return;
                    }
                    root.apply_search_results(items, complete, generation, cx);
                });
                if applied.is_err() {
                    return;
                }
            }
            // The stream ended — either after the frame that completed this
            // request, or because the connection dropped mid-request (which
            // is why this cannot live only on the `complete` branch above).
            // Clearing here is what stops a query that started slow and
            // then failed from leaving the tell showing forever, since
            // nothing else ever turns it back off for a generation that
            // never produces a complete frame.
            let _ = this.update(cx, |root, cx| {
                if root.generation == generation && root.pending_search_generation == Some(generation) {
                    root.searching = false;
                    root.pending_search_generation = None;
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

    /// Applies one search frame — the partial one, the complete one, or the
    /// single complete one a query with no deferred provider produces.
    ///
    /// **The partial frame replaces; the complete frame that follows one
    /// merges.** That asymmetry is the whole "no flicker, no reordering
    /// jump" requirement: by the time file results land, the captain has
    /// been reading (and possibly navigating) a real list for anywhere up
    /// to `files::QUERY_TIMEOUT`, and re-rendering the daemon's own
    /// authoritative order wholesale would visibly reshuffle it underneath
    /// them — `search::allocate` orders sections by content strength, so a
    /// decisive file match can legitimately sort *above* the Applications
    /// section that was already on screen. See [`merge_late_results`] for
    /// the rule that replaces that reshuffle.
    fn apply_search_results(&mut self, items: Vec<SearchItem>, complete: bool, generation: u64, cx: &mut Context<Self>) {
        if complete {
            self.searching = false;
            self.pending_search_generation = None;
        } else {
            // The real, non-timer signal that something is still coming:
            // results are on screen and at least one provider is still
            // running. `reveal_searching_tell_if_still_pending` reads this
            // rather than assuming a query that hasn't answered in
            // `SEARCHING_TELL_DELAY_MS` must still be pending.
            self.partial_generation = Some(generation);
        }

        // Re-read at apply time rather than snapshotting before the request
        // went out: between a partial frame and the complete one the
        // captain may have pressed Down, so "what is highlighted right now"
        // is the only correct thing for `resolve_selection` to follow.
        let previously_selected = self.results.get(self.selected).map(|item| (item.kind.clone(), item.id.clone()));

        // The mode list scrolls (`edge_fade::scroll_edge_fade` in
        // `render_mode_list`) rather than being budget-fit like the root
        // list — `items` is already capped at `MODE_RESULT_LIMIT` by the
        // request, and rendering all of it, letting overflow scroll, is
        // what makes the edge fade honest (see `edge_fade.rs`'s own module
        // doc comment). The root list never scrolls at all, so anything
        // that doesn't fit has to be dropped rather than clipped.
        // Live agents are lifted out of the list and into the grid above the
        // search field. Inside a mode this never applies — a mode is one
        // provider's own list, and the grid is a root-list affordance.
        let (tiles, items) = if self.active_mode().is_some() {
            (Vec::new(), items)
        } else {
            split_agent_tiles(items)
        };
        // A tile that no longer exists must not stay focused; clamp into the
        // new grid, or fall back to the list once it has emptied.
        self.agent_tiles = tiles;
        self.grid_selected = match self.grid_selected {
            Some(_) if self.agent_tiles.is_empty() => None,
            Some(tile) => Some(tile.min(self.agent_tiles.len() - 1)),
            None => None,
        };
        // **The grid's height comes out of the row budget, it is not added to
        // the panel.** `PANEL_HEIGHT_PX` is fixed for the process's whole
        // lifetime and the real `NSWindow` is never resized (`AGENTS.md`,
        // "Mode view resize seam"), so anything drawn above the input row is
        // space the rows no longer have. Getting this wrong does not look
        // like a layout bug — it looks like the last row being clipped by
        // `overflow_hidden`, which is the exact defect `fit_within_budget`
        // exists to prevent.
        let budget = CONTENT_AREA_MIN_HEIGHT_PX - self.agent_grid_height();
        self.results = if self.active_mode().is_some() {
            items
        } else if complete && self.partial_generation == Some(generation) {
            let anchor = std::mem::take(&mut self.results);
            merge_late_results(anchor, items, budget, self.selected)
        } else {
            fit_within_budget(items, budget)
        };

        let previous = previously_selected.as_ref().map(|(kind, id)| (kind.as_str(), id.as_str()));
        self.selected = resolve_selection(previous, &self.results);
        // Entering the theme mode must not immediately repaint the app in
        // whatever palette happens to sort first. Land on the one already in
        // use — which is also where a person expects the highlight to be —
        // and let arrowing away from it be the first thing that previews
        // anything.
        //
        // Gated on "the row that was highlighted a moment ago was not itself
        // a theme", which is exactly the *entering* case: `enter_mode` does
        // not clear `results`, so the first scoped response still sees the
        // root-list row (the `Themes` command) that was confirmed to get
        // here. Once the captain is filtering inside the mode the previous
        // row *is* a theme, `resolve_selection`'s ordinary follow-the-row
        // rule takes over, and previewing the top match is the point.
        let entering_the_theme_mode = previous.is_none_or(|(kind, _)| kind != "theme");
        if entering_the_theme_mode && self.active_mode().is_some_and(|m| m.chrome.provider_id == "theme") {
            let active = theme::active_theme().id;
            if let Some(index) = self.results.iter().position(|item| item.id == active) {
                self.selected = index;
            }
        }
        // Typing to filter moves the selection just as arrowing does, so it
        // previews too — `preview_selected_theme` is a no-op outside the
        // theme mode and when the selected row is already the live palette.
        self.preview_selected_theme();
        self.sync_mode_scroll_to_selection();
        self.report_search_latency(complete, generation, cx);
        cx.notify();
    }

    /// Prints one `neko: search-latency` line per applied frame when
    /// `NEKO_BENCH_SEARCH` is set — nothing at all otherwise.
    ///
    /// **What "render" means here, stated precisely rather than left to be
    /// assumed from the name**: the elapsed time from the keystroke's own
    /// `run_search` dispatch to the moment this frame's results are
    /// committed to `self.results` and `cx.notify()` schedules the repaint.
    /// It deliberately does *not* include GPUI's own frame cadence between
    /// that notify and the pixels changing (~8ms on this window, `AGENTS.md`'s
    /// "Summon latency" section) — that sits identically on top of every
    /// measurement, before and after this task, so including it would only
    /// add noise to the comparison the number exists to make.
    fn report_search_latency(&self, complete: bool, generation: u64, cx: &mut Context<Self>) {
        if !crate::evidence::log_search_latency() {
            return;
        }
        let Some((dispatched_generation, dispatched_at)) = self.search_dispatched_at else { return };
        if dispatched_generation != generation {
            return;
        }
        let phase = if complete { "complete" } else { "partial" };
        eprintln!(
            "neko: search-latency gen={generation} phase={phase} query={:?} rows={} elapsed_ms={:.2}",
            self.text_field.read(cx).content(),
            self.results.len(),
            dispatched_at.elapsed().as_secs_f64() * 1000.0,
        );
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
        // Down off the last tile lands on the first row; the grid and the
        // list are one continuous run as far as the arrow keys are concerned.
        if let Some(tile) = self.grid_selected {
            if tile + 1 < self.agent_tiles.len() {
                self.grid_selected = Some(tile + 1);
            } else {
                self.grid_selected = None;
                self.selected = 0;
            }
            cx.notify();
            return;
        }
        if !self.results.is_empty() {
            self.selected = (self.selected + 1).min(self.results.len() - 1);
            self.sync_mode_scroll_to_selection();
            if self.preview_selected_theme() {
                // A palette swap touches surfaces outside `Root`'s own
                // subtree (`TextField`'s custom element, the `⌘K` menu's
                // deferred floating layer), so a plain `cx.notify()` is not
                // enough on its own.
                _window.refresh();
            }
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
        if let Some(tile) = self.grid_selected {
            // Already at the top of everything — stay put rather than
            // wrapping to the bottom of the list, which would feel like the
            // selection teleported.
            self.grid_selected = Some(tile.saturating_sub(1));
            cx.notify();
            return;
        }
        // Up from the first row walks into the grid, landing on its last
        // tile — the one nearest the list, so the selection moves by one
        // visually rather than jumping across the whole strip.
        if self.selected == 0 && !self.agent_tiles.is_empty() {
            self.grid_selected = Some(self.agent_tiles.len() - 1);
            cx.notify();
            return;
        }
        self.selected = self.selected.saturating_sub(1);
        self.sync_mode_scroll_to_selection();
        if self.preview_selected_theme() {
            _window.refresh();
        }
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
        if self.active_mode().is_none() {
            return;
        }
        self.mode_scroll.scroll_to_item(mode_list_child_index(&self.results, self.selected));
    }

    fn confirm(&mut self, _: &Confirm, window: &mut Window, cx: &mut Context<Self>) {
        if self.actions_menu.is_some() {
            self.confirm_menu_action(window, cx);
            return;
        }
        // A focused tile owns Enter — the grid is part of the same
        // selection run, so the row underneath must not act instead.
        if let Some(tile) = self.grid_selected
            && let Some(item) = self.agent_tiles.get(tile).cloned()
        {
            self.perform_activation(
                Request::Activate { kind: item.kind, id: item.id, action: None },
                true,
                cx,
            );
            return;
        }
        let Some(item) = self.results.get(self.selected).cloned() else {
            return;
        };
        // A command row never reaches `Request::Activate` at all —
        // confirming it is a client-side UI transition, not a daemon
        // action (see `crate::modes`'s module doc comment).
        if let Some(mode_id) = item.enters_mode {
            // Preferences is a real window, not a mode — the one
            // `enters_mode` value that opens one. Everything else names a
            // mode; an unknown value resolves to nothing and is ignored.
            if mode_id == crate::preferences::PREFERENCES_MODE_ID {
                // The opener also dismisses the panel — that is one
                // operation, not two: the window is where the interaction
                // continues, and leaving the launcher floating over it would
                // be two competing surfaces for the same task. Both halves
                // live inside the injected closure because both are native
                // calls GPUI's test platform panics on.
                (self.open_preferences)(window, cx);
                return;
            }
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
        // Confirming a theme is confirming what is *already on screen* — the
        // preview applied it the moment the selection landed on it. All this
        // does is stop `exit_mode` from putting the old one back, and let the
        // daemon persist it. Cleared before the request is sent, not after:
        // the panel hides on success, and a captain who saw the palette they
        // picked survive Enter should not see it flicker back if the socket
        // is slow.
        if item.kind == "theme"
            && let Some(mode) = self.active_mode_mut()
        {
            mode.restore_theme = None;
        }
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
    /// restore it, clears the field to start the mode's own list fresh, and
    /// swaps the placeholder. Since "One constant panel width" the panel is
    /// already the mode's own width before and after this call, so this no
    /// longer touches the panel's size or the native background material at
    /// all — `ModeChrome::has_detail` now only decides whether
    /// `render_mode_content` renders a detail column, not how wide anything
    /// is. A no-op if `mode_id` doesn't name a registered mode (a
    /// stale/corrupted value) or a mode is already active (confirming a
    /// command row is only ever possible from the root list, since commands
    /// never appear inside a mode's own scoped search — but this guards the
    /// invariant rather than assuming it).
    /// Runs the shared pulse clock exactly while a live agent row is on
    /// screen, and stops it otherwise.
    ///
    /// **Called from `render`, deliberately.** The alternative — starting it
    /// in `run_search` when results contain a live row — cannot see the two
    /// cases that matter most: results that are present but not *painted*
    /// (`fit_within_budget` drops what does not fit), and a window that is
    /// hidden, which is what the panel is almost all of the time. A clock
    /// ticking behind a hidden window is the same defect this whole
    /// mechanism exists to prevent, just harder to notice.
    ///
    /// Returns an empty element so it can sit in the render tree; it paints
    /// nothing.
    fn sync_pulse(&self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        let wanted = window.is_window_active()
            && self.results.iter().any(|item| item.badge.as_deref() == Some("LIVE"));
        self.pulse.update(cx, |clock, cx| clock.set_running(wanted, cx));
        gpui::Empty
    }

    /// The height the grid is currently taking, and therefore the height
    /// the rows below it do not have. Zero when there is nothing to show, so
    /// a machine with no agents running loses no space at all.
    fn agent_grid_height(&self) -> f32 {
        if self.agent_tiles.is_empty() { 0.0 } else { theme::AGENT_GRID_HEIGHT_PX }
    }

    /// The grid of running agents, above the search field.
    ///
    /// Above the field rather than in the list because it answers a
    /// different question: the list is "what did you ask for", this is "what
    /// is happening without you". It is only ever drawn when something is
    /// genuinely running, so the resting panel is unchanged.
    ///
    /// Tiles are laid out in a single row that wraps, sized by
    /// `AGENT_TILE_MIN_WIDTH_PX`, so one agent gets a wide tile and four get
    /// four narrow ones without a column count being hard-coded.
    fn render_agent_grid(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let intensity = self.pulse.read(cx).intensity();
        let mut grid = div()
            .flex()
            .flex_wrap()
            .gap(px(8.))
            // `px_5`, matching the input row directly beneath it, so a tile's
            // left edge lines up with the search glyph.
            .px_5()
            .pt(px(12.))
            .pb(px(8.))
            .h(px(theme::AGENT_GRID_HEIGHT_PX))
            .overflow_hidden();
        for (index, item) in self.agent_tiles.iter().enumerate() {
            let focused = self.grid_selected == Some(index);
            let kind = item.kind.clone();
            let id = item.id.clone();
            // The live dot breathes on the same shared clock the LIVE badge
            // uses — one clock for the app, never a per-tile animation.
            let mut dot = theme::active().state_success;
            dot.a = 0.45 + 0.55 * intensity;
            grid = grid.child(
                div()
                    .id(SharedString::from(format!("agent-tile-{id}")))
                    .flex()
                    .flex_1()
                    .min_w(px(0.))
                    .flex_col()
                    .justify_center()
                    .gap(px(2.))
                    .h(px(theme::AGENT_TILE_HEIGHT_PX))
                    .px(px(10.))
                    .rounded(px(theme::ROW_RADIUS_PX))
                    .bg(if focused {
                        theme::active().surface_selected
                    } else {
                        theme::active().surface_input
                    })
                    .border_1()
                    .border_color(if focused {
                        theme::active().border_hairline_strong
                    } else {
                        theme::active().border_hairline
                    })
                    .cursor_pointer()
                    .on_click(cx.listener(move |root, _event, _window, cx| {
                        root.perform_activation(
                            Request::Activate { kind: kind.clone(), id: id.clone(), action: None },
                            true,
                            cx,
                        );
                    }))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .child(div().w(px(6.)).h(px(6.)).rounded(px(3.)).bg(dot).flex_shrink_0())
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.))
                                    .overflow_hidden()
                                    .text_size(px(12.5))
                                    .text_color(theme::active().text_primary)
                                    .child(SharedString::from(item.title.clone())),
                            ),
                    )
                    .children(item.subtitle.clone().map(|subtitle| {
                        div()
                            .overflow_hidden()
                            .text_size(px(11.))
                            .text_color(theme::active().text_tertiary)
                            .child(SharedString::from(subtitle))
                    })),
            );
        }
        grid
    }

    fn active_mode(&self) -> Option<&ActiveMode> {
        self.active_mode.as_ref()
    }

    fn active_mode_mut(&mut self) -> Option<&mut ActiveMode> {
        self.active_mode.as_mut()
    }

    fn enter_mode(&mut self, mode_id: &str, window: &mut Window, cx: &mut Context<Self>) {
        // Entering a mode from inside one would capture the *mode's* query
        // as `saved_query`, so exiting would restore the wrong text. Not
        // reachable today (commands only appear in the root list), but the
        // invariant is guarded rather than assumed.
        if self.active_mode.is_some() {
            return;
        }
        let Some(chrome) = modes::chrome_for(mode_id) else {
            return;
        };
        let saved_query = self.text_field.read(cx).content().to_string();
        self.active_mode = Some(ActiveMode {
            chrome,
            saved_query,
            restore_theme: (chrome.provider_id == "theme").then(|| theme::active_theme().id),
        });
        self.selected = 0;
        self.mode_scroll.set_offset(point(px(0.), px(0.)));
        self.close_actions_menu(window);
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
    /// placeholder, and closes any open actions menu. Since "One constant
    /// panel width" the panel doesn't narrow back here either — see
    /// `enter_mode`'s own doc comment.
    fn exit_mode(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(mode) = self.active_mode.take() else {
            return;
        };
        // Undo whatever the live preview applied. A no-op for a mode that
        // never previewed (the id is the one already active) and for a theme
        // that was confirmed (`restore_theme` is cleared by `confirm`).
        if let Some(previous) = mode.restore_theme {
            self.apply_theme(previous);
            window.refresh();
        }
        self.close_actions_menu(window);
        self.selected = 0;
        self.mode_scroll.set_offset(point(px(0.), px(0.)));
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
    /// Make `id` the live palette. Returns whether anything changed.
    ///
    /// Deliberately window-free and repaint-free: every call site already
    /// ends in a `cx.notify()` or a `window.refresh()` of its own, and the
    /// one genuinely native part of a theme swap — the window's
    /// `NSAppearance`, which the material behind the panel renders in — is
    /// reconciled once per frame in [`Root::render`] via
    /// [`Root::sync_window_appearance`] instead of at each call site. That
    /// keeps a preview, a commit, a daemon `ThemeChanged` broadcast and the
    /// startup read from needing four copies of the same two-step.
    ///
    /// A `false` return means `id` names no built-in — an id from a newer
    /// build, or a corrupted persisted setting. The live palette is left
    /// exactly as it was; a cosmetic setting is not worth interrupting
    /// anyone over.
    fn apply_theme(&self, id: &str) -> bool {
        theme::set_active(id)
    }

    /// Keeps the real `NSWindow`'s appearance in step with the active
    /// theme's, and does so *once per change*, not once per frame: the
    /// comparison is one enum compare against [`Root::applied_appearance`],
    /// and the AppKit call only happens when they differ.
    ///
    /// This matters because the panel is translucent over a native material
    /// (`material.rs` — `NSGlassEffectView`, or the
    /// `NSVisualEffectView(.popover)` fallback), and that material renders in
    /// whatever appearance the window is in. A cream Latte panel over a
    /// dark-appearance blur reads as a cream card with a dark halo leaking
    /// through everywhere the fill is thin — which is exactly what
    /// `data/neko-cozy-theme/report.md` predicted for a light direction, and
    /// the one thing a token swap alone cannot fix.
    fn sync_window_appearance(&mut self, window: &Window) {
        let wanted = theme::active_theme().appearance;
        if self.applied_appearance == Some(wanted) {
            return;
        }
        match (self.appearance_setter)(window, wanted) {
            Ok(()) => self.applied_appearance = Some(wanted),
            Err(e) => {
                // Recorded as applied anyway: retrying a failing AppKit call
                // on every subsequent frame would turn one logged failure
                // into an unbounded log flood on the render path.
                eprintln!("neko: could not set the window appearance: {e}");
                self.applied_appearance = Some(wanted);
            }
        }
    }

    /// Live preview: while the theme mode is active, moving the selection
    /// *is* trying the theme on. Reading a palette's name tells you nothing;
    /// watching the panel become it tells you everything.
    ///
    /// Gated on the active mode's own provider id rather than on the row's
    /// `kind`, so an ordinary root-list search that happens to surface a
    /// theme row never repaints the whole app as the captain arrows past it
    /// — previewing is something the theme *mode* does, not something a
    /// theme *row* does.
    ///
    /// Returns whether the palette actually changed, so callers that need a
    /// full-window repaint (rather than the `cx.notify()` they were already
    /// doing) can ask for one.
    fn preview_selected_theme(&self) -> bool {
        let Some(mode) = self.active_mode() else { return false };
        if mode.chrome.provider_id != "theme" {
            return false;
        }
        let Some(item) = self.results.get(self.selected) else { return false };
        if item.id == theme::active_theme().id {
            return false;
        }
        self.apply_theme(&item.id)
    }
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
    /// without touching anything else. Since "One constant panel width" the
    /// panel fills the whole window with no margin, so every click the
    /// window receives is a click inside the panel by construction — there
    /// is no separate margin dismiss path to reason about any more.
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
        if self.active_mode().is_some() {
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
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Reconciled here rather than at each theme-change call site — one
        // enum compare per frame, one AppKit call per actual change. See
        // this method's own doc comment.
        self.sync_window_appearance(window);
        let query_is_empty = self.text_field.read(cx).content().is_empty();
        div()
            .key_context("Panel")
            // Started and stopped by what is actually being painted, which
            // is the only thing that knows. A live row dropped by the pixel
            // budget, a query that no longer matches one, or the panel not
            // being on screen all stop the clock for free.
            .child(self.sync_pulse(window, cx))
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
            .w(px(theme::PANEL_WIDTH_WITH_DETAIL_PX))
            .h(px(PANEL_HEIGHT_PX))
            .bg(if self.translucent {
                theme::active().surface_panel_translucent
            } else {
                theme::active().surface_panel
            })
            .when(!self.translucent, |el| {
                el.border_1().border_color(theme::active().border_hairline_strong)
            })
            .rounded(px(theme::PANEL_RADIUS_PX))
            // **Deliberately no drawn shadow here — this used to be
            // `.shadow_lg()`.** That was the real, second cause of the
            // "black tent" halo the captain kept reporting even after the
            // native window shadow was disabled ("The double-panel shadow
            // defect" in `AGENTS.md`): `shadow_lg`'s blur/spread paints a
            // few px of soft, low-alpha black *outside* this div's own
            // bounds. Since "One constant panel width" this div is always
            // exactly the real `NSWindow`'s own width too, so there is no
            // longer any margin at all for a shadow to bleed into — but the
            // line stays removed regardless, since the translucent Glass
            // material (`self.translucent`) already reads as an elevated
            // surface on its own via real vibrancy, and the opaque fallback
            // keeps its `.border_1()` above for edge definition.
            // See `docs/evidence/panel-shadow-tent-fix-report.md`.
            .overflow_hidden()
            .when(!self.agent_tiles.is_empty(), |el| el.child(self.render_agent_grid(cx)))
            .child(self.render_input_row(cx))
            .child(match self.active_mode() {
                Some(mode) => self.render_mode_content(mode, cx),
                None => self.render_content_area(cx, query_is_empty).into_any_element(),
            })
            .child(self.render_footer(cx))
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
            .text_color(theme::active().text_primary)
            .text_size(px(18.))
            .child(match self.active_mode() {
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
                    .text_color(theme::active().text_tertiary)
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
        let tell = div().text_size(px(11.)).text_color(theme::active().text_tertiary).child("Searching…");
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
            container = container.child(self.render_row(idx, item, false, cx));
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
            .bg(theme::active().banner_danger_bg)
            .child(
                div()
                    .flex_1()
                    .text_size(px(12.5))
                    .line_height(px(18.))
                    .text_color(theme::active().text_secondary)
                    .child("⌥Space is off. Accessibility access was skipped, so the hotkey won't open neko. Reopen neko from the Dock to search anytime."),
            )
            .child(
                div()
                    .id("banner-open-settings")
                    .flex_shrink_0()
                    .text_size(px(12.))
                    .text_color(theme::active().text_secondary)
                    .cursor(CursorStyle::PointingHand)
                    .hover(|s| s.text_color(theme::active().text_primary))
                    .on_click(cx.listener(Self::open_accessibility_settings))
                    .child("Open System Settings"),
            )
            .child(
                div()
                    .id("banner-dismiss")
                    .flex_shrink_0()
                    .text_size(px(12.))
                    .text_color(theme::active().text_tertiary)
                    .cursor(CursorStyle::PointingHand)
                    .hover(|s| s.text_color(theme::active().text_primary))
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
            .bg(theme::active().banner_danger_bg)
            .child(
                div()
                    .flex_1()
                    .text_size(px(12.5))
                    .line_height(px(18.))
                    .text_color(theme::active().text_secondary)
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
    fn render_row(&self, idx: usize, item: &SearchItem, compact: bool, cx: &App) -> impl IntoElement {
        let selected = idx == self.selected;
        let title_color = theme::active().text_primary;
        let subtitle_color = if selected {
            theme::active().text_tertiary_on_selected
        } else {
            theme::active().text_tertiary
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
                .bg(theme::active().row_icon_socket_bg)
                .into_any_element(),
            Icon::Glyph(glyph) => glyph_element(*glyph, &item.id),
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
                row.bg(theme::active().surface_selected).rounded(px(theme::ROW_RADIUS_PX))
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
                // A live agent's badge breathes; every other badge is a
                // static type tag and stays exactly as it was. The waveform
                // comes from the one shared clock (`motion::PulseClock`) —
                // this element never animates itself, which is the rule that
                // keeps a repeating animation from pinning the window.
                let live = badge == "LIVE";
                let intensity = if live { self.pulse.read(cx).intensity() } else { 1.0 };
                let (bg, fg) = if live {
                    // Interpolating alpha rather than swapping colours, so
                    // the pulse reads as one thing brightening instead of
                    // two states flipping.
                    let mut bg = theme::active().state_success;
                    bg.a = 0.14 + 0.16 * intensity;
                    let mut fg = theme::active().state_success;
                    fg.a = 0.72 + 0.28 * intensity;
                    (bg, fg)
                } else {
                    (theme::active().row_icon_socket_bg, theme::active().text_tertiary)
                };
                div()
                    .flex_shrink_0()
                    .px(px(6.))
                    .py(px(2.))
                    .rounded(px(4.))
                    .bg(bg)
                    .text_size(px(10.))
                    .text_color(fg)
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
            .border_color(theme::active().border_hairline);

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
                    .text_color(theme::active().state_danger)
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
        let left_label: Option<SharedString> = match self.active_mode() {
            Some(mode) => Some(mode.chrome.title.into()),
            None => selected_item.map(|item| SharedString::from(item.title.clone())),
        };
        base.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_size(px(12.))
                    .text_color(theme::active().text_tertiary)
                    .children(left_label),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .text_size(px(12.))
                    .text_color(theme::active().text_secondary)
                    .child(primary_action)
                    .child(div().w(px(1.)).h(px(16.)).bg(theme::active().border_hairline_strong))
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
    fn render_mode_content(&self, mode: &ActiveMode, cx: &App) -> AnyElement {
        let content = div()
            .flex()
            .flex_1()
            .min_h(px(0.))
            .overflow_hidden()
            .child(self.render_mode_list(mode.chrome.has_detail, cx))
            .when(mode.chrome.has_detail, |el| el.child(self.render_mode_detail()));
        // A one-shot opacity reveal on entry, not a width/geometry
        // transition — the real `NSWindow` still never resizes at runtime
        // (`AGENTS.md`, "Mode view resize seam" — "cost two days"), and
        // since "One constant panel width" the panel `div`'s own width
        // never changes for a mode transition either; only the content
        // painted inside it does. `AnimationElement` is layout-transparent
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
    ///
    /// **`has_detail` decides the column, not just the neighbour.** With a
    /// detail pane the list is the frozen 264px column with a hairline down
    /// its right edge, and its rows are `compact` (no subtitle, no accessory
    /// — there is no room, and the detail pane says it better; see
    /// `AGENTS.md`, "Mode-view row anatomy"). Without one there is no
    /// neighbour to divide from and no reason to leave 496px empty, so the
    /// list takes the full panel and its rows render in full. Same rows, same
    /// renderer, same geometry tokens — only which of them apply.
    fn render_mode_list(&self, has_detail: bool, cx: &App) -> impl IntoElement {
        let mut container = div()
            .flex()
            .flex_col()
            .h_full()
            .px_2()
            .map(|el| {
                if has_detail {
                    el.flex_shrink_0()
                        .w(px(theme::MODE_LIST_COLUMN_WIDTH_PX))
                        .border_r_1()
                        .border_color(theme::active().border_hairline)
                } else {
                    el.flex_1().min_w(px(0.))
                }
            })
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
                container = container.child(self.render_row(idx, item, has_detail, cx));
            }
        }

        let fade_color = if self.translucent { theme::active().surface_panel_translucent } else { theme::active().surface_panel };
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
                    .text_color(theme::active().text_tertiary)
                    .child("Select an entry to preview it."),
            );
        };

        let preview = div()
            .flex_1()
            .min_h(px(0.))
            .overflow_hidden()
            .p_3()
            .rounded(px(theme::ROW_RADIUS_PX))
            .bg(theme::active().surface_input)
            .border_1()
            .border_color(theme::active().border_hairline)
            .text_size(px(13.))
            .text_color(theme::active().text_primary)
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
            // `translucent`. See `theme::active().menu_glass_tint`'s own doc comment.
            .bg(if self.menu_frost { theme::active().menu_glass_tint } else { theme::active().surface_raised })
            .border_1()
            .border_color(theme::active().border_hairline_strong)
            .shadow_lg()
            .children(menu.actions.iter().enumerate().map(|(idx, action)| {
                let selected = idx == menu.selected;
                let armed = selected && menu.confirm_armed && action.destructive;
                let label: SharedString = if armed {
                    format!("Confirm {} — ↵ again", action.label).into()
                } else {
                    action.label.clone().into()
                };
                let color = if action.destructive { theme::active().state_danger } else { theme::active().text_primary };
                div()
                    .id(("actions-menu-row", idx))
                    .flex()
                    .items_center()
                    .h(px(30.))
                    .px_2()
                    .rounded(px(theme::ROW_RADIUS_PX))
                    .when(selected, |el| el.bg(theme::active().surface_selected))
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

/// Folds a late, authoritative result set into what is already on screen,
/// under one rule: **late results may append, never reorder and never
/// displace the selection.**
///
/// The problem this exists for is not the lag the two-phase daemon fixed —
/// it is the thing that would otherwise feel *worse* than the lag. By the
/// time file search answers (up to `files::QUERY_TIMEOUT`), the captain has
/// been reading a real list for most of a second and may well have arrowed
/// down it. `search::allocate` orders sections by content strength, so its
/// authoritative answer can legitimately put a decisive Files match *above*
/// the Applications section already on screen — correct as a one-shot
/// answer, and a visible reshuffle under the captain's eyes as a late one.
///
/// So: `anchor` (what is rendered right now) keeps its exact order, and
/// only items the authoritative set introduced — necessarily the deferred
/// provider's own, since a fast provider cannot gain candidates between the
/// two frames — are appended after it. `allocate`'s own reservation and
/// section-strength ordering still decide *which* items exist and how many
/// slots each provider gets; this decides only where the new ones are
/// drawn relative to what the captain is already looking at.
///
/// Two consequences worth stating plainly rather than discovering later:
///
/// 1. **The result can be a different order than a single-shot response
///    for the same query would have produced.** That is deliberate, and it
///    self-corrects on the very next keystroke, which re-renders from a
///    fresh partial frame with no anchor to preserve.
/// 2. **If making room for the late section would drop the selected row,
///    the late section is not shown at all** (`anchor` is returned
///    unchanged). A captain who has arrowed down to row seven is about to
///    press Enter; moving that row — or worse, dropping it and snapping the
///    highlight back to the top — is a far worse outcome than file results
///    waiting for the next keystroke. `fit_within_budget` has to take the
///    room for a new section's header-plus-row from somewhere, and the only
///    place it can take it from is the tail of an earlier section.
fn merge_late_results(
    anchor: Vec<SearchItem>,
    authoritative: Vec<SearchItem>,
    budget_px: f32,
    selected: usize,
) -> Vec<SearchItem> {
    let already_shown = |item: &SearchItem| {
        anchor.iter().any(|shown| shown.kind == item.kind && shown.id == item.id)
    };
    let late: Vec<SearchItem> = authoritative.into_iter().filter(|item| !already_shown(item)).collect();
    if late.is_empty() {
        return anchor;
    }

    let selected_item = anchor.get(selected).map(|item| (item.kind.clone(), item.id.clone()));
    let mut merged = anchor.clone();
    merged.extend(late);
    let fitted = fit_within_budget(merged, budget_px);

    let selection_survived = selected_item.is_none_or(|(kind, id)| {
        fitted.iter().any(|item| item.kind == kind && item.id == id)
    });
    if selection_survived { fitted } else { anchor }
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
/// Splits running agents out of a response into `(tiles, rows)`.
///
/// Keyed on the badge the provider already sets, not on `kind == "agent"`:
/// an idle agent is an ordinary row and belongs in the list with everything
/// else. Only the live ones are news worth a tile.
///
/// Order is preserved on both sides, so the rows that stay keep whatever
/// section ordering `search::allocate` decided.
fn split_agent_tiles(results: Vec<SearchItem>) -> (Vec<SearchItem>, Vec<SearchItem>) {
    results.into_iter().partition(|item| item.kind == "agent" && item.badge.as_deref() == Some("LIVE"))
}

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
        .text_color(theme::active().text_tertiary)
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
        .text_color(theme::active().text_tertiary)
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
        .bg(theme::active().row_icon_socket_bg)
        .flex()
        .items_center()
        .justify_center()
        .child(
            div()
                .w(px(10.))
                .h(px(10.))
                .rounded(px(3.))
                .border_2()
                .border_color(theme::active().text_tertiary),
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
/// `row_id` is the row's own `SearchItem::id`. Only `Glyph::Palette` reads it
/// — a theme row draws *its own* palette, so a list of themes is a list of
/// previews rather than seventeen copies of the same mark. That is a lookup
/// keyed on data already on the row, not a `match` on which provider produced
/// it: any row whose id happens to name a built-in theme gets that theme's
/// swatch, and any row whose id doesn't (the `Themes` command in the root
/// list, whose id is `"themes"`) falls back to the live palette, which is the
/// honest thing for a row that means "open the theme list" rather than "be
/// this theme".
fn glyph_element(glyph: Glyph, row_id: &str) -> AnyElement {
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
            .child(div().w(px(12.)).h(px(1.5)).rounded(px(1.)).bg(theme::active().text_tertiary))
            .child(div().w(px(9.)).h(px(1.5)).rounded(px(1.)).bg(theme::active().text_tertiary))
            .child(div().w(px(12.)).h(px(1.5)).rounded(px(1.)).bg(theme::active().text_tertiary))
            .into_any_element(),
        // A rounded terminal-ish square with a status dot. The two variants
        // differ only in that dot: hollow and dim for an agent that exists,
        // filled and in the success colour for one that is running. Same
        // mark either way, so a list of agents reads as one kind of thing
        // and the live ones still pick themselves out.
        Glyph::Agent | Glyph::AgentLive => {
            let live = glyph == Glyph::AgentLive;
            slot.flex()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .relative()
                        .w(px(15.))
                        .h(px(13.))
                        .rounded(px(3.))
                        .border_1()
                        .border_color(if live {
                            theme::active().state_success_border
                        } else {
                            theme::active().text_tertiary
                        })
                        .child(
                            div()
                                .absolute()
                                .top(px(4.))
                                .left(px(5.))
                                .w(px(5.))
                                .h(px(5.))
                                .rounded(px(2.5))
                                .bg(if live {
                                    theme::active().state_success
                                } else {
                                    theme::active().text_tertiary
                                }),
                        ),
                )
                .into_any_element()
        }
        // Two horizontal rails, each with a knob at a different offset — the
        // settings mark. The offsets differ on purpose: two knobs at the same
        // x read as an equals sign at this size, not as controls that move.
        Glyph::Sliders => {
            let rail = |knob_left: f32| {
                div()
                    .relative()
                    .w(px(13.))
                    .h(px(5.))
                    .child(
                        div()
                            .absolute()
                            .top(px(2.))
                            .left(px(0.))
                            .w(px(13.))
                            .h(px(1.5))
                            .rounded(px(1.))
                            .bg(theme::active().text_tertiary),
                    )
                    .child(
                        div()
                            .absolute()
                            .top(px(0.))
                            .left(px(knob_left))
                            .w(px(4.))
                            .h(px(5.))
                            .rounded(px(1.5))
                            .bg(theme::active().text_secondary),
                    )
            };
            slot.flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(3.))
                .child(rail(8.))
                .child(rail(2.))
                .into_any_element()
        }
        // Four filled swatches in a 2x2 block, painted in the *live* theme's
        // own colours — the one glyph in this vocabulary that changes with
        // the active theme, deliberately: it is the affordance for changing
        // that theme, so it should show what is currently on.
        Glyph::Palette => {
            let swatch = |color| div().w(px(8.)).h(px(8.)).rounded(px(2.)).bg(color);
            // The row's own palette when it names one; the live one otherwise.
            let t = theme::theme_by_id(row_id).map_or_else(theme::active, |t| &t.palette);
            slot.flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(2.))
                // The four tokens that actually identify a palette at 8px:
                // what the panel is, what a selected row is, what text is,
                // and its one state colour.
                .child(
                    div()
                        .flex()
                        .gap(px(2.))
                        .child(swatch(t.surface_panel))
                        .child(swatch(t.text_primary)),
                )
                .child(
                    div()
                        .flex()
                        .gap(px(2.))
                        .child(swatch(t.state_danger))
                        .child(swatch(t.surface_selected)),
                )
                .into_any_element()
        }
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
                    .border_color(theme::active().text_tertiary),
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
                    .border_color(theme::active().text_tertiary),
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
                    .border_color(theme::active().text_tertiary)
                    .child(div().w(px(6.)).h(px(1.5)).rounded(px(1.)).bg(theme::active().text_tertiary)),
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
                            .bg(theme::active().text_tertiary),
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
                            .border_color(theme::active().text_tertiary),
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
                            .bg(theme::active().text_tertiary),
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
                            .border_color(theme::active().text_tertiary),
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
        .border_color(theme::active().text_tertiary)
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
                window.paint_path(path, theme::active().text_tertiary);
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
        .child(div().flex_shrink_0().text_color(theme::active().text_tertiary).child(SharedString::from(label.to_string())))
        .child(div().overflow_hidden().truncate().text_color(theme::active().text_secondary).child(SharedString::from(value)))
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

    /// How many rows the root list's fixed content budget actually holds
    /// when everything sits under one section header — derived from the
    /// same tokens `fit_section` uses rather than hard-coded, so these
    /// tests stay honest if the geometry ever changes.
    fn rows_that_fit_in_one_section() -> usize {
        ((CONTENT_AREA_MIN_HEIGHT_PX - theme::SECTION_HEADER_HEIGHT_PX) / theme::RESULT_ROW_HEIGHT_PX).floor()
            as usize
    }

    #[test]
    fn late_results_are_appended_below_what_is_already_on_screen_never_promoted_above_it() {
        // The reordering jump this rule exists to prevent: `allocate`
        // legitimately puts a decisive Files match in the *first* section
        // (it orders sections by content strength), which as a late answer
        // would shove everything the captain is reading downward.
        let anchor = vec![item_with_id("app", "safari"), item_with_id("app", "notes")];
        let authoritative = vec![
            item_with_id("file", "safari-notes.md"),
            item_with_id("app", "safari"),
            item_with_id("app", "notes"),
        ];

        let merged = merge_late_results(anchor, authoritative, CONTENT_AREA_MIN_HEIGHT_PX, 0);

        let order: Vec<(&str, &str)> = merged.iter().map(|i| (i.kind.as_str(), i.id.as_str())).collect();
        assert_eq!(
            order,
            vec![("app", "safari"), ("app", "notes"), ("file", "safari-notes.md")],
            "the anchor keeps its exact order and the late row lands after it"
        );
    }

    #[test]
    fn a_late_result_that_would_displace_the_selected_row_is_not_shown_at_all() {
        // The requirement stated most sharply: late results must never move
        // the selected row out from under a keypress. Filling the budget
        // with one section and selecting its *last* visible row means
        // making room for a Files header-plus-row can only come out of that
        // row — so the merge declines the late section entirely rather than
        // dropping the highlighted item and snapping the selection to the
        // top.
        let rows = rows_that_fit_in_one_section();
        let anchor: Vec<SearchItem> = (0..rows).map(|i| item_with_id("app", &format!("app{i}"))).collect();
        let selected = rows - 1;
        let authoritative = {
            let mut items = anchor.clone();
            items.push(item_with_id("file", "late.txt"));
            items
        };

        let merged = merge_late_results(anchor.clone(), authoritative, CONTENT_AREA_MIN_HEIGHT_PX, selected);

        assert_eq!(merged, anchor, "nothing changed on screen — not one row moved, not one row dropped");
        assert_eq!(
            resolve_selection(Some(("app", &format!("app{selected}"))), &merged),
            selected,
            "and the highlight is still on exactly the row it was on"
        );
    }

    #[test]
    fn a_late_result_still_lands_when_it_costs_only_rows_below_the_selection() {
        // The counterpart to the test above: declining the late section is
        // the exception, not the rule. With the selection near the top,
        // room for the Files section comes from rows the captain is not
        // pointing at, and the file row must actually appear.
        let rows = rows_that_fit_in_one_section();
        let anchor: Vec<SearchItem> = (0..rows).map(|i| item_with_id("app", &format!("app{i}"))).collect();
        let authoritative = {
            let mut items = anchor.clone();
            items.push(item_with_id("file", "late.txt"));
            items
        };

        let merged = merge_late_results(anchor, authoritative, CONTENT_AREA_MIN_HEIGHT_PX, 0);

        assert!(merged.iter().any(|i| i.kind == "file"), "the late file row is shown");
        assert_eq!(merged[0].id, "app0", "and the row under the highlight did not move");
    }

    #[test]
    fn a_complete_frame_that_adds_nothing_new_leaves_the_list_byte_identical() {
        // The common shape when file search matched nothing: the complete
        // frame carries exactly what the partial one did. Not one row may
        // be rebuilt, re-ordered, or re-fitted for it.
        let anchor = vec![item_with_id("app", "safari"), item_with_id("clipboard", "note")];
        let merged = merge_late_results(anchor.clone(), anchor.clone(), CONTENT_AREA_MIN_HEIGHT_PX, 1);
        assert_eq!(merged, anchor);
    }

    #[test]
    fn a_late_result_merges_into_an_empty_list_without_any_anchor_to_preserve() {
        // Nothing matched in the fast phase — there is no order to
        // preserve, so the authoritative answer is used as-is.
        let authoritative = vec![item_with_id("file", "budget.xlsx")];
        let merged = merge_late_results(Vec::new(), authoritative.clone(), CONTENT_AREA_MIN_HEIGHT_PX, 0);
        assert_eq!(merged, authoritative);
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

    // --- Preferences: it is a window, so the panel's only job is to open
    // one. Everything the window itself does is tested in
    // `crate::preferences::state` (pure) or is real I/O this cannot reach.

    #[gpui::test]
    fn confirming_the_preferences_command_opens_the_window_instead_of_entering_a_mode(
        cx: &mut TestAppContext,
    ) {
        let (client, _events) = NekoClient::connect(std::path::PathBuf::from("/tmp/neko-prefs-test.sock"));
        let accessibility: Rc<dyn AccessibilityChecker> = Rc::new(FakeAccessibilityChecker::new(true));
        let (opener, opened) = recording_preferences_opener();
        let window = cx.add_window(|_window, cx| {
            Root::build(client, accessibility, true, true, no_appearance_setter(), opener, cx)
        });
        window
            .update(cx, |root, window, cx| {
                root.results = vec![preferences_command_row()];
                root.selected = 0;
                root.confirm(&Confirm, window, cx);
                assert_eq!(opened.get(), 1, "confirming the row must open the Preferences window");
                assert!(
                    root.active_mode().is_none(),
                    "Preferences is a window; entering a mode here would put settings in the panel too"
                );
                assert!(root.activation_error.is_none(), "a UI transition is not a daemon activation");
            })
            .unwrap();
    }

    #[gpui::test]
    fn a_command_row_naming_no_real_mode_is_ignored_rather_than_breaking_the_panel(
        cx: &mut TestAppContext,
    ) {
        let window = test_root(cx);
        window
            .update(cx, |root, window, cx| {
                let mut row = preferences_command_row();
                row.enters_mode = Some("no-such-mode".to_string());
                root.results = vec![row];
                root.selected = 0;
                root.confirm(&Confirm, window, cx);
                assert!(root.active_mode().is_none());
            })
            .unwrap();
    }

    fn preferences_command_row() -> SearchItem {
        SearchItem {
            id: "preferences".to_string(),
            kind: "command".to_string(),
            title: "Preferences".to_string(),
            subtitle: None,
            icon: Icon::Glyph(Glyph::Sliders),
            section_label: "Commands".to_string(),
            action_label: "Open  ↵".to_string(),
            badge: Some("COMMAND".to_string()),
            accessory: None,
            enters_mode: Some(crate::preferences::PREFERENCES_MODE_ID.to_string()),
            group_label: None,
            actions: Vec::new(),
            source: None,
        }
    }

    #[gpui::test]
    fn up_from_the_first_row_walks_into_the_grid_and_down_walks_back_out(cx: &mut TestAppContext) {
        let window = test_root(cx);
        window
            .update(cx, |root, window, cx| {
                root.agent_tiles = vec![tile("a"), tile("b")];
                root.results = vec![agent_row("row-1"), agent_row("row-2")];
                root.selected = 0;

                // Up lands on the tile *nearest* the list, so the selection
                // moves by one visually rather than across the whole strip.
                root.select_previous(&SelectPrevious, window, cx);
                assert_eq!(root.grid_selected, Some(1));
                root.select_previous(&SelectPrevious, window, cx);
                assert_eq!(root.grid_selected, Some(0));
                // Already at the top of everything: stay, never wrap to the
                // bottom of the list.
                root.select_previous(&SelectPrevious, window, cx);
                assert_eq!(root.grid_selected, Some(0));

                root.select_next(&SelectNext, window, cx);
                assert_eq!(root.grid_selected, Some(1));
                root.select_next(&SelectNext, window, cx);
                assert_eq!(root.grid_selected, None, "off the last tile is back into the list");
                assert_eq!(root.selected, 0);
            })
            .unwrap();
    }

    #[gpui::test]
    fn with_no_tiles_the_arrow_keys_behave_exactly_as_they_always_did(cx: &mut TestAppContext) {
        let window = test_root(cx);
        window
            .update(cx, |root, window, cx| {
                root.results = vec![agent_row("row-1"), agent_row("row-2")];
                root.selected = 0;
                root.select_previous(&SelectPrevious, window, cx);
                assert_eq!(root.grid_selected, None, "no grid to walk into");
                assert_eq!(root.selected, 0);
            })
            .unwrap();
    }

    #[gpui::test]
    fn a_focused_tile_that_disappears_does_not_leave_the_selection_pointing_at_nothing(
        cx: &mut TestAppContext,
    ) {
        let window = test_root(cx);
        window
            .update(cx, |root, _window, _cx| {
                root.agent_tiles = vec![tile("a"), tile("b")];
                root.grid_selected = Some(1);
                // Simulates the agent finishing between two responses.
                root.agent_tiles = vec![tile("a")];
                root.grid_selected = match root.grid_selected {
                    Some(_) if root.agent_tiles.is_empty() => None,
                    Some(t) => Some(t.min(root.agent_tiles.len() - 1)),
                    None => None,
                };
                assert_eq!(root.grid_selected, Some(0), "clamped, not dangling");
            })
            .unwrap();
    }

    #[gpui::test]
    fn summoning_afresh_clears_a_focused_tile(cx: &mut TestAppContext) {
        let window = test_root(cx);
        window
            .update(cx, |root, window, cx| {
                root.agent_tiles = vec![tile("a")];
                root.grid_selected = Some(0);
                root.reset_for_summon(window, cx);
                assert_eq!(root.grid_selected, None);
            })
            .unwrap();
    }

    fn tile(id: &str) -> SearchItem {
        SearchItem { badge: Some("LIVE".to_string()), ..agent_row(id) }
    }

    #[test]
    fn only_live_agents_are_lifted_into_the_grid_idle_ones_stay_as_rows() {
        let live = SearchItem { badge: Some("LIVE".to_string()), ..agent_row("live-1") };
        let idle = SearchItem { badge: None, ..agent_row("idle-1") };
        let app = SearchItem { kind: "app".to_string(), ..agent_row("Finder") };
        let (tiles, rows) = split_agent_tiles(vec![app.clone(), live.clone(), idle.clone()]);
        assert_eq!(tiles.iter().map(|i| i.id.as_str()).collect::<Vec<_>>(), vec!["live-1"]);
        assert_eq!(rows.iter().map(|i| i.id.as_str()).collect::<Vec<_>>(), vec!["Finder", "idle-1"]);
    }

    #[test]
    fn an_agent_never_appears_both_as_a_tile_and_as_a_row() {
        let live = SearchItem { badge: Some("LIVE".to_string()), ..agent_row("a") };
        let (tiles, rows) = split_agent_tiles(vec![live]);
        assert_eq!(tiles.len(), 1);
        assert!(rows.is_empty(), "a tile is a move, not a copy — two rows for one agent is two Enters");
    }

    #[test]
    fn a_badge_that_is_not_live_on_a_non_agent_row_is_never_mistaken_for_a_tile() {
        // Clipboard rows carry TEXT/LINK badges; commands carry COMMAND.
        let clip = SearchItem {
            kind: "clipboard".to_string(),
            badge: Some("TEXT".to_string()),
            ..agent_row("copied")
        };
        let (tiles, rows) = split_agent_tiles(vec![clip]);
        assert!(tiles.is_empty());
        assert_eq!(rows.len(), 1);
    }

    #[gpui::test]
    fn the_grid_takes_its_height_out_of_the_row_budget_rather_than_growing_the_panel(
        cx: &mut TestAppContext,
    ) {
        let window = test_root(cx);
        window
            .update(cx, |root, _window, _cx| {
                assert_eq!(root.agent_grid_height(), 0.0, "no agents, no space taken");
                root.agent_tiles = vec![SearchItem { badge: Some("LIVE".into()), ..agent_row("a") }];
                assert_eq!(root.agent_grid_height(), theme::AGENT_GRID_HEIGHT_PX);
            })
            .unwrap();
    }

    fn agent_row(id: &str) -> SearchItem {
        SearchItem {
            id: id.to_string(),
            kind: "agent".to_string(),
            title: id.to_string(),
            subtitle: Some("claude · ~/x".to_string()),
            icon: Icon::Glyph(Glyph::AgentLive),
            section_label: "Agents".to_string(),
            action_label: "Open in Paseo  ↵".to_string(),
            badge: None,
            accessory: None,
            enters_mode: None,
            group_label: None,
            actions: Vec::new(),
            source: None,
        }
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
        cx.add_window(|_window, cx| Root::build(client, accessibility, true, true, no_appearance_setter(), no_preferences_opener(), cx))
    }

    /// A headless stand-in for `material::set_window_appearance` — GPUI's own
    /// test-platform window panics on `window_handle()`, so no test can make a
    /// real one. Records nothing; `recording_appearance_setter` is the variant
    /// for tests that need to assert what was asked for.
    /// GPUI's test-platform window `unimplemented!()`s on real window
    /// operations, so a panel test can never let the true opener run.
    fn no_preferences_opener() -> PreferencesOpener {
        Rc::new(|_window, _cx| {})
    }

    /// Records that the panel asked for the window, without opening one.
    fn recording_preferences_opener() -> (PreferencesOpener, Rc<std::cell::Cell<usize>>) {
        let count = Rc::new(std::cell::Cell::new(0usize));
        let seen = count.clone();
        (Rc::new(move |_window, _cx| seen.set(seen.get() + 1)), count)
    }

    fn no_appearance_setter() -> AppearanceSetter {
        Rc::new(|_window, _appearance| Ok(()))
    }

    type AppearanceLog = Rc<std::cell::RefCell<Vec<theme::Appearance>>>;

    fn recording_appearance_setter() -> (AppearanceSetter, AppearanceLog) {
        let log: AppearanceLog = Rc::new(std::cell::RefCell::new(Vec::new()));
        let sink = log.clone();
        (Rc::new(move |_window, appearance| { sink.borrow_mut().push(appearance); Ok(()) }), log)
    }

    fn test_root_recording_appearance(cx: &mut TestAppContext) -> (gpui::WindowHandle<Root>, AppearanceLog) {
        let (client, _events) = NekoClient::connect(std::path::PathBuf::from(format!(
            "/tmp/neko-panel-test-{}-{}.sock",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        )));
        let accessibility: Rc<dyn AccessibilityChecker> = Rc::new(FakeAccessibilityChecker::new(true));
        let (setter, log) = recording_appearance_setter();
        let window = cx.add_window(|_window, cx| Root::build(client, accessibility, true, true, setter, no_preferences_opener(), cx));
        (window, log)
    }

    /// A command row's `id`, `kind: "command"` — everything else is
    /// deliberately minimal, since only `enters_mode` and `action_label`
    /// matter to `confirm`'s own routing.
    fn theme_item(id: &str) -> SearchItem {
        SearchItem {
            id: id.into(),
            kind: "theme".into(),
            title: id.into(),
            subtitle: None,
            icon: neko_protocol::Icon::Glyph(Glyph::Palette),
            section_label: "Themes".into(),
            action_label: "Use Theme  ↵".into(),
            badge: None,
            accessory: None,
            enters_mode: None,
            group_label: None,
            actions: Vec::new(),
            source: None,
        }
    }

    /// Puts the panel into the theme mode with `items` listed, as if the
    /// daemon's scoped search had answered.
    fn enter_theme_mode(
        window: &gpui::WindowHandle<Root>,
        cx: &mut TestAppContext,
        items: Vec<SearchItem>,
    ) {
        window
            .update(cx, |root, window, cx| {
                root.results = vec![command_item("theme")];
                root.selected = 0;
                root.confirm(&Confirm, window, cx);
                root.apply_search_results(items, true, root.generation, cx);
            })
            .unwrap();
    }

    #[gpui::test]
    fn entering_the_theme_mode_lands_on_the_theme_already_in_use_and_changes_nothing(cx: &mut TestAppContext) {
        let _guard = theme::test_lock();
        theme::set_active("gruvbox-dark");
        let window = test_root(cx);
        enter_theme_mode(
            &window,
            cx,
            vec![theme_item("neutral"), theme_item("dracula"), theme_item("gruvbox-dark")],
        );
        window
            .update(cx, |root, _window, _cx| {
                assert_eq!(root.selected, 2, "the highlight must start on the theme in use, not on whatever sorts first");
                assert_eq!(
                    theme::active_theme().id,
                    "gruvbox-dark",
                    "merely opening the theme list must not repaint the app in another palette"
                );
            })
            .unwrap();
        theme::set_active(theme::DEFAULT_THEME_ID);
    }

    #[gpui::test]
    fn arrowing_through_the_theme_list_previews_each_one_live(cx: &mut TestAppContext) {
        let _guard = theme::test_lock();
        theme::set_active("neutral");
        let window = test_root(cx);
        enter_theme_mode(&window, cx, vec![theme_item("neutral"), theme_item("catppuccin-latte")]);
        window
            .update(cx, |root, window, cx| {
                assert_eq!(theme::active_theme().id, "neutral");
                root.select_next(&SelectNext, window, cx);
                assert_eq!(theme::active_theme().id, "catppuccin-latte", "moving the selection must apply the palette, not just highlight its name");
                root.select_previous(&SelectPrevious, window, cx);
                assert_eq!(theme::active_theme().id, "neutral", "arrowing back must come back too");
            })
            .unwrap();
        theme::set_active(theme::DEFAULT_THEME_ID);
    }

    #[gpui::test]
    fn escaping_the_theme_mode_reverts_every_previewed_change(cx: &mut TestAppContext) {
        let _guard = theme::test_lock();
        theme::set_active("nord");
        let window = test_root(cx);
        enter_theme_mode(&window, cx, vec![theme_item("nord"), theme_item("solarized-light"), theme_item("ember")]);
        window
            .update(cx, |root, window, cx| {
                root.select_next(&SelectNext, window, cx);
                root.select_next(&SelectNext, window, cx);
                assert_eq!(theme::active_theme().id, "ember");
                root.handle_dismiss(&crate::DismissWindow, window, cx);
                assert!(root.active_mode().is_none());
                assert_eq!(theme::active_theme().id, "nord", "Escape must put back the theme that was in use before the mode opened");
            })
            .unwrap();
        theme::set_active(theme::DEFAULT_THEME_ID);
    }

    #[gpui::test]
    fn confirming_a_theme_keeps_it_and_a_later_mode_exit_does_not_undo_it(cx: &mut TestAppContext) {
        let _guard = theme::test_lock();
        theme::set_active("neutral");
        let window = test_root(cx);
        enter_theme_mode(&window, cx, vec![theme_item("neutral"), theme_item("rose-pine")]);
        window
            .update(cx, |root, window, cx| {
                root.select_next(&SelectNext, window, cx);
                assert_eq!(theme::active_theme().id, "rose-pine");
                // Enter. The daemon socket in these tests has nobody
                // listening, so the `Request::Activate` this fires can never
                // succeed — which is exactly the case worth pinning: the
                // captain's choice must survive even when persistence fails,
                // rather than snapping back.
                root.confirm(&Confirm, window, cx);
                root.exit_mode(window, cx);
                assert_eq!(theme::active_theme().id, "rose-pine", "a confirmed theme must not be reverted by leaving the mode afterwards");
            })
            .unwrap();
        theme::set_active(theme::DEFAULT_THEME_ID);
    }

    #[gpui::test]
    fn filtering_the_theme_list_previews_the_top_match(cx: &mut TestAppContext) {
        let _guard = theme::test_lock();
        theme::set_active("neutral");
        let window = test_root(cx);
        enter_theme_mode(&window, cx, vec![theme_item("neutral"), theme_item("dracula")]);
        window
            .update(cx, |root, _window, cx| {
                // What a keystroke's scoped search response looks like: a
                // narrowed list with no previously-selected row surviving.
                root.selected = 0;
                root.results.clear();
                root.apply_search_results(vec![theme_item("dracula")], true, root.generation, cx);
                assert_eq!(theme::active_theme().id, "dracula", "typing to filter moves the selection, so it previews too");
            })
            .unwrap();
        theme::set_active(theme::DEFAULT_THEME_ID);
    }

    #[gpui::test]
    fn a_theme_row_in_the_root_list_never_previews_as_you_arrow_past_it(cx: &mut TestAppContext) {
        let _guard = theme::test_lock();
        theme::set_active("neutral");
        let window = test_root(cx);
        window
            .update(cx, |root, window, cx| {
                // No mode active — an ordinary root-list search that happens
                // to surface theme rows.
                root.results = vec![item("app"), theme_item("dracula")];
                root.selected = 0;
                root.select_next(&SelectNext, window, cx);
                assert_eq!(root.selected, 1);
                assert_eq!(theme::active_theme().id, "neutral", "previewing belongs to the theme mode, not to a theme row");
            })
            .unwrap();
        theme::set_active(theme::DEFAULT_THEME_ID);
    }

    #[gpui::test]
    fn a_light_theme_puts_the_window_into_the_light_appearance_and_a_dark_one_takes_it_back(cx: &mut TestAppContext) {
        let _guard = theme::test_lock();
        theme::set_active("neutral");
        let (window, log) = test_root_recording_appearance(cx);
        // `render` calls this on every frame; driving it directly is the
        // same call without needing a real draw (GPUI's test window cannot
        // paint one).
        window.update(cx, |root, window, _cx| root.sync_window_appearance(window)).unwrap();
        assert_eq!(log.borrow().last(), Some(&theme::Appearance::Dark));

        theme::set_active("catppuccin-latte");
        window.update(cx, |root, window, _cx| root.sync_window_appearance(window)).unwrap();
        assert_eq!(
            log.borrow().last(),
            Some(&theme::Appearance::Light),
            "a light palette over a dark-appearance blur reads as a dark halo — the native appearance has to follow"
        );

        let before = log.borrow().len();
        window.update(cx, |root, window, _cx| root.sync_window_appearance(window)).unwrap();
        assert_eq!(log.borrow().len(), before, "an unchanged appearance must not make an AppKit call every frame");

        theme::set_active("gruvbox-dark");
        window.update(cx, |root, window, _cx| root.sync_window_appearance(window)).unwrap();
        assert_eq!(log.borrow().last(), Some(&theme::Appearance::Dark));
        theme::set_active(theme::DEFAULT_THEME_ID);
    }

    fn command_item(mode: &str) -> SearchItem {
        SearchItem { enters_mode: Some(mode.to_string()), ..item_with_id("command", "clipboard-history") }
    }

    #[gpui::test]
    fn a_late_complete_frame_never_moves_the_row_the_captain_arrowed_down_to(cx: &mut TestAppContext) {
        // The same guarantee `a_late_result_that_would_displace_the_selected_row_is_not_shown_at_all`
        // pins on the pure merge, driven through `Root`'s own real frame
        // handling instead — including the case the pure test cannot
        // express: the captain pressing Down *between* the partial frame
        // and the complete one, so the selection the merge has to protect
        // is not the one that existed when the request went out.
        let window = test_root(cx);
        cx.run_until_parked();

        let rows = rows_that_fit_in_one_section();
        let partial: Vec<SearchItem> = (0..rows).map(|i| item_with_id("app", &format!("app{i}"))).collect();
        let complete = {
            let mut items = partial.clone();
            items.insert(0, item_with_id("file", "late.txt"));
            items
        };

        window
            .update(cx, |root, _window, cx| {
                root.generation = 1;
                root.pending_search_generation = Some(1);
                root.apply_search_results(partial.clone(), false, 1, cx);
            })
            .unwrap();

        // The captain arrows all the way down to the last visible row while
        // file search is still running.
        for _ in 0..rows {
            window.update(cx, |root, window, cx| root.select_next(&SelectNext, window, cx)).unwrap();
        }
        let selected_id = window
            .update(cx, |root, _window, _cx| {
                assert_eq!(root.selected, rows - 1, "arrowed to the last row");
                root.results[root.selected].id.clone()
            })
            .unwrap();

        window
            .update(cx, |root, _window, cx| root.apply_search_results(complete, true, 1, cx))
            .unwrap();

        window
            .update(cx, |root, _window, _cx| {
                assert_eq!(root.results, partial, "not one row moved when the late frame landed");
                assert_eq!(root.selected, rows - 1, "the highlight stayed on the same index");
                assert_eq!(root.results[root.selected].id, selected_id, "and on the same item");
                assert!(!root.searching, "the tell clears the moment the complete frame lands");
                assert_eq!(root.pending_search_generation, None);
            })
            .unwrap();
    }

    #[gpui::test]
    fn the_still_searching_tell_tracks_the_real_deferred_phase_not_a_bare_timer(cx: &mut TestAppContext) {
        // `reveal_searching_tell_if_still_pending` must only ever describe
        // a query that genuinely still has a provider running — the tell is
        // this state made visible, not a guess made from elapsed time.
        let window = test_root(cx);
        cx.run_until_parked();

        window
            .update(cx, |root, _window, cx| {
                root.generation = 4;
                root.pending_search_generation = Some(4);
                root.apply_search_results(vec![item_with_id("app", "safari")], false, 4, cx);
                assert_eq!(root.partial_generation, Some(4), "the partial frame records that more is coming");
                root.reveal_searching_tell_if_still_pending(4, cx);
                assert!(root.searching, "with the deferred provider still running, the tell is honest");

                root.apply_search_results(vec![item_with_id("app", "safari")], true, 4, cx);
                assert!(!root.searching, "and clears as soon as the complete frame lands");

                // A stale reveal for the same, now-resolved generation must
                // not turn it back on.
                root.reveal_searching_tell_if_still_pending(4, cx);
                assert!(!root.searching);
            })
            .unwrap();
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
                let mode = root.active_mode().expect("confirming a command row must enter a mode");
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
            .update(cx, |root, _window, _cx| assert!(root.active_mode().is_some(), "must be in the mode before exiting it"))
            .unwrap();

        window
            .update(cx, |root, window, cx| root.exit_mode(window, cx))
            .unwrap();
        cx.run_until_parked();

        window
            .update(cx, |root, _window, cx| {
                assert!(root.active_mode().is_none());
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

        window.update(cx, |root, _window, _cx| assert!(root.active_mode().is_none())).unwrap();
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
                assert!(root.active_mode().is_some(), "setup: still inside the mode before Escape");
            })
            .unwrap();

        window
            .update(cx, |root, window, cx| root.handle_dismiss(&crate::DismissWindow, window, cx))
            .unwrap();
        window
            .update(cx, |root, _window, _cx| {
                assert!(root.actions_menu.is_none(), "the first Escape must close the menu");
                assert!(root.active_mode().is_some(), "the first Escape must not also exit the mode in the same press");
            })
            .unwrap();

        window
            .update(cx, |root, window, cx| root.handle_dismiss(&crate::DismissWindow, window, cx))
            .unwrap();
        window
            .update(cx, |root, _window, _cx| {
                assert!(root.active_mode().is_none(), "the second Escape, with the menu already closed, must exit the mode");
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
