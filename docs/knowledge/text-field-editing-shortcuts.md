## Text field editing shortcuts

`neko-p0-fixes`'s fourth fix. `TextField` (`crates/neko/src/text_field.rs`)
had only `Backspace`/`Left`/`Right` before this task; it now also binds (in
`main.rs`'s `cx.bind_keys`) the standard macOS line/word editing set:
`cmd-backspace`/`cmd-delete` (delete to line start/end), `alt-backspace`/
`alt-delete` (delete previous/next word), `cmd-left`/`cmd-right` (jump to
line start/end), `alt-left`/`alt-right` (jump to previous/next word start).
Word boundaries go through `unicode_segmentation::UnicodeWordIndices`
(`TextField::word_start_before`/`word_end_after`), not a byte-level
`char::is_whitespace` scan — verified against `"café 東京 test"`: each CJK
ideograph is its own word-boundary stop (UAX#29 has no dictionary-based
segmentation), accented Latin stays within one word. Already a transitive
dependency via `global-hotkey -> keyboard-types`, so this added zero new
crate versions to the tree. One unit test per shortcut plus a dedicated
Unicode-awareness test, all against the editing model directly (no `Window`
needed, consistent with this file's existing test convention).

**Selection and clipboard, closing the seam this section used to describe —
`fm/neko-textinput`.** `TextField` now carries `selection_anchor:
Option<usize>`; `cursor` is always the moving end, `selection_anchor` the
fixed one. `selection_range()` normalizes the two into a `start..end` byte
range, `None` whenever there's no anchor or the two coincide (an empty
selection is the same as no selection). Bound in `main.rs`'s `cx.bind_keys`:
`shift-left`/`shift-right` (`SelectLeft`/`SelectRight`, one char at a time),
`shift-alt-left`/`shift-alt-right` (by word, reusing `word_start_before`/
`word_end_after`), `shift-cmd-left`/`shift-cmd-right` (to line start/end),
`cmd-a` (`SelectAll`, always re-anchors at the true start rather than
extending from wherever the cursor sits), `cmd-c`/`cmd-x`/`cmd-v`
(`crates/neko/src/pasteboard.rs`, the real `NSPasteboard`, every AppKit call
autoreleasepool-wrapped — same discipline as `neko_core::clipboard::
pasteboard` and `material.rs`, since an unpooled pasteboard call site is
exactly the defect class that took the daemon's own capture loop to 14+ GB
resident, see "Clipboard capture memory" above). A non-shift movement key
clears the selection (`selection_anchor = None`); any real edit clears it
too, inside `commit_edit`, the one chokepoint every content mutation goes
through. Typing a character or invoking any delete shortcut with a
selection active replaces/deletes exactly the selection first
(`edit_target_range()`), never falling through to its own direction-specific
range. The rendered highlight (`TextFieldElement::prepaint`/`paint`) reuses
`theme::SURFACE_SELECTED` — the same token `panel.rs`'s row-selection
highlight already uses — painted behind the shaped text so selected
characters stay legible, deliberately no new palette token. `⌘K` is bound
only at the `"Panel"` key context (`main.rs`), never `"TextField"`, so it
was never at risk of being shadowed by anything this task added.

**Every AppKit pasteboard test hits the same real, systemwide
`NSPasteboard`, with no per-test isolation — this raced under `cargo test`'s
default multithreading and had to be fixed with a lock, not skipped.**
`text_field::tests::pasteboard_test_lock()` is a single `static
Mutex<()>` every copy/cut/paste test holds for its whole body, so two such
tests can never interleave their real pasteboard writes/reads — confirmed
live: without it, 5 of the file's pasteboard tests failed intermittently
(wrong fixture string read back) whenever `cargo test --workspace` happened
to schedule more than one of them concurrently; with it, dozens of repeat
runs were clean. **The general lesson**: any test suite that touches a
real, unpartitioned OS-level shared resource (the systemwide pasteboard
here; a fixed socket path is `neko-client`'s own equivalent, per that
crate's own `temp_socket_path()`) needs its own explicit serialization if
more than one test in the suite touches it — `cargo test`'s default
parallelism doesn't know or care that two tests share state outside the
process.

Verified on the release binaries, window-scoped, without synthetic input —
same isolated-`HOME`/`verify_harness` discipline "Daemon concurrency" above
established (a real `neko-daemon` binary was never spawned: the isolated
`neko` client ran from a directory with no `neko-daemon` sibling and an
empty `PATH`, so `daemon_launcher::ensure_daemon_running`'s spawn attempt
failed cleanly rather than transiently starting the real capture loop).
`evidence.rs` gained `NEKO_SHOW_SELECTION=1` (only read alongside
`NEKO_SHOW_QUERY`) — drives `Root::select_query_for_evidence` →
`TextField::select_all_for_evidence`, the exact logic ⌘A's real handler
uses, factored out so it doesn't need a live `Window` — for a rendered
selection highlight without a synthetic keystroke, this repo's standing
rule. `docs/evidence/text-field-selection-highlight.png`: a real, isolated
summon window with `SURFACE_SELECTED` visibly highlighting the whole query
text, driven by this hook alone. Copy/cut/paste were verified the same way
the rest of this section already was — real `NSPasteboard` round-trips
through `pasteboard.rs`'s actual read/write functions in `text_field.rs`'s
own test suite, own fixture strings only, never the captain's real
clipboard — not by a live ⌘V keystroke, since synthesizing one is exactly
the kind of synthetic OS input this repo's standing rule (`AGENTS.md`
throughout, and this task's own launch brief) rules out.
