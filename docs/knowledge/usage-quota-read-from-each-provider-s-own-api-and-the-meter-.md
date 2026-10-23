## Usage: quota read from each provider's own API, and the meter row

`/usage` — the captain's own *"establish our own method to get the usages
from the added model providers"*. `neko_core::usage` is the read;
`neko_protocol::Meter` plus `panel::render_meter` is the render. **Read that
module's own doc comment before touching this area**; it is the normative
statement of what follows.

**The source is each provider's API, not Paseo.** Paseo's own WebSocket was
tried first and abandoned: the handshake needs an undocumented
`protocolVersion` and `provider.usage.list.request` was never answered — the
session routing it needs is not written down anywhere. Reading the vendor
APIs directly is fewer moving parts and has no daemon to be running. The
*method* is Paseo's, because it is simply how these providers work; every
line is neko's own, which matters more for that clone than any other in
`refs/` — it is **AGPL-3.0**.

**Three vendors, one `Vendor` enum** — the same shape `agents::Backend` uses,
and adding a fourth costs one variant plus one arm in each of `name`,
`credential`, `request`, `parse`. Nothing else: not the provider, not the
cache, not the fan-out, not the rendering. Each was verified against this
machine's own live credential, not written from Paseo's source alone:

| vendor | credential | endpoint | what it gives |
| --- | --- | --- | --- |
| Claude Code | Keychain `Claude Code-credentials` | `api.anthropic.com/api/oauth/usage` | percentage windows |
| Codex | `~/.codex/auth.json` (+ `account_id`) | `chatgpt.com/backend-api/wham/usage` | percentage windows |
| Grok | `~/.grok/auth.json` | `cli-chat-proxy.grok.com/v1/billing` | credits used/limit |

- **A vendor with no credential here is omitted entirely**, not rendered as
  a "not configured" row. Eight providers' worth of "you are not signed in"
  would bury the two real answers. Only when *nothing* is signed in does the
  pane say so, once.
- **No token ever touches `argv`.** Every header goes to `curl` on **stdin**
  via `--config -`; `-H "Authorization: Bearer …"` would put a live
  credential where any local process can read it out of `ps`. Never logged,
  never persisted.
- **The three fetches run concurrently** (`thread::scope`, the same shape the
  daemon uses to fan a search across providers). Sequentially, three
  `REQUEST_TIMEOUT_SECS` stalls would put 30s behind a keystroke.
- **`CACHE_TTL` is 90s** because a mode re-searches on every keystroke.
  Enter on any row invalidates it, so the pane can be refreshed on demand.
- **Codex windows are labelled by their own length, not their slot.** Paseo
  names `primary_window` "Session" and `secondary_window` "Weekly"; this
  machine's *primary* window is 604800 seconds, so slot naming would have
  called a seven-day window a session. `limit_window_seconds` cannot go
  stale that way.
- **Only Claude windows in the known table are shown.** The response also
  carries `nimbus_quill`/`amber_ladder`/`cinder_cove` — real numbers against
  codenames whose meaning is not public. Pinned by a test, including the
  case where a codename does carry a value.
- **Grok bills credits, so the fraction is derived** — and an account with
  `monthlyLimit: 0` (what this machine reports) gets a note, not a bar: zero
  of zero is not "0% used", and an empty bar and a full one would both lie.
- **Resets are durations, never an absolute clock.** "resets 13:59 UTC" reads
  as *today* at 13:59, which for a weekly window is six days wrong, and
  forces timezone arithmetic either way. `epoch_from_rfc3339` is hand-rolled
  (Howard Hinnant's days-from-civil) rather than taking a date dependency for
  one field — the same trade `clipboard` and `agents` already made — and is
  tested against 1970, the 2000 leap-century case, and a real response.

**Not built, and why**: Copilot's token here is a plain `gh` CLI one and
`copilot_internal/user` answers it **401** — and that route returns a plan
label with *no* windows anyway, so it could never produce a meter. Cursor,
Kimi, MiniMax and Z.AI have no credential on this machine, so implementing
them from Paseo's source alone would be unverifiable code. Each is 80–340
lines there.

### The meter row

**`SearchItem::meter` is the render half, and it is a wire-vocabulary
addition rather than a client-side special case.** A row carrying a `Meter`
(a `0..=1` fraction plus labelled stats) renders as a two-column reading
instead of the single-line `render_row` treatment. Same bounded-vocabulary
rule as `Icon`/`Glyph`: the provider supplies numbers and labels, the client
owns entirely what a meter looks like. Deliberately **not**
`if kind == "usage"` in `panel.rs`, which is what "Provider abstraction"
above exists to forbid — any provider with a quantity (a disk, a download)
gets the same treatment free.

**The layout is StackAI's, and it came out of a Mobbin sweep rather than
taste** (Wise "Spending limits", GitHub Copilot iOS "Usage", StackAI
"Usage", Monzo, Glide, Firecrawl). Six read-only quota screens converged on
four things this originally got wrong: the qualifying note sits **directly
under the title**, not stranded in a far-right column; **headroom** is
stated, not just consumption ("how much is left" is the question the pane is
opened to ask); the meter carries its numbers **beneath** it rather than
beside the title; and **no card or border anywhere**. `render_meter` is
title-plus-note on the left, meter-plus-numbers in a fixed
`METER_COLUMN_WIDTH_PX` column on the right — fixed rather than a flex share
so every reading's meter starts at the same x however long its title runs.

- **Discrete ticks, not one continuous fill** (`METER_TICK_COUNT`, 32). They
  give the eye something to count against, so two rows compare without
  reading either number, and **each lit tick takes its own point on the
  ramp**, which makes the ramp legible as a scale rather than a wash.
- **The first stat is the reading, the rest qualify it** — the first takes
  the ramp colour and leads, the others follow muted. Rendered
  `"{value} {label}"`, so a provider writes `("13%", "used")` and
  `("120 of 150", "credits left")`. A vocabulary rule, not knowledge of who
  produced the row.
- **`tnum` on every number in that column** (`panel::tabular_numerals`,
  gpui's `FontFeatures`). Proportional digits are different widths, so a
  column of percentages shifts under the eye each time the pane refreshes.
- **A row with no number stays an ordinary row**, and `Reading::Note` still
  carries a *title* so its left column lines up with the meters above it.

**The colour ramp interpolates in OKLCH — the space this palette is already
authored in — and two cheaper spaces were measured and rejected.**
`theme::ramp(state_success, state_danger, fraction)`, so a reading's colour
means the same fraction however wide the column is, every theme gets its own
ramp, and **no palette token was added** (`PALETTE_TOKEN_COUNT` is still 23).

| space | what it does to the neutral theme's own pair |
| --- | --- |
| RGB, channel-wise | `#a5965c` at half full — a dirty khaki; `tokyo-night` collapses to **3%** of its endpoints' chroma, which is grey |
| HSL | hue is fine, lightness is not: bulges to OKLCH L `0.844` at 60% against `0.721`/`0.680` at the ends, so a meter shouts loudest where nothing is happening |
| **OKLCH** | `0.721 → 0.680` monotonically; worst chroma across all seventeen themes is 0.755 of its endpoints |

`oklch_to_srgb_u8` is no longer `#[cfg(test)]` — `ramp` runs it at paint
time, which also pins the frozen palette table and the live meter colour to
one conversion rather than two that can drift. Three tests hold the
property: lightness never leaves the endpoints' range, chroma never drops
below `RAMP_CHROMA_FLOOR` (0.70 — **not** 1.0, because sRGB cannot hold a
saturated yellow at these lightnesses and clamping is the gamut's doing, not
the interpolation's), and a **negative control** asserting the RGB mix this
replaced would fail that same bound.

**Still open**: Copilot and the four unverifiable vendors above, and anything
on screen (no window-scoped capture was taken; the wire response and the
tests are the whole verification).
