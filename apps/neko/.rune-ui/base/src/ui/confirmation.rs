//! rune-ui registry: confirmation 0.9.0 Copied into your app by `rune-ui add confirmation`; yours to change.
//!
//! A tool call asking permission. While it waits it is the one thing that needs you: a slab with the warning mark and "Needs you", what
//! the tool wants to do, the exact command or input in a monospaced face, and three answers: **Allow once** (the ink), **Always allow**
//! (raised) and **Deny** (ghost). Tab reaches each answer and Return or Space gives it; Escape anywhere in the request denies. Once
//! answered (or expired) it becomes one quiet line saying what happened, with no buttons, so it cannot be answered twice.
//!
//! An optional `why` line under the request says why it is asked. The **destructive** form is for what cannot be taken back: there is
//! no "Always allow", the confirm button is the danger material, Return gives no answer (only a press or Space on a focused answer
//! does), and Escape still denies.
//!
//! ```ignore
//! Confirmation::new("perm", "Run a shell command", s.decision).detail("rm -rf target").on_decide(|s: &mut App, d| s.decision = d).build(t)
//! Confirmation::new("perm", "Delete 214 files", s.decision).why("Commands can change anything on this Mac.").destructive(true).build(t)
//! ```

use std::rc::Rc;

use rune::{El, Icon, Key, KeyEvent, Tx, col, row, text};
use rune_ui_style::weight;

use super::agent::{pre, return_presses, slab};
use super::base::{ButtonVariant, Shape, Theme, glyph};
use super::button::Button;
use super::icons;

/// Where the request stands.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Decision {
    #[default]
    Pending,
    AllowedOnce,
    AlwaysAllowed,
    Denied,
    /// Nobody answered in time.
    Expired,
}

impl Decision {
    pub fn word(self) -> &'static str {
        match self {
            Decision::Pending => "Needs you",
            Decision::AllowedOnce => "Allowed once",
            Decision::AlwaysAllowed => "Always allowed",
            Decision::Denied => "Denied",
            Decision::Expired => "Expired",
        }
    }
}

/// What an answer does.
pub type Decide<S> = Rc<dyn Fn(&mut S, Decision)>;

pub struct Confirmation<S: 'static> {
    key: String,
    title: String,
    detail: Option<String>,
    why: Option<String>,
    destructive: bool,
    decision: Decision,
    decide: Option<Decide<S>>,
}

impl<S: 'static> Confirmation<S> {
    /// `title`: what the tool wants to do, in words.
    pub fn new(key: impl Into<String>, title: impl Into<String>, decision: Decision) -> Confirmation<S> {
        Confirmation { key: key.into(), title: title.into(), detail: None, why: None, destructive: false, decision, decide: None }
    }

    /// The exact command or input, shown monospaced.
    pub fn detail(mut self, d: impl Into<String>) -> Self {
        self.detail = Some(d.into());
        self
    }

    /// One line under the request saying why it is asked ("Asked because commands can change anything on this Mac").
    pub fn why(mut self, w: impl Into<String>) -> Self {
        self.why = Some(w.into());
        self
    }

    /// The form for what cannot be taken back: no "Always allow", the confirm button in the danger material, and Return answers
    /// nothing (a press, or Space on a focused answer, does). Escape still denies.
    pub fn destructive(mut self, on: bool) -> Self {
        self.destructive = on;
        self
    }

    /// An answer given. Called only while the request is pending.
    pub fn on_decide(mut self, f: impl Fn(&mut S, Decision) + 'static) -> Self {
        self.decide = Some(Rc::new(f));
        self
    }

    pub fn build(self, t: &Theme) -> El {
        if self.decision != Decision::Pending {
            let (icon, ink): (&'static Icon, _) = match self.decision {
                Decision::AllowedOnce | Decision::AlwaysAllowed => (&icons::CIRCLE_CHECK, t.c.success.fg),
                Decision::Denied => (&icons::CIRCLE_X, t.c.danger.fg),
                _ => (&icons::CLOCK, t.ink_faint()),
            };
            return row().key(self.key).role("status").label(format!("{}: {}", self.title, self.decision.word())).w_full().h(t.control.md).px(t.space.at(1.5)).gap(t.metrics.gap).center_y()
                .child(glyph(icon, t.control.icon, ink))
                .child(text(self.decision.word()).font(weight(t.ty.base, t.ty.medium)).color(t.ink_muted()).a11y_hidden())
                .child(text(self.title).font(t.ty.base).color(t.ink_faint()).truncate().min_w(0.0).a11y_hidden());
        }
        let destructive = self.destructive;
        let answer = |suffix: &str, words: &str, v: ButtonVariant, d: Decision| {
            let mut b = Button::<S>::new(format!("{}-{suffix}", self.key), words).variant(v);
            let mut holder = row();
            if let Some(f) = self.decide.clone() {
                let g = f.clone();
                b = b.on_press(move |s: &mut S| f(s, d));
                if !destructive {
                    holder = holder.on_key(move |s: &mut S, k: &KeyEvent, _: &mut Tx| return_presses(k, || g(s, d)));
                }
            }
            holder.child(b.build(t))
        };
        let mut answers = row().key(format!("{}-answers", self.key)).gap(t.space.at(2.0)).flex_wrap()
            .child(answer("allow", "Allow once", if destructive { ButtonVariant::Destructive } else { ButtonVariant::Default }, Decision::AllowedOnce));
        if !destructive {
            answers = answers.child(answer("always", "Always allow", ButtonVariant::Secondary, Decision::AlwaysAllowed));
        }
        answers = answers.child(answer("deny", "Deny", ButtonVariant::Ghost, Decision::Denied));
        let head = row().w_full().gap(t.space.at(2.0)).center_y()
            .child(glyph(&icons::SHIELD, t.control.icon, t.c.warning.fg))
            .child(text("Needs you").font(weight(t.ty.sm, t.ty.medium)).color(t.c.warning.fg));
        let mut request = col().w_full().gap(t.space.at(1.0)).child(text(self.title.clone()).font(weight(t.ty.md, t.ty.medium)).color(t.ink()).wrap().min_w(0.0));
        if let Some(w) = &self.why {
            request = request.child(text(w.clone()).key(format!("{}-why", self.key)).font(t.ty.sm).color(t.ink_muted()).wrap().min_w(0.0));
        }
        let mut body = col().w_full().padding(t.metrics.pad_md).gap(t.space.at(3.0)).child(head).child(request);
        if let Some(d) = &self.detail {
            body = body.child(pre(t, &format!("{}-detail", self.key), d, t.ink(), 160.0));
        }
        let how = if destructive { "Allow once or deny. Escape denies." } else { "Allow once, always allow, or deny. Escape denies." };
        let mut panel = slab(t, self.key.clone()).role("alertdialog").label(format!("Permission: {}", self.title)).description(how).round_as(t).child(body.child(answers));
        if let Some(f) = self.decide {
            panel = panel.on_key(move |s: &mut S, k: &KeyEvent, _: &mut Tx| match k.key {
                Key::Escape => {
                    f(s, Decision::Denied);
                    true
                }
                // The destructive form keeps Return from reaching any default button: only a deliberate press answers it.
                Key::Enter => destructive,
                _ => false,
            });
        }
        panel
    }
}
