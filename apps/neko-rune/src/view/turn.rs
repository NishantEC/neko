//! Turns on screen, for the session view and Neko's column: a person's message, what an agent
//! said (markdown, streamed live), and the commands it ran as one-line cards that open to their
//! input and output (an edit opens to its diff). The same drawing serves both; `ns` keeps the
//! keys of one list apart from the other's.

use rune::{El, col, row, text};
use rune_kit::Markdown;
use rune_kit::diff::DiffView;

use crate::app::App;
use crate::status::{StatusKind, status_glyph};
use crate::theme::Neko;
use crate::turns::{Block, Role, Tool, ToolState, Turn, TurnKind};

/// Where a list of turns is drawn: names its keys and picks its density.
#[derive(Clone, Copy)]
pub struct Where {
    pub ns: &'static str,
    /// The narrow column: smaller gaps.
    pub compact: bool,
}

fn tool_kind(state: ToolState) -> StatusKind {
    match state {
        ToolState::Running => StatusKind::InProgress,
        ToolState::Done => StatusKind::Done,
        ToolState::Failed => StatusKind::Failed,
        ToolState::Waiting => StatusKind::Review,
        ToolState::Declined => StatusKind::Cancelled,
    }
}

fn state_words(state: ToolState) -> Option<&'static str> {
    match state {
        ToolState::Failed => Some("failed"),
        ToolState::Waiting => Some("needs your OK"),
        ToolState::Declined => Some("declined"),
        ToolState::Running => Some("running"),
        ToolState::Done => None,
    }
}

/// One tool call: a line with its state and what it did; open, the command and its output, or an
/// edit's diff. Approval buttons show while the call waits for one.
pub fn tool_card(s: &App, t: &Neko, w: Where, tool: &Tool, turn_id: &str) -> El {
    let key = format!("tool:{}", tool.id);
    let open = s.open(&key);
    let owned = tool.clone();
    let mut line = row()
        .key(format!("{}tool-{}", w.ns, tool.id))
        .role("button")
        .label(format!("{}{}", tool.title, state_words(tool.state).map(|w| format!(", {w}")).unwrap_or_default()))
        .expanded(open)
        .focusable(true)
        .w_full()
        .h(28.0)
        .px(8.0)
        .gap(8.0)
        .center_y()
        .radius(t.radius.row)
        .hover(t.surface.hover)
        .focus_ring(2.0, 0.0, t.kit.c.border_focus)
        .on_press(move |s: &mut App| s.toggle_tool(&owned))
        .child(text(if open { "▾" } else { "▸" }).font(t.ty.meta).color(t.text.faint).a11y_hidden())
        .child(status_glyph(tool_kind(tool.state), t, 12.0, Some("")))
        .child(text(tool.title.clone()).font(if tool.kind == crate::turns::Kind::Command { t.ty.mono } else { t.ty.meta }).color(t.text.muted).truncate().grow(1.0).min_w(0.0).a11y_hidden());
    if let Some(words) = state_words(tool.state).filter(|_| tool.state != ToolState::Running) {
        line = line.child(text(words).font(t.ty.id).color(if tool.state == ToolState::Failed { t.meaning.coral } else { t.text.faint }).a11y_hidden());
    }
    let mut c = col().key(format!("{}toolbox-{}", w.ns, tool.id)).w_full().gap(2.0).child(line);
    if tool.state == ToolState::Waiting
        && let Some(approval) = tool.approval.clone()
    {
        // A tool call is decided by the turn and the call; an action Neko proposes on a task by the
        // message and the approval's own id (`chat/decisions.tsx`).
        let (turn, tool_id, name) = (turn_id.to_owned(), tool.id.clone(), tool.name.clone());
        let decide = move |approve: bool| {
            if name == crate::turns::ACTION_TOOL {
                crate::link::Call::DecideChatAction { message_id: turn.clone(), action_id: approval.clone(), accept: approve }
            } else {
                crate::link::Call::DecideChatTool { turn_id: turn.clone(), call_id: tool_id.clone(), approve }
            }
        };
        let decline = decide.clone();
        let mut box_ = col().gap(4.0).px(8.0).pb(4.0);
        if let Some(note) = &tool.approval_note {
            box_ = box_.child(text(note.clone()).font(t.ty.meta).color(t.text.muted).wrap());
        }
        c = c.child(
            box_.child(
                row()
                    .gap(8.0)
                    .child(super::primary_button("Approve", &format!("{}approve-{}", w.ns, tool.id), t, move |s: &mut App| s.call(decide(true))))
                    .child(super::quiet_button("Decline", &format!("{}decline-{}", w.ns, tool.id), t, move |s: &mut App| s.call(decline(false)))),
            ),
        );
    }
    if open {
        let body = match (&tool.edit, s.work.edit_diffs.contains_key(&tool.id)) {
            (Some(_), true) => {
                let (a, b, id) = (tool.id.clone(), tool.id.clone(), tool.id.clone());
                let view = DiffView::new(move |s: &App| s.work.edit_diffs.get(&a).unwrap_or(&s.work.empty_diff), move |s: &mut App| s.work.edit_diffs.entry(b.clone()).or_insert_with(|| rune_kit::diff::DiffState::new(Vec::new())))
                    .key(format!("{}diff-{id}", w.ns))
                    .build(s, &t.kit);
                col().w_full().h(220.0).radius(t.radius.row).clip_rounded(t.radius.row).child(view)
            }
            _ => {
                let detail = tool.detail.clone().unwrap_or_else(|| "Nothing was saved for this call.".to_owned());
                col().w_full().max_h(260.0).px(10.0).py(8.0).radius(t.radius.row).fill(t.surface.card).scroll(rune::ScrollSpec { stick_bottom: false }).child(text(detail).font(t.ty.mono).color(t.text.muted).wrap().selectable(true))
            }
        };
        c = c.child(row().w_full().pl(28.0).child(body.key(format!("{}tooldetail-{}", w.ns, tool.id))));
    }
    c
}

fn markdown(t: &Neko, source: &str, streaming: bool) -> El {
    Markdown::new(source).streaming(streaming).selectable(true).build(&t.kit)
}

/// The turns, in order. `previous` is who spoke last before them (so a speaker's name shows only
/// when the speaker changes); the last agent turn streams while it is `pending`.
pub fn turns(s: &App, t: &Neko, w: Where, list: &[Turn]) -> Vec<El> {
    let mut out = Vec::new();
    let mut previous = String::new();
    for (index, turn) in list.iter().enumerate() {
        let speaker = match turn.role {
            Role::System => String::new(),
            _ => turn.author.clone(),
        };
        let shows_speaker = !speaker.is_empty() && speaker != previous;
        if !speaker.is_empty() {
            previous = speaker.clone();
        }
        let gap_before = if w.compact { 10.0 } else if shows_speaker { 22.0 } else { 8.0 };
        let last = index + 1 == list.len();
        match turn.role {
            Role::User => out.push(user_turn(t, w, turn).mt(gap_before)),
            Role::System => {
                for b in &turn.blocks {
                    if let Block::Status(line) = b {
                        out.push(row().key(format!("{}status-{}", w.ns, turn.id)).w_full().justify_center().mt(if w.compact { 8.0 } else { 18.0 }).child(text(line.clone()).font(t.ty.meta).color(t.text.faint).wrap()));
                    }
                }
            }
            Role::Agent => {
                let mut c = col().key(format!("{}turn-{}", w.ns, turn.id)).w_full().gap(4.0).mt(gap_before);
                if shows_speaker {
                    c = c.child(text(speaker).font(rune::TextStyle { weight: 500.0, ..t.ty.meta }).color(t.text.muted).px(4.0));
                }
                let streaming = turn.pending && last;
                let last_text = turn.blocks.iter().rposition(|b| matches!(b, Block::Text(_)));
                for (i, b) in turn.blocks.iter().enumerate() {
                    match b {
                        Block::Text(body) => c = c.child(row().w_full().px(4.0).child(markdown(t, body, streaming && Some(i) == last_text).w_full().min_w(0.0))),
                        Block::Tool(tool) => c = c.child(tool_card(s, t, w, tool, &turn.id)),
                        Block::Status(line) => c = c.child(text(line.clone()).font(t.ty.meta).color(t.text.faint).px(4.0)),
                    }
                }
                if turn.failed {
                    c = c.child(text("This reply didn’t finish.").font(t.ty.meta).color(t.meaning.coral).px(4.0));
                } else if turn.pending && turn.blocks.is_empty() {
                    c = c.child(text("Thinking…").font(t.ty.meta).color(t.text.faint).px(4.0).live_region(rune::LiveRegion::Polite));
                }
                out.push(c);
            }
        }
    }
    out
}

fn user_turn(t: &Neko, w: Where, turn: &Turn) -> El {
    let brief = turn.kind == TurnKind::Brief;
    let body = turn.text();
    let mut bubble = col()
        .key(format!("{}user-{}", w.ns, turn.id))
        .max_w(if w.compact { 260.0 } else { 560.0 })
        .px(12.0)
        .py(8.0)
        .gap(2.0)
        .radius(t.radius.row + 4.0)
        .fill(t.surface.active);
    if brief {
        bubble = bubble.child(text("Request").font(t.ty.id).color(t.text.faint));
    }
    bubble = bubble.child(text(body).font(t.ty.row).color(t.text.base).wrap().selectable(true));
    let mut c = col().w_full().items_end().gap(4.0).child(bubble);
    if let Some(words) = &turn.request {
        c = c.child(text(format!("In your words: “{}”", words.chars().take(240).collect::<String>())).font(t.ty.id).color(t.text.faint).wrap().max_w(460.0));
    }
    c
}
