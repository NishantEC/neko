//! rune-ui registry: base 0.9.0 What every component shares: the press type, size metrics, the 40 point hit area, a glyph, the
//! spinner, the words beside a control, and `paint` re-exported from the style layer. `rune-ui init` copies it; yours to change.

use std::rc::Rc;

use rune::{Color, El, Icon, LoopSpec, TextStyle, col, el, icon, row, text};
pub use rune_ui_style::{ButtonVariant, Corners, Delta, Material, Parts, Recipe, Size, State, States, Theme, Tint, Tone, paint, paint_state, recipes, resolve};

use super::icons;

/// What a press does, shared so a component can hand it to the drawn control and to its hit area.
pub type Press<S> = Rc<dyn Fn(&mut S)>;

/// A press from a closure.
pub fn press<S: 'static>(f: impl Fn(&mut S) + 'static) -> Press<S> {
    Rc::new(f)
}

/// What a choice does with the index chosen.
pub type Choose<S> = Rc<dyn Fn(&mut S, usize)>;

/// What a choice does with the words or key chosen.
pub type Pick<S> = Rc<dyn Fn(&mut S, &str)>;

/// What a value control does with the new value.
pub type Slide<S> = Rc<dyn Fn(&mut S, f32)>;

/// Size metrics for controls.
pub trait SizeExt {
    fn height(self, t: &Theme) -> f32;
    fn text(self, t: &Theme, weight: f32) -> TextStyle;
    fn icon(self, t: &Theme) -> f32;
    /// Side padding for words; the icon's side gets 2 points less (optical alignment).
    fn pad(self, t: &Theme) -> f32;
    /// The corner radius, kept a rounded rectangle at this height.
    fn radius(self, t: &Theme) -> f32;
}

impl SizeExt for Size {
    fn height(self, t: &Theme) -> f32 {
        match self {
            Size::Sm => t.control.sm,
            Size::Lg => t.control.lg,
            _ => t.control.md,
        }
    }

    fn text(self, t: &Theme, weight: f32) -> TextStyle {
        let base = match self {
            Size::Sm => t.ty.sm,
            Size::Lg => t.ty.md,
            _ => t.ty.base,
        };
        TextStyle { weight, ..base }
    }

    fn icon(self, t: &Theme) -> f32 {
        match self {
            Size::Sm => t.control.icon_sm,
            Size::Lg => t.control.icon_lg,
            _ => t.control.icon,
        }
    }

    fn pad(self, t: &Theme) -> f32 {
        match self {
            Size::Sm => t.metrics.pad_sm,
            Size::Lg => t.metrics.pad_lg,
            _ => t.metrics.pad_md,
        }
    }

    fn radius(self, t: &Theme) -> f32 {
        rune_ui_style::Radius::fit(if self == Size::Lg { t.radius.lg } else { t.radius.md }, self.height(t))
    }
}

/// The theme's corner style on a rounded surface: Apple's continuous curve or a plain arc (`base.corners` in the theme file). Rune
/// draws the fill, border, focus ring (outline) and clip in that shape; drop and inner shadows keep circular corners (indistinguishable
/// at these blurs). Use it with radii from `t.radius` (continuous corners draw that ladder larger, `CONTINUOUS_GAIN`); leave circles,
/// dots, thumbs and pills without it, since a continuous curve turns a circle into a squircle.
pub trait Shape {
    fn round_as(self, t: &Theme) -> El;
}

impl Shape for El {
    fn round_as(self, t: &Theme) -> El {
        if t.corners == Corners::Continuous { self.continuous_corners() } else { self }
    }
}

/// The hit area round a drawn control: a wrapper as much larger as the control is short of the hit size on each axis, pulled back by
/// the same negative margin, so layout keeps the control's own size. Keyed `hit(<key>)`, not focusable and out of the accessibility
/// tree (role `none`: a keyed box with a press would otherwise read as an unnamed button round the named control); the control keeps
/// its key, name, focus ring and press. Neighbours must sit twice `x` apart.
pub fn hit_area<S: 'static>(key: &str, face: El, x: f32, y: f32, on_press: Option<Press<S>>) -> El {
    if x <= 0.0 && y <= 0.0 {
        return face;
    }
    let mut w = row().key(format!("hit({key})")).role("none").mx(-x).my(-y).px(x).py(y).center().child(face);
    if let Some(f) = on_press {
        w = w.on_press(move |s: &mut S| f(s));
    }
    w
}

/// An icon in one colour, hidden from assistive technology (the control it sits in carries the name).
pub fn glyph(i: &'static Icon, size: f32, color: Color) -> El {
    icon(i, size).color(color).a11y_hidden()
}

/// Work in progress: a faint ring with a turning arc. The one thing that loops; Rune stops it under Reduce Motion. Prefer
/// [`spinner_in`], which also holds still when the theme says Reduce Motion (`Theme::reduced_motion`).
pub fn spinner(size: f32, color: Color, label: &str) -> El {
    ring(size, color, label, true)
}

/// [`spinner`] in a theme: under Reduce Motion the arc holds still (a quarter of the ring, still saying busy).
pub fn spinner_in(t: &Theme, size: f32, color: Color, label: &str) -> El {
    ring(size, color, label, !t.reduced_motion)
}

fn ring(size: f32, color: Color, label: &str, turning: bool) -> El {
    let track = icon(&icons::RING, size).absolute(0.0, 0.0).color(color.alpha(0.25));
    let mut arc = icon(&icons::ARC, size).absolute(0.0, 0.0).color(color);
    if turning {
        arc = arc.looping(LoopSpec::rotation(1.0));
    }
    el().size(size, size).role("progressbar").label(label.to_owned()).busy(true).child(track).child(arc)
}

/// Words beside a control: the label in the first ink on one line, an optional hint under it in the second, wrapping past 320 points.
pub fn words(t: &Theme, label: &str, help: Option<&str>) -> El {
    let mut c = col().shrink().gap(1.0).child(text(label.to_owned()).font(t.ty.base).color(t.ink()).truncate().a11y_hidden());
    if let Some(h) = help {
        c = c.child(text(h.to_owned()).font(t.ty.sm).color(t.ink_muted()).wrap().max_w(320.0).a11y_hidden());
    }
    c
}

/// The view's root: `content`, then every overlay (menus, select lists, popovers, dialogs, sheets, toasts, a command palette) over it.
///
/// **Mount exactly one, at the root of the view.** Components that open something push it into a `layer: &mut Vec<El>` instead of
/// laying it out where they are; this is where the layer goes. Overlays here are out of the layout: they draw above every later sibling,
/// escape clipping and scrolling ancestors, stay anchored to their triggers as the page scrolls, flip above a trigger near the window's
/// bottom, and stay inside the window. Anywhere else (inside a scroll view, inside a row) they would be laid out in place and move the
/// content around them.
pub fn overlay_root(content: El, layer: Vec<El>) -> El {
    rune_kit::root(content, layer)
}

/// States from the usual builder flags plus a pinned look.
pub fn states(pinned: Option<State>, disabled: bool, selected: bool) -> States {
    let mut s = States { disabled, selected, ..States::default() };
    if let Some(p) = pinned {
        s.set(p, true);
    }
    s
}
