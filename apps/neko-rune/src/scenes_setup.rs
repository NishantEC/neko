//! Made-up data and named states for the Setup pages' snapshots (`--scene setup-tools`, ...): workspaces,
//! connections (one working, one signed out, one broken, one paused), watches, skills with proposals,
//! memories and guidance, profiles and schedules. Nothing here names anyone or anything real (the
//! repository is public).

use neko_protocol::agent_profiles::{AgentProfile, AgentProfiles, ProfileReadGrant, WorkspaceAssignment};
use neko_protocol::decision_context::{DecisionRecord, PreferenceApplicability, PreferenceState, WorkingPreference};
use neko_protocol::improve::{ImproveEvent, ImproveKind, ImproveReply, ImproveState, Improvement};
use neko_protocol::mcp_host::{McpConnection, McpTool, Responsibility, ServerConfig, SourceLink, ToolGrant};
use neko_protocol::packs::{CatalogEntry, InstalledPack, PackContents, PackHealth, PackInfo, PackListing, PackPreview, PackOrphan, PackRequirement, PackStatus, PluginPower, RoutineText};
use neko_protocol::scheduled_plans::Schedule;
use neko_protocol::setup_import::{ImportCandidate, ImportCandidateKind};
use neko_protocol::skills::{CuratedSkill, Curation, CuratorApplied, EnabledSkill, Skill, SkillPatch, SkillProposal, SkillState};
use neko_protocol::workbench::{MemoryEntry, MemoryKind, MemoryOptions, MemoryProposal, Workspace};

use crate::app::{App, DEMO_NOW};
use crate::link::{SetupAction, SetupData};
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
        open_tasks: [("w1".to_owned(), 3usize)].into_iter().collect(),
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
    s.toggle_ws_line("w1/Routines");
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

// ---- packs (made-up: nothing here names a real tool or person)

fn pack_info(name: &str, title: &str, version: &str, description: &str) -> PackInfo {
    PackInfo {
        name: name.into(),
        title: title.into(),
        version: version.into(),
        description: description.into(),
        homepage: format!("https://packs.example.com/{name}"),
        licence: "MIT".into(),
        permissions: vec!["Reads the notes folder you point it at".into(), "Starts the tool server on this Mac".into()],
        requirements: vec![PackRequirement { name: "demo-cli".into(), hint: "brew install demo-cli".into(), found: true, path: Some("/usr/local/bin/demo-cli".into()) }],
        env: Vec::new(),
        contents: PackContents { connections: vec!["Demo notes".into()], skills: vec!["demo-summary".into()], plugin: None, routines: vec!["Demo digest".into()], luau: Vec::new() },
        health_command: vec!["demo-cli".into(), "--check".into()],
        runs: vec!["demo-cli serve --stdio".into(), "demo-cli --check".into()],
        warnings: Vec::new(),
        plugin: None,
        routine_texts: Vec::new(),
        skills_hash: String::new(),
        luau_hash: String::new(),
    }
}

fn installed_pack(info: PackInfo, source: &str, status: PackStatus, update: Option<&str>, health: Option<(bool, &str)>) -> InstalledPack {
    InstalledPack {
        info,
        source: source.into(),
        status,
        installed_ms: DEMO_NOW - 3 * DAY,
        approved_ms: DEMO_NOW - 3 * DAY,
        approved_digest: "d-approved".into(),
        health: health.map(|(ok, output)| PackHealth { ok, at_ms: DEMO_NOW - 20 * MIN, output: output.into() }),
        connection_ids: vec!["c1".into()],
        skill_paths: Vec::new(),
        plugin_dir: None,
        plugin_disabled: false,
        routine_ids: Vec::new(),
        update_available: update.map(str::to_owned),
        orphans: Vec::new(),
    }
}

pub fn demo_packs() -> PackListing {
    let mut needs = pack_info("demo-board", "Demo board", "0.3.0", "Shows a board of cards from a folder of notes.");
    needs.requirements = vec![PackRequirement { name: "demo-cli".into(), hint: "brew install demo-cli".into(), found: false, path: None }, PackRequirement { name: "demo-sync".into(), hint: "npm install --global demo-sync".into(), found: true, path: Some("/usr/local/bin/demo-sync".into()) }];
    needs.contents = PackContents { connections: vec!["Demo board".into()], skills: vec![], plugin: Some("demo-board".into()), routines: vec![], luau: vec!["layout.luau".into()] };
    let mut failing = pack_info("demo-feed", "Demo feed", "2.0.1", "Reads a feed and files what is new.");
    failing.contents.plugin = None;
    let notes = pack_info("demo-notes", "Demo notes", "1.2.0", "Keeps short notes next to your tasks.");
    let mut sketch = installed_pack(pack_info("demo-sketch", "Demo sketch", "0.9.0", "A small drawing page."), "/work/packs/demo-sketch", PackStatus::Changed, None, None);
    sketch.plugin_dir = Some("/data/plugins/demo-sketch".into());
    sketch.plugin_disabled = true;
    sketch.orphans = vec![PackOrphan { target: "/data/skills/old-sketch".into(), reason: "A folder from the last update could not be deleted.".into() }];
    PackListing {
        installed: vec![
            installed_pack(notes, "catalog:demo-notes", PackStatus::Healthy, Some("1.3.0"), Some((true, "demo-cli 1.2.0\nall checks passed"))),
            installed_pack(needs, "/work/packs/demo-board", PackStatus::NeedsSetup, None, Some((false, "demo-cli: command not found"))),
            installed_pack(failing, "https://git.example.com/team/demo-feed.git", PackStatus::Failing, None, Some((false, "The feed answered 503 twice.\nTry again in a few minutes."))),
            sketch,
        ],
        catalog: vec![
            CatalogEntry { info: pack_info("demo-notes", "Demo notes", "1.3.0", "Keeps short notes next to your tasks."), source: "catalog:demo-notes".into(), installed: true },
            CatalogEntry { info: pack_info("demo-clock", "Demo clock", "0.1.0", "Adds a world clock tool."), source: "catalog:demo-clock".into(), installed: false },
        ],
    }
}

/// A preview for a pack that asks for one value.
pub fn demo_preview() -> PackPreview {
    let mut info = pack_info("demo-clock", "Demo clock", "0.1.0", "Adds a world clock tool.");
    info.env = vec!["DEMO_CLOCK_KEY".into()];
    info.requirements = vec![PackRequirement { name: "demo-cli".into(), hint: "brew install demo-cli".into(), found: false, path: None }];
    info.warnings = vec!["Runs a script interpreter: bash".into(), "Runs inside Neko with access to Neko's own API".into()];
    info.plugin = Some(PluginPower { title: "Demo clock".into(), pages: vec!["Clocks".into()], viewers: vec![], fences: vec!["clock".into()], prompt: "When asked for the time somewhere, use the clock tool.".into(), hash: "ph".into() });
    info.routine_texts = vec![RoutineText { name: "Demo digest".into(), instruction: "Tell me the time in three cities.".into(), cadence: "Daily 09:00".into() }];
    PackPreview { info, source: "catalog:demo-clock".into(), installed_version: None, digest: "digest-clock-1".into(), changes: Vec::new(), needs_approval: false }
}

/// The preview of an update whose power changed.
pub fn demo_update_preview() -> PackPreview {
    let mut info = pack_info("demo-notes", "Demo notes", "1.3.0", "Keeps short notes next to your tasks.");
    info.warnings = vec!["Runs a script interpreter: bash".into()];
    PackPreview { info, source: "catalog:demo-notes".into(), installed_version: Some("1.2.0".into()), digest: "digest-notes-2".into(), changes: vec!["Its tool server now runs: bash -c demo-cli serve".into()], needs_approval: true }
}

fn packs_base(s: &mut App) {
    base(s, SetupSection::Packs);
    s.setup.packs = Some(demo_packs());
}

pub fn packs(s: &mut App) {
    packs_base(s);
}

pub fn packs_empty(s: &mut App) {
    base(s, SetupSection::Packs);
    s.setup.packs = Some(PackListing::default());
}

pub fn pack_detail(s: &mut App) {
    packs_base(s);
    s.open_pack("demo-board");
}

pub fn pack_add(s: &mut App) {
    packs_base(s);
    s.open_pack_dialog();
    s.outbox.clear();
}

pub fn pack_preview(s: &mut App) {
    packs_base(s);
    s.open_pack_dialog();
    s.outbox.clear();
    s.setup.dialog = Some(crate::view::setup::Dialog::Pack { tab: crate::view::setup::PackTab::Catalog, preview: Some(Box::new(demo_preview())), intent: crate::view::setup::PackIntent::Install });
}

pub fn pack_update(s: &mut App) {
    packs_base(s);
    s.update_pack("demo-notes");
    let call = SetupAction::PackPreview { source: "catalog:demo-notes".into() };
    s.pack_previewed(&call, demo_update_preview());
    s.outbox.clear();
}

pub fn pack_changed(s: &mut App) {
    packs_base(s);
    s.open_pack("demo-sketch");
}

pub fn pack_remove(s: &mut App) {
    packs_base(s);
    s.confirm_remove_pack("demo-feed");
}

// ---- improvements (made-up: nothing here names a real person, company or ticket)

#[allow(clippy::too_many_arguments)]
fn suggestion(id: &str, kind: ImproveKind, state: ImproveState, title: &str, why: &str, change: &str, ago: i64, can_undo: bool, undo_confirms: bool) -> Improvement {
    Improvement {
        id: id.into(),
        signal_key: format!("correction:{id}"),
        kind,
        title: title.into(),
        why: why.into(),
        change: change.into(),
        default_label: String::new(),
        target: String::new(),
        workspace_id: "w1".into(),
        evidence: "Asha corrected the same step in 3 reviews of Acme tickets (ENG-123 among them).\nThe agent skipped it each time.".into(),
        diff: String::new(),
        state,
        created_ms: DEMO_NOW - ago,
        applied_ms: matches!(state, ImproveState::Applied | ImproveState::Helped | ImproveState::DidntHelp).then_some(DEMO_NOW - ago + HOUR),
        resolved_ms: None,
        before: 3,
        after: if state == ImproveState::DidntHelp { 2 } else { 0 },
        inbox_seq: None,
        undo_seq: None,
        history: Vec::new(),
        can_undo,
        undo_confirms,
        task_id: None,
    }
}

fn improvements_listing(enabled: bool) -> ImproveReply {
    let mut open = suggestion("i1", ImproveKind::SkillPatch, ImproveState::Open, "Check the ENG-123 link before replying", "Asha asked for the ticket link in the first line of a reply three times.", "", DAY, false, false);
    open.target = "reviewer".into();
    open.default_label = "Improve the reviewer skill".into();
    open.diff = "--- a/SKILL.md\n+++ b/SKILL.md\n@@ -1,3 +1,4 @@\n Review the change.\n+Put the ticket link in the first line.\n-Skip the link.\n Keep it short.".into();
    let applied = suggestion("i2", ImproveKind::StandingOrder, ImproveState::Applied, "Say which branch a pull request targets", "Acme reviews asked for the base branch in every description.", "Name the base branch in each pull request description.", DAY, true, false);
    let rejected = suggestion("i3", ImproveKind::NewSkill, ImproveState::Rejected, "A skill for release notes", "Asha wrote release notes by hand twice this week.", "A new release-notes skill.", 5 * DAY, false, false);
    let mut helped = suggestion("i4", ImproveKind::InstallPack, ImproveState::Helped, "Install the issue tracker pack", "The agent asked for a tracker tool six times.", "", 12 * DAY, true, true);
    helped.target = "issue-tracker".into();
    helped.history = vec![ImproveEvent { at_ms: DEMO_NOW - 12 * DAY, text: "Suggested.".into() }, ImproveEvent { at_ms: DEMO_NOW - 11 * DAY, text: "Applied: the pack's preview was opened.".into() }, ImproveEvent { at_ms: DEMO_NOW - 4 * DAY, text: "The trouble stopped: it helped.".into() }];
    let bad = suggestion("i5", ImproveKind::CodeTask, ImproveState::DidntHelp, "Retry the flaky test setup", "The same setup failure came back in 4 Acme runs.", "Retry the fixture setup once before failing.", 12 * DAY, true, true);
    let sent = suggestion("i6", ImproveKind::NewSkill, ImproveState::Sent, "A skill for ENG-123 style tickets", "Asha filed three similar tickets.", "A new skill for ticket triage.", 2 * HOUR, false, false);
    ImproveReply { enabled, dev_checkout: true, proposals: vec![open, applied, rejected, helped, bad, sent], open_pack: None, note: None }
}

pub fn improvements(s: &mut App) {
    base(s, SetupSection::Improvements);
    s.setup.improve = Some(improvements_listing(true));
}

pub fn improvements_off(s: &mut App) {
    base(s, SetupSection::Improvements);
    s.setup.improve = Some(improvements_listing(false));
}

pub fn improvements_empty(s: &mut App) {
    base(s, SetupSection::Improvements);
    s.setup.improve = Some(ImproveReply { enabled: true, dev_checkout: false, proposals: Vec::new(), open_pack: None, note: None });
}

pub fn improvement_detail(s: &mut App) {
    improvements(s);
    s.improve_open("i1", false);
}

pub fn improvement_detail_pack(s: &mut App) {
    improvements(s);
    s.improve_open("i4", true);
}
