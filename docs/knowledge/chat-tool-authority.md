## Chat tool authority

**Full power is the default since 2026-11-06** (`neko_chat::super_agent`,
setting `neko_super_agent_v1`): the turn is writable, starts in the home folder
when unscoped, its bridge tools need no per-call approval, and it manages Neko
through `neko-daemon --request`. Everything below describes the read-only
mode, which is what Settings → AI & agents → Full power off gives.

Chat MCP tools use the selected workspace's lease and current connection catalog;
unscoped chat has no bridge. Server `readOnlyHint` participates in schema identity
and is supplied by the server, not a security guarantee. Unknown/action tools require
one inline approval per exact call. See `docs/architecture.md` and
`crates/neko-daemon/src/mcp_host/chat_tests.rs`; `node scripts/smoke-workbench.mjs`
exercises actual chat-worker/bridge IPC with approval, denial, Stop and restart.
Filesystem read privacy is still not provided by the Codex sandbox.

Profile ownership, scheduled planning, and bounded follow-up learning now extend
this boundary. Profile revisions fence both capability issuance and result
commit under the database lock. Learning proposals require explicit acceptance;
shared profile memory never shares tool authority. `docs/architecture.md`'s final
two sections and `docs/evidence/memory-learning.md` describe these current seams.
Agent CLI runs keep the user's full setup (config, plugins, skills, sub-agents,
web, MCP servers); only tool-free classifier runs are isolated. See the
"Agents run with everything" paragraph near the top before narrowing this.
