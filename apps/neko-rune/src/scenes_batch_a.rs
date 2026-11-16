//! Scenes for parity batch A (the owner's brief of 2026-11-16): the pictures in `docs/design/rune/batch-a-*.png` and the headless
//! tests read them. Synthetic data only, like every committed scene.

use neko_protocol::agent_models::ModelCatalog;
use neko_protocol::workbench::{ChatMessage, TaskStatus};

use crate::app::App;
use crate::compose::{Chip, ChipState, Dest};
use crate::link::InboxItem;
use crate::pages::CalMode;
use crate::scenes::{demo_full, live_scene};
use crate::scenes_pages::{calendar_data, calendar_now};

fn live(s: &mut App) {
    live_scene(s);
}

fn catalog() -> ModelCatalog {
    serde_json::from_value(serde_json::json!({
        "sources": [
            {"provider": "claude", "label": "Claude", "connection": "Pro", "status": "ready", "models": [
                {"id": "opus", "label": "Opus", "description": "The deepest reasoning", "access": "listed", "recommended": true,
                 "effort_options": [{"id": "low", "label": "Low"}, {"id": "medium", "label": "Medium"}, {"id": "high", "label": "High"}], "speed_options": []},
                {"id": "sonnet", "label": "Sonnet", "access": "listed", "effort_options": [{"id": "low", "label": "Low"}, {"id": "high", "label": "High"}], "speed_options": []}
            ]},
            {"provider": "codex", "label": "Codex", "connection": "Plus", "status": "ready", "models": [
                {"id": "gpt-5.5", "label": "GPT-5.5 Codex", "access": "checked", "effort_options": [{"id": "high", "label": "High"}], "speed_options": [{"id": "priority", "label": "Fast"}]}
            ]}
        ]
    }))
    .expect("a catalog")
}

fn message(id: &str, role: &str, text: &str, queued: bool) -> ChatMessage {
    serde_json::from_value(serde_json::json!({"id": id, "at_ms": 1, "started_ms": if role == "neko" { 1 } else { 0 }, "role": role, "queued": queued, "parts": [{"type": "text", "text": text}]})).expect("a message")
}

fn closed_inbox(s: &mut App) {
    let now = s.now_ms;
    let item = |seq: u64, title: &str, reason: &str, ago_h: i64| InboxItem {
        seq,
        title: title.into(),
        detail: String::new(),
        routine_id: "r-messages".into(),
        run_id: format!("run-{seq}"),
        closed_ms: Some(now - ago_h * 3_600_000),
        reason: reason.into(),
        from: "Messages".into(),
        at_ms: now - ago_h * 3_600_000,
        ..InboxItem::default()
    };
    s.neko.closed = vec![item(11, "Review the migration PR", "PR merged", 2), item(10, "Reply to Sam about the budget", "Done by you", 26), item(9, "Enable the calendar API", "Aged out", 60)];
}

/// Runs the scene called `name` (the list in `scenes.rs` points here).
pub fn run(name: &str, s: &mut App) {
    if let Some((_, f)) = BATCH_A.iter().find(|(n, _)| *n == name) {
        f(s);
    }
}

const BATCH_A: &[crate::scenes::Scene] = &[
    // 1. the header: opaque with a fade, the sky badge, and the model chip's popover.
    ("ba-review", |s| {
        live(s);
        s.apply_link(crate::link::LinkEvent::Live({
            let mut m = demo_full("t0");
            for t in m.tasks.iter_mut().filter(|t| t.id == "t0") {
                t.status = TaskStatus::ReadyForReview;
            }
            if let Some(d) = m.detail.as_mut() {
                d.task.status = TaskStatus::ReadyForReview;
            }
            m
        }));
    }),
    ("ba-model", |s| {
        live(s);
        s.runtime.catalog = Some(catalog());
        s.runtime_toggle();
        s.runtime.catalog = Some(catalog());
        s.runtime.loading = false;
        s.runtime.draft.provider = Some("claude".into());
        s.runtime.draft.model = Some("opus".into());
        s.runtime.draft.reasoning_effort = Some("high".into());
        s.outbox.clear();
    }),
    // 2. the task menu, the confirm dialog, All sessions and Stop all.
    ("ba-menu", |s| {
        live(s);
        s.session_menu = true;
    }),
    ("ba-delete", |s| {
        live(s);
        // The dialog the menu's Delete opens (the session is made finished so the item exists).
        if let Some(t) = s.summary.as_mut().and_then(|m| m.tasks.iter_mut().find(|t| t.id == "t0")) {
            t.status = TaskStatus::Failed;
        }
        s.do_action("t0", crate::task_actions::Act::Delete);
    }),
    ("ba-stop-all", |s| {
        live(s);
        s.ask_stop_all();
    }),
    ("ba-all-sessions", |s| {
        live(s);
        s.open_all_sessions();
        s.all.cursor_shown = true;
        s.all.cursor = 1;
    }),
    // 3. the composer: the popover, files attached, the drop overlay, the queue tray, the message menu.
    ("ba-slash", |s| {
        live(s);
        s.ask.replace_text("/");
        s.ask.focused = true;
    }),
    ("ba-mention", |s| {
        live(s);
        if let Some(m) = s.summary.as_mut() {
            m.connections = vec![("c1".into(), "Linear".into(), 12), ("c2".into(), "Calendar".into(), 7)];
        }
        s.compose.files = vec![crate::link::http::MentionFile { path: "/Users/me/neko/README.md".into(), name: "README.md".into(), location: "~/neko".into() }];
        s.ask.replace_text("look at @");
        s.ask.focused = true;
    }),
    ("ba-chips", |s| {
        live(s);
        s.composer.replace_text("See the notes and the screenshot");
        s.compose.chips.insert(
            Dest::Reply,
            vec![
                Chip { id: 1, name: "notes.txt".into(), size: 3300, state: ChipState::Ready { reference: "[notes.txt](file:///a)".into(), is_image: false } },
                Chip { id: 2, name: "screenshot.png".into(), size: 1_450_000, state: ChipState::Uploading },
                Chip { id: 3, name: "huge.zip".into(), size: 0, state: ChipState::Failed("Choose a file under 32 MB.".into()) },
            ],
        );
    }),
    ("ba-drop", |s| {
        live(s);
        s.compose.drop_over = true;
    }),
    ("ba-queue", |s| {
        live(s);
        if let Some(m) = s.summary.as_mut() {
            m.conversation.push(message("q1", "user", "Also check the dark theme", true));
            m.conversation.push(message("q2", "user", "And write a changelog line when it is done", true));
        }
    }),
    // 4. the palette, the shortcuts, the text menu, the switcher.
    ("ba-palette", |s| {
        live(s);
        s.plugin_pages = Some(vec![neko_protocol::PluginPage { plugin: "board".into(), id: "board:kanban".into(), title: "Kanban board".into() }]);
        s.palette_toggle();
    }),
    ("ba-palette-typed", |s| {
        live(s);
        s.palette_toggle();
        s.palette_query.replace_text("sett");
    }),
    ("ba-shortcuts", |s| {
        live(s);
        s.shortcuts_open = true;
    }),
    ("ba-text-menu", |s| {
        live(s);
        s.text_menu = Some(("The board component draws nothing when there are no tasks".into(), rune::PointerPress { x: 520.0, y: 360.0 }));
    }),
    ("ba-scope", |s| {
        live(s);
        s.scope_open = true;
    }),
    // 5. the Inbox's Closed recently, open.
    ("ba-inbox", |s| {
        live(s);
        closed_inbox(s);
        s.toggle("closed:open");
    }),
    // 6. the routine cadence editor (Every, Daily), the Month and an event's popover.
    ("ba-cadence", crate::scenes_pages::routines_open),
    ("ba-cadence-daily", |s| {
        crate::scenes_pages::routines_open(s);
        s.set_cadence_mode(false);
    }),
    ("ba-month", |s| {
        crate::scenes_pages::calendar(s);
        s.cal_mode(CalMode::Month);
        s.now_ms = calendar_now();
        let (from, to) = s.calendar_range();
        s.pages.calendar = Some(calendar_data(s.now_ms, from, to));
    }),
    ("ba-event", |s| {
        crate::scenes_pages::calendar_day(s);
        let (from, to) = s.calendar_range();
        if let Some(d) = s.pages.calendar.as_mut() {
            // A meeting on the day shown, with a meeting address.
            if let Some(e) = d.events.iter_mut().find(|e| !e.all_day && e.start_ms >= from && e.start_ms < to) {
                e.url = Some("https://meet.google.com/abc-defg-hij".into());
                e.location = Some("Room 4".into());
                s.pages.cal_event = Some(e.id.clone());
            }
        }
    }),
    // 7. toasts.
    ("ba-toasts", |s| {
        live(s);
        s.show_toast("Renamed.");
        s.show_toast("Moved to the front of the queue.");
        s.toast_with("Reviewed tasks here now land on local main.", "Undo", crate::link::Call::Unseen);
    }),
];
