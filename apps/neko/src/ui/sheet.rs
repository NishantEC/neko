//! rune-ui registry: sheet 0.10.0 Copied into your app by `rune-ui add sheet`; yours to change.
//!
//! A panel against one edge of the window over a scrim (settings, details, a form that needs room). The **floating** material, rounded
//! only on its free edges, a title and a close button, then its body. Behaviour is Rune's `El::modal` (focus trapped, Escape and a press
//! outside close it). `panel` draws it in place.
//!
//! ```ignore
//! Sheet::new("details", "Details").side(Side::Right).child(body).open(s.details).on_close(|s: &mut App| s.details = false).build(t, &mut layer)
//! ```

use std::rc::Rc;

use rune::{El, col, el, row, text};
use rune::Shadow;
use rune_ui_style::{ButtonVariant, Recipe, weight};

use super::base::{Press, Shape, Theme, paint};
use super::icons;

/// Which edge the sheet sits against.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Side {
    Left,
    #[default]
    Right,
    Bottom,
}

pub struct Sheet<S: 'static> {
    key: String,
    title: String,
    description: Option<String>,
    body: Vec<El>,
    side: Side,
    size: f32,
    open: bool,
    return_to: Option<String>,
    close: Option<Press<S>>,
}

impl<S: 'static> Sheet<S> {
    pub fn new(key: impl Into<String>, title: impl Into<String>) -> Sheet<S> {
        Sheet { key: key.into(), title: title.into(), description: None, body: Vec::new(), side: Side::Right, size: 380.0, open: false, return_to: None, close: None }
    }

    pub fn description(mut self, d: impl Into<String>) -> Self {
        self.description = Some(d.into());
        self
    }

    pub fn child(mut self, c: El) -> Self {
        self.body.push(c);
        self
    }

    pub fn side(mut self, s: Side) -> Self {
        self.side = s;
        self
    }

    /// Width (left, right) or height (bottom).
    pub fn size(mut self, points: f32) -> Self {
        self.size = points;
        self
    }

    pub fn open(mut self, open: bool) -> Self {
        self.open = open;
        self
    }

    /// The key of what opened it: focus goes back there when it closes.
    pub fn return_focus(mut self, key: impl Into<String>) -> Self {
        self.return_to = Some(key.into());
        self
    }

    pub fn on_close(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.close = Some(Rc::new(f));
        self
    }

    /// The panel alone (`height` points tall for a side sheet drawn in place).
    pub fn panel(&mut self, t: &Theme) -> El {
        let r = t.radius.xxl;
        let corners = match self.side {
            Side::Right => [r, 0.0, 0.0, r],
            Side::Left => [0.0, r, r, 0.0],
            Side::Bottom => [r, r, 0.0, 0.0],
        };
        let mut head = col().grow(1.0).shrink().min_w(0.0).gap(t.space.at(1.0)).child(text(self.title.clone()).font(weight(t.ty.lg, t.ty.semibold)).color(t.c.floating.fg).role("heading").level(2));
        if let Some(d) = &self.description {
            head = head.child(text(d.clone()).font(t.ty.base).color(t.ink_muted()).wrap().min_w(0.0));
        }
        let mut top = row().w_full().items_start().gap(t.space.at(3.0)).child(head);
        if let Some(f) = self.close.clone() {
            top = top.child(super::button::Button::<S>::new(format!("{}-close", self.key), "Close").variant(ButtonVariant::Ghost).size(rune_ui_style::Size::Sm).icon_only(&icons::CLOSE).on_press(move |s: &mut S| f(s)).build(t));
        }
        let mut p = col().key(format!("{}-sheet", self.key)).role("dialog").label(self.title.clone()).padding(t.metrics.panel).gap(t.metrics.stack - 4.0).corners(corners).round_as(t);
        p = match self.side {
            Side::Bottom => p.w_full().h(self.size),
            _ => p.w(self.size).h_full(),
        };
        p = paint(p, &Recipe::new().fill(t.c.floating.bg).shadows(&t.elevation.lg).shadow(Shadow { spread: 1.0, color: t.c.line, ..Shadow::default() }));
        p.child(top).children(std::mem::take(&mut self.body))
    }

    pub fn build(mut self, t: &Theme, layer: &mut Vec<El>) {
        if !self.open {
            return;
        }
        let (key, close, side, return_to) = (self.key.clone(), self.close.clone(), self.side, self.return_to.clone());
        let panel = self.panel(t);
        mount(t, layer, &key, panel, side, return_to.as_deref(), close);
    }
}

/// Mounts `panel` into `layer` against `side` of the window, over a scrim: focus trapped, Escape and a press outside call `close`,
/// focus going back to `return_to`. Sheet and Drawer share it.
pub fn mount<S: 'static>(t: &Theme, layer: &mut Vec<El>, key: &str, panel: El, side: Side, return_to: Option<&str>, close: Option<Press<S>>) {
    let mut panel = panel;
    if let Some(k) = return_to {
        panel = panel.focus_return(k);
    }
    if let Some(f) = close {
        panel = panel.modal(true, move |s: &mut S| f(s));
    }
    layer.push(el().key(format!("{key}-scrim")).absolute(0.0, 0.0).w_full().h_full().fill(t.scrim()).a11y_hidden());
    let holder = el().key(format!("{key}-layer")).absolute(0.0, 0.0).w_full().h_full();
    layer.push(match side {
        Side::Right => holder.justify_end().child(panel),
        Side::Left => holder.justify_start().child(panel),
        Side::Bottom => col().key(format!("{key}-layer")).absolute(0.0, 0.0).w_full().h_full().justify_end().child(panel),
    });
}
