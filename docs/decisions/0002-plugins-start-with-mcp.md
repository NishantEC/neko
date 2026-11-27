# 0002: Plugins start with MCP, and the person approves digests

Date: 2026-11-27. Status: accepted.

## Decision

Neko is meant to be built from plugins. The first slice builds the plugin host and one kind of plugin, an MCP server, and ships no
plugin at all. A plugin runs only after the person approved exactly its manifest, by the manifest's digest; a manifest that changed
is asked about again. A plugin's tools ask through Neko's own permission route like every other tool, never around it.

## Why MCP first

- It is a standard that Claude Code, Claude Desktop, Cursor and other tools already speak, with the same `mcpServers` JSON
  (`command`, `args`, `env`, or a `url`). People already have servers configured: pasting that JSON is the whole of adding one, and
  nothing about Neko's own agent has to change for a plugin to reach it. Claude Code is told about the servers with `--mcp-config`.
- Claude Code starts and speaks to the server itself. Neko needs a client only to check one (who it is, which tools), a few hundred
  lines, not a plugin runtime.
- It leaves the other kinds open without building them: a manifest has a `kind`, a plugin folder has a format, and a sandboxed
  script, a view or a plugin written by a cat is a later kind with a later decision.

## Why approvals are digests

- A plugin is code that runs on the person's Mac with the person's secrets. The only honest trust root is the person: no registry
  vouches for it, no install script ever runs, and nothing runs because it was added.
- The approval is of *exactly this manifest*: a SHA-256 of its canonical form (and of the folder it came from). Anything that
  changes what runs or what is handed to it changes the digest, so an edit, or a file in a plugin's folder that changed, makes the
  plugin `changed`: left out of every run, asked about again, with the lines that differ. Approving names the digest the person was
  shown, so a manifest that changes under the card is never approved by mistake.
- A digest covers the manifest, not the files a command runs. That is stated plainly (`plugins.md`) rather than papered over.

## Why secrets are taken out of manifests and kept in the Keychain

A pasted server's `env` and `headers` hold tokens. They become secret settings at once: the manifest (and so the digest, the store,
the events) holds only `${settings.name}`, and the value is a Keychain item. The value is in a file only for the length of a run that
uses it: a config of that run's own, mode 0600, removed when the run ends. A screen is told whether a secret is set, never what it is.

## Why no plugin is bundled

- Nothing the owner has not chosen runs on a Mac. A bundled plugin would be a plugin that was chosen for them.
- A host with no plugin is tested with fakes (`neko-fake-mcp` beside `neko-fake-claude`), so the tests never depend on a real server
  or the network, and the host is proved by what it refuses as much as by what it runs.
- A gallery, if there is ever one, is a later decision with its own trust model; this one does not need it.

## What this does not decide

Other kinds of plugin, a registry or gallery, sandboxing, views, plugins written by Neko's cats, and checking servers reached by a
web address. Each is later, and none changes the approval model above.

## Consequences

- Neko has a second place a person can add something that runs, so "Needs you" gains a kind that belongs to no task.
- Every plugin tool goes through the same permission modes as any tool, so "Ask before risky steps" asks about a plugin's tools too.
- ~~Claude Code keeps the person's own MCP servers.~~ **Changed 2026-11-27 (subagents):** every run is passed `--strict-mcp-config`, so a run has
  only Neko's own server and the plugins the person approved; Settings ▸ Plugins can give the person's own servers back. Why: the real
  CLI measured about 480 tools from one person's own servers in every run, which slows runs and is not what Neko configured; Neko's own
  tool for subagents must be one of very few tools for the agent to use it reliably. The default of "no surprises" now means "the
  tools the app shows". See `docs/knowledge/subagents.md`.
