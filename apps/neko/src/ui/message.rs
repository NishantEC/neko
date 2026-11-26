//! rune-ui registry: message 0.9.0 Copied into your app by `rune-ui add message`; yours to change.
//!
//! One turn of a conversation. A **user** message is a quiet bubble at the right: the muted fill, no shadow, no outline, at most four
//! fifths of the column. An **assistant** message is the page itself: its words sit on the page in a reading column (720 points at
//! most) with no bubble and no card, because the agent's answer is the content. Either may carry a small avatar, a row of attachments
//! above the words, and an action bar under them.
//!
//! ```ignore
//! Message::user("m1", "Asha").child(message::words(t, "Why is the build slow?")).build(t)
//! Message::assistant("m2", "Assistant").child(response).actions(bar).build(t)
//! // actions only while the message is hovered or holds focus (the app keeps one `Reveal` per message):
//! Message::user("m1", "Asha").child(words).actions(bar).reveal_actions(s.reveal("m1"), |s: &mut App| s.reveal_mut("m1")).build(t)
//! ```
//!
//! The rhythm of a conversation: [`TURN_GAP`] between turns (Conversation's gap), [`BLOCK_GAP`] between the blocks of one turn (a
//! reasoning line, tool lines, the answer, its actions). Nothing in a conversation sits further apart than [`TURN_GAP`].

use std::rc::Rc;

use rune::{El, Tx, col, row, text};

use super::base::{Shape, Theme};

/// Between turns, in grid steps (`t.space.at(TURN_GAP)`): 20 points at the default spacing.
pub const TURN_GAP: f32 = 5.0;
/// Between the blocks of one turn, in grid steps: 4 points, which keeps 36 point fold lines on a 40 point pitch (their hit areas
/// meet, never overlap).
pub const BLOCK_GAP: f32 = 1.0;

/// Whether a message's actions show, when they show only on hover or focus: the pointer is over the message, or keyboard focus is
/// inside it. The app keeps one per message and hands it back each frame; the message writes into it through the accessor given to
/// [`Message::reveal_actions`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Reveal {
    pub hovered: bool,
    pub focused: bool,
}

impl Reveal {
    pub fn shown(self) -> bool {
        self.hovered || self.focused
    }
}

type Wire = Rc<dyn Fn(El) -> El>;

/// Who is speaking.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum From {
    #[default]
    User,
    Assistant,
}

/// Plain words at the reading size, wrapping (for a user's message, or an answer that is not Markdown).
pub fn words(t: &Theme, s: impl Into<String>) -> El {
    text(s.into()).font(t.ty.md).color(t.ink()).wrap().min_w(0.0)
}

pub struct Message {
    key: String,
    from: From,
    name: String,
    body: Vec<El>,
    avatar: Option<El>,
    attachments: Vec<El>,
    actions: Option<El>,
    reveal: Option<(Reveal, Wire)>,
    column: f32,
}

impl Message {
    /// A person's message; `name` names it for assistive technology ("Asha said").
    pub fn user(key: impl Into<String>, name: impl Into<String>) -> Message {
        Message { key: key.into(), from: From::User, name: name.into(), body: Vec::new(), avatar: None, attachments: Vec::new(), actions: None, reveal: None, column: 720.0 }
    }

    /// The agent's message.
    pub fn assistant(key: impl Into<String>, name: impl Into<String>) -> Message {
        Message { from: From::Assistant, ..Message::user(key, name) }
    }

    pub fn child(mut self, e: El) -> Self {
        self.body.push(e);
        self
    }

    /// A small avatar beside the message (an `Avatar` at its small size).
    pub fn avatar(mut self, e: El) -> Self {
        self.avatar = Some(e);
        self
    }

    /// Files and images sent with it, shown above the words (`Attachment` chips).
    pub fn attachment(mut self, e: El) -> Self {
        self.attachments.push(e);
        self
    }

    /// The action bar under the words (`Actions`).
    pub fn actions(mut self, e: El) -> Self {
        self.actions = Some(e);
        self
    }

    /// Shows the action bar only while the message is hovered or holds keyboard focus. While hidden the bar takes no height and draws
    /// nothing, but its buttons stay in the Tab order: Tab reaches them, and the bar shows as soon as one has focus. `now` is this
    /// message's [`Reveal`] as the app keeps it; `access` reaches it in the app's state, so pointer and focus changes land there.
    pub fn reveal_actions<S: 'static, F>(mut self, now: Reveal, access: F) -> Self
    where
        F: for<'a> Fn(&'a mut S) -> &'a mut Reveal + Clone + 'static,
    {
        let wire: Wire = Rc::new(move |e: El| {
            let (hover, focus) = (access.clone(), access.clone());
            e.on_hover_change(move |s: &mut S, inside: bool, _: &mut Tx| hover(s).hovered = inside)
                .on_focus_within_change(move |s: &mut S, within: bool, _: &mut Tx| focus(s).focused = within)
        });
        self.reveal = Some((now, wire));
        self
    }

    /// The reading column's widest (720).
    pub fn column(mut self, w: f32) -> Self {
        self.column = w;
        self
    }

    /// The action bar's slot: in the flow under the words while shown; while hidden, out of the flow, no size, clipped, so it adds
    /// no height or gap and nothing shows, while its buttons keep their size and so stay reachable by Tab. One keyed slot in both
    /// states, so a button that takes focus keeps it as the bar appears.
    fn actions_slot(key: &str, bar: El, shown: bool, t: &Theme, end: bool) -> El {
        let slot = row().key(format!("{key}-actions-slot"));
        if shown {
            let slot = slot.pt(t.space.at(BLOCK_GAP));
            if end { slot.w_full().justify_end().child(bar) } else { slot.child(bar) }
        } else {
            slot.absolute(0.0, 0.0).size(0.0, 0.0).clip_rounded(0.0).child(bar)
        }
    }

    pub fn build(self, t: &Theme) -> El {
        let label = format!("{} said", self.name);
        let gap = t.space.at(2.0);
        let shown = self.reveal.as_ref().is_none_or(|(r, _)| r.shown());
        let wire = self.reveal.map(|(_, w)| w);
        // The words (and attachments) with their own gap; the action bar's slot after them brings its own space, so a hidden bar
        // leaves no gap behind.
        let mut stack = col().gap(gap).min_w(0.0);
        if !self.attachments.is_empty() {
            let mut r = row().key(format!("{}-attachments", self.key)).gap(t.space.at(2.0)).flex_wrap();
            if self.from == From::User {
                r = r.justify_end();
            }
            stack = stack.child(r.children(self.attachments));
        }
        let article = match self.from {
            From::User => {
                let bubble = col().key(format!("{}-bubble", self.key)).px(t.metrics.pad_md).py(t.space.at(2.5)).gap(gap).radius(t.radius.xl).round_as(t).fill(t.c.muted.bg).max_w(self.column * 0.8).min_w(0.0).children(self.body);
                stack = stack.items_end().child(bubble);
                let mut outer = col().items_end().min_w(0.0).child(stack);
                if let Some(a) = self.actions {
                    outer = outer.child(Message::actions_slot(&self.key, a, shown, t, true));
                }
                let mut line = row().w_full().justify_end().items_start().gap(t.space.at(2.5)).child(outer.shrink());
                if let Some(av) = self.avatar {
                    line = line.child(av);
                }
                col().key(self.key.clone()).role("article").label(label).w_full().max_w(self.column).child(line)
            }
            From::Assistant => {
                stack = stack.w_full().gap(t.space.at(BLOCK_GAP)).children(self.body);
                let mut outer = col().w_full().min_w(0.0).child(stack);
                if let Some(a) = self.actions {
                    outer = outer.child(Message::actions_slot(&self.key, a, shown, t, false));
                }
                let mut line = row().w_full().items_start().gap(t.space.at(3.0));
                if let Some(av) = self.avatar {
                    line = line.child(av);
                }
                col().key(self.key.clone()).role("article").label(label).w_full().max_w(self.column).child(line.child(outer.grow(1.0).shrink()))
            }
        };
        match wire {
            Some(w) => w(article),
            None => article,
        }
    }
}
