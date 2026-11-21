//! Named states for the cat screens' snapshots and the structure crawl (`--scene cats-3`, `cat-page`, ...): the sidebar with no cat, one,
//! three and five; the New cat sheet (empty, and refused by the daemon); a cat's page; its files and the editor; the archive question.
//! Names are placeholders only (miso, mochi, tofu, Acme).

use neko_protocol::cats::{Cat, CatFile, CatFileKind, CatFiles, FaceRef, ModelPolicy};
use neko_protocol::workbench::RuntimePreferences;

use crate::app::{App, DEMO_NOW};
use crate::link::{CatsData, RuntimeChoice, TaskMeta, WorkspaceRow};

/// Five cats a person might keep, in order; the first three are pinned.
pub fn demo_cats(n: usize) -> Vec<Cat> {
    #[allow(clippy::type_complexity)]
    let rows: [(&str, &str, &str, &str, bool, Option<u64>); 5] = [
        ("c1", "miso", "Web", "black-glasses", true, Some(5_000_000)),
        ("c2", "mochi", "Billing", "persian-glasses", true, Some(5_000_000)),
        ("c3", "tofu", "Infra", "tabby-headphones", true, Some(3_000_000)),
        ("c4", "nori", "Mobile", "cream-bowtie", false, None),
        ("c5", "kuro", "Docs", "calico-welding", false, None),
    ];
    rows.iter()
        .take(n)
        .enumerate()
        .map(|(i, (id, name, area, face, pinned, limit))| Cat {
            id: (*id).into(),
            name: (*name).into(),
            face: FaceRef((*face).into()),
            workspace_id: ["w1", "w2", "w3", "w1", "w2"][i].into(),
            area: (*area).into(),
            model_policy: ModelPolicy::default(),
            daily_limit_micro_usd: *limit,
            purpose: format!("Looks after the {} work: starts tasks, keeps an eye on them and tells you what needs you.", area.to_lowercase()),
            pinned: *pinned,
            order: i as i32,
            created_at_ms: DEMO_NOW - 86_400_000 * (5 - i as i64),
            archived: false,
        })
        .collect()
}

fn base(s: &mut App, n: usize) {
    crate::scenes::live_for_pages(s);
    if let Some(m) = s.summary.as_mut() {
        m.workspaces = vec![WorkspaceRow { id: "w1".into(), name: "Acme web".into() }, WorkspaceRow { id: "w2".into(), name: "Acme api".into() }, WorkspaceRow { id: "w3".into(), name: "Acme infra".into() }];
        // Who started what, so a face shows its marks: mochi has a plan to approve, miso and tofu are building.
        for (task, cat) in [("t6", "c1"), ("t2", "c1"), ("t0", "c2"), ("t7", "c3")] {
            let meta = m.meta.entry(task.to_owned()).or_insert_with(|| TaskMeta { workspace_id: "w1".into(), ..TaskMeta::default() });
            meta.cat_id = Some(cat.to_owned());
        }
    }
    s.cats.list = Some(demo_cats(n));
    s.cats.choices = vec![
        RuntimeChoice { provider: "claude".into(), model: "sonnet".into(), label: "Claude · Sonnet".into() },
        RuntimeChoice { provider: "claude".into(), model: "haiku".into(), label: "Claude · Haiku".into() },
        RuntimeChoice { provider: "codex".into(), model: "gpt".into(), label: "Codex · GPT".into() },
    ];
}

pub fn cats_0(s: &mut App) {
    base(s, 0);
}
pub fn cats_1(s: &mut App) {
    base(s, 1);
}
pub fn cats_3(s: &mut App) {
    base(s, 3);
}
pub fn cats_5(s: &mut App) {
    base(s, 5);
}

/// The New cat sheet over a window with three cats: the next free face, a name and an area typed.
pub fn sheet(s: &mut App) {
    base(s, 3);
    s.open_new_cat();
    s.pages.set_field(crate::cats::F_NAME, "nori");
    s.pages.set_field(crate::cats::F_AREA, "Mobile");
    s.pages.set_field(crate::cats::F_LIMIT, "4.00");
    s.pages.set_field(crate::cats::F_PURPOSE, "Looks after the mobile app: reviews pull requests and keeps the release notes.");
    s.outbox.clear();
}

/// The same sheet after the daemon refused it: the name is taken, and the limit is out of range.
pub fn sheet_errors(s: &mut App) {
    sheet(s);
    s.pages.set_field(crate::cats::F_NAME, "miso");
    if let Some(sheet) = s.cats.sheet.as_mut() {
        sheet.errors.insert("name", "There is already a cat called miso".into());
        sheet.errors.insert("limit", "The daily limit must be between 1 micro-dollar and 1,000,000 dollars".into());
    }
}

/// A cat's page, Overview: miso with a task building and one ready to merge.
pub fn page(s: &mut App) {
    base(s, 3);
    s.open_cat("c1");
}

fn files() -> CatFiles {
    let file = |kind: CatFileKind, text: &str| CatFile { kind, text: text.into(), updated_ms: DEMO_NOW - 3_600_000, prev: None, truncated: false };
    CatFiles {
        id: "c1".into(),
        files: vec![
            file(CatFileKind::Rules, "# Rules\n\nLooks after the web work: starts tasks, keeps an eye on them and tells you what needs you.\n\n- Small pull requests, one change each.\n- Ask before touching the checkout flow.\n"),
            file(CatFileKind::TaskRules, "# Task rules\n\nRun the tests before you say a task is done.\nKeep the diff under 300 lines.\n"),
            file(CatFileKind::Plan, "# Plan\n"),
            file(CatFileKind::Briefing, "# Briefing\n"),
        ],
    }
}

/// Files: the four files of miso.
pub fn files_tab(s: &mut App) {
    page(s);
    s.cat_choose_tab(crate::cats::CatTab::Files);
    s.cats.files.insert("c1".into(), files());
    s.outbox.clear();
}

/// The editor on Rules, clean.
pub fn editor(s: &mut App) {
    files_tab(s);
    s.open_cat_file(CatFileKind::Rules);
}

/// The editor with a change not saved yet.
pub fn editor_dirty(s: &mut App) {
    editor(s);
    let text = format!("{}- Never merge on a Friday.\n", s.pages.text(crate::cats::F_FILE));
    s.pages.set_field(crate::cats::F_FILE, &text);
}

/// Archive asks first.
pub fn archive(s: &mut App) {
    page(s);
    s.ask_archive_cat("c1");
}

/// What the daemon would answer to `List` with these cats (tests feed it to `apply_reply`).
pub fn listing(n: usize) -> CatsData {
    CatsData::Listing(demo_cats(n))
}

/// Preferences a cat could be given in a test.
pub fn claude_haiku() -> RuntimePreferences {
    RuntimePreferences { provider: Some("claude".into()), model: Some("haiku".into()), ..RuntimePreferences::default() }
}
