//! Plugins in the real daemon (docs/knowledge/plugins.md), with the fake CLI as `claude` and the fake MCP server as every plugin's
//! server. Secrets go to a file store that only a debug daemon started with `NEKO_SECRETS_FILE` has: no test touches the login
//! keychain. Every wait has a deadline.

mod support;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use neko_protocol::client::Client;
use neko_protocol::plugins::*;
use neko_protocol::*;
use serde_json::{Value, json};
use support::*;

const T: Duration = Duration::from_secs(30);
const SECRET: &str = "s3cret-value-xyz";

fn alive(pid: i32) -> bool {
    // SAFETY: signal 0 only asks whether the process exists.
    unsafe { libc::kill(pid, 0) == 0 }
}

fn until(what: &str, limit: Duration, mut ok: impl FnMut() -> bool) {
    let end = Instant::now() + limit;
    while !ok() {
        assert!(Instant::now() < end, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Kills a process a test's script started (by the pid it wrote) if the test failed before the daemon's guard did.
struct Reap(i32);

impl Drop for Reap {
    fn drop(&mut self) {
        if self.0 > 0 && alive(self.0) {
            // SAFETY: a process this test's own fake server started, by the pid it wrote.
            unsafe { libc::kill(self.0, libc::SIGKILL) };
        }
    }
}

fn fake_mcp() -> PathBuf {
    Path::new(env!("CARGO_BIN_EXE_neko-daemon")).with_file_name("neko-fake-mcp")
}

struct Rig {
    d: Daemon,
    record: PathBuf,
    secrets: PathBuf,
}

fn rig(name: &str, extra: &[(&str, &str)]) -> Rig {
    let dir = temp_dir(name);
    let record = dir.join("record.jsonl");
    let secrets = dir.join("secrets-test.json");
    let (r, s) = (record.to_string_lossy().into_owned(), secrets.to_string_lossy().into_owned());
    let mut env: Vec<(&str, &str)> = vec![("NEKO_FAKE_RECORD", &r), ("NEKO_SECRETS_FILE", &s)];
    env.extend_from_slice(extra);
    Rig { d: Daemon::start_with(dir, &env), record, secrets }
}

impl Rig {
    fn client(&self) -> Client {
        let mut c = self.d.client();
        assert!(matches!(c.request(Request::Subscribe).unwrap(), Reply::Subscribed { .. }));
        c
    }

    /// What the fake CLI was given, one JSON value per line.
    fn recorded(&self) -> Vec<Value> {
        std::fs::read_to_string(&self.record).unwrap_or_default().lines().filter_map(|l| serde_json::from_str(l).ok()).collect()
    }

    /// The runs the fake CLI was started for: its arguments, and the MCP config it was handed (None when it got none).
    fn runs(&self) -> Vec<(Vec<String>, Option<Value>)> {
        let mut out: Vec<(Vec<String>, Option<Value>)> = Vec::new();
        for v in self.recorded() {
            if let Some(a) = v["argv"].as_array() {
                out.push((a.iter().filter_map(|x| x.as_str().map(str::to_owned)).collect(), None));
            } else if let Some(c) = v.get("mcp_config")
                && let Some(last) = out.last_mut()
            {
                last.1 = Some(c.clone());
            }
        }
        out
    }
}

/// The plugins a run was given: the servers in its config other than Neko's own.
fn served(run: &(Vec<String>, Option<Value>)) -> Vec<String> {
    run.1.as_ref().and_then(|c| c["content"]["mcpServers"].as_object()).map(|m| m.keys().filter(|k| *k != "neko").cloned().collect()).unwrap_or_default()
}

/// An `mcpServers` paste for the fake server, called `name`, with `ACME_TOKEN` set to a secret.
fn paste(name: &str, extra_args: &[&str]) -> String {
    let mut args: Vec<String> = extra_args.iter().map(|a| (*a).to_owned()).collect();
    args.extend(["--name".into(), format!("fake-{name}")]);
    json!({"mcpServers": {name: {"command": fake_mcp().to_string_lossy(), "args": args, "env": {"ACME_TOKEN": SECRET}}}}).to_string()
}

fn add(c: &mut Client, paste: &str) -> Plugin {
    match c.request(Request::AddMcpJson { json: paste.into(), name: None }).unwrap() {
        Reply::Plugins { mut plugins } => plugins.remove(0),
        r => panic!("add: {r:?}"),
    }
}

fn approve(c: &mut Client, p: &Plugin) -> Plugin {
    match c.request(Request::ApprovePlugin { plugin: p.id.clone(), digest: p.digest.clone() }).unwrap() {
        Reply::Plugin { plugin } => plugin,
        r => panic!("approve: {r:?}"),
    }
}

fn list(c: &mut Client) -> Vec<Plugin> {
    match c.request(Request::ListPlugins).unwrap() {
        Reply::Plugins { plugins } => plugins,
        r => panic!("list: {r:?}"),
    }
}

fn test_plugin(c: &mut Client, p: &Plugin) -> Result<Plugin, (ErrorCode, String)> {
    match c.request(Request::TestPlugin { plugin: p.id.clone() }).unwrap() {
        Reply::Plugin { plugin } => Ok(plugin),
        Reply::Error { code, message } => Err((code, message)),
        r => panic!("test: {r:?}"),
    }
}

fn events_until(c: &mut Client, what: &str, mut done: impl FnMut(&Change) -> bool) -> Vec<Change> {
    let end = Instant::now() + T;
    let mut got = Vec::new();
    loop {
        assert!(Instant::now() < end, "timed out waiting for {what}; got {got:#?}");
        if let Some(ServerMessage::Event { change, .. }) = c.next_event(Duration::from_millis(200)).unwrap() {
            let stop = done(&change);
            got.push(change);
            if stop {
                return got;
            }
        }
    }
}

fn task_is(id: &TaskId, s: TaskStatus) -> impl FnMut(&Change) -> bool {
    let id = id.clone();
    move |c| matches!(c, Change::TaskUpdated { task } if task.id == id && task.status == s)
}

fn create(c: &mut Client, words: &str, cat: Option<&str>) -> Task {
    if let Some(cat) = cat {
        return support::task_of_cat(c, words, cat);
    }
    match c.request(Request::CreateTask { words: words.into(), workspace: None, in_folder: false, from_branch: None }).unwrap() {
        Reply::Task { task } => task,
        r => panic!("create: {r:?}"),
    }
}

fn transcript(c: &mut Client, task: &TaskId) -> Transcript {
    match c.request(Request::GetTranscript { task: task.clone() }).unwrap() {
        Reply::Transcript { transcript } => transcript,
        r => panic!("transcript: {r:?}"),
    }
}

fn pending(changes: &[Change]) -> Permission {
    changes.iter().find_map(|c| if let Change::PermissionChanged { permission } = c { (permission.state == PermissionState::Pending).then(|| permission.clone()) } else { None }).expect("a pending request")
}

fn set_mode(c: &mut Client, m: PermissionMode) {
    let s = Settings { runtime: "claude".into(), permission_mode: m, own_mcp: false };
    assert_eq!(c.request(Request::SetSettings { settings: s.clone() }).unwrap(), Reply::Settings { settings: s });
}

fn needs(c: &mut Client) -> Vec<NeedsItem> {
    match c.request(Request::ListNeeds).unwrap() {
        Reply::Needs { items } => items,
        r => panic!("needs: {r:?}"),
    }
}

/// Every file under `dir` with its bytes.
fn snapshot(dir: &Path) -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else {
                out.push((p.to_string_lossy().into_owned(), std::fs::read(&p).unwrap_or_default()));
            }
        }
    }
    out.sort();
    out
}

fn contains(hay: &[u8], needle: &str) -> bool {
    hay.windows(needle.len()).any(|w| w == needle.as_bytes())
}

/// Runs a task with the plugin-tool script and waits for it to finish, answering the plugin tool's request with `decision` when it asks.
fn run_plugin_task(c: &mut Client, server: &str, cat: Option<&str>, decision: Decision) -> (Task, Option<Permission>) {
    let task = create(c, &format!("[fake:plugin-tool:{server}:search]"), cat);
    let mut asked = None;
    let end = Instant::now() + T;
    loop {
        assert!(Instant::now() < end, "the task never finished");
        match c.next_event(Duration::from_millis(200)).unwrap() {
            Some(ServerMessage::Event { change: Change::PermissionChanged { permission }, .. }) if permission.task == task.id && permission.state == PermissionState::Pending => {
                assert_eq!(c.request(Request::AnswerPermission { permission: permission.id.clone(), decision }).unwrap(), Reply::Ok);
                asked = Some(permission);
            }
            Some(ServerMessage::Event { change: Change::TaskUpdated { task: t }, .. }) if t.id == task.id && t.status == TaskStatus::Done => return (task, asked),
            _ => {}
        }
    }
}

#[test]
fn adding_runs_nothing_and_the_plugin_waits_in_needs_you_for_its_approval() {
    let r = rig("plugins-add", &[]);
    let pidfile = r.d.dir.join("never-started");
    let mut c = r.client();
    let p = add(&mut c, &paste("acme-docs", &["--pidfile", &pidfile.to_string_lossy()]));
    assert_eq!((p.name.as_str(), p.approval, p.state()), ("acme-docs", Approval::Waiting, PluginState::Waiting));
    assert_eq!(p.manifest.mcp.env["ACME_TOKEN"], "${settings.acme_token}", "the value is not in the manifest");
    assert_eq!(p.value("acme_token").map(|v| v.set), Some(true));
    // It never ran, and it shows in Needs you as a plugin, with exactly what it would run.
    std::thread::sleep(Duration::from_millis(300));
    assert!(!pidfile.exists(), "adding started nothing");
    let items = needs(&mut c);
    assert_eq!(items.len(), 1);
    let Need::Plugin { ask } = &items[0].need else { panic!("{:?}", items[0]) };
    assert_eq!((ask.name.as_str(), ask.passes.clone(), ask.changed), ("acme-docs", vec!["ACME_TOKEN".to_owned()], false));
    assert!(ask.runs.contains("neko-fake-mcp") && ask.runs.contains("--pidfile"), "{}", ask.runs);
    // The wrong digest is refused; the right one approves, and the plugin leaves Needs you.
    assert!(matches!(c.request(Request::ApprovePlugin { plugin: p.id.clone(), digest: "sha256:nope".into() }).unwrap(), Reply::Error { code: ErrorCode::Refused, .. }));
    let a = approve(&mut c, &p);
    assert_eq!((a.approval, a.approved_digest.as_deref()), (Approval::Approved, Some(p.digest.as_str())));
    assert!(needs(&mut c).is_empty());
    // Bad input is refused with words.
    for (bad, why) in [("{ nope", "not valid JSON"), ("{\"hello\": 1}", "did not find an MCP server"), ("", "Paste a server")] {
        match c.request(Request::AddMcpJson { json: bad.into(), name: None }).unwrap() {
            Reply::Error { code: ErrorCode::InvalidPlugin, message } => assert!(message.contains(why), "{message}"),
            r => panic!("{r:?}"),
        }
    }
    assert!(matches!(c.request(Request::AddMcpJson { json: paste("acme-docs", &[]), name: None }).unwrap(), Reply::Error { code: ErrorCode::Refused, .. }), "a name is taken once");
    assert_eq!(list(&mut c).len(), 1);
}

#[test]
fn a_folder_plugin_is_added_from_its_manifest_file() {
    let r = rig("plugins-folder-add", &[]);
    let folder = r.d.dir.join("acme-plugin");
    std::fs::create_dir_all(&folder).unwrap();
    let manifest = json!({"neko_plugin": 1, "name": "folder-docs", "version": "0.3", "kind": "mcp", "mcp": {"command": fake_mcp().to_string_lossy(), "args": ["--name", "from-folder"]}});
    std::fs::write(folder.join(MANIFEST_FILE), manifest.to_string()).unwrap();
    let mut c = r.client();
    assert!(matches!(c.request(Request::AddPluginFolder { folder: r.d.dir.join("nowhere").to_string_lossy().into_owned() }).unwrap(), Reply::Error { code: ErrorCode::InvalidPlugin, .. }));
    let Reply::Plugin { plugin } = c.request(Request::AddPluginFolder { folder: folder.to_string_lossy().into_owned() }).unwrap() else { panic!("not a plugin") };
    assert_eq!((plugin.name.as_str(), plugin.version.as_str(), plugin.approval), ("folder-docs", "0.3", Approval::Waiting));
    assert_eq!(plugin.folder.as_deref().map(|f| f.ends_with("acme-plugin")), Some(true));
    let a = approve(&mut c, &plugin);
    let tested = test_plugin(&mut c, &a).unwrap();
    assert_eq!(tested.last_check.as_ref().map(|k| (k.ok, k.server.clone())), Some((true, Some("from-folder".into()))));
}

#[test]
fn a_secret_is_in_the_keychain_store_and_never_in_the_data_folder() {
    let mut r = rig("plugins-secret", &[]);
    let mut c = r.client();
    let p = add(&mut c, &paste("acme-docs", &[]));
    // A secret set later, through the settings request, goes the same way.
    let with_field = json!({"neko_plugin": 1, "name": "keyed", "kind": "mcp", "mcp": {"command": fake_mcp().to_string_lossy(), "env": {"K": "${settings.key}"}}, "settings": [{"key": "key", "kind": "secret", "label": "API key"}]});
    let dir = r.d.dir.join("keyed");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(MANIFEST_FILE), with_field.to_string()).unwrap();
    let Reply::Plugin { plugin: keyed } = c.request(Request::AddPluginFolder { folder: dir.to_string_lossy().into_owned() }).unwrap() else { panic!() };
    let Reply::Plugin { plugin: keyed } = c.request(Request::SetPluginSetting { plugin: keyed.id.clone(), key: "key".into(), value: Some("another-secret-123".into()) }).unwrap() else { panic!() };
    assert_eq!(keyed.value("key").map(|v| (v.set, v.value.clone())), Some((true, None)), "only whether it is set comes back");
    assert!(!serde_json::to_string(&list(&mut c)).unwrap().contains("another-secret-123"));
    approve(&mut c, &p);
    run_plugin_task(&mut c, "acme-docs", None, Decision::AllowOnce);
    drop(c);
    assert!(r.d.stop().success());
    // The test store has them (that is where a debug daemon was told to keep them) …
    let kept = std::fs::read_to_string(&r.secrets).unwrap();
    assert!(kept.contains(SECRET) && kept.contains("another-secret-123") && kept.contains(&format!("{}/acme_token", p.id)), "{kept}");
    // … and nothing else in the data folder has either: not the store, its log, the events, the transcript, the run's files.
    for (file, bytes) in snapshot(&r.d.dir) {
        if file == r.secrets.to_string_lossy() || file == r.record.to_string_lossy() {
            continue;
        }
        assert!(!contains(&bytes, SECRET) && !contains(&bytes, "another-secret-123"), "a secret is in {file}");
    }
    let mut stderr = String::new();
    std::io::Read::read_to_string(r.d.child.stderr.as_mut().unwrap(), &mut stderr).unwrap();
    assert!(!stderr.contains(SECRET) && !stderr.contains("another-secret-123"), "a secret is in the daemon's log");
}

#[test]
fn a_check_lists_the_tools_and_only_an_approved_plugin_is_run() {
    let r = rig("plugins-test", &[]);
    let pidfile = r.d.dir.join("pid");
    let mut c = r.client();
    let p = add(&mut c, &paste("acme-docs", &["--pidfile", &pidfile.to_string_lossy(), "--echo-env", "ACME_TOKEN"]));
    let (code, why) = test_plugin(&mut c, &p).unwrap_err();
    assert_eq!(code, ErrorCode::Refused);
    assert!(why.contains("Approve this plugin first"), "{why}");
    std::thread::sleep(Duration::from_millis(200));
    assert!(!pidfile.exists(), "a plugin waiting for approval is not run, not even to test it");
    let a = approve(&mut c, &p);
    let t = test_plugin(&mut c, &a).unwrap();
    let k = t.last_check.clone().unwrap();
    assert!(k.ok && k.error.is_none());
    assert_eq!((k.server.as_deref(), k.version.as_deref()), (Some("fake-acme-docs"), Some("1.2.3")));
    assert_eq!(k.tools, ["search", "fetch_page", "list_spaces", "env_ACME_TOKEN_set"], "the secret reached the server's environment");
    assert!(k.at_ms > 0);
    assert_eq!(list(&mut c)[0].last_check, Some(k), "the last check is kept");
    let server = std::fs::read_to_string(&pidfile).unwrap().trim().parse::<i32>().unwrap();
    until("the checked server to be gone", Duration::from_secs(10), || !alive(server));
}

#[test]
fn a_check_that_fails_says_why() {
    let r = rig("plugins-test-fail", &[("NEKO_PLUGIN_CHECK_MS", "1500")]);
    let mut c = r.client();
    for (mode, expect) in [("crash", "stopped before it answered (exit code 3)"), ("garbage", "not MCP"), ("refuse", "refused to start MCP: Bad token"), ("hang", "did not answer within")] {
        let p = add(&mut c, &paste(&format!("fake-{mode}"), &["--mode", mode]));
        let a = approve(&mut c, &p);
        let began = Instant::now();
        let t = test_plugin(&mut c, &a).unwrap();
        let k = t.last_check.unwrap();
        assert!(!k.ok && k.tools.is_empty(), "{mode}");
        assert!(k.error.as_deref().is_some_and(|e| e.contains(expect)), "{mode}: {:?}", k.error);
        assert!(began.elapsed() < Duration::from_secs(5), "{mode} took {:?}", began.elapsed());
    }
    let missing = add(&mut c, &json!({"mcpServers": {"missing": {"command": "/nonexistent/server-xyz"}}}).to_string());
    let a = approve(&mut c, &missing);
    let k = test_plugin(&mut c, &a).unwrap().last_check.unwrap();
    assert!(!k.ok && k.error.as_deref().is_some_and(|e| e.contains("could not start")), "{:?}", k.error);
    let remote = add(&mut c, r#"{"mcpServers": {"web": {"url": "https://example.com/mcp"}}}"#);
    let a = approve(&mut c, &remote);
    assert!(matches!(test_plugin(&mut c, &a), Err((ErrorCode::Refused, w)) if w.contains("web address")), "web servers are not tested yet");
}

#[test]
fn a_stop_during_a_hanging_check_stops_within_a_second_and_leaves_nothing_running() {
    let mut r = rig("plugins-stop", &[]);
    let (pid, grand) = (r.d.dir.join("pid"), r.d.dir.join("grand"));
    let mut c = r.client();
    let p = add(&mut c, &paste("hanger", &["--mode", "hang", "--pidfile", &pid.to_string_lossy(), "--grandchild", &grand.to_string_lossy()]));
    let a = approve(&mut c, &p);
    // The check hangs, on its own connection. Meanwhile another connection is answered at once: nothing waits on the plugin.
    let (port, token) = (r.d.port(), r.d.token());
    let id = a.id.clone();
    let asker = std::thread::spawn(move || {
        let mut c2 = Client::connect(port, &token, "test").unwrap();
        c2.request(Request::TestPlugin { plugin: id })
    });
    until("the server to start", Duration::from_secs(10), || pid.exists() && grand.exists() && std::fs::read_to_string(&pid).is_ok_and(|s| !s.is_empty()));
    // The guardian looks at the tree every 100 ms; what is born between two looks is the run guard's known gap (run-guard.md), so the
    // grandchild is given a few looks before the stop.
    std::thread::sleep(Duration::from_millis(500));
    let began = Instant::now();
    assert_eq!(list(&mut c).len(), 1);
    assert!(began.elapsed() < Duration::from_millis(500), "a request waited on the hanging check: {:?}", began.elapsed());
    let (server, child) = (std::fs::read_to_string(&pid).unwrap().trim().parse::<i32>().unwrap(), std::fs::read_to_string(&grand).unwrap().trim().parse::<i32>().unwrap());
    let _reap = (Reap(server), Reap(child));
    let stopped = Instant::now();
    r.d.signal(libc::SIGTERM);
    let end = Instant::now() + Duration::from_secs(5);
    while r.d.child.try_wait().unwrap().is_none() {
        assert!(Instant::now() < end, "the daemon did not stop");
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(stopped.elapsed() < Duration::from_millis(1200), "a stop took {:?} with a check hanging", stopped.elapsed());
    until("the hung server and what it started to be gone", Duration::from_secs(10), || !alive(server) && !alive(child));
    let _ = asker.join();
}

#[test]
fn a_run_gets_its_approved_plugins_in_a_config_of_its_own_and_the_file_goes_with_the_run() {
    let r = rig("plugins-run", &[]);
    let mut c = r.client();
    let p = add(&mut c, &paste("acme-docs", &[]));
    // Waiting: not handed over, and the tool is not there.
    let (before, _) = run_plugin_task(&mut c, "acme-docs", None, Decision::AllowOnce);
    let runs = r.runs();
    assert_eq!(runs.len(), 1);
    assert!(served(&runs[0]).is_empty(), "an unapproved plugin never reaches a run: {:?}", runs[0].1);
    assert!(transcript(&mut c, &before.id).parts.iter().any(|p| matches!(&p.body, PartBody::Agent { text, .. } if text.contains("was not available"))));
    approve(&mut c, &p);
    let (task, asked) = run_plugin_task(&mut c, "acme-docs", None, Decision::AllowOnce);
    // Following Claude Code's own rules: the plugin's tool asks in Needs you, by the plugin's name.
    let ask = asked.expect("the plugin's tool is asked about");
    assert_eq!((ask.title.as_str(), ask.tool.as_str(), ask.destructive), ("Use search from acme-docs", "mcp__acme-docs__search", false));
    assert!(ask.why.contains("a plugin you approved in Neko"), "{}", ask.why);
    assert_eq!(ask.detail, r#"{"query":"retry policy"}"#);
    let runs = r.runs();
    assert_eq!(runs.len(), 2);
    let (argv, config) = &runs[1];
    let at = argv.iter().position(|a| a == "--mcp-config").expect("the run's config is passed");
    assert!(argv.iter().any(|a| a == "--strict-mcp-config"), "a run has only the servers Neko gave it, not the person's own");
    let file = Path::new(&argv[at + 1]);
    assert!(file.starts_with(r.d.dir.join("plugins-run")), "{file:?}");
    let config = config.as_ref().expect("the config was read");
    assert_eq!(config["path"], argv[at + 1]);
    let server = &config["content"]["mcpServers"]["acme-docs"];
    assert_eq!(server["type"], "stdio");
    assert_eq!(server["command"], fake_mcp().to_string_lossy().as_ref());
    assert_eq!(server["env"]["ACME_TOKEN"], SECRET, "the secret is in the server's env, for this run");
    // Neko's own server is there too, before the plugins (docs/knowledge/subagents.md).
    assert_eq!(config["content"]["mcpServers"].as_object().unwrap().len(), 2);
    assert_eq!(config["content"]["mcpServers"]["neko"]["args"], json!(["--mcp"]));
    // The step shows the tool's own name, and it ran.
    let t = transcript(&mut c, &task.id);
    assert!(t.parts.iter().any(|p| matches!(&p.body, PartBody::Tool { name, state: ToolState::Done, .. } if name == "mcp__acme-docs__search")), "{:#?}", t.parts);
    assert_eq!(t.permissions[0].state, PermissionState::Allowed);
    // When the run ended its file went, and its folder; nothing of it is left.
    until("the run's config to be removed", Duration::from_secs(5), || !file.exists());
    assert_eq!(std::fs::read_dir(r.d.dir.join("plugins-run")).map(|d| d.count()).unwrap_or(0), 0, "nothing is left of a run's plugins");
}

#[test]
fn in_ask_before_risky_steps_the_plugins_tool_asks_the_same_way() {
    let r = rig("plugins-ask-mode", &[]);
    let mut c = r.client();
    let p = add(&mut c, &paste("acme-docs", &[]));
    approve(&mut c, &p);
    set_mode(&mut c, PermissionMode::Ask);
    let task = create(&mut c, "[fake:plugin-tool:acme-docs:search]", None);
    let ask = pending(&events_until(&mut c, "the plugin's tool to be asked", task_is(&task.id, TaskStatus::NeedsYou)));
    assert_eq!((ask.title.as_str(), ask.tool.as_str()), ("Use search from acme-docs", "mcp__acme-docs__search"));
    let item = needs(&mut c).into_iter().find(|i| i.task == task.id).expect("in Needs you");
    let Need::Permission { permission } = item.need else { panic!() };
    assert_eq!(permission.id, ask.id);
    // Deny: the step is denied, not run.
    assert_eq!(c.request(Request::AnswerPermission { permission: ask.id.clone(), decision: Decision::Deny }).unwrap(), Reply::Ok);
    events_until(&mut c, "done", task_is(&task.id, TaskStatus::Done));
    let t = transcript(&mut c, &task.id);
    assert!(t.parts.iter().any(|p| matches!(&p.body, PartBody::Tool { state: ToolState::Denied, .. })), "{:#?}", t.parts);
    // Always allow: scoped to that tool of that plugin, for the rest of the task. A second call in the same task is not asked again
    // (the follow-up resumes the same task).
    let (second, asked) = run_plugin_task(&mut c, "acme-docs", None, Decision::AlwaysAllow);
    assert!(asked.is_some(), "a new task asks again: always is for one task");
    let _ = second;
    // Full access: nothing asks.
    set_mode(&mut c, PermissionMode::Full);
    let (_, asked) = run_plugin_task(&mut c, "acme-docs", None, Decision::AllowOnce);
    assert!(asked.is_none());
}

#[test]
fn a_manifest_that_changed_is_left_out_of_runs_and_asks_again() {
    let r = rig("plugins-changed", &[]);
    let mut c = r.client();
    let p = add(&mut c, &paste("acme-docs", &[]));
    let a = approve(&mut c, &p);
    run_plugin_task(&mut c, "acme-docs", None, Decision::AllowOnce);
    assert_eq!(served(&r.runs()[0]), ["acme-docs"], "approved: handed over");
    let mut m = a.manifest.clone();
    m.mcp.args.push("--changed".into());
    let Reply::Plugin { plugin: changed } = c.request(Request::UpdatePluginManifest { plugin: a.id.clone(), manifest: m }).unwrap() else { panic!() };
    assert_eq!(changed.approval, Approval::Changed);
    let items = needs(&mut c);
    let Need::Plugin { ask } = &items[0].need else { panic!() };
    assert!(ask.changed && ask.changes.iter().any(|l| l.contains("--changed")), "{ask:?}");
    let (t, asked) = run_plugin_task(&mut c, "acme-docs", None, Decision::AllowOnce);
    assert!(asked.is_none(), "there is no such tool: nothing asks");
    let runs = r.runs();
    assert!(served(&runs[1]).is_empty(), "a changed plugin never reaches a run");
    assert!(transcript(&mut c, &t.id).parts.iter().any(|p| matches!(&p.body, PartBody::Agent { text, .. } if text.contains("was not available"))));
    // Approved again, it is back.
    approve(&mut c, &changed);
    run_plugin_task(&mut c, "acme-docs", None, Decision::AllowOnce);
    assert!(r.runs()[2].1.as_ref().is_some_and(|c| c["content"]["mcpServers"]["acme-docs"]["args"].as_array().is_some_and(|a| a.iter().any(|x| x == "--changed"))));
}

#[test]
fn a_plugin_folders_file_changing_is_a_changed_manifest_before_the_next_run() {
    let r = rig("plugins-folder-changed", &[]);
    let folder = r.d.dir.join("acme-plugin");
    std::fs::create_dir_all(&folder).unwrap();
    let write = |extra: &[&str]| {
        let mut args = vec!["--name".to_owned(), "from-folder".to_owned()];
        args.extend(extra.iter().map(|s| (*s).to_owned()));
        std::fs::write(folder.join(MANIFEST_FILE), json!({"neko_plugin": 1, "name": "folder-docs", "kind": "mcp", "mcp": {"command": fake_mcp().to_string_lossy(), "args": args}}).to_string()).unwrap();
    };
    write(&[]);
    let mut c = r.client();
    let Reply::Plugin { plugin } = c.request(Request::AddPluginFolder { folder: folder.to_string_lossy().into_owned() }).unwrap() else { panic!() };
    approve(&mut c, &plugin);
    run_plugin_task(&mut c, "folder-docs", None, Decision::AllowOnce);
    assert_eq!(served(&r.runs()[0]), ["folder-docs"]);
    // The file changes on disk; a screen watching hears of it by itself.
    write(&["--evil"]);
    let heard = events_until(&mut c, "the plugin to be changed", |ch| matches!(ch, Change::PluginUpdated { plugin } if plugin.approval == Approval::Changed));
    assert!(heard.iter().any(|ch| matches!(ch, Change::NeedsChanged { .. })) || needs(&mut c).iter().any(|i| matches!(&i.need, Need::Plugin { ask } if ask.changed)));
    run_plugin_task(&mut c, "folder-docs", None, Decision::AllowOnce);
    assert!(served(&r.runs()[1]).is_empty(), "a changed file is a plugin that does not run");
}

#[test]
fn which_cats_may_use_a_plugin_decides_who_gets_it() {
    let r = rig("plugins-cats", &[]);
    let mut c = r.client();
    let p = add(&mut c, &paste("acme-docs", &[]));
    approve(&mut c, &p);
    let Reply::Plugin { plugin } = c.request(Request::SetPluginAccess { plugin: p.id.clone(), access: Access { mode: AccessMode::Only, cats: vec!["cat_miso".into()] } }).unwrap() else { panic!() };
    assert_eq!(plugin.access.mode, AccessMode::Only);
    run_plugin_task(&mut c, "acme-docs", Some("cat_miso"), Decision::AllowOnce);
    run_plugin_task(&mut c, "acme-docs", Some("cat_tofu"), Decision::AllowOnce);
    let runs = r.runs();
    assert_eq!(served(&runs[0]), ["acme-docs"], "miso may");
    assert!(served(&runs[1]).is_empty(), "tofu may not");
    // Off is off for every cat.
    c.request(Request::SetPluginEnabled { plugin: p.id.clone(), on: false }).unwrap();
    run_plugin_task(&mut c, "acme-docs", Some("cat_miso"), Decision::AllowOnce);
    assert!(served(&r.runs()[2]).is_empty());
    assert!(matches!(c.request(Request::SetPluginAccess { plugin: p.id.clone(), access: Access { mode: AccessMode::Only, cats: vec!["cat_nobody".into()] } }).unwrap(), Reply::Error { code: ErrorCode::Refused, .. }));
}

#[test]
fn a_run_with_no_plugins_has_only_nekos_own_server() {
    let r = rig("plugins-none", &[]);
    let mut c = r.client();
    let task = create(&mut c, "Hello", None);
    events_until(&mut c, "done", task_is(&task.id, TaskStatus::Done));
    let runs = r.runs();
    assert!(runs[0].0.iter().any(|a| a == "--mcp-config"), "Neko's own server is in every run's config");
    assert!(served(&runs[0]).is_empty(), "and no plugin");
    assert_eq!(runs[0].1.as_ref().unwrap()["content"]["mcpServers"].as_object().unwrap().keys().collect::<Vec<_>>(), ["neko"]);
    assert!(!r.d.dir.join("plugins-run").exists() || std::fs::read_dir(r.d.dir.join("plugins-run")).unwrap().count() == 0);
}

#[test]
fn removing_a_plugin_removes_its_secrets_and_a_killed_daemons_config_is_swept_at_start() {
    let mut r = rig("plugins-remove", &[]);
    let mut c = r.client();
    let p = add(&mut c, &paste("acme-docs", &[]));
    assert!(std::fs::read_to_string(&r.secrets).unwrap().contains(SECRET));
    assert_eq!(c.request(Request::RemovePlugin { plugin: p.id.clone() }).unwrap(), Reply::Ok);
    assert!(list(&mut c).is_empty());
    let left = std::fs::read_to_string(&r.secrets).unwrap();
    assert!(!left.contains(SECRET) && !left.contains(&p.id), "its Keychain items went with it: {left}");
    assert!(matches!(c.request(Request::RemovePlugin { plugin: p.id.clone() }).unwrap(), Reply::Error { code: ErrorCode::NotFound, .. }));
    drop(c);
    assert!(r.d.stop().success());
    // A run's config a killed daemon left behind is removed when the next one starts.
    let stale = r.d.dir.join("plugins-run").join("run_dead");
    std::fs::create_dir_all(&stale).unwrap();
    std::fs::write(stale.join("mcp.json"), "{}").unwrap();
    let _again = Daemon::start_in(r.d.dir.clone());
    assert!(!stale.exists());
}
