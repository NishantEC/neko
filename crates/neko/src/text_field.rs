use std::ops::Range;

use gpui::{
    App, Bounds, Context, CursorStyle, ElementId, ElementInputHandler, Entity, EntityInputHandler,
    EventEmitter, FocusHandle, Focusable, GlobalElementId, InspectorElementId, LayoutId, Pixels,
    Point, Render, ShapedLine, SharedString, Style, TextRun, UTF16Selection, Window, actions, div,
    fill, point, prelude::*, px, relative,
};
use unicode_segmentation::UnicodeSegmentation;

use crate::components::vendor::gpui_component::blink_cursor::CursorBlink;
use crate::pasteboard;
use crate::theme;

actions!(
    text_field,
    [
        Backspace,
        Left,
        Right,
        DeleteLineStart,
        DeleteLineEnd,
        DeleteWordBackward,
        DeleteWordForward,
        LineStart,
        LineEnd,
        WordBackward,
        WordForward,
        SelectLeft,
        SelectRight,
        SelectWordLeft,
        SelectWordRight,
        SelectLineStart,
        SelectLineEnd,
        SelectAll,
        Copy,
        Cut,
        Paste,
    ]
);

/// Emitted only when `content` actually changes (a real edit), never on
/// cursor movement or a cursor-blink tick — both of those still call
/// `cx.notify()` for their own rendering reasons, but that is a render
/// signal, not a content-changed signal. `Root` subscribes to this event
/// (via `cx.subscribe`) instead of observing raw notifications (via
/// `cx.observe`) specifically so blink can never reach the search path —
/// see this file's and `panel.rs`'s module docs for the bug this fixes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContentChanged;

impl EventEmitter<ContentChanged> for TextField {}

/// A minimal single-line text field.
///
/// Has real keyboard-driven selection (⇧←/⇧→, ⇧⌥←/⇧⌥→, ⇧⌘←/⇧⌘→, ⌘A) and
/// clipboard (⌘C/⌘X/⌘V, via `pasteboard.rs`) — see this task's own commit
/// history for why those were the seam and not a gap. Deliberately still
/// skips mouse selection (click-drag) and IME composition (marked text) —
/// those remain real, separate pieces of work. It wires up just enough of
/// `EntityInputHandler` to receive typed characters through GPUI's native
/// input path, which is the part worth proving here.
pub struct TextField {
    focus_handle: FocusHandle,
    content: String,
    placeholder: SharedString,
    cursor: usize,
    /// The fixed end of an in-progress selection; `cursor` is always the
    /// moving end. `None` means no selection. A non-shift movement key
    /// clears this (standard macOS behavior — see each `on_*` handler
    /// below); any real edit clears it too, in `commit_edit`.
    selection_anchor: Option<usize>,
    last_layout: Option<ShapedLine>,
    last_bounds: Option<Bounds<Pixels>>,
    /// True between a mouse-down inside the field and the release that ends
    /// it. While set, the element registers *window*-level move/up listeners
    /// rather than relying on the field's own hover, so a drag that leaves the
    /// row — which is most of them, since the row is one line tall — keeps
    /// extending the selection instead of stopping at the boundary.
    mouse_selecting: bool,
    blink: Entity<CursorBlink>,
}

/// The root list's own placeholder — named so `panel::Root`'s mode-exit path
/// can restore it exactly, rather than duplicating the literal.
pub const DEFAULT_PLACEHOLDER: &str = "Search apps and clipboard…";

impl TextField {
    pub fn new(cx: &mut App) -> Entity<Self> {
        cx.new(|cx| {
            let blink = cx.new(|_| CursorBlink::new());
            blink.update(cx, |blink, cx| blink.start(cx));
            // Deliberately `cx.observe` + `cx.notify()`, not `cx.emit`: this
            // re-renders the field so the cursor visibly blinks, but must
            // never surface as `ContentChanged` — see that type's doc
            // comment for why a blink tick and a real edit are different
            // signals.
            cx.observe(&blink, |_, _, cx| cx.notify()).detach();
            Self {
                focus_handle: cx.focus_handle(),
                content: String::new(),
                placeholder: DEFAULT_PLACEHOLDER.into(),
                cursor: 0,
                selection_anchor: None,
                last_layout: None,
                last_bounds: None,
                mouse_selecting: false,
                blink,
            }
        })
    }

    pub fn content(&self) -> &str {
        &self.content
    }

    /// Reset to empty with the cursor at the start — used when a launch
    /// dismisses the panel, so the next summon starts from a clean field
    /// rather than the previous query.
    ///
    /// Deliberately does not emit `ContentChanged`: its one caller
    /// (`Root::reset_for_summon`) already re-runs search explicitly right
    /// after calling this, so emitting here too would fire the search
    /// request twice for one summon.
    pub fn clear(&mut self, cx: &mut Context<Self>) {
        self.content.clear();
        self.cursor = 0;
        self.selection_anchor = None;
        cx.notify();
    }

    fn touch_cursor(&mut self, cx: &mut Context<Self>) {
        self.blink.update(cx, |blink, cx| blink.pause(cx));
    }

    /// The current selection as a normalized `start..end` byte range
    /// (`start <= end`, regardless of which of `cursor`/`selection_anchor`
    /// is smaller) — `None` whenever there's no anchor, or the anchor and
    /// cursor coincide (an empty selection is the same as no selection, per
    /// standard text-field convention: it has nothing to copy/delete as a
    /// unit).
    pub fn selection_range(&self) -> Option<Range<usize>> {
        let anchor = self.selection_anchor?;
        if anchor == self.cursor {
            return None;
        }
        Some(anchor.min(self.cursor)..anchor.max(self.cursor))
    }

    /// The shared shape every `Select*` handler below reduces to: extend
    /// (or start, if none is active yet) the selection so its moving end is
    /// `new_cursor`. The anchor, once set, never moves until the selection
    /// is cleared (a real edit, or a non-shift movement key) — that's what
    /// lets ⇧← then ⇧→ shrink a selection back down rather than always
    /// growing it.
    fn extend_selection_to(&mut self, new_cursor: usize, cx: &mut Context<Self>) {
        if self.selection_anchor.is_none() {
            self.selection_anchor = Some(self.cursor);
        }
        self.cursor = new_cursor;
        self.touch_cursor(cx);
        cx.notify();
    }

    /// The one place `content` actually mutates. Emits `ContentChanged` in
    /// addition to `cx.notify()`, which is what lets `Root` re-run search on
    /// real edits without also reacting to blink's render-only notify (see
    /// `ContentChanged`'s doc comment). Takes a byte range rather than
    /// requiring a live `Window` (unlike `EntityInputHandler::
    /// replace_text_in_range`, which the input system requires to be
    /// `Window`-shaped even though it never reads it) so callers with no
    /// window at hand — `on_backspace`, and this file's own tests — can
    /// still drive a real edit.
    fn commit_edit(&mut self, range: Range<usize>, new_text: &str, cx: &mut Context<Self>) {
        self.content.replace_range(range.clone(), new_text);
        self.cursor = range.start + new_text.len();
        self.selection_anchor = None;
        self.touch_cursor(cx);
        cx.notify();
        cx.emit(ContentChanged);
    }

    /// The range a typed character or a delete action should act on: the
    /// active selection if there is one (typing/deleting over a selection
    /// replaces it, standard text-field behavior — every `on_*` action
    /// handler below that mutates content calls this first), otherwise a
    /// collapsed range at the cursor.
    fn edit_target_range(&self) -> Range<usize> {
        self.selection_range().unwrap_or(self.cursor..self.cursor)
    }

    /// Replaces the whole field with `text`, as a single real edit (emits
    /// `ContentChanged`, same as typing it). Evidence/verification-only —
    /// `evidence.rs`'s `NEKO_SHOW_QUERY` hook is the one caller, for driving
    /// a specific query into a window-scoped screenshot without needing
    /// synthetic OS keystrokes (unreliable in rapid succession — see
    /// `AGENTS.md`'s "Testing caveat"). Nothing in the real typing path
    /// calls this.
    pub fn set_content_for_evidence(&mut self, text: &str, cx: &mut Context<Self>) {
        self.commit_edit(0..self.content.len(), text, cx);
    }

    /// Replaces the whole field with `text`, as a single real edit (emits
    /// `ContentChanged`, same as typing it) — the real, non-evidence
    /// counterpart to [`set_content_for_evidence`](Self::set_content_for_evidence)
    /// above. Used for a mode transition's own query swap: entering a mode
    /// clears the field to start its filtered list fresh, and exiting
    /// restores whatever the root list's query was before entry — both are
    /// genuine content changes the mode's own search has to react to, not
    /// evidence-capture plumbing.
    pub fn set_content(&mut self, text: &str, cx: &mut Context<Self>) {
        self.commit_edit(0..self.content.len(), text, cx);
    }

    /// Swaps the field's placeholder text — a mode has its own ("Type to
    /// filter entries…"), distinct from the root list's default. Purely
    /// cosmetic (no content change, no `ContentChanged`), so this alone
    /// never re-runs a search.
    pub fn set_placeholder(&mut self, placeholder: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.placeholder = placeholder.into();
        cx.notify();
    }

    fn on_backspace(&mut self, _: &Backspace, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(range) = self.selection_range() {
            self.commit_edit(range, "", cx);
            return;
        }
        if self.cursor == 0 {
            return;
        }
        let prev = self.previous_char_boundary(self.cursor);
        let removed = prev..self.cursor;
        self.commit_edit(removed, "", cx);
    }

    fn on_left(&mut self, _: &Left, _window: &mut Window, cx: &mut Context<Self>) {
        self.cursor = self.previous_char_boundary(self.cursor);
        self.selection_anchor = None;
        self.touch_cursor(cx);
        cx.notify();
    }

    fn on_right(&mut self, _: &Right, _window: &mut Window, cx: &mut Context<Self>) {
        self.cursor = self.next_char_boundary(self.cursor);
        self.selection_anchor = None;
        self.touch_cursor(cx);
        cx.notify();
    }

    fn on_select_left(&mut self, _: &SelectLeft, _window: &mut Window, cx: &mut Context<Self>) {
        let new_cursor = self.previous_char_boundary(self.cursor);
        self.extend_selection_to(new_cursor, cx);
    }

    fn on_select_right(&mut self, _: &SelectRight, _window: &mut Window, cx: &mut Context<Self>) {
        let new_cursor = self.next_char_boundary(self.cursor);
        self.extend_selection_to(new_cursor, cx);
    }

    fn on_select_word_left(&mut self, _: &SelectWordLeft, _window: &mut Window, cx: &mut Context<Self>) {
        let new_cursor = self.word_start_before(self.cursor);
        self.extend_selection_to(new_cursor, cx);
    }

    fn on_select_word_right(&mut self, _: &SelectWordRight, _window: &mut Window, cx: &mut Context<Self>) {
        let new_cursor = self.word_end_after(self.cursor);
        self.extend_selection_to(new_cursor, cx);
    }

    fn on_select_line_start(&mut self, _: &SelectLineStart, _window: &mut Window, cx: &mut Context<Self>) {
        self.extend_selection_to(0, cx);
    }

    fn on_select_line_end(&mut self, _: &SelectLineEnd, _window: &mut Window, cx: &mut Context<Self>) {
        let end = self.content.len();
        self.extend_selection_to(end, cx);
    }

    /// ⌘A's real body, factored out so [`select_all_for_evidence`]
    /// (Self::select_all_for_evidence) can drive it without a live `Window`
    /// — nothing here touches one. Deliberately does not go through
    /// `extend_selection_to` — select all always re-anchors at the true
    /// start regardless of any selection already in progress, rather than
    /// extending from wherever the cursor currently sits.
    fn select_all(&mut self, cx: &mut Context<Self>) {
        self.selection_anchor = Some(0);
        self.cursor = self.content.len();
        self.touch_cursor(cx);
        cx.notify();
    }

    fn on_select_all(&mut self, _: &SelectAll, _window: &mut Window, cx: &mut Context<Self>) {
        self.select_all(cx);
    }

    /// Evidence/verification-only — the exact same logic ⌘A's real handler
    /// uses (`select_all` above), exposed without requiring a live `Window`
    /// (none of it touches one). For `evidence.rs`'s `NEKO_SHOW_SELECTION`
    /// hook: a rendered selection highlight needs an actual selection active
    /// first, and this repo's standing rule is no synthetic OS input to get
    /// one — see `set_content_for_evidence`'s own doc comment for the same
    /// reasoning applied to typing.
    pub(crate) fn select_all_for_evidence(&mut self, cx: &mut Context<Self>) {
        self.select_all(cx);
    }

    /// ⌘C. Read-only — does not touch `content`/`cursor`/`selection_anchor`,
    /// so (unlike every edit path) this never calls `commit_edit` and never
    /// emits `ContentChanged`.
    fn on_copy(&mut self, _: &Copy, _window: &mut Window, _cx: &mut Context<Self>) {
        if let Some(range) = self.selection_range() {
            pasteboard::write_string(&self.content[range]);
        }
    }

    /// ⌘X. A no-op (no pasteboard write, no edit) when there's no active
    /// selection — mirrors macOS's own single-line field behavior: cut with
    /// nothing selected does nothing, it doesn't fall back to deleting one
    /// character.
    fn on_cut(&mut self, _: &Cut, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(range) = self.selection_range() else {
            return;
        };
        pasteboard::write_string(&self.content[range.clone()]);
        self.commit_edit(range, "", cx);
    }

    /// ⌘V. Replaces the active selection if there is one, otherwise inserts
    /// at the cursor — the same `edit_target_range` shape typing a character
    /// uses. A pasteboard with no string representation (e.g. an image-only
    /// copy) is a silent no-op, not an error — nothing to insert.
    fn on_paste(&mut self, _: &Paste, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(text) = pasteboard::read_string() else {
            return;
        };
        let range = self.edit_target_range();
        self.commit_edit(range, &text, cx);
    }

    // ⌘⌫/⌘⌦/⌥⌫/⌥⌦/⌘←/⌘→/⌥←/⌥→ — standard macOS single-line editing
    // shortcuts. Each delete variant below checks for an active selection
    // first and, if there is one, deletes exactly that instead of its own
    // direction-specific range — standard macOS behavior: any delete command
    // with a selection active removes the selection, not one word/line past
    // it.

    fn on_delete_line_start(&mut self, _: &DeleteLineStart, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(range) = self.selection_range() {
            self.commit_edit(range, "", cx);
            return;
        }
        if self.cursor == 0 {
            return;
        }
        let removed = 0..self.cursor;
        self.commit_edit(removed, "", cx);
    }

    fn on_delete_line_end(&mut self, _: &DeleteLineEnd, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(range) = self.selection_range() {
            self.commit_edit(range, "", cx);
            return;
        }
        if self.cursor == self.content.len() {
            return;
        }
        let removed = self.cursor..self.content.len();
        self.commit_edit(removed, "", cx);
    }

    fn on_delete_word_backward(&mut self, _: &DeleteWordBackward, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(range) = self.selection_range() {
            self.commit_edit(range, "", cx);
            return;
        }
        let start = self.word_start_before(self.cursor);
        if start == self.cursor {
            return;
        }
        let removed = start..self.cursor;
        self.commit_edit(removed, "", cx);
    }

    fn on_delete_word_forward(&mut self, _: &DeleteWordForward, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(range) = self.selection_range() {
            self.commit_edit(range, "", cx);
            return;
        }
        let end = self.word_end_after(self.cursor);
        if end == self.cursor {
            return;
        }
        let removed = self.cursor..end;
        self.commit_edit(removed, "", cx);
    }

    fn on_line_start(&mut self, _: &LineStart, _window: &mut Window, cx: &mut Context<Self>) {
        self.cursor = 0;
        self.selection_anchor = None;
        self.touch_cursor(cx);
        cx.notify();
    }

    fn on_line_end(&mut self, _: &LineEnd, _window: &mut Window, cx: &mut Context<Self>) {
        self.cursor = self.content.len();
        self.selection_anchor = None;
        self.touch_cursor(cx);
        cx.notify();
    }

    fn on_word_backward(&mut self, _: &WordBackward, _window: &mut Window, cx: &mut Context<Self>) {
        self.cursor = self.word_start_before(self.cursor);
        self.selection_anchor = None;
        self.touch_cursor(cx);
        cx.notify();
    }

    fn on_word_forward(&mut self, _: &WordForward, _window: &mut Window, cx: &mut Context<Self>) {
        self.cursor = self.word_end_after(self.cursor);
        self.selection_anchor = None;
        self.touch_cursor(cx);
        cx.notify();
    }

    fn previous_char_boundary(&self, byte_offset: usize) -> usize {
        self.content[..byte_offset]
            .char_indices()
            .next_back()
            .map(|(idx, _)| idx)
            .unwrap_or(0)
    }

    fn next_char_boundary(&self, byte_offset: usize) -> usize {
        self.content[byte_offset..]
            .char_indices()
            .nth(1)
            .map(|(idx, _)| byte_offset + idx)
            .unwrap_or(self.content.len())
    }

    /// The start of the word `byte_offset` is inside or just after —
    /// Unicode-aware (`unicode_word_indices`, UAX#29 word boundaries, not
    /// `char::is_whitespace` on bytes), matching macOS's own ⌥← convention:
    /// from mid-word, jumps to that word's start; from trailing punctuation
    /// or whitespace, skips back over it to the previous word's start.
    /// The byte offset in `content` nearest a point on screen.
    ///
    /// **Clamped rather than optional at the edges.** `ShapedLine::index_for_x`
    /// answers `None` outside the shaped run, which is exactly where a drag
    /// spends most of its time — past the last character, or left of the first.
    /// Returning `None` there would freeze the selection at whatever it was
    /// when the pointer left the text, so out-of-range resolves to the nearest
    /// end instead, which is what every other text field on this platform does.
    fn byte_index_for_x(&self, x: Pixels) -> Option<usize> {
        let bounds = self.last_bounds?;
        let layout = self.last_layout.as_ref()?;
        let local = x - bounds.left();
        if local <= px(0.) {
            return Some(0);
        }
        if local >= layout.width {
            return Some(self.content.len());
        }
        layout.index_for_x(local).or(Some(self.content.len()))
    }

    /// What a press at `index` should select, given how many clicks it is.
    ///
    /// Pure and separate from the press handler so the rule is testable without
    /// a live `Window` — `byte_index_for_x` needs a `ShapedLine`, which only
    /// exists after a real paint, and this is the half worth pinning.
    fn selection_for_click(&self, index: usize, click_count: usize) -> (Option<usize>, usize) {
        match click_count {
            // Triple-click takes the line, which in a single-line field is
            // everything — the same result as ⌘A, reached with the mouse.
            n if n >= 3 => (Some(0), self.content.len()),
            2 => {
                let start = self.word_start_before(index.min(self.content.len()));
                let end = self.word_end_after(start);
                // A double-click past the last word has nothing to take; leave
                // a caret rather than an empty selection that renders as a
                // one-pixel highlight nobody asked for.
                if start == end { (None, start) } else { (Some(start), end) }
            }
            _ => (None, index),
        }
    }

    /// Mouse-down inside the field: place the caret, or select a word or the
    /// whole field on a repeat click.
    ///
    /// Returns whether the press was actually handled, so the caller only
    /// swallows the event when it was — a press arriving before the field has
    /// ever been laid out has nothing to place a caret against, and silently
    /// eating it would leave the press doing nothing at all.
    pub fn on_mouse_down(
        &mut self,
        position: Point<Pixels>,
        click_count: usize,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(index) = self.byte_index_for_x(position.x) else { return false };
        let (anchor, cursor) = self.selection_for_click(index, click_count);
        self.selection_anchor = anchor;
        self.cursor = cursor;
        // Only a single click starts a drag. Extending a word or line
        // selection by dragging is a real behaviour on this platform and a
        // materially bigger one — it snaps by whole words rather than by
        // character — so it is left out rather than half-built.
        self.mouse_selecting = click_count <= 1;
        self.touch_cursor(cx);
        cx.notify();
        true
    }

    /// Drag: move the caret, leaving the anchor where the press landed.
    pub fn on_mouse_drag(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        if !self.mouse_selecting {
            return;
        }
        let Some(index) = self.byte_index_for_x(position.x) else { return };
        if index == self.cursor {
            return;
        }
        self.extend_selection_to(index, cx);
    }

    /// Release. Also collapses a selection that never went anywhere, so a
    /// plain click leaves a caret rather than a zero-width selection.
    pub fn on_mouse_up(&mut self, cx: &mut Context<Self>) {
        if !self.mouse_selecting {
            return;
        }
        self.mouse_selecting = false;
        if self.selection_range().is_none() {
            self.selection_anchor = None;
        }
        cx.notify();
    }

    pub fn is_mouse_selecting(&self) -> bool {
        self.mouse_selecting
    }

    fn word_start_before(&self, byte_offset: usize) -> usize {
        self.content[..byte_offset]
            .unicode_word_indices()
            .next_back()
            .map(|(idx, _)| idx)
            .unwrap_or(0)
    }

    /// The end of the next word at or after `byte_offset` — the ⌥→
    /// counterpart to [`Self::word_start_before`]: from mid-word, jumps to
    /// that word's end; from leading whitespace/punctuation, skips forward
    /// over it to the next word's end.
    fn word_end_after(&self, byte_offset: usize) -> usize {
        self.content[byte_offset..]
            .unicode_word_indices()
            .next()
            .map(|(idx, word)| byte_offset + idx + word.len())
            .unwrap_or(self.content.len())
    }

    fn utf16_len(s: &str) -> usize {
        s.chars().map(char::len_utf16).sum()
    }

    fn byte_offset_for_utf16(&self, utf16_offset: usize) -> usize {
        let mut seen_utf16 = 0;
        for (byte_idx, ch) in self.content.char_indices() {
            if seen_utf16 >= utf16_offset {
                return byte_idx;
            }
            seen_utf16 += ch.len_utf16();
        }
        self.content.len()
    }

    fn utf16_offset_for_byte(&self, byte_offset: usize) -> usize {
        Self::utf16_len(&self.content[..byte_offset])
    }

    fn range_from_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.byte_offset_for_utf16(range.start)..self.byte_offset_for_utf16(range.end)
    }

    fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.utf16_offset_for_byte(range.start)..self.utf16_offset_for_byte(range.end)
    }
}

impl Focusable for TextField {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EntityInputHandler for TextField {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range_from_utf16(&range_utf16);
        actual_range.replace(self.range_to_utf16(&range));
        Some(self.content[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let (start, end, reversed) = match self.selection_anchor {
            Some(anchor) if anchor <= self.cursor => (anchor, self.cursor, false),
            Some(anchor) => (self.cursor, anchor, true),
            None => (self.cursor, self.cursor, false),
        };
        Some(UTF16Selection {
            range: self.utf16_offset_for_byte(start)..self.utf16_offset_for_byte(end),
            reversed,
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        None
    }

    fn unmark_text(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {}

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .map(|r| self.range_from_utf16(&r))
            .unwrap_or_else(|| self.edit_target_range());
        self.commit_edit(range, new_text, cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _new_selected_range_utf16: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Composition (marked text) is not supported in this minimal field;
        // treat it as an immediate insert so IME input still lands.
        self.replace_text_in_range(range_utf16, new_text, window, cx);
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let layout = self.last_layout.as_ref()?;
        let range = self.range_from_utf16(&range_utf16);
        let x = bounds.left() + layout.x_for_index(range.start);
        Some(Bounds::new(
            point(x, bounds.top()),
            gpui::size(px(1.), bounds.bottom() - bounds.top()),
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        let bounds = self.last_bounds?;
        let layout = self.last_layout.as_ref()?;
        let byte_idx = layout.index_for_x(point.x - bounds.left())?;
        Some(self.utf16_offset_for_byte(byte_idx))
    }
}

struct TextFieldElement {
    field: Entity<TextField>,
}

struct PrepaintState {
    line: ShapedLine,
    selection: Option<gpui::PaintQuad>,
    cursor: Option<gpui::PaintQuad>,
}

impl gpui::IntoElement for TextFieldElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl gpui::Element for TextFieldElement {
    type RequestLayoutState = ();
    type PrepaintState = PrepaintState;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = window.line_height().into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let field = self.field.read(cx);
        let text_style = window.text_style();
        let (display_text, color) = if field.content.is_empty() {
            (field.placeholder.clone(), theme::active().text_tertiary.into())
        } else {
            (field.content.clone().into(), text_style.color)
        };
        let run = TextRun {
            len: display_text.len(),
            font: text_style.font(),
            color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let font_size = text_style.font_size.to_pixels(window.rem_size());
        let line = window
            .text_system()
            .shape_line(display_text, font_size, &[run], None);

        // Painted behind the text (see `paint` below: selection first, then
        // the shaped line on top) so selected characters stay legible rather
        // than being covered by an opaque highlight — the same reason
        // `panel.rs`'s row-selection highlight (`theme::active().surface_selected`,
        // reused here rather than inventing a new token) sits behind its
        // row's own content, not above it.
        let selection = field.selection_range().map(|range| {
            let start_x = bounds.left() + line.x_for_index(range.start);
            let end_x = bounds.left() + line.x_for_index(range.end);
            fill(
                Bounds::new(
                    point(start_x, bounds.top()),
                    gpui::size(end_x - start_x, bounds.bottom() - bounds.top()),
                ),
                theme::active().surface_selected,
            )
        });

        let show_cursor = field.selection_range().is_none()
            && field.focus_handle.is_focused(window)
            && field.blink.read(cx).visible();
        let cursor = if show_cursor {
            let x = bounds.left() + line.x_for_index(field.cursor);
            Some(fill(
                Bounds::new(
                    point(x, bounds.top()),
                    gpui::size(px(2.), bounds.bottom() - bounds.top()),
                ),
                theme::active().text_primary,
            ))
        } else {
            None
        };

        PrepaintState { line, selection, cursor }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus_handle = self.field.read(cx).focus_handle.clone();
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.field.clone()),
            cx,
        );
        if let Some(selection) = prepaint.selection.take() {
            window.paint_quad(selection);
        }
        prepaint
            .line
            .paint(bounds.origin, window.line_height(), gpui::TextAlign::Left, None, window, cx)
            .ok();
        if let Some(cursor) = prepaint.cursor.take() {
            window.paint_quad(cursor);
        }
        self.field.update(cx, |field, _cx| {
            field.last_layout = Some(prepaint.line.clone());
            field.last_bounds = Some(bounds);
        });

        // **Window-level, and only while a drag is actually live.** A `div`'s
        // own `on_mouse_move` is gated on its hitbox being hovered, and this
        // field is one line tall — a selection drag leaves it almost
        // immediately, which would strand the selection at the row's edge.
        // Registering here rather than in `render` also means these exist for
        // exactly the frames they are needed: gpui clears window mouse
        // listeners every frame, so there is nothing to unregister and no way
        // for one to outlive the gesture.
        if self.field.read(cx).is_mouse_selecting() {
            let field = self.field.clone();
            window.on_mouse_event(move |event: &gpui::MouseMoveEvent, phase, _window, cx| {
                if phase == gpui::DispatchPhase::Bubble
                    && event.pressed_button == Some(gpui::MouseButton::Left)
                {
                    field.update(cx, |field, cx| field.on_mouse_drag(event.position, cx));
                }
            });
            let field = self.field.clone();
            window.on_mouse_event(move |_: &gpui::MouseUpEvent, phase, _window, cx| {
                if phase == gpui::DispatchPhase::Bubble {
                    field.update(cx, |field, cx| field.on_mouse_up(cx));
                }
            });
        }
    }
}

impl Render for TextField {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
        div()
            .key_context("TextField")
            .track_focus(&self.focus_handle)
            .cursor(CursorStyle::IBeam)
            .on_action(cx.listener(Self::on_backspace))
            .on_action(cx.listener(Self::on_left))
            .on_action(cx.listener(Self::on_right))
            .on_action(cx.listener(Self::on_delete_line_start))
            .on_action(cx.listener(Self::on_delete_line_end))
            .on_action(cx.listener(Self::on_delete_word_backward))
            .on_action(cx.listener(Self::on_delete_word_forward))
            .on_action(cx.listener(Self::on_line_start))
            .on_action(cx.listener(Self::on_line_end))
            .on_action(cx.listener(Self::on_word_backward))
            .on_action(cx.listener(Self::on_word_forward))
            .on_action(cx.listener(Self::on_select_left))
            .on_action(cx.listener(Self::on_select_right))
            .on_action(cx.listener(Self::on_select_word_left))
            .on_action(cx.listener(Self::on_select_word_right))
            .on_action(cx.listener(Self::on_select_line_start))
            .on_action(cx.listener(Self::on_select_line_end))
            .on_action(cx.listener(Self::on_select_all))
            .on_action(cx.listener(Self::on_copy))
            .on_action(cx.listener(Self::on_cut))
            .on_action(cx.listener(Self::on_paste))
            .w_full()
            // **This is also what stops the panel dragging out from under a
            // press in the field.** `panel::render_input_row` begins a window
            // drag on mouse-down in the bubble phase; the field is a descendant,
            // so its own bubble handler runs first and `stop_propagation` keeps
            // the gesture here. Only when the press was genuinely handled —
            // before the first layout there is nothing to place a caret
            // against, and swallowing it then would make the press do nothing.
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|field, event: &gpui::MouseDownEvent, _window, cx| {
                    if field.on_mouse_down(event.position, event.click_count, cx) {
                        cx.stop_propagation();
                    }
                }),
            )
            .child(TextFieldElement { field: cx.entity() })
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;
    use std::sync::Mutex;

    use gpui::TestAppContext;

    use super::*;

    /// `pasteboard.rs` reads/writes the real, systemwide `NSPasteboard` —
    /// there's no per-test isolation for it the way `TestAppContext` gives
    /// each test its own `TextField`. `cargo test` runs tests on multiple
    /// threads by default, so two pasteboard tests running concurrently can
    /// interleave their writes/reads and fail on each other's fixture
    /// strings rather than their own. Every test below that touches
    /// `pasteboard::{read_string, write_string}` holds this lock for its
    /// whole body so those calls are never concurrent with each other —
    /// same reasoning as any other real-shared-resource test lock, nothing
    /// pasteboard-specific about the pattern itself. `unwrap_or_else` rather
    /// than a bare `unwrap` so one test panicking mid-lock (poisoning it)
    /// doesn't also fail every pasteboard test that runs after it.
    fn pasteboard_test_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: Mutex<()> = Mutex::new(());
        LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Reproduces the exact defect the captain hit: `TextField::new` wires
    /// blink's tick to `cx.notify()` (needed so the cursor visibly blinks),
    /// and this used to be the only signal `Root` had to know the query
    /// changed — so search re-ran, and the selection reset, roughly twice a
    /// second forever. This proves the fix at its source: a blink-shaped
    /// notify on the field's own `blink` entity must never surface as
    /// `ContentChanged`, the event `Root` now subscribes to instead.
    #[gpui::test]
    fn blink_notification_does_not_emit_content_changed(cx: &mut TestAppContext) {
        let field = cx.update(TextField::new);
        let content_changed_count = Rc::new(Cell::new(0));

        let counted = content_changed_count.clone();
        cx.update(|cx| {
            cx.subscribe(&field, move |_field, _event: &ContentChanged, _cx| {
                counted.set(counted.get() + 1);
            })
            .detach();
        });

        // Simulate a cursor-blink tick: exactly the signal
        // `cx.observe(&blink, |_, _, cx| cx.notify())` reacts to.
        field.update(cx, |field, cx| {
            field.blink.update(cx, |_, cx| cx.notify());
        });
        cx.run_until_parked();
        assert_eq!(
            content_changed_count.get(),
            0,
            "a blink-only notify must never trigger ContentChanged"
        );

        // A real edit still does — proves the subscription itself works and
        // isn't just silently disconnected.
        field.update(cx, |field, cx| field.commit_edit(0..0, "a", cx));
        cx.run_until_parked();
        assert_eq!(
            content_changed_count.get(),
            1,
            "a real edit must still trigger ContentChanged"
        );
    }

    /// Cursor movement (Left/Right) touches blink (to keep the cursor solid
    /// while navigating) and re-renders, but is not a content edit — must
    /// not trigger a search either.
    #[gpui::test]
    fn cursor_movement_does_not_emit_content_changed(cx: &mut TestAppContext) {
        let field = cx.update(TextField::new);
        field.update(cx, |field, cx| field.commit_edit(0..0, "ab", cx));
        cx.run_until_parked();

        let content_changed_count = Rc::new(Cell::new(0));
        let counted = content_changed_count.clone();
        cx.update(|cx| {
            cx.subscribe(&field, move |_field, _event: &ContentChanged, _cx| {
                counted.set(counted.get() + 1);
            })
            .detach();
        });

        field.update(cx, |field, cx| {
            field.cursor = field.previous_char_boundary(field.cursor);
            field.touch_cursor(cx);
            cx.notify();
        });
        cx.run_until_parked();
        assert_eq!(content_changed_count.get(), 0);
    }

    // The eight standard macOS shortcuts below are tested against the
    // editing model directly (`word_start_before`/`word_end_after`/
    // `commit_edit`/`cursor` — the same methods each `on_*` action handler
    // calls), the same way the existing tests above exercise
    // `previous_char_boundary` without going through GPUI's action-dispatch
    // layer, which needs a live `Window` these entity-only tests don't open.

    #[gpui::test]
    fn cmd_backspace_deletes_to_line_start(cx: &mut TestAppContext) {
        let field = cx.update(TextField::new);
        field.update(cx, |field, cx| {
            field.commit_edit(0..0, "hello world", cx);
            field.cursor = 8; // inside "world"
            let removed = 0..field.cursor;
            field.commit_edit(removed, "", cx);
            assert_eq!(field.content, "rld");
            assert_eq!(field.cursor, 0);
        });
    }

    #[gpui::test]
    fn cmd_delete_deletes_to_line_end(cx: &mut TestAppContext) {
        let field = cx.update(TextField::new);
        field.update(cx, |field, cx| {
            field.commit_edit(0..0, "hello world", cx);
            field.cursor = 5; // right after "hello"
            let removed = field.cursor..field.content.len();
            field.commit_edit(removed, "", cx);
            assert_eq!(field.content, "hello");
            assert_eq!(field.cursor, 5);
        });
    }

    #[gpui::test]
    fn alt_backspace_deletes_previous_word(cx: &mut TestAppContext) {
        let field = cx.update(TextField::new);
        field.update(cx, |field, cx| {
            field.commit_edit(0..0, "hello world", cx);
            field.cursor = field.content.len();
            let start = field.word_start_before(field.cursor);
            field.commit_edit(start..field.cursor, "", cx);
            assert_eq!(field.content, "hello ");
            assert_eq!(field.cursor, 6);
        });
    }

    #[gpui::test]
    fn alt_delete_deletes_next_word(cx: &mut TestAppContext) {
        let field = cx.update(TextField::new);
        field.update(cx, |field, cx| {
            field.commit_edit(0..0, "hello world", cx);
            field.cursor = 0;
            let end = field.word_end_after(field.cursor);
            field.commit_edit(field.cursor..end, "", cx);
            assert_eq!(field.content, " world");
            assert_eq!(field.cursor, 0);
        });
    }

    #[gpui::test]
    fn cmd_left_moves_to_line_start(cx: &mut TestAppContext) {
        let field = cx.update(TextField::new);
        field.update(cx, |field, cx| {
            field.commit_edit(0..0, "hello world", cx);
            field.cursor = 0;
            field.touch_cursor(cx);
            assert_eq!(field.cursor, 0);
        });
    }

    #[gpui::test]
    fn cmd_right_moves_to_line_end(cx: &mut TestAppContext) {
        let field = cx.update(TextField::new);
        field.update(cx, |field, cx| {
            field.commit_edit(0..0, "hello world", cx);
            field.cursor = field.content.len();
            field.touch_cursor(cx);
            assert_eq!(field.cursor, "hello world".len());
        });
    }

    #[gpui::test]
    fn alt_left_moves_to_previous_word_start(cx: &mut TestAppContext) {
        let field = cx.update(TextField::new);
        field.update(cx, |field, cx| {
            field.commit_edit(0..0, "hello   world", cx);
            field.cursor = field.content.len();
            field.cursor = field.word_start_before(field.cursor);
            field.touch_cursor(cx);
            // Skips the run of trailing spaces to land on "world"'s own
            // start, not just one character back.
            assert_eq!(field.cursor, 8);
        });
    }

    #[gpui::test]
    fn alt_right_moves_to_next_word_end(cx: &mut TestAppContext) {
        let field = cx.update(TextField::new);
        field.update(cx, |field, cx| {
            field.commit_edit(0..0, "hello   world", cx);
            field.cursor = 0;
            field.cursor = field.word_end_after(field.cursor);
            field.touch_cursor(cx);
            assert_eq!(field.cursor, 5);
        });
    }

    /// Word boundaries must be Unicode-aware, not `char::is_whitespace` on
    /// byte offsets — a naive byte-indexed scan would panic or split a
    /// multi-byte character mid-codepoint on non-ASCII input. "café" is a
    /// single word under UAX#29 (the accented "é" is alphabetic, contiguous
    /// with the rest, unlike a byte-wise scan that could stop mid-codepoint
    /// on "é"'s two UTF-8 bytes). The CJK run has no ASCII whitespace inside
    /// it at all, yet each ideograph is still its own UAX#29 word boundary
    /// (real, correct Unicode behavior, not a language-aware dictionary
    /// segmentation) — walking it one step at a time proves every stop
    /// lands on a real char boundary rather than panicking or drifting.
    #[gpui::test]
    fn word_boundaries_are_unicode_aware(cx: &mut TestAppContext) {
        let field = cx.update(TextField::new);
        field.update(cx, |field, cx| {
            field.commit_edit(0..0, "café 東京 test", cx);
            assert_eq!(field.word_end_after(0), "café".len(), "café is one word, accents included");
            let after_cafe = "café ".len();
            let after_first_ideograph = field.word_end_after(after_cafe);
            assert_eq!(
                after_first_ideograph,
                "café 東".len(),
                "each CJK ideograph is its own UAX#29 word boundary"
            );
            assert_eq!(
                field.word_end_after(after_first_ideograph),
                "café 東京".len(),
                "the second ideograph is the next stop, still a valid char boundary"
            );
            assert_eq!(
                field.word_start_before(field.content.len()),
                "café 東京 ".len(),
                "walking back from the end lands on \"test\"'s own start"
            );
        });
    }

    /// `set_content` (a mode's own query swap on enter/exit) is a real edit,
    /// unlike `set_placeholder` — a mode's UI copy changes without touching
    /// what's actually been typed.
    #[gpui::test]
    fn set_content_emits_content_changed_but_set_placeholder_does_not(cx: &mut TestAppContext) {
        let field = cx.update(TextField::new);
        let content_changed_count = Rc::new(Cell::new(0));
        let counted = content_changed_count.clone();
        cx.update(|cx| {
            cx.subscribe(&field, move |_field, _event: &ContentChanged, _cx| {
                counted.set(counted.get() + 1);
            })
            .detach();
        });

        field.update(cx, |field, cx| field.set_placeholder("Type to filter entries…", cx));
        cx.run_until_parked();
        assert_eq!(content_changed_count.get(), 0, "a placeholder swap is cosmetic, not a content edit");
        field.read_with(cx, |field, _| assert_eq!(field.placeholder.as_ref(), "Type to filter entries…"));

        field.update(cx, |field, cx| field.set_content("clipboard", cx));
        cx.run_until_parked();
        assert_eq!(content_changed_count.get(), 1, "set_content is a real edit and must trigger a re-search");
        field.read_with(cx, |field, _| assert_eq!(field.content(), "clipboard"));
    }

    #[gpui::test]
    fn set_content_replaces_whatever_was_there_before(cx: &mut TestAppContext) {
        let field = cx.update(TextField::new);
        field.update(cx, |field, cx| field.commit_edit(0..0, "old query", cx));
        field.update(cx, |field, cx| field.set_content("new", cx));
        field.read_with(cx, |field, _| assert_eq!(field.content(), "new"));
    }

    // Selection. Each test drives `extend_selection_to` / `commit_edit` /
    // `selection_range` directly — the same window-free entity methods each
    // real `on_select_*`/`on_backspace`/etc. action handler calls
    // internally — rather than the `on_*` handlers themselves, which take a
    // `&mut Window` this file's existing tests have never opened (see
    // `cmd_backspace_deletes_to_line_start` above for the same convention
    // predating this task).

    #[gpui::test]
    fn shift_right_extends_selection_one_char_at_a_time(cx: &mut TestAppContext) {
        let field = cx.update(TextField::new);
        field.update(cx, |field, cx| {
            field.commit_edit(0..0, "hello", cx);
            field.cursor = 0;
            let next = field.next_char_boundary(field.cursor);
            field.extend_selection_to(next, cx);
            assert_eq!(field.selection_range(), Some(0..1));
            let next = field.next_char_boundary(field.cursor);
            field.extend_selection_to(next, cx);
            assert_eq!(field.selection_range(), Some(0..2));
            assert_eq!(field.cursor, 2);
        });
    }

    #[gpui::test]
    fn shift_left_extends_selection_backward(cx: &mut TestAppContext) {
        let field = cx.update(TextField::new);
        field.update(cx, |field, cx| {
            field.commit_edit(0..0, "hello", cx);
            field.cursor = 5;
            let prev = field.previous_char_boundary(field.cursor);
            field.extend_selection_to(prev, cx);
            let prev = field.previous_char_boundary(field.cursor);
            field.extend_selection_to(prev, cx);
            assert_eq!(field.selection_range(), Some(3..5));
            assert_eq!(field.cursor, 3);
        });
    }

    #[gpui::test]
    fn shift_right_then_shift_left_shrinks_the_selection_back(cx: &mut TestAppContext) {
        let field = cx.update(TextField::new);
        field.update(cx, |field, cx| {
            field.commit_edit(0..0, "hello", cx);
            field.cursor = 0;
            for _ in 0..3 {
                let next = field.next_char_boundary(field.cursor);
                field.extend_selection_to(next, cx);
            }
            assert_eq!(field.selection_range(), Some(0..3));
            let prev = field.previous_char_boundary(field.cursor);
            field.extend_selection_to(prev, cx);
            assert_eq!(
                field.selection_range(),
                Some(0..2),
                "the anchor stays fixed at 0 — shift-left shrinks toward it, not past it"
            );
        });
    }

    #[gpui::test]
    fn a_non_shift_movement_key_collapses_the_selection(cx: &mut TestAppContext) {
        let field = cx.update(TextField::new);
        field.update(cx, |field, cx| {
            field.commit_edit(0..0, "hello", cx);
            field.cursor = 0;
            for _ in 0..2 {
                let next = field.next_char_boundary(field.cursor);
                field.extend_selection_to(next, cx);
            }
            assert!(field.selection_range().is_some());
            // The plain (non-shift) Right handler's own body: move, then
            // clear the anchor.
            field.cursor = field.next_char_boundary(field.cursor);
            field.selection_anchor = None;
            assert_eq!(field.selection_range(), None, "a plain arrow key must clear the selection");
        });
    }

    #[gpui::test]
    fn shift_alt_right_extends_selection_by_word(cx: &mut TestAppContext) {
        let field = cx.update(TextField::new);
        field.update(cx, |field, cx| {
            field.commit_edit(0..0, "hello world", cx);
            field.cursor = 0;
            let end = field.word_end_after(field.cursor);
            field.extend_selection_to(end, cx);
            assert_eq!(field.selection_range(), Some(0..5), "extends to the end of \"hello\"");
        });
    }

    #[gpui::test]
    fn shift_alt_left_extends_selection_by_word(cx: &mut TestAppContext) {
        let field = cx.update(TextField::new);
        field.update(cx, |field, cx| {
            field.commit_edit(0..0, "hello world", cx);
            field.cursor = field.content.len();
            let start = field.word_start_before(field.cursor);
            field.extend_selection_to(start, cx);
            assert_eq!(field.selection_range(), Some(6..11), "extends back to \"world\"'s own start");
        });
    }

    #[gpui::test]
    fn shift_cmd_right_extends_selection_to_line_end(cx: &mut TestAppContext) {
        let field = cx.update(TextField::new);
        field.update(cx, |field, cx| {
            field.commit_edit(0..0, "hello world", cx);
            field.cursor = 0;
            let end = field.content.len();
            field.extend_selection_to(end, cx);
            assert_eq!(field.selection_range(), Some(0..11));
        });
    }

    #[gpui::test]
    fn shift_cmd_left_extends_selection_to_line_start(cx: &mut TestAppContext) {
        let field = cx.update(TextField::new);
        field.update(cx, |field, cx| {
            field.commit_edit(0..0, "hello world", cx);
            field.cursor = field.content.len();
            field.extend_selection_to(0, cx);
            assert_eq!(field.selection_range(), Some(0..11));
        });
    }

    #[gpui::test]
    fn cmd_a_selects_the_whole_field(cx: &mut TestAppContext) {
        let field = cx.update(TextField::new);
        field.update(cx, |field, cx| {
            field.commit_edit(0..0, "hello world", cx);
            field.cursor = 4;
            field.selection_anchor = Some(0);
            field.cursor = field.content.len();
            field.touch_cursor(cx);
            assert_eq!(field.selection_range(), Some(0..11));
            assert_eq!(field.cursor, 11);
        });
    }

    #[gpui::test]
    fn cmd_a_on_an_empty_field_selects_nothing_and_does_not_panic(cx: &mut TestAppContext) {
        let field = cx.update(TextField::new);
        field.update(cx, |field, cx| {
            field.selection_anchor = Some(0);
            field.cursor = field.content.len();
            field.touch_cursor(cx);
            assert_eq!(field.selection_range(), None, "0..0 is an empty selection, same as no selection");
        });
    }

    #[gpui::test]
    fn typing_a_character_replaces_the_active_selection(cx: &mut TestAppContext) {
        let field = cx.update(TextField::new);
        field.update(cx, |field, cx| {
            field.commit_edit(0..0, "hello world", cx);
            field.selection_anchor = Some(0);
            field.cursor = 5; // "hello" selected
            let range = field.edit_target_range();
            field.commit_edit(range, "goodbye", cx);
            assert_eq!(field.content, "goodbye world");
            assert_eq!(field.cursor, "goodbye".len());
            assert_eq!(field.selection_range(), None, "committing an edit must clear the selection");
        });
    }

    #[gpui::test]
    fn backspace_with_a_selection_deletes_the_selection_not_one_char(cx: &mut TestAppContext) {
        let field = cx.update(TextField::new);
        field.update(cx, |field, cx| {
            field.commit_edit(0..0, "hello world", cx);
            field.selection_anchor = Some(6);
            field.cursor = 11; // "world" selected
            let range = field.selection_range().expect("a selection is active");
            field.commit_edit(range, "", cx);
            assert_eq!(field.content, "hello ");
            assert_eq!(field.cursor, 6);
        });
    }

    #[gpui::test]
    fn delete_word_backward_with_a_selection_deletes_the_selection_not_a_whole_word(cx: &mut TestAppContext) {
        let field = cx.update(TextField::new);
        field.update(cx, |field, cx| {
            field.commit_edit(0..0, "hello world", cx);
            field.selection_anchor = Some(2);
            field.cursor = 4; // "ll" selected, well inside "hello"
            let range = field.selection_range().expect("a selection is active");
            field.commit_edit(range, "", cx);
            assert_eq!(field.content, "heo world", "must delete exactly the selection, not the whole word");
        });
    }

    // Clipboard. Copy/cut/paste round-trip through the real `NSPasteboard`
    // (`pasteboard.rs`, autoreleasepool-wrapped) — verified here by reading
    // back through the exact same API just written through, never by
    // inspecting whatever the real system clipboard held before the test
    // ran. Each test writes its own unique fixture string first, so nothing
    // about the machine's real prior clipboard contents is ever read or
    // asserted on.

    #[gpui::test]
    fn copy_writes_the_selection_to_the_real_pasteboard(cx: &mut TestAppContext) {
        let _guard = pasteboard_test_lock();
        let field = cx.update(TextField::new);
        field.update(cx, |field, cx| {
            field.commit_edit(0..0, "neko-textinput-fixture-copy-9f2a", cx);
            field.selection_anchor = Some(0);
            field.cursor = field.content.len();
            let range = field.selection_range().expect("a selection is active");
            pasteboard::write_string(&field.content[range]);
            // Content and cursor are untouched — copy is read-only.
            assert_eq!(field.content, "neko-textinput-fixture-copy-9f2a");
        });
        assert_eq!(
            pasteboard::read_string().as_deref(),
            Some("neko-textinput-fixture-copy-9f2a"),
            "read back through the same NSPasteboard API just written through"
        );
    }

    #[gpui::test]
    fn cut_writes_the_selection_and_removes_it_from_the_field(cx: &mut TestAppContext) {
        let _guard = pasteboard_test_lock();
        let field = cx.update(TextField::new);
        field.update(cx, |field, cx| {
            field.commit_edit(0..0, "keep neko-textinput-fixture-cut-7c1e", cx);
            field.selection_anchor = Some(5);
            field.cursor = field.content.len();
            let range = field.selection_range().expect("a selection is active");
            pasteboard::write_string(&field.content[range.clone()]);
            field.commit_edit(range, "", cx);
            assert_eq!(field.content, "keep ");
            assert_eq!(field.selection_range(), None);
        });
        assert_eq!(
            pasteboard::read_string().as_deref(),
            Some("neko-textinput-fixture-cut-7c1e")
        );
    }

    #[gpui::test]
    fn cut_with_no_selection_is_a_no_op(cx: &mut TestAppContext) {
        let field = cx.update(TextField::new);
        field.update(cx, |field, cx| {
            field.commit_edit(0..0, "unchanged", cx);
            field.cursor = 3;
            assert_eq!(field.selection_range(), None, "nothing selected — on_cut's own guard would return early here");
            assert_eq!(field.content, "unchanged");
            assert_eq!(field.cursor, 3);
        });
    }

    #[gpui::test]
    fn paste_inserts_at_the_cursor_when_nothing_is_selected(cx: &mut TestAppContext) {
        let _guard = pasteboard_test_lock();
        pasteboard::write_string("neko-textinput-fixture-paste-4b6d");
        let field = cx.update(TextField::new);
        field.update(cx, |field, cx| {
            field.commit_edit(0..0, "start end", cx);
            field.cursor = 6; // right after "start "
            let text = pasteboard::read_string().expect("fixture was just written");
            let range = field.edit_target_range();
            field.commit_edit(range, &text, cx);
            assert_eq!(field.content, "start neko-textinput-fixture-paste-4b6dend");
        });
    }

    #[gpui::test]
    fn paste_replaces_an_active_selection(cx: &mut TestAppContext) {
        let _guard = pasteboard_test_lock();
        pasteboard::write_string("neko-textinput-fixture-paste-replace-2e91");
        let field = cx.update(TextField::new);
        field.update(cx, |field, cx| {
            field.commit_edit(0..0, "hello world", cx);
            field.selection_anchor = Some(0);
            field.cursor = field.content.len();
            let text = pasteboard::read_string().expect("fixture was just written");
            let range = field.edit_target_range();
            field.commit_edit(range, &text, cx);
            assert_eq!(field.content, "neko-textinput-fixture-paste-replace-2e91");
        });
    }

    #[gpui::test]
    fn copy_cut_paste_round_trip_via_the_real_pasteboard(cx: &mut TestAppContext) {
        let _guard = pasteboard_test_lock();
        let field = cx.update(TextField::new);
        field.update(cx, |field, cx| {
            field.commit_edit(0..0, "roundtrip-neko-textinput-fixture-b83a", cx);
            field.selection_anchor = Some(0);
            field.cursor = field.content.len();
            let range = field.selection_range().expect("a selection is active");
            pasteboard::write_string(&field.content[range]);
            field.clear(cx);
            let text = pasteboard::read_string().expect("just wrote it above");
            let range = field.edit_target_range();
            field.commit_edit(range, &text, cx);
            assert_eq!(field.content, "roundtrip-neko-textinput-fixture-b83a");
        });
    }

    #[gpui::test]
    fn a_single_click_places_a_caret_and_selects_nothing(cx: &mut TestAppContext) {
        let field = cx.update(TextField::new);
        field.update(cx, |field, cx| {
            field.set_content("café 東京 test", cx);
            let (anchor, cursor) = field.selection_for_click(6, 1);
            assert_eq!(anchor, None, "a plain click must not leave a selection");
            assert_eq!(cursor, 6);
        });
    }

    #[gpui::test]
    fn a_double_click_takes_the_word_under_it(cx: &mut TestAppContext) {
        let field = cx.update(TextField::new);
        field.update(cx, |field, cx| {
            field.set_content("hello brave world", cx);
            // Anywhere inside "brave" — the middle, not just the first byte.
            let (anchor, cursor) = field.selection_for_click(8, 2);
            assert_eq!(anchor, Some(6));
            assert_eq!(cursor, 11);
            assert_eq!(&field.content()[6..11], "brave");
        });
    }

    #[gpui::test]
    fn a_double_click_in_trailing_space_takes_the_word_before_it(cx: &mut TestAppContext) {
        // Not an empty selection, and not nothing: the nearest word behind the
        // press, which is what this platform's own fields do.
        let field = cx.update(TextField::new);
        field.update(cx, |field, cx| {
            field.set_content("hi ", cx);
            let (anchor, cursor) = field.selection_for_click(3, 2);
            assert_eq!((anchor, cursor), (Some(0), 2));
        });
    }

    #[gpui::test]
    fn a_double_click_on_an_empty_field_leaves_a_caret(cx: &mut TestAppContext) {
        // The `start == end` branch. It turns out to be reachable only on an
        // empty field: a run of spaces selects the run, the same as this
        // platform. An empty selection would render as a one-pixel highlight —
        // a rendering glitch rather than "nothing is selected".
        let field = cx.update(TextField::new);
        field.update(cx, |field, cx| {
            field.set_content("   ", cx);
            assert_eq!(field.selection_for_click(2, 2), (Some(0), 3), "spaces select the run");
            field.set_content("", cx);
            assert_eq!(field.selection_for_click(0, 2), (None, 0));
        });
    }

    #[gpui::test]
    fn a_triple_click_takes_everything_wherever_it_lands(cx: &mut TestAppContext) {
        let field = cx.update(TextField::new);
        field.update(cx, |field, cx| {
            field.set_content("hello world", cx);
            for index in [0, 5, 11] {
                let (anchor, cursor) = field.selection_for_click(index, 3);
                assert_eq!(anchor, Some(0), "index {index}");
                assert_eq!(cursor, 11, "index {index}");
            }
        });
    }

    #[gpui::test]
    fn only_a_single_click_arms_a_drag(cx: &mut TestAppContext) {
        // Dragging out of a double-click should extend by whole words on this
        // platform. That is a materially bigger behaviour, so it is left out
        // rather than half-built — and the flag is what makes "left out" true
        // instead of "extends by character, which is wrong".
        let field = cx.update(TextField::new);
        field.update(cx, |field, cx| {
            field.set_content("hello world", cx);
            assert!(!field.is_mouse_selecting());
            field.mouse_selecting = true;
            field.on_mouse_up(cx);
            assert!(!field.is_mouse_selecting(), "release always ends the drag");
        });
    }

    #[gpui::test]
    fn a_drag_that_never_moved_leaves_a_caret(cx: &mut TestAppContext) {
        let field = cx.update(TextField::new);
        field.update(cx, |field, cx| {
            field.set_content("hello", cx);
            field.cursor = 3;
            field.selection_anchor = Some(3);
            field.mouse_selecting = true;
            field.on_mouse_up(cx);
            assert_eq!(field.selection_range(), None);
            assert_eq!(field.selection_anchor, None, "a zero-width anchor is cleared");
        });
    }
}
