## Quota in the menu bar — the half that finds you

Switching provider only helps if you know to switch. `⌘K` on a New Agent row
answers "which has room" at the moment you are already starting one; this is
the half that reaches you before that, on the surface that is always visible.

**It has to travel the wire, and that is the whole design constraint.** Quota
lives in `neko_core::usage`, behind the crate boundary `neko` never crosses —
and the menu bar is not a search surface, so there is no request it could ride
in on. `Event::QuotaChanged { quotas: Vec<QuotaSummary> }` is the answer, the
same shape `AttentionChanged` already uses: the daemon watches, the client
renders. `QuotaSummary` is deliberately *not* the whole `usage::Reading` —
the menu shows one line per vendor, and shipping windows, resets and detail
strings for a surface that renders none of them would be inventing a second,
richer channel beside the `usage` mode that exists for exactly that.

**`run_quota_poll` is its own thread at `QUOTA_POLL_INTERVAL` (120s), not a
branch of the attention poll.** They differ by a hundredfold in both cost and
urgency: a blocked agent is news within a second and costs one loopback call,
a quota moves over hours and costs three HTTPS round trips against
rate-limited vendor APIs. Folding them together would make the cheap one
expensive or the expensive one useless. 120s also sits just above
`usage::CACHE_TTL` (90s), so each tick does real work rather than returning
what the last one already had. Broadcasts on change only.

**One title, composed from two facts.** The button carries a single label and
the two inputs arrive on different schedules, so `menu_bar::STATE` holds both
and `title_for` recomposes — otherwise whichever arrived last would clobber
the other's work. **A blocked agent outranks a low quota**: one is waiting on
you now, the other will matter shortly, and there is room for one number with
no way to say which it is. Below `QUOTA_WARN_PERCENT` (10) the title reads
`4%`; above it, nothing — a permanent scold is not a warning.

**An unknown reading never warns, and is still listed.** `percent_left: None`
— signed out, a failed read, a plan with no metered window — is carried
rather than dropped, and the menu says "no quota reading" for it. "I could not
tell you" and "you have none left" are the two answers a person would act on
most differently, and only one of them is worth interrupting for. Verified
live over the wire: `Claude Code 83% left · Codex 96% left · Grok unknown`,
where Grok is correct — that account reports a zero monthly limit.

The menu's quota lines are **inert** (no target, disabled): there is nothing
to *do* to a reading there, and the Usage mode is where it can be acted on. A
disabled item still reads clearly, which is all a status line has to do.

`count_label` is gone rather than left beside `title_for` — one question, one
answer — with its "zero is no badge, and no `99+` cap" rule folded into the
function that now owns it.
