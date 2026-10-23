## Agents: what is running right now

`fm/neko-preferences`, from the captain's own *"I also want the feasibility
of seeing all the agents that are running on my device."*
`neko_core::agents::AgentsProvider` (id `"agent"`, section "Agents"), the
eighth provider.

**Source of truth is Paseo's own on-disk state**, `~/.paseo/agents/
<workspace>/<uuid>.json` — one plain JSON document per agent. All three
candidate sources were checked against the same machine before choosing:

| source | what it reported | verdict |
| --- | --- | --- |
| the JSON files | 2 running, 53 idle, 172 closed | **chosen** |
| `ps` (process table) | 2 live `claude` processes | agreed with the files |
| Paseo's MCP `list_agents` | **1** running | under-reported — not built on |
| the `paseo` CLI | correct, but a subprocess per search | nothing to buy; the data is a file |

A source that disagrees with the process table about what is running is not
the one to build on. `apps.rs`/`files.rs` pay a subprocess cost for `mdfind`
because Spotlight has no in-process API; here there is no such excuse.

**Every live agent on the machine was Paseo-hosted** — both running `claude`
processes had `Paseo Daemon` as their parent and **no controlling terminal at
all** (`tty: ??`). That is why the captain's own "if it's a claude session in
a Ghostty tab, focus that tab" is **not built**: there was not one real
instance to build against, and terminal-tab focusing needs Accessibility APIs
and a process→tab mapping that would have been written entirely on
speculation. `agents::Backend` is the enum a second host slots into.

**`Provider::answers_empty_root_query` returns `true` here, unlike themes and
preferences.** "What is running right now" is exactly what is worth seeing
the moment the panel opens, and it is self-limiting — two rows on a real
machine, not two hundred. Idle agents need a query, because they are context
rather than news, and they outnumber live ones roughly 25:1.

**Two settings, an Agents tab, and a `Backend` seam that exists because it
was asked for.** The captain's *"Paseo is something we use currently, maybe I
change to something else later"* is a stated requirement, not an imagined
one — so `Backend` is an enum with one variant, and adding a second costs a
variant, a read function, and one arm. It is deliberately **not** a plugin
system: each backend reads a different tool's own on-disk format, and there
is nothing generic to abstract until a second exists to compare against. The
tab shows the source as a **statement, not a picker** — a dropdown with one
entry is a promise the app cannot keep — along with a live census
(`2 running · 53 idle · read from ~/.paseo/agents`), because a source of
truth you cannot see is one you cannot debug when the list looks wrong.

**Unverified, and shipped anyway: the deep link.**
`activate` opens `paseo:/h/local/agent/<id>`. The route shape is read from
Paseo's own bundled `@getpaseo/protocol/agent-deep-link`
(`buildAgentDeepLinkRoute`); `local` as the server id is **inferred** from
`=== "local"` comparisons in the same bundle and has never been opened. If
Enter on an agent row does nothing, this is the first thing to check.

**Spawning agents is now built** — see "Starting an agent: the New Agent
command" immediately below; this section's own prediction ("`paseo run
<prompt>` would be a command row") turned out to be exactly the shape.
**Still not built**: transcript sources (`~/.codex/sessions`,
`~/.claude/projects`, `~/.grok/sessions`). That one is deliberate —
`jazzyalex/agent-sessions` prices each at ~1,000 lines in its own
`docs/adding-a-session-source.md`, and it answers "what did an agent do", not
"what is running".

**The grid above the search field.** Running agents render as tiles above the
input row rather than as rows in the list, because they answer a different
question: the list is "what did you ask for", the grid is "what is happening
without you". Two things about it are load-bearing:

- **Its height comes out of the row budget, it does not grow the panel.**
  `PANEL_HEIGHT_PX` is fixed for the process's whole lifetime and the real
  `NSWindow` is never resized ("Mode view resize seam"), so anything drawn
  above the input row is space the rows no longer have. Getting this wrong
  does not look like a layout bug — it looks like the last row being clipped
  by `overflow_hidden`, the exact defect `fit_within_budget` exists to
  prevent. `panel::Root::agent_grid_height` returns `0.0` when there is
  nothing to show, so a machine with no agents running loses no space at all,
  and a `const _: () = assert!(...)` makes a grid taller than the content
  area a **build** error rather than a runtime surprise.
- **`split_agent_tiles` moves live agents out of `results`, never copies
  them.** The same agent as both a tile and a row is two things to press
  Enter on for one agent. It keys on the `LIVE` badge the provider already
  sets rather than on `kind == "agent"`, so an idle agent stays an ordinary
  row. Never applied inside a mode — a mode is one provider's own list.

**Four tiles, 2×2, below the search field.** Running agents first, then the
most recent — running outranks recency, never the other way round, which is
the rule that is easiest to break by sorting on time alone and is pinned by a
test. The provider caps its own empty-query answer at `GRID_CAPACITY` and
carries the order as **descending scores**, because `search::allocate` ranks
by score and has no reason to know two agents with equal scores are ordered
by time. Timestamps compare as strings: Paseo writes RFC 3339, which sorts
lexicographically in chronological order, so this needs no date dependency
(the same trade `clipboard`'s relative times already made).

**Idle agents join the grid only when nothing is typed.** During a search
only live ones do — otherwise typing would silently lift idle rows out of the
list and the grid would mean two different things depending on whether you
had typed. At rest it answers "what have you been working on"; during a
search it stays out of the way.

**Below the input row, not above it — and that is an interaction fix, not a
position preference.** Above, the tiles sat outside the content area: Down
from the field went straight past them into the rows, so the grid was
unreachable going forward, and Up landed on the *last* tile first. Reaching
tile 1 meant pressing Up four times. Below, the grid is simply the first
thing in the content area, the selection starts there, and Down runs
1→2→3→4→rows with Up the exact reverse.

**A tile is a row that happens to sit in a grid.** No border and no resting
fill: transparent until selected, then the same `surface_selected` pill a row
gets. It also insets by `theme::CONTENT_INSET_PX`, the *same* token the
results container uses — the two had drifted to 20px and 8px, which put every
tile 12px inside the selected row's highlight and read as two different
widths. A test asserts the grid spans **exactly** the results width rather
than merely fitting inside it; "fits" was the old assertion and it passed
happily while looking wrong.

**Only one thing is selected at a time.** `Root::selected` is the list's own
cursor and keeps its value while the keyboard is up in the grid — correct,
and exactly why `row_is_highlighted` folds `grid_selected.is_none()` into the
answer. Without it a focused tile and the list's remembered row both painted
a highlight and the panel showed two selections at once.

**The tile shows the host app's icon with the tool badged onto it**, the shape
macOS uses for a document and its owning app. The icon is Paseo's own,
extracted through the same cache every app row uses (cached once per process
— extraction is real AppKit work at tens of ms, and this would otherwise run
per agent per keystroke). The badge is the *tool* rather than the model,
because a badge on a 22px icon has room for one glyph and "which tool" is the
distinction that survives being reduced to one; `claude` renders Anthropic's
own mark, and anything without a vendored logo falls back to its initial —
`simple-icons` has no OpenAI mark, so that fallback is a real path, not a
placeholder. The line under the title is the **workspace**, which is what
tells two agents running the same model apart.

**Paseo keeps three names per agent and they are not interchangeable.** The
**title** is the first prompt, verbatim and never updated — `"hi"` on a
session with 872 messages since, and three of this machine's agents share
`"what's the update on the agents tasks"`. The **workspace** `displayName`
is the branch or worktree the session runs in (`feat/doctors-maps`, `main`).
The **project** `displayName` is the repository
(a private client repository). Only the last two distinguish one session
from another, so a tile is **branch over repository**, and the prompt stays
in the search haystack — it is how a person remembers what they asked for,
even though it does not identify a session on screen.
`agents::read_workspace_names` does the join from `~/.paseo/projects/`
(`workspaces.json`, `projects.json`); 215 of this machine's 229 agents
resolve, and the rest fall back to the directory name, then the prompt.

**A resolved name can still say nothing, and that is a separate case from a
failed lookup.** Paseo names a `kind: "directory"` workspace after its
folder and gives it a project of the same name — no branch, no repository
slug — so both of this machine's `~/Documents/hme` agents came out `hme`
over `hme` and were indistinguishable from each other. The lookup had
succeeded. `subtitle` therefore branches three ways rather than falling
through one chain: a project that differs from the title is the line; a
project that *repeats* it hands the line to the prompt, the only field left
that separates two sessions in one directory; and nothing resolved at all
keeps the path, because the title is then already the directory's own name
and the prompt would be the second thing on the tile saying nothing. Two
pre-existing tests caught the first draft of this, which let the prompt win
the no-project case too.

**`projects/` is `agents/`'s sibling, not its child** — `~/.paseo/agents`
and `~/.paseo/projects` — so `read_workspace_names` takes the agents root
and steps up. Getting that wrong is silent: every lookup misses and every
tile falls back to its path, which reads as a deliberate design rather than
a bug. It was wrong in the first draft here and was caught by checking the
constant, not by the tests, which had no `projects/` fixture until this
found the need for one.

**`agents::title_from_prompt` still tidies that prompt** for the fallback
case and for search: one line, leading decoration off (`>`, `#`, bullets,
the `▎` a quoted block opens with), whitespace runs collapsed, and a prompt
that is *nothing but* a URL compacted — `https://github.co…` truncated raw
spends a 150px tile on the scheme and host, the two parts every such link
shares, so `compact_url` keeps the identifying tail
(`github.com/…/pull/4501`, `figma.com/design/Care-Comms`, with Figma's
22-character file key dropped as an opaque id). A prompt that merely
*mentions* a link is left alone; rewriting inside a sentence would be
rewriting what was typed.

**Brand marks are a separate vendored category from Lucide, and the tests say
so.** gpui renders an SVG to an alpha mask, so a *filled* UI icon becomes a
solid blob — but a brand mark **must** be filled, because it is a silhouette.
`every_vendored_icon_is_pinned_and_on_the_24px_grid` asserts stroke-only for
`icons/lucide/` and filled for everything else. See
`THIRD_PARTY_LICENSES/simple-icons-CC0-1.0.txt`: CC0 waives copyright and
cannot grant trademark rights, so the marks are used nominatively (naming the
tool that runs an agent) and that position needs revisiting if neko is ever
distributed.

**Tiles are keyboard-reachable, using the same Up/Down as the list.** That is
forced rather than chosen: Left and Right are bound to the search field's own
cursor movement (`main.rs`'s `cx.bind_keys`, `"TextField"` context), so a grid
claiming them would break typing to reach it. Treating the tiles as rows that
happen to sit above the input costs no new keys at all — `Root::grid_selected`
is `Some` while the selection is up in the grid, Up from the first result
walks into it (landing on the tile *nearest* the list, so the selection moves
by one visually rather than across the whole strip), and Down off the last
tile walks back out. Up at the top stays put rather than wrapping to the
bottom of the list, which would read as the selection teleporting. A focused
tile owns Enter, is cleared by `reset_for_summon` like every other
per-summon state, and is **clamped** when a response arrives with fewer tiles
— an agent finishing between two responses must not leave the selection
pointing at nothing.
