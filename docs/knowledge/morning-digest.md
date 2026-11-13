## The morning digest (2026-11-14)

One calm summary a day, composed from data Neko already has. No account, no
model call. It is shown at the top of Home and pushed once to the phone.

**What it says** (`neko_core::digest::compose`, pure and tested; DESIGN.md
section 6: status, then bottleneck, then what needs the person):
- the headline, one sentence: "3 things finished, 1 thing is stuck (Blocked: …).
  2 things need you." or "… Nothing needs you right now.";
- **Finished** since the last digest (at most three days back, 24 h the first
  time): tasks completed, goals DONE, PRs merged (a task whose PR state is MERGED
  is listed once, as the PR);
- **Stuck** and why: goals BLOCKED (their question and default), HELD (usage
  limit), PAUSED (their reason), tasks that failed since, and the spend
  ceiling holding new work;
- **Needs a decision**: the open Inbox items, each with its default or "No default";
- today's calendar count and first timed event, only when Calendar is on and
  allowed ("3 events today, first: Standup at 09:30."); and yesterday's reported
  spend, only when something was reported;
- every list shows five lines and "and N more".

**Skipped** when nothing finished, nothing is stuck and nothing needs a
decision: the calendar and spend lines alone are not a digest. A skipped day is
still handled, so it is not retried.

**When.** `PhoneSettings.digest` (on) and `digest_hour`/`digest_minute`
(08:30 local), in Settings, Phone. The daemon's 30 s phone tick composes it once
per local day after that time, never more than six hours late (a Mac that
slept through the morning skips the day), and Home composes it too when it
opens after the time (`Request::MorningDigest`), whichever asks first. It is
stored in its own row, `neko_digest_v1` (day handled, when composed, the digest,
pushed): about one small write a day, nothing timer-driven in the snapshot.

**Push.** Once per digest, when Reminders or ntfy is on, outside quiet hours
(it waits for them to end, and is dropped 12 h after composing): one reminder
whose notes hold the whole digest (marker `neko:4000000000000`,
`phone::DIGEST_SEQ`) and one ntfy message (title only while "Send titles
only" is on). **Completing the reminder marks the digest read**, which hides
the Home block; marking it read on Home deletes the reminder. Typing under it
answers nothing.

**Home.** One block above the goals line and the conversation
(`web/src/work/digest.tsx`): the headline, a caret that opens the lists, "Mark
read" on hover. Nothing when there is no unread digest.

**The "Morning brief" routine is separate and untouched.** The routine
(`RoutineKind::Digest`, daily 10:30, Claude Code) reaches out to the person's
accounts (PRs, issues, calendar, monitoring) and costs a model run; the digest
only reads Neko's own data and is free. Merging them would have changed routines
the person may have customised, so nothing in that routine, its prompt or its
schedule moved.

Check: `NEKO_WEB_ONLY=digest node scripts/web-verify.mjs` (`scripts/verify/digest.mjs`);
Rust: `neko_core::digest::tests`, `neko_daemon::morning_digest::tests`.
