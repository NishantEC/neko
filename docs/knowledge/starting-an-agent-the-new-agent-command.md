## Starting an agent: the New Agent command

`fm/neko-new-agent`, closing the gap the section above listed as not built.
`neko_core::new_agent` is the write half of `agents.rs`'s read half — **read
that module's own doc comment before touching this area**; it is the normative
statement of everything summarised here.

**The shape cost exactly what `modes.rs`'s accounting promised, plus one wire
field.** One `CommandSpec` (`New Agent`), one `ModeChrome` (`new-agent`,
`has_detail: false`), one `Provider` registered in `AppState::mode_providers`
(not the root list — a directory to start an agent in is not an answer to a
root query). No change to `enter_mode`/`exit_mode`/`run_search`'s scoping
branch, and no new `Request`/`Response` variant.

**The one protocol touch, and why the two cheaper alternatives are wrong.**
`Request::Activate` gained `query: String` — the search field's contents at the
moment Enter was pressed — and `Provider::activate_with_query` is the defaulted
trait method that receives it (delegating to `activate`, exactly the
arrangement `search_cancellable` has with `search`, so every other provider
implements nothing). It exists because this is the first row whose action takes
an **argument**: the row is a working directory, the query is the prompt.
Both alternatives were considered and are silently wrong:
- **Fold the prompt into `SearchItem::id`.** The id then changes on every
  keystroke, and `panel::resolve_selection` follows the highlight by
  `(kind, id)` — so a captain who picks a project and types one more word is
  returned to the first row, and Enter starts the agent in the wrong
  repository. Silent, and it produces a *wrong action* rather than an error.
- **Let the provider remember the last query it was searched with.** That races
  the daemon's own documented out-of-order request completion
  (`handle_connection`), which can leave a stale prompt behind — same class of
  failure.

**Two things are chosen, never inferred, and both are shown on the row.**
- **Where.** Rows are the projects Paseo already knows about,
  `~/.paseo/projects/projects.json` (8 real entries, one per root, ordered by
  recency) — deliberately **not** `workspaces.json` (200 entries, 21 live, the
  same directory repeated up to four times plus transient worktrees). The
  daemon's own `cwd` is meaningless (it inherits whatever started it, often
  `/`), `$HOME` is worse than nothing, and a hidden preference would put the
  most consequential input behind a settings window.
- **Which tool.** `paseo run` **requires** `--provider` — verified live
  (`MISSING_PROVIDER`), with no default anywhere in `~/.paseo/config.json`. So
  the provider is the one the most recent real agent used *in that directory*
  (`agents::provider_usage`, which reads closed agents too — 172 of 227 on this
  machine), falling back to the most recent anywhere. A machine with no agent
  history cannot be answered honestly: the row says "Start one from Paseo
  first" and Enter refuses, rather than this app deciding which coding tool
  somebody uses.

**The query is the prompt, not a filter — the only provider here where typing
does not narrow the list.** Every project is returned for every query, in the
same order, which is also what keeps the row ids stable. Known gap, stated
rather than hidden: there is no way to reach a directory Paseo has never seen.

**`--cwd` is not authoritative when the CLI can see it was launched from
inside another agent — found live, and it is a real production defect, not a
test artifact.** The one verification spawn ran with `--cwd /tmp/…` and
produced an agent whose real `Cwd` was `/Users/example/Documents/neko`: the
inherited `PASEO_AGENT_CWD`. The CLI's own bundled source states the rule
(`resolveRunWorkspace` in `app.asar`): workspace precedence is `--workspace`,
then `$PASEO_AGENT_ID` ("daemon resolves the caller's workspace"), then
`$PASEO_WORKSPACE_ID` ("exported by workspace terminals"), and only then a
workspace minted for `cwd`. `--cwd` *is* passed in the agent-scoped case and
still loses, because the daemon resolves it server-side from `callerAgentId`.
`neko-daemon` inherits the environment of whatever started it, so a captain
running neko from an agent session or a Paseo workspace terminal would have hit
this on every spawn. The child's environment is now stripped of all three
variables (`new_agent::AGENT_SCOPING_ENV`), which drops the CLI to the
mint-a-workspace-for-cwd case. **Not re-verified live** — that would have cost
a second real agent, and the brief allowed one.

**Failure is inline and never silent.** Activation waits for the CLI (bounded
at `CONFIRM_TIMEOUT`, 20s) and returns `Response::Error`, which
`panel::Root::activation_error` renders in the footer with the panel left
open. Three findings shaped that:
- **The CLI reports errors as JSON on `stdout`, not `stderr`**, exit code 1 —
  reading stderr alone produced a useless "exited with status 1".
  `json_error_message` reads `error.message` + `error.details`.
- **A timeout claims neither outcome** ("may still be starting; check Paseo")
  and, unlike `files.rs`'s `mdfind`, **does not kill the child** — the agent
  may already exist, and killing it while reporting failure is the worse lie.
- **It blocks its own request thread for the CLI's duration** (~1s of Electron
  boot; the one measured end-to-end run took **2.88s**). That is not the daemon
  blocking — `handle_connection` gives every request its own thread, and
  nothing here touches the `Db` mutex or the reader loop. The cost buys the
  honest answer. A "starting…" tell in the panel, or Paseo's own local RPC port
  (private and undocumented, deliberately not reverse-engineered), are the two
  ways to remove it later.

**Verification.** Hermetic tests only, plus **one** real agent, created once and
immediately archived and deleted (agent count 31 → 32 → 31, temp directory
removed). The spawn seam is injected (`new_agent::AgentSpawner`, the same shape
as `agents::AgentsProvider::with_root` and `panel::AppearanceSetter`) — there is
no env var and no dry-run flag, so the only way to reach the real CLI is to hold
a `PaseoCli`, and no test does. The live run also proved the read half against
the captain's own machine: 8 real project rows, each naming the tool actually
used there. **Not verified**: anything on screen (no client launch, no
screenshot), and the env-stripping fix above.
