## Switching provider when a quota runs out

The captain's actual workflow, in his words: Claude Code through Paseo, "and
sometimes Codex and Grok as well — if I run out of tokens". neko already had
both halves of that and they had never been introduced. `usage.rs` reads
Claude, Codex and Grok quotas from each vendor's own API; `new_agent.rs`
picked a provider from history with **no way to override it**. And the vendor
ids `usage` uses are already exactly the strings Paseo's `--provider` takes —
`claude`, `codex`, `grok` — so the two spoke the same language without
knowing it.

Each **New Agent** row now carries `⌘K` entries for every other vendor, and
each entry states the headroom: `Start with Codex — 94% left`. The current
provider is left out, since Enter already does that one.

- **The most-used window governs**, not an average
  (`VendorUsage::headroom_percent`). A vendor at 95% on its five-hour window
  and 10% on its weekly has 5% of room for the next few hours; averaging to
  47% would be a comfortable-looking lie at exactly the moment the number is
  being acted on. Rounded **down**, which is the safe direction.
- **`None` shows no number rather than a guess** — signed out, a failed read,
  or a plan with no metered window. Verified live: this machine's Grok
  account reports `monthlyLimit: 0`, so it correctly renders `Start with
  Grok` with nothing after it while Codex reads 94%.
- **Never fetched on the search path.** `new_agent`'s search runs per
  keystroke of the prompt, and the blocking `usage()` fans out to three HTTPS
  requests. `usage::cached()` returns only what is in hand and
  `warm_in_background()` (single-flight) fills it a keystroke or two later —
  a stalled first character would be worse than a number that arrives second.
- **`Provider::perform_action_with_query`** is the one new seam, defaulted to
  discard the query and delegate exactly as `activate_with_query` does. A
  `new-agent` row's id is a directory and its query is the *prompt*, so
  "start this one with Codex" is unanswerable from the id alone. The query
  was already on the wire; it simply stopped at the daemon on the action path.
- **This path does not require history, unlike Enter's.** Enter infers a tool
  and refuses when it cannot; this one was *told*, which makes it the way out
  of a fresh machine as well as the way out of a spent quota.
