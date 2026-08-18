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
pub fn entries(db: &crate::Db) -> rusqlite::Result<Vec<ClipboardEntry>> {
    Ok(db
        .clipboard_entries()?
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
fn clipboard_recency_boost(copied_at_unix_ms: i64, now_unix_ms: i64) -> f32 {
    let age_ms = (now_unix_ms - copied_at_unix_ms).max(0) as f32;
    let age_hours = age_ms / (1000.0 * 60.0 * 60.0);
    let half_life_hours = 3.0;
    8.0 * 0.5f32.powf(age_hours / half_life_hours)
}

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
}
