## The Agents mode, and what the control plane costs

**The Agents mode** (`agents::AgentControlProvider`, id `"agent-control"`) is
every agent with the keyboard pointed at what you came to do: say something
else to it. Enter sends the search field's contents as a prompt; `⌘K` offers
the agent's own session modes plus Cancel and Archive.

It is a **separate provider from `AgentsProvider`, not a flag on it**, because
Enter means something different here — in the root list an agent row opens
the session in Paseo, and one provider cannot have two primary actions. The
**query is the prompt, not a filter**, the same rule `new_agent` follows:
filtering on it would shrink the list as you described the task and move the
row out from under the selection mid-sentence.

**`background: true` is not optional and is not a default.** Paseo derives it
as `Boolean(callerAgentId)` (`paseo-tools.ts:1881`) and neko is a top-level
caller with no agent id — so the default is `false`, which makes the tool
`await waitForAgentWithTimeout` (`:1906`), holding a daemon request thread
until the agent *finishes*, which can be minutes. Read out of Paseo's source
rather than discovered by hanging, and pinned by
`a_prompt_is_always_sent_in_the_background` against
`send_prompt_arguments` — split out from the call for exactly that reason.

**The modes come from the provider that owns them.** Claude and Codex offer
genuinely different sets, so a compiled-in list would be wrong for whichever
was not used to write it; `provider_modes` reads `list_providers` and caches
for two minutes, because it is configuration rather than state. Verified
live: seven `⌘K` actions on a Claude agent — five modes, Cancel, Archive.

### Measured, 2026-08-25

| | figure |
| --- | --- |
| warm summon (release, `NEKO_BENCH`) | **1.5–9.5ms, mean ≈5.1ms**; cold 37.4ms |
| daemon idle RSS, poller running | **13.8 MB, flat over 35s**, 0.0% CPU |

Consistent with the ~8.56ms mean recorded after the gpui throttle fix, so
none of tonight's work touched the summon path — which is expected: every
control-plane provider is daemon-side.

### A 17 MB clipboard entry was being scored on every keystroke

Found by measuring rather than by reading, and **pre-existing** — clipboard
search long predates the control plane. A burst of thirty one-character root
searches took the daemon from 14 MB to **1.6 GB**. It is transient rather
than a leak (the allocator gives it back), but a launcher that allocates tens
of megabytes per keypress is a launcher that stutters.

The cause: `Db::clipboard_entries` selected `content` in full for all 200
rows, and this machine's own clipboard holds **16.9 MB across them, with a
single entry of 17 MB**. Every keystroke read all of it and ran `fuzzy_score`
over the lot.

The fix is `substr(content, 1, ?)` **in SQLite**, so the bytes never enter the
process — `clipboard::MAX_MATCHED_BYTES` (8 KB). Nothing downstream can tell:
a row shows one line, the detail pane a paragraph, and
`CLIPBOARD_TITLE_LIKE_CHARS` (60) already collapses a long entry's score
toward nothing. Same burst after the fix: **1661 MB → 33.5 MB**, and a
further hundred searches added 0.3 MB. Clipboard search still matches.

**The general rule**: a provider that reads a table reads *columns*, and a
column holding arbitrary pasted content is unbounded by construction. Bound
it in the query, not after it.
