//! rune-ui registry: task 0.10.0 Copied into your app by `rune-ui add task`; yours to change.
//!
//! A run card: the one thing on an agent screen that should stand out once work is finished. A slab with the run's title and a status
//! pill (its meaning as a wash, a dot and words), then quiet facts in tabular figures and one primary action in ink and any secondary actions beside it (`secondary`). Every fact is
//! optional and simply absent when not given: files changed with `+n −n`, checks passed and failed (none when there were no checks,
//! never "0 passed"), cost (or tokens in its place), time, the branch, and a free note line under them.
//!
//! ```ignore
//! let run = Run::new("ENG-123: retry failed pages", Status::Done).changes(4, 120, 34).checks(212, 0).cost(0.42).secs(252.0);
//! Task::new("run", &run).action("Review changes", |s: &mut App| s.review()).build(t)
//! ```

use std::rc::Rc;

use rune::{El, col, row, text};
use rune_ui_style::{tabular, weight};

use super::agent::{Status, elapsed, money, tokens};
use super::badge::Badge;
use super::base::{ButtonVariant, Press, Shape, Theme, glyph, paint};
use super::button::Button;
use super::icons;

/// Files a run changed and the lines it added and removed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Changes {
    pub files: usize,
    pub added: usize,
    pub removed: usize,
}

/// Checks a run ran.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Checks {
    pub passed: usize,
    pub failed: usize,
}

/// What the app knows about a run. Every fact is optional.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Run {
    pub title: String,
    pub status: Status,
    pub changes: Option<Changes>,
    pub checks: Option<Checks>,
    /// Dollars.
    pub cost: Option<f64>,
    /// Tokens spent: shown in the cost's place when there is no cost.
    pub tokens: Option<u64>,
    pub secs: Option<f64>,
    pub branch: Option<String>,
    /// A free line under the facts ("Waiting on review from Lee").
    pub note: Option<String>,
}

impl Run {
    pub fn new(title: impl Into<String>, status: Status) -> Run {
        Run { title: title.into(), status, ..Run::default() }
    }

    pub fn changes(mut self, files: usize, added: usize, removed: usize) -> Self {
        self.changes = Some(Changes { files, added, removed });
        self
    }

    pub fn checks(mut self, passed: usize, failed: usize) -> Self {
        self.checks = Some(Checks { passed, failed });
        self
    }

    pub fn cost(mut self, usd: f64) -> Self {
        self.cost = Some(usd);
        self
    }

    pub fn tokens(mut self, n: u64) -> Self {
        self.tokens = Some(n);
        self
    }

    pub fn secs(mut self, s: f64) -> Self {
        self.secs = Some(s);
        self
    }

    pub fn branch(mut self, b: impl Into<String>) -> Self {
        self.branch = Some(b.into());
        self
    }

    pub fn note(mut self, n: impl Into<String>) -> Self {
        self.note = Some(n.into());
        self
    }
}

pub struct Task<'a, S: 'static> {
    key: String,
    run: &'a Run,
    action: Option<(String, Press<S>)>,
    secondary: Vec<(String, Press<S>)>,
}

impl<'a, S: 'static> Task<'a, S> {
    pub fn new(key: impl Into<String>, run: &'a Run) -> Task<'a, S> {
        Task { key: key.into(), run, action: None, secondary: Vec::new() }
    }

    /// The one primary action.
    pub fn action(mut self, words: impl Into<String>, f: impl Fn(&mut S) + 'static) -> Self {
        self.action = Some((words.into(), Rc::new(f)));
        self
    }

    /// A secondary action beside the primary one (Undo, Open the branch): raised, not ink, in the order given, each its own Tab
    /// stop. They share the actions row, keyed `<key>-secondary<N>` from zero. Without a primary action they still show.
    pub fn secondary(mut self, words: impl Into<String>, f: impl Fn(&mut S) + 'static) -> Self {
        self.secondary.push((words.into(), Rc::new(f)));
        self
    }

    pub fn build(self, t: &Theme) -> El {
        let r = self.run;
        let fact = |icon, words: El| row().gap(t.space.at(1.5)).center_y().child(glyph(icon, t.control.icon_sm, t.ink_faint())).child(words);
        let figures = |s: String| text(s).font(tabular(t.ty.sm)).color(t.ink_muted());
        let mut facts = Vec::new();
        if let Some(c) = r.changes {
            let changes = row().gap(t.space.at(1.5)).child(figures(format!("{} file{}", c.files, if c.files == 1 { "" } else { "s" })))
                .child(text(format!("+{}", c.added)).font(tabular(t.ty.sm)).color(t.c.success.fg))
                .child(text(format!("\u{2212}{}", c.removed)).font(tabular(t.ty.sm)).color(t.c.danger.fg));
            facts.push(fact(&icons::FILE, changes));
        }
        if let Some(c) = r.checks.filter(|c| c.passed + c.failed > 0) {
            let mut checks = row().gap(t.space.at(1.5));
            if c.passed > 0 {
                checks = checks.child(figures(format!("{} passed", c.passed)));
            }
            if c.failed > 0 {
                checks = checks.child(text(format!("{} failed", c.failed)).font(tabular(t.ty.sm)).color(t.c.danger.fg));
            }
            facts.push(fact(&icons::CIRCLE_CHECK, checks));
        }
        match (r.cost, r.tokens) {
            (Some(usd), _) => facts.push(fact(&icons::TAG, figures(money(usd)))),
            (None, Some(n)) => facts.push(fact(&icons::TAG, figures(format!("{} tokens", tokens(n))))),
            _ => {}
        }
        if let Some(secs) = r.secs {
            facts.push(fact(&icons::CLOCK, figures(elapsed(secs))));
        }
        if let Some(b) = &r.branch {
            facts.push(fact(&icons::COMMIT, text(b.clone()).font(super::agent::mono(t, t.ty.sm)).color(t.ink_muted()).truncate_middle().min_w(0.0).shrink()));
        }
        let head = row().w_full().gap(t.space.at(3.0)).items_start()
            .child(text(r.title.clone()).font(weight(t.ty.md, t.ty.medium)).color(t.ink()).wrap().min_w(0.0).grow(1.0).shrink())
            .child(Badge::new(format!("{}-status", self.key), r.status.word()).tone(r.status.tone()).dot().build(t));
        let mut card = paint(col().key(self.key.clone()).role("group").label(format!("{}: {}", r.title, r.status.word())).w_full().padding(t.metrics.inset).gap(t.space.at(4.0)).radius(t.radius.xl).round_as(t), &t.mat.slab.base).child(head);
        if !facts.is_empty() || r.note.is_some() {
            let mut block = col().w_full().gap(t.space.at(2.0));
            if !facts.is_empty() {
                block = block.child(row().key(format!("{}-facts", self.key)).w_full().gap(t.space.at(5.0)).flex_wrap().children(facts));
            }
            if let Some(n) = &r.note {
                block = block.child(text(n.clone()).key(format!("{}-note", self.key)).font(t.ty.sm).color(t.ink_muted()).wrap().min_w(0.0));
            }
            card = card.child(block);
        }
        if self.action.is_some() || !self.secondary.is_empty() {
            let mut actions = row().key(format!("{}-actions", self.key)).gap(t.space.at(2.0)).flex_wrap();
            if let Some((words, f)) = self.action {
                actions = actions.child(Button::new(format!("{}-action", self.key), words).on_press(move |s: &mut S| f(s)).build(t));
            }
            for (i, (words, f)) in self.secondary.into_iter().enumerate() {
                actions = actions.child(Button::new(format!("{}-secondary{i}", self.key), words).variant(ButtonVariant::Secondary).on_press(move |s: &mut S| f(s)).build(t));
            }
            card = card.child(actions);
        }
        card
    }
}
