## Clipboard history

`neko_core::clipboard` — capture, storage, and restore all live in the
daemon, nothing in the client (`neko`) crate touches AppKit for this at all;
it's just another `Request`/`Response` pair, per the search section above.

- **Capture is a background thread the daemon spawns unconditionally**
  (`run_capture_loop`, started in `neko-daemon/src/main.rs` next to the
  icon-extraction thread), polling `NSPasteboard.generalPasteboard`'s
  `changeCount` every 400ms (`clipboard::POLL_INTERVAL`). Like
  `icons.rs`'s `NSWorkspace::iconForFile`, `NSPasteboard` needs no run loop
  for the read/write calls themselves — see the frontmost-app caveat below
  for the one place in this module that *does* need one.
- **Only `NSPasteboardTypeString` is read** — plain text and, heuristically
  or via `NSPasteboardTypeURL`, links. Images are an explicit non-goal;
  `clipboard.rs`'s module doc comment records the seam (an `Image`
  `ClipboardContentKind`, a PNG cache under `~/Library/Caches/neko/clipboard/`
  mirroring `icons.rs`, no half-built variant sitting unused in the meantime).
- **Privacy**: pasteboard content flagged `org.nspasteboard.ConcealedType` or
  `org.nspasteboard.TransientType` (the convention 1Password and other
  password managers use) is never recorded — `clipboard::is_privacy_marked`,
  pure and unit-tested, plus verified live against the real pasteboard with a
  JXA script setting those exact marker types (no password manager was
  installed on the verification machine to test against directly; say so
  rather than claim more than was checked).
- **Dedup and bound**: `content` is the SQLite primary key
  (`clipboard_entries` table), so a repeat copy is an `ON CONFLICT DO UPDATE`
  that moves it to the top rather than a new row. Bounded to
  `clipboard::HISTORY_LIMIT` (200) by count, not age — pruned after every
  insert. Both behaviors are covered in `db.rs`'s tests.
- **"Paste" writes the pasteboard, it does not simulate ⌘V.**
  `ClipboardProvider::activate` (routed there from `Request::Activate {
  kind: "clipboard", id }` — `id` is the entry's own content; see "Provider
  abstraction" above for why this is a generic request rather than a
  clipboard-specific one) calls `clipboard::write_to_pasteboard` and nothing
  else — the frontmost app after neko hides still needs a real ⌘V from the
  user. Decided this way (not
  answered by the plan or design report) because a synthetic keystroke would
  need the same accessibility-permission machinery the brief explicitly
  scoped out ("do not build onboarding or permission-request UI"), and
  "puts it back on the pasteboard" is the brief's own literal wording.
  Bonus: because writing back is a real pasteboard change, the capture loop
  notices it on its own next tick and re-records it — a paste is
  indistinguishable from an ordinary re-copy, including the recency bump.
- **Source-app attribution is best-effort, not a guarantee**, in two
  independent ways: (1) it's whatever `NSWorkspace.frontmostApplication`
  says at capture time, which can be wrong if the real copying app and the
  poll tick that notices it race (bounded by `POLL_INTERVAL`); (2) **a
  process without a spinning run loop gets a stale, frozen answer from
  `frontmostApplication`, not a live one** — confirmed with a standalone
  repro (a plain background-thread binary, no `NSApplication`/`CFRunLoop`
  anywhere, read `frontmostApplication` 16 times over 8 seconds while the
  real frontmost app was switched four times via `osascript`; every read
  returned the *first* app's name). The fix, in `clipboard::pasteboard::
  frontmost_app_name`, is a `CFRunLoop::run_in_mode(kCFRunLoopDefaultMode,
  0.05, true)` pump immediately before reading the property — confirmed live
  in the daemon binary afterward (`docs/evidence/summon-panel-clipboard-
  results.png`'s "Copied from Arc" row). **Any future code in this daemon
  that reads live AppKit/Workspace state from a background thread should
  expect the same staleness and pump a run loop first** — this is not
  specific to clipboard, it's a property of any notification-backed
  `NSWorkspace`/`NSRunningApplication` query made off a real run loop.
- **Row icon is a painted content-type glyph, not a per-entry icon.**
  Clipboard entries have no favicon/thumbnail fetching in this slice, so the
  row-icon slot would otherwise render the same empty placeholder square
  `render_row` already uses for an app with no cached icon yet — fine as a
  *transient* state for apps (self-heals once the icon-extraction pass
  catches up), wrong as a *permanent* one for clipboard, where nothing will
  ever fill it in. `panel::glyph_element` fills the slot instead: a
  three-bar mark for `Glyph::Text`, two overlapping rounded-square rings for
  `Glyph::Link` — hand-painted from plain `div`s, the same pattern
  `search_glyph` already established (no bundled SVG-asset pipeline existed
  in this codebase then, and per the design report's §6 finding, a Unicode
  symbol isn't a reliable substitute either). **Both are vendored Lucide
  SVGs now** — `fm/neko-icons` built the asset pipeline whose absence this
  sentence gives as the reason; the design report's §6 finding still
  stands, since an SVG asset is not a font glyph. See "Icons: real SVGs,
  and the one that stays painted" below. See "Provider abstraction"
  below for `Icon`/`Glyph`, the same rendering vocabulary every other
  provider's rows draw from now, and `glyph_element`'s own `File`/`Folder`
  cases the file-search provider added.
- **No two-column detail pane.** The design's screen 12 (`12-first-clipboard-
  use.html`) is a dedicated, wider (760px, `theme::PANEL_WIDTH_WITH_DETAIL_PX`)
  clipboard-only mode with a preview pane. This task renders clipboard rows
  inline in the one shared 680px list instead (screen 11's pattern: "one
  query, two result types, same list" — apps and clipboard each under their
  own section header, row-level type tag / source subtitle / relative-time
  accessory matching screen 12's own row treatment). The two-column mode
  stays a follow-up; `PANEL_WIDTH_WITH_DETAIL_PX` is still unused, still
  reserved.
