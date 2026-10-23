## Markdown in the detail pane, the menu bar menu, the chime, and the banner that cannot exist

Four pieces in one pass, finishing the comet survey's remaining rows.

**Markdown (`crates/neko/src/markdown.rs`).** Agents write markdown and the
conversation pane rendered it as one flat monospace string, raw markers and
all. Parsed with `pulldown-cmark` (MIT, `default-features = false` — one new
transitive, `unicase`), which is what comet's own markdown stack parses with;
what was *not* taken is comet's ~4000-line streaming incremental reparse,
because that machinery exists for a transcript still being typed and this pane
shows a finished summary fetched whole. Inline styling rides gpui's own
`StyledText::with_highlights`, so bold and code sit inside naturally wrapping
text. **One honest limit: `HighlightStyle` has weight, style, colour and
background but no font family**, so inline code gets its wash in the
surrounding proportional face; fenced blocks are their own elements and get
real Menlo. The wire carries `SearchItem::preview_markdown`
(`#[serde(default)]`) — the provider states the *format*, the client owns the
rendering, never `if kind == "conversation"` in the panel. Only
`ConversationProvider` sets it.

**Two parser bugs, both caught by evidence rather than by the first test
suite, both now pinned.** A single flat span buffer let a nested item's close
scoop up its parent's text (`- outer / - inner` → `["outerinner", ""]`) —
every open container buffers its own spans now. And blocks emitted at *close*
put a parent item's text after all of its children, which on screen read as
the sublist belonging to the item above — visible in the first screenshot,
invisible to parse tests that only checked membership and depth. A parent
item flushes when its sublist opens. **The general lesson: order-sensitive
output needs an order-sensitive test**, and a membership assertion will pass
happily while the screen is wrong.

**Verification**: `NEKO_FORCE_PREVIEW_MARKDOWN` (evidence.rs) +
`NEKO_VERIFY_SEED_MARKDOWN` (verify_harness) — the real markdown rows need a
live Paseo daemon an isolated `HOME` cannot have, and pointing a client at the
captain's real daemon would put his actual transcripts in a screenshot, so a
seeded clipboard fixture drives the identical `markdown::render` path.
`docs/evidence/markdown-detail-pane.png`.

**The menu bar item has a real menu** — Summon neko / Preferences… / Quit
neko. Quit is the load-bearing item: an accessory app has no Dock icon and no
⌘Tab entry, so before it the only ways to stop neko were `kill` or logging
out. Left click still summons; right or ctrl click opens the menu, decided per
event in the action handler because **`NSStatusItem.menu` is all-or-nothing**
(assigning it makes every click open the menu and the action never fire). Two
traps recorded from building it: **auto-enabled menu items validate against
the responder chain**, and a target outside any chain renders the whole menu
greyed out — `setAutoenablesItems(false)` plus per-item enable; and a real
right-click cannot be synthesised here, so the launch log reads the menu back
item by item (`menu [Summon neko, Preferences…, —, Quit neko]`) — that line is
the proof.

**A chime when an agent blocks** (`crates/neko/src/sound.rs`) — comet's
`sound.rs` approach (platform audio CLI, zero Rust audio deps, failures
swallowed), but playing a *system* sound (Glass) rather than embedding one:
the point is to sound like a notification, not like neko, and Sosumi/Funk are
already claimed by system defaults on this machine. Only a **rise** in the
waiting count chimes — a fall is an agent handled elsewhere, equal is the
daemon re-asserting — and the first event after startup compares against zero,
so a client launched while agents already wait chimes once. `afplay` is reaped
with `status()`; evidence runs are suppressed structurally;
`NEKO_DISABLE_SOUND` is the kill switch.

**Notification banners are structurally unreachable from a bare Mach-O — now
proven by probe, not assumed.** This file already recorded that
`UNUserNotificationCenter` needs a bundle identifier; comet reaches the
*deprecated* `NSUserNotification` route instead by adopting an installed
app's bundle identity, and neko has no installed bundle to adopt. A compiled
probe (clang, AppKit, run once, deleted) settled the fallback too:
`[NSUserNotificationCenter defaultUserNotificationCenter]` returns **nil**
when `[[NSBundle mainBundle] bundleIdentifier]` is nil. Both routes die the
same way. **Banners become possible the day neko ships as a `.app`, and not
before** — the same threshold `SMAppService` already waits behind. The chime
above is the ambient ping in the meantime.
