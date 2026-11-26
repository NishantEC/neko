//! rune-ui registry: context 0.9.0. Copied into your app by `rune-ui add context`; yours to change.
//!
//! How full the model's context is, and what it has cost. The trigger is a small ghost button holding a **ring** (a 20 point circle
//! whose quiet stroke fills clockwise from the top; the default) or a **bar** (a well holding an ink fill), with the share in tabular
//! figures beside it (optional). Pressing it, or Return or Space while it has focus, opens a popover from the shared overlay layer:
//! the context used against its window, the token totals (in, out, cached), the cost when there is one (no cost, no row), and an
//! optional note line. Escape or a press outside closes it and focus goes back to the trigger. The fill takes the warning ink from
//! 80% and the danger ink from 95%: colour as state.
//!
//! ```ignore
//! let usage = Usage { context: 86_000, limit: 200_000, input: 48_200, output: 6_100, cache: 31_000, cost: Some(0.42) };
//! Context::new("ctx", &usage, s.open == Some("ctx")).on_open(|s: &mut App, o| s.open = o.then(|| "ctx")).build(t, &mut layer)
//! Context::new("ctx", &usage, open).face(Face::Bar).note("Billed to Acme").build(t, &mut layer)
//! ```

use std::f32::consts::{PI, TAU};
use std::rc::Rc;

use rune::{Color, El, KeyEvent, Shape, Tx, canvas, col, el, row, text};
use rune_kit::Placement;
use rune_ui_style::{Recipe, tabular, weight};

use super::agent::{money, return_presses, tokens};
use super::base::{Shape as _, Theme, hit_area, paint, paint_state};

/// The context and what it has cost so far.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Usage {
    /// Tokens in the context now: what the share is of.
    pub context: u64,
    /// The model's context window.
    pub limit: u64,
    /// Tokens sent, in all.
    pub input: u64,
    /// Tokens received, in all.
    pub output: u64,
    /// Tokens read from the cache, in all.
    pub cache: u64,
    /// Dollars; `None` draws no cost row.
    pub cost: Option<f64>,
}

impl Usage {
    /// The context used over its window, 0 to 1.
    pub fn share(&self) -> f32 {
        if self.limit == 0 { 0.0 } else { (self.context as f32 / self.limit as f32).clamp(0.0, 1.0) }
    }
}

/// How the trigger shows the share.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Face {
    /// A small circular ring filling clockwise from the top.
    #[default]
    Ring,
    /// A short bar: a well holding an ink fill.
    Bar,
}

/// What opening or closing does (`true`: opened).
pub type Toggle<S> = Rc<dyn Fn(&mut S, bool)>;

pub struct Context<'a, S: 'static> {
    key: String,
    usage: &'a Usage,
    open: bool,
    face: Face,
    percent: bool,
    note: Option<String>,
    change: Option<Toggle<S>>,
}

impl<'a, S: 'static> Context<'a, S> {
    pub fn new(key: impl Into<String>, usage: &'a Usage, open: bool) -> Context<'a, S> {
        Context { key: key.into(), usage, open, face: Face::Ring, percent: true, note: None, change: None }
    }

    /// The ring (the default) or the bar.
    pub fn face(mut self, f: Face) -> Self {
        self.face = f;
        self
    }

    /// The share in figures beside the face (on by default).
    pub fn percent(mut self, on: bool) -> Self {
        self.percent = on;
        self
    }

    /// One quiet line at the foot of the breakdown, under the cost ("Billed to Acme", "Resets at 14:00").
    pub fn note(mut self, words: impl Into<String>) -> Self {
        self.note = Some(words.into());
        self
    }

    /// Opened (true) or closed (false: a press outside, Escape, or the trigger again).
    pub fn on_open(mut self, f: impl Fn(&mut S, bool) + 'static) -> Self {
        self.change = Some(Rc::new(f));
        self
    }

    /// The fill's ink: quiet until 80%, the warning ink from 80%, the danger ink from 95%.
    fn ink(t: &Theme, share: f32, quiet: Color) -> Color { // guard: allow (a return type, not a colour)
        if share >= 0.95 { t.c.danger.solid } else if share >= 0.8 { t.c.warning.solid } else { quiet }
    }

    fn bar(t: &Theme, share: f32, w: f32) -> El {
        let ink = Context::<S>::ink(t, share, t.c.primary.bg);
        paint(row().w(w).h(6.0).radius(3.0), &t.mat.well.base).child(paint(el().w_frac(share).h(6.0).radius(3.0), &Recipe::new().fill(ink)))
    }

    /// A ring `d` points across: a faint track, and the share as an arc from twelve o'clock, clockwise, with round ends.
    pub fn ring(t: &Theme, share: f32, d: f32) -> El {
        let w = (d / 8.0).max(2.0);
        let (c, r) = (d / 2.0, d / 2.0 - w / 2.0);
        let point = |a: f32| [c + r * a.cos(), c + r * a.sin()];
        let track: Vec<[f32; 2]> = (0..=64).map(|i| point(i as f32 / 64.0 * TAU)).collect();
        let mut shape = Shape::new().stroke(track, w, t.ink_faint().alpha(0.16));
        if share > 0.0 {
            let sweep = share.min(1.0) * TAU;
            let steps = ((sweep / 0.08).ceil() as usize).max(2);
            let arc: Vec<[f32; 2]> = (0..=steps).map(|i| point(-PI / 2.0 + sweep * i as f32 / steps as f32)).collect();
            shape = shape.stroke(arc, w, Context::<S>::ink(t, share, t.ink_muted()));
        }
        canvas(shape).size(d, d).a11y_hidden()
    }

    pub fn build(self, t: &Theme, layer: &mut Vec<El>) -> El {
        let u = self.usage;
        let share = u.share();
        let pct = format!("{:.0}%", share * 100.0);
        let h = t.control.sm;
        let trigger = format!("{}-trigger", self.key);
        let name = format!("Context {pct} used, {} of {} tokens", tokens(u.context), tokens(u.limit));
        let mut face = row().key(trigger.clone()).role("meter").label(name).range_value(f64::from(share * 100.0), 0.0, 100.0).expanded(self.open).focusable(true).h(h).px(t.space.at(if self.face == Face::Ring { 1.5 } else { 2.0 })).gap(t.space.at(1.5)).center_y().radius(rune_ui_style::Radius::fit(t.radius.md, h)).round_as(t);
        face = paint_state(face, &t.mat.ghost, Default::default()).child(match self.face {
            Face::Ring => Context::<S>::ring(t, share, 20.0),
            Face::Bar => Context::<S>::bar(t, share, 28.0),
        });
        if self.percent {
            face = face.child(text(pct.clone()).font(tabular(t.ty.sm)).color(t.ink_muted()).a11y_hidden());
        }
        let toggle = self.change.clone().map(|f| {
            let open = self.open;
            super::base::press(move |s: &mut S| f(s, !open))
        });
        if let Some(p) = toggle.clone() {
            let q = p.clone();
            face = face.on_press(move |s: &mut S| p(s)).on_key(move |s: &mut S, k: &KeyEvent, _: &mut Tx| {
                if k.key == rune::Key::Text(" ".to_owned()) && k.mods == rune::Mods::default() {
                    q(s);
                    return true;
                }
                return_presses(k, || q(s))
            });
        }
        if self.open {
            let line = |name: &str, value: String| row().w_full().h(t.space.at(6.0)).center_y().child(text(name.to_owned()).font(t.ty.sm).color(t.ink_muted()).grow(1.0)).child(text(value).font(tabular(t.ty.sm)).color(t.c.floating.fg));
            let rule = || el().w_full().h(1.0).my(t.space.at(1.0)).fill(t.c.line).a11y_hidden();
            let mut panel = col().key(format!("{}-panel", self.key)).role("dialog").label("Context usage").w(260.0).padding(t.metrics.pad_md - 2.0).gap(t.space.at(1.0)).radius(t.radius.lg).round_as(t);
            panel = paint(panel, &t.mat.floating.base)
                .child(row().w_full().items_end().child(text("Context").font(weight(t.ty.base, t.ty.semibold)).color(t.c.floating.fg).grow(1.0)).child(text(pct).font(tabular(t.ty.sm)).color(t.ink_muted())))
                .child(row().w_full().py(t.space.at(1.5)).child(Context::<S>::bar(t, share, 232.0)))
                .child(line("Used", tokens(u.context)))
                .child(line("Window", tokens(u.limit)))
                .child(rule())
                .child(line("In", tokens(u.input)))
                .child(line("Out", tokens(u.output)))
                .child(line("Cached", tokens(u.cache)));
            if let Some(c) = u.cost {
                panel = panel.child(rule()).child(line("Cost", money(c)));
            }
            if let Some(n) = &self.note {
                panel = panel.child(text(n.clone()).key(format!("{}-note", self.key)).font(t.ty.xs).color(t.ink_faint()).wrap().min_w(0.0).pt(t.space.at(1.0)));
            }
            let mut panel = panel.focus_return(&trigger);
            if let Some(f) = self.change.clone() {
                let g = f.clone();
                panel = panel.modal(true, move |s: &mut S| g(s, false));
                layer.push(rune_kit::scrim(format!("{}-scrim", self.key), move |s: &mut S| f(s, false)));
            }
            layer.push(rune_kit::popover::anchored(t.to_kit(), panel, &trigger, Placement::TopEnd, t.space.at(1.5)));
        }
        hit_area(&trigger, face, 0.0, t.control.slop(h), toggle)
    }
}
