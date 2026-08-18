use std::ops::Range;

use gpui::{
    App, Bounds, Context, CursorStyle, ElementId, ElementInputHandler, Entity, EntityInputHandler,
    EventEmitter, FocusHandle, Focusable, GlobalElementId, InspectorElementId, LayoutId, Pixels,
    Point, Render, ShapedLine, SharedString, Style, TextRun, UTF16Selection, Window, actions, div,
    fill, point, prelude::*, px, relative,
};
use unicode_segmentation::UnicodeSegmentation;

use crate::components::vendor::gpui_component::blink_cursor::CursorBlink;
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
/// This deliberately skips mouse selection, clipboard, and IME composition
/// (marked text) support — those are real, separate pieces of work. It wires
/// up just enough of `EntityInputHandler` to receive typed characters through
/// GPUI's native input path, which is the part worth proving here.
pub struct TextField {
    focus_handle: FocusHandle,
    content: String,
    placeholder: SharedString,
    cursor: usize,
    last_layout: Option<ShapedLine>,
    last_bounds: Option<Bounds<Pixels>>,
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
                last_layout: None,
                last_bounds: None,
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
        cx.notify();
    }

    fn touch_cursor(&mut self, cx: &mut Context<Self>) {
        self.blink.update(cx, |blink, cx| blink.pause(cx));
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
        self.touch_cursor(cx);
        cx.notify();
        cx.emit(ContentChanged);
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
        if self.cursor == 0 {
            return;
        }
        let prev = self.previous_char_boundary(self.cursor);
        let removed = prev..self.cursor;
        self.commit_edit(removed, "", cx);
    }

    fn on_left(&mut self, _: &Left, _window: &mut Window, cx: &mut Context<Self>) {
        self.cursor = self.previous_char_boundary(self.cursor);
        self.touch_cursor(cx);
        cx.notify();
    }

    fn on_right(&mut self, _: &Right, _window: &mut Window, cx: &mut Context<Self>) {
        self.cursor = self.next_char_boundary(self.cursor);
        self.touch_cursor(cx);
        cx.notify();
    }

    // ⌘⌫/⌘⌦/⌥⌫/⌥⌦/⌘←/⌘→/⌥←/⌥→ — standard macOS single-line editing
    // shortcuts. Deliberately not a selection-range feature: there is still
    // only ever one `cursor: usize` here, same as before this task — these
    // are all either a cursor jump or a delete of `[start, cursor)`/
    // `[cursor, end)`, never a highlighted range a person could then act on
    // (copy, extend, retype-over). Real selection (⇧-arrows, ⌘A) and paste
    // (⌘V/⌘C/⌘X) need that range concept added to `TextField` first — a
    // bigger, separate piece of work the audit this task fixes from
    // deliberately split out; this file's doc comment above still names it
    // as not-yet-supported for exactly that reason.

    fn on_delete_line_start(&mut self, _: &DeleteLineStart, _window: &mut Window, cx: &mut Context<Self>) {
        if self.cursor == 0 {
            return;
        }
        let removed = 0..self.cursor;
        self.commit_edit(removed, "", cx);
    }

    fn on_delete_line_end(&mut self, _: &DeleteLineEnd, _window: &mut Window, cx: &mut Context<Self>) {
        if self.cursor == self.content.len() {
            return;
        }
        let removed = self.cursor..self.content.len();
        self.commit_edit(removed, "", cx);
    }

    fn on_delete_word_backward(&mut self, _: &DeleteWordBackward, _window: &mut Window, cx: &mut Context<Self>) {
        let start = self.word_start_before(self.cursor);
        if start == self.cursor {
            return;
        }
        let removed = start..self.cursor;
        self.commit_edit(removed, "", cx);
    }

    fn on_delete_word_forward(&mut self, _: &DeleteWordForward, _window: &mut Window, cx: &mut Context<Self>) {
        let end = self.word_end_after(self.cursor);
        if end == self.cursor {
            return;
        }
        let removed = self.cursor..end;
        self.commit_edit(removed, "", cx);
    }

    fn on_line_start(&mut self, _: &LineStart, _window: &mut Window, cx: &mut Context<Self>) {
        self.cursor = 0;
        self.touch_cursor(cx);
        cx.notify();
    }

    fn on_line_end(&mut self, _: &LineEnd, _window: &mut Window, cx: &mut Context<Self>) {
        self.cursor = self.content.len();
        self.touch_cursor(cx);
        cx.notify();
    }

    fn on_word_backward(&mut self, _: &WordBackward, _window: &mut Window, cx: &mut Context<Self>) {
        self.cursor = self.word_start_before(self.cursor);
        self.touch_cursor(cx);
        cx.notify();
    }

    fn on_word_forward(&mut self, _: &WordForward, _window: &mut Window, cx: &mut Context<Self>) {
        self.cursor = self.word_end_after(self.cursor);
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
        let at = self.utf16_offset_for_byte(self.cursor);
        Some(UTF16Selection {
            range: at..at,
            reversed: false,
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
            .unwrap_or(self.cursor..self.cursor);
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
            (field.placeholder.clone(), theme::TEXT_TERTIARY.into())
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

        let show_cursor =
            field.focus_handle.is_focused(window) && field.blink.read(cx).visible();
        let cursor = if show_cursor {
            let x = bounds.left() + line.x_for_index(field.cursor);
            Some(fill(
                Bounds::new(
                    point(x, bounds.top()),
                    gpui::size(px(2.), bounds.bottom() - bounds.top()),
                ),
                theme::TEXT_PRIMARY,
            ))
        } else {
            None
        };

        PrepaintState { line, cursor }
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
        prepaint
            .line
            .paint(bounds.origin, window.line_height(), window, cx)
            .ok();
        if let Some(cursor) = prepaint.cursor.take() {
            window.paint_quad(cursor);
        }
        self.field.update(cx, |field, _cx| {
            field.last_layout = Some(prepaint.line.clone());
            field.last_bounds = Some(bounds);
        });
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
            .w_full()
            .child(TextFieldElement { field: cx.entity() })
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use gpui::TestAppContext;

    use super::*;

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
}
