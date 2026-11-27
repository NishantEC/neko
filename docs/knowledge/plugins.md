# Plugins (2026-11-27)

What Neko is extended with. The first kind of plugin is an MCP server. This slice is the host and that one kind: no plugin comes with
Neko, none is installed by it and none is written in the repository, not even as an example. Protocol 11, store 8 (the config a run gets now also holds Neko's own server and is strict about the person's own servers: below, `subagents.md`). Why MCP first,
why digests and why nothing is bundled: `docs/decisions/0002-plugins-start-with-mcp.md`.

## The model

`crates/neko-protocol/src/plugins.rs` (the shapes, the manifest, the digest, the reading of pasted JSON), `crates/neko-core/src/plugins.rs`
(the records and the rules), `crates/neko-core/src/mcp.rs` (what a server is launched with, the run's config, the check),
`crates/neko-core/src/secrets.rs` (the Keychain), `crates/neko-daemon/src/plugins.rs` (the requests, the deadlines, what a run is handed).

A plugin has an id (`plugin_` and 26 letters and digits), a name, a version, a kind (`mcp`; the field is there so later kinds fit), its
manifest, the digest of the manifest, an approval, a switch (on or off), which cats may use it, its settings, its last check and when
it was made and changed.

- **Name.** 1 to 40 characters of `A-Z a-z 0-9 - _`, no `__`, starting with a letter or digit, unique without regard to case. It is the
  server's name in Claude Code, so its tools are `mcp__<name>__<tool>` and a screen can say "acme-docs · search". A pasted server's
  name is made into one (other characters become `-`).
- **Approval** is one of four, never stored as such but worked out from the digests (`Row::approval`):

  | State | When | Runs? |
  | --- | --- | --- |
  | `waiting` | never approved | no |
  | `approved` | the digest approved is the digest it has now | yes, when it is also on, set up and open to the cat |
  | `changed` | something was approved, and the manifest is not that any more | no |
  | `declined` | the person said no to the manifest it has now | no |

  A screen's words for the whole of it (`Plugin::state()`): "Waiting for your approval", "On", "Off", "Changed since you approved it",
  "Declined", "Needs setting up" (a required setting has no value), "Can't be read" (`problem`: its folder or file is gone or no longer
  parses).
- **Which cats.** `access` is every cat (the default), only the cats listed, or every cat except those listed. Only with no cats is
  refused. Cats are never deleted (they retire), so a listed id stays valid.
- **Settings** are what the manifest declares. A value is kept in `plugin_values` except a secret, whose row only says it is set. A
  screen is told, per field, whether it is set and (not for a secret) its value.

## The manifest file

A plugin folder holds `neko-plugin.json`. It is read strictly: a key Neko does not know is an error, so a misspelt key is never ignored.

```json
{
  "neko_plugin": 1,
  "name": "acme-docs",
  "version": "1.2.0",
  "description": "Search Acme's docs.",
  "kind": "mcp",
  "mcp": {
    "command": "node",
    "args": ["${plugin.dir}/server.js", "--space", "${settings.space}"],
    "env": { "ACME_TOKEN": "${settings.token}" }
  },
  "settings": [
    { "key": "token", "kind": "secret", "label": "Acme token", "help": "From Acme's account page.", "required": true },
    { "key": "space", "kind": "choice", "label": "Space", "choices": ["public", "team"], "default": "public" }
  ]
}
```

- `neko_plugin`: the format, 1. Another number is refused with the numbers in words.
- `version` is free text (default `0`); `description` is one line of at most 200 characters.
- `kind` is `mcp`. `mcp` has either a program (`command`, `args`, `env`) or a web address (`url`, `headers`, `transport` `http` or
  `sse`), never both. A `command` takes no headers, a `url` takes no args or env.
- `settings` (at most 20): `key` (`a-z`, digits, `_`), `kind` (`text`, `secret`, `yes_no`, `choice`, `folder`), `label` (one line),
  `help` (at most 300 characters), `default` (not for a secret; `yes` or `no` for a yes/no; one of the values for a choice),
  `choices` (a choice's 1 to 20 values) and `required`.
- **Templates.** Any string of `mcp` may hold `${settings.<key>}` (the setting's value, put in when a run starts or a check runs; a
  yes/no is `true` or `false`; an unset field is its default, else empty) and, for a plugin from a folder, `${plugin.dir}` (that
  folder's real path). Every `${settings.<key>}` must name a declared setting. Anything else, `${HOME}` for one, is left as written for
  Claude Code to read.
- The manifest file may be at most 64 KB.

## Adding a plugin

Adding never runs anything. The plugin waits for approval (`waiting`), and a "Needs you" item asks for it.

1. **Add MCP server… (`add_mcp_json`)** takes what people already have for Claude Desktop, Claude Code, Cursor and the like: one server's
   JSON (`{"command": …}` or `{"url": …}`), a `{"mcpServers": {…}}` block (or VS Code's `servers`), or just the map of servers. Each
   server becomes a plugin, all or none (a name already taken refuses the whole paste). A server with no name is named from its `name`,
   its package (`npx -y @acme/docs-server@latest` is `acme-docs-server`) or its address; a name may be given for one server.
   `parse_mcp_json` is pure and shared, so a screen shows what a paste will add before it sends it, with the daemon's own words for what
   is wrong ("That is not valid JSON: expected `,` or `}` (line 3, column 5)."). Keys Neko does not use (`timeout`, `disabled`) are left
   out and said so. `type` `sse` or `http` (or `streamable-http`) sets the transport.
   - **Secrets leave the manifest at once.** Every value in `env` or `headers` becomes a *secret* setting of its own
     (`ACME_TOKEN` becomes the setting `acme_token`, a header `Authorization` becomes `header_authorization`), and the manifest holds only
     `${settings.acme_token}`. The value goes to the Keychain (below). So the digest, the store and every event hold only names. A value
     that is already a reference to Claude Code's own environment (`${HOME}/x`) is kept as written. Arguments are kept as they are: a
     secret in an argument (`--token abc`) cannot be told from any other word, so it stays in the manifest. Put it in a setting.
2. **Add plugin folder… (`add_plugin_folder`)** reads `neko-plugin.json` in the folder (links resolved). The folder's real path is part
   of the plugin and of its digest.

A plugin with the same name as one already there (any case) is refused with the name, and so is a 65th plugin.

## Approval and digests

The trust root is the person's approval of exactly one manifest. There is no registry, no install script, and no code in a plugin that
runs by itself: a plugin's server runs only when Claude Code starts it for a run, or Neko starts it for a check, and only after this.

- **The digest** is `sha256:` and the SHA-256 (written in `neko-protocol`, so Neko takes no dependency for the one hash its trust
  rests on; it is checked against the published vectors and against Python's `hashlib`) of `neko-plugin-manifest-v1`, a new line, the
  manifest's canonical JSON, and for a plugin from a folder a new line, `folder:` and its path. Canonical: the fields in the order
  they are declared (never changed; a field added later is left out while empty, so digests already approved stay), maps by key, no
  spaces. The same manifest written in another order or with other spacing has the same digest, and anything that changes what runs
  or what is asked changes it (`tests/plugins.rs` pins one).
- **Approve** (`approve_plugin {plugin, digest}`) names the digest the person was shown. It is refused ("This plugin changed while you
  were looking at it") when the plugin's manifest is not that one by now, so a manifest that changes under the card is never approved
  by mistake. Approval records the digest, the manifest and the time. **Decline** records the digest declined: no more asking until the
  manifest changes (an approved plugin is turned off or removed, not declined).
- **A changed manifest** (`update_plugin_manifest` for a pasted plugin; the file for a folder's) is `changed`: it is left out of
  runs and checks until approved again, and "Needs you" asks again showing what differs from what was approved ("Runs: node server.js
  to bun server.js", "Now also passes ACME_REGION", "New setting: …"). Only the newest approval counts: putting back an older
  manifest is `changed` too, unless it is the one approved last. Putting a folder's file back as it was approved makes it approved
  again (the digest is the same).
- **A folder's file is looked at again** (without any lock held, so a slow disk holds nothing up): every few seconds while a screen is
  subscribed and some plugin comes from a folder (`neko-plugin-files`; with nothing to watch it sleeps and sets no timer), when a run
  starts, and when a check starts. A file that is gone or does not parse is a `problem`, said once; the plugin is left out.
- **What a digest does not cover.** It covers the manifest, not the files a command runs: `node ${plugin.dir}/server.js` stays approved
  when `server.js` changes. A plugin is code the person chose to run, as an MCP server added to any tool is.

## Settings and secrets

`set_plugin_setting {plugin, key, value}` checks the value against what the field declared (a choice is one of its values, a folder is
a full path, a yes/no is `yes` or `no`, text at most 2,000 characters, a secret at most 16 KB), and then:

- a plain value is kept in `plugin_values`;
- a **secret** goes to the macOS Keychain and nowhere else. `plugin_values` holds a row that says it is set. The reply and every event
  carry `set: true` and no value; the store, the event log, the daemon's log, the transcript and a run's files never hold it. Tests
  look for it in every file of a data folder after a run (`crates/neko-daemon/tests/plugins.rs`).

**The Keychain route.** A generic password, service `dev.neko.plugins`, account `<plugin id>/<setting key>`, reached through the
`security-framework` crate (MIT or Apache-2.0; with its three small dependencies it is in `THIRD_PARTY_LICENSES/crates.md` and
`NOTICE`), which calls the system's own API. The other way, the `security` command, would put the secret in its arguments, where any
process of the same user can read it while the command runs. The account holds the plugin's id, not its name, so a rename never
orphans an item.

- Everything goes through the `Secrets` trait. Tests use `MemorySecrets`; the daemon tests and the QA rig start a *debug* daemon with
  `NEKO_SECRETS_FILE=<path>` and get `FileSecrets` (one JSON file, mode 0600). A release build never reads that variable, so an
  installed Neko keeps a secret nowhere but the Keychain. No test or scenario touches the login keychain.
- A Keychain call can wait on the person (a prompt, a locked keychain). None is made on a connection's thread or under the core's lock:
  each runs on a thread with a 20-second deadline and notices a stop ("The Keychain did not answer in time. If macOS is asking whether
  Neko may use it, allow that and try again.").
- A paste's secrets are written after its plugins are added; if one cannot be, nothing is left half done (the others are deleted and the
  plugins removed). Removing a plugin deletes its secrets first and is refused, keeping the plugin, if the Keychain will not: no secret
  is left behind without a plugin.
- Not verified against a real Keychain (see the end).

## How a run gets its plugins

When a task's run starts (`runs.rs`, after the start checkpoint and the cat's rules are ready), the daemon:

1. looks at every folder plugin's file again;
2. takes the plugins that are **approved exactly as they are now, on, set up, and open to the task's cat** (`Plugin::runs_for`);
3. reads each one's secrets from the Keychain; a plugin whose secret cannot be read is left out with a warning notice in the task
   ("acme-docs was left out of this run. The Keychain no longer has Acme token. Set it again in Settings.");
4. writes one config for this run: `<data folder>/plugins-run/<run id>/mcp.json`, the folder 0700 and the file 0600. Neko's own server
   `neko` (`neko-daemon --mcp`, `subagents.md`) is first in it, in every run, with no secret in it:

```json
{"mcpServers": {"neko": {"type": "stdio", "command": "/Applications/Neko.app/Contents/MacOS/neko-daemon", "args": ["--mcp"]},
                "acme-docs": {"type": "stdio", "command": "node", "args": ["server.js"], "env": {"ACME_TOKEN": "…"}},
                "web": {"type": "http", "url": "https://example.com/mcp", "headers": {"X-Key": "…"}}}}
```

   Every `${settings.…}` and `${plugin.dir}` is filled in; secrets are in `env` and `headers` here, for this run, and nowhere else;
5. starts Claude Code with `--mcp-config <that file>` and tells the run's book the plugins' names.

The file and its folder are removed when the run function returns, however the run ends (done, stopped, failed, interrupted by a
quit, a failed start); a daemon that was killed leaves one behind, and the next one sweeps `plugins-run/` at start. A run whose cat has
no plugin gets a file with `neko` alone.

**`--strict-mcp-config`, unless the person asks for their own servers.** Claude Code reads MCP servers from the person's own settings as
well as from `--mcp-config`. The plugin host first left that alone ("add to what Claude Code does, take nothing away"). The probe of
2026-11-27 measured what it costs: one account's own settings held about 25 servers and 480 tools, which every run loaded, which slows
a run and gives it tools Neko did not choose. So each run is started with `--strict-mcp-config` and has exactly Neko's own server and
the plugins the person approved. Settings ▸ Plugins has "Also give runs my own Claude Code MCP servers" (`Settings.own_mcp`, off by
default) for a person who wants the old behaviour; then a server of theirs with a plugin's name is Claude Code's to resolve, and
`neko` is a name no plugin may take. Plugins and the person's servers show their names in a permission request (below).

A plugin reached by a web address goes into the config as `type` `http` (or `sse`), `url` and `headers`.

## Permissions for a plugin's tools

A plugin's tool is `mcp__<plugin>__<tool>`. It goes through the same route as every tool (`permissions.md`): `stream::action` makes it
`Action::Mcp {server, tool, input}` and the policy decides.

- **Following Claude Code's own rules**: Claude Code asks (its default for an MCP tool, unless the person's own rules allow it), the
  request arrives as `can_use_tool` and Neko asks in "Needs you".
- **Ask before risky steps**: the hook sees every tool call; an MCP tool asks.
- **Full access**: it goes ahead, as everything does.

The request reads "Use search from acme-docs", why: "acme-docs is a plugin you approved in Neko. Its tools can reach whatever the
plugin can." (for a server that is not one of this run's plugins: "… is an MCP server from your own Claude Code settings …"), and the
call's input as compact JSON, shortened. "Always allow" is scoped to that tool of that server for the rest of the task
(`tool:mcp__acme-docs__search`); another tool of the same plugin still asks. A plugin never asks less than another tool, and nothing
in a manifest, a setting or a plugin's own answer changes a permission, a flag or the hook.

## The check

`test_plugin {plugin}` starts the plugin's program and asks it who it is. Only a plugin the person approved, exactly as it is now, is run
(a folder's file is looked at first).

- Under the run guard (`run-guard.md`): the program is a child of a guardian in a process group of its own, every descendant is
  recorded, and when the check ends, or the daemon ends however it ends, the guardian kills them all. The test with a server that hangs
  and starts a grandchild checks both are gone.
- The program is given `PATH` (this process's, the common folders and the login shell's), `HOME`, `USER`, `LANG`, `TMPDIR`, and the
  manifest's `env` with its settings put in. Nothing else of the daemon's environment.
- It speaks JSON-RPC 2.0, one message per line, on stdin and stdout: `initialize` (protocol `2025-06-18`; the server answers with the
  one it speaks), the `initialized` notification, then `tools/list`, every page up to 500 tools. A server's `ping` is answered and its
  other requests are refused. Lines that are not JSON (a banner) are skipped; if it never answers, the last one is quoted. A server
  with no `tools` capability, or one that answers `tools/list` with "method not found", has no tools.
- **The deadline is 10 seconds** for all of it (`NEKO_PLUGIN_CHECK_MS` shortens it in tests). Errors are in words: "Neko could not
  start node. Is it installed? (…)", "server stopped before it answered (exit code 3). It said: “fatal: …”.", "server did not answer within 10
  seconds.", "server refused to start MCP: Bad token", a banner that is not MCP.
- **Off the connection.** The request is answered on a thread of its own and the reply is sent when the check ends, so the connection,
  every other request and a stop never wait on a plugin. A stop during a hanging check stops the daemon within a second and the
  server dies with it (tested). A second check of one plugin while the first runs is refused (`busy`).
- The result (`Check`: when, ok, the server's own name and version, the tool names, or the error) is the plugin's `last_check`, pushed
  as `plugin_updated`.
- **Web servers are not checked.** A plugin with a `url` is handed to Claude Code as it is and `test_plugin` says so: a check would
  need a TLS client, and Neko takes no such dependency for this. Streamable HTTP can be added when that is worth it.

## In "Needs you"

A plugin `waiting` or `changed` is a "Needs you" item (`needs-you.md`): `Need::Plugin {ask}` with the plugin's name, version, the exact
command line (or address), the names of what it is handed, where it came from, and for a change the lines that differ. It belongs to no
task or workspace or cat: its `task`, `workspace`, `cat` and `cat_name` are empty and its id is `plugin:<id>`. How long it has waited
is since the manifest became what it is now. Approve and Decline are `approve_plugin` (naming the digest on the card) and
`decline_plugin`.

## In the app

Settings ▸ Plugins (⌘, then the sixth page): the plugins with their state in words, Add MCP server… (a sheet that reads the paste live with
`parse_mcp_json` and shows names only, never a value) and Add plugin folder…, and per plugin its switch, its declared settings as fields (a
secret is a masked field and then "Set" with Change and Remove, never shown again), which cats may use it, Test with its result, and Remove
(asks first). "Needs you" shows a plugin as a plug (no cat) in a group of its own, with the exact command, what it is handed and Approve and
Decline, no default key and no button on its notification. A cat's page lists the plugins it may use, and a plugin's tool step reads
"acme-docs · search". Details and keys: `app-shell.md`, `design.md`.

## Requests and events

See `protocol.md`. Requests that may wait on something outside the daemon (`add_mcp_json`, `add_plugin_folder`, `set_plugin_setting`,
`test_plugin`, `remove_plugin`) are answered on a thread of their own and their reply is sent when ready, possibly after the reply to a
request sent later. Events: `plugin_added`, `plugin_updated` (whole), `plugin_removed`; each also marks "Needs you" for a new look.
Store events (`plugin_added`, `plugin_approved`, `plugin_declined`, `plugin_changed`, `plugin_enabled`, `plugin_disabled`,
`plugin_access`, `plugin_setting` (the key and whether it is set, never a value), `plugin_checked`, `plugin_problem`,
`plugin_removed`) carry names and digests only.

## The store

Migration 8 (`store.md`): `plugins` (the manifest and its digest, the digest, manifest and time approved, the digest declined, the
switch, the access, the last check, a problem, when the manifest became what it is) and `plugin_values` (per setting: its value, or just
that a secret is set; removed with the plugin).

## The fakes

`neko-fake-mcp` (`crates/neko-core/src/bin/neko-fake-mcp.rs`, built beside the daemon, never shipped) answers `initialize` and
`tools/list` from a script and can hang, crash, speak garbage, refuse or never list (`--mode`), page its tools, write its pid and a
grandchild's, and say whether an environment variable reached it. `neko-fake-claude` takes `--mcp-config`, appends the file's content to
`NEKO_FAKE_RECORD` as soon as it starts (Neko removes the file when the run ends, so a test reads what the run was given there), and
plays `[fake:plugin-tool:SERVER:TOOL]`: it calls `mcp__SERVER__TOOL` through the permission route and says whether it ran; when the
config did not give a server called SERVER there is no such tool and the call fails at once, asking nothing.

## Tests

- `crates/neko-protocol/tests/plugins.rs`: the manifest file and every way it can be wrong, the digest (stable, changing, pinned), the
  lines of what changed, a pasted block (several servers, secrets set apart, notes), one server alone, the bare map, every bad paste,
  what a screen shows follows from the plugin, the pinned JSON.
- `crates/neko-core/tests/plugins.rs`: a new plugin waits and never reaches a run; the digest that gates a run; decline; Needs you shows
  exactly what will run and what changed; which cats; names and an all-or-nothing paste; settings checked against what was declared; the
  per-run config; a folder's file changing, going, not parsing; the migration from store 7; the check against the fake server (healthy,
  paged, no list, environment, hangs and what it started is gone, crash, garbage, refusal, a slow list, a program that is not there).
- `crates/neko-core/src/policy.rs`, `claude/mod.rs`, `claude/stream.rs`: a plugin's tool asks by name and "always" is that tool alone;
  `--mcp-config` is added and nothing else changes; `--strict-mcp-config` goes with it unless the person's own servers are asked for; a tool's name is read as a server's.
- `crates/neko-daemon/tests/plugins.rs`, with the real daemon, the fake CLI and the fake server: adding runs nothing and waits in "Needs
  you"; a folder; a secret is in the test store and in no file of the data folder or the daemon's log; a check lists tools and only an
  approved plugin is run; a check that fails says why (crash, garbage, refusal, hang, a missing program, a web server); a stop during a
  hanging check stops within a second and leaves nothing running while another connection is answered at once; a run gets its config
  (Neko's own server and the plugin, the secret in `env`, `--strict-mcp-config`), its tool asks by name, and the file and folder are gone after;
  the same in "Ask before risky steps" (allow, deny, always, and full access); a changed manifest is left out and asks again; a folder's
  file changing is noticed by a watching screen; which cats; a run with no plugins has a config with `neko` alone; removing deletes the secrets;
  a killed daemon's config is swept.

## What is not here yet

- **Other kinds:** sandboxed scripts, views, plugins written by cats, a gallery or registry. The manifest has `kind` and a plugin
  folder format so they fit later.
- Checking a server reached by a web address (Streamable HTTP, OAuth).
- A code digest for the files a command runs.
- Per-tool allow lists inside a plugin (a plugin's tools ask like any other tool; a cat may or may not have the plugin at all).
- Updating a plugin from somewhere; there is no network in the host.

## Still not sure: check with a real Claude Code and a real server

None of this ran against the real thing: no test or scene starts the real `claude`, a real MCP server or the login keychain.

1. ~~That Claude Code 2.1.331 reads `--mcp-config <file>` of this shape.~~ Confirmed by the 2026-11-27 probe for `stdio` with `--strict-mcp-config`
   (the server `connected`, its tools `mcp__<name>__<tool>`, a missing program shown as status `failed` with no message in `mcp_server_errors`).
   Not yet for `http` and `sse`.
2. That a tool of such a server arrives as `can_use_tool` (and in the hook) named `mcp__<name>__<tool>` with the name unchanged (plugin
   names use only the characters Claude Code keeps).
3. That an MCP tool's `tool_result` and a denied call read like any other (the transcript shows them as tool steps).
4. That the Keychain call from the daemon works as the installed, signed Neko without a prompt, and what the prompt says after a rebuild
   (an unsigned development build may be asked about every time).
5. The real servers a person will add (`npx`, `uvx`, Docker): that the PATH the check gives them finds their runtime, and that the
   handshake and `tools/list` pages work with them.
