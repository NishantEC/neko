//! Toasts: the lines the window says for a moment ("Renamed.", "Saved to Downloads"). Bottom centre, 40 high, radius
//! 12, at most three stacked (the oldest goes when a fourth comes), four seconds each. A toast may carry one button
//! (an "Undo" wherever the web offers one; the web offers none on a toast today, so none is built here), which sends a
//! request when pressed and puts the toast away.

use rune::{Color, El, TextStyle, col, row, text};

use crate::app::App;
use crate::link::Call;
use crate::theme::Neko;

/// How long a toast stays, from the first frame it is on screen.
pub const SECONDS: f64 = 4.0;
/// The most toasts on screen at once.
pub const MAX: usize = 3;
/// A toast's height and radius.
pub const HEIGHT: f32 = 40.0;
pub const RADIUS: f32 = 12.0;

#[derive(Clone, Debug, PartialEq)]
pub struct ToastItem {
    pub id: u64,
    pub words: String,
    /// A button at the right: its label, and the request it sends.
    pub action: Option<(String, Call)>,
    /// When it goes, by the frame clock; set the first tick after it shows.
    pub until: Option<f64>,
}

impl App {
    /// Says `words` in a toast. The same words already up are not stacked again; they stay a while longer.
    pub fn show_toast(&mut self, words: &str) {
        self.push_toast(words, None);
    }

    /// A toast with one button; pressing it sends `call` (an Undo, an Open).
    pub fn toast_with(&mut self, words: &str, label: &str, call: Call) {
        self.push_toast(words, Some((label.to_owned(), call)));
    }

    fn push_toast(&mut self, words: &str, action: Option<(String, Call)>) {
        if let Some(same) = self.toasts.iter_mut().find(|t| t.words == words) {
            same.until = None;
            same.action = action;
            return;
        }
        self.toast_counter += 1;
        self.toasts.push(ToastItem { id: self.toast_counter, words: words.to_owned(), action, until: None });
        while self.toasts.len() > MAX {
            self.toasts.remove(0);
        }
    }

    /// What the toasts say, one line each (for tests and the screen reader).
    pub fn toast_words(&self) -> String {
        self.toasts.iter().map(|t| t.words.as_str()).collect::<Vec<_>>().join("\n")
    }

    /// The toast's button: sends its request and puts the toast away.
    pub fn press_toast(&mut self, id: u64) {
        let Some(at) = self.toasts.iter().position(|t| t.id == id) else { return };
        let item = self.toasts.remove(at);
        if let Some((_, call)) = item.action {
            self.call(call);
        }
    }

    pub fn dismiss_toast(&mut self, id: u64) {
        self.toasts.retain(|t| t.id != id);
    }

    /// Once a frame: starts each new toast's clock and takes the old ones down. True when what shows changed.
    pub fn tick_toast(&mut self, t: f64, tx: &mut rune::Tx) -> bool {
        if self.toasts.is_empty() {
            return false;
        }
        let mut changed = false;
        let mut next: Option<f64> = None;
        for item in &mut self.toasts {
            if item.until.is_none() {
                item.until = Some(t + SECONDS);
                changed = true;
            }
            let at = item.until.unwrap_or(t + SECONDS);
            next = Some(next.map_or(at, |n: f64| n.min(at)));
        }
        let before = self.toasts.len();
        self.toasts.retain(|i| i.until.is_none_or(|at| t < at));
        changed |= self.toasts.len() != before;
        if let Some(at) = next.filter(|_| !self.toasts.is_empty()) {
            tx.wake_after(std::time::Duration::from_secs_f64((at - t).max(0.05)));
        }
        changed
    }
}

/// The stack, at the bottom centre of the window.
pub fn mount(s: &App, t: &Neko, layer: &mut Vec<El>) {
    if s.toasts.is_empty() {
        return;
    }
    let mut stack = col().key("toast-region").role("region").label("Notifications").pin(Some(0.0), None, Some(0.0), Some(20.0)).items_center().gap(8.0);
    for item in &s.toasts {
        let id = item.id;
        let mut r = row()
            .key(format!("toast-{id}"))
            .role("status")
            .label(item.words.clone())
            .h(HEIGHT)
            .px(14.0)
            .gap(12.0)
            .center_y()
            .radius(RADIUS)
            .fill(t.kit.c.surface_raised)
            .border(1.0, t.surface.field_border)
            .transition(rune::Transition::none())
            .motion(rune::Spring::NONE)
            .child(text(item.words.clone()).font(t.ty.row).color(t.text.base).truncate().max_w(520.0).a11y_hidden());
        if let Some((label, _)) = &item.action {
            r = r.child(
                row()
                    .key(format!("toast-action-{id}"))
                    .role("button")
                    .label(label.clone())
                    .focusable(true)
                    .h(28.0)
                    .px(8.0)
                    .center()
                    .radius(8.0)
                    .hover(t.surface.hover)
                    .focus_ring(2.0, 2.0, t.surface.ring)
                    .on_press(move |s: &mut App| s.press_toast(id))
                    .child(text(label.clone()).font(TextStyle { weight: 500.0, ..t.ty.row }).color(t.meaning.iris).a11y_hidden()),
            );
        }
        stack = stack.child(r);
    }
    let _ = Color::TRANSPARENT;
    layer.push(stack);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn at_most_three_stack_and_the_oldest_goes_first() {
        let mut app = App::headless();
        for words in ["one", "two", "three", "four"] {
            app.show_toast(words);
        }
        assert_eq!(app.toast_words(), "two\nthree\nfour");
    }

    #[test]
    fn the_same_words_are_not_stacked_twice() {
        let mut app = App::headless();
        app.show_toast("Renamed.");
        app.show_toast("Renamed.");
        assert_eq!(app.toasts.len(), 1);
    }

    #[test]
    fn a_button_sends_its_request_and_takes_the_toast_down() {
        let mut app = App::headless();
        app.toast_with("Stopped 3 tasks.", "Undo", Call::Unseen);
        let id = app.toasts[0].id;
        app.press_toast(id);
        assert!(app.toasts.is_empty());
        assert_eq!(app.outbox, vec![Call::Unseen]);
    }
}
