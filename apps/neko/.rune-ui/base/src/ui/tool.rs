//! rune-ui registry: tool 0.8.0 Copied into your app by `rune-ui add tool`; yours to change.
//!
//! One tool call as one quiet line: its status mark, its name, a short summary of what it did, how long it took. It folds open to the
//! input it was given and the output it returned (or its error, in the danger ink), monospaced on the slab. The line is never louder
//! than the answer around it.
//!
//! ```ignore
//! Tool::new("t1", &call, s.open("t1")).on_toggle(|s: &mut App| s.flip("t1")).build(t)
//! ```

use std::rc::Rc;

use rune::El;

use super::agent::{FoldLine, Status, caption, elapsed, mark, pre};
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
    pinned: Option<State>,
    toggle: Option<Press<S>>,
}

impl<'a, S: 'static> Tool<'a, S> {
    pub fn new(key: impl Into<String>, call: &'a ToolCall, open: bool) -> Tool<'a, S> {
        Tool { key: key.into(), call, open, pinned: None, toggle: None }
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
        let label = if c.summary.is_empty() { c.name.clone() } else { format!("{} {}", c.name, c.summary) };
        let mut line = FoldLine::<S>::new(self.key.clone(), label, self.open).lead(mark(t, c.status, t.control.icon));
        line = line.meta(match (c.status, c.secs) {
            (Status::Pending | Status::Waiting, _) => c.status.word().to_owned(),
            (_, Some(s)) => elapsed(s),
            (s, None) => s.word().to_owned(),
        });
        if let Some(p) = self.pinned {
            line = line.state(p);
        }
        if let Some(f) = self.toggle {
            line = line.on_toggle(move |s: &mut S| f(s));
        }
        let mut body = vec![caption(t, "Input"), pre(t, &format!("{}-input", self.key), &c.input, t.ink(), 200.0)];
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
