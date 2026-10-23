## Reading an agent's conversation inside neko

**The tiles bypassed this for a day, and the shape of the bug is the part
worth keeping.** `confirm`'s agent-*tile* branch (and the tile's click
handler, separately) built a bare `Request::Activate`, never checking
`enters_mode` — while at rest `split_agent_tiles` moves every live and recent
agent out of the rows and into the grid, making tiles the only agent surface
most summons ever show. So the conversation view worked, was fully wired on
the wire, and was unreachable from exactly the place agents are visible:
Enter on a tile opened Paseo. Four call sites doing Enter's arithmetic
separately is the same shape that split `⌘K` from its hint; everything Enter
means once the item is known now lives in one `act_on_item`, shared by rows,
tiles, and both click handlers, with both directions pinned (a tile with
`enters_mode` enters the mode carrying its subject; one without still
activates).

**It is a chat now, and the data source reversed to make that possible.**
The first conversation view read Paseo's `get_agent_activity` and rendered its
entries as a list with a detail pane. That source was right about size and
wrong about shape: **the activity feed has no user turns** (verified live —
659 activities on a real agent, all of them the agent's own voice), and a
conversation with one speaker is not a conversation. Both speakers exist on
disk: a claude agent's Paseo document carries `persistence.sessionId` and
`cwd`, which together address Claude Code's own session transcript —
`~/.claude/projects/<munged-cwd>/<sessionId>.jsonl`, chronological, with real
roles. `conversation.rs` reads the **tail** (`TAIL_BYTES`, sized by
measurement: 3 MB of a real 32 MB session held only 3 user turns, because
tool results and pasted images make single lines enormous) and keeps the last
`TURN_LIMIT` turns. The extraction rules are the load-bearing part and are
pinned: a `user` line carrying a `tool_result` block is a tool answer, not
the captain speaking; sidechain and meta lines belong to subagents and the
harness; an image block leaves an `[image]` marker rather than vanishing; the
cwd munge (`[^A-Za-z0-9]` → `-`) is verified against the real directory
listing, not inferred. The activity feed stays as the fallback for non-claude
agents — one voice is still better than an empty pane.

**The layout is `ModeChrome::transcript`** — a third layout after plain-list
and list+detail, and the panel branches on chrome, never on the provider id.
User turns are right-aligned bubbles (max 480px — full-width bubbles read as
banners), agent turns are full-width markdown, tool calls are one-line chips
(badge + monospace argument). `SearchItem::speaker` (`"user"`/`"agent"`/
`"tool"`, `#[serde(default)]`) is the wire vocabulary; rows arrive oldest
first and the mode **enters at the bottom**, selection on the newest turn,
gated exactly like the theme mode's landing so a later frame of the same
conversation never fights a captain who scrolled up to reread (pinned).
Selection stays a subtle wash — a transcript is read far more than driven —
and `⌘K`'s "Open in Paseo" still needs to know which turn is meant.

**One layout bug worth keeping: everything overflowed the right edge on the
first capture.** The transcript sits in `render_mode_content`'s flex *row*,
where flexbox's `min-width: auto` sizes a child by its content — and a
paragraph's content width is the unwrapped line. `min_w(0)` on the wrapper is
what makes text wrap instead of escape; parse tests could never have seen it.

**The composer — the mode is a chat you can talk in now, not just read.**
The panel *inverts* for a transcript mode: exchange on top, the same input
row at the bottom, where a field whose Enter sends belongs — same `TextField`
entity either way, so focus and editing carry over; only child order changes.
Typing drafts (the scoped search still sends the subject, so keystrokes never
touch the list); Enter sends the draft to the **subject** via
`Request::Activate { kind: "conversation", id: subject, query }` —
`transcript_send_request` is its own method so the test asserts the id is the
agent and never the selected turn, which would quietly no-op. The field
clears on send like every messaging surface; an empty draft swallows Enter
rather than acting on a turn. `ConversationProvider::activate_with_query`
routes by id shape (`agent#rank` stays a read-only no-op) and **invalidates
the cache on send**, so the sent bubble appears on the next frame.
`CACHE_TTL` dropped 10s → 2s, and `enter_mode_about` spawns a 2.5s poll loop
that dies with the mode (checked per tick against the exact mode+subject —
no cancellation channel needed), so replies stream in. **Being at the bottom
is sticky** (reading the newest turn follows new turns down, pinned) while
reading an older turn is never disturbed (also pinned).
`docs/evidence/conversation-composer.png` — bubbles, GREP/TASK chips, and
the composer carrying a typed draft with its `Send ↵` hint.

**The header, the typing bubble, tool output on demand, and subagent runs —
the "everything out" pass.** The transcript owns a session header now: back
arrow, the agent's name and workspace **from the row that opened the mode**
(`ActiveMode::subject_item` — the provider already composed those; deriving
them client-side would be a second copy of `agents.rs`'s naming rules), and a
working/idle dot on the right. Liveness is a **row**: the provider appends a
`speaker: "working"` row while the doc's `lastStatus` is running, so the
header dot, the typing bubble (three dots breathing on the shared
`PulseClock` — never their own animation) and the poll can never disagree
about whether the agent is busy. Verified live: the fixture doc was flipped
to running *after* the mode was entered, and the header and bubble appeared
on the next poll tick (`docs/evidence/conversation-header-working.png`).

Tool chips fold open on click: `parse_transcript_tail` joins each
`tool_result` back to its call by `tool_use_id` (bounded at
`RESULT_LIMIT_CHARS` — a chat where every `ls` prints unasked is a
terminal), the row's `preview` carries it, and a `show output` hint appears
only where there is genuinely something to disclose. Contiguous **sidechain**
traffic collapses to one `SUBAGENT · N steps` chip in its place — only the
subagent's own assistant events count (its incoming tool results would
double every step), and per-step attribution via the uuid chains is
deliberately not guessed at.

**Inline images are built.** The jsonl carries them as base64 — one real
screenshot is ~138 KB of it — so the daemon decodes once to
`~/Library/Caches/neko/conversation-images/<generation>/<hash>.<ext>` and the
wire carries a **path**, never the bytes: `SearchItem::images`, the same
arrangement `Icon::Image` already uses for app artwork. Three properties are
load-bearing and each is pinned. **Keyed by a hash of the payload and skipped
when the file exists**, because the client re-parses the tail every 2.5s and
would otherwise re-decode every image in the window several times a minute.
**The `[image]` marker survives anything that does not decode** (a URL
source, a full disk) — a turn pointing at nothing is worse than a marker.
**A turn that is only a picture is still a turn**, since dropping it for
having no words would delete the message. On disk the cache is bounded
(`MAX_CACHED_IMAGES`, pruned oldest-first, and only when a genuinely new file
is written); in memory it gets a **second** `RowIconCache` instance
(`for_conversation`, capacity 24) rather than sharing the row cache — one
chat image is hundreds of times a 22px icon's decoded area, and sharing would
let a screenshot evict a screenful of row artwork or the reverse.
`docs/evidence/conversation-image.png`.

**Deferred, with its reason**: Codex transcripts (`~/.codex/sessions` exists and its rollout format is
readable, but **no Paseo codex agent exists on this machine**, so the
`persistence.sessionId` → rollout-file join would be unverifiable code — the
exact bar `usage` applied to Cursor/Kimi/MiniMax/Z.AI).

**`⌘↵` is the jump to Paseo — the old Enter, one keystroke away.** Bound at
the `"Panel"` context, resolved by **data, never by provider id**: whatever
is highlighted must actually carry an `open-in-paseo` action (an agent row or
tile does, an app row does not, and ⌘↵ on the latter means nothing rather
than guessing), and inside a transcript the target is the mode's *subject*,
whatever turn the selection sits on — both pinned. The header carries the
clickable form, labelled with its key
(`docs/evidence/conversation-open-in-paseo.png`), and hides on success: the
whole point of the keystroke is that the interaction continues in Paseo's
window.

**"⌘↵ doesn't work" was two things, and neither was the binding.** Diagnosed
by driving gpui's own keystroke→binding→action dispatch **in process**
(`cmd_enter_dispatches_through_the_real_binding_to_the_handler` — the first
real dispatch test in this repo; `TestAppContext::simulate_keystrokes` is
in-process machinery, not the synthetic OS input this repo forbids). Two
harness rules it learned the hard way, kept in its own comments: **dispatch
reads the rendered frame's tree**, so the window must draw between mutating
the view and simulating (`cx.refresh()` + `run_until_parked`); and
**`simulate_keystrokes` runs the executor until parked**, so by assertion
time the activation has already *completed* against the harness's dead
socket — the proof of dispatch is the failed request's own
`activation_error`, never the in-flight `activating` flag, which a first
version asserted on and wrongly concluded dispatch was broken. The
re-entrancy trap on the same road: dispatching from inside
`window.update(cx, |root, ..|)` panics ("already being updated") because the
listener updates `Root` — but that panic *is* proof the handler ran.

What was actually wrong for the captain: **the likeliest press is the silent
one** — a fresh summon shows agents in the grid while the selection starts
on the top *row*, so ⌘↵ resolved no target and did nothing, which reads as
the key being broken. It answers inline now ("select an agent to open in
Paseo"). The daemon half was proven separately over the live wire
(`Activate {kind:"conversation", action:"open-in-paseo"}` → `Activated`,
Paseo genuinely opened).

**Verification is seeded files, not hooks**: the isolated evidence `HOME`
carries a real-shaped agent document plus a real-shaped session jsonl, so the
entire pipeline — doc resolution, munge, tail read, turn extraction,
rendering — runs for real on fixture data
(`docs/evidence/conversation-chat-transcript.png`). Two environment facts
from getting that capture: a typed query excludes *idle* agents unless the
`agents-include-idle` preference is on (flipped over the wire in the evidence
run), and a *running* fixture agent gets lifted into the grid during a search,
where `NEKO_SHOW_CONFIRM`'s Enter cannot reach it.

An agent row's Enter used to open Paseo. That is right when you want to
*work* with an agent and wrong when you only want to know what it has been
doing — switching apps to read three lines is most of the cost of not having
asked. Enter now reads it here; `⌘K` → "Open in Paseo" is the way to the real
window, and `agents::open_agent_in_paseo` is one function so the two cannot
drift.

**`ActiveMode::subject` is the new idea, and it is small.** Every mode until
now was one provider's own *list*, so scoping the search to the provider was
the whole of "which mode am I in". A conversation is a list *about* one thing,
and the provider cannot know which agent unless it is told — so the row's id
travels into the mode and `run_search` sends it as the scoped query.
`ConversationProvider::search` therefore receives an **agent id** where a
filtering provider receives typing. The consequence is stated in that module
rather than discovered later: **typing in this mode does nothing**, because
the query slot is spoken for. Searching *within* a conversation would need a
second query channel the protocol does not have.

**Paseo's curated summary, not the raw transcript.** Claude Code writes its
own to `~/.claude/projects/<cwd>/<session>.jsonl` and this machine's is
**13 MB for one session**; `get_agent_activity` returns the same thing reduced
to `[Write] path` plus the agent's own prose, over a channel neko already
speaks. `parse_activity` drops the daemon's own "Showing 6 of 1046
activities" header — that is about the fetch, not the agent — and keeps a line
it cannot parse rather than dropping it, because a conversation view that
silently omits what the agent said is worse than one showing a line it did not
understand.

**Every mode has its own empty line now** (`ModeChrome::empty_line`). They all
rendered `NO_MATCHES` — "No matches, try fewer characters" — including the
ones where nothing had been typed: Terminals and Schedules are empty on a
machine with none, and a provider that swallows a transport error is empty
when Paseo is simply down. All of them told you to delete characters you never
typed.
