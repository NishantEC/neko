use std::ops::Range;

use gpui::{
    App, Bounds, Context, CursorStyle, ElementId, ElementInputHandler, Entity, EntityInputHandler,
    FocusHandle, Focusable, GlobalElementId, InspectorElementId, LayoutId, Pixels, Point, Render,
    ShapedLine, SharedString, Style, TextRun, UTF16Selection, Window, actions, div, fill, point,
    prelude::*, px, relative,
};

use crate::components::vendor::gpui_component::blink_cursor::CursorBlink;
use crate::theme;

actions!(text_field, [Backspace, Left, Right]);

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

impl TextField {
    pub fn new(cx: &mut App) -> Entity<Self> {
        cx.new(|cx| {
            let blink = cx.new(|_| CursorBlink::new());
            blink.update(cx, |blink, cx| blink.start(cx));
            cx.observe(&blink, |_, _, cx| cx.notify()).detach();
            Self {
                focus_handle: cx.focus_handle(),
                content: String::new(),
                placeholder: "Search apps and clipboard…".into(),
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
    pub fn clear(&mut self, cx: &mut Context<Self>) {
        self.content.clear();
        self.cursor = 0;
        cx.notify();
    }

    fn touch_cursor(&mut self, cx: &mut Context<Self>) {
        self.blink.update(cx, |blink, cx| blink.pause(cx));
    }

    fn on_backspace(&mut self, _: &Backspace, window: &mut Window, cx: &mut Context<Self>) {
        if self.cursor == 0 {
            return;
        }
        let prev = self.previous_char_boundary(self.cursor);
        let removed = prev..self.cursor;
        self.cursor = prev;
        self.replace_text_in_range(Some(self.range_to_utf16(&removed)), "", window, cx);
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
        self.content.replace_range(range.clone(), new_text);
        self.cursor = range.start + new_text.len();
        self.touch_cursor(cx);
        cx.notify();
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
            .w_full()
            .child(TextFieldElement { field: cx.entity() })
    }
}
