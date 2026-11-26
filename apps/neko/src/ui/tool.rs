//! rune-ui registry: tool 0.9.0 Copied into your app by `rune-ui add tool`; yours to change.
//!
//! One tool call as one quiet line that reads like a sentence: its status mark, its name and a short summary of what it did (or the
//! whole line the app supplies, such as the command itself), a quiet note after it ("exit 1"), and how long it took when that was a
//! second or more. A long line is cut in the middle, keeping both ends, and the whole of it is in the fold. It folds open to free words
//! the app supplies, the input it was given and the output it returned (or its error, in the danger ink), monospaced on the slab. A
//! call a person denied is neutral (a shield and "Denied"), not a failure. The line is never louder than the answer around it.
//!
//! ```ignore
//! Tool::new("t1", &call, s.open("t1")).on_toggle(|s: &mut App| s.flip("t1")).build(t)
//! Tool::new("t2", &call, open).label("cargo test --workspace").note("exit 1").build(t)
//! ```

use std::rc::Rc;

use rune::{El, text};

use super::agent::{FoldLine, Status, caption, mark, pre, took};
use super::base::{Press, State, Theme};

/// What the app knows about one call.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ToolCall {
    /// The tool's name ("read_file").
    pub name: String,
    /// A few words on what it did ("src/main.rs"); after the name.
    pub summary: String,
    pub status: Status,
    /// Seconds it took (or has taken so far).
    pub secs: Option<f64>,
    pub input: String,
    pub output: Option<String>,
    pub error: Option<String>,
}

pub struct Tool<'a, S: 'static> {
    key: String,
    call: &'a ToolCall,
    open: bool,
    label: Option<String>,
    note: Option<String>,
    words: Option<String>,
    pinned: Option<State>,
    toggle: Option<Press<S>>,
}

impl<'a, S: 'static> Tool<'a, S> {
    pub fn new(key: impl Into<String>, call: &'a ToolCall, open: bool) -> Tool<'a, S> {
        Tool { key: key.into(), call, open, label: None, note: None, words: None, pinned: None, toggle: None }
    }

    /// The whole line, in place of the name and summary (the command itself, "Read src/main.rs"). Cut in the middle when long; the
    /// fold holds all of it.
    pub fn label(mut self, words: impl Into<String>) -> Self {
        self.label = Some(words.into());
        self
    }

    /// A quiet note after the line ("exit 1", "3 matches").
    pub fn note(mut self, words: impl Into<String>) -> Self {
        self.note = Some(words.into());
        self
    }

    /// Free words at the top of the fold, before the input (what the tool was for, a summary of its result).
    pub fn text(mut self, words: impl Into<String>) -> Self {
        self.words = Some(words.into());
        self
    }

    pub fn state(mut self, s: State) -> Self {
        self.pinned = Some(s);
        self
    }

    pub fn on_toggle(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.toggle = Some(Rc::new(f));
        self
    }

    pub fn build(self, t: &Theme) -> El {
        let c = self.call;
        let given = self.label.is_some();
        let label = self.label.unwrap_or_else(|| if c.summary.is_empty() { c.name.clone() } else { format!("{} {}", c.name, c.summary) });
        let mut line = FoldLine::<S>::new(self.key.clone(), label.clone(), self.open).lead(mark(t, c.status, t.control.icon)).truncate_middle(true);
        if let Some(n) = &self.note {
            line = line.meta(n.clone());
        }
        // What the figures say: a word where the mark alone would need colour to read (waiting, denied, a failure with no time), the
        // time when it was a second or more, and nothing for a quick step that went well.
        let figure = match (c.status, c.secs.and_then(took)) {
            (Status::Pending | Status::Waiting | Status::Denied, _) => Some(c.status.word().to_owned()),
            (_, Some(d)) => Some(d),
            (Status::Error | Status::Skipped, None) => Some(c.status.word().to_owned()),
            _ => None,
        };
        if let Some(f) = figure {
            line = line.meta(f);
        }
        if let Some(p) = self.pinned {
            line = line.state(p);
        }
        if let Some(f) = self.toggle {
            line = line.on_toggle(move |s: &mut S| f(s));
        }
        let mut body = Vec::new();
        // The whole line, when the app gave it and it says more than the input below (a long command cut in the middle above).
        if given && label.trim() != c.input.trim() {
            body.push(text(label).font(t.ty.base).color(t.ink_muted()).wrap().min_w(0.0));
        }
        if let Some(w) = self.words {
            body.push(text(w).font(t.ty.base).color(t.ink_muted()).wrap().min_w(0.0));
        }
        if !c.input.is_empty() {
            body.push(caption(t, "Input"));
            body.push(pre(t, &format!("{}-input", self.key), &c.input, t.ink(), 200.0));
        }
        if let Some(e) = &c.error {
            body.push(caption(t, "Error"));
            body.push(pre(t, &format!("{}-error", self.key), e, t.c.danger.fg, 200.0));
        } else if let Some(o) = &c.output {
            body.push(caption(t, "Output"));
            body.push(pre(t, &format!("{}-output", self.key), o, t.ink_muted(), 240.0));
        }
        line.build(t, body)
    }
}
