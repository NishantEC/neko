//! rune-ui registry: alert_dialog 0.5.0. Copied into your app by `rune-ui add alert_dialog`; yours to change. Needs: button, dialog.
//!
//! A dialog that needs an answer: a press outside does not close it (Escape still means "cancel"), it has no close button, and its two
//! answers are explicit. The destructive form makes the action the danger colour. Built on `Dialog`.
//!
//! ```ignore
//! AlertDialog::new("delete", "Delete the project?").description("This cannot be undone.").destructive()
//!     .cancel("Keep it", |s: &mut App| s.asking = false).action("Delete", |s: &mut App| s.delete()).open(s.asking).build(t, &mut layer)
//! ```

use std::rc::Rc;

use rune::El;
use rune_ui_style::ButtonVariant;

use super::base::{Press, Theme};
use super::button::Button;
use super::dialog::Dialog;

type Answer<S> = Option<(String, Press<S>)>;

pub struct AlertDialog<S: 'static> {
    key: String,
    title: String,
    description: Option<String>,
    destructive: bool,
    open: bool,
    return_to: Option<String>,
    cancel: Answer<S>,
    action: Answer<S>,
}

impl<S: 'static> AlertDialog<S> {
    pub fn new(key: impl Into<String>, title: impl Into<String>) -> AlertDialog<S> {
        AlertDialog { key: key.into(), title: title.into(), description: None, destructive: false, open: false, return_to: None, cancel: None, action: None }
    }

    pub fn description(mut self, d: impl Into<String>) -> Self {
        self.description = Some(d.into());
        self
    }

    /// The action is the danger colour.
    pub fn destructive(mut self) -> Self {
        self.destructive = true;
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

    /// The safe answer (also what Escape does).
    pub fn cancel(mut self, words: impl Into<String>, f: impl Fn(&mut S) + 'static) -> Self {
        self.cancel = Some((words.into(), Rc::new(f)));
        self
    }

    pub fn action(mut self, words: impl Into<String>, f: impl Fn(&mut S) + 'static) -> Self {
        self.action = Some((words.into(), Rc::new(f)));
        self
    }

    fn dialog(self, t: &Theme) -> Dialog<S> {
        let mut buttons = Vec::new();
        let mut d = Dialog::<S>::new(self.key.clone(), self.title).width(400.0).click_outside(false).open(self.open);
        if let Some(k) = self.return_to {
            d = d.return_focus(k);
        }
        if let Some(desc) = self.description {
            d = d.description(desc);
        }
        if let Some((words, f)) = self.cancel {
            let g = f.clone();
            buttons.push(Button::<S>::new(format!("{}-cancel", self.key), words).variant(ButtonVariant::Secondary).on_press(move |s: &mut S| g(s)).build(t));
            d = d.on_close(move |s: &mut S| f(s));
        }
        if let Some((words, f)) = self.action {
            buttons.push(Button::<S>::new(format!("{}-action", self.key), words).variant(if self.destructive { ButtonVariant::Destructive } else { ButtonVariant::Default }).on_press(move |s: &mut S| f(s)).build(t));
        }
        d.footer(buttons)
    }

    pub fn panel(self, t: &Theme) -> El {
        self.dialog(t).panel(t)
    }

    pub fn build(self, t: &Theme, layer: &mut Vec<El>) {
        self.dialog(t).build(t, layer)
    }
}
