//! Made-up data and named states for the Setup pages' snapshots (`--scene setup-tools`, ...): workspaces,
//! connections (one working, one signed out, one broken, one paused), watches, skills with proposals,
//! memories and guidance, profiles and schedules. Nothing here names anyone or anything real (the
//! repository is public).

use neko_protocol::agent_profiles::{AgentProfile, AgentProfiles, ProfileReadGrant, WorkspaceAssignment};
use neko_protocol::decision_context::{DecisionRecord, PreferenceApplicability, PreferenceState, WorkingPreference};
use neko_protocol::mcp_host::{McpConnection, McpTool, Responsibility, ServerConfig, SourceLink, ToolGrant};
use neko_protocol::scheduled_plans::Schedule;
use neko_protocol::setup_import::{ImportCandidate, ImportCandidateKind};
use neko_protocol::skills::{CuratedSkill, Curation, CuratorApplied, EnabledSkill, Skill, SkillPatch, SkillProposal, SkillState};
use neko_protocol::workbench::{MemoryEntry, MemoryKind, MemoryOptions, MemoryProposal, Workspace};

use crate::app::{App, DEMO_NOW};
use crate::link::SetupData;
use crate::view::setup::{AddTab, SetupSection};
use crate::views::FullView;

const MIN: i64 = 60_000;
const HOUR: i64 = 3_600_000;
const DAY: i64 = 24 * HOUR;

fn tool(name: &str, description: &str, read_only: bool) -> McpTool {
    McpTool { read_only, name: name.into(), description: description.into(), input_schema: "{\"type\":\"object\",\"properties\":{\"query\":{\"type\":\"string\"}}}".into(), schema_hash: format!("h-{name}") }
}

fn connection(id: &str, workspace: &str, label: &str, config: ServerConfig, enabled: bool, error: Option<&str>, tools: Vec<McpTool>) -> McpConnection {
    McpConnection { oauth: false, id: id.into(), workspace_id: workspace.into(), label: label.into(), config, enabled, trusted: true, has_credentials: false, discovered_ms: (!tools.is_empty()).then_some(DEMO_NOW - HOUR), tools, error: error.map(str::to_owned), source_link: None, pack: None }
}

#[allow(clippy::too_many_arguments)]
fn watch(id: &str, workspace: &str, name: Option<&str>, instruction: &str, connection: &str, enabled: bool, last: Option<i64>, failures: u32, result: &str) -> Responsibility {
    Responsibility { id: id.into(), workspace_id: workspace.into(), instruction: instruction.into(), connection_ids: vec![connection.into()], enabled, prepare_low_risk: false, next_due_ms: DEMO_NOW + 8 * MIN, last_attempt_ms: last, last_result: result.into(), failures, name: name.map(str::to_owned), name_by_user: false }
}

fn remembered(id: &str, kind: MemoryKind, workspace: Option<&str>, text: &str, source: &str, ago: i64) -> MemoryEntry {
    MemoryEntry { agent_profile_id: "default".into(), id: id.into(), kind, workspace_id: workspace.map(str::to_owned), text: text.into(), source: source.into(), created_at_ms: DEMO_NOW - ago, updated_at_ms: DEMO_NOW - ago }
}

fn schedule(id: &str, workspace: &str, name: &str, prompt: &str, rule: &str, enabled: bool) -> Schedule {
    Schedule { id: id.into(), name: name.into(), prompt: prompt.into(), workspace_id: Some(workspace.into()), rule: rule.into(), timezone: "Europe/Berlin".into(), anchor_ms: DEMO_NOW - 20 * DAY, next_due_ms: enabled.then_some(DEMO_NOW + 9 * HOUR), enabled, source_id: None, last_task_id: None, last_result: if enabled { "Wrote the plan: three items need a decision.".into() } else { String::new() } }
}

fn preference(id: &str, state: PreferenceState, instruction: &str, terms: &[&str]) -> WorkingPreference {
    WorkingPreference {
        id: id.into(),
        workspace_id: "w1".into(),
        agent_profile_id: "default".into(),
        state,
        applicability: PreferenceApplicability { terms: terms.iter().map(|t| (*t).to_owned()).collect(), task_ids: vec![] },
        instruction: instruction.into(),
        supporting_record_ids: vec![],
        exceptions: vec![],
        version: 2,
        created_at_ms: DEMO_NOW - 3 * DAY,
        updated_at_ms: DEMO_NOW - DAY,
    }
}

fn decision() -> DecisionRecord {
    serde_json::from_value(serde_json::json!({
        "id": "d1", "episode_id": "e1", "version": 1, "supersedes_record_id": null, "workspace_id": "w1", "task_id": null,
        "source": {"fingerprint": "f", "revision": null, "evidence": [], "declared_evidence": []},
        "action": "prepare_fix", "alternatives": [], "rationale": "A small, reversible change.", "provenance": "supervisor",
        "runtime": {"provider": "codex", "model": "", "reasoning_effort": null, "service_tier": null},
        "preference_versions": [], "scope": {"agent_profile_id": "default", "profile_revision": 0, "repository": "/work/neko", "responsibility_ids": [], "connection_ids": []},
        "expected_outcome": null, "observed_outcome": "The test passed after one retry.", "delivery_stage": "local_accepted", "correction": null, "created_at_ms": DEMO_NOW - DAY
    }))
    .expect("a decision record")
}

pub fn demo_setup() -> SetupData {
    let mut tracker = connection("c1", "", "Issue tracker", ServerConfig::Http { url: "https://mcp.tracker.example.com/mcp".into() }, true, None, vec![tool("search_issues", "Find issues by text, assignee or state.", true), tool("get_issue", "Read one issue with its comments.", true), tool("update_issue", "Change an issue's state or assignee.", false)]);
    tracker.has_credentials = true;
    let monitor = connection("c2", "w1", "Error monitor", ServerConfig::Http { url: "https://mcp.monitor.example.com/mcp".into() }, true, Some("OAuth sign-in required (401)"), vec![]);
    let mut files = connection("c3", "w1", "Local files", ServerConfig::Stdio { command: "/usr/local/bin/files-mcp".into(), args: vec!["--root".into(), "/work/neko".into()], cwd: None }, true, Some("Could not start /usr/local/bin/files-mcp: no such file"), vec![]);
    files.source_link = Some(SourceLink { source_path: "/work/neko/.mcp.json".into(), candidate_id: "cand-files".into(), config_hash: "x".into(), executable_identity: None });
    let calendar = connection("c4", "w2", "Team calendar", ServerConfig::Http { url: "https://mcp.calendar.example.com/mcp".into() }, false, None, vec![tool("list_events", "List events in a range.", true)]);
    let skill = |name: &str, description: &str, ws: Option<&str>| Skill { path: format!("/work/neko/.skills/{name}/SKILL.md"), name: name.into(), description: description.into(), source: "workspace".into(), workspace_id: ws.map(str::to_owned), content_hash: format!("hash-{name}") };
    let review = skill("code-review", "Review a change the way this team does", Some("w1"));
    let release = skill("release-notes", "Write release notes from merged pull requests", Some("w1"));
    let triage = skill("bug-triage", "Sort a bug report into the right queue", Some("w1"));
    let skills = SkillState {
        enabled: vec![EnabledSkill { workspace_id: "w1".into(), path: review.path.clone(), content_hash: review.content_hash.clone() }, EnabledSkill { workspace_id: "w1".into(), path: release.path.clone(), content_hash: "old-hash".into() }],
        available: vec![review.clone(), release.clone(), triage],
        proposals: vec![
            SkillProposal {
                id: "p1".into(),
                workspace_id: "w1".into(),
                name: "code-review".into(),
                body: "--- a/SKILL.md\n+++ b/SKILL.md\n@@ -3,5 +3,6 @@\n Review the diff, not the author.\n-Ask for tests on every change.\n+Ask for tests on every change that alters behaviour.\n+Skip style notes a formatter would fix.\n Keep comments to one idea each.\n".into(),
                content_hash: "c1".into(),
                source: "Improvement from a lesson in 3 reviews".into(),
                audit_url: None,
                audit_status: "Made by Neko from your own reviews; nothing was fetched.".into(),
                audit_reviewed_hash: None,
                patch: Some(SkillPatch { path: review.path.clone(), base_hash: "b".into(), revised: "x".into(), name: "code-review".into() }),
            },
            SkillProposal {
                id: "p2".into(),
                workspace_id: "w1".into(),
                name: "changelog".into(),
                body: "# Changelog\n\nWhen asked for a changelog, group merged pull requests under Added, Changed and Fixed.\nUse the pull request title, not its description.\n".into(),
                content_hash: "c2".into(),
                source: "github.com/example/skills (SKILL.md)".into(),
                audit_url: Some("https://skills.example.com/audit/changelog".into()),
                audit_status: "A published audit exists. Read it before you accept.".into(),
                audit_reviewed_hash: None,
                patch: None,
            },
        ],
        curator: Curation {
            off: false,
            last_run_ms: DEMO_NOW - 6 * HOUR,
            skills: vec![CuratedSkill { path: review.path.clone(), state: "neko".into() }, CuratedSkill { path: release.path.clone(), state: "locked".into() }],
            recent: vec![CuratorApplied { path: release.path.clone(), name: "release-notes".into(), at_ms: DEMO_NOW - 2 * DAY, diff: "@@ -8,3 +8,2 @@\n-Mention every contributor twice.\n Mention each contributor once.\n".into() }],
        },
    };
    let candidate = |id: &str, name: &str, local: bool, problem: Option<&str>| ImportCandidate {
        id: id.into(),
        kind: ImportCandidateKind::Connection,
        source: "Claude Code project settings".into(),
        scope: "project".into(),
        workspace: Some("/work/neko".into()),
        name: name.into(),
        metadata: [("transport".to_owned(), if local { "stdio" } else { "http" }.to_owned()), ("config_summary".to_owned(), if local { "/usr/local/bin/browser-mcp --headless" } else { "https://mcp.docs.example.com/mcp" }.to_owned())].into_iter().collect(),
        problem: problem.map(str::to_owned),
    };
    SetupData {
        workspaces: vec![
            Workspace { id: "w1".into(), name: "Neko".into(), repository: "/work/neko".into(), instructions: "Small pull requests, tests first.\nNever touch the release branch.".into(), away_enabled: false },
            Workspace { id: "w2".into(), name: "Docs site".into(), repository: "/work/docs-site".into(), instructions: String::new(), away_enabled: false },
        ],
        folders: [("w1".to_owned(), vec!["/work/neko".to_owned(), "/work/neko-web".to_owned()]), ("w2".to_owned(), vec!["/work/docs-site".to_owned()])].into_iter().collect(),
        connections: vec![tracker, monitor, files, calendar],
        grants: vec![ToolGrant { connection_id: "c1".into(), workspace_id: "w1".into(), tool_name: "search_issues".into(), schema_hash: "h-search_issues".into() }],
        watches: vec![
            watch("r1", "w1", Some("Assigned issues"), "New issues assigned to me, and anything of mine that becomes blocked", "c1", true, Some(DEMO_NOW - 12 * MIN), 0, "Two new issues: both already have a plan."),
            watch("r2", "w1", None, "New or spiking errors in my projects", "c2", false, Some(DEMO_NOW - DAY), 2, ""),
            watch("r3", "w2", None, "Pages that changed without a review", "c1", false, None, 0, ""),
        ],
        suggested: vec!["r3".into()],
        candidates: vec![candidate("cand-docs", "docs-search", false, None), candidate("cand-browser", "browser-tools", true, None), candidate("cand-broken", "legacy-bridge", true, Some("Its command is not an absolute path"))],
        skills,
        memory: vec![
            remembered("m1", MemoryKind::Profile, None, "I prefer small pull requests, with tests first.", "user", 5 * DAY),
            remembered("m2", MemoryKind::Profile, None, "Answer in short sentences. Say what changed, then why.", "chat", 9 * DAY),
            remembered("m3", MemoryKind::Workspace, Some("w1"), "Run the check script before every push; it takes about a minute.", "ticket:ab12", 2 * DAY),
            remembered("m4", MemoryKind::Decision, Some("w1"), "Retries belong in the job runner, never in the caller.", "chat", DAY),
        ],
        memory_proposals: vec![MemoryProposal { id: "mp1".into(), agent_profile_id: "default".into(), workspace_id: Some("w2".into()), kind: MemoryKind::Workspace, text: "The docs site builds from the main branch every night.".into(), source: "ticket:cd34".into(), created_at_ms: DEMO_NOW - HOUR }],
        memory_options: Some(MemoryOptions::default()),
        preferences: vec![preference("g1", PreferenceState::Proposed, "Prefer the smallest fix that makes the failing test pass.", &["flaky test", "failing test"]), preference("g2", PreferenceState::Confirmed, "Ask before changing a public interface.", &["public interface", "breaking change"])],
        decisions: vec![decision()],
        profiles: AgentProfiles {
            profiles: vec![AgentProfile { id: "default".into(), name: "Neko".into(), instructions: String::new() }, AgentProfile { id: "reviewer".into(), name: "Reviewer".into(), instructions: "Be terse. Review like a staff engineer.\nFlag risk first, style last.".into() }],
            assignments: vec![WorkspaceAssignment { workspace_id: "w2".into(), profile_id: "reviewer".into() }],
            read_grants: vec![ProfileReadGrant { reader_id: "reviewer".into(), source_id: "default".into() }],
            active_profile_id: "default".into(),
            revision: 3,
        },
        schedules: vec![schedule("s1", "w1", "Morning brief", "Summarise what changed overnight and what needs me today.", "FREQ=DAILY;BYHOUR=8;BYMINUTE=30", true), schedule("s2", "w1", "Weekly review", "Look back over the week and propose three priorities.", "FREQ=WEEKLY;BYDAY=FR;BYHOUR=16;BYMINUTE=0", false), schedule("s3", "w2", "Link check", "Find broken links in the published pages.", "FREQ=HOURLY;INTERVAL=6", true)],
        plan_first: true,
    }
}

/// Setup over the usual made-up workbench, on a section, with its data in.
fn base(s: &mut App, section: SetupSection) {
    super::scenes::live_for_pages(s);
    s.now_ms = DEMO_NOW;
    if let Some(m) = s.summary.as_mut() {
        m.setup = demo_setup();
    }
    s.pages.routines = Some(super::scenes_pages::routines_data());
    s.open_setup(section);
    s.outbox.clear();
}

pub fn workspaces(s: &mut App) {
    base(s, SetupSection::Workspaces);
}

pub fn workspaces_open(s: &mut App) {
    base(s, SetupSection::Workspaces);
    s.toggle_workspace("w1");
}

pub fn workspace_dialog(s: &mut App) {
    base(s, SetupSection::Workspaces);
    s.open_workspace_dialog(Some("w1"));
}

pub fn watching(s: &mut App) {
    base(s, SetupSection::Watching);
    s.toggle_row("watch:r1");
}

pub fn watch_dialog(s: &mut App) {
    base(s, SetupSection::Watching);
    s.open_watch_dialog(None, "w1");
}

pub fn tools(s: &mut App) {
    base(s, SetupSection::Tools);
}

pub fn tool_detail(s: &mut App) {
    base(s, SetupSection::Tools);
    s.open_connection("c1");
    s.toggle_row("tool:c1:update_issue");
}

pub fn tool_signin(s: &mut App) {
    base(s, SetupSection::Tools);
    s.open_connection("c2");
}

pub fn add_connection(s: &mut App) {
    base(s, SetupSection::Tools);
    s.open_add_connection(AddTab::Import);
}

pub fn skills(s: &mut App) {
    base(s, SetupSection::Skills);
    s.toggle_row("proposal:p1");
    s.toggle_row("skill:/work/neko/.skills/release-notes/SKILL.md");
}

pub fn memory(s: &mut App) {
    base(s, SetupSection::Memory);
}

pub fn profiles(s: &mut App) {
    base(s, SetupSection::Profiles);
    s.toggle_row("profile:reviewer");
}

pub fn schedules(s: &mut App) {
    base(s, SetupSection::Schedules);
    s.toggle_row("schedule:s1");
}

pub fn confirm(s: &mut App) {
    base(s, SetupSection::Schedules);
    s.confirm_delete_schedule("s2");
}

/// The sessions column's Setup row with its count: the broken connections and the proposals waiting.
pub fn counted(s: &mut App) {
    base(s, SetupSection::Tools);
    s.view = Some(FullView::Setup);
}
