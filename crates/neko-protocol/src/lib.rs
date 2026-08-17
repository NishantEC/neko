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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResultKind {
    App,
    Clipboard,
}

/// The clipboard content types this v1 distinguishes. Images are an
/// explicit non-goal for this slice — see `neko-core`'s `clipboard` module
/// doc comment for the seam a future task plugs an `Image` variant into.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClipboardContentKind {
    Text,
    Link,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchItem {
    /// Stable identifier the daemon can resolve back to an action target —
    /// for `ResultKind::App` this is the bundle path; for
    /// `ResultKind::Clipboard` this is the entry's own content (also the
    /// SQLite primary key, so it doubles as the dedup key).
    pub id: String,
    pub kind: ResultKind,
    pub title: String,
    pub subtitle: Option<String>,
    /// Absolute path to a cached PNG icon, if one was extracted.
    pub icon_path: Option<String>,
    /// `Some` only for `ResultKind::Clipboard` — the content-type tag the
    /// design shows next to a clipboard row (`LINK`, `TEXT`).
    pub content_kind: Option<ClipboardContentKind>,
    /// `Some` only for `ResultKind::Clipboard` — a precomputed relative
    /// timestamp ("12m", "3h") for the row's accessory text.
    pub accessory: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Request {
    Ping,
    Search { query: String, limit: usize },
    Launch { id: String },
    /// Writes a stored clipboard entry's content back onto the system
    /// pasteboard — "paste" as in "make this the current clipboard
    /// contents," not a synthesized ⌘V keystroke into the frontmost app.
    Paste { id: String },
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
    Launched,
    Pasted,
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
            },
        };
        let mut buf = Vec::new();
        write_frame(&mut buf, &frame).unwrap();
        let decoded = read_frame(&buf[..]).unwrap().unwrap();
        match decoded {
            Frame::Request { id, request: Request::Search { query, limit } } => {
                assert_eq!(id, 7);
                assert_eq!(query, "fin");
                assert_eq!(limit, 8);
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
