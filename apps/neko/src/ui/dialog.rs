//! rune-ui registry: dialog 0.5.0. Copied into your app by `rune-ui add dialog`; yours to change. Needs: button.
//!
//! A modal panel over a scrim: the **floating** material at the dialog's radius, a title, a description, a body, a footer of buttons, a
//! close button in its corner. Behaviour is Rune's `El::modal`: focus is trapped inside, the page behind is inert, Escape and a press
//! outside call `on_close`. The app owns whether it is open; `panel` draws it in place (galleries, a confirmation inline).
//!
//! ```ignore
//! Dialog::new("rename", "Rename project").description("A new name for the placeholder project.").child(input)
//!     .footer(vec![cancel, save]).open(s.renaming).on_close(|s: &mut App| s.renaming = false).build(t, &mut layer)
//! ```

use std::rc::Rc;

use rune::{El, col, el, row, text};
use rune_ui_style::{ButtonVariant, weight};

use super::base::{Press, Shape, Theme, paint};
use super::button::Button;
use super::icons;

pub struct Dialog<S: 'static> {
    key: String,
    title: String,
    description: Option<String>,
    body: Vec<El>,
    footer: Vec<El>,
    width: f32,
    open: bool,
    outside: bool,
    return_to: Option<String>,
    close: Option<Press<S>>,
}

impl<S: 'static> Dialog<S> {
    pub fn new(key: impl Into<String>, title: impl Into<String>) -> Dialog<S> {
        Dialog { key: key.into(), title: title.into(), description: None, body: Vec::new(), footer: Vec::new(), width: 440.0, open: false, outside: true, return_to: None, close: None }
    }

    pub fn description(mut self, d: impl Into<String>) -> Self {
        self.description = Some(d.into());
        self
    }

    pub fn child(mut self, c: El) -> Self {
        self.body.push(c);
        self
    }

    /// The buttons at the bottom right, least important first.
    pub fn footer(mut self, buttons: Vec<El>) -> Self {
        self.footer = buttons;
        self
    }

    pub fn width(mut self, w: f32) -> Self {
        self.width = w;
        self
    }

    pub fn open(mut self, open: bool) -> Self {
        self.open = open;
        self
    }

    /// Whether a press outside closes it (on by default; `AlertDialog` turns it off).
    pub fn click_outside(mut self, on: bool) -> Self {
        self.outside = on;
        self
    }

    /// The key of what opened it: focus goes back there when it closes.
    pub fn return_focus(mut self, key: impl Into<String>) -> Self {
        self.return_to = Some(key.into());
        self
    }

    /// Escape, a press outside, or the close button.
    pub fn on_close(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.close = Some(Rc::new(f));
        self
    }

    /// The panel alone, drawn in place.
    pub fn panel(&mut self, t: &Theme) -> El {
        let mut head = col().key(format!("{}-header", self.key)).grow(1.0).shrink().min_w(0.0).gap(t.space.at(1.5)).child(text(self.title.clone()).key(format!("{}-title", self.key)).font(weight(t.ty.lg, t.ty.semibold)).color(t.c.floating.fg).role("heading").level(2).wrap().min_w(0.0));
        if let Some(d) = &self.description {
            head = head.child(text(d.clone()).font(t.ty.base).color(t.ink_muted()).wrap().min_w(0.0));
        }
        let mut top = row().w_full().gap(t.space.at(3.0)).items_start().child(head);
        if let Some(f) = self.close.clone().filter(|_| self.outside) {
            top = top.child(Button::<S>::new(format!("{}-close", self.key), "Close").variant(ButtonVariant::Ghost).size(rune_ui_style::Size::Sm).icon_only(&icons::CLOSE).on_press(move |s: &mut S| f(s)).build(t));
        }
        let mut p = col().key(format!("{}-dialog", self.key)).role("dialog").label(self.title.clone()).w_full().max_w(self.width).padding(t.metrics.panel).gap(t.metrics.stack - 4.0).radius(t.radius.xxl).round_as(t);
        if let Some(d) = &self.description {
            p = p.description(d.clone());
        }
        p = paint(p, &rune_ui_style::Recipe::new().fill(t.c.floating.bg).shadows(&t.elevation.lg).shadow(rune::Shadow { spread: 1.0, color: t.c.line, ..rune::Shadow::default() }));
        p = p.child(top).children(std::mem::take(&mut self.body));
        if !self.footer.is_empty() {
            p = p.child(row().key(format!("{}-footer", self.key)).w_full().gap(t.space.at(2.0)).justify_end().pt(t.space.at(1.0)).children(std::mem::take(&mut self.footer)));
        }
        p
    }

    /// Nothing in place; while open, the scrim and the panel go into `layer`.
    pub fn build(mut self, t: &Theme, layer: &mut Vec<El>) {
        if !self.open {
            return;
        }
        let key = self.key.clone();
        let close = self.close.clone();
        let outside = self.outside;
        let return_to = self.return_to.clone();
        let mut panel = self.panel(t);
        if let Some(k) = return_to {
            panel = panel.focus_return(k);
        }
        if let Some(f) = close {
            panel = panel.modal(outside, move |s: &mut S| f(s));
        }
        layer.push(el().key(format!("{key}-scrim")).absolute(0.0, 0.0).w_full().h_full().fill(t.c.page.fg.alpha(if t.dark { 0.5 } else { 0.18 })).a11y_hidden());
        layer.push(el().key(format!("{key}-layer")).absolute(0.0, 0.0).w_full().h_full().padding(t.space.at(4.0)).center().child(panel));
    }
}
