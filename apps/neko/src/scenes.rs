//! The scenes a headless run can show (`neko --snap out.png --scene task-open`): placeholder data only, never the person's.

use neko_protocol::{Task, TaskId, TaskStatus};
use rune::TextEdit;

use crate::state::App;

pub const SCENES: &[(&str, fn(&mut App))] = &[("empty-home", |_| {}), ("home-typed", home_typed), ("task-open", task_open), ("tasks-home", tasks_home)];

/// A task made `minutes_ago` before the fixed clock.
fn task(n: u32, words: &str, minutes_ago: i64) -> Task {
    let (now, _) = crate::fixed_clock();
    let title = words.lines().next().unwrap_or("").to_owned();
    Task { id: TaskId(format!("task_{n:026}")), workspace: "default".into(), words: words.into(), title, created_ms: now - minutes_ago * 60_000, status: TaskStatus::New }
}

/// Four tasks: three from today and one from earlier in the week.
pub fn sample_tasks() -> Vec<Task> {
    vec![
        task(4, "Draft the release notes for 0.2 and send them to Lee for a read", 12),
        task(3, "Rename the Settings button to Preferences across the app\n\nKeep the keyboard shortcut (⌘,) and check ENG-123 is covered.", 48),
        task(2, "Find out why the nightly build of acme-api is slow", 190),
        task(1, "Tidy the Downloads folder into Archive and Screenshots", 3 * 24 * 60),
    ]
}

fn home_typed(s: &mut App) {
    s.draft = TextEdit::new("Rename the Settings button to Preferences across the app");
    s.draft.focused = true;
}

fn tasks_home(s: &mut App) {
    s.tasks = sample_tasks();
}

fn task_open(s: &mut App) {
    s.tasks = sample_tasks();
    s.open = Some(s.tasks[1].id.clone());
}
