//! The Setup pages (Workspaces, Watching, Tools, Skills, Memory, Profiles, Schedules) as used, driven through
//! Rune's headless `Runtime`: over made-up data (what each section shows, what its controls ask of the daemon,
//! that every destructive action asks first) and against a real daemon, the verification harness
//! `web_harness` on an isolated data folder (one create, one edit and one destructive action with its confirm
//! per section). Without the harness binary the daemon tests say so and pass, as `tests/harness.rs` does.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use neko_rune::app::{App, Dispatch};
use neko_rune::link::{Call, SetupAction};
use neko_rune::scenes;
use neko_rune::view::setup::{AddTab, Dialog, SetupSection};
use neko_rune::view::view;
use neko_rune::views::FullView;
use rune::{Env, Runtime};

const WINDOW: (f32, f32) = (1360.0, 900.0);

fn settle(rt: &mut Runtime<App>) {
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
}

fn click(rt: &mut Runtime<App>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("no element keyed {key}"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    settle(rt);
}

fn texts(rt: &Runtime<App>) -> Vec<String> {
    rt.elements().into_iter().filter_map(|e| e.text).collect()
}

fn has_text(rt: &Runtime<App>, needle: &str) -> bool {
    texts(rt).iter().any(|t| t.contains(needle))
}

fn fill(rt: &mut Runtime<App>, key: &str, text: &str) {
    let (key, text) = (key.to_owned(), text.to_owned());
    rt.update(move |s, _| s.pages.set_field(&key, &text));
    settle(rt);
}

fn scene(name: &str) -> Runtime<App> {
    scene_in(name, WINDOW)
}

fn scene_in(name: &str, window: (f32, f32)) -> Runtime<App> {
    let mut app = App::headless();
    let (_, f) = scenes::SCENES.iter().find(|(n, _)| *n == name).unwrap_or_else(|| panic!("no scene {name}"));
    f(&mut app);
    app.outbox.clear();
    let mut rt = Runtime::new(app, view, window, Env { system_dark: true, scale: 1.0 });
    rt.frame(0.0);
    settle(&mut rt);
    rt
}

/// Without a daemon nothing answers, so a form stays open: this closes it as the answer would.
fn answered(rt: &mut Runtime<App>) {
    rt.update(|s, _| s.close_dialog());
    settle(rt);
    rt.state.outbox.clear();
}

fn sent(rt: &Runtime<App>) -> Vec<SetupAction> {
    rt.state.outbox.iter().filter_map(|c| if let Call::Setup(s) = c { Some((**s).clone()) } else { None }).collect()
}

// ---- over made-up data

#[test]
fn the_rail_lists_the_seven_sections_in_order_and_each_opens_its_own_page() {
    let mut rt = scene_in("setup-workspaces", (1800.0, 900.0));
    let order: Vec<&str> = SetupSection::ALL.iter().map(|s| s.title()).collect();
    assert_eq!(order, ["Workspaces", "Watching", "Tools", "Skills", "Memory", "Profiles", "Schedules"]);
    let rail = rt.elements().into_iter().find(|e| e.key.as_deref() == Some("setup-rail")).expect("the rail").rect;
    assert_eq!(rail[2], 200.0, "the rail is 200 wide, as Settings' is");
    let mut ys = Vec::new();
    for s in SetupSection::ALL {
        let key = format!("setup-section-{}", s.key());
        ys.push(rt.find(&key).unwrap_or_else(|| panic!("{key}")).1);
        click(&mut rt, &key);
        assert_eq!(rt.state.setup.section, s);
        assert!(rt.find(&format!("full-setup-{}", s.key())).is_some(), "{} has its page", s.title());
        assert!(texts(&rt).iter().any(|t| t == s.title()), "{} is a title", s.title());
    }
    assert!(ys.windows(2).all(|w| w[0] < w[1]), "top to bottom in that order: {ys:?}");
}

#[test]
fn the_sessions_column_counts_what_needs_attention_and_deep_links_choose_the_section() {
    let mut rt = scene("setup-counted");
    let count = rt.elements().into_iter().find(|e| e.key.as_deref() == Some("count-setup")).and_then(|e| e.text);
    // Two connections need you, a suggested watch, two skill proposals, a memory proposal and one piece of guidance.
    assert_eq!(count.as_deref(), Some("7"));
    assert!(rt.find("view-setup").is_some() && !has_text(&rt, "Workspaces 2"));
    // "Open connection" lands in Tools, on that connection's detail with a back arrow.
    rt.update(|s, _| s.open_connection("c2"));
    settle(&mut rt);
    assert_eq!(rt.state.setup.section, SetupSection::Tools);
    assert!(rt.find("tool-back").is_some() && has_text(&rt, "Error monitor"));
    click(&mut rt, "tool-back");
    assert!(rt.find("conn-c2").is_some() && rt.find("tool-back").is_none());
}

#[test]
fn a_status_glyph_says_connected_needs_sign_in_or_failed() {
    let rt = scene("setup-tools");
    let label = |id: &str| rt.elements().into_iter().find(|e| e.key.as_deref() == Some(&format!("conn-{id}"))).and_then(|e| e.label).unwrap();
    assert!(label("c1").contains("Available") && label("c2").contains("Sign in needed") && label("c3").contains("Needs attention") && label("c4").contains("Paused"), "{} | {} | {} | {}", label("c1"), label("c2"), label("c3"), label("c4"));
    assert!(rt.find("conn-reconnect-c2").is_some() && rt.find("conn-reconnect-c3").is_some() && rt.find("conn-reconnect-c1").is_none());
}

#[test]
fn every_destructive_action_asks_first_and_sends_only_when_confirmed() {
    // (scene, the button that starts it, what the confirmation says, the request after "yes")
    #[allow(clippy::type_complexity)]
    let cases: [(&str, &str, &str, fn(&SetupAction) -> bool); 5] = [
        ("setup-watching", "watch-delete-r1", "Delete watch", |c| matches!(c, SetupAction::RemoveWatch { id } if id == "r1")),
        ("setup-schedules", "schedule-delete-s2", "Delete schedule", |c| matches!(c, SetupAction::RemoveSchedule { id } if id == "s2")),
        ("setup-memory", "memory-forget-m1", "Forget memory", |c| matches!(c, SetupAction::ForgetMemory { id } if id == "m1")),
        ("setup-memory", "preference-forget-g1", "Forget guidance", |c| matches!(c, SetupAction::ForgetPreference { id, .. } if id == "g1")),
        ("setup-tools", "conn-switch-c1", "Pause & revoke access", |c| matches!(c, SetupAction::SetEnabled { id, enabled: false } if id == "c1")),
    ];
    for (name, start, button, is_it) in cases {
        let mut rt = scene(name);
        click(&mut rt, start);
        assert!(rt.find("confirm-yes").is_some(), "{name}: {start} asks first");
        assert!(sent(&rt).is_empty(), "{name}: nothing is sent before the person says yes: {:?}", sent(&rt));
        assert!(has_text(&rt, button), "{name}: the button says what it does ({button}): {:?}", texts(&rt));
        // Cancel keeps everything.
        click(&mut rt, "confirm-no");
        assert!(rt.find("confirm-yes").is_none() && sent(&rt).is_empty(), "{name}: Cancel sends nothing");
        click(&mut rt, start);
        click(&mut rt, "confirm-yes");
        assert!(sent(&rt).iter().any(is_it), "{name}: yes sends the request: {:?}", sent(&rt));
    }
}

#[test]
fn a_watch_is_turned_on_only_after_it_says_what_that_allows_and_then_checks_at_once() {
    let mut rt = scene("setup-watching");
    // r2 is paused and its tool is signed out: turning it on goes to Tools instead of asking.
    click(&mut rt, "watch-switch-r2");
    assert_eq!(rt.state.setup.section, SetupSection::Tools, "access first");
    assert!(sent(&rt).is_empty());
    // A usable one (the suggestion) says what it allows; yes saves it enabled, then wakes it.
    let mut rt = scene("setup-watching");
    click(&mut rt, "suggestion-on-r3");
    assert!(has_text(&rt, "Turn on this watch?") && has_text(&rt, "Allows unattended checks every 10 minutes"), "{:?}", texts(&rt));
    click(&mut rt, "confirm-yes");
    assert!(matches!(sent(&rt).as_slice(), [SetupAction::SaveWatch(w)] if w.enabled && w.id == "r3"), "{:?}", sent(&rt));
    // The wake follows the daemon's answer, not the click.
    rt.update(|s, _| s.setup_applied(&SetupAction::Wake { id: "r3".into() }));
    assert!(sent(&rt).iter().any(|c| matches!(c, SetupAction::Wake { id } if id == "r3")), "{:?}", sent(&rt));
}

#[test]
fn a_form_that_is_not_ready_says_what_is_missing_and_sends_nothing() {
    let mut rt = scene("setup-workspaces");
    click(&mut rt, "ws-add");
    assert!(rt.find("dlg-ws-save").is_some());
    // Save is not offered until there is a name and a folder.
    rt.update(|s, _| s.save_workspace());
    settle(&mut rt);
    assert!(sent(&rt).is_empty() && has_text(&rt, "Give the workspace a name."), "{:?}", texts(&rt));
    fill(&mut rt, "dlg:name", "Scratch");
    rt.update(|s, _| s.save_workspace());
    settle(&mut rt);
    assert!(has_text(&rt, "Add at least one folder where this work lives."));
    // A typed folder joins the list and names an unnamed workspace.
    fill(&mut rt, "dlg:name", "");
    fill(&mut rt, "dlg:path", "/work/scratch-notes/");
    click(&mut rt, "dlg-folder-add");
    assert_eq!(rt.state.pages.text("dlg:name"), "scratch-notes");
    assert!(has_text(&rt, "/work/scratch-notes/"));
    click(&mut rt, "dlg-ws-save");
    let ws = sent(&rt);
    assert!(matches!(ws.as_slice(), [SetupAction::SaveWorkspace { workspace, folders }] if workspace.name == "scratch-notes" && workspace.repository == "/work/scratch-notes/" && folders.len() == 1), "{ws:?}");
}

#[test]
fn a_local_connection_needs_its_trust_and_credentials_never_show() {
    let mut rt = scene("setup-tools");
    rt.update(|s, _| s.open_add_connection(AddTab::Manual));
    settle(&mut rt);
    fill(&mut rt, "conn:label", "Files");
    click(&mut rt, "dlg-conn-type");
    rt.update(|s, _| s.connection_form_local(true));
    settle(&mut rt);
    fill(&mut rt, "conn:target", "/usr/local/bin/files-mcp");
    fill(&mut rt, "conn:credentials", "s3cret-token");
    rt.update(|s, _| s.add_connection_manually());
    settle(&mut rt);
    assert!(sent(&rt).is_empty() && has_text(&rt, "Trust the executable"), "trust comes first: {:?}", texts(&rt));
    rt.update(|s, _| s.connection_form_trust());
    settle(&mut rt);
    click(&mut rt, "dlg-conn-save");
    assert!(matches!(sent(&rt).as_slice(), [SetupAction::AddConnection { trust_local_process: true, credentials: Some(_), .. }]));
    assert!(!format!("{:?}", rt.state.outbox).contains("s3cret-token"), "a secret is never printed");
    assert!(!texts(&rt).iter().any(|t| t.contains("s3cret-token")), "and never shown");
}

#[test]
fn the_registry_search_shows_hosted_servers_and_says_why_when_it_cannot() {
    fn offline(_: &str) -> Result<String, String> {
        Err("This app can’t reach the public registry. Add a server URL manually.".to_owned())
    }
    fn found(_: &str) -> Result<String, String> {
        Ok(r#"{"servers":[{"server":{"name":"io.github.acme/acme-mcp","description":"Acme things","remotes":[{"type":"streamable-http","url":"https://mcp.acme.example/mcp"}]}}]}"#.to_owned())
    }
    let mut rt = scene("setup-tools");
    rt.update(|s, _| {
        s.setup.fetch = offline;
        s.open_add_connection(AddTab::Browse);
    });
    settle(&mut rt);
    assert!(has_text(&rt, "sentry-mcp"), "the featured server is always there");
    fill(&mut rt, "conn:search", "acme");
    click(&mut rt, "dlg-registry-search");
    assert!(has_text(&rt, "can’t reach the public registry"), "{:?}", texts(&rt));
    rt.update(|s, _| {
        s.setup.fetch = found;
        s.setup.registry = Default::default();
    });
    click(&mut rt, "dlg-registry-search");
    assert!(has_text(&rt, "acme-mcp") && has_text(&rt, "Community listing"), "{:?}", texts(&rt));
    click(&mut rt, "dlg-registry-acme-mcp");
    assert!(has_text(&rt, "https://mcp.acme.example/mcp"));
    click(&mut rt, "dlg-registry-add");
    assert!(matches!(sent(&rt).as_slice(), [SetupAction::AddConnection { label, config, credentials: None, .. }] if label == "acme-mcp" && matches!(config, neko_protocol::mcp_host::ServerConfig::Http { url } if url == "https://mcp.acme.example/mcp")), "{:?}", sent(&rt));
}

#[test]
fn a_tool_is_allowed_per_workspace_through_the_right_request() {
    let mut rt = scene("setup-tool-detail");
    // The global tracker is granted per workspace (the one shown); a connection that belongs to a workspace uses its own.
    click(&mut rt, "tool-get_issue-grant");
    assert!(matches!(sent(&rt).as_slice(), [SetupAction::Grant { connection_id, workspace_id, tool, allowed: true, .. }] if connection_id == "c1" && workspace_id == "w1" && tool == "get_issue"), "{:?}", sent(&rt));
    rt.update(|s, _| s.open_connection("c3"));
    settle(&mut rt);
    rt.state.outbox.clear();
    // c3 is broken: it cannot be granted until it works.
    rt.update(|s, _| s.set_tool_grant("c3", "x", "h", true));
    assert!(matches!(sent(&rt).as_slice(), [SetupAction::Grant { workspace_id, .. }] if workspace_id.is_empty()), "the connection's own workspace: {:?}", sent(&rt));
}

#[test]
fn skills_enable_lock_and_apply_or_reject_a_proposal() {
    let mut rt = scene_in("setup-skills", (1360.0, 1700.0));
    click(&mut rt, "skill-switch-bug-triage");
    assert!(matches!(sent(&rt).as_slice(), [SetupAction::SkillEnable { enabled: true, path, .. }] if path.ends_with("bug-triage/SKILL.md")), "{:?}", sent(&rt));
    click(&mut rt, "skill-lock-code-review");
    assert!(sent(&rt).iter().any(|c| matches!(c, SetupAction::SkillLock { locked: true, .. })));
    // A proposal that improves a skill shows its diff; Apply and Reject decide it.
    assert!(rt.find("proposal-body-p1").is_some() && has_text(&rt, "+Skip style notes a formatter would fix."));
    click(&mut rt, "proposal-apply-btn-p1");
    assert!(sent(&rt).iter().any(|c| matches!(c, SetupAction::SkillDecide { id, accept: true, .. } if id == "p1")));
    click(&mut rt, "proposal-reject-btn-p1");
    assert!(sent(&rt).iter().any(|c| matches!(c, SetupAction::SkillDecide { id, accept: false, .. } if id == "p1")));
    // A proposal with a published audit: Apply waits for the person to open it and confirm they read it.
    rt.update(|s, _| s.toggle_row("proposal:p2"));
    settle(&mut rt);
    let (_, _) = rt.find("proposal-confirm-p2").expect("the audit confirm");
    rt.state.outbox.clear();
    click(&mut rt, "proposal-apply-btn-p2");
    assert!(sent(&rt).is_empty(), "Apply is held until the audit is confirmed");
    click(&mut rt, "proposal-view-audit-p2");
    click(&mut rt, "proposal-confirm-p2");
    assert!(sent(&rt).iter().any(|c| matches!(c, SetupAction::SkillAudit { id, .. } if id == "p2")), "{:?}", sent(&rt));
}

#[test]
fn memory_adds_edits_and_filters_and_guidance_is_kept_or_dismissed() {
    let mut rt = scene("setup-memory");
    click(&mut rt, "mem-add");
    fill(&mut rt, "dlg:text", "Prefer small pull requests.");
    click(&mut rt, "dlg-mem-save");
    assert!(matches!(sent(&rt).as_slice(), [SetupAction::SaveMemory(m)] if m.id.is_empty() && m.text == "Prefer small pull requests." && m.source == "user" && m.workspace_id.is_none()), "{:?}", sent(&rt));
    answered(&mut rt);
    click(&mut rt, "memory-edit-m3");
    assert_eq!(rt.state.pages.text("dlg:text"), "Run the check script before every push; it takes about a minute.");
    fill(&mut rt, "dlg:text", "Run the check script before every push.");
    click(&mut rt, "dlg-mem-save");
    assert!(matches!(sent(&rt).as_slice(), [SetupAction::SaveMemory(m)] if m.id == "m3" && m.workspace_id.as_deref() == Some("w1") && m.text == "Run the check script before every push."), "{:?}", sent(&rt));
    answered(&mut rt);
    // Search narrows the list.
    fill(&mut rt, "mem:search", "retries");
    assert!(rt.find("memory-m4").is_some() && rt.find("memory-m1").is_none());
    fill(&mut rt, "mem:search", "");
    rt.state.outbox.clear();
    click(&mut rt, "memory-keep-mp1");
    assert!(matches!(sent(&rt).as_slice(), [SetupAction::DecideMemory { id, accept: true }] if id == "mp1"));
    click(&mut rt, "preference-keep-g1");
    assert!(sent(&rt).iter().any(|c| matches!(c, SetupAction::KeepPreference { id, version: 2, .. } if id == "g1")));
    click(&mut rt, "mem-switch-learning");
    assert!(sent(&rt).iter().any(|c| matches!(c, SetupAction::SetMemoryOptions(o) if !o.learning && o.use_memory)));
}

#[test]
fn profiles_save_assign_default_and_share_memories() {
    let mut rt = scene("setup-profiles");
    click(&mut rt, "profile-new");
    fill(&mut rt, "dlg:name", "Writer");
    fill(&mut rt, "dlg:text", "Short sentences.");
    click(&mut rt, "dlg-profile-save");
    assert!(matches!(sent(&rt).as_slice(), [SetupAction::SaveProfile(p)] if p.id.is_empty() && p.name == "Writer" && p.instructions == "Short sentences."), "{:?}", sent(&rt));
    answered(&mut rt);
    click(&mut rt, "profile-default-btn-reviewer");
    assert!(matches!(sent(&rt).as_slice(), [SetupAction::ActivateProfile { id }] if id == "reviewer"));
    // Open, "Use in" a workspace, and the read grant.
    click(&mut rt, "profile-use-reviewer-w1");
    assert!(sent(&rt).iter().any(|c| matches!(c, SetupAction::AssignProfile { workspace_id, profile_id } if workspace_id == "w1" && profile_id == "reviewer")));
    click(&mut rt, "profile-grant-reviewer-default");
    assert!(sent(&rt).iter().any(|c| matches!(c, SetupAction::ReadGrant { reader, source, allowed: false } if reader == "reviewer" && source == "default")), "{:?}", sent(&rt));
}

#[test]
fn schedules_save_paused_enable_and_run_now() {
    let mut rt = scene("setup-schedules");
    click(&mut rt, "sch-new");
    fill(&mut rt, "dlg:name", "Evening wrap-up");
    fill(&mut rt, "dlg:text", "List what finished today.");
    fill(&mut rt, "sch:time", "18:30");
    click(&mut rt, "dlg-sch-save");
    let calls = sent(&rt);
    assert!(matches!(calls.as_slice(), [SetupAction::SaveSchedule(s)] if s.id.is_empty() && !s.enabled && s.rule == "FREQ=DAILY;BYHOUR=18;BYMINUTE=30" && s.workspace_id.as_deref() == Some("w1") && !s.timezone.is_empty()), "saving always pauses: {calls:?}");
    answered(&mut rt);
    click(&mut rt, "schedule-switch-s2");
    assert!(matches!(sent(&rt).as_slice(), [SetupAction::ScheduleEnabled { id, enabled: true }] if id == "s2"));
    click(&mut rt, "schedule-run-s1");
    assert!(sent(&rt).iter().any(|c| matches!(c, SetupAction::RunSchedule { id } if id == "s1")));
    // Editing keeps its anchor and workspace and pauses it again.
    rt.state.outbox.clear();
    click(&mut rt, "schedule-edit-s1");
    assert_eq!(rt.state.pages.text("sch:time"), "08:30");
    click(&mut rt, "dlg-sch-save");
    assert!(matches!(sent(&rt).as_slice(), [SetupAction::SaveSchedule(s)] if s.id == "s1" && !s.enabled && s.rule == "FREQ=DAILY;BYHOUR=8;BYMINUTE=30" && s.anchor_ms != 0), "{:?}", sent(&rt));
}

#[test]
fn an_empty_list_is_one_quiet_line_and_one_action() {
    for (section, key) in [
        (SetupSection::Workspaces, "ws-none"),
        (SetupSection::Watching, "watch-no-workspace"),
        (SetupSection::Skills, "skills-no-workspace"),
        (SetupSection::Schedules, "sch-no-workspace"),
        (SetupSection::Tools, "tools-empty"),
        (SetupSection::Profiles, "profiles-empty"),
    ] {
        let mut app = App::headless();
        app.apply_link(neko_rune::link::LinkEvent::Live(neko_rune::link::Summary { digest: "empty".into(), ..Default::default() }));
        if matches!(section, SetupSection::Profiles) {
            app.summary.as_mut().unwrap().setup.profiles.profiles.clear();
        }
        app.open_setup(section);
        let mut rt = Runtime::new(app, view, WINDOW, Env { system_dark: true, scale: 1.0 });
        rt.frame(0.0);
        settle(&mut rt);
        let line = rt.find(key).unwrap_or_else(|| panic!("{section:?}: no {key}"));
        let _ = line;
        assert!(rt.find(&format!("{key}-action")).is_some(), "{section:?}: one action beside the line");
        let actions = rt.elements().into_iter().filter(|e| e.key.as_deref().is_some_and(|k| k.ends_with("-action"))).count();
        assert_eq!(actions, 1, "{section:?}: exactly one");
    }
}

#[test]
fn dialogs_hold_the_keyboard_and_close_with_escape_or_cancel() {
    let mut rt = scene("setup-schedules");
    click(&mut rt, "sch-new");
    assert!(matches!(rt.state.setup.dialog, Some(Dialog::Schedule { .. })));
    click(&mut rt, "dlg-sch-cancel");
    assert!(rt.state.setup.dialog.is_none() && rt.find("dlg-schedule-dialog").is_none());
    assert!(rt.state.view == Some(FullView::Setup));
}

// ---- against the isolated daemon

struct Harness {
    data: PathBuf,
    child: Child,
}

impl Drop for Harness {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.data);
        let _ = std::fs::remove_dir_all(format!("{}-web", self.data.display()));
        let _ = std::fs::remove_dir_all(format!("{}-files", self.data.display()));
        let _ = std::fs::remove_file(format!("{}-seed.json", self.data.display()));
    }
}

fn harness_binary() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("NEKO_WEB_HARNESS") {
        return Some(PathBuf::from(p)).filter(|p| p.exists());
    }
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let home = std::env::var_os("HOME").map(PathBuf::from)?;
    [home.join("Library/Caches/neko/target/debug/web_harness"), repo.join("target/debug/web_harness")].into_iter().find(|p| p.exists())
}

fn start() -> Option<Harness> {
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let Some(binary) = harness_binary() else {
        assert!(std::env::var_os("NEKO_REQUIRE_HARNESS").is_none(), "NEKO_REQUIRE_HARNESS is set and web_harness is not built");
        eprintln!("skipped: web_harness is not built (cargo build -p neko-daemon --bin web_harness)");
        return None;
    };
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let data = PathBuf::from(format!("/tmp/nrs-{}-{}", std::process::id(), N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)));
    let web = PathBuf::from(format!("{}-web", data.display()));
    let seed = PathBuf::from(format!("{}-seed.json", data.display()));
    std::fs::create_dir_all(&data).unwrap();
    std::fs::create_dir_all(&web).unwrap();
    std::fs::write(web.join("index.html"), "<html></html>").unwrap();
    let wrote = Command::new("node")
        .current_dir(&repo)
        .args(["--input-type=module", "-e", "import { webSeed } from './scripts/fixtures/web-seed.mjs'; import fs from 'node:fs'; fs.writeFileSync(process.argv[1], JSON.stringify(webSeed()));"])
        .arg(&seed)
        .status();
    if !wrote.is_ok_and(|s| s.success()) {
        assert!(std::env::var_os("NEKO_REQUIRE_HARNESS").is_none(), "node could not write the seed");
        eprintln!("skipped: node could not write the harness seed");
        let _ = std::fs::remove_dir_all(&data);
        return None;
    }
    let child = Command::new(binary)
        .current_dir(&data)
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("HOME", &data)
        .env("NEKO_DATA_DIR", &data)
        .env("NEKO_WEB_PORT", "0")
        .env("NEKO_WEB_DIR", &web)
        .env("NEKO_WEB_SEED", &seed)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("the harness starts");
    let harness = Harness { data: data.clone(), child };
    let socket = data.join("neko.sock");
    let deadline = Instant::now() + Duration::from_secs(20);
    while !socket.exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(100));
    }
    assert!(socket.exists(), "the harness did not start");
    Some(harness)
}

/// An app that talks to the harness on the calling thread, open on a Setup section.
fn live(h: &Harness, section: SetupSection) -> Runtime<App> {
    let mut app = App::headless();
    app.clock = neko_rune::app::system_ms;
    app.now_ms = neko_rune::app::system_ms();
    app.dispatch = Dispatch::Blocking { socket: h.data.join("neko.sock") };
    app.call(Call::Refresh);
    app.open_setup(section);
    let mut rt = Runtime::new(app, view, WINDOW, Env { system_dark: true, scale: 1.0 });
    rt.frame(0.0);
    settle(&mut rt);
    rt
}

fn data(rt: &Runtime<App>) -> &neko_rune::link::SetupData {
    &rt.state.summary.as_ref().expect("the daemon answered").setup
}

fn go(rt: &mut Runtime<App>, section: SetupSection) {
    rt.update(move |s, _| s.open_setup(section));
    settle(rt);
}

#[test]
fn daemon_workspaces_are_created_and_edited_through_the_dialog() {
    let Some(h) = start() else { return };
    let folder = format!("{}-files", h.data.display());
    std::fs::create_dir_all(&folder).unwrap();
    let mut rt = live(&h, SetupSection::Workspaces);
    let before = data(&rt).workspaces.len();
    assert!(before >= 2, "the seed's workspaces are listed: {before}");
    click(&mut rt, "ws-add");
    // Typed in the dialog, the way a person does it.
    click(&mut rt, "dlg:name");
    rt.paste("Scratch notes");
    settle(&mut rt);
    fill(&mut rt, "dlg:path", &folder);
    click(&mut rt, "dlg-folder-add");
    fill(&mut rt, "dlg:text", "Keep it small.");
    click(&mut rt, "dlg-ws-save");
    assert!(rt.state.setup.dialog.is_none(), "the daemon took it: {:?}", rt.state.pages.notes);
    let made = data(&rt).workspaces.iter().find(|w| w.name == "Scratch notes").cloned().expect("the new workspace");
    // The daemon keeps the real path (`/tmp` is `/private/tmp` on a Mac).
    let kept = data(&rt).folders_of(&made);
    assert!(made.instructions == "Keep it small." && kept.len() == 1 && std::fs::canonicalize(&kept[0]).ok() == std::fs::canonicalize(&folder).ok(), "{kept:?}");
    // Edit: its details are indented rows under it; its instructions change.
    click(&mut rt, &format!("ws-edit-{}", made.id));
    assert_eq!(rt.state.pages.text("dlg:name"), "Scratch notes");
    fill(&mut rt, "dlg:text", "Keep it small and tested.");
    click(&mut rt, "dlg-ws-save");
    assert_eq!(data(&rt).workspace(&made.id).map(|w| w.instructions.as_str()), Some("Keep it small and tested."));
    click(&mut rt, &format!("ws-{}", made.id));
    assert!(rt.find(&format!("ws-details-{}", made.id)).is_some() && has_text(&rt, &kept[0]));
    // Sync now with nothing to wake says so beside the row.
    click(&mut rt, &format!("ws-sync-{}", made.id));
    assert!(has_text(&rt, "No active watches to sync here."), "{:?}", texts(&rt));
}

#[test]
fn daemon_connections_and_watches_are_added_edited_renamed_and_deleted_with_a_confirm() {
    let Some(h) = start() else { return };
    let mut rt = live(&h, SetupSection::Tools);
    // Tools: add a hosted server by hand, then pause and enable it.
    click(&mut rt, "tools-add");
    click(&mut rt, "dlg-conn-tabs");
    rt.update(|s, _| s.connection_dialog_tab(AddTab::Manual));
    settle(&mut rt);
    fill(&mut rt, "conn:label", "Issue tracker");
    fill(&mut rt, "conn:target", "https://mcp.tracker.example.test/mcp");
    click(&mut rt, "dlg-conn-save");
    assert!(rt.state.setup.dialog.is_none(), "{:?}", rt.state.pages.notes);
    let conn = data(&rt).connections.iter().find(|c| c.label == "Issue tracker").cloned().expect("the new connection");
    assert!(rt.find(&format!("conn-{}", conn.id)).is_some());
    click(&mut rt, &format!("conn-switch-{}", conn.id));
    assert_eq!(data(&rt).connection(&conn.id).map(|c| c.enabled), Some(false), "paused (no grants, so no confirmation was needed)");
    click(&mut rt, &format!("conn-switch-{}", conn.id));
    assert_eq!(data(&rt).connection(&conn.id).map(|c| c.enabled), Some(true));
    // A connection with an error opens to its detail with the way back.
    click(&mut rt, &format!("conn-{}", conn.id));
    assert!(rt.find("tool-back").is_some() && has_text(&rt, "Connection details"));
    click(&mut rt, "tool-back");

    // Watching: a watch over that connection, saved paused.
    go(&mut rt, SetupSection::Watching);
    let ws = data(&rt).workspaces[0].id.clone();
    click(&mut rt, "watch-new");
    fill(&mut rt, "dlg:text", "New issues assigned to me");
    click(&mut rt, &format!("dlg-watch-tool-{}", conn.id));
    click(&mut rt, "dlg-watch-save");
    assert!(rt.state.setup.dialog.is_none(), "{:?}", rt.state.pages.notes);
    let watch = data(&rt).watches.iter().find(|w| w.instruction == "New issues assigned to me").cloned().expect("the new watch");
    assert_eq!((watch.workspace_id.as_str(), watch.enabled, watch.connection_ids.clone()), (ws.as_str(), false, vec![conn.id.clone()]));
    // Edit its instruction; rename it; the title follows.
    click(&mut rt, &format!("watch-edit-{}", watch.id));
    fill(&mut rt, "dlg:text", "New issues assigned to me, and anything blocked");
    click(&mut rt, "dlg-watch-save");
    assert_eq!(data(&rt).watch(&watch.id).map(|w| w.instruction.as_str()), Some("New issues assigned to me, and anything blocked"));
    rt.update({
        let id = watch.id.clone();
        move |s, _| s.open_rename_watch(&id)
    });
    settle(&mut rt);
    fill(&mut rt, "dlg:name", "Assigned issues");
    click(&mut rt, "dlg-rename-save");
    assert_eq!(data(&rt).watch(&watch.id).and_then(|w| w.name.clone()).as_deref(), Some("Assigned issues"));
    assert!(has_text(&rt, "Assigned issues"));
    // Turning it on needs the tools to be usable: this one has no tools yet, so Tools opens instead.
    click(&mut rt, &format!("watch-switch-{}", watch.id));
    assert_eq!(rt.state.setup.section, SetupSection::Tools);
    go(&mut rt, SetupSection::Watching);
    // Delete asks first; yes removes it.
    click(&mut rt, &format!("watch-delete-{}", watch.id));
    assert!(rt.find("confirm-yes").is_some() && data(&rt).watch(&watch.id).is_some(), "nothing is deleted before the yes");
    click(&mut rt, "confirm-yes");
    assert!(rt.state.setup.dialog.is_none() && data(&rt).watch(&watch.id).is_none(), "{:?}", rt.state.pages.notes);
}

#[test]
fn daemon_skills_are_found_in_a_workspace_folder_and_enabled() {
    let Some(h) = start() else { return };
    let folder = format!("{}-files", h.data.display());
    std::fs::create_dir_all(format!("{folder}/.claude/skills/tidy")).unwrap();
    std::fs::write(format!("{folder}/.claude/skills/tidy/SKILL.md"), "---\nname: tidy\ndescription: Tidy a folder\n---\nKeep it tidy.\n").unwrap();
    let mut rt = live(&h, SetupSection::Workspaces);
    click(&mut rt, "ws-add");
    fill(&mut rt, "dlg:name", "Tidy place");
    fill(&mut rt, "dlg:path", &folder);
    click(&mut rt, "dlg-ws-save");
    let ws = data(&rt).workspaces.iter().find(|w| w.name == "Tidy place").map(|w| w.id.clone()).expect("the workspace");
    go(&mut rt, SetupSection::Skills);
    rt.update(move |s, _| s.setup.skills_workspace = Some(ws));
    settle(&mut rt);
    click(&mut rt, "skills-refresh");
    assert!(rt.find("skill-tidy").is_some(), "found in the folder: {:?} {:?}", data(&rt).skills.available, rt.state.pages.notes);
    click(&mut rt, "skill-switch-tidy");
    assert_eq!(data(&rt).skills.enabled.len(), 1, "enabled for the workspace");
    assert!(has_text(&rt, "Enabled"));
    // Turning the curator off and on again.
    click(&mut rt, "skill-curator-switch");
    assert!(data(&rt).skills.curator.off);
    click(&mut rt, "skill-curator-switch");
    assert!(!data(&rt).skills.curator.off);
    // Disable again, from the row's switch.
    click(&mut rt, "skill-switch-tidy");
    assert!(data(&rt).skills.enabled.is_empty());
}

#[test]
fn daemon_memories_are_added_edited_and_forgotten_with_a_confirm() {
    let Some(h) = start() else { return };
    let mut rt = live(&h, SetupSection::Memory);
    click(&mut rt, "mem-add");
    fill(&mut rt, "dlg:text", "I prefer small pull requests, with tests first.");
    click(&mut rt, "dlg-mem-save");
    assert!(rt.state.setup.dialog.is_none(), "{:?}", rt.state.pages.notes);
    let m = data(&rt).memory.iter().find(|m| m.text.starts_with("I prefer small")).cloned().expect("the new memory");
    assert_eq!((m.source.as_str(), m.workspace_id.clone()), ("user", None));
    assert!(has_text(&rt, "I prefer small pull requests"));
    click(&mut rt, &format!("memory-edit-{}", m.id));
    fill(&mut rt, "dlg:text", "I prefer small pull requests.");
    click(&mut rt, "dlg-mem-save");
    assert_eq!(data(&rt).memory.iter().find(|x| x.id == m.id).map(|x| x.text.as_str()), Some("I prefer small pull requests."));
    // The options switch.
    click(&mut rt, "mem-switch-learning");
    assert_eq!(data(&rt).memory_options.map(|o| o.learning), Some(false));
    click(&mut rt, "mem-switch-learning");
    // Guidance is saved as a suggestion until kept.
    click(&mut rt, "mem-add-guidance");
    fill(&mut rt, "dlg:text", "Prefer the smallest fix that passes.");
    fill(&mut rt, "dlg:name", "failing test, flaky test");
    click(&mut rt, "dlg-pref-save");
    assert!(data(&rt).preferences.iter().any(|p| p.instruction.starts_with("Prefer the smallest") && p.state == neko_protocol::decision_context::PreferenceState::Proposed), "{:?} {:?}", data(&rt).preferences, rt.state.pages.notes);
    // Forget asks first.
    click(&mut rt, &format!("memory-forget-{}", m.id));
    assert!(rt.find("confirm-yes").is_some() && data(&rt).memory.iter().any(|x| x.id == m.id));
    click(&mut rt, "confirm-yes");
    assert!(data(&rt).memory.iter().all(|x| x.id != m.id), "{:?}", rt.state.pages.notes);
}

#[test]
fn daemon_profiles_are_saved_assigned_and_made_the_default() {
    let Some(h) = start() else { return };
    let mut rt = live(&h, SetupSection::Profiles);
    click(&mut rt, "profile-new");
    fill(&mut rt, "dlg:name", "Reviewer");
    fill(&mut rt, "dlg:text", "Be terse. Review like a staff engineer.");
    click(&mut rt, "dlg-profile-save");
    let p = data(&rt).profiles.profiles.iter().find(|p| p.name == "Reviewer").cloned().expect("the new profile");
    click(&mut rt, &format!("profile-edit-{}", p.id));
    fill(&mut rt, "dlg:text", "Be terse.");
    click(&mut rt, "dlg-profile-save");
    assert_eq!(data(&rt).profiles.profiles.iter().find(|x| x.id == p.id).map(|x| x.instructions.as_str()), Some("Be terse."));
    click(&mut rt, &format!("profile-default-btn-{}", p.id));
    assert_eq!(data(&rt).profiles.active_profile_id, p.id);
    // Open it and use it in a workspace.
    // A workspace with tickets cannot change its agent (the daemon says so beside the profile); an empty one can.
    let folder = format!("{}-files", h.data.display());
    std::fs::create_dir_all(&folder).unwrap();
    rt.update(move |s, _| s.setup_call(SetupAction::SaveWorkspace { workspace: neko_protocol::workbench::Workspace { id: String::new(), name: "Quiet".into(), repository: folder.clone(), instructions: String::new(), away_enabled: false }, folders: vec![folder] }));
    settle(&mut rt);
    let ws = data(&rt).workspaces.iter().find(|w| w.name == "Quiet").expect("the quiet workspace").id.clone();
    click(&mut rt, &format!("profile-{}", p.id));
    click(&mut rt, &format!("profile-use-{}-{ws}", p.id));
    assert_eq!(data(&rt).profiles.owner(&ws), p.id, "{:?}", rt.state.pages.notes);
    // And let it read the original's memories.
    click(&mut rt, &format!("profile-grant-{}-default", p.id));
    assert!(data(&rt).profiles.read_grants.iter().any(|g| g.reader_id == p.id && g.source_id == "default"), "{:?}", rt.state.pages.notes);
}

#[test]
fn daemon_schedules_are_saved_paused_enabled_run_and_deleted_with_a_confirm() {
    let Some(h) = start() else { return };
    let mut rt = live(&h, SetupSection::Schedules);
    click(&mut rt, "sch-new");
    fill(&mut rt, "dlg:name", "Morning brief");
    fill(&mut rt, "dlg:text", "Summarise what changed overnight.");
    fill(&mut rt, "sch:time", "08:30");
    click(&mut rt, "dlg-sch-save");
    assert!(rt.state.setup.dialog.is_none(), "{:?}", rt.state.pages.notes);
    let s = data(&rt).schedules.iter().find(|s| s.name == "Morning brief").cloned().expect("the new schedule");
    assert!(!s.enabled && s.rule == "FREQ=DAILY;BYHOUR=8;BYMINUTE=30", "saving pauses it: {s:?}");
    assert!(has_text(&rt, "Daily at 08:30"));
    click(&mut rt, &format!("schedule-edit-{}", s.id));
    fill(&mut rt, "dlg:text", "Summarise what changed overnight and what needs me.");
    click(&mut rt, "dlg-sch-save");
    assert_eq!(data(&rt).schedules.iter().find(|x| x.id == s.id).map(|x| x.prompt.as_str()), Some("Summarise what changed overnight and what needs me."));
    click(&mut rt, &format!("schedule-switch-{}", s.id));
    assert_eq!(data(&rt).schedules.iter().find(|x| x.id == s.id).map(|x| x.enabled), Some(true));
    let tasks = rt.state.summary.as_ref().unwrap().tasks.len();
    click(&mut rt, &format!("schedule-run-{}", s.id));
    assert!(rt.state.summary.as_ref().unwrap().tasks.len() >= tasks, "a read-only plan was queued");
    click(&mut rt, &format!("schedule-delete-{}", s.id));
    assert!(rt.find("confirm-yes").is_some() && data(&rt).schedules.iter().any(|x| x.id == s.id));
    click(&mut rt, "confirm-yes");
    assert!(data(&rt).schedules.iter().all(|x| x.id != s.id), "{:?}", rt.state.pages.notes);
}
