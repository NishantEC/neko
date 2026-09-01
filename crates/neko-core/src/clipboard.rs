//! Clipboard capture: polls the macOS general pasteboard on a background
//! thread, persists distinct copies to SQLite, and can write a stored entry
//! back onto the pasteboard. This all runs inside the daemon — no GPUI run
//! loop needed. Unlike hotkey registration (`AGENTS.md`'s "why the hotkey is
//! registered where it is"), `NSPasteboard` is a plain Objective-C call with
//! no run-loop dependency, the same headless-background-thread shape
//! `icons.rs`'s `NSWorkspace::iconForFile` already proves works — so capture
//! keeps running whether or not a client window is open, which is the whole
//! point of it living in the resident daemon rather than the client.
//!
//! **Images are an explicit non-goal for this slice.** Only
//! `NSPasteboardTypeString` is read, so an image-only copy (a screenshot, a
//! dragged photo) is silently not recorded. The seam for a future pass: a
//! `ClipboardContentKind::Image` variant, a cached-PNG store keyed by a
//! content hash under `~/Library/Caches/neko/clipboard/` (mirroring
//! `icons.rs`'s own icon cache), and a row that renders the thumbnail via
//! `img()` instead of the type-tag badge. Not started here — no half-built
//! `Image` variant exists to avoid a dead code path with no reader.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use neko_protocol::{Glyph, Icon, SearchItem};

use crate::provider::{Provider, ProviderError};
use crate::search::{Candidate, fuzzy_score};

/// The clipboard content types this v1 distinguishes, used only internally
/// by this module for classification/storage — the wire protocol no longer
/// has a matching type (`neko_protocol::Glyph`/`badge` carry the
/// client-facing equivalent instead, set in `ClipboardProvider::search`
/// below). Images are an explicit non-goal for this slice — see this
/// module's doc comment for the seam a future task plugs an `Image`
/// variant into.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipboardContentKind {
    Text,
    Link,
}

/// Poll interval for detecting pasteboard changes via `NSPasteboard`'s
/// `changeCount`. Short enough that a copy shows up in history well within
/// a human's next keystroke; long enough not to spin a full core polling an
/// integer.
pub const POLL_INTERVAL: Duration = Duration::from_millis(400);

/// Bounds the `clipboard_entries` table so it can't grow without limit.
/// An entry-count cap rather than an age cap — it directly bounds storage
/// regardless of how bursty copying is, and 200 is a "recent history" list
/// a search field can page through, not an archive.
pub const HISTORY_LIMIT: usize = 200;

/// Pasteboard types that mean "do not record this" — the convention
/// documented at nspasteboard.org and honored by password managers
/// (1Password among them) to keep secrets out of clipboard-history tools.
/// Honoring these is not optional (see the launch brief's privacy
/// requirement).
const PRIVACY_MARKER_TYPES: &[&str] = &[
    "org.nspasteboard.ConcealedType",
    "org.nspasteboard.TransientType",
];

#[derive(Debug, Clone, PartialEq)]
pub struct ClipboardEntry {
    pub content: String,
    pub content_kind: ClipboardContentKind,
    pub source_app: Option<String>,
    pub copied_at_unix_ms: i64,
}

/// `true` if the pasteboard's advertised types include one of
/// [`PRIVACY_MARKER_TYPES`]. Pure and independent of any live pasteboard so
/// it can be unit-tested without touching AppKit — the macOS-only glue that
/// collects `NSPasteboard.types()` into a `Vec<String>` is the only
/// untested-by-necessity part (same shape as `icons.rs`'s AppKit calls).
pub fn is_privacy_marked(types: &[String]) -> bool {
    types
        .iter()
        .any(|t| PRIVACY_MARKER_TYPES.contains(&t.as_str()))
}

/// A conservative heuristic for "this looks like a URL, not prose": no
/// whitespace, and an explicit scheme. Used as a fallback when the
/// pasteboard didn't also advertise `NSPasteboardTypeURL` (some apps only
/// ever put plain text on the pasteboard for a copied link).
fn looks_like_url(s: &str) -> bool {
    let s = s.trim();
    if s.is_empty() || s.contains(char::is_whitespace) {
        return false;
    }
    const SCHEMES: &[&str] = &["http://", "https://", "ftp://", "mailto:", "file://"];
    SCHEMES.iter().any(|scheme| s.starts_with(scheme))
}

/// Classifies captured content as `Link` or `Text`. `has_url_type` is
/// authoritative when true (the source app told us directly, via
/// `NSPasteboardTypeURL`); otherwise falls back to [`looks_like_url`].
pub fn classify(content: &str, has_url_type: bool) -> ClipboardContentKind {
    if has_url_type || looks_like_url(content) {
        ClipboardContentKind::Link
    } else {
        ClipboardContentKind::Text
    }
}

/// A single-line row title: newlines collapsed to spaces (multi-line copies
/// are common — a paragraph, a code snippet — and `SearchItem::title` is
/// rendered as one truncated line), plain-text entries wrapped in quotes to
/// match the design's clipboard-row treatment (`"the difference is
/// ownership…"`), links left bare.
pub fn preview(content: &str, kind: ClipboardContentKind) -> String {
    let collapsed: String = content.split_whitespace().collect::<Vec<_>>().join(" ");
    match kind {
        ClipboardContentKind::Link => collapsed,
        ClipboardContentKind::Text => format!("\"{collapsed}\""),
    }
}

/// A short relative-time label ("now", "12m", "3h", "5d") for the row's
/// accessory text, matching the design's `row-accessory` treatment.
pub fn relative_time(now_unix_ms: i64, then_unix_ms: i64) -> String {
    let diff_secs = (now_unix_ms - then_unix_ms).max(0) / 1000;
    if diff_secs < 60 {
        "now".to_string()
    } else if diff_secs < 3600 {
        format!("{}m", diff_secs / 60)
    } else if diff_secs < 86_400 {
        format!("{}h", diff_secs / 3600)
    } else {
        format!("{}d", diff_secs / 86_400)
    }
}

fn content_kind_to_db(kind: ClipboardContentKind) -> &'static str {
    match kind {
        ClipboardContentKind::Text => "text",
        ClipboardContentKind::Link => "link",
    }
}

fn content_kind_from_db(s: &str) -> ClipboardContentKind {
    match s {
        "link" => ClipboardContentKind::Link,
        _ => ClipboardContentKind::Text,
    }
}

/// Persists one captured (or re-pasted) copy, deduplicating and pruning per
/// [`HISTORY_LIMIT`] — see `Db::record_clipboard_entry`.
pub fn record_entry(
    db: &crate::Db,
    content: &str,
    content_kind: ClipboardContentKind,
    source_app: Option<&str>,
    copied_at_unix_ms: i64,
) -> rusqlite::Result<()> {
    db.record_clipboard_entry(
        content,
        content_kind_to_db(content_kind),
        source_app,
        copied_at_unix_ms,
        HISTORY_LIMIT,
    )
}

/// All stored entries, most recently copied first.
/// How much of an entry is read for matching and rendering.
///
/// Generous next to what anything downstream uses — a row shows one line,
/// the detail pane a paragraph, and `CLIPBOARD_TITLE_LIKE_CHARS` (60) is
/// where a long entry's score starts collapsing anyway — and tiny next to
/// what was being read: a 17 MB entry, in full, on every keystroke. See
/// `Db::clipboard_entries`.
pub const MAX_MATCHED_BYTES: usize = 8 * 1024;

pub fn entries(db: &crate::Db) -> rusqlite::Result<Vec<ClipboardEntry>> {
    Ok(db
        .clipboard_entries(MAX_MATCHED_BYTES)?
        .into_iter()
        .map(|(content, kind, source_app, copied_at_unix_ms)| ClipboardEntry {
            content,
            content_kind: content_kind_from_db(&kind),
            source_app,
            copied_at_unix_ms,
        })
        .collect())
}

/// A launched-recently boost for clipboard entries, tapering much faster
/// than an app's (half-life of 3 hours, not 5 days) — a clipboard history is
/// inherently a "recent things" list, so a query with no other signal
/// (matches everything, or a tied fuzzy score) should surface the last few
/// copies first.
///
/// `pub(crate)`, not private: `search::allocate`'s section-ordering pass
/// needs this exact ceiling (see [`CLIPBOARD_RECENCY_BOOST_CEILING`]'s own
/// doc comment) to discount how much of a clipboard candidate's score is
/// "just copied" freshness rather than query relevance, when deciding which
/// *section* leads — sharing the one constant both places use keeps that
/// discount from silently drifting out of sync with this function's actual
/// maximum if the half-life or ceiling ever changes here.
fn clipboard_recency_boost(copied_at_unix_ms: i64, now_unix_ms: i64) -> f32 {
    let age_ms = (now_unix_ms - copied_at_unix_ms).max(0) as f32;
    let age_hours = age_ms / (1000.0 * 60.0 * 60.0);
    let half_life_hours = 3.0;
    CLIPBOARD_RECENCY_BOOST_CEILING * 0.5f32.powf(age_hours / half_life_hours)
}

/// The maximum value [`clipboard_recency_boost`] can return (at age zero) —
/// factored out to a named, shared constant specifically so `search::
/// allocate`'s section-ordering pass can reference the same number rather
/// than hard-coding a second `8.0` that could quietly stop matching this
/// function's own ceiling.
pub(crate) const CLIPBOARD_RECENCY_BOOST_CEILING: f32 = 8.0;

/// A clipboard entry at or under this length scores on the same footing
/// `fuzzy_score` already gives every other provider's title-length
/// candidates — a URL, a short snippet, a single line, is genuinely
/// title-like. [`clipboard_length_normalization`] leaves it untouched.
const CLIPBOARD_TITLE_LIKE_CHARS: usize = 60;

/// A clipboard entry is a pasted paragraph, not a title — `fuzzy_score`'s
/// own length penalty (a flat 2%-per-character deduction, sized for
/// title-length strings like app/pane names) barely dents a match found
/// once inside a multi-hundred-character block of text, so a long entry
/// that merely *contains* the query routinely outscored a short, exact
/// title match elsewhere (a real captain-reported defect: "wallpaper"
/// returned nine clipboard rows out of ten, crowding out the System
/// Settings pane actually named "Wallpaper" — see
/// `docs/evidence/settings-and-clipboard-ranking.md`). This scales a
/// candidate's raw match score down proportionally once its content passes
/// [`CLIPBOARD_TITLE_LIKE_CHARS`], so a 600-character paragraph's match
/// component is worth a tenth of a title-length one instead of ~98% of
/// it — applied only to the `fuzzy_score` component in
/// [`ClipboardProvider::search`], not to the recency boost added after it,
/// since how long ago something was copied is a genuine signal independent
/// of how long the copied text happens to be.
fn clipboard_length_normalization(content_chars: usize) -> f32 {
    if content_chars <= CLIPBOARD_TITLE_LIKE_CHARS {
        1.0
    } else {
        CLIPBOARD_TITLE_LIKE_CHARS as f32 / content_chars as f32
    }
}

/// A day-relative group label for clipboard history mode's own time-grouped
/// list (`data/neko-design/report.md` mockup 12: "Today" / "Yesterday"
/// section headers) — `SearchItem::group_label`, read only by the client's
/// mode rendering, never by the ordinary root-list view.
///
/// **UTC calendar-day arithmetic, not local-timezone-aware — a known,
/// disclosed approximation, not a hidden bug.** No date/time-formatting
/// dependency exists anywhere in this codebase yet (`clipboard::relative_time`,
/// the only other time-label this module produces, is pure duration
/// arithmetic, not a calendar), and adding one (`chrono` or equivalent) for
/// two section-header labels wasn't judged worth a new dependency. A copy
/// made shortly before or after local midnight can land in the "wrong" UTC
/// day bucket relative to the captain's own wall clock — say so rather than
/// claim more.
fn day_bucket_label(now_unix_ms: i64, copied_at_unix_ms: i64) -> String {
    const DAY_MS: i64 = 86_400_000;
    let now_day = now_unix_ms.div_euclid(DAY_MS);
    let entry_day = copied_at_unix_ms.div_euclid(DAY_MS);
    match now_day - entry_day {
        n if n <= 0 => "Today".to_string(),
        1 => "Yesterday".to_string(),
        n => format!("{n} days ago"),
    }
}

/// The clipboard actions menu's three secondary actions, offered on every
/// clipboard row — see `AGENTS.md`'s "Commands and modes" section. "Paste"
/// duplicates the row's own primary `Request::Activate` action (matching
/// Raycast's own action-panel convention of listing the default action
/// first); neko's "paste" and "copy" are the identical operation today
/// (write to the system pasteboard — see `AGENTS.md`'s "Clipboard history"
/// section on why neko never simulates a keystroke), offered as two labels
/// because a captain reaching for "Copy" shouldn't have to know that.
fn clipboard_item_actions() -> Vec<neko_protocol::ItemAction> {
    vec![
        neko_protocol::ItemAction { id: "paste".to_string(), label: "Paste".to_string(), destructive: false },
        neko_protocol::ItemAction { id: "copy".to_string(), label: "Copy".to_string(), destructive: false },
        neko_protocol::ItemAction { id: "delete".to_string(), label: "Delete".to_string(), destructive: true },
    ]
}

/// The clipboard-history provider: matches by fuzzy-scoring each stored
/// entry's own content, boosted by how recently it was copied. Rows never
/// carry a per-entry icon (no favicon/thumbnail fetching in this slice —
/// unchanged from before this task) — a painted content-type glyph fills
/// the icon slot instead, chosen here rather than by the client, per this
/// task's "every rendering decision comes from the provider" rule.
pub struct ClipboardProvider {
    db: Arc<Mutex<crate::Db>>,
}

impl ClipboardProvider {
    pub fn new(db: Arc<Mutex<crate::Db>>) -> Self {
        Self { db }
    }
}

impl Provider for ClipboardProvider {
    fn id(&self) -> &'static str {
        "clipboard"
    }

    fn section_label(&self) -> &'static str {
        "Clipboard"
    }

    /// **An empty root query returns no clipboard entries.** Unlike apps —
    /// where the top few are a useful thing to be shown unprompted — the most
    /// recent thing copied is very often the most private: a password
    /// manager's payload is already filtered
    /// ([`is_privacy_marked`]), but an invoice, a client email or a chunk of
    /// somebody's source is not, and rendering it the instant the panel opens
    /// puts it on screen in front of whoever is standing there. Typing is the
    /// signal that it was actually wanted.
    ///
    /// The `Clipboard History` mode is unaffected: it scopes to this provider
    /// explicitly, and a scoped search never passes through the root-list
    /// guard.
    fn answers_empty_root_query(&self) -> bool {
        false
    }

    fn search(&self, query: &str, now_unix_ms: i64) -> Vec<Candidate> {
        let stored = {
            let db = self.db.lock().unwrap();
            entries(&db).unwrap_or_default()
        };
        stored
            .iter()
            .filter_map(|entry| {
                let raw_score = fuzzy_score(query, &entry.content)?;
                let mut score = raw_score * clipboard_length_normalization(entry.content.chars().count());
                score += clipboard_recency_boost(entry.copied_at_unix_ms, now_unix_ms);
                let (badge, glyph) = match entry.content_kind {
                    ClipboardContentKind::Text => ("TEXT", Glyph::Text),
                    ClipboardContentKind::Link => ("LINK", Glyph::Link),
                };
                Some(Candidate {
                    score,
                    item: SearchItem {
                        id: entry.content.clone(),
                        kind: "clipboard".to_string(),
                        title: preview(&entry.content, entry.content_kind),
                        subtitle: entry.source_app.as_ref().map(|app| format!("Copied from {app}")),
                        icon: Icon::Glyph(glyph),
                        section_label: "Clipboard".to_string(),
                        action_label: "Paste  ↵".to_string(),
                        badge: Some(badge.to_string()),
                        accessory: Some(relative_time(now_unix_ms, entry.copied_at_unix_ms)),
                        enters_mode: None,
                        group_label: Some(day_bucket_label(now_unix_ms, entry.copied_at_unix_ms)),
                        actions: clipboard_item_actions(),
                        source: entry.source_app.clone(),
                        meter: None,
                        keeps_open: false,
                        preview_markdown: false,
                        speaker: None,
                        images: Vec::new(),
                        preview: None,
                    },
                })
            })
            .collect()
    }

    fn activate(&self, id: &str) -> Result<(), ProviderError> {
        if write_to_pasteboard(id) {
            Ok(())
        } else {
            Err(ProviderError("failed to write to the pasteboard".to_string()))
        }
    }

    /// "paste"/"copy" are the identical operation for neko today (see
    /// [`clipboard_item_actions`]'s doc comment) — both just re-run
    /// [`activate`](Self::activate). "delete" permanently removes the entry
    /// from history; deleting something already gone (a double-delete, or a
    /// stale action-menu reference to an entry that pruned out in the
    /// meantime) is treated as success, not an error — the end state the
    /// caller wanted ("this content is not in history") already holds.
    fn perform_action(&self, id: &str, action_id: &str) -> Result<(), ProviderError> {
        match action_id {
            "paste" | "copy" => self.activate(id),
            "delete" => {
                let db = self.db.lock().unwrap();
                db.delete_clipboard_entry(id).map_err(|e| ProviderError(e.to_string()))
            }
            other => Err(ProviderError(format!("no action '{other}' on this row"))),
        }
    }
}

#[cfg(target_os = "macos")]
mod pasteboard {
    //! The only AppKit-touching code in this module. `NSPasteboard`, like
    //! `NSWorkspace::iconForFile` in `icons.rs`, needs no active run loop —
    //! plain Objective-C calls, safe to make from a headless background
    //! thread.
    //!
    //! **On "detect and degrade honestly" for permissions**: unlike
    //! Accessibility (`AXIsProcessTrusted()`), macOS exposes no distinct
    //! "pasteboard access denied" signal separate from "no matching data" —
    //! a read that's blocked and a read that's simply empty both come back
    //! as `nil`/`None`. There is nothing more specific to detect, so
    //! treating `None` as "nothing to capture this tick" (never panicking,
    //! never blocking) *is* the honest degrade path here, not a shortcut
    //! around one.
    //!
    //! **Every AppKit call in this module runs inside an autorelease
    //! pool.** This thread has no `NSApplication`/`CFRunLoop` of its own —
    //! a normal Cocoa event loop drains an autorelease pool once per event
    //! automatically, but a plain background thread never does, so any
    //! `-autorelease`d object produced here (and `NSPasteboard`/
    //! `NSWorkspace`/`CFRunLoop::run_in_mode` all produce plenty of
    //! transient ones internally, not just the values this module returns)
    //! leaks for the rest of the process's life. Confirmed live and fixed
    //! in this pass — see `AGENTS.md`'s "Clipboard capture memory" section
    //! for the measured before/after. [`poll`] and [`write_string`] are
    //! the only two public entry points into AppKit this module has, and
    //! both wrap their entire body in [`objc2::rc::autoreleasepool`] —
    //! everything below them (`raw_*`) is a private implementation detail
    //! that cannot be called unpooled from outside this module.

    use objc2_app_kit::{NSPasteboard, NSPasteboardTypeString, NSPasteboardTypeURL, NSWorkspace};
    use objc2_core_foundation::{CFRunLoop, kCFRunLoopDefaultMode};
    use objc2_foundation::NSString;

    /// One poll tick's result, converted to owned Rust data before
    /// [`poll`] returns — nothing objc2-managed survives past the
    /// autorelease pool that produced it.
    pub struct Tick {
        pub change_count: i64,
        pub content: Option<(String, bool)>,
        pub source_app: Option<String>,
    }

    /// The pasteboard's current `changeCount`, pooled on its own — used
    /// only once, to seed [`super::run_capture_loop`]'s starting point
    /// before the hot loop begins.
    pub fn current_change_count() -> i64 {
        objc2::rc::autoreleasepool(|_pool| raw_change_count())
    }

    /// One capture tick, entirely inside a single autorelease pool: the
    /// `changeCount` check, and — only if it moved since
    /// `last_change_count` — the pasteboard read and the frontmost-app
    /// lookup (which pumps a run loop; see `raw_frontmost_app_name`).
    /// Collapsing all three into one pooled call, rather than three public
    /// functions each trusting its caller to wrap it, is deliberate: there
    /// is no unpooled path left into this module's AppKit calls for a
    /// future change to fall into by accident.
    pub fn poll(last_change_count: i64) -> Tick {
        objc2::rc::autoreleasepool(|_pool| {
            let change_count = raw_change_count();
            if change_count == last_change_count {
                return Tick {
                    change_count,
                    content: None,
                    source_app: None,
                };
            }
            Tick {
                change_count,
                content: raw_read_current(),
                source_app: raw_frontmost_app_name(),
            }
        })
    }

    /// The pasteboard's current `changeCount` — bumps on every distinct
    /// change, the standard way to poll a `NSPasteboard` without missing or
    /// double-processing a copy.
    fn raw_change_count() -> i64 {
        NSPasteboard::generalPasteboard().changeCount() as i64
    }

    /// Reads the current pasteboard's plain-text string content, unless
    /// it's marked private (see [`super::is_privacy_marked`]) or has no
    /// string representation at all (e.g. an image-only copy — see this
    /// module's file-level doc comment). Returns `(content, has_url_type)`.
    fn raw_read_current() -> Option<(String, bool)> {
        // SAFETY: objc2-app-kit's generated `NSPasteboardTypeString`/
        // `NSPasteboardTypeURL` are plain (non-`safe`) `extern "C"` statics,
        // so reading them needs `unsafe` even under edition 2024's
        // safe-extern-static rules (only items an `unsafe extern` block
        // explicitly opts in via `safe` get that treatment). Reading them
        // is just reading a linker-provided constant pointer — no actual
        // invariant to uphold beyond "the framework is loaded," which it is
        // by definition in an AppKit-linked binary.
        unsafe {
            let pb = NSPasteboard::generalPasteboard();
            let types = pb.types();
            let type_strings: Vec<String> = types
                .as_ref()
                .map(|types| types.iter().map(|t| t.to_string()).collect())
                .unwrap_or_default();
            if super::is_privacy_marked(&type_strings) {
                return None;
            }
            let has_url_type = types
                .as_ref()
                .is_some_and(|types| types.containsObject(NSPasteboardTypeURL));
            let string = pb.stringForType(NSPasteboardTypeString)?;
            Some((string.to_string(), has_url_type))
        }
    }

    /// The frontmost application's display name at the moment of capture —
    /// an approximation of "who copied this," the same one every polling
    /// clipboard manager uses, since macOS attaches no source-app metadata
    /// to the general pasteboard itself. Because polling has up to
    /// [`super::POLL_INTERVAL`] of latency, a very fast app-switch between
    /// the copy and the next poll tick can attribute to the wrong app; not
    /// solvable without a push notification macOS doesn't offer for this.
    ///
    /// **This is the shipped clipboard-source-attribution feature — do not
    /// remove the run-loop pump below to make the leak this module used to
    /// have go away.** The autorelease pool in [`poll`] is the actual fix;
    /// this function's behavior is unchanged.
    fn raw_frontmost_app_name() -> Option<String> {
        // `NSWorkspace`'s `frontmostApplication` is kept current by
        // distributed notifications the process only dequeues while its
        // run loop spins — a plain background thread (this one; the daemon
        // has no `NSApplication`/`CFRunLoop` anywhere) never pumps one, so
        // without this the value freezes at whatever was frontmost when the
        // process — or this thread's first call — started, confirmed with
        // a standalone repro: 16 reads over 8s, switching the real
        // frontmost app four times, returned the *first* app's name for
        // every single read. A brief run-loop pump before reading is the
        // standard fix and reproducibly tracks live changes in the same
        // repro.
        //
        // SAFETY: reading `kCFRunLoopDefaultMode`, a plain (non-`safe`)
        // `extern "C"` static — see `raw_read_current`'s SAFETY comment.
        let mode = unsafe { kCFRunLoopDefaultMode };
        CFRunLoop::run_in_mode(mode, 0.05, true);
        let app = NSWorkspace::sharedWorkspace().frontmostApplication()?;
        app.localizedName().map(|s| s.to_string())
    }

    /// Writes `content` as the pasteboard's sole contents — "paste" in the
    /// sense of "make this the system clipboard again," not a synthesized
    /// keystroke. Returns whether the write succeeded. Pooled for the same
    /// reason [`poll`] is, even though this is called far less often (once
    /// per `ClipboardProvider::activate`, not on a timer).
    pub fn write_string(content: &str) -> bool {
        objc2::rc::autoreleasepool(|_pool| {
            // SAFETY: same as `raw_read_current` — reading `NSPasteboardTypeString`.
            unsafe {
                let pb = NSPasteboard::generalPasteboard();
                pb.clearContents();
                let ns = NSString::from_str(content);
                pb.setString_forType(&ns, NSPasteboardTypeString)
            }
        })
    }
}

#[cfg(not(target_os = "macos"))]
mod pasteboard {
    pub struct Tick {
        pub change_count: i64,
        pub content: Option<(String, bool)>,
        pub source_app: Option<String>,
    }

    pub fn current_change_count() -> i64 {
        0
    }

    pub fn poll(last_change_count: i64) -> Tick {
        Tick {
            change_count: last_change_count,
            content: None,
            source_app: None,
        }
    }

    pub fn write_string(_content: &str) -> bool {
        false
    }
}

/// One capture attempt: if the pasteboard changed since `last_change_count`,
/// classify and persist it (unless privacy-marked or empty of string
/// content). Split out from [`run_capture_loop`] so the polling shape
/// itself needs no macOS-only code — only `pasteboard::*` does. The
/// autorelease-pool guarantee lives entirely inside `pasteboard::poll` (see
/// its doc comment) — this function only ever sees owned Rust data.
fn poll_once(db: &crate::Db, last_change_count: &mut i64) {
    let tick = pasteboard::poll(*last_change_count);
    *last_change_count = tick.change_count;

    let Some((content, has_url_type)) = tick.content else {
        return;
    };
    if content.trim().is_empty() {
        return;
    }
    let kind = classify(&content, has_url_type);
    if let Err(e) = record_entry(db, &content, kind, tick.source_app.as_deref(), crate::now_unix_ms()) {
        eprintln!("neko-daemon: failed to record clipboard entry: {e}");
    }
}

/// Runs forever, polling the pasteboard on [`POLL_INTERVAL`]. Intended to be
/// the body of its own background thread (see `neko-daemon`'s `main.rs`),
/// the same shape as `icons.rs`'s icon-extraction thread — started once,
/// outlives every individual client connection.
pub fn run_capture_loop(db: &std::sync::Mutex<crate::Db>) {
    let mut last_change_count = pasteboard::current_change_count();
    loop {
        std::thread::sleep(POLL_INTERVAL);
        let db = db.lock().unwrap();
        poll_once(&db, &mut last_change_count);
    }
}

/// Writes a stored entry's content back onto the system pasteboard. `false`
/// off macOS or if the write failed.
pub fn write_to_pasteboard(content: &str) -> bool {
    pasteboard::write_string(content)
}

#[cfg(test)]
mod empty_query_tests {
    use super::*;
    use crate::provider::Provider;

    #[test]
    fn the_clipboard_never_answers_an_empty_root_query() {
        // The most recent thing copied is very often the most private, and
        // an empty query means nobody asked for it yet.
        let db = std::sync::Arc::new(std::sync::Mutex::new(crate::Db::open_in_memory().unwrap()));
        assert!(!ClipboardProvider::new(db).answers_empty_root_query());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bare_url_with_a_scheme_looks_like_a_url() {
        assert!(looks_like_url("https://example.com/path"));
        assert!(looks_like_url("mailto:hi@example.com"));
    }

    #[test]
    fn prose_does_not_look_like_a_url() {
        assert!(!looks_like_url("the difference is ownership, not a longer name"));
        assert!(!looks_like_url("check out https://example.com for details"));
    }

    #[test]
    fn classify_trusts_an_explicit_url_type_even_without_a_recognized_scheme() {
        assert_eq!(classify("myapp://open", true), ClipboardContentKind::Link);
    }

    #[test]
    fn classify_falls_back_to_the_heuristic_without_a_url_type() {
        assert_eq!(classify("https://example.com", false), ClipboardContentKind::Link);
        assert_eq!(classify("just some text", false), ClipboardContentKind::Text);
    }

    #[test]
    fn concealed_and_transient_markers_are_privacy_marked() {
        assert!(is_privacy_marked(&["org.nspasteboard.ConcealedType".to_string()]));
        assert!(is_privacy_marked(&["org.nspasteboard.TransientType".to_string()]));
        assert!(is_privacy_marked(&[
            "public.utf8-plain-text".to_string(),
            "org.nspasteboard.ConcealedType".to_string(),
        ]));
    }

    #[test]
    fn ordinary_text_types_are_not_privacy_marked() {
        assert!(!is_privacy_marked(&["public.utf8-plain-text".to_string()]));
        assert!(!is_privacy_marked(&[]));
    }

    #[test]
    fn text_preview_collapses_whitespace_and_quotes() {
        assert_eq!(
            preview("line one\nline two", ClipboardContentKind::Text),
            "\"line one line two\""
        );
    }

    #[test]
    fn link_preview_stays_bare() {
        assert_eq!(
            preview("https://example.com/path", ClipboardContentKind::Link),
            "https://example.com/path"
        );
    }

    #[test]
    fn relative_time_buckets_by_magnitude() {
        let now = 1_000_000_000;
        assert_eq!(relative_time(now, now - 30_000), "now");
        assert_eq!(relative_time(now, now - 5 * 60_000), "5m");
        assert_eq!(relative_time(now, now - 3 * 3_600_000), "3h");
        assert_eq!(relative_time(now, now - 2 * 86_400_000), "2d");
    }

    #[test]
    fn record_entry_and_read_back_round_trips_through_db() {
        let db = crate::Db::open_in_memory().unwrap();
        record_entry(&db, "hello", ClipboardContentKind::Text, Some("Terminal"), 100).unwrap();
        let entries = entries(&db).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].content, "hello");
        assert_eq!(entries[0].content_kind, ClipboardContentKind::Text);
        assert_eq!(entries[0].source_app.as_deref(), Some("Terminal"));
        assert_eq!(entries[0].copied_at_unix_ms, 100);
    }

    #[test]
    fn provider_search_filters_by_content_and_carries_the_badge_and_verb() {
        let db = crate::Db::open_in_memory().unwrap();
        record_entry(&db, "hello world", ClipboardContentKind::Text, Some("Terminal"), 100).unwrap();
        record_entry(&db, "goodbye", ClipboardContentKind::Text, None, 200).unwrap();
        let provider = ClipboardProvider::new(Arc::new(Mutex::new(db)));

        let results = provider.search("hello", 1000);
        assert_eq!(results.len(), 1);
        let item = &results[0].item;
        assert_eq!(item.id, "hello world");
        assert_eq!(item.kind, "clipboard");
        assert_eq!(item.section_label, "Clipboard");
        assert_eq!(item.action_label, "Paste  ↵");
        assert_eq!(item.badge.as_deref(), Some("TEXT"));
        assert_eq!(item.subtitle.as_deref(), Some("Copied from Terminal"));
        assert!(item.accessory.is_some());
        assert_eq!(item.icon, Icon::Glyph(Glyph::Text));
    }

    #[test]
    fn provider_search_favors_recency_on_an_empty_query() {
        let db = crate::Db::open_in_memory().unwrap();
        record_entry(&db, "older", ClipboardContentKind::Text, None, 100).unwrap();
        record_entry(&db, "newer", ClipboardContentKind::Text, None, 900).unwrap();
        let provider = ClipboardProvider::new(Arc::new(Mutex::new(db)));
        let results = provider.search("", 1000);
        let best = results.iter().max_by(|a, b| a.score.total_cmp(&b.score)).unwrap();
        assert_eq!(best.item.id, "newer");
    }

    #[test]
    fn provider_search_tags_a_link_entry_with_the_link_glyph_and_badge() {
        let db = crate::Db::open_in_memory().unwrap();
        record_entry(&db, "https://example.com", ClipboardContentKind::Link, None, 100).unwrap();
        let provider = ClipboardProvider::new(Arc::new(Mutex::new(db)));
        let results = provider.search("example", 1000);
        assert_eq!(results[0].item.badge.as_deref(), Some("LINK"));
        assert_eq!(results[0].item.icon, Icon::Glyph(Glyph::Link));
    }

    // --- Defect 2: a long, paragraph-shaped clipboard entry that merely
    // contains the query must not systematically outscore a short,
    // title-shaped candidate the way it did before length normalization. ---

    #[test]
    fn short_entries_are_left_exactly_as_fuzzy_score_scored_them() {
        assert_eq!(clipboard_length_normalization(10), 1.0);
        assert_eq!(clipboard_length_normalization(CLIPBOARD_TITLE_LIKE_CHARS), 1.0);
    }

    #[test]
    fn long_entries_are_scaled_down_proportionally_to_their_length() {
        let factor = clipboard_length_normalization(CLIPBOARD_TITLE_LIKE_CHARS * 10);
        assert!((factor - 0.1).abs() < 0.001, "a 10x-over-threshold entry should score at ~10% of its raw match");
    }

    #[test]
    fn a_short_exact_match_outscores_a_long_paragraph_that_merely_contains_the_query_at_the_same_age() {
        let db = crate::Db::open_in_memory().unwrap();
        let paragraph = format!(
            "{}wallpaper{}",
            "filler text ".repeat(20),
            " more unrelated filler content padding this out well past the title-like length threshold".repeat(2)
        );
        assert!(paragraph.chars().count() > CLIPBOARD_TITLE_LIKE_CHARS * 3, "fixture paragraph must be clearly long");
        record_entry(&db, &paragraph, ClipboardContentKind::Text, None, 1000).unwrap();
        record_entry(&db, "wallpaper", ClipboardContentKind::Text, None, 1000).unwrap();
        let provider = ClipboardProvider::new(Arc::new(Mutex::new(db)));
        let results = provider.search("wallpaper", 1000);
        let short = results.iter().find(|c| c.item.id == "wallpaper").unwrap();
        let long = results.iter().find(|c| c.item.id == paragraph).unwrap();
        assert!(short.score > long.score, "short exact match ({}) should beat the long paragraph ({})", short.score, long.score);
    }

    // --- Commands and modes: group_label, actions, perform_action, source ---

    #[test]
    fn day_bucket_labels_today_yesterday_and_older_by_utc_calendar_day() {
        const DAY_MS: i64 = 86_400_000;
        let now = 10 * DAY_MS + 12345; // any time on "day 10"
        assert_eq!(day_bucket_label(now, now), "Today");
        assert_eq!(day_bucket_label(now, now - DAY_MS), "Yesterday");
        assert_eq!(day_bucket_label(now, now - 3 * DAY_MS), "3 days ago");
    }

    #[test]
    fn day_bucket_treats_a_future_timestamp_as_today_rather_than_panicking_or_going_negative() {
        // Clock skew guard: a copy timestamped fractionally after `now`
        // (client/daemon clock drift) must not read as some nonsensical
        // "-1 days ago".
        let now = 5_000_000_000;
        assert_eq!(day_bucket_label(now, now + 60_000), "Today");
    }

    #[test]
    fn search_results_carry_a_group_label_and_the_standard_action_set() {
        let db = crate::Db::open_in_memory().unwrap();
        record_entry(&db, "hello", ClipboardContentKind::Text, Some("Terminal"), 1000).unwrap();
        let provider = ClipboardProvider::new(Arc::new(Mutex::new(db)));
        let results = provider.search("hello", 1000);
        let item = &results[0].item;
        assert_eq!(item.group_label.as_deref(), Some("Today"));
        assert_eq!(item.source.as_deref(), Some("Terminal"));
        let action_ids: Vec<&str> = item.actions.iter().map(|a| a.id.as_str()).collect();
        assert_eq!(action_ids, vec!["paste", "copy", "delete"]);
        assert!(item.actions.iter().find(|a| a.id == "delete").unwrap().destructive);
        assert!(!item.actions.iter().find(|a| a.id == "paste").unwrap().destructive);
        assert!(item.enters_mode.is_none());
    }

    #[test]
    fn perform_action_delete_removes_the_entry_from_history() {
        let db = Arc::new(Mutex::new(crate::Db::open_in_memory().unwrap()));
        record_entry(&db.lock().unwrap(), "gone soon", ClipboardContentKind::Text, None, 100).unwrap();
        let provider = ClipboardProvider::new(db.clone());
        assert!(provider.perform_action("gone soon", "delete").is_ok());
        assert!(entries(&db.lock().unwrap()).unwrap().is_empty());
    }

    // Deliberately no test calls `perform_action` with `"copy"`/`"paste"`
    // (or `activate` directly) against a real `ClipboardProvider`: both
    // route to the real `pasteboard::write_string` on macOS, which is a
    // live, unconditional write to *the system pasteboard* — running that
    // in a unit test on the captain's own machine would overwrite whatever
    // he actually has copied with test fixture text, exactly the kind of
    // real-data interference this task's own brief warns against (just in
    // the opposite direction — this codebase writing *to* his pasteboard,
    // not reading *from* his clipboard history). The routing itself
    // ("copy" and "paste" both call `self.activate`) is a one-line, visibly
    // correct fact in `perform_action`'s own body above, not something that
    // needs a live-AppKit test to confirm — consistent with the rest of
    // this module, which has never had a test that calls `activate()` on a
    // real `ClipboardProvider` either.

    #[test]
    fn perform_action_with_an_unknown_action_id_errors() {
        let db = crate::Db::open_in_memory().unwrap();
        record_entry(&db, "x", ClipboardContentKind::Text, None, 100).unwrap();
        let provider = ClipboardProvider::new(Arc::new(Mutex::new(db)));
        let result = provider.perform_action("x", "reverse-it");
        assert!(result.is_err());
        assert!(result.unwrap_err().0.contains("no action"));
    }

    #[test]
    fn deleting_an_entry_that_is_already_gone_is_not_an_error() {
        let db = crate::Db::open_in_memory().unwrap();
        let provider = ClipboardProvider::new(Arc::new(Mutex::new(db)));
        assert!(provider.perform_action("never existed", "delete").is_ok());
    }
}
