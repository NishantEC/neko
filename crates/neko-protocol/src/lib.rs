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
    Activate { kind: String, id: String, action: Option<String> },
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Response {
    Pong,
    SearchResults { items: Vec<SearchItem> },
    Activated,
    Hotkey { config: HotkeyConfig },
    HotkeyConflict { reason: Option<String> },
    OnboardingState { completed: bool, accessibility_banner_dismissed: bool },
    ClipboardHistoryEnabled { enabled: bool },
    Error { message: String },
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
    fn default_summon_combo_displays_as_option_space() {
        assert_eq!(HotkeyCombo::default_summon().display(), "⌥Space");
    }
}
