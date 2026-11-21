//! The scenes the look is measured with (`apps/neko-rune/scripts/look-pairs.mjs`): Home and a cat's page filled with the prototype's own
//! placeholder data (`~/Documents/neko-design/prototype-2026-11-21/_src/data.js`: Asha, miso · Web, mochi · Billing, tofu · Infra, the same
//! task titles, the same numbers), at 18:40 local so the greeting is the evening one. Placeholders only.

use neko_protocol::cats::{CatFile, CatFileKind, CatFiles};
use neko_protocol::digest::Digest;
use neko_protocol::routines::SpendToday;
use neko_protocol::workbench::TaskStatus;

use crate::app::{App, DEMO_NOW};
use crate::clock;
use crate::link::{CalendarData, LinkEvent, RuntimeChoice, Summary, TaskLine, TaskMeta, TaskRow, Wait, WorkspaceRow};

const MIN: i64 = 60_000;
const HOUR: i64 = 3_600_000;

/// 18:40 on the demo day, local time.
fn look_now() -> i64 {
    clock::day_start(DEMO_NOW) + 18 * HOUR + 40 * MIN
}

#[allow(clippy::too_many_arguments)]
fn task(id: &str, title: &str, status: TaskStatus, ago_min: i64, cat: &str, pr: &str, progress: Option<(u32, u32)>, note: &str) -> (TaskRow, TaskMeta) {
    let row = TaskRow {
        id: id.into(),
        key: None,
        title: title.into(),
        status,
        updated_at_ms: look_now() - ago_min * MIN,
        goal: String::new(),
        wait: if status == TaskStatus::AwaitingApproval { Wait::Question } else if status == TaskStatus::ReadyForReview { Wait::Review } else { Wait::None },
        progress,
        line: TaskLine { pr: pr.into(), note: note.into(), ..TaskLine::default() },
    };
    let meta = TaskMeta { workspace_id: ["w1", "w2", "w3"][["c1", "c2", "c3"].iter().position(|c| *c == cat).unwrap_or(0)].into(), cat_id: Some(cat.into()), created_at_ms: look_now() - ago_min * MIN - HOUR, pull_request: (!pr.is_empty()).then(|| pr.into()), ..TaskMeta::default() };
    (row, meta)
}

fn base(s: &mut App) {
    s.clock = look_now;
    s.now_ms = look_now();
    s.account.name = "Asha".into();
    let all = [
        task("s1", "Update the Orders empty-state copy", TaskStatus::ReadyForReview, 120, "c1", "#211", None, ""),
        task("d3", "Bump the date picker", TaskStatus::Completed, 300, "c1", "#204", None, ""),
        task("s2", "Retry failed payments", TaskStatus::AwaitingApproval, 16, "c2", "", None, "ENG-238 · default: yes, at 19:00 today"),
        task("d1", "Fix duplicate invoice emails", TaskStatus::Completed, 180, "c2", "#212", None, ""),
        task("s3", "Rotate the staging keys", TaskStatus::Building, 5, "c3", "", Some((2, 5)), ""),
        task("d2", "Pin the base image versions", TaskStatus::Completed, 240, "c3", "#213", None, ""),
    ];
    let mut summary = Summary { digest: "look".into(), runtime: "Claude · Sonnet".into(), workspaces: vec![WorkspaceRow { id: "w1".into(), name: "acme-web".into() }, WorkspaceRow { id: "w2".into(), name: "acme-api".into() }, WorkspaceRow { id: "w3".into(), name: "acme-infra".into() }], ..Summary::default() };
    for (row, meta) in all {
        summary.meta.insert(row.id.clone(), meta);
        summary.tasks.push(row);
    }
    s.apply_link(LinkEvent::Live(summary));
    let mut cats = crate::scenes_cats::demo_cats(3);
    // miso answers on Claude Sonnet, as the composer's chip says.
    cats[0].model_policy.preferences = neko_protocol::workbench::RuntimePreferences { provider: Some("claude".into()), model: Some("sonnet".into()), ..Default::default() };
    s.cats.list = Some(cats);
    s.cats.choices = vec![
        RuntimeChoice { provider: "claude".into(), model: "sonnet".into(), label: "Claude · Sonnet".into() },
        RuntimeChoice { provider: "claude".into(), model: "haiku".into(), label: "Claude · Haiku".into() },
    ];
    // Seven rounds, two events today, $6.20 spent, no Inbox item (what waits for you is mochi's task).
    let mut routines = crate::scenes_pages::routines_data();
    for v in &mut routines.views {
        v.routine.enabled = true;
        for run in &mut v.runs {
            run.needs_you.clear();
        }
    }
    let spare = routines.views[0].clone();
    for n in 0..2 {
        let mut v = spare.clone();
        v.routine.id = format!("rx{n}");
        routines.views.push(v);
    }
    routines.spend_today = SpendToday { tasks_usd: 6.20, routines_usd: 0.0, ceiling_cents: Some(2000), held: false };
    s.pages.routines = Some(routines);
    let mut cal: CalendarData = crate::scenes_pages::calendar_data(look_now(), 0, 0);
    let day = clock::day_start(look_now());
    cal.events.truncate(2);
    for (i, e) in cal.events.iter_mut().enumerate() {
        e.start_ms = day + (9 + i as i64) * HOUR;
        e.end_ms = e.start_ms + HOUR;
        e.all_day = false;
    }
    s.pages.calendar = Some(cal);
    s.neko.digest = Some(Digest {
        at_ms: day + 17 * HOUR + 40 * MIN,
        headline: "**3 tasks merged** since this morning ($0.88). **mochi** needs a decision on the retry rule before 19:00. **tofu** is rotating the staging keys, 2 of 5 done. **miso** has one pull request ready to merge (PR #211).".into(),
        ..Digest::default()
    });
    s.neko.loaded = true;
    s.layout.context.set_open(false);
}

/// Home as the prototype's `home-0`.
pub fn home(s: &mut App) {
    base(s);
    s.open_new();
    if let Some(f) = s.new_form.as_mut() {
        f.choices = s.cats.choices.clone();
        f.runtime = Some("claude".into());
        f.model = Some("sonnet".into());
    }
    // A request in the box is the typed state; the measured one is empty.
    s.outbox.clear();
}

/// A cat's page as the prototype's `cat-miso-0`: miso, Overview.
pub fn cat(s: &mut App) {
    base(s);
    s.open_cat("c1");
    let file = |kind: CatFileKind, text: &str| CatFile { kind, text: text.into(), updated_ms: clock::day_start(look_now()) + 17 * HOUR + 40 * MIN, prev: None, truncated: false };
    s.cats.files.insert(
        "c1".into(),
        CatFiles {
            id: "c1".into(),
            files: vec![
                file(CatFileKind::Rules, "# Rules\n"),
                file(CatFileKind::TaskRules, "# Task rules\n"),
                file(CatFileKind::Plan, "# Plan\n"),
                file(CatFileKind::Briefing, "# Briefing\n\nGood evening, Asha. The date picker bump is merged (PR #204) and the Orders empty-state copy is ready to merge (PR #211). The 8:30 round found nothing to fix.\n"),
            ],
        },
    );
    s.outbox.clear();
}
