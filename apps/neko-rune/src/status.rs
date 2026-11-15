//! One status language (DESIGN.md §3): every task, routine item and run uses these glyphs,
//! everywhere, always with words beside them or as their label. The same seven as the web UI's
//! `web/src/ui/status-glyph.tsx`, with the same geometry (a 16-unit box):
//!
//! | Kind | Glyph | Colour | Means |
//! | --- | --- | --- | --- |
//! | Backlog | dotted circle | faint | not started, nothing scheduled |
//! | Todo | empty circle | muted | queued, will start |
//! | InProgress | half-filled circle | Amber | working, or waiting on you |
//! | Review | circle with a centre dot | Sky | ready for you to review |
//! | Done | check in a filled circle | Mint | finished |
//! | Failed | cross in a filled circle | Coral | stopped with an error |
//! | Cancelled | cross in a filled circle | faint | stopped on purpose |
//!
//! `status_glyph(kind, &theme, size)` is the component; `status_of` maps a daemon `TaskStatus`.

use neko_protocol::workbench::TaskStatus;
use rune::{Color, El, Icon, el, icon};

use crate::theme::Neko;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StatusKind {
    Backlog,
    Todo,
    InProgress,
    Review,
    Done,
    Failed,
    Cancelled,
}

impl StatusKind {
    pub const ALL: [StatusKind; 7] = [
        StatusKind::Backlog,
        StatusKind::Todo,
        StatusKind::InProgress,
        StatusKind::Review,
        StatusKind::Done,
        StatusKind::Failed,
        StatusKind::Cancelled,
    ];

    /// The glyph's accessible name when no words sit beside it.
    pub fn label(self) -> &'static str {
        match self {
            StatusKind::Backlog => "Backlog",
            StatusKind::Todo => "To do",
            StatusKind::InProgress => "In progress",
            StatusKind::Review => "In review",
            StatusKind::Done => "Done",
            StatusKind::Failed => "Failed",
            StatusKind::Cancelled => "Cancelled",
        }
    }
}

/// A task's status as one of the glyphs (`statusOf` in the web UI).
pub fn status_of(status: TaskStatus) -> StatusKind {
    match status {
        TaskStatus::Queued => StatusKind::Todo,
        TaskStatus::Planning | TaskStatus::Building | TaskStatus::Reviewing | TaskStatus::AwaitingApproval => StatusKind::InProgress,
        TaskStatus::ReadyForReview => StatusKind::Review,
        TaskStatus::Completed => StatusKind::Done,
        TaskStatus::Failed => StatusKind::Failed,
        TaskStatus::Cancelled => StatusKind::Cancelled,
    }
}

/// The words for a task status, more exact than the glyph's: `AwaitingApproval` is "Needs you".
pub fn status_label(status: TaskStatus) -> &'static str {
    match status {
        TaskStatus::AwaitingApproval => "Needs you",
        TaskStatus::Planning => "Planning",
        other => status_of(other).label(),
    }
}

static DOTTED: Icon = Icon {
    name: "neko-status-backlog",
    view: 16.0,
    // Ten dots on the ring: the web draws a dashed stroke, which Rune's icon rasteriser draws
    // solid, so the dashes are written out.
    body: r#"<g fill="currentColor"><circle cx="14" cy="8" r="0.8"/><circle cx="12.85" cy="11.53" r="0.8"/><circle cx="9.85" cy="13.71" r="0.8"/><circle cx="6.15" cy="13.71" r="0.8"/><circle cx="3.15" cy="11.53" r="0.8"/><circle cx="2" cy="8" r="0.8"/><circle cx="3.15" cy="4.47" r="0.8"/><circle cx="6.15" cy="2.29" r="0.8"/><circle cx="9.85" cy="2.29" r="0.8"/><circle cx="12.85" cy="4.47" r="0.8"/></g>"#,
};
static EMPTY: Icon = Icon {
    name: "neko-status-todo",
    view: 16.0,
    body: r#"<circle cx="8" cy="8" r="6" fill="none" stroke="currentColor" stroke-width="1.5"/>"#,
};
static HALF: Icon = Icon {
    name: "neko-status-in-progress",
    view: 16.0,
    body: r#"<circle cx="8" cy="8" r="6" fill="none" stroke="currentColor" stroke-width="1.5"/><path d="M8 4.5a3.5 3.5 0 0 1 0 7z" fill="currentColor"/>"#,
};
static RING_DOT: Icon = Icon {
    name: "neko-status-review",
    view: 16.0,
    body: r#"<circle cx="8" cy="8" r="6" fill="none" stroke="currentColor" stroke-width="1.5"/><circle cx="8" cy="8" r="2.25" fill="currentColor"/>"#,
};
static CHECK: Icon = Icon {
    name: "neko-status-check",
    view: 16.0,
    body: r#"<path d="M5.2 8.2l1.9 1.9 3.7-4" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"/>"#,
};
static CROSS: Icon = Icon {
    name: "neko-status-cross",
    view: 16.0,
    body: r#"<path d="M5.8 5.8l4.4 4.4M10.2 5.8l-4.4 4.4" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"/>"#,
};

/// The glyph's colour in a theme (`TONE` in the web UI).
pub fn status_color(kind: StatusKind, t: &Neko) -> Color {
    match kind {
        StatusKind::Backlog | StatusKind::Cancelled => t.text.faint,
        StatusKind::Todo => t.text.muted,
        StatusKind::InProgress => t.meaning.amber,
        StatusKind::Review => t.meaning.sky,
        StatusKind::Done => t.meaning.mint,
        StatusKind::Failed => t.meaning.coral,
    }
}

/// The glyph, `size` points square (14 in rows). Named for assistive technology by the status's
/// words unless `label` is `Some("")`, for use beside the words.
pub fn status_glyph(kind: StatusKind, t: &Neko, size: f32, label: Option<&str>) -> El {
    let colour = status_color(kind, t);
    let body = match kind {
        StatusKind::Backlog => icon(&DOTTED, size).color(colour),
        StatusKind::Todo => icon(&EMPTY, size).color(colour),
        StatusKind::InProgress => icon(&HALF, size).color(colour),
        StatusKind::Review => icon(&RING_DOT, size).color(colour),
        // A filled disc with the mark cut out in the window's colour, as the web draws it.
        StatusKind::Done => disc(colour, icon(&CHECK, size).color(t.surface.window), size),
        StatusKind::Failed | StatusKind::Cancelled => disc(colour, icon(&CROSS, size).color(t.surface.window), size),
    };
    let name = label.unwrap_or(kind.label());
    let body = body.key(format!("status-{kind:?}"));
    if name.is_empty() { body.a11y_hidden() } else { body.role("img").label(name) }
}

fn disc(colour: Color, mark: El, size: f32) -> El {
    // In the 16-unit box the disc has radius 7, so it sits 1 unit in from each edge.
    let d = size * 14.0 / 16.0;
    let inset = size / 16.0;
    el().size(size, size)
        .child(el().size(d, d).radius(d / 2.0).fill(colour).absolute(inset, inset))
        .child(mark.size(size, size).absolute(0.0, 0.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_task_status_has_a_glyph_and_words() {
        use TaskStatus::*;
        let table = [
            (Queued, StatusKind::Todo, "To do"),
            (Planning, StatusKind::InProgress, "Planning"),
            (AwaitingApproval, StatusKind::InProgress, "Needs you"),
            (Building, StatusKind::InProgress, "In progress"),
            (Reviewing, StatusKind::InProgress, "In progress"),
            (ReadyForReview, StatusKind::Review, "In review"),
            (Completed, StatusKind::Done, "Done"),
            (Failed, StatusKind::Failed, "Failed"),
            (Cancelled, StatusKind::Cancelled, "Cancelled"),
        ];
        for (status, kind, words) in table {
            assert_eq!(status_of(status), kind, "{status:?}");
            assert_eq!(status_label(status), words, "{status:?}");
        }
    }

    #[test]
    fn colour_means_one_thing() {
        for dark in [true, false] {
            let t = crate::theme::theme(dark);
            assert_eq!(status_color(StatusKind::InProgress, &t).hex(), t.meaning.amber.hex());
            assert_eq!(status_color(StatusKind::Review, &t).hex(), t.meaning.sky.hex());
            assert_eq!(status_color(StatusKind::Done, &t).hex(), t.meaning.mint.hex());
            assert_eq!(status_color(StatusKind::Failed, &t).hex(), t.meaning.coral.hex());
            // A stopped-on-purpose task and a backlog item are quiet: nothing needs anyone.
            assert_eq!(status_color(StatusKind::Cancelled, &t).hex(), t.text.faint.hex());
            assert_eq!(status_color(StatusKind::Backlog, &t).hex(), t.text.faint.hex());
        }
    }

    #[test]
    fn labels_are_distinct() {
        let mut seen: Vec<&str> = StatusKind::ALL.iter().map(|k| k.label()).collect();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), StatusKind::ALL.len());
    }
}
