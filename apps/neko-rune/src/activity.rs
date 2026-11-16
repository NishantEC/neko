//! Neko's chat holds conversation only (polish spec §0). What the daemon posts on its own
//! (`neko_chat::post_notice`: it updated itself, is reinstalling, rolled back, an outage) is not a turn
//! and never draws as a chat message: it folds into one quiet Activity line at the top of the column.
//!
//! A notice is told by its shape, which `post_notice` makes and a turn never has (the daemon's test
//! `a_notice_has_the_shape_the_native_client_folds_out_of_the_chat` pins it): Neko's role, no start
//! time (a turn records when it started), not pending or failed, text parts only, no tool calls, and it
//! does not answer the person (the message before it is not theirs).

use neko_protocol::workbench::{ChatMessage, ChatRole};

/// The chat shows this many messages, then "Earlier messages" loads this many more.
pub const CHAT_PAGE: usize = 30;

/// Whether `message`, which follows `prev`, is something Neko posted on its own.
pub fn is_notice(prev: Option<&ChatMessage>, message: &ChatMessage) -> bool {
    message.role == ChatRole::Neko
        && !message.pending
        && !message.failed
        && !message.queued
        && message.started_ms == 0
        && message.tool_calls.is_empty()
        && !message.parts.is_empty()
        && message.parts.iter().all(|p| p["type"] == "text")
        && prev.is_none_or(|p| p.role != ChatRole::User)
}

/// A notice, as the Activity line lists it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notice {
    pub id: String,
    pub at_ms: i64,
    pub text: String,
}

/// The conversation, split: the notices (oldest first) and the chat's own messages.
pub struct Split {
    pub notices: Vec<Notice>,
    /// Indexes into the conversation of the messages that are conversation.
    pub chat: Vec<usize>,
}

pub fn split(conversation: &[ChatMessage]) -> Split {
    let mut notices = Vec::new();
    let mut chat = Vec::new();
    for (i, m) in conversation.iter().enumerate() {
        if is_notice(i.checked_sub(1).map(|p| &conversation[p]), m) {
            notices.push(Notice { id: m.id.clone(), at_ms: m.at_ms, text: m.text() });
        } else {
            chat.push(i);
        }
    }
    Split { notices, chat }
}

/// What the Activity line says: the latest notice and how many others there are (`+N`).
pub fn summary(notices: &[Notice]) -> Option<(&Notice, usize)> {
    notices.last().map(|latest| (latest, notices.len() - 1))
}

/// The conversation messages the column draws: the last `pages` × [`CHAT_PAGE`] of the chat's own,
/// and how many earlier ones are held back.
pub fn visible(conversation: &[ChatMessage], split: &Split, pages: usize) -> (Vec<ChatMessage>, usize) {
    let keep = CHAT_PAGE * pages.max(1);
    let skip = split.chat.len().saturating_sub(keep);
    (split.chat[skip..].iter().map(|&i| conversation[i].clone()).collect(), skip)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Built the way `neko_core::neko_chat::message` builds what `post_notice` posts.
    fn notice(id: &str, text: &str) -> ChatMessage {
        ChatMessage {
            agent_profile_revision: 0,
            agent_profile_id: neko_protocol::agent_profiles::default_profile_id(),
            id: id.into(),
            at_ms: 1_800_000_000_000,
            role: ChatRole::Neko,
            parts: ChatMessage::text_parts(text),
            ticket_ids: vec![],
            pending: false,
            queued: false,
            failed: false,
            responsibility_ids: vec![],
            actions: vec![],
            started_ms: 0,
            tool_calls: vec![],
            workspace_id: None,
        }
    }

    fn user(id: &str, text: &str) -> ChatMessage {
        ChatMessage { role: ChatRole::User, ..notice(id, text) }
    }

    /// A real turn: it started at some time, and used a tool.
    fn reply(id: &str, text: &str) -> ChatMessage {
        let mut parts = vec![serde_json::json!({"type": "step-start"}), serde_json::json!({"type": "dynamic-tool", "toolName": "list_tasks", "toolCallId": "c1", "state": "output-available", "input": {}, "output": {}})];
        parts.extend(ChatMessage::text_parts(text));
        ChatMessage { parts, started_ms: 1_799_999_990_000, ..notice(id, text) }
    }

    fn plain_reply(id: &str, text: &str) -> ChatMessage {
        ChatMessage { started_ms: 1_799_999_990_000, ..notice(id, text) }
    }

    #[test]
    fn post_notice_messages_never_render_as_chat_messages() {
        let conversation = vec![
            user("u1", "make me a chair"),
            reply("r1", "The chair is on the canvas."),
            notice("n1", "Neko updated itself to 9a2ed3af (UI live)."),
            notice("n2", "Reinstalling Neko at 9a2ed3af now that it is idle."),
            notice("n3", "Neko updated itself to 9a2ed3af (app reinstalled)."),
            user("u2", "thanks"),
            plain_reply("r2", "Anytime."),
        ];
        let s = split(&conversation);
        assert_eq!(s.notices.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(), ["n1", "n2", "n3"]);
        let (shown, held_back) = visible(&conversation, &s, 1);
        assert_eq!(shown.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(), ["u1", "r1", "u2", "r2"], "the chat is the conversation only");
        assert_eq!(held_back, 0);
        // Nothing a notice said is in a turn the chat would draw.
        let drawn = crate::turns::chat_turns(&shown);
        assert!(drawn.iter().all(|t| !format!("{t:?}").contains("updated itself") && !format!("{t:?}").contains("Reinstalling")));
    }

    #[test]
    fn a_reply_to_the_person_is_never_a_notice_even_with_one_text_part() {
        // The same shape as a notice but it follows the person's message and carries a start time.
        let conversation = vec![user("u1", "hello"), plain_reply("r1", "Hi."), user("u2", "and now"), notice("r2", "An answer written by code with no start time")];
        let s = split(&conversation);
        assert!(s.notices.is_empty(), "a message that answers the person is conversation: {:?}", s.notices);
        assert_eq!(s.chat.len(), 4);
    }

    #[test]
    fn a_turn_in_flight_or_failed_or_with_tools_stays_in_the_chat() {
        let mut pending = notice("p", "working");
        pending.pending = true;
        let mut failed = notice("f", "it broke");
        failed.failed = true;
        let conversation = vec![user("u", "go"), reply("r", "done"), pending, failed, ChatMessage { started_ms: 0, ..reply("t", "tools without a start time") }];
        let s = split(&conversation);
        assert!(s.notices.is_empty(), "{:?}", s.notices);
    }

    #[test]
    fn the_activity_line_says_the_latest_notice_and_the_count_of_the_others() {
        assert!(summary(&[]).is_none(), "no notices, no line");
        let s = split(&[notice("a", "one")]);
        let (latest, more) = summary(&s.notices).unwrap();
        assert_eq!((latest.text.as_str(), more), ("one", 0));
        let s = split(&[notice("a", "one"), notice("b", "two"), notice("c", "three")]);
        let (latest, more) = summary(&s.notices).unwrap();
        assert_eq!((latest.text.as_str(), more), ("three", 2));
    }

    #[test]
    fn the_chat_shows_the_last_30_messages_and_earlier_ones_load_30_at_a_time() {
        let mut conversation = Vec::new();
        for i in 0..40 {
            conversation.push(user(&format!("u{i}"), "q"));
            conversation.push(plain_reply(&format!("r{i}"), "a"));
            conversation.push(notice(&format!("n{i}"), "ticker"));
        }
        let s = split(&conversation);
        assert_eq!(s.chat.len(), 80);
        assert_eq!(s.notices.len(), 40);
        let (page1, held) = visible(&conversation, &s, 1);
        assert_eq!((page1.len(), held), (30, 50));
        assert_eq!(page1.last().unwrap().id, "r39");
        let (page2, held) = visible(&conversation, &s, 2);
        assert_eq!((page2.len(), held), (60, 20));
        let (page3, held) = visible(&conversation, &s, 3);
        assert_eq!((page3.len(), held), (80, 0));
    }
}
