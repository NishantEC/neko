## The agent control plane: MCP, and the permission inbox

`docs/plan-agent-control-plane.md` is the plan; this is what is built. neko
used to *see* agents (scraping `~/.paseo/agents`) and start one (shelling out
to the CLI). Everything else meant switching to Paseo.

**Paseo's daemon exposes its whole agent surface as MCP over HTTP** at
`POST /mcp/agents` — **61 tools**, confirmed live. The route is exempt from
the daemon's global bearer middleware, and its own `isAgentMcpRequestAuthorized`
returns `true` outright when no daemon password is configured, which is this
machine's state. That single fact is why the plan is cheap: one client, 61
capabilities, and none of the undocumented WebSocket session routing `/usage`
bounced off.

**`neko_core::mcp` is the base and the only way anything here talks to
Paseo.** Read its module comment before touching this area. Three things
about the daemon are *observed*, each with a test:

- it answers the same endpoint as **SSE or as a plain body** depending on the
  request, so `parse_frame` accepts either;
- it reports a failed tool call as a **successful response carrying
  `isError`** — exactly the shape a caller would otherwise treat as data —
  so `tool_result` collapses that with JSON-RPC `error` into one kind;
- it issues **no session id** for a stateless client, which is what lets the
  client re-`initialize` per call and hold nothing. That is deliberate: the
  daemon is restarted constantly during development, and a client caching a
  dead session is silently wrong until something notices.

The endpoint comes from `~/.paseo/paseo.pid`'s own `listen` field and is
parsed as a `SocketAddr`, so a leftover pid file fails discovery rather than
aiming a request somewhere unexpected. Live checks are `#[ignore]`d —
`cargo test -p neko-core live -- --ignored` — because every interesting
property here belongs to Paseo, not to this code.

**The permission inbox (`neko_core::permissions`) is the feature that
justifies the plan.** An agent that hits something it may not do stops and
waits, and until now the only way to notice was to switch to Paseo. Blocked
agents now lead the root list: Enter approves, `⌘K` denies.

- **`answers_empty_root_query` is `true`**, unlike themes and preferences —
  a blocked agent is the most time-sensitive thing this app knows, it is
  self-limiting, and it is *news*, the same test `agents.rs` passes.
- **`ATTENTION_BONUS` (1000) is a deliberate exception** to "providers
  compete on `fuzzy_score`". That scale ranks an app against a file; it has
  nothing to say about "a person is waiting". A typed query still has to
  match, or the inbox would pin itself above every search result forever.
- **It always defers** (`defers_for`), gated on the daemon being reachable.
  Answering costs a `curl`, and the fast group exists so nothing touching a
  socket delays first paint. Gated rather than hard-coded `true` because a
  provider that will not make a request has nothing to defer *for* — a
  wasted second frame is a wire frame and a client render for a guaranteed
  empty result. Verified live: an empty root query is answered in **two**
  frames with Paseo up.
- **`CACHE_TTL` is 1.5s**, not the tens of seconds every other daemon-backed
  provider uses. Approving something already handled in Paseo's own window is
  the one failure this must not have; a short window is most of the defence
  and `respond_to_permission` rejecting an unknown id is the rest.
- **Deny is not marked `destructive`.** Denying is a normal answer, not a
  mis-key to guard against, and arming it behind a second Enter would make
  the safer reply the slower one.

**Agent rows carry real actions now** (`agents::agent_actions`): Cancel run
and Archive, both over MCP. Cancel is not destructive and Archive is —
cancelling is the everyday "stop, that's wrong" and putting recovery behind a
second Enter makes it slower than the mistake; archiving soft-deletes.
A closed agent is offered neither. **The read path was deliberately not moved
onto MCP**: seeing your agents should not stop working because a daemon
restarted, so reads stay on `~/.paseo/agents` and only writes need the daemon.

**`PermissionsProvider::disabled()` exists for one reason**: `AppState::new`
delegates to `with_test_providers`, which the entire daemon suite goes
through, so an always-live provider there would put a real network round trip
inside every hermetic test — and the tests address providers by index, which
is why it is appended rather than inserted at the front. Same shape as
`FileProvider::empty()` and `SettingsProvider::with_panes`.

**Verified live over the real wire**: an empty root query in two frames, an
agent row carrying `actions=[cancel,archive]`, and both write paths reaching
the daemon (its own validator answering `requireAgent: agentId must be a
UUID`) while a malformed row id is refused locally without a request.
