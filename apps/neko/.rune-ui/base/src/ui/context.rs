//! rune-ui registry: context 0.6.0. Copied into your app by `rune-ui add context`; yours to change.
//!
//! How full the model's context is, and what it has cost: a small ghost button with a bar (a well holding an ink fill) and the share in
//! tabular figures; pressing it opens a popover from the shared overlay layer with the breakdown (input, output, cache read, the total
//! against the limit, and cost). The bar takes the warning ink past 80% and the danger ink past 95%: colour as state.
//!
//! ```ignore
//! Context::new("ctx", &usage, s.open == Some("ctx")).on_open(|s: &mut App, o| s.open = o.then(|| "ctx")).build(t, &mut layer)
//! ```

use std::rc::Rc;

use rune::{El, col, el, row, text};
use rune_kit::Placement;
use rune_ui_style::{Recipe, tabular, weight};

use super::agent::{money, tokens};
use super::base::{Shape, Theme, hit_area, paint, paint_state};

/// Token counts and cost so far.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Usage {
    pub input: u64,
    pub output: u64,
    pub cache: u64,
    /// The model's context window.
    pub limit: u64,
    /// Dollars.
    pub cost: f64,
}

impl Usage {
    pub fn used(&self) -> u64 {
        self.input + self.output + self.cache
    }

    /// Used over the limit, 0 to 1.
    pub fn share(&self) -> f32 {
        if self.limit == 0 { 0.0 } else { (self.used() as f32 / self.limit as f32).clamp(0.0, 1.0) }
    }
}

/// What opening or closing does (`true`: opened).
pub type Toggle<S> = Rc<dyn Fn(&mut S, bool)>;

pub struct Context<'a, S: 'static> {
    key: String,
    usage: &'a Usage,
    open: bool,
    change: Option<Toggle<S>>,
}

impl<'a, S: 'static> Context<'a, S> {
    pub fn new(key: impl Into<String>, usage: &'a Usage, open: bool) -> Context<'a, S> {
        Context { key: key.into(), usage, open, change: None }
    }

    /// Opened (true) or closed (false: a press outside, Escape, or the trigger again).
    pub fn on_open(mut self, f: impl Fn(&mut S, bool) + 'static) -> Self {
        self.change = Some(Rc::new(f));
        self
    }

    fn bar(t: &Theme, share: f32, w: f32) -> El {
        let ink = if share > 0.95 { t.c.danger.solid } else if share > 0.8 { t.c.warning.solid } else { t.c.primary.bg };
        paint(row().w(w).h(6.0).radius(3.0), &t.mat.well.base).child(paint(el().w_frac(share).h(6.0).radius(3.0), &Recipe::new().fill(ink)))
    }

    pub fn build(self, t: &Theme, layer: &mut Vec<El>) -> El {
        let u = self.usage;
        let share = u.share();
        let pct = format!("{:.0}%", share * 100.0);
        let h = t.control.sm;
        let trigger = format!("{}-trigger", self.key);
        let mut face = row().key(trigger.clone()).role("meter").label(format!("Context {pct} used")).range_value(f64::from(share * 100.0), 0.0, 100.0).expanded(self.open).focusable(true).h(h).px(t.space.at(2.0)).gap(t.space.at(2.0)).center_y().radius(rune_ui_style::Radius::fit(t.radius.md, h)).round_as(t);
        face = paint_state(face, &t.mat.ghost, Default::default()).child(Context::<S>::bar(t, share, 28.0)).child(text(pct.clone()).font(tabular(t.ty.sm)).color(t.ink_muted()).a11y_hidden());
        let toggle = self.change.clone().map(|f| {
            let open = self.open;
            super::base::press(move |s: &mut S| f(s, !open))
        });
        if let Some(p) = toggle.clone() {
            face = face.on_press(move |s: &mut S| p(s));
        }
        if self.open {
            let line = |name: &str, value: String| row().w_full().h(t.space.at(6.0)).center_y().child(text(name.to_owned()).font(t.ty.sm).color(t.ink_muted()).grow(1.0)).child(text(value).font(tabular(t.ty.sm)).color(t.c.floating.fg));
            let mut panel = col().key(format!("{}-panel", self.key)).role("dialog").label("Context usage").w(260.0).padding(t.metrics.pad_md - 2.0).gap(t.space.at(1.0)).radius(t.radius.lg).round_as(t);
            panel = paint(panel, &t.mat.floating.base)
                .child(row().w_full().items_end().child(text("Context").font(weight(t.ty.base, t.ty.semibold)).color(t.c.floating.fg).grow(1.0)).child(text(format!("{} / {}", tokens(u.used()), tokens(u.limit))).font(tabular(t.ty.sm)).color(t.ink_muted())))
                .child(row().w_full().py(t.space.at(1.5)).child(Context::<S>::bar(t, share, 232.0)))
                .child(line("Input", tokens(u.input)))
                .child(line("Output", tokens(u.output)))
                .child(line("Cache read", tokens(u.cache)))
                .child(el().w_full().h(1.0).fill(t.c.line).a11y_hidden())
                .child(line("Cost", money(u.cost)));
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
