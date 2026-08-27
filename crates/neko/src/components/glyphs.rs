//! Hand-painted glyphs, for symbols the design report found unreliable as
//! font text (§4 finding 5, §6): "the ⌥ (U+2325) glyph has no reliable font
//! rendering... found the hard way building the onboarding hotkey screen."
//! `panel.rs`'s `search_glyph` already established the pattern of a small
//! painted shape instead of a trusted Unicode codepoint; `opt_glyph` here
//! traces the mockups' own hand-drawn SVG path
//! (`M4 15h4.5L14.5 5H18`, from a 20×20 viewBox) with GPUI's vector path
//! API instead, so onboarding doesn't need an embedded SVG asset pipeline
//! for one glyph.

use gpui::{PathBuilder, Pixels, Rgba, canvas, point, prelude::*, px};

pub fn opt_glyph(size: Pixels, color: Rgba) -> impl IntoElement {
    canvas(
        move |_bounds, _window, _cx| (),
        move |bounds, (), window, _cx| {
            let scale = f32::from(bounds.size.width) / 20.0;
            let ox = f32::from(bounds.origin.x);
            let oy = f32::from(bounds.origin.y);
            let pt = |x: f32, y: f32| point(px(ox + x * scale), px(oy + y * scale));

            let mut builder = PathBuilder::stroke(px((1.6f32 * scale).max(1.0)));
            builder.move_to(pt(4.0, 15.0));
            builder.line_to(pt(8.5, 15.0));
            builder.line_to(pt(14.5, 5.0));
            builder.line_to(pt(18.0, 5.0));
            if let Ok(path) = builder.build() {
                window.paint_path(path, color);
            }
        },
    )
    .w(size)
    .h(size)
}
