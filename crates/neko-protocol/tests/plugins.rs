//! Plugins' manifest, the reading of pasted MCP JSON, the digest, and the shapes on the wire (docs/knowledge/plugins.md).

use neko_protocol::plugins::*;
use neko_protocol::*;

const FILE: &str = r#"{
  "neko_plugin": 1,
  "name": "acme-docs",
  "version": "1.2.0",
  "description": "Search Acme's docs.",
  "kind": "mcp",
  "mcp": { "command": "node", "args": ["${plugin.dir}/server.js", "--space", "${settings.space}"], "env": { "ACME_TOKEN": "${settings.token}" } },
  "settings": [
    { "key": "token", "kind": "secret", "label": "Acme token", "help": "From Acme's account page.", "required": true },
    { "key": "space", "kind": "choice", "label": "Space", "choices": ["public", "team"], "default": "public" },
    { "key": "cache", "kind": "yes_no", "label": "Keep a cache", "default": "yes" },
    { "key": "root", "kind": "folder", "label": "Docs folder" },
    { "key": "note", "kind": "text", "label": "A note" }
  ]
}"#;

fn manifest() -> Manifest {
    Manifest::parse_file(FILE).expect("the example manifest is good")
}

#[test]
fn a_manifest_file_is_read_and_its_fields_are_kept() {
    let m = manifest();
    assert_eq!((m.name.as_str(), m.version.as_str(), m.kind), ("acme-docs", "1.2.0", PluginKind::Mcp));
    assert_eq!(m.mcp.command.as_deref(), Some("node"));
    assert_eq!(m.settings.len(), 5);
    assert_eq!(m.settings[0].kind, FieldKind::Secret);
    assert!(m.settings[0].required);
    assert_eq!(m.passes(), vec!["ACME_TOKEN"]);
    assert_eq!(m.command_line(Some("/Users/example/plugins/acme")), "node /Users/example/plugins/acme/server.js --space ${settings.space}");
}

#[test]
fn a_manifest_file_that_is_not_good_says_why() {
    let bad = |from: &str, to: &str| Manifest::parse_file(&FILE.replacen(from, to, 1)).unwrap_err();
    assert!(bad("\"neko_plugin\": 1", "\"neko_plugin\": 2").contains("format 2"));
    assert!(bad("acme-docs", "acme docs").contains("not a plugin name"));
    assert!(bad("acme-docs", "a__b").contains("not a plugin name"));
    assert!(bad("\"command\": \"node\"", "\"command\": \"node\", \"url\": \"https://example.com/mcp\"").contains("not both"));
    assert!(bad("\"command\": \"node\", ", "").contains("needs a command"));
    assert!(bad("\"ACME_TOKEN\"", "\"ACME TOKEN\"").contains("not an environment variable"));
    assert!(bad("${settings.space}", "${settings.nope}").contains("no setting “nope”"));
    assert!(bad("\"kind\": \"secret\"", "\"kind\": \"secret\", \"default\": \"x\"").contains("default that does not fit"));
    assert!(bad("\"choices\": [\"public\", \"team\"]", "\"choices\": []").contains("needs 1 to 20 values"));
    assert!(bad("\"key\": \"space\"", "\"key\": \"token\"").contains("declared twice"));
    assert!(bad("\"key\": \"space\"", "\"key\": \"Space\"").contains("not a setting key"));
    assert!(bad("\"name\": \"acme-docs\"", "\"name\": \"acme-docs\", \"commnad\": \"x\"").contains("unknown field"), "a misspelt key is not ignored");
    assert!(Manifest::parse_file("{").unwrap_err().contains("cannot be read"));
    assert!(Manifest::parse_file(&" ".repeat(MAX_MANIFEST_BYTES + 1)).unwrap_err().contains("longer than"));
    let url_with_args = r#"{"neko_plugin":1,"name":"w","kind":"mcp","mcp":{"url":"https://example.com/mcp","args":["x"]}}"#;
    assert!(Manifest::parse_file(url_with_args).unwrap_err().contains("takes no args"));
    let ftp = r#"{"neko_plugin":1,"name":"w","kind":"mcp","mcp":{"url":"ftp://example.com/mcp"}}"#;
    assert!(Manifest::parse_file(ftp).unwrap_err().contains("http://"));
}

#[test]
fn the_digest_is_stable_and_follows_what_the_manifest_says() {
    let a = manifest();
    assert!(a.digest(None).starts_with("sha256:") && a.digest(None).len() == 7 + 64);
    // The same manifest written in another order and with other spacing has the same digest.
    let reordered: serde_json::Value = serde_json::from_str(FILE).unwrap();
    let mut shuffled = serde_json::Map::new();
    for k in ["settings", "mcp", "kind", "description", "version", "name", "neko_plugin"] {
        shuffled.insert(k.to_owned(), reordered[k].clone());
    }
    let b = Manifest::parse_file(&serde_json::to_string_pretty(&shuffled).unwrap()).unwrap();
    assert_eq!(a.digest(None), b.digest(None));
    // Anything that changes what runs, or what is asked, changes it.
    let mut c = a.clone();
    c.mcp.args.push("--verbose".into());
    assert_ne!(a.digest(None), c.digest(None));
    let mut d = a.clone();
    d.version = "1.2.1".into();
    assert_ne!(a.digest(None), d.digest(None));
    let mut e = a.clone();
    e.settings[4].label = "Another note".into();
    assert_ne!(a.digest(None), e.digest(None));
    // The folder it came from is part of it: the same manifest somewhere else is not what was approved.
    assert_ne!(a.digest(Some("/Users/example/a")), a.digest(Some("/Users/example/b")));
    assert_ne!(a.digest(None), a.digest(Some("/Users/example/a")));
    // And it never changes: this digest is pinned, since approvals already given depend on it.
    let tiny = Manifest::parse_file(r#"{"neko_plugin":1,"name":"tiny","kind":"mcp","mcp":{"command":"tool"}}"#).unwrap();
    assert_eq!(serde_json::to_string(&tiny).unwrap(), r#"{"neko_plugin":1,"name":"tiny","version":"0","kind":"mcp","mcp":{"command":"tool"}}"#);
    assert_eq!(tiny.digest(None), "sha256:8d67542f9cbfcc01673cc8909b4b6489b975be60250072f7650a6cacb2540627");
}

#[test]
fn changes_are_named_in_plain_lines() {
    let old = manifest();
    let mut new = old.clone();
    new.mcp.args[0] = "${plugin.dir}/other.js".into();
    new.mcp.env.insert("ACME_REGION".into(), "eu".into());
    new.version = "2.0.0".into();
    new.settings.push(SettingField { key: "extra".into(), kind: FieldKind::Text, label: "Extra".into(), help: None, default: None, choices: Vec::new(), required: false });
    let lines = new.changes_from(&old, Some("/p"));
    assert!(lines.contains(&"Version: 1.2.0 to 2.0.0".to_owned()), "{lines:?}");
    assert!(lines.iter().any(|l| l.starts_with("Runs: node /p/server.js") && l.contains("node /p/other.js")), "{lines:?}");
    assert!(lines.contains(&"Now also passes ACME_REGION".to_owned()), "{lines:?}");
    assert!(lines.contains(&"New setting: Extra".to_owned()), "{lines:?}");
    assert_eq!(old.changes_from(&old, None), vec!["Something else in the manifest changed."]);
}

#[test]
fn a_pasted_block_of_servers_becomes_one_plugin_each_with_secrets_set_apart() {
    let paste = r#"{
      "mcpServers": {
        "Acme Docs": { "command": "npx", "args": ["-y", "@acme/docs-server"], "env": { "ACME_TOKEN": "abc123", "REGION": "eu" } },
        "remote": { "type": "http", "url": "https://example.com/mcp", "headers": { "Authorization": "Bearer xyz" }, "timeout": 5000 },
        "events": { "type": "sse", "url": "https://example.com/sse" }
      }
    }"#;
    let found = parse_mcp_json(paste, None).unwrap();
    assert_eq!(found.iter().map(|p| p.manifest.name.as_str()).collect::<Vec<_>>(), ["Acme-Docs", "events", "remote"]);
    let docs = &found[0];
    assert_eq!(docs.manifest.mcp.command.as_deref(), Some("npx"));
    assert_eq!(docs.manifest.mcp.args, ["-y", "@acme/docs-server"]);
    // The values are not in the manifest: a template is, and the value is set apart as a secret of its own.
    assert_eq!(docs.manifest.mcp.env["ACME_TOKEN"], "${settings.acme_token}");
    assert_eq!(docs.secrets, vec![("acme_token".to_owned(), "abc123".to_owned()), ("region".to_owned(), "eu".to_owned())]);
    assert!(docs.manifest.settings.iter().all(|f| f.kind == FieldKind::Secret));
    let manifest_json = serde_json::to_string(&docs.manifest).unwrap();
    assert!(!manifest_json.contains("abc123") && !manifest_json.contains("\"eu\""), "{manifest_json}");
    let remote = &found[2];
    assert_eq!(remote.manifest.mcp.url.as_deref(), Some("https://example.com/mcp"));
    assert_eq!(remote.manifest.mcp.headers["Authorization"], "${settings.header_authorization}");
    assert_eq!(remote.secrets, vec![("header_authorization".to_owned(), "Bearer xyz".to_owned())]);
    assert_eq!(remote.notes, vec!["“timeout” of remote is not used by Neko and was left out."]);
    assert_eq!(found[1].manifest.mcp.transport.as_deref(), Some("sse"));
    assert!(found.iter().all(|p| p.manifest.validate().is_ok()));
}

#[test]
fn one_server_alone_or_the_bare_map_is_read_too() {
    let one = parse_mcp_json(r#"{"command":"npx","args":["-y","@acme/docs-server@latest"]}"#, None).unwrap();
    assert_eq!(one.len(), 1);
    assert_eq!(one[0].manifest.name, "acme-docs-server", "named from its package");
    let named = parse_mcp_json(r#"{"command":"node","args":["s.js"]}"#, Some("my notes")).unwrap();
    assert_eq!(named[0].manifest.name, "my-notes");
    let inner_name = parse_mcp_json(r#"{"name":"lee-search","command":"node"}"#, None).unwrap();
    assert_eq!(inner_name[0].manifest.name, "lee-search");
    let by_url = parse_mcp_json(r#"{"url":"https://mcp.example.com/v1"}"#, None).unwrap();
    assert_eq!(by_url[0].manifest.name, "mcp-example-com");
    let bare = parse_mcp_json(r#"{"a":{"command":"x"},"b":{"url":"http://127.0.0.1:9/mcp"}}"#, None).unwrap();
    assert_eq!(bare.iter().map(|p| p.manifest.name.as_str()).collect::<Vec<_>>(), ["a", "b"]);
    let vscode = parse_mcp_json(r#"{"servers":{"s":{"command":"x"}}}"#, None).unwrap();
    assert_eq!(vscode[0].manifest.name, "s");
    // A reference to Claude Code's own environment is passed through as written, not made a secret.
    let env_ref = parse_mcp_json(r#"{"command":"x","env":{"A":"${HOME}/x"}}"#, Some("e")).unwrap();
    assert_eq!(env_ref[0].manifest.mcp.env["A"], "${HOME}/x");
    assert!(env_ref[0].secrets.is_empty() && env_ref[0].manifest.settings.is_empty());
}

#[test]
fn a_paste_that_is_not_a_server_says_what_is_wrong() {
    let err = |t: &str| parse_mcp_json(t, None).unwrap_err();
    assert_eq!(err("   "), "Paste a server's JSON here.");
    assert!(err("{\"mcpServers\": {").starts_with("That is not valid JSON: "), "{}", err("{\"mcpServers\": {"));
    assert!(err("{\n  \"command\": \"x\",\n}").contains("line 3"), "{}", err("{\n  \"command\": \"x\",\n}"));
    assert!(err("[1, 2]").contains("Expected a JSON object"));
    assert!(err("{\"hello\": 1}").contains("did not find an MCP server"));
    assert!(err("{\"mcpServers\": {}}").contains("no servers"));
    assert!(err("{\"mcpServers\": {\"a\": 5}}").contains("“a” is not an object"));
    assert!(err("{\"mcpServers\": {\"a\": {\"args\": [\"x\"]}}}").contains("needs a command"));
    assert!(err("{\"mcpServers\": {\"a\": {\"command\": \"x\", \"url\": \"https://e.com\"}}}").contains("not both"));
    assert!(err("{\"mcpServers\": {\"a\": {\"command\": \"x\", \"args\": [1]}}}").contains("every arg must be text"));
    assert!(err("{\"mcpServers\": {\"a\": {\"command\": \"x\", \"env\": {\"K\": [1]}}}}").contains("must be text"));
    assert!(err("{\"mcpServers\": {\"a\": {\"url\": \"e.com/mcp\"}}}").contains("http://"));
    assert!(err("{\"mcpServers\": {\"a b\": {\"command\": \"x\"}, \"a-b\": {\"command\": \"y\"}}}").contains("Two servers would both be called"));
}

#[test]
fn what_a_screen_shows_follows_from_the_plugin() {
    let m = manifest();
    let base = Plugin {
        id: "plugin_1".into(),
        name: m.name.clone(),
        version: m.version.clone(),
        description: None,
        kind: PluginKind::Mcp,
        folder: None,
        digest: m.digest(None),
        manifest: m,
        approval: Approval::Waiting,
        approved_digest: None,
        approved_ms: None,
        changes: Vec::new(),
        enabled: true,
        access: Access::default(),
        values: Vec::new(),
        missing: Vec::new(),
        last_check: None,
        problem: None,
        created_ms: 1,
        changed_ms: 1,
    };
    assert_eq!(base.state(), PluginState::Waiting);
    let on = Plugin { approval: Approval::Approved, ..base.clone() };
    assert_eq!(on.state(), PluginState::On);
    assert!(on.runs_for("cat_miso"));
    assert_eq!(Plugin { enabled: false, ..on.clone() }.state(), PluginState::Off);
    assert_eq!(Plugin { missing: vec!["Acme token".into()], ..on.clone() }.state(), PluginState::NeedsSetup);
    assert_eq!(Plugin { problem: Some("gone".into()), ..on.clone() }.state(), PluginState::Problem);
    assert_eq!(Plugin { approval: Approval::Changed, ..base.clone() }.state(), PluginState::Changed);
    assert!(!Plugin { approval: Approval::Changed, ..on.clone() }.runs_for("cat_miso"), "changed: never runs");
    assert!(!base.runs_for("cat_miso"), "waiting: never runs");
    let only = Plugin { access: Access { mode: AccessMode::Only, cats: vec!["cat_miso".into()] }, ..on.clone() };
    assert!(only.runs_for("cat_miso") && !only.runs_for("cat_tofu"));
    let except = Plugin { access: Access { mode: AccessMode::Except, cats: vec!["cat_miso".into()] }, ..on };
    assert!(!except.runs_for("cat_miso") && except.runs_for("cat_tofu"));
}

#[test]
fn the_shapes_on_the_wire_are_pinned() {
    let m = |c: &ClientMessage| serde_json::to_string(c).unwrap();
    assert_eq!(m(&ClientMessage { id: 1, request: Request::ListPlugins }), r#"{"id":1,"type":"list_plugins"}"#);
    assert_eq!(m(&ClientMessage { id: 2, request: Request::AddMcpJson { json: "{}".into(), name: None } }), r#"{"id":2,"type":"add_mcp_json","json":"{}"}"#);
    assert_eq!(m(&ClientMessage { id: 3, request: Request::AddPluginFolder { folder: "/Users/example/acme".into() } }), r#"{"id":3,"type":"add_plugin_folder","folder":"/Users/example/acme"}"#);
    assert_eq!(m(&ClientMessage { id: 4, request: Request::ApprovePlugin { plugin: "plugin_1".into(), digest: "sha256:ab".into() } }), r#"{"id":4,"type":"approve_plugin","plugin":"plugin_1","digest":"sha256:ab"}"#);
    assert_eq!(m(&ClientMessage { id: 5, request: Request::SetPluginSetting { plugin: "plugin_1".into(), key: "token".into(), value: Some("s".into()) } }), r#"{"id":5,"type":"set_plugin_setting","plugin":"plugin_1","key":"token","value":"s"}"#);
    assert_eq!(m(&ClientMessage { id: 6, request: Request::SetPluginAccess { plugin: "plugin_1".into(), access: Access { mode: AccessMode::Only, cats: vec!["cat_miso".into()] } } }), r#"{"id":6,"type":"set_plugin_access","plugin":"plugin_1","access":{"mode":"only","cats":["cat_miso"]}}"#);
    assert_eq!(m(&ClientMessage { id: 7, request: Request::TestPlugin { plugin: "plugin_1".into() } }), r#"{"id":7,"type":"test_plugin","plugin":"plugin_1"}"#);
    let check = Check { at_ms: 5, ok: true, server: Some("fake-docs".into()), version: Some("1.2.3".into()), tools: vec!["search".into()], error: None };
    assert_eq!(serde_json::to_string(&check).unwrap(), r#"{"at_ms":5,"ok":true,"server":"fake-docs","version":"1.2.3","tools":["search"]}"#);
    // A secret is only ever "set".
    assert_eq!(serde_json::to_string(&FieldValue { key: "token".into(), value: None, set: true }).unwrap(), r#"{"key":"token","set":true}"#);
    let ask = PluginAsk { plugin: "plugin_1".into(), name: "acme-docs".into(), version: "1".into(), digest: "sha256:ab".into(), runs: "node server.js".into(), remote: false, passes: vec!["ACME_TOKEN".into()], changed: false, changes: Vec::new(), description: None, folder: None };
    let need = Need::Plugin { ask };
    assert_eq!(serde_json::to_string(&need).unwrap(), r#"{"kind":"plugin","ask":{"plugin":"plugin_1","name":"acme-docs","version":"1","digest":"sha256:ab","runs":"node server.js","remote":false,"passes":["ACME_TOKEN"]}}"#);
    assert_eq!(serde_json::from_str::<Need>(&serde_json::to_string(&need).unwrap()).unwrap(), need);
    assert_eq!(PROTOCOL_VERSION, 18);
    assert_eq!(serde_json::to_string(&Reply::error(ErrorCode::InvalidPlugin, "no")).unwrap(), r#"{"type":"error","code":"invalid_plugin","message":"no"}"#);
}
