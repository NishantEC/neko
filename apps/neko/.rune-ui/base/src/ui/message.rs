//! rune-ui registry: message 0.8.0 Copied into your app by `rune-ui add message`; yours to change.
//!
//! One turn of a conversation. A **user** message is a quiet bubble at the right: the muted fill, no shadow, no outline, at most four
//! fifths of the column. An **assistant** message is the page itself: its words sit on the page in a reading column (720 points at
//! most) with no bubble and no card, because the agent's answer is the content. Either may carry a small avatar, a row of attachments
//! above the words, and an action bar under them.
//!
//! ```ignore
//! Message::user("m1", "Asha").child(message::words(t, "Why is the build slow?")).build(t)
//! Message::assistant("m2", "Assistant").child(response).actions(bar).build(t)
//! ```

use rune::{El, col, row, text};

use super::base::{Shape, Theme};

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
    column: f32,
}

impl Message {
    /// A person's message; `name` names it for assistive technology ("Asha said").
    pub fn user(key: impl Into<String>, name: impl Into<String>) -> Message {
        Message { key: key.into(), from: From::User, name: name.into(), body: Vec::new(), avatar: None, attachments: Vec::new(), actions: None, column: 720.0 }
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

    /// The reading column's widest (720).
    pub fn column(mut self, w: f32) -> Self {
        self.column = w;
        self
    }

    pub fn build(self, t: &Theme) -> El {
        let label = format!("{} said", self.name);
        let gap = t.space.at(2.0);
        let mut stack = col().gap(gap).min_w(0.0);
        if !self.attachments.is_empty() {
            let mut r = row().key(format!("{}-attachments", self.key)).gap(t.space.at(2.0)).flex_wrap();
            if self.from == From::User {
                r = r.justify_end();
            }
            stack = stack.child(r.children(self.attachments));
        }
        match self.from {
            From::User => {
                let bubble = col().key(format!("{}-bubble", self.key)).px(t.metrics.pad_md).py(t.space.at(2.5)).gap(gap).radius(t.radius.xl).round_as(t).fill(t.c.muted.bg).max_w(self.column * 0.8).min_w(0.0).children(self.body);
                stack = stack.items_end().child(bubble);
                if let Some(a) = self.actions {
                    stack = stack.child(a);
                }
                let mut line = row().w_full().justify_end().items_start().gap(t.space.at(2.5)).child(stack.shrink());
                if let Some(av) = self.avatar {
                    line = line.child(av);
                }
                col().key(self.key).role("article").label(label).w_full().max_w(self.column).child(line)
            }
            From::Assistant => {
                stack = stack.w_full().children(self.body);
                if let Some(a) = self.actions {
                    stack = stack.child(a);
                }
                let mut line = row().w_full().items_start().gap(t.space.at(3.0));
                if let Some(av) = self.avatar {
                    line = line.child(av);
                }
                col().key(self.key).role("article").label(label).w_full().max_w(self.column).child(line.child(stack.grow(1.0).shrink()))
            }
        }
    }
}
