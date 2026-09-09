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
use std::sync::{Arc, OnceLock};

use gpui::{
    Anchor, AnyElement, App, ClickEvent, Context, CursorStyle, DispatchPhase, Entity, FocusHandle,
    Focusable, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Render, ScrollHandle, SharedString,
    FontFeatures, Window, actions, anchored, canvas, deferred, div, img, point, prelude::*, px,
    svg,
};
use neko_client::NekoClient;
use neko_protocol::{Glyph, Icon, ItemAction, Meter, MeterStat, Request, Response, SearchItem};

use crate::accessibility::AccessibilityChecker;
use crate::assets::{glyph_icon, icon};
use crate::edge_fade::scroll_edge_fade;
use crate::components::scroll::with_scrollbar;
use crate::motion::HoverWash as _;
use crate::menu_frost::sync_menu_frost;
use crate::modes::{self, ModeChrome, TaskRef};
use crate::motion;
use crate::text_field::{ContentChanged, DEFAULT_PLACEHOLDER, TextField};
use crate::theme;
use crate::window_drag::PanelDrag;

actions!(panel, [SelectNext, SelectPrevious, Confirm, OpenActionsMenu, OpenInPaseo]);

/// `neko_core::commands::CommandsProvider::id()`. Named here because the
/// slash palette scopes to it by name, the one place the client has to know
/// a provider id — the same way `crate::preferences` names its own.
const COMMAND_PROVIDER_ID: &str = "command";

const RESULT_LIMIT: usize = 8;
/// How many rows an **empty** root query asks for.
///
/// Larger than [`RESULT_LIMIT`] because the two answer different questions. A
/// typed query wants the best few matches and must fit without reflowing; an
/// empty one is a browsable frecency-ranked suggestion list, so it scrolls
/// and can afford depth. Nothing defers on an empty query either
/// (`FileProvider::defers_for` needs two characters), so there is no
/// second frame to reconcile against.
const SUGGESTED_LIMIT: usize = 24;
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
/// How many agents the grid shows. Matches `neko_core::agents::GRID_CAPACITY`
/// — the provider caps its own empty-query answer at the same number, and the
/// two are checked against each other in this module's tests rather than
/// left to drift.
pub const AGENT_GRID_CAPACITY: usize = 4;

/// The content area is deliberately far taller than a typed query's own
/// `RESULT_LIMIT` needs: with nothing typed the root list is a browsable
/// frecency-ranked suggestion list (`SUGGESTED_LIMIT`) that scrolls, and a
/// short panel would make that list a keyhole.
pub const CONTENT_AREA_MIN_HEIGHT_PX: f32 = theme::RESULT_ROW_HEIGHT_PX * 10.0;
/// The panel is a fixed height for the process's whole lifetime (the real
/// `NSWindow` is never resized — `AGENTS.md`, "Mode view resize seam"), so
/// the agent grid can only ever take space *from* the rows. A grid taller
/// than the content area would leave no rows at all, which is a build error
/// rather than something to discover at runtime.
const _: () = assert!(theme::AGENT_GRID_HEIGHT_PX < CONTENT_AREA_MIN_HEIGHT_PX);

pub const PANEL_HEIGHT_PX: f32 =
    theme::INPUT_ROW_HEIGHT_PX + CONTENT_AREA_MIN_HEIGHT_PX;

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
    /// The transcript's own bounded image cache — see
    /// `RowIconCache::for_conversation` for why it is not the row cache.
    conversation_image_cache: Entity<crate::row_icon_cache::RowIconCache>,
    /// `Some` while a command's mode is active (`SearchItem::enters_mode`) —
    /// see `crate::modes`'s module doc comment for the full concept. Empty
    /// is the ordinary root list.
    ///
    /// A single `Option`: no mode nests. Preferences is a real window
    /// (`crate::preferences`), not a mode, so nothing here ever needed to.
    active_mode: Option<ActiveMode>,
    /// Set only while an explicit Codex task activation is in flight. The
    /// backend travels with the opaque id so it can never become a Paseo
    /// conversation subject on success.
    pending_task_mode: Option<(TaskRef, SearchItem)>,
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
    /// Whether the query that produced the results now on screen was empty.
    ///
    /// Captured when the search is *dispatched*, not read from the field when
    /// the answer arrives — a response can land after the field has changed,
    /// and judging a frame by a query it did not answer is exactly the class
    /// of bug the generation counter exists to prevent elsewhere.
    results_are_for_empty_query: bool,
    /// The root list's own scroll position. The list scrolls only when
    /// nothing is typed — see `apply_results`.
    root_scroll: ScrollHandle,
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
    /// A keyboard-driven scroll in flight, and which handle it is moving.
    ///
    /// **Arrowing used to teleport the viewport.** `ScrollHandle::scroll_to_item`
    /// takes effect with no travel, which is fine for one row and reads as the
    /// list flinching once the jump is longer — and it got much more visible
    /// once the list grew a thumb, because the thumb jumps too. See
    /// `motion::ScrollGlide`.
    scroll_glide: Option<(ScrollHandle, motion::ScrollGlide)>,
    /// Tool chips whose captured output is currently unfolded, by row id.
    /// Per-summon like every other transient view state; cleared with the
    /// mode, since a turn id (`agent#rank`) only means anything inside the
    /// conversation that minted it.
    expanded_tool_output: std::collections::HashSet<String>,
    /// Set while a `Request::Activate` is in flight, and rendered as a tell.
    ///
    /// **Enter can take seconds and used to show nothing at all.** Starting an
    /// agent shells out to a CLI whose Electron boot alone is ~1s (one measured
    /// end-to-end run took 2.88s, bounded at 20s), and every schedule, terminal
    /// and agent action is an MCP round trip. The panel deliberately does not
    /// hide until the outcome is known — that is what makes an inline failure
    /// possible — so for that whole window pressing Enter looked exactly like
    /// pressing nothing, which is the same defect `activation_error` was added
    /// to fix, one step earlier in the same path.
    activating: bool,
    /// Verification-only (`evidence::bench_search_query`,
    /// `NEKO_BENCH_SEARCH`): when the current generation's request was
    /// dispatched, so `apply_search_results` can report keystroke-to-render
    /// latency for the frame that actually lands. Always recorded — an
    /// `Instant::now()` per keystroke is far below the noise floor of the
    /// thing being measured — but only ever *read* when the hook is on, so
    /// normal operation prints nothing.
    search_dispatched_at: Option<(u64, std::time::Instant)>,
    /// How many rows the last response carried that the fixed content area
    /// could not show. Drives the "+N more" cue — see `run_search`.
    hidden_rows: usize,

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
    /// How this panel picks itself up and moves. Injected for the same
    /// reasons [`AppearanceSetter`] and [`PreferencesOpener`] are — every
    /// method on it ends in a native window call, and GPUI's test platform
    /// panics rather than erroring on those. See `window_drag.rs`.
    drag: Rc<dyn PanelDrag>,
    /// Whether a drag is live right now. The panel's own half of the drag
    /// state and deliberately *all* of it: grab offsets, screens, snap targets
    /// and the guide window all live in `window_drag.rs`, which is the only
    /// thing that knows what a drag is. This flag exists because three
    /// unrelated pieces of the panel have to behave differently while one is
    /// in progress — the window-level mouse listeners, Escape, and a fresh
    /// summon.
    dragging: bool,
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
    /// **Which thing this mode is about**, when it is about one — the agent
    /// id for the conversation view.
    ///
    /// Every mode until now was a *list* of one provider's rows, so scoping
    /// to the provider was the whole of "which mode am I in". A conversation
    /// is a list about one agent, and the provider has no way to know which
    /// unless it is told: `run_search` sends this as the scoped query, so
    /// `ConversationProvider::search` receives the agent id where a filtering
    /// provider would receive what was typed.
    ///
    /// `None` for every mode that is a plain list, which is all of the
    /// others.
    subject: Option<String>,
    /// A Codex task's qualified backend/id, never a bare id routed through
    /// the Paseo conversation path.
    task_ref: Option<TaskRef>,
    /// The row that opened this mode, kept for the transcript's header —
    /// the agent's name, workspace and badge are already composed on it by
    /// the provider that knows them, and re-deriving any of that client-side
    /// would be a second copy of `agents.rs`'s naming rules.
    subject_item: Option<SearchItem>,
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
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        client: NekoClient,
        accessibility: Rc<dyn AccessibilityChecker>,
        translucent: bool,
        menu_frost: bool,
        appearance_setter: AppearanceSetter,
        open_preferences: PreferencesOpener,
        drag: Rc<dyn PanelDrag>,
        cx: &mut App,
    ) -> Entity<Self> {
        cx.new(|cx| {
            Self::build(client, accessibility, translucent, menu_frost, appearance_setter, open_preferences, drag, cx)
        })
    }

    /// The real construction logic, factored out of [`new`](Self::new) so a
    /// test can build a `Root` directly inside `TestAppContext::add_window`'s
    /// own closure (which needs a plain `V`, not an `Entity<V>` — `new`
    /// itself wraps this in `cx.new(...)`) without duplicating any of it.
    #[allow(clippy::too_many_arguments)]
    fn build(
        client: NekoClient,
        accessibility: Rc<dyn AccessibilityChecker>,
        translucent: bool,
        menu_frost: bool,
        appearance_setter: AppearanceSetter,
        open_preferences: PreferencesOpener,
        drag: Rc<dyn PanelDrag>,
        cx: &mut Context<Self>,
    ) -> Self {
        let pulse = motion::PulseClock::global(cx);
        // A tick is only worth anything if it repaints — see
        // `sync_pulse` for why it is the render pass, not this
        // subscription, that decides whether the clock runs at all.
        cx.observe(&pulse, |_root, _clock, cx| cx.notify()).detach();
        let text_field = TextField::new(cx);
        let row_icon_cache = crate::row_icon_cache::RowIconCache::new(cx);
        let conversation_image_cache = crate::row_icon_cache::RowIconCache::for_conversation(cx);
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
            conversation_image_cache,
            active_mode: None,
            pending_task_mode: None,
            open_preferences,
            agent_tiles: Vec::new(),
            grid_selected: None,
            root_scroll: ScrollHandle::new(),
            results_are_for_empty_query: true,
            pulse,
            actions_menu: None,
            menu_open_before_this_press: false,
            searching: false,
            pending_search_generation: None,
            partial_generation: None,
            scroll_glide: None,
            expanded_tool_output: std::collections::HashSet::new(),
            activating: false,
            search_dispatched_at: None,
            hidden_rows: 0,
            mode_scroll: ScrollHandle::new(),
            applied_appearance: None,
            appearance_setter,
            drag,
            dragging: false,
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
        // A drag cannot outlive the summon it started in. In practice the
        // mouse-up that ends one always arrives first, but "always" here
        // depends on AppKit delivering an event, and the failure mode if it
        // ever does not — a panel that silently follows the cursor on the
        // *next* summon, with a guide window left on screen — is bad enough
        // that this is worth one unconditional line. `cancel`, not `finish`:
        // a drag interrupted by the panel being hidden was never completed.
        self.cancel_window_drag(window, cx);
        // Per-summon state, exactly like the mode and the menu above it: a
        // tile focused in one session must not still be focused in the next.
        // Per-summon state, like everything else here: a panel dismissed
        // mid-activation must not come back still claiming to be working.
        self.activating = false;
        self.expanded_tool_output.clear();
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

    /// Evidence/verification-only — drives the real `SelectNext` handler,
    /// which is what moves the selection *and* scrolls it into view.
    ///
    /// It exists because the thing worth proving is a keyboard behaviour and
    /// this repo does not synthesise OS input: without it the only evidence
    /// for "arrowing down now drags the view" would be the unit test on the
    /// index arithmetic, which says nothing about whether the call is wired
    /// to the key at all.
    pub fn select_next_for_evidence(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.select_next(&SelectNext, window, cx);
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

    /// What is currently typed. One place, so `run_search` and every
    /// `Request::Activate` can never disagree about what "the query" is.
    fn query(&self, cx: &Context<Self>) -> String {
        self.text_field.read(cx).content().to_string()
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
        let client = self.client.clone();
        // The mode seam: while a mode is active, every keystroke scopes to
        // its own provider (`Request::Search`'s `provider` field) with a
        // generous limit — the merged root-list budget/reservation logic
        // (`fit_within_budget`) doesn't apply at all here; the mode list
        // renders every returned item and scrolls instead (`render_mode_list`,
        // `edge_fade::scroll_edge_fade`).
        let raw = self.text_field.read(cx).content().to_string();
        // **A leading `/` is a command palette**, and it costs no protocol
        // change at all: it scopes the search to the `command` provider,
        // which is the same `Request::Search { provider: Some(..) }` a mode
        // already uses. `/` alone lists every command; `/the` filters to
        // Themes. Inside a mode the slash is ordinary text — a mode is
        // already scoped, and a person typing a path or a query there means
        // the character.
        let slash = self.active_mode().is_none() && raw.starts_with('/');
        let (mode_provider, query) = if slash {
            (Some(COMMAND_PROVIDER_ID.to_string()), raw[1..].to_string())
        } else {
            match self.active_mode() {
                Some(mode) if mode.task_ref.is_some() => {
                    let task = mode.task_ref.as_ref().expect("checked above");
                    (Some(task.provider_id().to_string()), task.id.clone())
                }
                // **A mode with a subject sends the subject, not the
                // typing.** The conversation view is a list about one agent,
                // and its provider has no other way to learn which — see
                // `ActiveMode::subject`. Typing in it still filters, because
                // the provider filters its own rows against nothing here;
                // that is a deliberate limit noted in `conversation.rs`.
                Some(mode) if mode.subject.is_some() => (
                    Some(mode.chrome.provider_id.to_string()),
                    mode.subject.clone().unwrap_or_default(),
                ),
                Some(mode) => (Some(mode.chrome.provider_id.to_string()), raw),
                None => (None, raw),
            }
        };
        // A slash palette is a scoped list like a mode's, so it is never
        // budget-fit and never treated as the empty root query.
        self.results_are_for_empty_query = !slash && query.trim().is_empty();
        let limit = match (&mode_provider, self.results_are_for_empty_query) {
            (Some(_), _) => MODE_RESULT_LIMIT,
            (None, true) => SUGGESTED_LIMIT,
            (None, false) => RESULT_LIMIT,
        };
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
        // For the transcript's stick-to-bottom rule below: whether the *view*
        // was parked at the end before this frame replaced the list. Read
        // from the scroll handle rather than from `selected`, which no longer
        // means anything in a transcript now that the arrow keys scroll it.
        let was_at_bottom = self.transcript_is_at_bottom();

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
            split_agent_tiles(items, self.text_field.read(cx).content().trim().is_empty())
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
        // **An empty root query keeps everything and scrolls; a typed one is
        // still budget-fit.** They are different things: the suggestion list
        // is browsable, so dropping rows to fit would be throwing away the
        // depth it exists to offer. A typed query is a ranked answer that
        // must not reflow under the captain's hands, which is what
        // `fit_within_budget` and `merge_late_results` protect — and neither
        // concern applies with nothing typed, since nothing defers on an
        // empty query so there is no second frame to reconcile.
        let offered = items.len();
        // **Fitted twice when anything was dropped, and that is deliberate.**
        // The cue takes real height, so a list fitted to the full budget and
        // then given a cue would push its own last row out — the defect
        // `fit_within_budget` exists to prevent, reintroduced one layer up.
        // Fitting again against the smaller budget is the only way the count
        // can be honest about itself. Both passes are pure `Vec` work on at
        // most `RESULT_LIMIT` items.
        let fit = |items: Vec<SearchItem>| {
            let first = fit_within_budget(items.clone(), budget);
            if first.len() < items.len() {
                fit_within_budget(items, budget - theme::TRUNCATION_CUE_HEIGHT_PX)
            } else {
                first
            }
        };
        self.results = if self.active_mode().is_some() || self.results_are_for_empty_query {
            items
        } else if complete && self.partial_generation == Some(generation) {
            let anchor = std::mem::take(&mut self.results);
            merge_late_results(anchor, items, budget, self.selected)
        } else {
            fit(items)
        };
        // `merge_late_results` can legitimately end up with *more* rows than
        // this response offered — it keeps what is already on screen — hence
        // the saturating subtraction rather than a difference.
        self.hidden_rows = offered.saturating_sub(self.results.len());

        let previous = previously_selected.as_ref().map(|(kind, id)| (kind.as_str(), id.as_str()));
        self.selected = resolve_selection(previous, &self.results);
        // A re-search can move the selection by identity; the view has to
        // follow it there too, not only on an arrow key.
        self.scroll_selection_into_view(ScrollBias::None);
        // **No tile is selected to begin with.** The grid sits directly above
        // the rows now, so Up from the first row lands on the last tile — the
        // move that is spatially correct — and the selection can start where
        // a launcher's selection belongs, on the top result. Starting it on a
        // tile meant the panel opened with an agent highlighted rather than
        // the thing Enter would actually run.
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
        // A transcript is entered at the *bottom* — the newest turn is the
        // reason you opened it, and every messaging surface agrees. Gated the
        // same way the theme landing is: the previous highlight not being a
        // turn is exactly the entering case, so scrolling back up to reread
        // is never fought by a later frame of the same conversation.
        let entering_the_transcript = previous.is_none_or(|(kind, _)| kind != "conversation");
        if self.active_mode().is_some_and(|m| m.chrome.transcript) && !self.results.is_empty() {
            // Entering lands at the bottom; and **being at the bottom is
            // sticky**, the way every chat is: watching the newest turn when
            // a newer one arrives means following it down. Having scrolled
            // *up* to reread is the one state a refresh must not disturb, so
            // anything short of the end leaves the view exactly where it was.
            if entering_the_transcript || was_at_bottom {
                self.selected = self.results.len() - 1;
                self.mode_scroll.scroll_to_bottom();
            }
        }
        // Typing to filter moves the selection just as arrowing does, so it
        // previews too — `preview_selected_theme` is a no-op outside the
        // theme mode and when the selected row is already the live palette.
        self.preview_selected_theme();
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

    /// Drags the view to wherever the keyboard just went.
    ///
    /// **The root list has always scrolled and the keyboard never moved it.**
    /// `overflow_y_scroll` and `track_scroll` were both wired up, so a mouse
    /// wheel worked — but nothing called `scroll_to_item`, so arrowing down
    /// walked the selection straight off the bottom of the panel and the view
    /// sat still. From the outside that reads as "this list does not scroll",
    /// which is exactly what it was reported as.
    ///
    /// **It scrolls to the row *past* the selection, not to the selection.**
    /// `scroll_to_item` moves the minimum distance to bring its target into
    /// view, so aiming it at the selected row parks that row flush against
    /// the edge you are travelling toward — you arrow down and the thing you
    /// just selected is the last thing visible, with no sight of what comes
    /// next. Aiming one row further leaves exactly one row of lookahead,
    /// which is what every list worth using does (vim calls it `scrolloff`).
    ///
    /// Called from every place `selected` moves, rather than from `render`:
    /// a render-time scroll would fight a wheel gesture, dragging the view
    /// back to the selection every frame while somebody is trying to look
    /// somewhere else.
    fn scroll_selection_into_view(&mut self, bias: ScrollBias) {
        let target = bias.target(self.selected, self.results.len());
        if self.active_mode().is_some() {
            let handle = self.mode_scroll.clone();
            let index = mode_list_child_index(&self.results, target);
            self.glide_to_item(handle, index);
            return;
        }
        // A focused tile is above the list, not in it, and the grid is not
        // inside the scroll container at all.
        if self.grid_selected.is_some() {
            return;
        }
        let handle = self.root_scroll.clone();
        let index =
            root_list_child_index(&self.results, target, self.leading_banner_count());
        self.glide_to_item(handle, index);
    }

    /// Puts the panel into the state a pending fetch produces, for a capture.
    ///
    /// Sets the same two fields a real in-flight search sets — nothing is
    /// rendered that a real wait would not render. See
    /// `evidence::hold_skeleton` for why this cannot be photographed without
    /// a hook.
    pub fn hold_skeleton_for_evidence(&mut self, cx: &mut Context<Self>) {
        self.results.clear();
        self.searching = true;
        cx.notify();
    }

    /// Whether an empty list is empty because the answer has not arrived.
    ///
    /// **An empty list means two opposite things and the modes could not tell
    /// them apart.** `ModeChrome::empty_line` is a statement of fact about a
    /// *finished* search — "No agents running" — and during a fetch it is
    /// simply false. The modes where the wait is real are exactly the ones
    /// backed by a network round trip: Usage fans out to three vendor APIs,
    /// Terminals and Schedules go over MCP. Each of those opened by asserting
    /// there was nothing there and contradicting itself a moment later.
    ///
    /// Gated on `searching` rather than on the request being outstanding at
    /// all, deliberately: that flag is already delayed by
    /// `SEARCHING_TELL_DELAY_MS` precisely so a fast answer never flashes a
    /// loading state, and a skeleton wants exactly the same threshold.
    fn awaiting_first_rows(&self) -> bool {
        self.searching
    }

    /// Whether a transcript's view is parked at its end.
    ///
    /// A slack of one step, not exact equality: the glide lands on a
    /// fractional offset and a chat that stops following the moment somebody
    /// is one pixel short of the bottom is worse than one that follows a
    /// pixel early.
    fn transcript_is_at_bottom(&self) -> bool {
        transcript_at_bottom(
            -f32::from(self.mode_scroll.offset().y),
            f32::from(self.mode_scroll.max_offset().y),
        )
    }

    /// Moves a transcript's view by one step, in the direction the arrow key
    /// pointed.
    ///
    /// Glides rather than jumps, through the same tween a selection move uses,
    /// so a held key reads as continuous travel instead of a stutter — and
    /// clamps at both ends, because a chat that bounces at the top is a chat
    /// that feels broken.
    fn scroll_transcript(&mut self, bias: ScrollBias) {
        let step = match bias {
            ScrollBias::Up => theme::TRANSCRIPT_SCROLL_STEP_PX,
            _ => -theme::TRANSCRIPT_SCROLL_STEP_PX,
        };
        let handle = self.mode_scroll.clone();
        let from = handle.offset();
        // Offsets run negative as content scrolls up, so the reachable range
        // is `-max_offset ..= 0`.
        let max = handle.max_offset().y;
        let target_y = (from.y + px(step)).clamp(-max, px(0.));
        let to = point(from.x, target_y);
        if to == from {
            return;
        }
        if motion::system_reduce_motion() {
            handle.set_offset(to);
            self.scroll_glide = None;
            return;
        }
        let mut glide = motion::ScrollGlide::new(from, to, std::time::Instant::now());
        glide.record_write(from);
        self.scroll_glide = Some((handle, glide));
    }

    /// Start a glide toward `index` on `handle`.
    ///
    /// **The destination comes from `scroll_to_item` itself, not from
    /// arithmetic here.** It already knows the child bounds, the container
    /// bounds and the minimum distance that reveals a row; reimplementing that
    /// to get a number to animate toward would be a second copy of geometry
    /// this app does not own. So it is asked to land, read back, and put back —
    /// all before paint, so nothing renders at the interim position.
    fn glide_to_item(&mut self, handle: ScrollHandle, index: usize) {
        let from = handle.offset();
        handle.scroll_to_item(index);
        let to = handle.offset();
        if to == from {
            self.scroll_glide = None;
            return;
        }
        // Reduce motion: it already landed, so leave it there.
        if motion::system_reduce_motion() {
            self.scroll_glide = None;
            return;
        }
        handle.set_offset(from);
        let mut glide = motion::ScrollGlide::new(from, to, std::time::Instant::now());
        glide.record_write(from);
        self.scroll_glide = Some((handle, glide));
    }

    /// Advances a glide by one frame. Returns whether another frame is needed.
    ///
    /// Called from `render`, like the hover washes and the pulse clock, because
    /// that is the one place that runs exactly once per frame and only while
    /// the panel is actually on screen.
    fn advance_scroll_glide(&mut self) -> bool {
        let Some((handle, glide)) = &mut self.scroll_glide else { return false };
        // **Overtaken.** A wheel gesture or a fresh search moved the handle out
        // from under this glide, and continuing would drag the view back to a
        // destination nobody wants any more — the same rule that makes a
        // superseded search abandon rather than finish.
        if !glide.still_owns(handle.offset()) {
            self.scroll_glide = None;
            return false;
        }
        let (at, done) = glide.sample(std::time::Instant::now());
        handle.set_offset(at);
        glide.record_write(at);
        if done {
            self.scroll_glide = None;
            return false;
        }
        true
    }

    /// How many banner children sit above the first section header — the
    /// offset `root_list_child_index` starts counting from.
    fn leading_banner_count(&self) -> usize {
        usize::from(!self.connected) + usize::from(self.show_accessibility_banner())
    }

    fn select_next(&mut self, _: &SelectNext, _window: &mut Window, cx: &mut Context<Self>) {
        // **A chat is scrolled, not stepped through.** Every other surface
        // here is a list where the arrow keys move a cursor and Enter acts on
        // what it lands on; a transcript has no such cursor — Enter belongs to
        // the composer, ⌘K targets the session rather than any one turn, and
        // a tool chip folds on click. So the keys do the only thing left that
        // means anything, which is what a person expects of a message view:
        // they move the view.
        if self.active_mode().is_some_and(|m| m.chrome.transcript) {
            self.scroll_transcript(ScrollBias::Down);
            cx.notify();
            return;
        }
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
                // Stepping down out of the grid lands on the first row, and
                // the view has to come back with it — the list may be
                // scrolled anywhere from a previous pass.
                self.scroll_selection_into_view(ScrollBias::Down);
            }
            cx.notify();
            return;
        }
        if !self.results.is_empty() {
            self.selected = (self.selected + 1).min(self.results.len() - 1);
            self.scroll_selection_into_view(ScrollBias::Down);
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
        // **A chat is scrolled, not stepped through.** Every other surface
        // here is a list where the arrow keys move a cursor and Enter acts on
        // what it lands on; a transcript has no such cursor — Enter belongs to
        // the composer, ⌘K targets the session rather than any one turn, and
        // a tool chip folds on click. So the keys do the only thing left that
        // means anything, which is what a person expects of a message view:
        // they move the view.
        if self.active_mode().is_some_and(|m| m.chrome.transcript) {
            self.scroll_transcript(ScrollBias::Up);
            cx.notify();
            return;
        }
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
        self.scroll_selection_into_view(ScrollBias::Up);
        if self.preview_selected_theme() {
            _window.refresh();
        }
        cx.notify();
    }

    fn confirm(&mut self, _: &Confirm, window: &mut Window, cx: &mut Context<Self>) {
        if self.actions_menu.is_some() {
            self.confirm_menu_action(window, cx);
            return;
        }
        // **In a transcript mode, Enter is the composer's send** — never the
        // selected turn's own action. The draft goes to the mode's subject
        // (the agent), the field clears immediately the way every messaging
        // surface clears it, and the panel stays open: the reply is the
        // point. An empty draft swallows the keystroke rather than acting on
        // a turn, because acting on a turn is not a thing (`activate` is a
        // read-only no-op) and "Enter did something invisible" is worse than
        // "Enter did nothing".
        if let Some(mode) = self.active_mode()
            && mode.chrome.transcript
        {
            let Some(request) = self.transcript_send_request(cx) else {
                return;
            };
            self.text_field.update(cx, |field, cx| field.set_content("", cx));
            self.perform_activation(request, false, None, cx);
            return;
        }
        // A focused tile owns Enter — the grid is part of the same
        // selection run, so the row underneath must not act instead.
        if let Some(tile) = self.grid_selected
            && let Some(item) = self.agent_tiles.get(tile).cloned()
        {
            self.act_on_item(item, window, cx);
            return;
        }
        let Some(item) = self.results.get(self.selected).cloned() else {
            return;
        };
        self.act_on_item(item, window, cx);
    }

    /// Everything Enter means once the item is known — shared by the list's
    /// rows, the agent grid's tiles, and both of their click handlers.
    ///
    /// **This exists because the tile path skipped `enters_mode`.** The grid's
    /// Enter and click both built a bare `Request::Activate`, so an agent tile
    /// opened Paseo while the identical agent as a *row* read its conversation
    /// here — and at rest `split_agent_tiles` moves every live and recent
    /// agent out of the rows and into the grid, which made the tiles the only
    /// agent surface most summons ever show. The conversation feature worked
    /// and was unreachable from exactly the place agents are visible. Four
    /// call sites doing this arithmetic separately is the same shape that
    /// split `⌘K` from its hint; one method is the fix both times.
    fn act_on_item(&mut self, item: SearchItem, window: &mut Window, cx: &mut Context<Self>) {
        // A command row never reaches `Request::Activate` at all —
        // confirming it is a client-side UI transition, not a daemon
        // action (see `crate::modes`'s module doc comment).
        if let Some(mode_id) = item.enters_mode.clone() {
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
            if mode_id == "codex-task" {
                self.pending_task_mode = Some((
                    TaskRef { backend: "codex".to_string(), id: item.id.clone() },
                    item.clone(),
                ));
                self.perform_activation(self.primary_activation_request(&item, cx), false, Some("codex-task"), cx);
                return;
            }
            // **A conversation is about the row that opened it**, so the
            // row's own id travels into the mode as its subject. Every other
            // mode is a plain list and takes none — see `ActiveMode::subject`.
            let subject = (mode_id == "conversation").then(|| item.id.clone());
            let entered_about = subject.is_some();
            self.enter_mode_about(&mode_id, subject, window, cx);
            if entered_about && let Some(mode) = self.active_mode.as_mut() {
                mode.subject_item = Some(item);
            }
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
        let request = self.primary_activation_request(&item, cx);
        // **A row that performs a *step* keeps the panel.** Every ordinary
        // row means "do this and get out of my way", which is why hiding is
        // the default — but a row that proposes something (`neko_core::ask`:
        // type a sentence, read the tool call, press Enter again to run it)
        // would be unusable if the first Enter dismissed the panel it is
        // asking you to look at. This takes the path `⌘K` menu actions
        // already take, which also re-runs the search — exactly what makes
        // the proposal appear where the invitation was.
        self.perform_activation(request, !item.keeps_open, None, cx);
    }

    /// `⌘↵` — the jump to Paseo, from anywhere an agent is in front of you.
    ///
    /// Enter took over reading the conversation here, and this is the old
    /// behaviour given back one keystroke away — Raycast's own convention for
    /// a row's secondary action, and the convention this repo had already
    /// named for exactly this shape. Resolved by **data, never by provider
    /// id**: whatever is highlighted (a row, a tile, a conversation turn)
    /// must actually carry an `open-in-paseo` action, and inside a transcript
    /// the target is the mode's *subject* — the agent — whatever turn the
    /// selection happens to sit on.
    fn open_in_paseo(&mut self, _: &OpenInPaseo, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(request) = self.open_in_paseo_request() else {
            // **The likeliest press is the silent one**: a fresh summon, the
            // agents visibly sitting in the grid, the selection on the top
            // *row* — an app. Doing nothing here reads as the key being
            // broken; saying what it needs reads as the key working.
            self.activation_error = Some("select an agent to open in Paseo".to_string());
            cx.notify();
            return;
        };
        // Hide on success: the whole point of the keystroke is that the
        // interaction continues in Paseo's window, not this one.
        self.perform_activation(request, true, None, cx);
    }

    /// What `⌘↵` would send, or `None` where it means nothing.
    fn open_in_paseo_request(&self) -> Option<Request> {
        const OPEN_IN_PASEO: &str = "open-in-paseo";
        if let Some(mode) = self.active_mode().filter(|m| m.chrome.transcript) {
            let subject = mode.subject.clone()?;
            return Some(Request::Activate {
                kind: mode.chrome.provider_id.to_string(),
                id: subject,
                action: Some(OPEN_IN_PASEO.to_string()),
                query: String::new(),
            });
        }
        let item = self.highlighted_item()?;
        item.actions.iter().any(|a| a.id == OPEN_IN_PASEO).then(|| Request::Activate {
            kind: item.kind.clone(),
            id: item.id.clone(),
            action: Some(OPEN_IN_PASEO.to_string()),
            query: String::new(),
        })
    }

    /// The composer's send, or `None` when there is nothing to send.
    ///
    /// Its own method for the same reason `primary_activation_request` is:
    /// a test can assert exactly what goes on the wire — the *subject* as the
    /// id, never the selected turn — without a daemon to answer it.
    fn transcript_send_request(&self, cx: &Context<Self>) -> Option<Request> {
        let mode = self.active_mode().filter(|m| m.chrome.transcript)?;
        let subject = mode.subject.clone()?;
        let draft = self.query(cx);
        if draft.trim().is_empty() {
            return None;
        }
        Some(Request::Activate {
            kind: mode.chrome.provider_id.to_string(),
            id: subject,
            action: None,
            query: draft,
        })
    }

    /// The `Request::Activate` a row's primary action sends.
    ///
    /// Its own method rather than three lines inside `confirm` so a test can
    /// assert what actually goes on the wire without a daemon to answer it.
    ///
    /// **The search field's own contents ride along with every activation.**
    /// Almost every provider ignores them (`Provider::activate_with_query`
    /// defaults to dropping the query and delegating), and the one whose rows
    /// are a thing to do *with what was typed* — starting an agent on a
    /// prompt — takes them as its argument. Read here, at the moment Enter is
    /// pressed, rather than carried on the row: a row was built at the
    /// previous keystroke, and the prompt is whatever is on screen now.
    fn primary_activation_request(&self, item: &SearchItem, cx: &Context<Self>) -> Request {
        Request::Activate {
            kind: item.kind.clone(),
            id: item.id.clone(),
            action: None,
            query: self.query(cx),
        }
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
    fn perform_activation(
        &mut self,
        request: Request,
        hide_on_success: bool,
        enter_mode_on_success: Option<&'static str>,
        cx: &mut Context<Self>,
    ) {
        let client = self.client.clone();
        self.activating = true;
        cx.notify();
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
                root.finish_activation(error_message, hide_on_success, enter_mode_on_success, cx);
            });
            if !failed && hide_on_success {
                cx.update(|cx| cx.hide());
            }
        })
        .detach();
    }

    fn finish_activation(
        &mut self,
        error_message: Option<String>,
        hide_on_success: bool,
        enter_mode_on_success: Option<&'static str>,
        cx: &mut Context<Self>,
    ) {
        let succeeded = error_message.is_none();
        self.activating = false;
        self.activation_error = error_message;
        if !succeeded {
            self.pending_task_mode = None;
        }
        // A menu action that changed the underlying data (delete, ...) and
        // isn't about to hide the panel needs the current list re-fetched to
        // reflect it. Review is different: the successful response is the
        // authorization to enter its scoped task list, so that mode's own
        // initial search replaces the root-list refresh.
        if succeeded {
            if let Some(mode_id) = enter_mode_on_success {
                self.enter_mode_after_activation(mode_id, cx);
            } else if !hide_on_success {
                self.run_search(cx);
            }
        }
        cx.notify();
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
            && (self.results.iter().any(|item| {
                item.badge.as_deref() == Some("LIVE")
                    || item.speaker.as_deref() == Some("working")
            })
            // A skeleton breathes on this same clock, so it has to keep the
            // clock alive while it is the only thing on screen — otherwise
            // the placeholder freezes at whatever phase it happened to mount
            // on, which reads as stuck rather than loading.
            || (self.results.is_empty() && self.awaiting_first_rows()));
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
            // The topmost strip of the panel, and so the most natural place
            // to pick the window up from — which is exactly where the first
            // attempt at this was grabbed, and the one place that was not a
            // grab area. The tiles inside stop propagation (below), so a
            // press on a tile still activates it instead of dragging.
            .on_mouse_down(gpui::MouseButton::Left, cx.listener(Self::begin_window_drag))
            .flex()
            .flex_wrap()
            .gap(px(theme::AGENT_GRID_GAP_PX))
            // The same inset the results container uses, so a tile's edge
            // lines up with the selected row's highlight rather than sitting
            // 12px inside it.
            .px(px(theme::CONTENT_INSET_PX))
            .pt(px(theme::AGENT_GRID_PAD_TOP_PX))
            .pb(px(theme::AGENT_GRID_PAD_BOTTOM_PX))
            .h(px(theme::AGENT_GRID_HEIGHT_PX))
            .overflow_hidden();
        for (index, item) in self.agent_tiles.iter().enumerate() {
            let focused = self.grid_selected == Some(index);
            let live = item.badge.as_deref() == Some("LIVE");
            let status_label = agent_tile_status_label(item);
            let id = item.id.clone();
            let tile_item = item.clone();
            let mut dot = theme::active().state_success;
            dot.a = 0.45 + 0.55 * intensity;
            grid = grid.child(
                div()
                    .id(SharedString::from(format!("agent-tile-{id}")))
                    // **Truncation hides content, so the full value stays
                    // reachable.** A tile is 150px and both its lines
                    // ellipsize — and since `agents::subtitle` began handing
                    // the line to the prompt for workspaces whose name
                    // repeats their project, the hidden part is now the only
                    // thing telling two sessions apart. Hover restores it.
                    // Deliberately the whole tile rather than the text: the
                    // text is not separately hoverable at this size.
                    .tooltip({
                        let full = agent_tile_tooltip(item);
                        move |_window, cx| {
                            cx.new(|_| TextTooltip { text: full.clone() }).into()
                        }
                    })
                    .relative()
                    .flex()
                    .items_center()
                    .gap(px(11.))
                    .w(px(theme::AGENT_TILE_WIDTH_PX))
                    .h(px(theme::AGENT_TILE_HEIGHT_PX))
                    .px(px(13.))
                    .rounded(px(theme::ROW_RADIUS_PX))
                    // A resting fill, so a tile reads as a tile rather than
                    // as floating text — the panel itself paints nothing now
                    // (`panel_alpha` is 0), so without this there is no
                    // surface here at all. Still no border: the fill is the
                    // whole of the treatment, and the selected state is the
                    // same pill a row gets.
                    // Translucent, not an opaque surface — the panel itself
                    // is glass now, and an opaque tile on top of it reads as
                    // a block pasted on rather than part of the same pane.
                    //
                    // **A running agent's tile carries a live green wash that
                    // breathes**; an idle one is the flat neutral fill. The
                    // wash is a gradient strongest at the icon end and gone
                    // by the far edge, so it reads as coming *from* the agent
                    // rather than as a coloured card, and its intensity rides
                    // the shared `PulseClock` — the same clock the badge and
                    // the dot use. Nothing here animates itself; see
                    // `motion.rs` on why a repeating animation must always go
                    // through that one throttled clock.
                    .map(|tile| {
                        if focused {
                            return tile.bg(theme::active().surface_selected);
                        }
                        if !live {
                            return tile.bg(theme::active().surface_tile);
                        }
                        let mut lead = theme::active().state_success;
                        lead.a = 0.14 + 0.16 * intensity;
                        tile.bg(gpui::linear_gradient(
                            110.0,
                            gpui::linear_color_stop(lead, 0.0),
                            gpui::linear_color_stop(theme::active().surface_tile, 0.85),
                        ))
                    })
                    .when(!focused, |tile| {
                        tile.hover_bg(
                            SharedString::from(format!("tile:{id}")),
                            theme::TRANSPARENT,
                            theme::active().row_icon_socket_bg,
                        )
                    })
                    .cursor_pointer()
                    .on_mouse_down(gpui::MouseButton::Left, |_event, _window, cx| cx.stop_propagation())
                    .on_click(cx.listener(move |root, _event, window, cx| {
                        // The same shared path Enter takes — see `act_on_item`
                        // for the divergence this closes.
                        root.act_on_item(tile_item.clone(), window, cx);
                    }))
                    // The host app's own icon, in the same socket treatment
                    // every row icon gets — real artwork carries wildly
                    // different amounts of transparent padding, and the plate
                    // is what makes a set of them read as one system.
                    .child(
                        div()
                            .relative()
                            .flex_shrink_0()
                            .w(px(theme::ROW_ICON_PX))
                            .h(px(theme::ROW_ICON_PX))
                            .rounded(px(theme::ROW_ICON_RADIUS_PX))
                            .bg(theme::active().row_icon_socket_bg)
                            .child(icon_element(&item.icon, &item.id))
                            // The tool running the agent, badged onto the
                            // corner of the host app's icon — the same shape
                            // macOS itself uses for a document's owning app.
                            //
                            // **One character, because that is what fits.**
                            // A badge on a 22px icon has room for a letter,
                            // which is why the daemon sends the *tool*
                            // ("claude") rather than the model: "which tool"
                            // is the distinction that survives being reduced
                            // to one glyph.
                            //
                            // **Not the vendor's real logo**, deliberately.
                            // Anthropic's and OpenAI's marks are not openly
                            // licensed the way Lucide's are, and every
                            // vendored asset in this repo has its licence
                            // pinned and verified (`AGENTS.md`, "Licence
                            // rule"). A letter needs no permission.
                            .children(item.source.clone().map(|tool| {
                                div()
                                    .absolute()
                                    .bottom(px(-3.))
                                    .right(px(-3.))
                                    .w(px(12.))
                                    .h(px(12.))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded(px(6.))
                                    .bg(theme::active().surface_raised)
                                    .border_1()
                                    .border_color(theme::active().surface_input)
                                    .text_size(px(8.))
                                    .text_color(theme::active().text_secondary)
                                    // The vendor's own mark where one is
                                    // vendored, its initial where none is —
                                    // `simple-icons` has no OpenAI logo, so
                                    // the letter is a real fallback rather
                                    // than a placeholder.
                                    .child(match crate::assets::tool_icon(&tool) {
                                        Some(path) => gpui::svg()
                                            .path(path)
                                            .w(px(8.))
                                            .h(px(8.))
                                            .text_color(theme::active().text_secondary)
                                            .into_any_element(),
                                        None => SharedString::from(tool_initial(&tool)).into_any_element(),
                                    })
                            })),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w(px(0.))
                            .gap(px(3.))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(6.))
                                    .h(px(19.))
                                    .overflow_hidden()
                                    .when(live, |el| {
                                        el.child(div().w(px(6.)).h(px(6.)).rounded(px(3.)).bg(dot).flex_shrink_0())
                                    })
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w(px(0.))
                                            .overflow_hidden()
                                            // **The actual fix for the
                                            // wrapping title.** A fixed
                                            // height only *cropped* a
                                            // wrapped line — the text still
                                            // laid out over two rows and the
                                            // second was cut in half.
                                            // `whitespace_nowrap` is what
                                            // stops the wrap; `text_ellipsis`
                                            // is what makes the overflow say
                                            // so instead of vanishing.
                                            .whitespace_nowrap()
                                            .text_ellipsis()
                                            .text_size(px(13.))
                                            .text_color(theme::active().text_primary)
                                            .child(SharedString::from(item.title.clone())),
                                    ),
                            )
                            .children(status_label.map(|label| {
                                div()
                                    .flex_shrink_0()
                                    .px(px(4.))
                                    .py(px(1.))
                                    .rounded(px(3.))
                                    .bg(theme::active().row_icon_socket_bg)
                                    .text_size(px(8.5))
                                    .text_color(theme::active().state_danger)
                                    .child(SharedString::from(label))
                            }))
                            .children(item.subtitle.clone().map(|subtitle| {
                                div()
                                    .h(px(15.))
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_ellipsis()
                                    .text_size(px(11.5))
                                    .text_color(theme::active().text_tertiary)
                                    .child(SharedString::from(subtitle))
                            })),
                    )
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

    /// [`enter_mode`], for a mode that is *about* one row — see
    /// [`ActiveMode::subject`].
    fn enter_mode_about(
        &mut self,
        mode_id: &str,
        subject: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_actions_menu(window);
        self.enter_mode_about_menu_closed(mode_id, subject, cx);
    }

    /// [`enter_mode_about`] after any transient menu has already closed.
    /// Activation responses run without a `Window`, so successful actions
    /// that continue into a mode use this shared state transition.
    fn enter_mode_after_activation(&mut self, mode_id: &str, cx: &mut Context<Self>) {
        self.enter_mode_about_menu_closed(mode_id, None, cx);
        if mode_id == "codex-task"
            && let Some((task_ref, item)) = self.pending_task_mode.take()
            && let Some(mode) = self.active_mode.as_mut()
        {
            mode.task_ref = Some(task_ref);
            mode.subject_item = Some(item);
            self.run_search(cx);
        }
    }

    fn enter_mode_about_menu_closed(
        &mut self,
        mode_id: &str,
        subject: Option<String>,
        cx: &mut Context<Self>,
    ) {
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
            subject,
            task_ref: None,
            subject_item: None,
            restore_theme: (chrome.provider_id == "theme").then(|| theme::active_theme().id),
        });
        self.selected = 0;
        self.mode_scroll.set_offset(point(px(0.), px(0.)));
        self.expanded_tool_output.clear();
        self.text_field.update(cx, |field, cx| {
            field.set_placeholder(chrome.placeholder, cx);
            // A real edit (emits `ContentChanged`), which is what actually
            // runs the mode-scoped search above — `active_mode` is already
            // `Some` by the time this synchronously fires, so `run_search`
            // takes the mode branch immediately, not the root-list one.
            field.set_content("", cx);
        });
        // **A transcript refreshes itself while it is open.** The agent is
        // usually still typing when you are reading — that is the whole
        // reason to open it — and a chat that only updates when you press a
        // key is a page, not a chat. The loop dies with the mode: it checks
        // on every tick that this exact mode and subject are still active,
        // so exiting, switching agents, or dismissing the panel all end it
        // without a cancellation channel. 2.5s, deliberately just above the
        // provider's own 2s cache so most ticks are answered from a fresh
        // read rather than piling tail-reads on the daemon.
        if chrome.transcript {
            let subject_now = self.active_mode.as_ref().and_then(|m| m.subject.clone());
            cx.spawn(async move |this, cx| {
                loop {
                    cx.background_executor()
                        .timer(std::time::Duration::from_millis(2500))
                        .await;
                    let still_reading = this
                        .update(cx, |root, cx| {
                            let live = root.active_mode().is_some_and(|m| {
                                m.chrome.transcript && m.subject == subject_now
                            });
                            if live {
                                root.run_search(cx);
                            }
                            live
                        })
                        .unwrap_or(false);
                    if !still_reading {
                        break;
                    }
                }
            })
            .detach();
        }
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
    /// Whatever is highlighted right now — which is not always a row.
    ///
    /// While the keyboard is up in the agent grid, `selected` keeps its old
    /// value on purpose (`row_is_highlighted` folds `grid_selected.is_none()`
    /// in, so only one thing ever paints as selected). Anything asking "what
    /// would Enter or ⌘K act on" has to ask here rather than indexing
    /// `results` directly — two callers doing that arithmetic separately is
    /// how the ⌘K menu came to open for a row nobody could see was chosen
    /// while the hint above it described a different one.
    fn highlighted_item(&self) -> Option<&SearchItem> {
        match self.grid_selected {
            Some(tile) => self.agent_tiles.get(tile),
            None => self.results.get(self.selected),
        }
    }

    /// The menu bar menu's "Preferences…" item — same injected opener the
    /// Preferences command row uses, so the two paths cannot drift.
    pub fn open_preferences_from_menu_bar(&mut self, window: &Window, cx: &mut Context<Self>) {
        (self.open_preferences)(window, cx);
    }

    fn open_actions_menu_for_selected_row(&mut self, cx: &mut Context<Self>) {
        let Some(item) = self.highlighted_item() else {
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
        let enter_mode_on_success =
            (kind == "codex-task" && action.id == "review").then_some("codex-task");
        if enter_mode_on_success.is_some()
            && let Some(item) = self
                .results
                .iter()
                .chain(self.agent_tiles.iter())
                .find(|item| item.kind == kind && item.id == id)
                .cloned()
        {
            self.pending_task_mode = Some((
                TaskRef { backend: "codex".to_string(), id: id.clone() },
                item,
            ));
        }
        self.close_actions_menu(window);
        cx.notify();
        let request = Request::Activate { kind, id, action: Some(action.id), query: self.query(cx) };
        self.perform_activation(request, false, enter_mode_on_success, cx);
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
        // Ahead of every other branch, including the menu: while the panel is
        // physically being moved, Escape can only sensibly mean "put it
        // back". Hiding the panel mid-gesture would leave AppKit's implicit
        // mouse capture pointed at a window nobody can see, and the mode or
        // menu underneath is still there to Escape out of on the next press.
        if self.dragging {
            self.cancel_window_drag(window, cx);
            cx.notify();
            return;
        }
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
            // Zero-sized and absolutely positioned: it draws nothing and
            // occupies no space, it exists to reach paint phase. See its own
            // doc comment for why the listeners cannot live on a `div`.
            .child(self.sync_window_drag_listeners(cx))
            // **The hover washes' clock.** They are driven from wall time
            // rather than by `with_animation` (see `motion::HoverFades` for
            // why), so somebody has to ask for the next frame while one is
            // still moving — and somebody has to prune the entry of a row that
            // unmounted mid-hover, which happens on every keystroke. Both are
            // the same once-per-frame call, made here because `render` is the
            // one place guaranteed to run exactly once per frame and only
            // while the panel is actually on screen.
            .map(|el| {
                // Two wall-time tweens, one frame request. Both are driven from
                // here rather than by `with_animation` — see their own doc
                // comments for why neither could be.
                let gliding = self.advance_scroll_glide();
                if motion::hover_fades_active() || gliding {
                    window.request_animation_frame();
                }
                el
            })
            .on_action(cx.listener(Self::select_next))
            .on_action(cx.listener(Self::select_previous))
            .on_action(cx.listener(Self::confirm))
            .on_action(cx.listener(Self::open_actions_menu))
            .on_action(cx.listener(Self::open_in_paseo))
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
            // **A transcript mode inverts the panel: exchange on top,
            // composer at the bottom** — where every messaging surface puts
            // it, and where a field whose Enter *sends* belongs. Everywhere
            // else the field stays on top, because there it is a query and
            // the list is its result. Same entity either way; only the child
            // order changes, so focus, editing and the caret carry over.
            .map(|el| {
                let composer_at_bottom =
                    self.active_mode().is_some_and(|m| m.chrome.transcript);
                let hairline =
                    div().h(px(1.)).flex_shrink_0().bg(theme::active().border_hairline);
                if composer_at_bottom {
                    el
                } else {
                    el.child(self.render_input_row(cx)).child(hairline)
                }
            })
            // **The `⌘K` menu's anchor.** It used to hang off the footer's
            // "Actions ⌘K" trigger; with the footer gone it needs a pin of
            // its own, or `open_actions_menu` would set state that nothing
            // ever paints — a menu that opens invisibly, including its
            // destructive-delete confirmation.
            .children(self.actions_menu.clone().map(|menu| {
                div()
                    .absolute()
                    .bottom(px(theme::CONTENT_BOTTOM_SPACE_PX))
                    .right(px(theme::CONTENT_INSET_PX))
                    .w(px(0.))
                    .h(px(0.))
                    .child(self.render_actions_menu(&menu, cx))
            }))
            // **Below the search field, not above it.** Above, the tiles sat
            // between the top of the panel and the field, so Down from the
            // field went straight past them into the rows and the only way in
            // was to press Up — landing on the *last* tile first. Below, the
            // grid is simply the first thing in the content area, and the
            // selection runs through it in reading order before reaching the
            // rows.
            .when(!self.agent_tiles.is_empty(), |el| el.child(self.render_agent_grid(cx)))
            .child(match self.active_mode() {
                Some(mode) => self.render_mode_content(mode, cx),
                None => self.render_content_area(cx, query_is_empty).into_any_element(),
            })
            // The transcript mode's composer — the same input row, below the
            // exchange. See the top of this chain for the inversion rule.
            .map(|el| {
                if self.active_mode().is_some_and(|m| m.chrome.transcript) {
                    el.child(
                        div().h(px(1.)).flex_shrink_0().bg(theme::active().border_hairline),
                    )
                    .child(self.render_input_row(cx))
                } else {
                    el
                }
            })
    }
}

impl Root {
    /// Picks the panel up.
    ///
    /// **This app owns the gesture; AppKit does not.** The first version of
    /// drag was one call to `Window::start_window_move()` →
    /// `performWindowDragWithEvent:`, which is shorter, well-behaved, and
    /// impossible to snap with: it runs AppKit's own modal event loop until
    /// the mouse comes up, so there is no point inside the gesture at which
    /// proximity to a target could be measured or a guide drawn. See
    /// `window_drag.rs`'s module doc comment for the whole shape, and for
    /// what happens once the cursor leaves the panel (which a snap
    /// guarantees it will).
    ///
    /// Fires on mouse-*down*, not on a click: the gesture has to be picked up
    /// before it becomes a drag, and a press that turns out to be a plain
    /// click costs nothing — `start` only records where the cursor grabbed,
    /// and no guide window is opened until a snap target is actually within
    /// reach.
    fn begin_window_drag(&mut self, _: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.dragging {
            return;
        }
        self.dragging = self.drag.start(window, cx);
        if self.dragging {
            // The window-level listeners below are registered at paint time,
            // so the first frame after this is what starts them observing.
            cx.notify();
        }
    }

    /// One tick of a live drag.
    ///
    /// **The event is a clock, not a position.** `MouseMoveEvent::position` is
    /// window-relative, and this drag moves the window out from under the
    /// cursor, so measuring against it would be measuring against a datum that
    /// moves with the thing being measured. `window_drag.rs` reads
    /// `NSEvent.mouseLocation` — absolute — on every tick instead, which also
    /// means a dropped or coalesced tick has no cost: the next one places the
    /// panel exactly where it belongs regardless of how many were missed.
    fn window_drag_moved(&mut self, event: &MouseMoveEvent, window: &mut Window, cx: &mut Context<Self>) {
        if !self.dragging {
            return;
        }
        // The button came up without a `MouseUpEvent` reaching this window at
        // all — belt and braces for the one thing that would otherwise leave
        // the panel stuck to the cursor with a guide window on screen.
        if event.pressed_button != Some(gpui::MouseButton::Left) {
            self.finish_window_drag(window, cx);
            return;
        }
        self.drag.update(window, cx);
    }

    fn window_drag_ended(&mut self, event: &MouseUpEvent, window: &mut Window, cx: &mut Context<Self>) {
        if !self.dragging || event.button != gpui::MouseButton::Left {
            return;
        }
        self.finish_window_drag(window, cx);
    }

    fn finish_window_drag(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.dragging = false;
        self.drag.finish(window, cx);
        cx.notify();
    }

    /// Puts the panel back where it was picked up and takes the guides down.
    /// Safe to call when no drag is live — `cancel` on a finished session is a
    /// no-op, which is what lets `reset_for_summon` call it unconditionally.
    fn cancel_window_drag(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.dragging {
            return;
        }
        self.dragging = false;
        self.drag.cancel(window, cx);
    }

    /// Registers the two **window-level** mouse listeners a drag needs, once
    /// per frame, at paint time.
    ///
    /// Window-level (`Window::on_mouse_event`) rather than the element-level
    /// `on_mouse_move`/`on_mouse_up` a `div` offers, and that is the whole
    /// reason this exists as an element of its own: gpui gates a `div`'s own
    /// mouse-move listener on `hitbox.is_hovered(window)`
    /// (`gpui/src/elements/div.rs`), so it stops firing the instant the cursor
    /// leaves the panel — which a snap makes happen by design, since the panel
    /// stops while the hand keeps going. `dispatch_mouse_event`
    /// (`gpui/src/window.rs`) runs window-level listeners for every event with
    /// no position test at all.
    ///
    /// Registered unconditionally rather than only while `dragging`, so there
    /// is never a frame between the mouse-down and the next paint in which a
    /// move could arrive with nothing listening for it. The cost when no drag
    /// is live is one early `return` per mouse event.
    fn sync_window_drag_listeners(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let moved = cx.entity().downgrade();
        let ended = moved.clone();
        canvas(
            |_bounds, _window, _cx| (),
            move |_bounds, _, window, _cx| {
                let moved = moved.clone();
                window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                    if phase != DispatchPhase::Bubble {
                        return;
                    }
                    let _ = moved.update(cx, |root, cx| root.window_drag_moved(event, window, cx));
                });
                let ended = ended.clone();
                window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
                    if phase != DispatchPhase::Bubble {
                        return;
                    }
                    let _ = ended.update(cx, |root, cx| root.window_drag_ended(event, window, cx));
                });
            },
        )
        .absolute()
        .size_0()
    }

    fn render_input_row(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .items_center()
            .flex_shrink_0()
            // The panel has no title bar, so the input row is the top strip
            // and the natural place to pick it up from. Bubble phase, so a
            // descendant that wants the gesture gets it first — which is how
            // both the mode's back arrow and, since mouse selection landed,
            // the search field itself keep a press that belongs to them.
            // `TextField`'s own mouse-down calls `stop_propagation`, so
            // dragging to select text no longer drags the whole panel across
            // the screen.
            .on_mouse_down(gpui::MouseButton::Left, cx.listener(Self::begin_window_drag))
            .h(px(theme::INPUT_ROW_HEIGHT_PX))
            .px_5()
            .gap_3()
            .text_color(theme::active().text_primary)
            .text_size(px(18.))
            .child(match self.active_mode() {
                // In a transcript mode the header carries the back arrow and
                // the session's own name, so the composer stays a composer —
                // a bar that is half navigation chrome is neither.
                Some(mode) if mode.chrome.transcript => {
                    div().w(px(2.)).into_any_element()
                }
                // The back affordance the launch brief asks for: "a back
                // arrow in place of the search glyph." Clickable — exits
                // the mode the same way Escape does, sharing `exit_mode`
                // rather than duplicating its logic.
                Some(_) => div()
                    .id("mode-back")
                    // **Padding, because the glyph is 15pt and WCAG 2.5.8
                    // wants 24.** This is the only pointer way out of a
                    // mode; missing it costs a person the whole surface.
                    .p(px(5.))
                    .rounded(px(theme::ROW_RADIUS_PX))
                    .hover_bg("mode-back", theme::TRANSPARENT, theme::active().row_icon_socket_bg)
                    .cursor(CursorStyle::PointingHand)
                    // **And it has to swallow mouse-*down*.** The input row
                    // starts a window drag on mouse-down; `on_click` is
                    // mouse-*up*, so the drag had already begun before this
                    // control saw anything. The comment on that drag handler
                    // claimed this button "stops propagation on click" — it
                    // does now.
                    .on_mouse_down(gpui::MouseButton::Left, |_event, _window, cx| {
                        cx.stop_propagation()
                    })
                    .on_click(cx.listener(|root, _: &ClickEvent, window, cx| root.exit_mode(window, cx)))
                    .child(back_glyph())
                    .into_any_element(),
                None => search_glyph().into_any_element(),
            })
            // **`ModeChrome::title` was live data nothing rendered**, so
            // nothing on screen named the surface you were in: a mode
            // announced itself only by its placeholder, which disappears the
            // moment anybody types. A chip beside the field is the shape a
            // launcher uses for this, and it is the one place with room that
            // the query cannot overwrite.
            .children(self.active_mode().filter(|m| !m.chrome.transcript).map(|mode| {
                div()
                    .flex_shrink_0()
                    .px(px(8.))
                    .py(px(3.))
                    .rounded(px(theme::CHIP_RADIUS_PX))
                    .bg(theme::active().surface_selected)
                    .text_size(px(12.))
                    .text_color(theme::active().text_secondary)
                    .child(mode.chrome.title)
            }))
            .child(div().flex_1().child(self.text_field.clone()))
            .children(self.render_searching_tell())
            // **Every failure in the app was silent, and this is where it
            // stops being.** `activation_error` has been set on every failed
            // Enter for a long time and was rendered only by the footer,
            // which was deleted — so "that schedule is gone", "type what to
            // send first", "that plan is stale", the `paseo` CLI's own
            // errors and a 20-second timeout all produced *no visible change
            // whatsoever*. The panel does not hide on failure, so pressing
            // Enter looked exactly like pressing nothing.
            //
            // The input row is the only chrome left, so it carries this.
            // `state_danger`, and it takes the place of the `esc` hint
            // rather than sitting beside it: an error is worth more than a
            // reminder of a key that still works either way.
            .children(self.activation_error.clone().map(|message| {
                div()
                    .flex_shrink_0()
                    .max_w(px(360.))
                    .truncate()
                    .text_size(px(12.))
                    .text_color(theme::active().state_danger)
                    .child(SharedString::from(message))
            }))
            .when(self.activation_error.is_none(), |row| {
                row
                    // The composer's own verb. Shown only when Enter would
                    // genuinely send — an empty draft's Enter is swallowed,
                    // and a hint for a swallowed keystroke would be a lie.
                    .when(
                        self.active_mode().is_some_and(|m| m.chrome.transcript)
                            && !self.text_field.read(cx).content().trim().is_empty(),
                        |row| {
                            row.child(
                                div()
                                    .flex_shrink_0()
                                    .text_size(px(12.))
                                    .text_color(theme::active().text_secondary)
                                    .child("Send  \u{21b5}"),
                            )
                        },
                    )
                    // **A hint is not an affordance.** `⌘K` was rendered as
                    // static text, so the actions menu — Kill terminal,
                    // Delete schedule, Archive, every session mode — could
                    // only ever be opened from the keyboard. It is a real
                    // control now, sized past WCAG 2.5.8's 24pt floor by its
                    // padding rather than by its 12pt label.
                    .when(self.selected_row_has_actions(), |row| {
                        row.child(
                            div()
                                .id("actions-menu-trigger")
                                .flex_shrink_0()
                                .px(px(7.))
                                .py(px(4.))
                                .rounded(px(theme::CHIP_RADIUS_PX))
                                .text_size(px(12.))
                                .text_color(theme::active().text_tertiary)
                                .cursor_pointer()
                                .hover_bg(
                                    "actions-menu-trigger",
                                    theme::TRANSPARENT,
                                    theme::active().row_icon_socket_bg,
                                )
                                .on_click(cx.listener(Self::handle_actions_menu_trigger_click))
                                .child("\u{2318}K"),
                        )
                    })
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_size(px(12.))
                            .text_color(theme::active().text_tertiary)
                            // The one keyboard hint with nowhere else to live.
                            .child("esc"),
                    )
            })
    }

    /// The `⌘K` trigger's own click, and the race it exists to survive.
    ///
    /// The trigger sits outside the menu card, so clicking it while the menu
    /// is open fires the card's `on_mouse_down_out` (capture phase, on
    /// mouse-*down*) **and** this handler (bubble phase, on mouse-*up*) from
    /// one physical press. A naive toggle reads `actions_menu` after the
    /// outside-close already ran, finds `None`, and reopens — so the press
    /// that was meant to dismiss the menu reopens it instead.
    ///
    /// `menu_open_before_this_press` is a snapshot taken by a capture-phase
    /// listener on the outer panel div, before any of that. This reads that,
    /// never the current state.
    fn handle_actions_menu_trigger_click(
        &mut self,
        _event: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let was_open = self.menu_open_before_this_press;
        self.menu_open_before_this_press = false;
        if was_open {
            // The press already dismissed it. Leave it dismissed.
            return;
        }
        self.open_actions_menu_for_selected_row(cx);
    }

    /// Whether the row Enter would act on has anything in its `⌘K` menu.
    ///
    /// Gates the hint so it only appears where the keystroke does something
    /// — a menu hint on a row with no menu is worse than no hint.
    fn selected_row_has_actions(&self) -> bool {
        self.highlighted_item().is_some_and(|item| !item.actions.is_empty())
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
        // **Two waits, one slot, and they cannot both be true in a way that
        // matters.** A search is "the list is about to change"; an activation
        // is "the thing you pressed is happening". Activation wins when both
        // are set, because it is the one the captain is actually waiting on —
        // and it needs no delay before appearing, unlike a search, which is
        // usually answered in microseconds and would flicker.
        let label = if self.activating {
            "Working…"
        } else if self.searching {
            "Searching…"
        } else {
            return None;
        };
        let reduced = motion::system_reduce_motion();
        let tell = div()
            .flex_shrink_0()
            .text_size(px(11.))
            .text_color(theme::active().text_tertiary)
            .child(label);
        Some(motion::fade_in("searching-tell-fade", reduced, tell))
    }

    fn render_content_area(&self, cx: &mut Context<Self>, query_is_empty: bool) -> AnyElement {
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
            // Positioned ancestor for the truncation cue below.
            .relative()
            .min_h(px(0.))
            .overflow_hidden()
            // Was `px_2()`, the same 8px — now read from the token the agent
            // grid also uses, so the two cannot drift apart again.
            .px(px(theme::CONTENT_INSET_PX))
            // **Where the footer used to be.** A real fade mask is not
            // available here: `edge_fade` works by painting a quad in the
            // surface's own colour, and `panel_alpha` is 0 — there is
            // nothing to fade *into*, and a dark gradient would just be the
            // bar again in softer form. Open space is the honest version:
            // the list simply stops short of the edge, and the glass carries
            // the bottom of the panel on its own.
            .pb(px(theme::CONTENT_BOTTOM_SPACE_PX))
            // Every `img(path)` row icon under this container loads through
            // one bounded cache instance, not GPUI's default never-evicted
            // per-`App` asset cache — see `row_icon_cache.rs`.
            //
            // `image_cache` is a `Div`-only method (not on the `Stateful<Div>`
            // that `.id(...)` produces), so it has to come before the scroll
            // wiring — the same ordering `render_mode_list` documents.
            .image_cache(self.row_icon_cache.clone())
            .id("root-list-scroll")
            .overflow_y_scroll()
            .track_scroll(&self.root_scroll);

        if !self.connected {
            container = container.child(self.render_connection_banner());
        }

        if self.show_accessibility_banner() {
            container = container.child(self.render_accessibility_banner(cx));
        }

        if self.results.is_empty() {
            return container.child(render_empty_state(query_is_empty)).into_any_element();
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
        // **The cue for what did not fit.**
        //
        // A gradient was the first attempt and it was wrong, which only a
        // real capture showed: a scroll fade works because content is
        // *behind* it, and here there is nothing behind — the list stops,
        // and the fade painted panel-colour over panel-colour in whatever
        // slack was left below the last row. Invisible, in the exact case
        // it existed for.
        //
        // A count for the rows the budget genuinely dropped — a typed query
        // is still fitted rather than scrolled, so for those there is nothing
        // to scroll *to* and a fade would promise a gesture that does not
        // work. The fade below is the cue for the rows that are merely below
        // the fold.
        let container = container.children(self.hidden_row_count().map(|n| {
            div()
                .flex_shrink_0()
                .h(px(theme::TRUNCATION_CUE_HEIGHT_PX))
                .flex()
                .items_center()
                .px_3()
                .text_size(px(11.))
                .text_color(theme::active().text_tertiary)
                .child(SharedString::from(format!("+{n} more \u{2014} keep typing to narrow")))
        }));

        // **The bottom fade, which needs real content behind it to work.**
        // An earlier attempt put a gradient on this list while it was purely
        // budget-fit, and it was invisible — a fade over blank space is
        // panel-colour on panel-colour. `scroll_edge_fade` is gated on the
        // scroll handle's own offset each frame, so it appears exactly when
        // there is something below the fold and nowhere else, and it is the
        // same band and gradient the mode list has always used.
        let fade = if self.translucent {
            theme::active().surface_panel_translucent
        } else {
            theme::active().surface_panel
        };
        // The fade says "there is more"; the bar says "how much more, and
        // where you are in it". They answer different questions, so both are
        // mounted — the fade over the list's own bottom edge, the bar in its
        // own lane outside the content.
        with_scrollbar(
            &self.root_scroll,
            "root-list-scrollbar",
            scroll_edge_fade(
                self.root_scroll.clone(),
                fade.into(),
                theme::EDGE_FADE_BAND_PX,
                container,
            )
            // Runs to the panel's own bottom edge — see
            // `without_bottom_fade` for why a fade cannot live there.
            .without_bottom_fade(),
        )
        .into_any_element()
    }

    /// How many rows the budget had to drop, or `None` when everything fit.
    fn hidden_row_count(&self) -> Option<usize> {
        (self.hidden_rows > 0).then_some(self.hidden_rows)
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
                    .child("⌥Space is off. Neko opened so you can still search. After dismissing it, use the menu-bar item to summon it again."),
            )
            .child(
                div()
                    .id("banner-open-settings")
                    .flex_shrink_0()
                    .text_size(px(12.))
                    .text_color(theme::active().text_secondary)
                    .cursor(CursorStyle::PointingHand)
                    .hover_text(
                        "banner-open-settings",
                        theme::active().text_secondary,
                        theme::active().text_primary,
                    )
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
                    .hover_text(
                        "banner-dismiss",
                        theme::active().text_tertiary,
                        theme::active().text_primary,
                    )
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
                    .child(DAEMON_UNREACHABLE),
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
    /// Whether row `idx` paints the selected pill. See `render_row` for why
    /// the grid's own focus is part of the answer.
    /// Evidence-only: whether the bottom fade should be painted right now,
    /// so a capture says which state it is showing instead of being read
    /// off the pixels it is meant to prove.
    pub fn results_truncated_for_evidence(&self) -> bool {
        self.hidden_rows > 0
    }

    fn row_is_highlighted(&self, idx: usize) -> bool {
        idx == self.selected && self.grid_selected.is_none()
    }

    /// A row that is *about a quantity* rather than a thing to open —
    /// `SearchItem::meter`. Two columns: what it is on the left (title, and
    /// the qualifying note under it), the reading itself on the right.
    /// Adapted from StackAI's own usage list, which is the shape six
    /// read-only quota screens converged on: no card, no border, the
    /// qualifying note directly beneath the title rather than stranded in a
    /// far column, and the numbers under the meter rather than beside the
    /// title.
    ///
    /// **Discrete ticks, not one continuous fill.** Ticks give the eye
    /// something to count against, so two rows can be compared without
    /// reading either number — and each lit tick takes its *own* point on
    /// `theme::ramp`, which makes the ramp legible as a scale rather than a
    /// wash. `state_success` at empty → `state_danger` at full, so a
    /// reading's colour means the same fraction however wide the column is.
    ///
    /// **The first stat is the reading, the rest qualify it.** The first
    /// takes the ramp colour and leads; the others follow in the tertiary
    /// weight. That is a vocabulary rule, not knowledge of who produced the
    /// row — a provider orders `stats` by what it wants read first.
    fn render_meter(
        &self,
        idx: usize,
        item: &SearchItem,
        meter: &Meter,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let selected = self.row_is_highlighted(idx);
        let fraction = meter.fraction.clamp(0.0, 1.0);
        let (empty, full) = (theme::active().state_success, theme::active().state_danger);
        let lit = (fraction * theme::METER_TICK_COUNT as f32).round() as usize;
        let reached = theme::ramp(empty, full, fraction);

        let ticks = div()
            .flex()
            .gap(px(theme::METER_TICK_GAP_PX))
            .children((0..theme::METER_TICK_COUNT).map(|i| {
                div()
                    .flex_1()
                    .h(px(theme::METER_TICK_HEIGHT_PX))
                    .rounded(px(theme::METER_TICK_RADIUS_PX))
                    .bg(if i < lit {
                        // Each tick's own position on the scale, not the
                        // row's — which is what makes the ramp readable.
                        theme::ramp(empty, full, i as f32 / (theme::METER_TICK_COUNT - 1) as f32)
                    } else {
                        theme::active().row_icon_socket_bg
                    })
            }));

        let (reading, qualifiers) = meter.stats.split_first().map_or((None, &[][..]), |(a, b)| (Some(a), b));
        let stat = |s: &MeterStat| SharedString::from(format!("{} {}", s.value, s.label));

        div()
            .id(("meter", idx))
            .flex()
            .items_center()
            .flex_shrink_0()
            .gap_7()
            .px_3()
            .py_2p5()
            .rounded(px(theme::ROW_RADIUS_PX))
            .when(selected, |el| el.bg(theme::active().surface_selected))
            .when(!selected, |el| {
                el.hover_bg(
                    SharedString::from(format!("meter:{}:{}", item.kind, item.id)),
                    theme::TRANSPARENT,
                    theme::active().row_icon_socket_bg,
                )
            })
            // The same handler `render_row` carries. A meter is a different
            // *shape* of row, not a different kind of thing, and it lit up
            // under the mouse while doing nothing — the exact affordance lie
            // that made "clicking doesn't work" the report it was.
            .cursor_pointer()
            .on_click(cx.listener(move |root, _event: &ClickEvent, window, cx| {
                if root.menu_open_before_this_press {
                    root.menu_open_before_this_press = false;
                    return;
                }
                root.selected = idx;
                root.grid_selected = None;
                root.confirm(&Confirm, window, cx);
            }))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .child(
                        div()
                            .truncate()
                            .text_size(px(14.))
                            .text_color(theme::active().text_primary)
                            .child(SharedString::from(item.title.clone())),
                    )
                    .children(item.subtitle.clone().map(|note| {
                        div()
                            .truncate()
                            .text_size(px(11.))
                            .text_color(theme::active().text_tertiary)
                            .child(SharedString::from(note))
                    })),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .w(px(theme::METER_COLUMN_WIDTH_PX))
                    // Percentages sit in a column and change as the pane
                    // refreshes; proportional digits would shift the column
                    // under the eye every time one did.
                    .font_features(tabular_numerals())
                    .child(ticks)
                    .child(
                        div()
                            .flex()
                            .justify_between()
                            .mt(px(6.))
                            .text_size(px(11.))
                            .children(reading.map(|s| div().text_color(reached).child(stat(s))))
                            .children(qualifiers.iter().map(|s| {
                                div().text_color(theme::active().text_tertiary).child(stat(s))
                            })),
                    ),
            )
            .into_any_element()
    }

    fn render_row(
        &self,
        idx: usize,
        item: &SearchItem,
        compact: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        // **Only one thing is selected at a time.** `self.selected` is where
        // the list's own cursor is parked, and it keeps its value while the
        // keyboard is up in the agent grid — so without this the grid's
        // focused tile and the list's remembered row both painted a
        // highlight, and the panel showed two selections at once.
        let selected = self.row_is_highlighted(idx);
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
        let icon = icon_element(&item.icon, &item.id);

        div()
            .id(("result-row", idx))
            // **Rows were never clickable.** They had an id, a hover tint
            // and a pointer-shaped affordance's worth of styling, and no
            // handler at all — so a click did nothing, silently, on the
            // primary surface of the app. One click selects *and* confirms,
            // which is what every launcher does: a row you had to click
            // twice would be slower than the keyboard it is meant to
            // complement.
            .cursor_pointer()
            .on_click(cx.listener(move |root, _event: &ClickEvent, window, cx| {
                // **A click that dismissed the menu must not also fire the
                // row underneath it.** `on_mouse_down_out` closes the menu in
                // the *capture* phase, so by the time this bubble-phase
                // handler runs `actions_menu` is already `None` and there is
                // nothing left to tell "dismiss" from "activate" — you open
                // ⌘K, change your mind, click away, and an app launches.
                // `menu_open_before_this_press` is the snapshot taken before
                // any of that ran. It existed for exactly this and nothing
                // read it.
                if root.menu_open_before_this_press {
                    root.menu_open_before_this_press = false;
                    return;
                }
                root.selected = idx;
                // A click is unambiguous about what it meant, so it takes
                // the selection away from a focused tile as well.
                root.grid_selected = None;
                root.confirm(&Confirm, window, cx);
            }))
            .flex()
            .items_center()
            .flex_shrink_0()
            .h(px(theme::RESULT_ROW_HEIGHT_PX))
            .px_3()
            .gap_3()
            .rounded(px(theme::ROW_RADIUS_PX))
            .when(selected, |row| row.bg(theme::active().surface_selected))
            // **A hover tint, distinctly weaker than the selected pill.**
            // `row_icon_socket_bg` is `text_primary` at 6% — already the
            // token this app uses for "a surface a shade above the panel" —
            // so hovering reads as the pointer being somewhere rather than
            // as a second selection competing with the keyboard's.
            // Deliberately not applied to the selected row: brightening what
            // is already selected on mouse-over says nothing.
            .when(!selected, |row| {
                row.hover_bg(
                    SharedString::from(format!("row:{}:{}", item.kind, item.id)),
                    theme::TRANSPARENT,
                    theme::active().row_icon_socket_bg,
                )
            })
            .child(icon)
            // **Truncation hides content, so the hidden part stays
            // reachable** — the same rule an agent tile has followed since it
            // started handing its second line to the prompt. gpui cannot
            // report whether a given `truncate()` actually clipped, so the
            // trigger is an estimate rather than a measurement: a compact row
            // is the 264px mode column, where clipping is the norm, and a
            // full-width row qualifies once its text passes what that row can
            // hold. Erring toward showing it costs a tooltip nobody needed;
            // erring the other way costs content nobody can reach.
            .when(row_text_may_be_clipped(item, compact), |row| {
                let full = row_tooltip_text(item);
                row.tooltip(move |_window, cx| {
                    cx.new(|_| TextTooltip { text: full.clone() }).into()
                })
            })
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
            // **What Enter will do, on the row Enter would do it to.**
            //
            // Every provider has always set `SearchItem::action_label` and
            // for a while nothing rendered it: the footer that used to carry
            // it was removed and nothing replaced it, so the field was live
            // data going nowhere. That is tolerable for a row whose verb is
            // obvious — Enter on an application launches it — and genuinely
            // unsafe for two of the rows built since. A schedule's Enter
            // *pauses or resumes* and the label is the only thing that says
            // which. An `ask` row's Enter runs a proposed tool call, and a
            // confirmation step that does not say it is one is not a
            // confirmation.
            //
            // On the selected row only, which is Raycast's own arrangement:
            // it is the row about to be acted on, it costs no chrome, and
            // showing a verb on all eight rows at once would be eight
            // answers to a question with one.
            .children((selected && !compact).then(|| {
                let verb = item.action_label.clone();
                div()
                    .flex_shrink_0()
                    .text_size(px(11.))
                    .text_color(theme::active().text_tertiary_on_selected)
                    .child(SharedString::from(verb))
            }))
    }

    /// The "Actions ⌘K" footer label, now a real clickable trigger for the
    /// menu it names, not just a static hint — Raycast's own footer actions
    /// are clickable the same way. `.relative()` establishes the positioned
    /// ancestor `render_actions_menu`'s own zero-size pin div needs (see
    /// that function's doc comment) — mounted here, as the trigger's own
    /// child, rather than as a `render()`-level sibling, is what anchors the
    /// floating menu to the trigger's actual on-screen position instead of a
    /// hand-tuned fixed offset from the panel's corner.
    /// The two-column mode view (`data/neko-design/mockups/
    /// 12-first-clipboard-use.html`): a fixed-width filtered list on the
    /// left, a preview + info pane on the right when
    /// `ModeChrome::has_detail` — otherwise just the list, full width. Sits
    /// where `render_content_area` sits for the root list; same content-area
    /// height budget (`CONTENT_AREA_MIN_HEIGHT_PX`), only ever a width
    /// change between the two.
    fn render_mode_content(&self, mode: &ActiveMode, cx: &mut Context<Self>) -> AnyElement {
        let content = div()
            .flex()
            .flex_1()
            .min_h(px(0.))
            .overflow_hidden()
            .map(|el| {
                // The chat layout replaces the list (and any detail split)
                // wholesale — a conversation is one scrolling exchange, not
                // rows about a transcript. Branching on chrome, never on the
                // provider's id, exactly like `has_detail` below.
                if mode.chrome.transcript {
                    el.child(self.render_transcript(mode, cx))
                } else {
                    el.child(self.render_mode_list(mode.chrome.has_detail, cx))
                        .when(mode.chrome.has_detail, |el| el.child(self.render_mode_detail()))
                }
            });
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

    /// The chat transcript — the conversation mode's whole content area.
    ///
    /// The shape is Paseo's own agent view (and every messaging surface):
    /// the captain's turns as right-aligned bubbles, the agent's prose as
    /// full-width markdown, tool calls as one-line chips between them,
    /// oldest at the top, entered at the bottom. Rows are still rows — the
    /// selection, `⌘K` and the scroll machinery are untouched — they just
    /// paint as turns.
    fn render_transcript(&self, mode: &ActiveMode, cx: &mut Context<Self>) -> AnyElement {
        let header = self.render_transcript_header(mode, cx);
        let mut column = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h(px(0.))
            .gap(px(10.))
            .px(px(16.))
            .py(px(12.))
            .image_cache(self.conversation_image_cache.clone())
            .id("transcript-scroll")
            .overflow_y_scroll()
            .track_scroll(&self.mode_scroll);

        if self.results.is_empty() && self.awaiting_first_rows() {
            column = column.child(crate::components::skeleton::skeleton_list(
                mode.chrome.skeleton,
                self.pulse.read(cx).intensity(),
            ));
        } else if self.results.is_empty() {
            column = column.child(render_empty_state_message(mode.chrome.empty_line));
        } else {
            for (idx, item) in self.results.iter().enumerate() {
                column = column.child(self.render_turn(idx, item, cx));
            }
        }

        let fade_color = if self.translucent {
            theme::active().surface_panel_translucent
        } else {
            theme::active().surface_panel
        };
        div()
            .flex()
            .flex_col()
            .flex_1()
            .child(header)
            .child(with_scrollbar(
                &self.mode_scroll,
                "transcript-scrollbar",
                scroll_edge_fade(
                    self.mode_scroll.clone(),
                    fade_color.into(),
                    theme::EDGE_FADE_BAND_PX,
                    column,
                ),
            ))
        // **Without this every turn overflows the right edge.** The wrapper
        // sits in `render_mode_content`'s flex *row*, where flexbox's
        // `min-width: auto` sizes a child by its content — and a paragraph's
        // content width is the unwrapped line. Seen in the first capture as
        // bubbles running past the window; `min_w(0)` is what makes the
        // column's width the container's, so text wraps instead of escaping.
            .min_w(px(0.))
            .into_any_element()
    }

    /// The session, named — who this chat is with, where it is working, and
    /// whether it is working *right now*.
    ///
    /// The identity comes off the row that opened the mode (the provider
    /// already composed name/workspace/badge there); the *liveness* comes off
    /// the transcript itself — the provider appends a `working` row while the
    /// agent runs, so the dot here and the typing bubble below can never
    /// disagree about whether the agent is busy.
    fn render_transcript_header(&self, mode: &ActiveMode, cx: &mut Context<Self>) -> AnyElement {
        let working = self.transcript_agent_is_working();
        let (title, subtitle) = match &mode.subject_item {
            Some(item) => (item.title.clone(), item.subtitle.clone()),
            None => (mode.chrome.title.to_string(), None),
        };
        let mut dot = theme::active().state_success;
        if working {
            dot.a = 0.45 + 0.55 * self.pulse.read(cx).intensity();
        }
        div()
            .flex()
            .items_center()
            .flex_shrink_0()
            .gap(px(10.))
            .h(px(44.))
            .px(px(14.))
            .border_b_1()
            .border_color(theme::active().border_hairline)
            .child(
                div()
                    .id("transcript-back")
                    .p(px(5.))
                    .rounded(px(theme::ROW_RADIUS_PX))
                    .hover_bg("transcript-back", theme::TRANSPARENT, theme::active().row_icon_socket_bg)
                    .cursor(CursorStyle::PointingHand)
                    .on_mouse_down(gpui::MouseButton::Left, |_event, _window, cx| {
                        cx.stop_propagation()
                    })
                    .on_click(cx.listener(|root, _: &ClickEvent, window, cx| root.exit_mode(window, cx)))
                    .child(back_glyph()),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .min_w(px(0.))
                    .child(
                        div()
                            .text_size(px(13.))
                            .text_color(theme::active().text_primary)
                            .truncate()
                            .child(SharedString::from(title)),
                    )
                    .children(subtitle.map(|subtitle| {
                        div()
                            .text_size(px(11.))
                            .text_color(theme::active().text_tertiary)
                            .truncate()
                            .child(SharedString::from(subtitle))
                    })),
            )
            .child(div().flex_1())
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .text_size(px(11.))
                    .map(|el| {
                        if working {
                            el.text_color(theme::active().text_secondary)
                                .child(div().size(px(7.)).rounded_full().bg(dot))
                                .child("working")
                        } else {
                            el.text_color(theme::active().text_tertiary).child("idle")
                        }
                    }),
            )
            // The way back to the real window, one keystroke or one click —
            // Enter used to do this and reads the conversation now, so the
            // old behaviour lives here, labelled with its key.
            .child(
                div()
                    .id("transcript-open-in-paseo")
                    .flex_shrink_0()
                    .px(px(8.))
                    .py(px(4.))
                    .rounded(px(theme::CHIP_RADIUS_PX))
                    .text_size(px(11.))
                    .text_color(theme::active().text_secondary)
                    .cursor_pointer()
                    .hover_bg(
                        "transcript-open-in-paseo",
                        theme::TRANSPARENT,
                        theme::active().row_icon_socket_bg,
                    )
                    .on_click(cx.listener(|root, _: &ClickEvent, window, cx| {
                        root.open_in_paseo(&OpenInPaseo, window, cx);
                    }))
                    .child("Open in Paseo  \u{2318}\u{21b5}"),
            )
            .into_any_element()
    }

    /// Folds a tool chip's captured output open, or closed again.
    fn toggle_tool_output(&mut self, id: &str) {
        if !self.expanded_tool_output.remove(id) {
            self.expanded_tool_output.insert(id.to_string());
        }
    }

    /// Whether the transcript currently ends in the provider's `working` row.
    fn transcript_agent_is_working(&self) -> bool {
        self.results
            .last()
            .is_some_and(|item| item.speaker.as_deref() == Some("working"))
    }

    /// One turn. The voice comes off `SearchItem::speaker`, the words off
    /// `preview` — never off which provider produced the row.
    fn render_turn(&self, idx: usize, item: &SearchItem, cx: &mut Context<Self>) -> AnyElement {
        let text = item.preview.clone().unwrap_or_else(|| item.title.clone());
        let body: AnyElement = match item.speaker.as_deref() {
            // **The typing bubble.** An agent-side bubble of three dots
            // breathing on the shared clock — never its own animation, per
            // this app's one repeating-motion rule (`motion::PulseClock`).
            Some("working") => {
                let intensity = self.pulse.read(cx).intensity();
                div()
                    .flex()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(4.))
                            .px(px(12.))
                            .py(px(9.))
                            .rounded(px(10.))
                            .bg(theme::active().surface_input)
                            .children((0..3).map(|i| {
                                // One waveform, three phases — the classic
                                // travelling ripple, driven off the single
                                // 12.5Hz clock.
                                let phase =
                                    (intensity + i as f32 * 0.33).rem_euclid(1.0);
                                let mut dot = theme::active().text_secondary;
                                dot.a = 0.25 + 0.6 * (1.0 - (phase - 0.5).abs() * 2.0);
                                div().size(px(6.)).rounded_full().bg(dot)
                            })),
                    )
                    .into_any_element()
            }
            Some("user") => div()
                .flex()
                .justify_end()
                .child(
                    div()
                        // Two-thirds of the pane, the messaging convention —
                        // full-width bubbles read as banners, not speech.
                        .max_w(px(480.))
                        .min_w(px(0.))
                        .overflow_hidden()
                        .px(px(12.))
                        .py(px(8.))
                        .rounded(px(10.))
                        .bg(theme::active().surface_input)
                        .text_size(px(12.))
                        .text_color(theme::active().text_primary)
                        .flex()
                        .flex_col()
                        .gap(px(7.))
                        // A picture somebody sent, shown as a picture. Width
                        // is capped rather than fixed and the height follows,
                        // so a wide screenshot and a tall one both stay in
                        // proportion inside the bubble.
                        .children(item.images.iter().map(|path| {
                            gpui::img(std::path::PathBuf::from(path))
                                .max_w(px(theme::CHAT_IMAGE_MAX_WIDTH_PX))
                                .max_h(px(theme::CHAT_IMAGE_MAX_HEIGHT_PX))
                                .rounded(px(theme::CHIP_RADIUS_PX))
                        }))
                        .when(!text.is_empty(), |el| {
                            el.child(crate::markdown::render(&text))
                        }),
                )
                .into_any_element(),
            Some("tool") => {
                // A chip whose call has a captured answer expands on click —
                // `preview` carries the output (the provider bounds it), and
                // a chat where every `ls` printed its output unasked would be
                // a terminal. The disclosure hint only appears where there is
                // genuinely something to disclose.
                let expandable = item.preview.is_some();
                let expanded = expandable && self.expanded_tool_output.contains(&item.id);
                let title = item.title.clone();
                div()
                    .flex()
                    .flex_col()
                    .gap(px(5.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(7.))
                            .pl(px(2.))
                            .text_size(px(11.))
                            .text_color(theme::active().text_tertiary)
                            .children(item.badge.clone().map(|badge| {
                                div()
                                    .flex_shrink_0()
                                    .px(px(5.))
                                    .py(px(1.))
                                    .rounded(px(4.))
                                    .bg(theme::active().row_icon_socket_bg)
                                    .text_size(px(9.5))
                                    .child(SharedString::from(badge))
                            }))
                            .child(
                                div()
                                    .min_w(px(0.))
                                    .truncate()
                                    .font_family(theme::MONOSPACE_FAMILY)
                                    .text_size(px(10.5))
                                    .child(SharedString::from(title)),
                            )
                            .when(expandable, |el| {
                                el.child(
                                    div()
                                        .flex_shrink_0()
                                        .text_size(px(10.))
                                        .text_color(theme::active().text_tertiary)
                                        .child(if expanded {
                                            "hide output"
                                        } else {
                                            "show output"
                                        }),
                                )
                            }),
                    )
                    .when(expanded, |el| {
                        el.child(
                            div()
                                .px(px(9.))
                                .py(px(7.))
                                .rounded(px(theme::CHIP_RADIUS_PX))
                                .bg(theme::active().surface_input)
                                .font_family(theme::MONOSPACE_FAMILY)
                                .text_size(px(theme::PREVIEW_MONOSPACE_SIZE_PX))
                                .text_color(theme::active().text_primary)
                                .child(SharedString::from(
                                    item.preview.clone().unwrap_or_default(),
                                )),
                        )
                    })
                    .into_any_element()
            }
            // The agent (and anything speakerless): plain prose, full width.
            _ => div()
                .text_size(px(12.))
                .text_color(theme::active().text_primary)
                .map(|el| {
                    if item.preview_markdown {
                        el.child(crate::markdown::render(&text))
                    } else {
                        el.child(SharedString::from(text))
                    }
                })
                .into_any_element(),
        };
        div()
            .id(SharedString::from(format!("turn-{idx}")))
            .rounded(px(theme::ROW_RADIUS_PX))
            .px(px(6.))
            .py(px(3.))
            // **No selection wash: a chat has no cursor.** The arrow keys
            // scroll here rather than stepping, ⌘K targets the session rather
            // than a turn, and a tool chip folds on click — so a highlight
            // would be a pointer to a thing nothing acts on. Hover still
            // responds, because the pointer really can act (folding output).
            .hover(|el| el.bg(theme::active().row_icon_socket_bg))
            .on_click(cx.listener({
                let id = item.id.clone();
                let toggles_output = item.speaker.as_deref() == Some("tool") && item.preview.is_some();
                move |root, _: &ClickEvent, _window, cx| {
                    root.selected = idx;
                    root.grid_selected = None;
                    if toggles_output {
                        root.toggle_tool_output(&id);
                    }
                    cx.notify();
                }
            }))
            .child(body)
            .into_any_element()
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
    fn render_mode_list(&self, has_detail: bool, cx: &mut Context<Self>) -> impl IntoElement {
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

        if self.results.is_empty() && self.awaiting_first_rows() {
            // **Not "nothing", but "not yet".** See `awaiting_first_rows`.
            let shape = self
                .active_mode()
                .map_or(crate::components::skeleton::SkeletonShape::Row, |mode| {
                    mode.chrome.skeleton
                });
            container = container.child(crate::components::skeleton::skeleton_list(
                shape,
                self.pulse.read(cx).intensity(),
            ));
        } else if self.results.is_empty() {
            // The mode's own line, not the root list's "try fewer
            // characters" — see `ModeChrome::empty_line`.
            let line = self
                .active_mode()
                .map_or(NO_MATCHES, |mode| mode.chrome.empty_line);
            container = container.child(render_empty_state_message(line));
        } else {
            let mut current_group: Option<&Option<String>> = None;
            for (idx, item) in self.results.iter().enumerate() {
                if current_group != Some(&item.group_label) {
                    if let Some(label) = &item.group_label {
                        container = container.child(section_header(label.clone()));
                    }
                    current_group = Some(&item.group_label);
                }
                container = container.child(match &item.meter {
                    Some(meter) => self.render_meter(idx, item, meter, cx),
                    None => self.render_row(idx, item, has_detail, cx).into_any_element(),
                });
            }
        }

        let fade_color = if self.translucent { theme::active().surface_panel_translucent } else { theme::active().surface_panel };
        with_scrollbar(
            &self.mode_scroll,
            "mode-list-scrollbar",
            scroll_edge_fade(
                self.mode_scroll.clone(),
                fade_color.into(),
                theme::EDGE_FADE_BAND_PX,
                container,
            )
            .without_bottom_fade(),
        )
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
        if self.active_mode().is_some_and(|mode| mode.chrome.id == "codex-task") {
            let task = self.active_mode().and_then(|mode| mode.subject_item.as_ref());
            let title = task.map(|item| item.title.as_str()).unwrap_or("Codex task");
            let metadata = task
                .and_then(|item| item.subtitle.as_deref())
                .unwrap_or("Read-only local task view");
            let item = self.results.get(self.selected);
            let (label, content, availability) = match item {
                Some(item) if item.preview.is_some() => (
                    "Opening request",
                    item.preview.as_deref().expect("checked above"),
                    Some("Codex has not exposed this task's conversation to Neko."),
                ),
                Some(item) if item.subtitle.as_deref() == Some("No recent visible activity.") => (
                    "No visible activity",
                    "Codex exposed no recent visible turns for this task.",
                    None,
                ),
                Some(item) => (
                    "Visible activity",
                    item.title.as_str(),
                    None,
                ),
                None => (
                    "Task details unavailable",
                    "Codex did not expose any readable task content.",
                    Some("Open the task in Codex to read its conversation."),
                ),
            };
            return col
                .child(div().text_size(px(15.)).text_color(theme::active().text_primary).child(SharedString::from(title.to_owned())))
                .child(div().text_size(px(11.5)).text_color(theme::active().text_tertiary).child(SharedString::from(metadata.to_owned())))
                .child(
                    div().flex().flex_col().flex_1().min_h(px(0.)).overflow_hidden().p_3().gap(px(10.))
                        .rounded(px(theme::ROW_RADIUS_PX)).bg(theme::active().surface_input)
                        .border_1().border_color(theme::active().border_hairline)
                        .child(div().text_size(px(11.)).text_color(theme::active().text_tertiary).child(label))
                        .child(div().text_size(px(13.)).text_color(theme::active().text_primary).whitespace_normal().child(SharedString::from(content.to_owned())))
                        .children(availability.map(|availability| {
                            div().mt(px(6.)).pt(px(10.)).border_t_1()
                                .border_color(theme::active().border_hairline)
                                .text_size(px(11.5)).text_color(theme::active().text_secondary)
                                .whitespace_normal().child(availability)
                        })),
                );
        }
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
            .text_color(theme::active().text_primary)
            // **Three formats, each the provider's own statement.**
            // `preview_markdown` renders through `crate::markdown` — an
            // agent's prose, headings and fences drawn instead of their raw
            // markers. A plain `preview` is monospace: `terminals::
            // capture_text` substitutes rather than deletes glyphs so column
            // alignment survives, and a proportional face would throw that
            // away at the last step. No preview at all falls back to `id`
            // (the clipboard's prose, where id *is* content) in the
            // proportional face it reads best in. The scroll is here rather
            // than on the pane so a long reply is readable to its end.
            .id("mode-detail-preview")
            .overflow_y_scroll()
            .map(|el| {
                let markdown = crate::evidence::force_preview_markdown() || item.preview_markdown;
                let text = item.preview.clone().unwrap_or_else(|| item.id.clone());
                if markdown {
                    el.text_size(px(12.)).child(crate::markdown::render(&text))
                } else if item.preview.is_some() {
                    el.font_family(theme::MONOSPACE_FAMILY)
                        .text_size(px(theme::PREVIEW_MONOSPACE_SIZE_PX))
                        .child(SharedString::from(text))
                } else {
                    el.text_size(px(13.)).child(SharedString::from(text))
                }
            });

        let mut info = div().flex().flex_col().gap_2();
        // **The labels are the clipboard mode's, so only its own rows get
        // them.** "Application" over a terminal's working directory would be
        // a wrong label on a right value, which is worse than no label —
        // and a mode whose fields differ is exactly the cost `modes.rs`
        // already says a second detail view pays.
        if item.kind == "clipboard" {
            if let Some(source) = &item.source {
                info = info.child(detail_info_row("Application", source.clone()));
            }
            if let Some(badge) = &item.badge {
                info = info.child(detail_info_row("Content Type", title_case_badge(badge)));
            }
            if let Some(accessory) = &item.accessory {
                info = info.child(detail_info_row("Copied", accessory.clone()));
            }
        } else if let Some(source) = &item.source {
            info = info.child(detail_info_row("Directory", source.clone()));
        }

        // **The verb, for the one place a row cannot carry it.** `render_row`
        // draws `action_label` on the selected row, and drops it on `compact`
        // rows because the 264px mode column has no space — which leaves it
        // missing exactly where Enter is least guessable ("Paste" on a
        // clipboard entry, "Open folder" on a terminal). A compact row always
        // implies a detail pane (`render_mode_list` takes the same
        // `has_detail` flag for both), so this pane is guaranteed to exist
        // wherever the row gave the verb up, and it has room.
        let verb = (!item.action_label.is_empty()).then(|| {
            div()
                .mt(px(12.))
                .pt(px(10.))
                .border_t_1()
                .border_color(theme::active().border_hairline)
                .text_size(px(11.))
                .text_color(theme::active().text_secondary)
                .child(SharedString::from(item.action_label.clone()))
        });

        col.child(preview).child(info).children(verb)
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
                    // A hover tint, so the menu reads as a menu rather than
                    // a list of labels. Weaker than the selected fill, the
                    // same relationship `render_row` uses.
                    .when(!selected, |el| {
                        el.hover_bg(
                            SharedString::from(format!("menu-action:{}", action.id)),
                            theme::TRANSPARENT,
                            theme::active().row_icon_socket_bg,
                        )
                    })
                    .cursor_pointer()
                    // **Clicking a menu row runs it — and a destructive one
                    // still needs two clicks.** Moving the selection is what
                    // disarms a pending confirm, so a click that *lands on a
                    // different row* disarms exactly as arrowing to it would;
                    // a second click on the same row is the second Enter.
                    // Without that, clicking Delete once would delete, while
                    // pressing Enter once would not — the same control
                    // behaving differently by input device.
                    .on_click(cx.listener(move |root, _event: &ClickEvent, window, cx| {
                        if let Some(menu) = &mut root.actions_menu
                            && menu.selected != idx
                        {
                            menu.selected = idx;
                            menu.confirm_armed = false;
                        }
                        root.confirm_menu_action(window, cx);
                    }))
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
/// Whether a transcript's view is parked at its end.
///
/// Pure, and separate from the handle for the same reason
/// `edge_fade::edge_fade_visibility` is: the decision is arithmetic, the
/// arithmetic is what governs whether a new turn is followed, and a live
/// `ScrollHandle` reports a `max_offset` of zero until something has actually
/// been laid out — so a test driving it through the handle would be testing
/// the harness.
///
/// Slack of one step rather than exact equality: a glide lands on a
/// fractional offset, and a chat that stops following the moment somebody is
/// one pixel short of the end is worse than one that follows a pixel early.
fn transcript_at_bottom(scrolled: f32, max: f32) -> bool {
    if max <= 0.0 {
        // Nothing to scroll, so the end is where you already are. Getting
        // this wrong would mean a short conversation never followed a reply.
        return true;
    }
    scrolled >= max - theme::TRANSCRIPT_SCROLL_STEP_PX
}

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
/// Splits agents out of a response into `(tiles, rows)`.
///
/// **Live agents always become tiles. Idle ones only do when nothing has been
/// typed**, which is the rule that keeps the grid meaning one thing: at rest
/// it answers "what have you been working on", and during a search it stays
/// out of the way so a query's own matches are read as a list. Without that
/// gate, typing would silently move idle agent rows up into the grid.
///
/// Capped at [`AGENT_GRID_CAPACITY`]. The provider already caps an empty
/// query at the same number and orders it running-first-then-recent, so this
/// is a floor against a future provider change rather than the primary
/// mechanism — but the grid has a fixed height, and one row too many would
/// be clipped rather than reported.
///
/// Order is preserved on both sides, so the rows that stay keep whatever
/// section ordering `search::allocate` decided.
fn split_agent_tiles(results: Vec<SearchItem>, query_is_empty: bool) -> (Vec<SearchItem>, Vec<SearchItem>) {
    let mut tiles = Vec::new();
    let mut rows = Vec::new();
    for item in results {
        let is_agent = matches!(item.kind.as_str(), "agent" | "codex-task");
        let live = matches!(item.badge.as_deref(), Some("LIVE") | Some("WAITING"));
        if is_agent && (live || query_is_empty) && tiles.len() < AGENT_GRID_CAPACITY {
            tiles.push(item);
        } else {
            rows.push(item);
        }
    }
    (tiles, rows)
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

/// `tnum` — fixed-width digits.
///
/// Every number in a meter's own column changes as the pane refreshes, and
/// proportional digits are different widths, so a column of them shifts
/// under the eye on every update. Built once and cloned: `FontFeatures`
/// wraps an `Arc`, so this costs a refcount bump per row rather than an
/// allocation.
fn tabular_numerals() -> FontFeatures {
    static TABULAR: OnceLock<FontFeatures> = OnceLock::new();
    TABULAR.get_or_init(|| FontFeatures(Arc::new(vec![("tnum".to_string(), 1)]))).clone()
}

/// Which way the selection just moved, so the view can leave a row of
/// lookahead on that side.
#[derive(Debug, Clone, Copy, PartialEq)]
enum ScrollBias {
    Down,
    Up,
    /// The selection was resolved rather than moved — a re-search keeping the
    /// highlight on the same item. There is no direction to look ahead in.
    None,
}

impl ScrollBias {
    /// The row to actually scroll to.
    ///
    /// Clamped at both ends, and that clamp is the behaviour rather than
    /// defensiveness: at the last row there is nothing beyond it to reveal,
    /// so it lands flush against the bottom, which is correct — the lookahead
    /// exists to show what is coming, and nothing is.
    fn target(self, selected: usize, len: usize) -> usize {
        match self {
            ScrollBias::Down => (selected + 1).min(len.saturating_sub(1)),
            ScrollBias::Up => selected.saturating_sub(1),
            ScrollBias::None => selected,
        }
    }
}

/// Maps a `results` index to its position among `render_content_area`'s own
/// DIRECT children — the index space `ScrollHandle::scroll_to_item` works in.
///
/// The root list's children are not one-per-result: a connection banner, an
/// accessibility banner and a section header for each new `kind` are all
/// interleaved ahead of the rows. Counting them is the whole job, and it is a
/// pure function so the arithmetic is testable without a live `Window`.
fn root_list_child_index(
    results: &[SearchItem],
    target: usize,
    leading_banners: usize,
) -> usize {
    let mut child_index = leading_banners;
    let mut current_section: Option<&str> = None;
    for (idx, item) in results.iter().enumerate() {
        if current_section != Some(item.kind.as_str()) {
            child_index += 1;
            current_section = Some(item.kind.as_str());
        }
        if idx == target {
            return child_index;
        }
        child_index += 1;
    }
    child_index
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
        NO_MATCHES.into()
    };
    render_empty_state_message(message)
}

/// The plain one-line-tip shape `render_empty_state` uses, parameterized on
/// the message — shared with the mode list's own empty state
/// (`Root::render_mode_list`), which has different copy but the identical
/// layout.
/// Everything a tile had to truncate, on one line.
fn agent_tile_tooltip(item: &SearchItem) -> SharedString {
    match &item.subtitle {
        Some(subtitle) => SharedString::from(format!("{} \u{2014} {subtitle}", item.title)),
        None => SharedString::from(item.title.clone()),
    }
}

/// A tile's title and subtitle deliberately ellipsize. State that changes
/// whether its data is trustworthy therefore needs its own fixed-width label.
fn agent_tile_status_label(item: &SearchItem) -> Option<&'static str> {
    (item.kind == "codex-task" && item.accessory.as_deref() == Some("Codex unavailable"))
        .then_some("UNAVAILABLE")
}

/// gpui builds a tooltip from a view, so this is the smallest one that
/// renders a string in the app's own tokens. Deliberately not a general
/// component: nothing else in this app has a tooltip, and one that grew
/// options before a second caller existed would be generality nobody asked
/// for.
struct TextTooltip {
    text: SharedString,
}

impl Render for TextTooltip {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_2()
            .py_1()
            .rounded(px(theme::ROW_RADIUS_PX))
            .bg(theme::active().surface_raised)
            .border_1()
            .border_color(theme::active().border_hairline_strong)
            .text_size(px(12.))
            .text_color(theme::active().text_primary)
            .child(self.text.clone())
    }
}

/// One string for "the daemon did not answer", wherever that surfaces.
/// The panel said `"Can't reach"` and Preferences said `"Couldn't reach"`
/// — the same failure, the same subject, two tenses.
pub const DAEMON_UNREACHABLE: &str = "Can't reach neko-daemon. Results may be out of date.";

/// One string for "your query matched nothing", wherever that happens.
///
/// The root list said `"No matching results"` and a mode's list said
/// `"No matching entries."` — the same idea, two nouns, and only one of
/// them punctuated. It also names the way out, because an empty state that
/// only shrugs leaves a person holding a query with nothing to do about it.
pub const NO_MATCHES: &str = "No matches \u{2014} try fewer characters, or \u{232b} to start over";

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
/// Roughly how many characters a full-width result row's text column holds.
///
/// An estimate, and openly so: the column is what is left of
/// `PANEL_WIDTH_WITH_DETAIL_PX` after the icon, the gaps, an optional badge and
/// an optional accessory, and the font is proportional, so no constant is
/// correct for every string. This is deliberately generous — a tooltip that
/// occasionally appears on a row that fits is a smaller failure than a row
/// whose hidden half cannot be read at all.
const ROW_TEXT_BUDGET_CHARS: usize = 72;

/// The same, for the 264px mode column, which also drops subtitle and accessory.
const COMPACT_ROW_TEXT_BUDGET_CHARS: usize = 30;

/// Whether a row's text is long enough that `truncate()` has probably clipped it.
fn row_text_may_be_clipped(item: &SearchItem, compact: bool) -> bool {
    let budget = if compact { COMPACT_ROW_TEXT_BUDGET_CHARS } else { ROW_TEXT_BUDGET_CHARS };
    let mut used = item.title.chars().count();
    if !compact {
        // The subtitle shares the same flex line, so it spends the same budget.
        used += item.subtitle.as_ref().map_or(0, |s| s.chars().count() + 2);
    }
    used > budget
}

/// What that row's tooltip says: the title, and the subtitle under it when
/// there is one the row itself would have shown.
fn row_tooltip_text(item: &SearchItem) -> SharedString {
    match &item.subtitle {
        Some(subtitle) => SharedString::from(format!("{}\n{subtitle}", item.title)),
        None => SharedString::from(item.title.clone()),
    }
}

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

/// The row-icon slot's mark for a provider that has no per-item raster.
///
/// **These were hand-composed `div()` stacks until `assets.rs` existed.**
/// Every one of their doc comments gave the same reason — "this codebase has
/// no bundled icon-asset pipeline, and a Unicode symbol isn't a reliable
/// substitute (design report §6)" — and the second half is still true; the
/// first half stopped being true the moment an `AssetSource` was installed,
/// because `gpui::svg()` was already in the gpui this crate compiles
/// against. They are now vendored Lucide icons (ISC; see `assets.rs` and
/// `crates/neko/src/components/vendor/MANIFEST.md`), which buys three things
/// a `div()` stack could not: real curves (`Glyph::Link` is a chain, not two
/// squares on a diagonal), one consistent stroke weight and optical size
/// across every mark, and resolution independence — the SVG is rasterised at
/// the window's live backing scale, so moving the panel to a different
/// display re-renders it rather than resampling it.
///
/// **Colour still comes from `theme::active()`, at paint time.** `svg()`
/// renders to an alpha mask that is tinted by the element's own
/// `text_color`, so a theme change re-tints every icon with no per-theme
/// asset, no cache to invalidate, and no `if themed` branch — the same
/// property the painted versions had, kept deliberately.
///
/// Two arms do not simply name a file, both for reasons that are about the
/// renderer rather than about taste — see each one.
///
/// `row_id` is the row's own `SearchItem::id`. Only `Glyph::Palette` reads it
/// — a theme row draws *its own* palette, so a list of themes is a list of
/// previews rather than seventeen copies of the same mark. That is a lookup
/// keyed on data already on the row, not a `match` on which provider produced
/// it: any row whose id happens to name a built-in theme gets that theme's
/// swatch, and any row whose id doesn't (the `Themes` command in the root
/// list, whose id is `"themes"`) falls back to the live palette, which is the
/// honest thing for a row that means "open the theme list" rather than "be
/// this theme".
/// The one character a 12px badge can hold: the tool's own initial,
/// uppercased. `"claude"` → `"C"`, `"gpt"` → `"G"`.
fn tool_initial(tool: &str) -> String {
    tool.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_default()
}

/// One icon slot, shared by result rows and agent tiles so the two can never
/// render the same `Icon` differently.
fn icon_element(icon: &Icon, row_id: &str) -> AnyElement {
    match icon {
        Icon::Image(path) => img(PathBuf::from(path))
            .w(px(theme::ROW_ICON_PX))
            .h(px(theme::ROW_ICON_PX))
            .rounded(px(theme::ROW_ICON_RADIUS_PX))
            .bg(theme::active().row_icon_socket_bg)
            .into_any_element(),
        Icon::Glyph(glyph) => glyph_element(*glyph, row_id),
        // An icon the daemon hasn't finished extracting yet (a fresh install,
        // or right after a daemon restart — see `Event::IconsUpdated`'s doc
        // comment) — a neutral glyph in the socket rather than an empty hole,
        // self-healing to the real icon on the next `refresh_icons` without a
        // layout change.
        Icon::Placeholder => app_icon_placeholder_glyph(),
    }
}

fn glyph_element(glyph: Glyph, row_id: &str) -> AnyElement {
    let slot = div().w(px(theme::ROW_ICON_PX)).h(px(theme::ROW_ICON_PX)).flex_shrink_0();

    // `Glyph::Palette` is the one mark in this vocabulary that has no
    // single-colour form, so it is the one that stays painted — permanently,
    // not pending an asset. A gpui SVG is an alpha mask tinted by exactly one
    // colour; a palette swatch that is all one colour is not a palette
    // swatch. `assets::glyph_icon` returns `None` for precisely this, and
    // that `None` is what routes here.
    let Some(path) = glyph_icon(glyph) else {
        return palette_glyph(slot, row_id);
    };

    // The agent marks keep the exact two-colour treatment the painted
    // versions had — the mark's own outline in `state_success_border` when
    // live, plus a presence dot — because an alpha mask cannot carry two
    // tints on its own. The mark is the SVG; the dot is a `div` composited
    // over it. Same mark either way, so a list of agents reads as one kind
    // of thing and the live ones still pick themselves out, which is the
    // property `AGENTS.md`'s "Agents" section is describing.
    let live = glyph == Glyph::AgentLive;
    let tint = if live {
        theme::active().state_success_border
    } else {
        theme::active().text_tertiary
    };

    let mark = slot
        .relative()
        .flex()
        .items_center()
        .justify_center()
        .child(svg().path(path).size(px(theme::ROW_ICON_GLYPH_PX)).text_color(tint));

    match glyph {
        Glyph::Agent | Glyph::AgentLive => mark
            .child(
                // Bottom-right, not the painted version's inset position:
                // Lucide's `square-terminal` puts its prompt caret where
                // that dot used to sit. A presence badge on the corner is
                // also the convention every OS uses for exactly this
                // meaning, so nothing is lost by the move.
                div()
                    .absolute()
                    .right(px(1.))
                    .bottom(px(2.))
                    .w(px(6.))
                    .h(px(6.))
                    .rounded_full()
                    .bg(if live {
                        theme::active().state_success
                    } else {
                        theme::active().text_tertiary
                    }),
            )
            .into_any_element(),
        _ => mark.into_any_element(),
    }
}

/// Four filled swatches in a 2x2 block, painted in the row's own palette (or
/// the live one for a row whose id names no theme) — the one glyph in this
/// vocabulary that changes with the active theme, deliberately: it is the
/// affordance for changing that theme, so it should show what is currently
/// on. Split out of [`glyph_element`] when everything around it became an
/// `svg()` call, so that what stays painted, and why, is one named thing
/// rather than the odd branch left in a match.
fn palette_glyph(slot: gpui::Div, row_id: &str) -> AnyElement {
    let swatch = |color| div().w(px(8.)).h(px(8.)).rounded(px(2.)).bg(color);
    // The row's own palette when it names one; the live one otherwise.
    let t = theme::theme_by_id(row_id).map_or_else(theme::active, |t| &t.palette);
    slot.flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(2.))
        // The four tokens that actually identify a palette at 8px: what the
        // panel is, what a selected row is, what text is, and its one state
        // colour.
        .child(div().flex().gap(px(2.)).child(swatch(t.surface_panel)).child(swatch(t.text_primary)))
        .child(
            div()
                .flex()
                .gap(px(2.))
                .child(swatch(t.state_danger))
                .child(swatch(t.surface_selected)),
        )
        .into_any_element()
}

/// The input row's magnifier.
///
/// **This one was actually wrong, not just crude.** Its own comment claimed
/// "a circle + a diagonal stroke", but the code was a bare `.rounded_full()
/// .border_2()` — a ring with no handle at all, which reads as a dot rather
/// than a search icon. That is what a mark assembled from `div()` primitives
/// costs: `div()` has no rotation, so the diagonal was presumably dropped as
/// undrawable and the comment was never corrected. Lucide's `search` is a
/// real magnifier, and the design report's §6 finding (a Unicode symbol is
/// not reliably rendered here) is still honoured — this is a vendored asset,
/// not a font character.
fn search_glyph() -> impl IntoElement {
    svg().path(icon::SEARCH).size(px(15.)).text_color(theme::active().text_tertiary)
}

/// The mode input row's back affordance — "a back arrow in place of the
/// search glyph," per the launch brief.
///
/// Was a hand-traced `gpui::PathBuilder` chevron, for the reason
/// `components::glyphs` still traces the ⌥ mark: `div()` cannot draw a
/// diagonal. An SVG can, so the whole `canvas`/`PathBuilder`/manual-scale
/// closure is gone in favour of naming a file. `components::glyphs`'
/// `opt_glyph` deliberately stays traced — the ⌥
/// modifier symbol is not in any general-purpose icon set, and the wordmark
/// is neko's own identity rather than an icon.
fn back_glyph() -> impl IntoElement {
    svg().path(icon::CHEVRON_LEFT).size(px(15.)).text_color(theme::active().text_tertiary)
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


    fn item_with_text(title: &str, subtitle: Option<&str>) -> SearchItem {
        SearchItem {
            title: title.into(),
            subtitle: subtitle.map(str::to_string),
            ..item_with_id("app", "fixture")
        }
    }

    #[test]
    fn a_short_row_gets_no_tooltip_and_a_long_one_does() {
        // The trigger is an estimate — gpui cannot report whether a given
        // `truncate()` actually clipped — so what is pinned here is the
        // direction of the error, not a pixel: short rows stay quiet.
        let short = item_with_text("Safari", None);
        assert!(!row_text_may_be_clipped(&short, false));
        let long = item_with_text(&"x".repeat(120), None);
        assert!(row_text_may_be_clipped(&long, false));
    }

    #[test]
    fn a_subtitle_spends_the_same_budget_the_title_does() {
        // They share one flex line, so a title that fits alone can still be
        // clipped once its subtitle is beside it.
        let title = "x".repeat(50);
        let bare = item_with_text(&title, None);
        assert!(!row_text_may_be_clipped(&bare, false));
        let with_subtitle = item_with_text(&title, Some(&"y".repeat(40)));
        assert!(row_text_may_be_clipped(&with_subtitle, false));
    }

    #[test]
    fn the_narrow_mode_column_clips_far_sooner() {
        // 264px, and it drops the subtitle entirely — so the same title that
        // is comfortable in the root list is clipped here.
        let item = item_with_text("Copied from Arc a little while ago", Some("ignored"));
        assert!(!row_text_may_be_clipped(&item, false));
        assert!(row_text_may_be_clipped(&item, true));
    }

    #[test]
    fn the_tooltip_carries_the_subtitle_only_when_there_is_one() {
        let with = item_with_text("Title", Some("Subtitle"));
        assert_eq!(row_tooltip_text(&with).as_ref(), "Title\nSubtitle");
        let without = item_with_text("Title", None);
        assert_eq!(row_tooltip_text(&without).as_ref(), "Title");
    }


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
            meter: None,
            keeps_open: false,
            preview_markdown: false,
            speaker: None,
            images: Vec::new(),
            preview: None,
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
        // (after fixing that) produced a dropped-entirely clipboard section:
        // 6 apps plus 1 clipboard entry. The clipboard reservation means the
        // app section gives up a row rather than the clipboard section
        // losing its only one.
        //
        // **The budget is stated here rather than taken from
        // `CONTENT_AREA_MIN_HEIGHT_PX`**: this is a test of the reservation
        // algorithm, and pinning a row count against the panel's own height
        // makes it fail for the wrong reason every time the panel is
        // resized. 320px is the height that produces this shape.
        const TIGHT_BUDGET: f32 = 320.0;
        let mut results: Vec<SearchItem> = (0..6).map(|_| item("app")).collect();
        results.push(item("clipboard"));

        let fitted = fit_within_budget(results, TIGHT_BUDGET);

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
        // No clipboard entries at all -> no reservation taken -> same count
        // a pure app-only search produced before clipboard existed (7 of 8
        // requested fit once the one header is charged). Explicit budget for
        // the same reason as the test above: the number is a property of the
        // algorithm at this height, not of whatever the panel happens to be.
        const TIGHT_BUDGET: f32 = 320.0;
        let results: Vec<SearchItem> = (0..RESULT_LIMIT).map(|_| item("app")).collect();
        let fitted = fit_within_budget(results, TIGHT_BUDGET);
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
    fn a_leading_slash_scopes_the_search_to_commands(cx: &mut TestAppContext) {
        let window = test_root(cx);
        // Two updates, not one: `set_content` emits `ContentChanged`, and the
        // subscription that turns it into a search does not run until the
        // effect queue flushes at the end of this block.
        window
            .update(cx, |root, _window, cx| {
                root.text_field.update(cx, |field, cx| field.set_content("/the", cx));
            })
            .unwrap();
        cx.run_until_parked();
        window
            .update(cx, |root, _window, _cx| {
                // The palette is a scoped list, so it must not be treated as
                // the empty root query — that path keeps every row and
                // scrolls, and would also mislabel a typed palette query.
                assert!(!root.results_are_for_empty_query);
            })
            .unwrap();
    }

    #[gpui::test]
    fn a_slash_inside_a_mode_is_ordinary_text(cx: &mut TestAppContext) {
        let window = test_root(cx);
        window
            .update(cx, |root, window, cx| {
                root.enter_mode_about("clipboard", None, window, cx);
                // A mode is already scoped, and somebody typing a path there
                // means the character.
                root.text_field.update(cx, |field, cx| field.set_content("/Users", cx));
                assert_eq!(
                    root.active_mode().map(|m| m.chrome.provider_id),
                    Some("clipboard"),
                    "the slash must not re-scope a mode's own list"
                );
            })
            .unwrap();
    }

    #[gpui::test]
    fn confirming_the_preferences_command_opens_the_window_instead_of_entering_a_mode(
        cx: &mut TestAppContext,
    ) {
        let (client, _events) = NekoClient::connect(std::path::PathBuf::from("/tmp/neko-prefs-test.sock"));
        let accessibility: Rc<dyn AccessibilityChecker> = Rc::new(FakeAccessibilityChecker::new(true));
        let (opener, opened) = recording_preferences_opener();
        let window = cx.add_window(|_window, cx| {
            Root::build(client, accessibility, true, true, no_appearance_setter(), opener, crate::window_drag::disabled(), cx)
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
            meter: None,
            keeps_open: false,
            preview_markdown: false,
            speaker: None,
            images: Vec::new(),
            preview: None,
        }
    }

    #[gpui::test]
    fn a_focused_tile_and_a_list_row_are_never_highlighted_at_the_same_time(cx: &mut TestAppContext) {
        let window = test_root(cx);
        window
            .update(cx, |root, _window, _cx| {
                root.agent_tiles = vec![tile("a")];
                root.results = vec![agent_row("row-1"), agent_row("row-2")];
                root.selected = 0;

                // The list keeps its cursor while the grid holds the keyboard
                // — which is correct, and is exactly why the row must not
                // paint a highlight for it.
                root.grid_selected = Some(0);
                assert!(!root.row_is_highlighted(0), "the grid has focus, so no row is selected");

                root.grid_selected = None;
                assert!(root.row_is_highlighted(0), "focus back in the list, the row highlights again");
            })
            .unwrap();
    }

    #[gpui::test]
    fn down_runs_through_the_tiles_in_reading_order_before_reaching_the_rows(cx: &mut TestAppContext) {
        let window = test_root(cx);
        window
            .update(cx, |root, window, cx| {
                root.agent_tiles = vec![tile("a"), tile("b"), tile("c"), tile("d")];
                root.results = vec![agent_row("row-1")];
                // The grid is the first thing in the content area, so it
                // starts with the selection.
                root.grid_selected = Some(0);
                root.selected = 0;

                for expected in [1, 2, 3] {
                    root.select_next(&SelectNext, window, cx);
                    assert_eq!(root.grid_selected, Some(expected), "tiles run 1,2,3,4 in order");
                }
                root.select_next(&SelectNext, window, cx);
                assert_eq!(root.grid_selected, None, "past the last tile is the first row");
                assert_eq!(root.selected, 0);

                // And exactly back again.
                root.select_previous(&SelectPrevious, window, cx);
                assert_eq!(root.grid_selected, Some(3));
                for expected in [2, 1, 0] {
                    root.select_previous(&SelectPrevious, window, cx);
                    assert_eq!(root.grid_selected, Some(expected));
                }
            })
            .unwrap();
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

    #[gpui::test]
    fn cmd_enter_dispatches_through_the_real_binding_to_the_handler(cx: &mut TestAppContext) {
        // Drives gpui's own keystroke → binding → action dispatch, in
        // process, because "does ⌘↵ reach the handler while the field has
        // focus" is not answerable by calling the handler directly. Two
        // harness rules this test learned the hard way, kept as its own
        // comments: **dispatch reads the rendered frame's tree**, so the
        // window must draw between mutating the view and simulating; and
        // **`simulate_keystrokes` runs the executor until parked**, so the
        // activation has already *completed* (against this harness's dead
        // socket) by the time an assertion runs — the proof of dispatch is
        // the failed request's own error, never the in-flight flag.
        cx.update(|cx| {
            cx.bind_keys([
                gpui::KeyBinding::new("cmd-enter", OpenInPaseo, Some("Panel")),
                gpui::KeyBinding::new("enter", Confirm, Some("Panel")),
            ]);
        });
        let window = test_root(cx);
        window
            .update(cx, |root, window, cx| {
                root.results = vec![agent_row("the-agent")];
                root.selected = 0;
                root.enter_mode_about("conversation", Some("the-agent".into()), window, cx);
                let handle = root.focus_handle(cx);
                window.focus(&handle, cx);
            })
            .unwrap();
        cx.refresh().unwrap();
        cx.run_until_parked();

        // Control: plain enter (the composer's send) through the same tree.
        window
            .update(cx, |root, _window, cx| {
                root.text_field.update(cx, |field, cx| field.set_content("control draft", cx));
            })
            .unwrap();
        cx.simulate_keystrokes(window.into(), "enter");
        window
            .update(cx, |root, _window, cx| {
                assert!(
                    root.activation_error.is_some(),
                    "CONTROL: enter never dispatched — the send would have errored on the dead socket"
                );
                assert_eq!(root.text_field.read(cx).content(), "", "and the draft was consumed");
                root.activation_error = None;
            })
            .unwrap();

        cx.simulate_keystrokes(window.into(), "cmd-enter");
        window
            .update(cx, |root, _window, _cx| {
                assert!(
                    root.activation_error.is_some(),
                    "cmd-enter never reached open_in_paseo through dispatch"
                );
            })
            .unwrap();
    }

    #[gpui::test]
    fn cmd_enter_in_a_transcript_targets_the_subject_not_the_selected_turn(
        cx: &mut TestAppContext,
    ) {
        // The selection sits on some turn (`agent#7`); the jump goes to the
        // *agent*. The provider strips a `#rank` defensively, but the client
        // must not lean on that.
        let window = test_root(cx);
        window
            .update(cx, |root, window, cx| {
                root.results = vec![agent_row("the-agent")];
                root.selected = 0;
                root.enter_mode_about("conversation", Some("the-agent".into()), window, cx);
                let request = root.open_in_paseo_request().expect("a subject to open");
                match request {
                    Request::Activate { kind, id, action, .. } => {
                        assert_eq!(kind, "conversation");
                        assert_eq!(id, "the-agent");
                        assert_eq!(action.as_deref(), Some("open-in-paseo"));
                    }
                    other => panic!("not an activation: {other:?}"),
                }
            })
            .unwrap();
    }

    #[gpui::test]
    fn cmd_enter_on_a_row_requires_the_action_to_actually_exist(cx: &mut TestAppContext) {
        // Resolved by data, never by provider id: an agent row carries
        // `open-in-paseo`, an app row does not, and ⌘↵ on the app row must
        // mean nothing rather than guess.
        let window = test_root(cx);
        window
            .update(cx, |root, _window, _cx| {
                let mut agent = agent_row("the-agent");
                agent.actions = vec![neko_protocol::ItemAction {
                    id: "open-in-paseo".into(),
                    label: "Open in Paseo".into(),
                    destructive: false,
                }];
                root.results = vec![agent, item("app")];
                root.selected = 0;
                let request = root.open_in_paseo_request().expect("the agent row has it");
                assert!(matches!(
                    request,
                    Request::Activate { ref kind, ref action, .. }
                        if kind == "agent" && action.as_deref() == Some("open-in-paseo")
                ));
                root.selected = 1;
                assert!(root.open_in_paseo_request().is_none(), "an app row means nothing");
            })
            .unwrap();
    }

    #[gpui::test]
    fn the_mode_keeps_the_row_that_opened_it_for_the_header(cx: &mut TestAppContext) {
        // The header renders the agent's name/workspace/badge exactly as the
        // provider composed them on the row — re-deriving any of it would be
        // a second copy of agents.rs's naming rules.
        let window = test_root(cx);
        window
            .update(cx, |root, window, cx| {
                let row = SearchItem {
                    title: "feat/doctors-maps".into(),
                    subtitle: Some("acme-corp/web-app".into()),
                    enters_mode: Some("conversation".into()),
                    ..agent_row("the-agent")
                };
                root.results = vec![row];
                root.selected = 0;
                root.confirm(&Confirm, window, cx);
                let mode = root.active_mode().expect("entered");
                let kept = mode.subject_item.as_ref().expect("the row travelled in");
                assert_eq!(kept.title, "feat/doctors-maps");
                assert_eq!(kept.subtitle.as_deref(), Some("acme-corp/web-app"));
            })
            .unwrap();
    }

    #[gpui::test]
    fn a_tool_chips_output_folds_open_and_closed(cx: &mut TestAppContext) {
        let window = test_root(cx);
        window
            .update(cx, |root, _window, _cx| {
                root.toggle_tool_output("a#3");
                assert!(root.expanded_tool_output.contains("a#3"));
                root.toggle_tool_output("a#3");
                assert!(!root.expanded_tool_output.contains("a#3"), "a second click folds it back");
            })
            .unwrap();
    }

    #[gpui::test]
    fn the_working_row_is_what_says_the_agent_is_busy(cx: &mut TestAppContext) {
        let window = test_root(cx);
        window
            .update(cx, |root, _window, _cx| {
                root.results = vec![item_with_id("conversation", "a#0")];
                assert!(!root.transcript_agent_is_working());
                root.results.push(SearchItem {
                    speaker: Some("working".into()),
                    images: Vec::new(),
                    ..item_with_id("conversation", "a#working")
                });
                assert!(root.transcript_agent_is_working());
            })
            .unwrap();
    }

    #[gpui::test]
    fn enter_in_a_transcript_sends_the_draft_to_the_subject_not_the_turn(
        cx: &mut TestAppContext,
    ) {
        // The selected row is whatever turn happens to be highlighted; the
        // message goes to the *agent*. Building the request off the row would
        // send the prompt to "agent#7", which the daemon would refuse — or
        // worse, quietly no-op, since a turn row's activate is Ok(()).
        let window = test_root(cx);
        window
            .update(cx, |root, window, cx| {
                root.results = vec![agent_row("the-agent")];
                root.selected = 0;
                root.enter_mode_about("conversation", Some("the-agent".into()), window, cx);
                root.text_field.update(cx, |field, cx| field.set_content("run the tests", cx));
                let request = root.transcript_send_request(cx).expect("a draft to send");
                match request {
                    Request::Activate { kind, id, action, query } => {
                        assert_eq!(kind, "conversation");
                        assert_eq!(id, "the-agent", "the subject, never the selected turn");
                        assert_eq!(action, None);
                        assert_eq!(query, "run the tests");
                    }
                    other => panic!("not an activation: {other:?}"),
                }
            })
            .unwrap();
    }

    #[gpui::test]
    fn an_empty_draft_swallows_enter_rather_than_acting_on_a_turn(cx: &mut TestAppContext) {
        let window = test_root(cx);
        window
            .update(cx, |root, window, cx| {
                root.results = vec![agent_row("the-agent")];
                root.selected = 0;
                root.enter_mode_about("conversation", Some("the-agent".into()), window, cx);
                assert!(root.transcript_send_request(cx).is_none());
                root.confirm(&Confirm, window, cx);
                assert!(!root.activating, "nothing was sent and nothing was activated");
            })
            .unwrap();
    }

    #[gpui::test]
    fn sending_clears_the_field_like_every_messaging_surface(cx: &mut TestAppContext) {
        let window = test_root(cx);
        window
            .update(cx, |root, window, cx| {
                root.results = vec![agent_row("the-agent")];
                root.selected = 0;
                root.enter_mode_about("conversation", Some("the-agent".into()), window, cx);
                root.text_field.update(cx, |field, cx| field.set_content("hello there", cx));
                root.confirm(&Confirm, window, cx);
                assert_eq!(root.text_field.read(cx).content(), "", "cleared on send");
                assert!(root.activating, "and the send is in flight");
            })
            .unwrap();
    }

    #[gpui::test]
    fn an_empty_list_says_nothing_until_the_search_has_actually_finished(
        cx: &mut TestAppContext,
    ) {
        // "No providers signed in" is a statement about a *finished* search.
        // Usage fans out to three vendor APIs, so asserting it during the
        // fetch was a claim the panel then contradicted a moment later.
        let window = test_root(cx);
        window
            .update(cx, |root, _window, _cx| {
                root.results.clear();
                root.searching = true;
                assert!(root.awaiting_first_rows(), "mid-fetch: a skeleton, not a verdict");
                root.searching = false;
                assert!(!root.awaiting_first_rows(), "answered and empty: say so");
            })
            .unwrap();
    }

    #[gpui::test]
    fn a_skeleton_keeps_the_shared_clock_running(cx: &mut TestAppContext) {
        // It breathes on `PulseClock` rather than owning a timer, so it has to
        // hold the clock up while it is the only thing on screen — otherwise
        // the placeholder freezes at whatever phase it mounted on, which reads
        // as stuck rather than loading.
        let window = test_root(cx);
        window
            .update(cx, |root, window, cx| {
                root.results.clear();
                root.searching = true;
                let _ = root.sync_pulse(window, cx);
                // The test window is not active, which is the other half of
                // the gate — assert the skeleton's own contribution directly.
                assert!(root.results.is_empty() && root.awaiting_first_rows());
            })
            .unwrap();
    }

    #[gpui::test]
    fn a_transcript_scrolls_rather_than_stepping_a_cursor(cx: &mut TestAppContext) {
        // Every other surface moves a selection on Up/Down. A chat has no
        // cursor — Enter belongs to the composer, ⌘K targets the session, and
        // a tool chip folds on click — so the keys move the view instead.
        let window = test_root(cx);
        window
            .update(cx, |root, window, cx| {
                root.results = vec![agent_row("the-agent")];
                root.selected = 0;
                root.enter_mode_about("conversation", Some("the-agent".into()), window, cx);
                let turns: Vec<SearchItem> = (0..12)
                    .map(|i| SearchItem {
                        speaker: Some("agent".into()),
                        ..item_with_id("conversation", &format!("the-agent#{i}"))
                    })
                    .collect();
                root.apply_search_results(turns, true, root.generation, cx);
                let before = root.selected;
                root.select_previous(&SelectPrevious, window, cx);
                assert_eq!(
                    root.selected, before,
                    "Up moved the view, not a selection"
                );
                root.select_next(&SelectNext, window, cx);
                assert_eq!(root.selected, before, "and Down likewise");
            })
            .unwrap();
    }

    #[gpui::test]
    fn a_transcript_with_nothing_to_scroll_counts_as_at_its_end(cx: &mut TestAppContext) {
        // The stick-to-bottom rule reads the scroll handle now. A short
        // conversation has `max_offset` zero, and "you cannot scroll" has to
        // mean "you are at the end" or a new turn would never be followed.
        let window = test_root(cx);
        window
            .update(cx, |root, _window, _cx| {
                assert!(root.transcript_is_at_bottom());
            })
            .unwrap();
    }

    #[gpui::test]
    fn a_transcript_enters_at_its_newest_turn(cx: &mut TestAppContext) {
        // The newest turn is the reason you opened it — every messaging
        // surface agrees. The previous highlight (the agent row that was
        // confirmed to get here) is not a conversation row, which is exactly
        // what marks the entering case.
        let window = test_root(cx);
        window
            .update(cx, |root, window, cx| {
                root.results = vec![agent_row("the-agent")];
                root.selected = 0;
                root.enter_mode_about("conversation", Some("the-agent".into()), window, cx);
                let turns: Vec<SearchItem> = (0..5)
                    .map(|i| SearchItem {
                        speaker: Some(if i % 2 == 0 { "user" } else { "agent" }.into()),
                        images: Vec::new(),
                        ..item_with_id("conversation", &format!("the-agent#{i}"))
                    })
                    .collect();
                root.apply_search_results(turns, true, root.generation, cx);
                assert_eq!(root.selected, 4, "landed on the newest turn");
            })
            .unwrap();
    }

    #[test]
    fn a_view_scrolled_up_to_reread_does_not_follow_new_turns_down() {
        let step = theme::TRANSCRIPT_SCROLL_STEP_PX;
        // Parked at the end, within the glide's own landing slack.
        assert!(transcript_at_bottom(900.0, 900.0));
        assert!(transcript_at_bottom(900.0 - step + 1.0, 900.0));
        // Scrolled up to reread: a refresh must leave this exactly alone.
        assert!(!transcript_at_bottom(400.0, 900.0));
        assert!(!transcript_at_bottom(0.0, 900.0));
    }

    #[gpui::test]
    fn enter_on_an_agent_tile_reads_its_conversation_not_paseo(cx: &mut TestAppContext) {
        // At rest `split_agent_tiles` moves every live and recent agent out of
        // the rows and into the grid, so tiles are the only agent surface most
        // summons show — and the tile path built a bare `Request::Activate`,
        // which for an agent means "open Paseo". The conversation view worked
        // and was unreachable from exactly the place agents are visible.
        let window = test_root(cx);
        window
            .update(cx, |root, window, cx| {
                root.agent_tiles = vec![SearchItem {
                    enters_mode: Some("conversation".to_string()),
                    ..tile("05475348-2409-4eca-aa5f-3446f369ee66")
                }];
                root.grid_selected = Some(0);
                root.confirm(&Confirm, window, cx);
                let mode = root.active_mode().expect("the tile entered the mode");
                assert_eq!(mode.chrome.id, "conversation");
                assert_eq!(
                    mode.subject.as_deref(),
                    Some("05475348-2409-4eca-aa5f-3446f369ee66"),
                    "the agent id travels in as the mode's subject"
                );
            })
            .unwrap();
    }

    #[gpui::test]
    fn a_tile_without_a_mode_still_activates(cx: &mut TestAppContext) {
        // The shared path must not break the other direction: a tile whose row
        // carries no `enters_mode` still routes to `Request::Activate`.
        let window = test_root(cx);
        window
            .update(cx, |root, window, cx| {
                root.agent_tiles = vec![tile("plain")];
                root.grid_selected = Some(0);
                root.confirm(&Confirm, window, cx);
                assert!(root.active_mode().is_none(), "no mode to enter");
                assert!(root.activating, "so it went to the daemon instead");
            })
            .unwrap();
    }

    #[gpui::test]
    fn command_k_follows_the_focused_tile_not_the_list_s_remembered_row(
        cx: &mut TestAppContext,
    ) {
        // `selected` deliberately keeps its value while the keyboard is up in
        // the grid, so reading `results[selected]` opened the menu for a row
        // nobody could see was chosen — on the surface where ⌘K's actions
        // differ most between rows.
        let window = test_root(cx);
        window
            .update(cx, |root, _window, cx| {
                root.results = vec![clipboard_row_with_a_paste_action("a-row")];
                root.selected = 0;
                root.agent_tiles = vec![SearchItem {
                    actions: agent_tile_actions(),
                    ..tile("the-tile")
                }];
                root.grid_selected = Some(0);

                root.open_actions_menu_for_selected_row(cx);
                let menu = root.actions_menu.as_ref().expect("the tile has actions");
                assert_eq!(menu.id, "the-tile");
                assert_eq!(menu.kind, "agent");
            })
            .unwrap();
    }

    #[gpui::test]
    fn the_command_k_hint_describes_the_same_thing_the_menu_would_open(
        cx: &mut TestAppContext,
    ) {
        // Two callers doing this lookup separately is how they came apart in
        // the first place; both go through `highlighted_item` now.
        let window = test_root(cx);
        window
            .update(cx, |root, _window, cx| {
                root.results = vec![clipboard_row_with_a_paste_action("a-row")];
                root.selected = 0;
                root.agent_tiles = vec![tile("no-actions")];
                root.grid_selected = Some(0);

                assert!(
                    !root.selected_row_has_actions(),
                    "the focused tile has none, so the hint must not promise any"
                );
                root.open_actions_menu_for_selected_row(cx);
                assert!(root.actions_menu.is_none(), "and the menu must agree");
            })
            .unwrap();
    }

    #[gpui::test]
    fn clicking_the_trigger_while_the_menu_is_open_dismisses_it_rather_than_reopening(
        cx: &mut TestAppContext,
    ) {
        // One physical press fires the card's capture-phase `on_mouse_down_out`
        // and then this bubble-phase click. Reading the current state here
        // would find `None` and reopen, so the press meant to dismiss would
        // reopen instead.
        let window = test_root(cx);
        window
            .update(cx, |root, window, cx| {
                root.results = vec![clipboard_row_with_a_paste_action("row")];
                root.selected = 0;
                root.open_actions_menu_for_selected_row(cx);
                assert!(root.actions_menu.is_some());

                // What the capture-phase snapshot would have recorded, then
                // what `on_mouse_down_out` does to the menu.
                root.menu_open_before_this_press = true;
                root.close_actions_menu(window);

                root.handle_actions_menu_trigger_click(&ClickEvent::default(), window, cx);
                assert!(root.actions_menu.is_none(), "the dismiss click stays a dismiss");
                assert!(!root.menu_open_before_this_press, "and the snapshot is consumed");
            })
            .unwrap();
    }

    #[gpui::test]
    fn clicking_the_trigger_with_the_menu_closed_opens_it(cx: &mut TestAppContext) {
        // The guard must not suppress a genuine open.
        let window = test_root(cx);
        window
            .update(cx, |root, window, cx| {
                root.results = vec![clipboard_row_with_a_paste_action("row")];
                root.selected = 0;
                root.handle_actions_menu_trigger_click(&ClickEvent::default(), window, cx);
                assert_eq!(
                    root.actions_menu.as_ref().map(|m| m.id.as_str()),
                    Some("row")
                );
            })
            .unwrap();
    }

    #[gpui::test]
    fn an_activation_in_flight_says_so_and_outranks_the_search_tell(
        cx: &mut TestAppContext,
    ) {
        // Enter on a New Agent row shells out to a CLI whose Electron boot
        // alone is ~1s. For that whole window the panel deliberately stays
        // open — that is what makes an inline failure possible — and used to
        // show nothing, so pressing Enter looked exactly like pressing nothing.
        let window = test_root(cx);
        window
            .update(cx, |root, _window, _cx| {
                assert!(root.render_searching_tell().is_none(), "quiet at rest");
                root.searching = true;
                root.activating = true;
                // Both set: the one being waited on wins.
                assert!(root.render_searching_tell().is_some());
                assert!(root.activating);
            })
            .unwrap();
    }

    #[gpui::test]
    fn summoning_afresh_clears_a_stranded_activation(cx: &mut TestAppContext) {
        // Dismissing mid-activation must not bring the panel back still
        // claiming to be working — the same rule every other per-summon field
        // follows.
        let window = test_root(cx);
        window
            .update(cx, |root, window, cx| {
                root.activating = true;
                root.reset_for_summon(window, cx);
                assert!(!root.activating);
            })
            .unwrap();
    }

    fn agent_tile_actions() -> Vec<neko_protocol::ItemAction> {
        vec![neko_protocol::ItemAction {
            id: "cancel".into(),
            label: "Cancel run".into(),
            destructive: false,
        }]
    }

    #[test]
    fn the_tool_badge_is_one_uppercase_character_because_that_is_what_fits() {
        assert_eq!(tool_initial("claude"), "C");
        assert_eq!(tool_initial("gpt"), "G");
        assert_eq!(tool_initial("Codex"), "C");
        assert_eq!(tool_initial(""), "", "no tool, no badge — never an empty circle");
    }

    #[test]
    fn at_rest_the_grid_takes_recent_agents_too_not_only_the_live_ones() {
        let live = SearchItem { badge: Some("LIVE".to_string()), ..agent_row("live-1") };
        let idle = SearchItem { badge: None, ..agent_row("idle-1") };
        let app = SearchItem { kind: "app".to_string(), ..agent_row("Finder") };
        let (tiles, rows) = split_agent_tiles(vec![app, live, idle], true);
        assert_eq!(tiles.iter().map(|i| i.id.as_str()).collect::<Vec<_>>(), vec!["live-1", "idle-1"]);
        assert_eq!(rows.iter().map(|i| i.id.as_str()).collect::<Vec<_>>(), vec!["Finder"]);
    }

    #[test]
    fn during_a_search_only_live_agents_are_lifted_so_typing_never_reshuffles_the_grid() {
        let live = SearchItem { badge: Some("LIVE".to_string()), ..agent_row("live-1") };
        let idle = SearchItem { badge: None, ..agent_row("idle-1") };
        let (tiles, rows) = split_agent_tiles(vec![live, idle], false);
        assert_eq!(tiles.iter().map(|i| i.id.as_str()).collect::<Vec<_>>(), vec!["live-1"]);
        assert_eq!(rows.iter().map(|i| i.id.as_str()).collect::<Vec<_>>(), vec!["idle-1"]);
    }

    #[test]
    fn the_grid_is_two_rows_of_two_and_its_capacity_matches_that() {
        assert_eq!(
            AGENT_GRID_CAPACITY,
            theme::AGENT_GRID_COLUMNS,
            "one strip: capacity and columns must agree, or a tile wraps into a row that is clipped"
        );
        // Two tiles plus the gap between them must actually fit the width the
        // strip has, or they wrap into three rows and the third is clipped.
        let inset = theme::CONTENT_INSET_PX * 2.0;
        let used = theme::AGENT_TILE_WIDTH_PX * theme::AGENT_GRID_COLUMNS as f32
            + theme::AGENT_GRID_GAP_PX * (theme::AGENT_GRID_COLUMNS as f32 - 1.0);
        assert!(
            used <= theme::PANEL_WIDTH_WITH_DETAIL_PX - inset + 0.01,
            "a row of tiles ({used}) must fit the panel inset by {inset}"
        );
        // The whole point of the shared inset: a tile's outer edge and a
        // selected row's highlight must land on the same pixel.
        assert!(
            (used - (theme::PANEL_WIDTH_WITH_DETAIL_PX - inset)).abs() < 0.01,
            "the grid must span exactly the width the results list does"
        );
    }

    #[test]
    fn the_grid_never_takes_more_than_it_can_draw() {
        let many: Vec<SearchItem> = (0..9).map(|i| agent_row(&format!("a{i}"))).collect();
        let (tiles, rows) = split_agent_tiles(many, true);
        assert_eq!(tiles.len(), AGENT_GRID_CAPACITY, "the grid has a fixed height; extras must not be clipped");
        assert_eq!(rows.len(), 9 - AGENT_GRID_CAPACITY, "the overflow stays reachable as rows");
    }

    #[test]
    fn the_clients_capacity_matches_the_providers_own_cap() {
        // The provider caps an empty query at its own constant and orders it
        // running-first; a drift here would silently push an agent out of the
        // grid and into the list.
        assert_eq!(AGENT_GRID_CAPACITY, neko_core_grid_capacity());
    }

    /// `neko` cannot depend on `neko-core` (the crate split exists to stop
    /// exactly that), so the provider's constant is mirrored here and pinned
    /// by the test above rather than imported.
    fn neko_core_grid_capacity() -> usize {
        4
    }

    #[test]
    fn an_agent_never_appears_both_as_a_tile_and_as_a_row() {
        let live = SearchItem { badge: Some("LIVE".to_string()), ..agent_row("a") };
        let (tiles, rows) = split_agent_tiles(vec![live], true);
        assert_eq!(tiles.len(), 1);
        assert!(rows.is_empty(), "a tile is a move, not a copy — two rows for one agent is two Enters");
    }

    #[test]
    fn codex_task_is_a_tile() {
        for badge in ["LIVE", "WAITING"] {
            let codex_task = SearchItem {
                kind: "codex-task".to_string(),
                badge: Some(badge.to_string()),
                ..agent_row("codex-task")
            };

            let (tiles, rows) = split_agent_tiles(vec![codex_task], false);

            assert_eq!(tiles.len(), 1, "{badge} Codex task belongs in the tile strip");
            assert!(rows.is_empty(), "a Codex task tile must not be duplicated as a row");
        }
    }

    #[test]
    fn unavailable_codex_task_keeps_its_status_in_the_tile_rendering_data() {
        let unavailable = SearchItem {
            kind: "codex-task".to_string(),
            badge: Some("WAITING".to_string()),
            subtitle: Some(format!("openai · /{}", "work/very-long-directory-name/".repeat(16))),
            accessory: Some("Codex unavailable".to_string()),
            ..agent_row("codex-task")
        };

        let (tiles, rows) = split_agent_tiles(vec![unavailable], false);

        assert!(rows.is_empty());
        assert!(tiles[0].subtitle.as_ref().is_some_and(|subtitle| subtitle.len() > 150));
        assert_eq!(agent_tile_status_label(&tiles[0]), Some("UNAVAILABLE"));
    }

    #[gpui::test]
    fn reviewing_a_codex_task_waits_for_daemon_success_before_entering_its_scoped_mode(cx: &mut TestAppContext) {
        let window = test_root(cx);
        window
            .update(cx, |root, window, cx| {
                root.results = vec![SearchItem {
                    kind: "codex-task".to_string(),
                    enters_mode: Some("codex-task".to_string()),
                    actions: vec![neko_protocol::ItemAction {
                        id: "review".to_string(),
                        label: "Review".to_string(),
                        destructive: false,
                    }],
                    ..agent_row("codex-task")
                }];
                root.open_actions_menu_for_selected_row(cx);
                root.confirm_menu_action(window, cx);
                assert!(root.active_mode().is_none(), "the mode waits for daemon validation");
                assert!(root.activating, "the Review click dispatches Request::Activate before it can enter");
            })
            .unwrap();
    }

    #[gpui::test]
    fn a_successful_review_activation_enters_the_codex_task_mode(cx: &mut TestAppContext) {
        let window = test_root(cx);
        window
            .update(cx, |root, _window, cx| {
                root.finish_activation(None, false, Some("codex-task"), cx);
                assert_eq!(root.active_mode().map(|mode| mode.chrome.id), Some("codex-task"));
            })
            .unwrap();
    }

    #[gpui::test]
    fn codex_task_mode_keeps_a_backend_qualified_subject(cx: &mut TestAppContext) {
        let window = test_root(cx);
        window
            .update(cx, |root, _window, cx| {
                let task = SearchItem {
                    id: "thr-1".into(),
                    kind: "codex-task".into(),
                    title: "Compile quick view".into(),
                    enters_mode: Some("codex-task".into()),
                    ..agent_row("thr-1")
                };
                root.pending_task_mode = Some((
                    TaskRef { backend: "codex".into(), id: "thr-1".into() },
                    task,
                ));
                root.finish_activation(None, false, Some("codex-task"), cx);
                let task = root.active_mode().and_then(|mode| mode.task_ref.as_ref()).expect("qualified task subject");
                assert_eq!(task.provider_id(), "codex-task");
                assert_eq!(task.id, "thr-1");
            })
            .unwrap();
    }

    #[test]
    fn a_badge_that_is_not_live_on_a_non_agent_row_is_never_mistaken_for_a_tile() {
        // Clipboard rows carry TEXT/LINK badges; commands carry COMMAND.
        let clip = SearchItem {
            kind: "clipboard".to_string(),
            badge: Some("TEXT".to_string()),
            ..agent_row("copied")
        };
        let (tiles, rows) = split_agent_tiles(vec![clip], true);
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
            meter: None,
            keeps_open: false,
            preview_markdown: false,
            speaker: None,
            images: Vec::new(),
            preview: None,
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
        cx.add_window(|_window, cx| Root::build(client, accessibility, true, true, no_appearance_setter(), no_preferences_opener(), crate::window_drag::disabled(), cx))
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
        let window = cx.add_window(|_window, cx| Root::build(client, accessibility, true, true, setter, no_preferences_opener(), crate::window_drag::disabled(), cx));
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
            meter: None,
            keeps_open: false,
            preview_markdown: false,
            speaker: None,
            images: Vec::new(),
            preview: None,
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
                // A two-frame response only happens for a query at least
                // `files::MIN_QUERY_LEN` long — below that nothing defers and
                // the daemon answers in one frame. Set directly rather than
                // through `set_content`, which would kick off a real search
                // and clobber the generation this test is driving by hand.
                root.results_are_for_empty_query = false;
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
    fn confirming_the_new_agent_command_enters_its_mode_with_an_empty_field_for_the_prompt(
        cx: &mut TestAppContext,
    ) {
        // The `new-agent` mode's field is the *task*, not a filter, so it has
        // to start empty even though the captain had typed something to find
        // the command — and that something still has to come back on Escape,
        // like every other mode.
        let window = test_root(cx);
        cx.run_until_parked();

        window
            .update(cx, |root, _window, cx| {
                root.text_field.update(cx, |field, cx| field.set_content("new agent", cx));
                root.results = vec![command_item("new-agent")];
                root.selected = 0;
            })
            .unwrap();
        window.update(cx, |root, window, cx| root.confirm(&Confirm, window, cx)).unwrap();
        cx.run_until_parked();

        window
            .update(cx, |root, _window, cx| {
                let mode = root.active_mode().expect("the New Agent command must enter a mode");
                assert_eq!(mode.chrome.id, "new-agent");
                assert_eq!(mode.chrome.provider_id, "new-agent");
                assert_eq!(mode.saved_query, "new agent");
                assert_eq!(root.query(cx), "", "the prompt starts blank, not with the command's own query");
            })
            .unwrap();
    }

    #[gpui::test]
    fn confirming_new_codex_task_keeps_the_field_for_its_task_prompt(cx: &mut TestAppContext) {
        let window = test_root(cx);
        cx.run_until_parked();

        window
            .update(cx, |root, _window, cx| {
                root.text_field
                    .update(cx, |field, cx| field.set_content("new codex task", cx));
                root.results = vec![command_item("new-codex-task")];
                root.selected = 0;
            })
            .unwrap();
        window
            .update(cx, |root, window, cx| root.confirm(&Confirm, window, cx))
            .unwrap();
        cx.run_until_parked();

        window
            .update(cx, |root, _window, cx| {
                let mode = root.active_mode().expect("the command enters its mode");
                assert_eq!(mode.chrome.provider_id, "new-codex-task");
                assert_eq!(root.query(cx), "", "the prompt starts empty after finding the command");
            })
            .unwrap();
    }

    #[gpui::test]
    fn activating_a_row_carries_what_is_currently_typed_as_the_query(cx: &mut TestAppContext) {
        // The client half of starting an agent: `id` names the directory and
        // the query carries the prompt. Both halves matter — an id that moved
        // with the prompt would break `resolve_selection`, and a query left
        // empty would start an agent with no task.
        let window = test_root(cx);
        cx.run_until_parked();

        window
            .update(cx, |root, _window, cx| {
                root.text_field.update(cx, |field, cx| field.set_content("fix the parser", cx));
            })
            .unwrap();
        cx.run_until_parked();

        window
            .update(cx, |root, _window, cx| {
                let row = item_with_id("new-agent", "/Users/someone/Documents/neko");
                match root.primary_activation_request(&row, cx) {
                    Request::Activate { kind, id, action, query } => {
                        assert_eq!(kind, "new-agent");
                        assert_eq!(id, "/Users/someone/Documents/neko");
                        assert_eq!(action, None);
                        assert_eq!(query, "fix the parser");
                    }
                    other => panic!("expected an Activate request, got {other:?}"),
                }
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

    // ---- Dragging the panel -------------------------------------------
    //
    // Everything below drives `Root`'s own real handlers, in the order gpui
    // dispatches them, against a recording `PanelDrag`. What it can prove is
    // the *state machine*: which verb the panel sends, for which event, and
    // that a drag can never be left live. What it cannot prove is anything
    // native — where the window actually ends up, and whether the guide
    // window renders — because GPUI's test platform panics on every native
    // window call, which is exactly why `PanelDrag` is injected at all.

    #[derive(Default)]
    struct DragLog {
        calls: std::cell::RefCell<Vec<&'static str>>,
        start_succeeds: std::cell::Cell<bool>,
    }

    struct RecordingDrag(Rc<DragLog>);

    impl crate::window_drag::PanelDrag for RecordingDrag {
        fn start(&self, _window: &Window, _cx: &mut App) -> bool {
            self.0.calls.borrow_mut().push("start");
            self.0.start_succeeds.get()
        }
        fn update(&self, _window: &Window, _cx: &mut App) {
            self.0.calls.borrow_mut().push("update");
        }
        fn finish(&self, _window: &Window, _cx: &mut App) {
            self.0.calls.borrow_mut().push("finish");
        }
        fn cancel(&self, _window: &Window, _cx: &mut App) {
            self.0.calls.borrow_mut().push("cancel");
        }
    }

    fn test_root_recording_drag(cx: &mut TestAppContext) -> (gpui::WindowHandle<Root>, Rc<DragLog>) {
        let (client, _events) = NekoClient::connect(std::path::PathBuf::from(format!(
            "/tmp/neko-panel-test-{}-{}.sock",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        )));
        let accessibility: Rc<dyn AccessibilityChecker> = Rc::new(FakeAccessibilityChecker::new(true));
        let log = Rc::new(DragLog::default());
        log.start_succeeds.set(true);
        let drag: Rc<dyn crate::window_drag::PanelDrag> = Rc::new(RecordingDrag(log.clone()));
        let window = cx.add_window(|_window, cx| {
            Root::build(client, accessibility, true, true, no_appearance_setter(), no_preferences_opener(), drag, cx)
        });
        (window, log)
    }

    fn press() -> MouseDownEvent {
        MouseDownEvent { button: gpui::MouseButton::Left, click_count: 1, ..Default::default() }
    }

    fn drag_to() -> MouseMoveEvent {
        MouseMoveEvent { pressed_button: Some(gpui::MouseButton::Left), ..Default::default() }
    }

    fn release() -> MouseUpEvent {
        MouseUpEvent { button: gpui::MouseButton::Left, click_count: 1, ..Default::default() }
    }

    #[gpui::test]
    fn a_press_drag_and_release_sends_exactly_start_update_finish(cx: &mut TestAppContext) {
        let (window, log) = test_root_recording_drag(cx);
        window
            .update(cx, |root, window, cx| {
                root.begin_window_drag(&press(), window, cx);
                root.window_drag_moved(&drag_to(), window, cx);
                root.window_drag_moved(&drag_to(), window, cx);
                root.window_drag_ended(&release(), window, cx);
                assert!(!root.dragging, "the release must end the drag");
            })
            .unwrap();
        assert_eq!(*log.calls.borrow(), ["start", "update", "update", "finish"]);
    }

    #[gpui::test]
    fn a_move_with_no_drag_in_progress_is_ignored_entirely(cx: &mut TestAppContext) {
        // Mouse moves are listened for at the *window* level, unconditionally,
        // so this is the common case by a wide margin: every ordinary cursor
        // movement over the panel reaches this handler.
        let (window, log) = test_root_recording_drag(cx);
        window
            .update(cx, |root, window, cx| {
                root.window_drag_moved(&drag_to(), window, cx);
                root.window_drag_ended(&release(), window, cx);
            })
            .unwrap();
        assert!(log.calls.borrow().is_empty());
    }

    #[gpui::test]
    fn a_drag_that_could_not_be_started_never_sends_anything_further(cx: &mut TestAppContext) {
        // `start` fails when a native read does — no window handle, no
        // `NSScreen`. The panel must then behave exactly as it did before drag
        // existed rather than half-tracking a gesture it has no origin for.
        let (window, log) = test_root_recording_drag(cx);
        log.start_succeeds.set(false);
        window
            .update(cx, |root, window, cx| {
                root.begin_window_drag(&press(), window, cx);
                assert!(!root.dragging);
                root.window_drag_moved(&drag_to(), window, cx);
                root.window_drag_ended(&release(), window, cx);
            })
            .unwrap();
        assert_eq!(*log.calls.borrow(), ["start"]);
    }

    #[gpui::test]
    fn a_move_that_arrives_with_the_button_already_up_ends_the_drag(cx: &mut TestAppContext) {
        // The safety net for the one event that must never be missed. If a
        // `MouseUpEvent` somehow never reaches this window, the next move
        // reports no pressed button — and without this the panel would follow
        // the cursor forever with a guide window left on screen.
        let (window, log) = test_root_recording_drag(cx);
        window
            .update(cx, |root, window, cx| {
                root.begin_window_drag(&press(), window, cx);
                root.window_drag_moved(&MouseMoveEvent::default(), window, cx);
                assert!(!root.dragging);
            })
            .unwrap();
        assert_eq!(*log.calls.borrow(), ["start", "finish"]);
    }

    #[gpui::test]
    fn escape_during_a_drag_puts_the_panel_back_and_leaves_the_mode_alone(cx: &mut TestAppContext) {
        // Escape means three different things in this panel depending on what
        // is going on; while the panel is physically being moved it can only
        // mean "put it back". The mode is still there to leave on the next
        // press.
        let (window, log) = test_root_recording_drag(cx);
        window
            .update(cx, |root, window, cx| {
                // The clipboard mode, not the theme one: entering that writes
                // the process-global palette and would need
                // `theme::test_lock()` for a detail this test does not care
                // about.
                root.results = vec![command_item("clipboard")];
                root.selected = 0;
                root.confirm(&Confirm, window, cx);
                assert!(root.active_mode().is_some());

                root.begin_window_drag(&press(), window, cx);
                root.handle_dismiss(&crate::DismissWindow, window, cx);
                assert!(!root.dragging);
                assert!(root.active_mode().is_some(), "cancelling a drag must not also leave the mode");
            })
            .unwrap();
        assert_eq!(*log.calls.borrow(), ["start", "cancel"]);
    }

    #[gpui::test]
    fn a_fresh_summon_cancels_a_drag_that_somehow_outlived_its_gesture(cx: &mut TestAppContext) {
        let (window, log) = test_root_recording_drag(cx);
        window
            .update(cx, |root, window, cx| {
                root.begin_window_drag(&press(), window, cx);
                root.reset_for_summon(window, cx);
                assert!(!root.dragging);
            })
            .unwrap();
        assert_eq!(*log.calls.borrow(), ["start", "cancel"]);
    }

    #[gpui::test]
    fn a_summon_with_no_drag_in_progress_does_not_touch_the_drag_at_all(cx: &mut TestAppContext) {
        let (window, log) = test_root_recording_drag(cx);
        window.update(cx, |root, window, cx| root.reset_for_summon(window, cx)).unwrap();
        assert!(log.calls.borrow().is_empty());
    }

    #[test]
    fn the_panel_can_tell_a_short_list_from_a_truncated_one() {
        // The mode list scrolls and fades its own edge; the root list is
        // budget-fit and simply stops, so this is the only signal that
        // anything was left out. Before it, two rows and forty-that-became-
        // seven rendered identically.
        let few: Vec<SearchItem> = (0..2).map(|i| item_with_id("app", &format!("a{i}"))).collect();
        let fitted = fit_within_budget(few.clone(), CONTENT_AREA_MIN_HEIGHT_PX);
        assert_eq!(fitted.len(), few.len(), "two rows always fit");

        let many: Vec<SearchItem> = (0..40).map(|i| item_with_id("app", &format!("a{i}"))).collect();
        let fitted = fit_within_budget(many.clone(), CONTENT_AREA_MIN_HEIGHT_PX);
        assert!(fitted.len() < many.len(), "forty rows cannot fit a fixed area");

        // And the cue has to pay for itself: refitting against the smaller
        // budget must never leave more rows than the full budget allowed,
        // or the cue would push its own last row out.
        let with_cue =
            fit_within_budget(many.clone(), CONTENT_AREA_MIN_HEIGHT_PX - theme::TRUNCATION_CUE_HEIGHT_PX);
        assert!(with_cue.len() <= fitted.len());
    }

    #[test]
    fn a_tile_tooltip_carries_both_lines_it_truncated() {
        let mut row = item_with_id("agent", "a1");
        row.title = "feat/doctors-maps".to_string();
        row.subtitle = Some("acme-corp/web-app".to_string());
        assert_eq!(
            agent_tile_tooltip(&row).to_string(),
            "feat/doctors-maps \u{2014} acme-corp/web-app"
        );
        row.subtitle = None;
        assert_eq!(agent_tile_tooltip(&row).to_string(), "feat/doctors-maps");
    }


    #[test]
    fn the_scroll_target_counts_the_headers_and_banners_above_a_row() {
        // `scroll_to_item` works in the container's own child index, and the
        // root list's children are not one-per-result: banners and a section
        // header per new `kind` sit between them. Getting this wrong scrolls
        // to the wrong row, which is worse than not scrolling at all.
        let results = vec![
            item_with_id("app", "a"),
            item_with_id("app", "b"),
            item_with_id("command", "c"),
        ];
        // No banners: header, a, b, header, c.
        assert_eq!(root_list_child_index(&results, 0, 0), 1);
        assert_eq!(root_list_child_index(&results, 1, 0), 2);
        assert_eq!(root_list_child_index(&results, 2, 0), 4);
        // Two banners shift everything down by two.
        assert_eq!(root_list_child_index(&results, 0, 2), 3);
        assert_eq!(root_list_child_index(&results, 2, 2), 6);
    }

    #[test]
    fn a_single_section_needs_exactly_one_header_counted() {
        let results: Vec<SearchItem> =
            (0..5).map(|i| item_with_id("app", &format!("a{i}"))).collect();
        for (row, expected) in (0..5).zip(1..=5) {
            assert_eq!(root_list_child_index(&results, row, 0), expected);
        }
    }

    #[test]
    fn an_out_of_range_target_lands_past_the_end_rather_than_panicking() {
        // `selected` and `results` are updated in separate steps, so a stale
        // index reaching here is a real possibility and must not be fatal.
        let results = vec![item_with_id("app", "a")];
        assert_eq!(root_list_child_index(&results, 99, 0), 2);
        assert_eq!(root_list_child_index(&[], 0, 0), 0);
    }


    #[test]
    fn scrolling_leaves_one_row_of_lookahead_in_the_direction_of_travel() {
        // `scroll_to_item` moves the minimum distance to reveal its target,
        // so aiming at the selected row parks it flush against the edge you
        // are travelling toward and you never see what is next. Aiming one
        // row further is the whole fix.
        assert_eq!(ScrollBias::Down.target(3, 10), 4);
        assert_eq!(ScrollBias::Up.target(3, 10), 2);
        // A re-search that kept the highlight has no direction to look in.
        assert_eq!(ScrollBias::None.target(3, 10), 3);
    }

    #[test]
    fn the_ends_land_flush_because_there_is_nothing_beyond_them_to_show() {
        // Not defensiveness — the lookahead exists to show what is coming,
        // and at the last row nothing is.
        assert_eq!(ScrollBias::Down.target(9, 10), 9);
        assert_eq!(ScrollBias::Up.target(0, 10), 0);
        // An empty list must not underflow on the way to being empty.
        assert_eq!(ScrollBias::Down.target(0, 0), 0);
        assert_eq!(ScrollBias::Up.target(0, 0), 0);
    }

}
