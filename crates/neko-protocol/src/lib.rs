//! Wire types shared between `neko-daemon` and `neko-client`.
//!
//! This crate is intentionally inert: serde types, a length-prefixed framing
//! codec, and the one filesystem path both sides must agree on. No search
//! ranking, no SQLite, no AppKit — that all lives in `neko-core` (daemon side)
//! or the `neko` app crate (client-only concerns like live OS hotkey capture).

use std::io::{self, Read, Write};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// A modifier key in a hotkey combination, independent of any particular
/// hotkey-registration crate's own enum so this type can stay in the pure
/// wire layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Modifier {
    Cmd,
    Alt,
    Ctrl,
    Shift,
}

/// A hotkey combination: zero or more modifiers plus one key, named the way
/// `global-hotkey`'s `Code` enum names it (e.g. `"Space"`, `"KeyA"`) so the
/// client can round-trip it without the protocol crate depending on that
/// crate's types directly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HotkeyCombo {
    pub modifiers: Vec<Modifier>,
    pub key: String,
}

impl HotkeyCombo {
    pub fn new(modifiers: Vec<Modifier>, key: impl Into<String>) -> Self {
        Self {
            modifiers,
            key: key.into(),
        }
    }

    /// The slice-1 / v1 default: ⌥Space.
    pub fn default_summon() -> Self {
        Self::new(vec![Modifier::Alt], "Space")
    }

    /// A human-readable rendering like `⌥Space`, for UI and logs.
    pub fn display(&self) -> String {
        let mut s = String::new();
        for m in &self.modifiers {
            s.push_str(match m {
                Modifier::Cmd => "⌘",
                Modifier::Alt => "⌥",
                Modifier::Ctrl => "⌃",
                Modifier::Shift => "⇧",
            });
        }
        s.push_str(&self.key);
        s
    }
}

/// The daemon-persisted hotkey setting, versioned by `updated_at` so a
/// client can tell a push event apart from stale state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HotkeyConfig {
    pub combo: HotkeyCombo,
    pub updated_at_unix_ms: i64,
}

/// One selectable built-in theme, as the daemon advertises it. **Names and
/// ids only** — the actual colour values live in the `neko` app crate
/// (`theme.rs`), because a palette is a client-side rendering concern the
/// daemon has no use for and this crate must stay inert (serde types only, no
/// `gpui`, no `Rgba`).
///
/// The two halves are pinned together by `theme::tests::themes_match_the_protocol_registry`
/// in the client, so a theme added on one side and not the other fails a test
/// rather than shipping as a row that cannot be selected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuiltinTheme {
    pub id: &'static str,
    pub name: &'static str,
    /// One short line under the name in the `Themes` mode's list, e.g.
    /// `"Dark · Catppuccin · translucent"`.
    pub description: &'static str,
}

/// Every theme neko ships, in list order. This is a compiled-in table, not
/// something scanned or persisted: "what themes exist" is a build-time fact
/// (there is no user-supplied-palette loading, and no extension host yet — see
/// `AGENTS.md`'s "Seams for follow-up work").
pub const BUILTIN_THEMES: &[BuiltinTheme] = &[
    BuiltinTheme { id: "neutral", name: "Neko Neutral", description: "Dark · neutral grey · translucent" },
    BuiltinTheme { id: "ember", name: "Ember", description: "Dark · aubergine to rust · translucent" },
    BuiltinTheme { id: "catnap", name: "Catnap", description: "Dark · violet and magenta · translucent" },
    BuiltinTheme { id: "catppuccin-mocha", name: "Catppuccin Mocha", description: "Dark · Catppuccin · translucent" },
    BuiltinTheme { id: "catppuccin-macchiato", name: "Catppuccin Macchiato", description: "Dark · Catppuccin · translucent" },
    BuiltinTheme { id: "catppuccin-frappe", name: "Catppuccin Frappé", description: "Dark · Catppuccin · translucent" },
    BuiltinTheme { id: "catppuccin-latte", name: "Catppuccin Latte", description: "Light · Catppuccin · translucent" },
    BuiltinTheme { id: "gruvbox-dark", name: "Gruvbox Dark", description: "Dark · Gruvbox · translucent" },
    BuiltinTheme { id: "gruvbox-light", name: "Gruvbox Light", description: "Light · Gruvbox · translucent" },
    BuiltinTheme { id: "solarized-dark", name: "Solarized Dark", description: "Dark · Solarized · translucent" },
    BuiltinTheme { id: "solarized-light", name: "Solarized Light", description: "Light · Solarized · translucent" },
    BuiltinTheme { id: "nord", name: "Nord", description: "Dark · Nord · translucent" },
    BuiltinTheme { id: "tokyo-night", name: "Tokyo Night", description: "Dark · Tokyo Night Storm · translucent" },
    BuiltinTheme { id: "rose-pine", name: "Rosé Pine", description: "Dark · Rosé Pine · translucent" },
    BuiltinTheme { id: "rose-pine-dawn", name: "Rosé Pine Dawn", description: "Light · Rosé Pine Dawn · translucent" },
    BuiltinTheme { id: "dracula", name: "Dracula", description: "Dark · Dracula · translucent" },
    BuiltinTheme { id: "everforest-dark", name: "Everforest Dark", description: "Dark · Everforest · translucent" },
];

/// The theme a fresh install renders, and the fallback for a persisted id that
/// names no built-in. Deliberately the palette neko already shipped: upgrading
/// must not change how anybody's app looks without them asking.
pub const DEFAULT_THEME_ID: &str = "neutral";

pub fn builtin_theme(id: &str) -> Option<&'static BuiltinTheme> {{
    BUILTIN_THEMES.iter().find(|t| t.id == id)
}}

/// A row's icon slot content. Closed by design, unlike `SearchItem::kind`
/// below — this is a bounded set of things the client actually knows how to
/// paint (a cached raster, or one of a handful of hand-drawn glyphs), not an
/// extension point. A provider that wants a genuinely new visual (not just a
/// new *result type*) adds a `Glyph` variant and one paint function in the
/// client — still far cheaper than today's per-kind `match` sprinkled across
/// the row, section header, and footer, and orthogonal to which provider
/// produced the row. The real per-extension-drawable-vocabulary problem is
/// explicitly out of scope for this task — see `AGENTS.md`'s "Provider
/// abstraction" section.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Icon {
    /// An absolute path to a cached PNG, rendered via `img()`.
    Image(String),
    /// No per-item raster (yet, or ever) — paint this built-in shape
    /// instead of an empty socket.
    Glyph(Glyph),
    /// An empty placeholder square — the "no icon yet, self-heals later"
    /// state apps use while their real icon is still warming in the
    /// background icon-extraction pass.
    Placeholder,
}

/// A small hand-painted shape for the row-icon slot, in the same spirit as
/// `panel.rs`'s `search_glyph` — this codebase has no bundled SVG-asset
/// pipeline, and a Unicode symbol isn't a reliable substitute (design
/// report §6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Glyph {
    /// Three stacked bars — a clipboard entry with no URL type.
    Text,
    /// Two overlapping rounded-square rings — a clipboard entry that looks
    /// like a link.
    Link,
    /// A plain document outline — a file search result.
    File,
    /// A folder shape — a directory search result.
    Folder,
    /// A clipboard board with a clip tab — a command that enters clipboard
    /// history mode (`SearchItem::enters_mode`). The same shape
    /// `data/neko-design/report.md`'s mockup 12 uses for the mode's own
    /// input-row glyph, reused here for the root-list row that leads to it.
    Clipboard,
    /// A rounded terminal-ish square with a dot — an agent that exists but
    /// is not currently doing anything.
    Agent,
    /// The same mark with a filled presence dot — a *running* agent. A
    /// separate variant rather than a flag on [`Glyph::Agent`] because the
    /// client paints it differently (see `panel.rs`'s live treatment), and
    /// the vocabulary here is "what to paint", not "what it means".
    AgentLive,
    /// Two stacked horizontal rails, each with a small knob at a different
    /// offset — the settings/preferences mark. Painted rather than a font
    /// glyph or an SVG asset, the same as every other shape in this
    /// vocabulary (there is no icon-asset pipeline in this codebase).
    Sliders,
    /// Four filled swatches in a 2×2 block, painted in the *live* theme's own
    /// panel/selected/success/danger colours — a theme row and the `Themes`
    /// command both use it. The one glyph in this vocabulary whose appearance
    /// changes with the active theme, deliberately: it is the affordance for
    /// changing that theme.
    Palette,
}

/// One named secondary action a row's `⌘K` actions menu can offer, beyond
/// the primary action `action_label`/`Request::Activate` already cover —
/// e.g. clipboard's "Copy" and "Delete" alongside its default "Paste".
/// Carried as plain data on `SearchItem`, the same "provider describes it,
/// client just renders it" shape `action_label`/`badge`/`icon` already
/// established — see `AGENTS.md`'s "Commands and modes" section. Empty for
/// every provider that has nothing beyond its one primary action (apps,
/// files, settings, commands) — the client menu simply has nothing to show
/// for those rows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ItemAction {
    /// Routed back through `Request::Activate`'s own `action` field to
    /// `Provider::perform_action`.
    pub id: String,
    /// The menu row's own label, e.g. `"Paste"`, `"Copy"`, `"Delete"`.
    pub label: String,
    /// Whether this action is destructive and permanent (deletes data) —
    /// the client requires a second, explicit confirmation before actually
    /// performing it, and renders it in the danger color.
    pub destructive: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchItem {
    /// Stable identifier the daemon can resolve back to an action target —
    /// for the app provider this is the bundle path; for the clipboard
    /// provider this is the entry's own content (also the SQLite primary
    /// key, so it doubles as the dedup key); for the file provider, the
    /// file's absolute path.
    pub id: String,
    /// The provider that produced this row (`Provider::id()`, e.g. `"app"`,
    /// `"clipboard"`, `"file"`) — a plain string, not a closed enum, so a
    /// future provider never needs to touch this crate to introduce a new
    /// result type. Used only for two things: grouping a contiguous run of
    /// results under one section header, and routing `Request::Activate`
    /// back to the provider that owns `id`'s namespace. Every other
    /// rendering decision (icon, section label, action verb) is carried as
    /// plain data on this struct instead of being derived from `kind` by a
    /// client-side `match` — see `AGENTS.md`'s "Provider abstraction"
    /// section for why that distinction is the point of this refactor.
    pub kind: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub icon: Icon,
    /// The header text above this row's contiguous section, e.g.
    /// "Applications", "Clipboard", "Files".
    pub section_label: String,
    /// The footer's primary-action label when this row is selected, e.g.
    /// `"Open  ↵"`, `"Paste  ↵"` — matches whatever `Request::Activate`
    /// actually does for this provider.
    pub action_label: String,
    /// A short uppercase type tag rendered next to the row (`"TEXT"`,
    /// `"LINK"`) — `None` for providers that don't have one.
    pub badge: Option<String>,
    /// A short trailing accessory string (a relative timestamp, ...) —
    /// `None` for providers that don't have one.
    pub accessory: Option<String>,
    /// When set, confirming this row enters a client-side "mode" instead of
    /// calling `Request::Activate` — the value is the mode id (today,
    /// always the scoped provider's own `id()`, `"clipboard"`), which
    /// `neko`'s `modes` module resolves to that mode's chrome (placeholder,
    /// footer title) and to `Request::Search`'s `provider` field for the
    /// mode's own filtered list. `None` for every ordinary result. See
    /// `AGENTS.md`'s "Commands and modes" section.
    pub enters_mode: Option<String>,
    /// An additional, orthogonal grouping label a mode's own list can use
    /// instead of `section_label` (e.g. `"Today"`, `"Yesterday"` for
    /// clipboard history) — `render_content_area`'s ordinary root-list
    /// rendering never reads this field at all; only a mode's own list
    /// rendering does. Kept separate from `section_label` because the same
    /// item needs a *different* header depending on whether it's shown in
    /// the merged root list (grouped by provider) or inside its own mode
    /// (grouped by this). `None` for providers that don't group this way.
    pub group_label: Option<String>,
    /// Secondary actions this row's `⌘K` menu offers — see [`ItemAction`].
    /// Empty for providers with nothing beyond their one primary action.
    pub actions: Vec<ItemAction>,
    /// A short "where this came from" label, distinct from `subtitle`
    /// (which providers already compose into a full sentence, e.g.
    /// "Copied from Terminal") — this is just the bare value, for a detail
    /// pane's own labeled field ("Application: Terminal"), where a full
    /// sentence would be redundant against a label already saying what the
    /// field means. `None` for providers with nothing to say here.
    pub source: Option<String>,
    /// A quantity this row is *about*, rather than a thing to open — a
    /// quota window, a disk, a download. A row carrying one renders as a
    /// card with a bar and stat columns instead of an ordinary text row
    /// (`panel::render_meter_card`); `None` — every provider before this
    /// field existed — renders exactly as before.
    ///
    /// Same bounded-vocabulary rule as [`Icon`]/[`Glyph`]: the provider
    /// supplies numbers and labels, the client owns entirely what a meter
    /// *looks* like. Adding this is deliberately not a client-side `match`
    /// on `kind == "usage"`, which is what `AGENTS.md`'s "Provider
    /// abstraction" section exists to forbid.
    #[serde(default)]
    pub meter: Option<Meter>,
    /// Confirming this row performs a *step*, not a finish — so the panel
    /// stays open and re-searches instead of getting out of the way.
    ///
    /// Every ordinary row means "do this and let me get on with it", which
    /// is why hiding is the default. A row that proposes something (see
    /// `neko_core::ask`: type a sentence, get a tool call, press Enter
    /// again to run it) would be unusable if the first Enter dismissed the
    /// panel it is asking you to look at. Reuses the path `⌘K` menu actions
    /// already take — `panel::Root::perform_activation`'s
    /// `hide_on_success: false` — rather than adding a second one.
    #[serde(default)]
    pub keeps_open: bool,
    /// Many lines of text a mode's detail pane should show verbatim — a
    /// terminal's captured output, and anything else too big for a row.
    ///
    /// The clipboard mode's detail pane predates this and renders
    /// `SearchItem::id`, which works only because a clipboard entry's id
    /// *is* its content. Nothing else has that coincidence, so a provider
    /// with a preview to show now says so directly instead of smuggling it
    /// through an identifier.
    #[serde(default)]
    pub preview: Option<String>,
    /// Whether `preview` is markdown the client should render as such, rather
    /// than plain text.
    ///
    /// Same bounded-vocabulary rule as `Icon`/`Meter`: the provider states the
    /// *format*, the client owns entirely what rendering it looks like — never
    /// `if kind == "conversation"` in the panel, which is what the provider
    /// abstraction exists to forbid. `false` (the wire default) is plain text,
    /// which every pre-existing provider already meant.
    #[serde(default)]
    pub preview_markdown: bool,
    /// Who a conversation turn belongs to — `"user"`, `"agent"`, or `"tool"`.
    ///
    /// Read only by the transcript layout (`ModeChrome::transcript`); `None`
    /// everywhere else. Same bounded-vocabulary rule as `Icon`/`Meter`/
    /// `preview_markdown`: the provider states which voice a row speaks in,
    /// the client owns entirely what a voice looks like.
    #[serde(default)]
    pub speaker: Option<String>,
    /// Cached image files this row carries, as absolute paths.
    ///
    /// **Paths, never bytes.** One screenshot in a real transcript is ~138 KB
    /// of base64; the daemon decodes it once to
    /// `~/Library/Caches/neko/conversation-images/` and sends where it landed,
    /// so a re-search does not ship the picture again on every keystroke.
    /// Same arrangement `Icon::Image` already uses for app artwork.
    #[serde(default)]
    pub images: Vec<String>,
}

/// A 0..=1 reading plus the labelled values that explain it. See
/// [`SearchItem::meter`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Meter {
    /// Clamped to `0.0..=1.0` by the renderer, so a provider that reports
    /// over-quota draws a full bar rather than one that overflows its own
    /// track.
    pub fraction: f32,
    /// Small-label-over-large-value pairs, rendered as evenly spaced
    /// columns beneath the bar. Two is the shape this was designed
    /// against; the renderer lays out however many it is given.
    pub stats: Vec<MeterStat>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MeterStat {
    pub label: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Request {
    Ping,
    /// `provider`, when set, scopes this search to exactly one provider's
    /// own `search()` — no cross-provider `allocate()`, no section
    /// reservation, just that provider's own candidates sorted by score.
    /// This is the mode seam: entering a mode (`SearchItem::enters_mode`)
    /// means every subsequent keystroke searches only the mode's own
    /// provider, scoped to as many matches as its own list wants to
    /// consider, not the shared root-list budget. `None` (every call site
    /// before this field existed) is the ordinary merged root-list search,
    /// completely unchanged.
    Search { query: String, limit: usize, provider: Option<String> },
    /// Perform a `SearchItem`'s action — launch an app, write the
    /// pasteboard, open a file, or (when `action` is set) one of a
    /// provider's own secondary actions (`Provider::perform_action`) —
    /// routed by `kind` to whichever provider produced `id`. `action: None`
    /// is the provider's one primary action (`Provider::activate`,
    /// unchanged from before this field existed); `action: Some(id)` is a
    /// named secondary action from that row's own `SearchItem::actions`
    /// (e.g. clipboard's "copy"/"delete" alongside its default "paste").
    /// One generic request for every provider, present and future: a new
    /// provider never needs a new `Request` variant, just an `activate`
    /// (and, optionally, `perform_action`) implementation of its own.
    /// Replaces what used to be two separate per-kind requests (`Launch`,
    /// `Paste`) — see `AGENTS.md`'s "Provider abstraction" section.
    ///
    /// `query` is the search field's own contents at the moment of
    /// activation — empty for every activation that isn't driven by one
    /// (the Preferences window's own writes, for instance). It exists
    /// because a row's action can legitimately take an *argument*: the
    /// `new-agent` provider's rows are "start an agent **on what you
    /// typed**, here", where `id` names the working directory and `query`
    /// carries the prompt. Every other provider ignores it for free —
    /// `Provider::activate_with_query` defaults to dropping it and calling
    /// `activate`, exactly as `search_cancellable` defaults to dropping its
    /// cancel token.
    ///
    /// **Two cheaper-looking alternatives were tried first and are wrong**,
    /// recorded here so they are not re-tried: encoding the prompt into
    /// `id` alongside the directory makes the row's identity change on
    /// every keystroke, and `panel::resolve_selection` keys the highlight
    /// on `(kind, id)` — so a captain who picks a project and then types
    /// one more word is silently returned to the first row, and Enter
    /// starts the agent in the wrong repository. Having the provider
    /// remember the last query it was searched with races the daemon's own
    /// documented out-of-order request completion (`handle_connection`),
    /// which can leave a stale prompt behind. Both failures are silent and
    /// both produce a *wrong action*, not a visible error.
    Activate { kind: String, id: String, action: Option<String>, #[serde(default)] query: String },
    GetHotkey,
    /// Fast, side-effect-free check against known OS/third-party reserved
    /// combinations (Spotlight, Mission Control, ...). Does not persist
    /// anything and does not prove the combo is free at the OS level — only
    /// a live registration attempt (client-side) can prove that.
    CheckHotkeyConflict { candidate: HotkeyCombo },
    /// Persist a candidate the client has already live-registered
    /// successfully. The daemon trusts the caller on the OS-level part of
    /// the conflict check and only owns storage + fan-out.
    CommitHotkey { candidate: HotkeyCombo },
    /// Whether the first-run onboarding arc has been completed, and whether
    /// the accessibility-refused banner (design report §3, step 08) has
    /// been dismissed.
    GetOnboardingState,
    /// Mark onboarding finished (or, for test/reset purposes, un-finished —
    /// see `neko`'s `NEKO_RESET_ONBOARDING` env var).
    SetOnboardingComplete { completed: bool },
    /// Dismiss the "accessibility is off" banner shown in the summoned
    /// panel after onboarding, once seen. Never re-shown once dismissed,
    /// unless accessibility is later re-declined after being re-granted.
    DismissAccessibilityBanner,
    /// The clipboard-history *permission* toggle onboarding asks for
    /// (design report §3, steps 06-07). Not TCC-gated on macOS — this is
    /// neko's own setting, not an OS grant. The seam a parallel clipboard-
    /// history watcher reads before it starts watching `NSPasteboard`.
    GetClipboardHistoryEnabled,
    SetClipboardHistoryEnabled { enabled: bool },
    /// The persisted theme id (see [`BUILTIN_THEMES`]). The client asks once at
    /// startup, before its first frame, so the very first summon already
    /// renders in the chosen palette rather than flashing the default.
    ///
    /// There is deliberately no `SetTheme` counterpart: committing a theme goes
    /// through the ordinary `Request::Activate { kind: "theme", id }` path like
    /// every other provider's result, so the `Themes` mode needed no
    /// theme-specific write request at all.
    GetTheme,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Response {
    Pong,
    /// One search reply. **A single `Request::Search` can be answered by
    /// more than one of these** — see [`Response::ends_request`] and
    /// `AGENTS.md`'s "Two-phase search" section.
    ///
    /// `complete: false` is a *partial* answer: every provider that could
    /// respond instantly has, and at least one slower provider
    /// (`Provider::defers_for`, i.e. file search's `mdfind` round-trip) is
    /// still running. Render it — that is the whole point, it arrives in
    /// well under a millisecond where the merged answer can take up to
    /// `files::QUERY_TIMEOUT` — but expect a second frame for the same
    /// request id carrying the full, correctly-allocated result set.
    ///
    /// `complete: true` is the last frame for this request id, whether it
    /// followed a partial one or answered the whole request on its own
    /// (which is what happens whenever no registered provider defers for
    /// this query — a short query, or a build with no slow provider
    /// registered at all).
    SearchResults { items: Vec<SearchItem>, complete: bool },
    Activated,
    Hotkey { config: HotkeyConfig },
    HotkeyConflict { reason: Option<String> },
    OnboardingState { completed: bool, accessibility_banner_dismissed: bool },
    ClipboardHistoryEnabled { enabled: bool },
    Theme { id: String },
    Error { message: String },
}

impl Response {
    /// Whether this is the final response for its request id. Everything
    /// except a partial [`Response::SearchResults`] is — this is the one
    /// place the "a request may be answered more than once" rule is
    /// written down, so `neko-client`'s reader loop can decide when to
    /// retire a request's correlation entry without re-deriving the rule
    /// from the payload shape at each call site.
    pub fn ends_request(&self) -> bool {
        !matches!(self, Response::SearchResults { complete: false, .. })
    }
}

/// Server-initiated messages, delivered on the same connection as request
/// replies but not correlated to a request id.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Event {
    HotkeyChanged { config: HotkeyConfig },
    /// A batch of background icon extraction finished — at least one
    /// `SearchItem::icon_path` a client already has may now resolve where
    /// it previously didn't. Icon extraction runs on the daemon's own
    /// background thread (real AppKit work, tens of ms per app), started
    /// after the app index is already searchable, so a client's very first
    /// search reply after a fresh install or a daemon restart is expected
    /// to have empty `icon_path`s for apps not extracted yet. Without this
    /// push, nothing ever prompts the client to ask again — search results
    /// already delivered are a one-time snapshot, not a live view, so the
    /// icon sockets would stay blank until the next thing that happens to
    /// re-run a search (typing, or a fresh summon), which is not
    /// guaranteed to happen soon, or at all, in the same process lifetime.
    IconsUpdated,
    /// A client committed a theme. Broadcast so every *other* connected client
    /// repaints too — the committing one has already applied it locally (live
    /// preview means it was applied before the round-trip even started).
    ThemeChanged { id: String },
    /// How many agents are stopped, waiting for a person — broadcast when
    /// the number *changes*, not on every poll.
    ///
    /// The rows themselves already arrive through the ordinary search path,
    /// so this exists for the case a search cannot serve: the panel is
    /// hidden. neko is resident and invisible almost all of the time, and an
    /// agent that blocks while it is hidden would otherwise wait until the
    /// next summon to be noticed. The client puts this on the Dock tile.
    AttentionChanged { count: usize },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Frame {
    Request { id: u64, request: Request },
    Response { id: u64, response: Response },
    Event(Event),
}

/// `~/Library/Application Support/neko/neko.sock` — the one thing both the
/// daemon and the client must agree on without either depending on the
/// other.
pub fn socket_path() -> PathBuf {
    support_dir().join("neko.sock")
}

/// `~/Library/Application Support/neko/neko.db` — SQLite, daemon-owned,
/// single writer. The client never opens this file.
pub fn database_path() -> PathBuf {
    support_dir().join("neko.db")
}

pub fn support_dir() -> PathBuf {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"));
    home.join("Library/Application Support/neko")
}

/// Write one length-prefixed JSON frame: a u32-LE byte length followed by
/// the JSON payload. Local IPC only, so JSON's debuggability outweighs the
/// bytes a binary codec would save.
pub fn write_frame<W: Write>(mut w: W, frame: &Frame) -> io::Result<()> {
    let payload = serde_json::to_vec(frame).map_err(io::Error::other)?;
    let len = u32::try_from(payload.len()).map_err(io::Error::other)?;
    w.write_all(&len.to_le_bytes())?;
    w.write_all(&payload)?;
    w.flush()
}

/// Read one length-prefixed JSON frame. Returns `Ok(None)` on a clean EOF
/// between frames (the other side closed the connection).
pub fn read_frame<R: Read>(mut r: R) -> io::Result<Option<Frame>> {
    let mut len_bytes = [0u8; 4];
    match r.read_exact(&mut len_bytes) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e),
    }
    let len = u32::from_le_bytes(len_bytes) as usize;
    let mut payload = vec![0u8; len];
    r.read_exact(&mut payload)?;
    let frame = serde_json::from_slice(&payload).map_err(io::Error::other)?;
    Ok(Some(frame))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_round_trips_through_the_wire_codec() {
        let frame = Frame::Request {
            id: 7,
            request: Request::Search {
                query: "fin".into(),
                limit: 8,
                provider: None,
            },
        };
        let mut buf = Vec::new();
        write_frame(&mut buf, &frame).unwrap();
        let decoded = read_frame(&buf[..]).unwrap().unwrap();
        match decoded {
            Frame::Request { id, request: Request::Search { query, limit, provider } } => {
                assert_eq!(id, 7);
                assert_eq!(query, "fin");
                assert_eq!(limit, 8);
                assert_eq!(provider, None);
            }
            other => panic!("unexpected frame: {other:?}"),
        }
    }

    #[test]
    fn empty_stream_reads_as_clean_eof() {
        let buf: &[u8] = &[];
        assert!(read_frame(buf).unwrap().is_none());
    }

    #[test]
    fn a_partial_search_result_does_not_end_its_request_but_everything_else_does() {
        assert!(!Response::SearchResults { items: Vec::new(), complete: false }.ends_request());
        assert!(Response::SearchResults { items: Vec::new(), complete: true }.ends_request());
        assert!(Response::Pong.ends_request());
        assert!(Response::Error { message: "x".into() }.ends_request());
    }

    #[test]
    fn default_summon_combo_displays_as_option_space() {
        assert_eq!(HotkeyCombo::default_summon().display(), "⌥Space");
    }
}
