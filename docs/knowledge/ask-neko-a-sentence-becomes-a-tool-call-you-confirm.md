## Ask neko: a sentence becomes a tool call you confirm

L6 of `docs/plan-agent-control-plane.md`, and the payoff. Every layer beneath
made a capability reachable by *name* — find the schedule, then pause it.
This one lets you say what you want and turns it into one of those same
calls. **Read `neko_core::ask`'s module comment before touching it.**

**It works because the Keychain OAuth token `neko_core::usage` already reads
also drives `api.anthropic.com/v1/messages` with tools** — confirmed live
before a line was written, and the make-or-break unknown for the whole layer.
`anthropic-beta: oauth-2025-04-20` plus Claude Code's own system-prompt line
is what makes an OAuth credential acceptable there. `usage::claude_access_token`
is now the single reader, so there is one place that knows where this token
lives and one to audit. Stdin, never `argv`, like everything else here.

### The two rules, and how each is enforced

**It proposes; you confirm.** Planning and running are separate keystrokes,
always, with the exact call rendered between them. That needed one additive
protocol field — `SearchItem::keeps_open` — which routes Enter through the
path `⌘K` menu actions already take (`perform_activation`'s
`hide_on_success: false`), so the panel stays put *and* re-searches, which is
exactly what turns the invitation row into the proposal row. No new machinery.

**It can only reach what neko already exposes.** `ask::CATALOG` is a fixed,
compiled-in list of eight verbs and `Plan::from_tool_use` refuses anything
outside it, so a hallucinated or drifted name never reaches `McpClient::call`
— the worst a confused model can do is produce a row that will not run.
Pinned by a test naming `browser_evaluate`, which is a real Paseo tool that
runs arbitrary JavaScript over the same channel and must never be reachable
from here.

Three smaller decisions that carry weight:

- **The summary is written by neko, never by the model** (`ask::describe`),
  built from the arguments so it cannot describe a different call from the
  one beside it. A model-written summary would break the only thing making
  confirmation meaningful.
- **`tool_choice` is `auto`, not `any`.** Forcing a tool turns "none of these
  can do that" into a confidently wrong call. Verified: *"what is the capital
  of France"* comes back as a sentence declining to act.
- **Reads are absent from the catalog.** The palette answers "show me my
  schedules" faster than a model round trip, and routing it through one would
  be a slow, expensive way to reach a mode one keystroke away.

**`search` never plans.** It runs per keystroke; planning happens only on a
deliberate Enter and `search` renders whatever that left behind. A failed
plan is remembered as the row's own text rather than returned as an error —
in the footer it would leave the row still saying "Plan it", as though
nothing had happened. The plan is cleared after any run, so Enter twice
cannot fire it twice, and a plan whose question has changed underneath it
refuses as stale rather than running against the wrong world.

**Context is gathered, not looked up.** The live agent and schedule ids go
into the prompt, because a lookup tool would be a second round trip and a
second chance to be wrong for information neko can fetch in milliseconds.

Verified end to end over the real wire: `"ask neko"` reaches the command;
`"open a terminal in /tmp"` planned to `create_terminal(cwd: /tmp)` with
`keeps_open` flipping `true` → `false` between the two steps; Enter ran it and
a real terminal appeared in `list_terminals`; the plan cleared itself
afterwards. The probe terminal was killed.
