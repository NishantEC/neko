//! Plugins in the core (docs/knowledge/plugins.md): the records, the approval that gates a run, the settings, the per-run config and
//! the check of a server against the fake MCP server. No real server runs, and nothing reaches the Keychain.

mod support;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use neko_core::mcp::{self, Launch, Target};
use neko_core::plugins::{PluginError, Stored};
use neko_core::store::SCHEMA_VERSION;
use neko_core::{Core, FixedClock, SequentialIds, Store};
use neko_protocol::plugins::*;
use neko_protocol::{Need, PROTOCOL_VERSION};
use support::*;

fn open(d: &Temp) -> Core {
    Core::open(&d.0.join("neko.sqlite3"), Box::new(FixedClock::new(1_764_000_000_000, 1000)), Box::new(SequentialIds::default())).unwrap()
}

fn fake_mcp() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_neko-fake-mcp"))
}

const PASTE: &str = r#"{"mcpServers": {"acme-docs": {"command": "node", "args": ["server.js"], "env": {"ACME_TOKEN": "s3cret-value"}}}}"#;

fn add(c: &mut Core, paste: &str) -> Vec<Plugin> {
    c.add_plugins(parse_mcp_json(paste, None).unwrap(), None).unwrap().into_iter().map(|(p, _)| p).collect()
}

fn approved(c: &mut Core, paste: &str) -> Plugin {
    let p = add(c, paste).remove(0);
    c.approve_plugin(&p.id, &p.digest).unwrap().0
}

#[test]
fn a_new_plugin_waits_and_nothing_of_it_reaches_a_run() {
    let d = Temp::new("plugins-wait");
    let mut c = open(&d);
    let p = add(&mut c, PASTE).remove(0);
    assert_eq!((p.name.as_str(), p.approval, p.state(), p.enabled), ("acme-docs", Approval::Waiting, PluginState::Waiting, true));
    assert!(p.approved_digest.is_none() && p.approved_ms.is_none());
    assert_eq!(p.digest, p.manifest.digest(None));
    assert!(c.plugins_for("cat_miso").unwrap().is_empty(), "an unapproved plugin never reaches a run");
    // The paste's secret is marked set, and that is all the store knows of it.
    assert_eq!(p.value("acme_token").map(|v| (v.set, v.value.clone())), Some((true, None)));
    let events: Vec<String> = c.store.events_since(0).unwrap().into_iter().map(|e| e.1).collect();
    assert_eq!(events.last().map(String::as_str), Some("plugin_added"));
}

#[test]
fn only_an_approval_of_the_digest_it_has_now_lets_a_plugin_run() {
    let d = Temp::new("plugins-approve");
    let mut c = open(&d);
    let p = add(&mut c, PASTE).remove(0);
    assert!(matches!(c.approve_plugin(&p.id, "sha256:not-this-one"), Err(PluginError::Refused(w)) if w.contains("changed while you were looking")));
    assert!(c.plugins_for("cat_miso").unwrap().is_empty());
    let (a, _) = c.approve_plugin(&p.id, &p.digest).unwrap();
    assert_eq!((a.approval, a.state(), a.approved_digest.as_deref()), (Approval::Approved, PluginState::On, Some(p.digest.as_str())));
    assert!(a.approved_ms.is_some_and(|ms| ms > 1_764_000_000_000), "when it was approved is kept");
    assert_eq!(c.plugins_for("cat_miso").unwrap().len(), 1);
    // An edit to the manifest: changed, left out, and its digest is a new one.
    let mut m = a.manifest.clone();
    m.mcp.args.push("--verbose".into());
    let (ch, _) = c.update_plugin_manifest(&p.id, m).unwrap();
    assert_eq!((ch.approval, ch.state()), (Approval::Changed, PluginState::Changed));
    assert_ne!(ch.digest, p.digest);
    assert_eq!(ch.approved_digest.as_deref(), Some(p.digest.as_str()), "what was approved is remembered");
    assert!(ch.changes.iter().any(|l| l.contains("server.js --verbose")), "{:?}", ch.changes);
    assert!(c.plugins_for("cat_miso").unwrap().is_empty(), "a changed plugin never reaches a run");
    // Approving the old digest again is refused; the new one is fine.
    assert!(c.approve_plugin(&p.id, &p.digest).is_err());
    assert_eq!(c.approve_plugin(&p.id, &ch.digest).unwrap().0.state(), PluginState::On);
    // Putting the first manifest back does not bring the first approval back: only the newest approval counts.
    let mut back = ch.manifest.clone();
    back.mcp.args.pop();
    assert_eq!(c.update_plugin_manifest(&p.id, back).unwrap().0.approval, Approval::Changed, "only the last approval counts");
}

#[test]
fn declining_stops_the_asking_until_the_manifest_changes() {
    let d = Temp::new("plugins-decline");
    let mut c = open(&d);
    let p = add(&mut c, PASTE).remove(0);
    assert_eq!(neko_core::needs::derive(&c.store).unwrap().len(), 1);
    let (dec, _) = c.decline_plugin(&p.id).unwrap();
    assert_eq!((dec.approval, dec.state()), (Approval::Declined, PluginState::Declined));
    assert!(neko_core::needs::derive(&c.store).unwrap().is_empty(), "no asking again");
    assert!(c.plugins_for("cat_miso").unwrap().is_empty());
    let mut m = dec.manifest.clone();
    m.version = "2".into();
    c.update_plugin_manifest(&p.id, m).unwrap();
    assert_eq!(neko_core::needs::derive(&c.store).unwrap().len(), 1, "a new manifest is asked about again");
    let a = c.approve_plugin(&p.id, &c.store.plugin(&p.id).unwrap().unwrap().digest).unwrap().0;
    assert!(matches!(c.decline_plugin(&a.id), Err(PluginError::Refused(_))), "an approved plugin is turned off or removed, not declined");
}

#[test]
fn needs_you_shows_exactly_what_will_run_and_what_changed() {
    let d = Temp::new("plugins-needs");
    let mut c = open(&d);
    let p = add(&mut c, PASTE).remove(0);
    let items = neko_core::needs::derive(&c.store).unwrap();
    assert_eq!(items.len(), 1);
    let item = &items[0];
    assert_eq!((item.id.as_str(), item.title.as_str(), item.task.0.as_str(), item.workspace.as_str(), item.cat.as_str()), (format!("plugin:{}", p.id).as_str(), "acme-docs", "", "", ""));
    let Need::Plugin { ask } = &item.need else { panic!("not a plugin item: {item:?}") };
    assert_eq!((ask.runs.as_str(), ask.passes.clone(), ask.changed, ask.digest.as_str()), ("node server.js", vec!["ACME_TOKEN".to_owned()], false, p.digest.as_str()));
    let ask_json = serde_json::to_string(&item).unwrap();
    assert!(!ask_json.contains("s3cret-value"), "{ask_json}");
    c.approve_plugin(&p.id, &p.digest).unwrap();
    assert!(neko_core::needs::derive(&c.store).unwrap().is_empty());
    let mut m = p.manifest.clone();
    m.mcp.command = Some("bun".into());
    c.update_plugin_manifest(&p.id, m).unwrap();
    let again = neko_core::needs::derive(&c.store).unwrap();
    let Need::Plugin { ask } = &again[0].need else { panic!() };
    assert!(ask.changed && ask.runs == "bun server.js" && ask.changes.iter().any(|l| l == "Runs: node server.js to bun server.js"), "{ask:?}");
}

#[test]
fn which_cats_may_use_a_plugin_is_all_or_a_list() {
    let d = Temp::new("plugins-access");
    let mut c = open(&d);
    let p = approved(&mut c, PASTE);
    let ids = |c: &Core, cat: &str| c.plugins_for(cat).unwrap().len();
    assert_eq!((ids(&c, "cat_miso"), ids(&c, "cat_tofu")), (1, 1), "every cat by default");
    c.set_plugin_access(&p.id, Access { mode: AccessMode::Only, cats: vec!["cat_miso".into(), "cat_miso".into()] }).unwrap();
    assert_eq!((ids(&c, "cat_miso"), ids(&c, "cat_tofu"), ids(&c, "cat_mochi")), (1, 0, 0));
    assert_eq!(c.store.plugin(&p.id).unwrap().unwrap().access.cats, vec!["cat_miso"], "no repeats");
    c.set_plugin_access(&p.id, Access { mode: AccessMode::Except, cats: vec!["cat_miso".into()] }).unwrap();
    assert_eq!((ids(&c, "cat_miso"), ids(&c, "cat_tofu")), (0, 1));
    assert!(matches!(c.set_plugin_access(&p.id, Access { mode: AccessMode::Only, cats: vec![] }), Err(PluginError::Refused(_))));
    assert!(matches!(c.set_plugin_access(&p.id, Access { mode: AccessMode::Only, cats: vec!["cat_nobody".into()] }), Err(PluginError::Refused(_))));
    c.set_plugin_access(&p.id, Access::default()).unwrap();
    assert_eq!(ids(&c, "cat_miso"), 1);
    c.set_plugin_enabled(&p.id, false).unwrap();
    assert_eq!(ids(&c, "cat_miso"), 0, "off is off");
    assert_eq!(c.store.plugin(&p.id).unwrap().unwrap().state(), PluginState::Off);
}

#[test]
fn names_are_unique_and_a_paste_is_all_or_nothing() {
    let d = Temp::new("plugins-names");
    let mut c = open(&d);
    add(&mut c, PASTE);
    let again = parse_mcp_json(r#"{"mcpServers": {"fresh": {"command": "x"}, "ACME-docs": {"command": "y"}}}"#, None).unwrap();
    assert!(matches!(c.add_plugins(again, None), Err(PluginError::Refused(w)) if w.contains("already a plugin called")));
    assert_eq!(c.plugins().unwrap().len(), 1, "nothing of the paste was added");
    let many: String = (0..=MAX_PLUGINS).map(|i| format!("\"s{i}\":{{\"command\":\"x\"}}")).collect::<Vec<_>>().join(",");
    assert!(parse_mcp_json(&format!("{{{many}}}"), None).is_err());
}

#[test]
fn settings_are_checked_against_what_the_plugin_declared() {
    let d = Temp::new("plugins-settings");
    let mut c = open(&d);
    let m = Manifest::parse_file(
        r#"{"neko_plugin":1,"name":"acme","kind":"mcp","mcp":{"command":"node","args":["s.js","${settings.space}"],"env":{"T":"${settings.token}","CACHE":"${settings.cache}"}},
        "settings":[{"key":"token","kind":"secret","label":"Token","required":true},{"key":"space","kind":"choice","label":"Space","choices":["a","b"],"default":"a"},
        {"key":"cache","kind":"yes_no","label":"Cache","default":"yes"},{"key":"root","kind":"folder","label":"Root"},{"key":"note","kind":"text","label":"Note"}]}"#,
    )
    .unwrap();
    let p = c.add_plugins(vec![Parsed { manifest: m, secrets: vec![], notes: vec![] }], None).unwrap().remove(0).0;
    let a = c.approve_plugin(&p.id, &p.digest).unwrap().0;
    assert_eq!((a.state(), a.missing.clone()), (PluginState::NeedsSetup, vec!["Token".to_owned()]), "a required setting with no value");
    assert!(c.plugins_for("cat_miso").unwrap().is_empty(), "a plugin that needs setting up is left out");
    let bad = |c: &Core, k: &str, v: &str| c.check_plugin_setting(&p.id, k, Some(v)).unwrap_err().to_string();
    assert!(bad(&c, "nope", "x").contains("no setting called nope"));
    assert!(bad(&c, "space", "z").contains("not one of the choices"));
    assert!(bad(&c, "cache", "maybe").contains("yes or no"));
    assert!(bad(&c, "root", "relative/path").contains("full path"));
    assert!(bad(&c, "note", &"x".repeat(MAX_VALUE + 1)).contains("at most"));
    assert!(bad(&c, "token", &"x".repeat(MAX_SECRET + 1)).contains("too long"));
    let after_secret = c.put_plugin_setting(&p.id, "token", Some("s3cret-value")).unwrap().0;
    assert_eq!(after_secret.value("token").map(|v| (v.set, v.value.clone())), Some((true, None)), "a secret is only ever set");
    assert_eq!(after_secret.state(), PluginState::On);
    let json = serde_json::to_string(&after_secret).unwrap();
    assert!(!json.contains("s3cret-value"), "{json}");
    assert!(c.store.plugin_values(&p.id).unwrap()["token"] == Stored { value: None, secret: true }, "the store's row holds no value");
    c.put_plugin_setting(&p.id, "note", Some("Lee's note")).unwrap();
    c.put_plugin_setting(&p.id, "space", Some("b")).unwrap();
    assert_eq!(c.store.plugin(&p.id).unwrap().unwrap().value("note").unwrap().value.as_deref(), Some("Lee's note"));
    // What a run is handed: values, defaults, yes as true, and a secret from the caller.
    let stored = c.store.plugin_values(&p.id).unwrap();
    let plugin = c.store.plugin(&p.id).unwrap().unwrap();
    let secrets = BTreeMap::from([("token".to_owned(), "s3cret-value".to_owned())]);
    let Target::Stdio(l) = mcp::target(&plugin, &stored, &secrets) else { panic!("a command") };
    assert_eq!((l.program.as_str(), l.args.clone()), ("node", vec!["s.js".to_owned(), "b".to_owned()]));
    assert_eq!((l.env["T"].as_str(), l.env["CACHE"].as_str()), ("s3cret-value", "true"));
    c.put_plugin_setting(&p.id, "token", None).unwrap();
    assert_eq!(c.store.plugin(&p.id).unwrap().unwrap().state(), PluginState::NeedsSetup, "cleared again");
    let (accounts, _) = c.remove_plugin(&p.id).unwrap();
    assert_eq!(accounts, vec![format!("{}/token", p.id)], "the secrets to take out of the Keychain");
    assert!(c.store.plugin_values(&p.id).unwrap().is_empty() && c.plugins().unwrap().is_empty());
}

#[test]
fn a_run_gets_a_config_of_its_plugins_with_their_secrets_in_env() {
    let d = Temp::new("plugins-config");
    let mut c = open(&d);
    let p = approved(&mut c, r#"{"mcpServers": {"acme-docs": {"command": "node", "args": ["s.js"], "env": {"ACME_TOKEN": "tok"}}, "remote": {"type": "sse", "url": "https://example.com/sse", "headers": {"X-Key": "k"}}}}"#);
    let plugins = c.plugins().unwrap();
    let stored = |id: &str| c.store.plugin_values(id).unwrap();
    let docs = plugins.iter().find(|x| x.name == "acme-docs").unwrap();
    let remote = plugins.iter().find(|x| x.name == "remote").unwrap();
    let servers = vec![
        ("acme-docs".to_owned(), mcp::target(docs, &stored(&docs.id), &BTreeMap::from([("acme_token".to_owned(), "tok".to_owned())]))),
        ("remote".to_owned(), mcp::target(remote, &stored(&remote.id), &BTreeMap::from([("header_x_key".to_owned(), "k".to_owned())]))),
    ];
    let cfg = mcp::claude_config(&servers);
    assert_eq!(
        cfg,
        serde_json::json!({"mcpServers": {
            "acme-docs": {"type": "stdio", "command": "node", "args": ["s.js"], "env": {"ACME_TOKEN": "tok"}},
            "remote": {"type": "sse", "url": "https://example.com/sse", "headers": {"X-Key": "k"}}
        }})
    );
    assert_eq!(p.name, "acme-docs");
}

#[test]
fn a_folder_plugin_is_read_from_its_file_and_a_changed_file_is_a_changed_manifest() {
    let d = Temp::new("plugins-folder");
    let mut c = open(&d);
    let dir = d.0.join("acme");
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join(MANIFEST_FILE);
    let text = r#"{"neko_plugin":1,"name":"acme-docs","version":"1","kind":"mcp","mcp":{"command":"node","args":["${plugin.dir}/server.js"]}}"#;
    std::fs::write(&file, text).unwrap();
    assert!(matches!(neko_core::plugins::read_folder(&d.0.join("nowhere").to_string_lossy()), Err(PluginError::Invalid(_))));
    assert!(matches!(neko_core::plugins::read_folder(&d.0.to_string_lossy()), Err(PluginError::Invalid(w)) if w.contains("no neko-plugin.json")));
    let (m, real) = neko_core::plugins::read_folder(&dir.to_string_lossy()).unwrap();
    let p = c.add_plugins(vec![Parsed { manifest: m, secrets: vec![], notes: vec![] }], Some(&real)).unwrap().remove(0).0;
    assert_eq!(p.folder.as_deref(), Some(real.as_str()));
    assert_eq!(p.digest, p.manifest.digest(Some(&real)), "the folder is part of the digest");
    assert!(matches!(c.update_plugin_manifest(&p.id, p.manifest.clone()), Err(PluginError::Refused(_))), "its file is what is edited");
    c.approve_plugin(&p.id, &p.digest).unwrap();
    // Nothing differs: nothing happens.
    assert!(c.apply_folder_read(&p.id, neko_core::plugins::read_plugin_file(&real)).unwrap().is_none());
    // The file changes: the plugin is changed, and is left out.
    std::fs::write(&file, text.replace("server.js", "evil.js")).unwrap();
    let (ch, _) = c.apply_folder_read(&p.id, neko_core::plugins::read_plugin_file(&real)).unwrap().expect("a change");
    assert_eq!(ch.state(), PluginState::Changed);
    assert!(ch.changes.iter().any(|l| l.contains("evil.js")), "{:?}", ch.changes);
    assert!(c.plugins_for("cat_miso").unwrap().is_empty());
    // Put back as it was: it is the very manifest that was approved, so it runs again.
    std::fs::write(&file, text).unwrap();
    assert_eq!(c.apply_folder_read(&p.id, neko_core::plugins::read_plugin_file(&real)).unwrap().unwrap().0.state(), PluginState::On);
    // The file goes: a problem, and not run.
    std::fs::remove_file(&file).unwrap();
    let (gone, _) = c.apply_folder_read(&p.id, neko_core::plugins::read_plugin_file(&real)).unwrap().unwrap();
    assert_eq!(gone.state(), PluginState::Problem);
    assert!(gone.problem.as_deref().is_some_and(|w| w.contains("is not in")), "{:?}", gone.problem);
    assert!(c.plugins_for("cat_miso").unwrap().is_empty());
    assert!(c.apply_folder_read(&p.id, neko_core::plugins::read_plugin_file(&real)).unwrap().is_none(), "the same problem is said once");
    // A manifest that no longer parses is a problem too.
    std::fs::write(&file, "{ not json").unwrap();
    assert!(c.apply_folder_read(&p.id, neko_core::plugins::read_plugin_file(&real)).unwrap().unwrap().0.problem.is_some());
}

#[test]
fn the_migration_from_store_seven_adds_plugins_and_changes_nothing_else() {
    let d = Temp::new("plugins-migrate");
    let path = d.0.join("neko.sqlite3");
    {
        let mut core = Core::open(&path, Box::new(FixedClock::new(1_764_000_000_000, 1000)), Box::new(SequentialIds::default())).unwrap();
        core.create_task("Rename the sidebar label").unwrap();
    }
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch("DROP TABLE plugin_values; DROP TABLE plugins; ALTER TABLE tasks DROP COLUMN last_read_ms; ALTER TABLE tasks DROP COLUMN activity_ms; ALTER TABLE tasks DROP COLUMN latest_line; PRAGMA user_version = 7;").unwrap();
    }
    assert_eq!(Store::open_at(&path, 7).unwrap().version().unwrap(), 7);
    let mut core = Core::open(&path, Box::new(FixedClock::new(1_764_000_100_000, 1000)), Box::new(SequentialIds::default())).unwrap();
    assert_eq!((core.store.version().unwrap(), SCHEMA_VERSION), (9, 9));
    assert_eq!(core.store.all_tasks().unwrap().len(), 1);
    assert!(core.plugins().unwrap().is_empty());
    assert_eq!(PROTOCOL_VERSION, 12);
    assert_eq!(add(&mut core, PASTE).len(), 1);
}

// ---- The check of a server, against the fake MCP server -----------------------------------------------------------------------------

fn launch(args: &[&str]) -> Launch {
    Launch { program: fake_mcp().to_string_lossy().into_owned(), args: args.iter().map(|a| (*a).to_owned()).collect(), env: BTreeMap::new() }
}

fn run_check(l: &Launch, limit: Duration) -> Result<mcp::Found, String> {
    let d = Temp::new("mcp-cwd");
    mcp::check(&fake_mcp(), l, &d.0, "/usr/bin:/bin", limit)
}

#[test]
fn a_healthy_server_says_who_it_is_and_lists_every_tool() {
    let found = run_check(&launch(&[]), Duration::from_secs(10)).unwrap();
    assert_eq!((found.server.as_deref(), found.version.as_deref()), (Some("fake-docs"), Some("1.2.3")));
    assert_eq!(found.tools, ["search", "fetch_page", "list_spaces"]);
    let paged = run_check(&launch(&["--tools", "a,b,c,d,e", "--pages", "2", "--name", "paged"]), Duration::from_secs(10)).unwrap();
    assert_eq!((paged.server.as_deref(), paged.tools), (Some("paged"), vec!["a", "b", "c", "d", "e"].into_iter().map(String::from).collect::<Vec<_>>()), "every page");
    let none = run_check(&launch(&["--mode", "no-list"]), Duration::from_secs(10)).unwrap();
    assert!(none.tools.is_empty(), "a server with nothing to list has no tools");
}

#[test]
fn the_server_gets_its_environment_and_nothing_of_the_daemons() {
    let mut l = launch(&["--echo-env", "ACME_TOKEN"]);
    assert!(run_check(&l, Duration::from_secs(10)).unwrap().tools.contains(&"env_ACME_TOKEN_unset".to_owned()));
    l.env.insert("ACME_TOKEN".into(), "x".into());
    assert!(run_check(&l, Duration::from_secs(10)).unwrap().tools.contains(&"env_ACME_TOKEN_set".to_owned()));
}

#[test]
fn a_server_that_never_answers_ends_at_the_deadline_and_nothing_it_started_outlives_it() {
    let d = Temp::new("mcp-hang");
    let (pid, grand) = (d.0.join("pid"), d.0.join("grand"));
    let l = launch(&["--mode", "hang", "--pidfile", &pid.to_string_lossy(), "--grandchild", &grand.to_string_lossy()]);
    let began = Instant::now();
    let why = run_check(&l, Duration::from_secs(2)).unwrap_err();
    let took = began.elapsed();
    assert!(why.contains("fake-mcp did not answer within 2 seconds"), "{why}");
    assert!(took >= Duration::from_secs(2) && took < Duration::from_secs(4), "{took:?}");
    let (server, child) = (pid_in(&pid), pid_in(&grand));
    let _reap = (Reap(server), Reap(child));
    until("the server and what it started to be gone", Duration::from_secs(10), || !alive(server) && !alive(child));
}

#[test]
fn a_server_that_crashes_or_speaks_garbage_or_refuses_says_so_in_words() {
    let crash = run_check(&launch(&["--mode", "crash"]), Duration::from_secs(10)).unwrap_err();
    assert!(crash.contains("stopped before it answered (exit code 3)") && crash.contains("the fake server fell over"), "{crash}");
    let garbage = run_check(&launch(&["--mode", "garbage"]), Duration::from_secs(10)).unwrap_err();
    assert!(garbage.contains("not MCP") && garbage.contains("<html>"), "{garbage}");
    let refused = run_check(&launch(&["--mode", "refuse"]), Duration::from_secs(10)).unwrap_err();
    assert!(refused.contains("refused to start MCP: Bad token"), "{refused}");
    let slow = run_check(&launch(&["--mode", "slow-list"]), Duration::from_secs(1)).unwrap_err();
    assert!(slow.contains("did not answer within 1 second"), "{slow}");
    let missing = run_check(&Launch { program: "/nonexistent/server".into(), args: vec![], env: BTreeMap::new() }, Duration::from_secs(5)).unwrap_err();
    assert!(missing.contains("could not start server"), "{missing}");
}
