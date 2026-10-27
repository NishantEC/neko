# neko — project knowledge

## How to use this file

Detailed project knowledge lives in `docs/knowledge/`. **Before changing an area, read its doc below** (grep this index for the topic). Do not load them all. When you learn something durable, add it to the matching doc, not here; keep this file small.

## Resource limits (2026-10-18)

Measured on the owner's machine: the daemon wrote 43 GB in 12 hours (15
macOS disk-write violations in 8 days), 84 tickets made 72 worktrees, all 17
Completed tickets changed nothing, and 12 of 18 Failed tickets never started.

- **Storage layout (2026-10-21).** The workbench is no longer one row: tasks
  are rows in `workbench_tasks`, their events append-only in
  `workbench_task_events` (only new events are inserted and dropped ones
  deleted), MCP connections and sources have rows of their own, and
  `workbench_snapshot_v1` keeps the rest (~110 KB, was 4.7 MB). `load`
  assembles the same `Snapshot`; `save` writes only changed rows, in one
  transaction. Modelled on vibe-kanban, Codex, Crystal and Terragon, all of
  which store a row or file per entity. The old single row is kept once as
  `workbench_snapshot_v1_backup` for one release, then delete it. SQLite runs
  WAL with `synchronous=NORMAL` and an 8 MB `journal_size_limit`.
- **Snapshot writes.** The workbench was one multi-megabyte JSON row
  (`workbench_snapshot_v1`). `Db::set_setting` skips a value identical to
  what it last wrote (the cache is cleared on rollback), `heartbeat_ms` is
  kept in memory and never persisted (it changed every 2-second tick, so
  every tick rewrote the whole row), and agent tool events go through
  `store::save_deferred` / `Db::set_setting_deferred`: readers see them at
  once, disk gets them at most every 5s (`DEFERRED_WRITE_DELAY`), folded
  into the next immediate save that changes something, the tick's
  `flush_due` or `Db` drop (a rolled-back transaction restores them). A crash may lose ≤5s of tool events, never a user action,
  status change or `USAGE` (budget) event: those still write immediately.
  Do not persist per-tick values in the snapshot. **Anything that changes on
  a timer gets its own small row** (2026-10-21, measured 4.4 MB/s → see below):
  MCP tool receipts (`workbench_mcp_receipts_v1`, deferred), what GitHub reports
  per PR (`workbench_pull_request_status_v1`, deferred) and each watch's
  schedule (`workbench_watch_schedule_v1`, immediate: due times are durable
  claims). `encode` strips them from the snapshot and `load` attaches them, so
  the 4.6 MB row is rewritten only by real decisions. Measure with the
  kernel's per-process counter (`proc_pid_rusage`, `ri_diskio_byteswritten`)
  and diff the stored row to name the field before guessing.
- **Writable runs per repository.** `admission::cap_writable_per_repository`
  runs before load pacing: per repository (`root_for`), active workers whose
  ticket `writes()` (Building/Reviewing/AwaitingApproval, or Queued that will
  build directly) count against `Snapshot::writable_per_repository()`
  (default 2 since 2026-10-22, was 4; `SetMaxWritablePerRepository`, Settings → Budget). Over-cap
  candidates stay queued; read-only candidates (planning, read-only asks,
  pending splits) always pass. Keep `writes()` in step with
  `execute_with_worktree`'s `work_directly`.
- **Worktree lifecycle.** Removal of Completed/merged/idle folders is the
  existing parking sweep (work saved to the branch first, so uncommitted or
  unpushed work is never lost). Added: at ReadyForReview the folder's
  Git-ignored `node_modules`/`.venv` are deleted
  (`native_runner::prune_dependency_folders`; this also covers split tickets,
  which are never parked), and resuming in an existing folder re-seeds
  `node_modules` (`reseed_dependency_folders`; `.venv` is never seeded, it
  holds absolute paths). `parking::ensure_restored` now also recreates a
  recorded folder that is missing without being parked, at the same path,
  from `codex/neko-<folder>` or, if that branch is gone, from `base_commit`
  (`native_runner::parking::recreate_missing_worktree`), with a ticket event.
- **Duplicates.** `neko_core::duplicates::duplicate_of` matches an *open*
  ticket in the workspace by source key (a watched source's `external_id`,
  or a title's leading `HEA-3006:`-style key) or, within one repository, by
  normalized title (key prefix, case, punctuation dropped; ≥12 chars). Used
  when a watch files a source (`responsibility::apply_result`; the old
  lookup was per watch, so two watches on one Linear connection filed every
  issue twice) and when chat opens a ticket; a match gets a "Filed again"
  note and the source/chat links it. A ticket the user creates by hand is
  always made. Same-problem-different-words stays with model triage.
- **Existing work before the first run.** When a ticket gets its first
  checkout, `native_runner::prior_work::find` looks for its issue keys in
  local branch names and in commits on local branches the base lacks, and
  for open PRs by key or title (`gh pr list --search`, 20s, skipped in
  tests); findings become a ticket event and part of the run's direction
  (check it first; `NO_CHANGE: covered by …` if it already fixes it).
- **A broken MCP connection no longer blocks tickets.** 12 of 18 Failed
  tickets died at claim with "Run tool scope is paused, unavailable, or
  changed": `RunAuthority::for_task` took every granted connection in the
  workspace and `valid()` requires each grant to authorize, so one
  connection with an auth error refused every new ticket. `for_task` now
  drops connections that are not usable right now (`usable_connection`)
  and runs without their tools; responsibility wakes (`RunAuthority::new`)
  stay strict. Mid-run invalidation is unchanged: a connection that breaks
  during a run still discards it ("Task authority changed…").
- **Processes an agent leaves behind.** Claude Code runs each shell command
  in its own process group, so killing the agent's group missed them (a
  `jest` from a Neko task ran 7 days). `execute_with_guard` records the
  run's descendants every 250ms (`Descendants`, libproc) and, when the run
  ends for any reason, SIGKILLs those still alive (pid + start time match,
  so reused pids are safe) and their groups. Claude/OpenCode still have no
  quiet-stall check (they emit no command-start events); the wall-clock
  `RunSpec::timeout` (30 min builders, 10 min planning/review) bounds them.

**Long-lived children die with the daemon** (`native_runner::spawn_guarded`):
the app-index `mdfind -live` and the Codex app-server run under the same
guardian agent runs use, which ends the child's process group when the
daemon's end of a socket closes, however the daemon ends. Spawned plainly,
each daemon restart left an `mdfind -live` behind until the next app install
(ten found, up to seven days old). The guardian blocks on that socket
(`poll`, 100 ms) instead of waking every 10 ms, since it may live all day.

**Machine steward** (`neko_core::machine`): a thread reads load, memory
pressure, compressed memory, swap, free disk, kernel_task and RAM per kind of
agent every 15 s. `apply_policy` turns that into `holding` (disk under 15 GB,
memory critical, load 20+, or night load over 12; nothing new starts) or `cap`
(01:00–06:00 or memory warning: one agent at a time). Admission obeys it; the
sidebar strip shows it. Since 2026-10-22 it also asks the shared
`~/.agents/bin/machine-ok` each reading (`comfort_wait`): its WAIT holds new
work, because the owner wants the Mac cool and quiet, not only safe. `neko-daemon --machine-check [--hook]` gives agents
outside Neko the same answer (exit 1 / a PreToolUse deny while held). The
hook also caps test runners at 2 workers (rewrites the command in
`bypassPermissions` sessions, otherwise refuses with the capped command, never
auto-approving), and hands each session Neko's machine rules once per change
(`additionalContext`, tracked per `session_id`), so agents that started days
ago follow rules written today. The daemon publishes those rules to
`agent-rules.md` in the data dir; Neko's own runs also get Vitest/pytest
worker-cap env, and at night over load 12 the steward renices build and test
processes to 10 (nothing is stopped).
**Web-only changes ship without a reinstall** (2026-10-27): commit `web/`,
then `bash scripts/ship-web.sh` (~1 min; `--rollback` undoes). It swaps
`<data>/web-live`, which the running daemon serves per request; anything
outside `web/` since the installed app still needs `install-native.sh`. See
`docs/knowledge/neko-updates-itself.md`.
## Native design system and the check gate (2026-10-14)

`native/NekoKit/Sources/NekoNative/DesignSystem/` owns every visual value
(`Tokens`, `Motion`, `Surfaces`, `Controls`, `Shell`, `Appearance`) and the
screen shells: `NekoPage`, `NekoSection`, `NekoRow`, `NekoToggleRow`,
`NekoDividedList`, `NekoSheet`, `NekoEmptyState`, `NekoNotice`. Rules and the
catalogue are in `docs/design/principles.md`; FluidVoice (GPL-3.0, `refs/`)
was the reference for the shape, not for code. **Screens compose shells and
never draw their own chrome; to change how a page or sheet looks, change the
shell.** `DesignSystemGuardTests` reads the sources and fails on a radius,
font size, colour, animation curve or raw button style outside the design
system (`Activity/` owns its glyph hues), and caps raw padding/spacing
numbers at a ceiling that may only fall. Glass goes through
`NekoGlassSurface`, which falls back to an opaque bordered card under Reduce
Transparency or Increased Contrast; motion goes through `NekoMotion`, which
disappears under Reduce Motion. Pages are a typed `Page` enum (raw values are
the old keys, so `NEKO_START_PAGE` still works) and navigation sets
`model.requestedPage` rather than posting strings. Settings is searchable
(`SettingsTarget`, ⌘F). The ⌘K panel is built at launch and every show/hide
carries a `PaletteGeneration` token so stale callbacks cannot close a fresh
panel. `scripts/render-previews.sh` renders the shells offscreen in light and
dark without showing or focusing a window; prominent buttons look grey and
glass looks empty there, which is the inactive offscreen window, not a defect.

`scripts/check.sh` is the gate: clippy's always-a-defect lints, every Rust
test and the Swift tests (~70s). `install-native.sh` runs it first and
installs nothing on failure (`--skip-checks` overrides); `.githooks/pre-commit`
runs `--quick` (compile only) when Rust or Swift is staged, enabled with
`git config core.hooksPath .githooks`. Both wait for load average < 20.
`cargo fmt` is deliberately not in the gate: this codebase keeps long lines.
## Documentation written for people

Four documents cover the shape of this project for a human reader. Point
someone at those first; they are shorter and they are current.

| Document | Covers |
| --- | --- |
| `README.md` | What neko is and is not, build and run, the licence position |
| `docs/architecture.md` | Crates, the daemon/client split, the wire protocol, providers, ranking, modes, themes, the window decisions |
| `docs/adding-a-provider.md` | Adding a result type through the `Provider` seam |
| `docs/adding-a-theme.md` | Adding a theme: the token contract and the tests that gate it |

**This file stays the authority.** Those four are summaries chosen for a first
read; every one of them drops nuance this file keeps — what was measured, what
was tried and ruled out, and which fixes were wrong before they were right.
When one of them disagrees with this file, this file is the record and the code
is the truth. Nothing was removed from here to write them.

Keep them in sync when a documented shape changes: the crate boundary, the
`Provider` trait, `search::allocate`'s passes, the theme token contract, the
build steps, or any of the window decisions in `architecture.md`'s last two
sections. A behaviour change that leaves one of those pages wrong is not
finished.

## Knowledge index

- [Current standalone workspace (2026-09-24)](docs/knowledge/current-standalone-workspace.md) — The native workspace now follows system appearance, with Light / Dark…
- [Neko is the chief of staff (2026-10-17)](docs/knowledge/neko-is-the-chief-of-staff.md) — Every task goes through Neko.
- [Pull requests after they open (2026-10-21)](docs/knowledge/pull-requests-after-they-open.md) — Learned from a Paseo agent that walked Neko's PRs to merge by…
- [One registry: plugins are not a separate world (2026-10-22)](docs/knowledge/one-registry-plugins-are-not-a-separate-world.md) — Owner: "plugins and all are a different thing in our heads, it should…
- [Chat is the AI SDK's (2026-10-22)](docs/knowledge/chat-is-the-ai-sdk-s.md) — Owner decision: Neko's chat protocol is the Vercel AI SDK's (ai, @ai-sdk/react).
- [Neko updates itself (2026-10-22)](docs/knowledge/neko-updates-itself.md) — Owner: Neko "should be able to write itself over".
- [Current scope: the spine, clipboard history, the first-run onboarding ](docs/knowledge/current-scope-the-spine-clipboard-history-the-first-run-onbo.md) — Slice 1 was a single-crate proof that GPUI works at all.
- [Crate layout](docs/knowledge/crate-layout.md) — crates/…
- [The daemon/client split, and why the hotkey is registered where it is](docs/knowledge/the-daemon-client-split-and-why-the-hotkey-is-registered-whe.md) — The daemon is resident, owns SQLite as the single writer…
- [The hotkey is a runtime-configurable setting, not a constant](docs/knowledge/the-hotkey-is-a-runtime-configurable-setting-not-a-constant.md) — Added mid-task, on top of the original single-crate proof's…
- [Onboarding (design report §3)](docs/knowledge/onboarding-design-report-3.md) — six visible Paper steps live in onboarding/setup.rs;
- [Application discovery](docs/knowledge/application-discovery.md) — nekocore::apps (scanapplications, watchapplications).
- [Click-outside dismissal and inline activation errors](docs/knowledge/click-outside-dismissal-and-inline-activation-errors.md) — Two more neko-p0-fixes defects, both in main.rs/panel.rs.
- [Text field editing shortcuts](docs/knowledge/text-field-editing-shortcuts.md) — neko-p0-fixes's fourth fix.
- [Two-phase search: results as you type](docs/knowledge/two-phase-search-results-as-you-type.md) — fm/neko-instant-search, fixing the captain's own daily complaint…
- [Search and ranking](docs/knowledge/search-and-ranking.md) — nekocore::search::fuzzyscore is a small, dependency-free…
- [Icons](docs/knowledge/icons.md) — nekocore::icons extracts each app's real icon via…
- [Clipboard history](docs/knowledge/clipboard-history.md) — nekocore::clipboard — capture, storage, and restore all live in…
- [Clipboard capture memory: every AppKit call site must be pool-wrapped](docs/knowledge/clipboard-capture-memory-every-appkit-call-site-must-be-pool.md) — The capture thread ran forever, on a plain background thread with…
- [Provider abstraction](docs/knowledge/provider-abstraction.md) — nekocore::provider::Provider is the seam every result type in the…
- [Commands and modes](docs/knowledge/commands-and-modes.md) — The audit's top-priority finding:
- [Preferences: a real window, and the two rejected shapes before it](docs/knowledge/preferences-a-real-window-and-the-two-rejected-shapes-before.md) — fm/neko-preferences, from the captain's own "it's better to have…
- [Agents: what is running right now](docs/knowledge/agents-what-is-running-right-now.md) — fm/neko-preferences, from the captain's own "I also want the…
- [Starting an agent: the New Agent command](docs/knowledge/starting-an-agent-the-new-agent-command.md) — fm/neko-new-agent, closing the gap the section above listed as not built.
- [Usage: quota read from each provider's own API, and the meter row](docs/knowledge/usage-quota-read-from-each-provider-s-own-api-and-the-meter-.md) — /usage — the captain's own "establish our own method to get the…
- [The better-* interface pass](docs/knowledge/the-better-interface-pass.md) — A better-interface-orchestrated review of every surface…
- [The agent control plane: MCP, and the permission inbox](docs/knowledge/the-agent-control-plane-mcp-and-the-permission-inbox.md) — docs/plan-agent-control-plane.md is the plan;
- [Ambient awareness: the attention poll and the Dock badge](docs/knowledge/ambient-awareness-the-attention-poll-and-the-dock-badge.md) — L3 of docs/plan-agent-control-plane.md.
- [Schedules: the agents that start themselves](docs/knowledge/schedules-the-agents-that-start-themselves.md) — L4 of docs/plan-agent-control-plane.md.
- [Ask neko: a sentence becomes a tool call you confirm](docs/knowledge/ask-neko-a-sentence-becomes-a-tool-call-you-confirm.md) — L6 of docs/plan-agent-control-plane.md, and the payoff.
- [Terminals, and the half of L5 that was not built](docs/knowledge/terminals-and-the-half-of-l5-that-was-not-built.md) — nekocore::terminals — a Terminals mode listing Paseo's…
- [The better-* pass over the control-plane surfaces](docs/knowledge/the-better-pass-over-the-control-plane-surfaces.md) — Same six domain skills, run over what L2–L6 added (permission rows,…
- [The Agents mode, and what the control plane costs](docs/knowledge/the-agents-mode-and-what-the-control-plane-costs.md) — every agent with the keyboard pointed at what you came to do:
- [`action_label` was live data going nowhere](docs/knowledge/action-label-was-live-data-going-nowhere.md) — Found by photographing the surfaces that had never been seen, which is…
- [The mark](docs/knowledge/the-mark.md) — neko has its own mark:
- [Out of the Dock: the activation policy and the menu bar item](docs/knowledge/out-of-the-dock-the-activation-policy-and-the-menu-bar-item.md) — reason: gpui's mac backend…
- [The root list scrolled all along; the keyboard just never moved it](docs/knowledge/the-root-list-scrolled-all-along-the-keyboard-just-never-mov.md) — Reported as "why does this not scroll — if I go down it will just go…
- [What three reviewers found, and the two defects worse than the one rep](docs/knowledge/what-three-reviewers-found-and-the-two-defects-worse-than-th.md) — The captain reported "mouse clicking doesn't work" and asked for designers.
- [Reading an agent's conversation inside neko](docs/knowledge/reading-an-agent-s-conversation-inside-neko.md) — worth keeping. confirm's agent-tile branch (and the tile's…
- [Two evidence-safety defects, found by a capture that reported `key win](docs/knowledge/two-evidence-safety-defects-found-by-a-capture-that-reported.md) — Both were introduced by work earlier the same night, and both are the…
- [`⌘K` did nothing, because almost nothing had actions](docs/knowledge/k-did-nothing-because-almost-nothing-had-actions.md) — Reported as "command K does nothing", and the keystroke, the handler and…
- [The shared pulse clock, and the repeating-animation rule](docs/knowledge/the-shared-pulse-clock-and-the-repeating-animation-rule.md) — motion::PulseClock — the seam motion.rs's own doc comment reserved,…
- [Markdown in the detail pane, the menu bar menu, the chime, and the ban](docs/knowledge/markdown-in-the-detail-pane-the-menu-bar-menu-the-chime-and-.md) — Four pieces in one pass, finishing the comet survey's remaining rows.
- [Motion: hover washes, a scroll glide, and gpui's own reduce-motion fla](docs/knowledge/motion-hover-washes-a-scroll-glide-and-gpui-s-own-reduce-mot.md) — Ported from comet's crates/ui/src/motion.rs (MIT), which pins the…
- [Killing an evidence client: `pkill -x neko` is wrong, and so is `pkill](docs/knowledge/killing-an-evidence-client-pkill-x-neko-is-wrong-and-so-is-p.md) — Recorded because it has now gone wrong twice, in opposite directions, and…
- [One client, and why "still not fixed" was true](docs/knowledge/one-client-and-why-still-not-fixed-was-true.md) — fixed structurally. Launch at login starts a client;
- [A skeleton, and the empty line that was lying during a fetch](docs/knowledge/a-skeleton-and-the-empty-line-that-was-lying-during-a-fetch.md) — Reported as "usage shows nothing then shows everything — shouldn't there be…
- [Quota in the menu bar — the half that finds you](docs/knowledge/quota-in-the-menu-bar-the-half-that-finds-you.md) — Switching provider only helps if you know to switch.
- [Switching provider when a quota runs out](docs/knowledge/switching-provider-when-a-quota-runs-out.md) — The captain's actual workflow, in his words:
- [A chat is scrolled, not stepped through — and what you can paste into ](docs/knowledge/a-chat-is-scrolled-not-stepped-through-and-what-you-can-past.md) — of the row list. That was wrong for the surface:
- [The bottom corners the scroll fade squared off](docs/knowledge/the-bottom-corners-the-scroll-fade-squared-off.md) — Reported as "the corners aren't rounded at the bottom, I think it's the…
- [The mouse, and the things the panel knew and never showed](docs/knowledge/the-mouse-and-the-things-the-panel-knew-and-never-showed.md) — A pass over every finding three reviewers left open, plus what the…
- [Third-party UI: vendored per file, because the dependency route is clo](docs/knowledge/third-party-ui-vendored-per-file-because-the-dependency-rout.md) — gpui-component targets the crates.io gpui;
- [Icons: real SVGs, and the one that stays painted](docs/knowledge/icons-real-svgs-and-the-one-that-stays-painted.md) — fm/neko-icons. Full record, including what was and was not verified live:
- [Mode-view row anatomy and the neutral re-tone](docs/knowledge/mode-view-row-anatomy-and-the-neutral-re-tone.md) — neko-mode-visual, prompted by a captain screenshot of the clipboard…
- [Mode view resize seam](docs/knowledge/mode-view-resize-seam.md) — neko-mode-resize, fixing a real defect two prior…
- [The double-panel shadow defect](docs/knowledge/the-double-panel-shadow-defect.md) — fm/neko-double-panel, fixing the regression the correction just…
- [The panel shadow tent — the second overlapping shadow source](docs/knowledge/the-panel-shadow-tent-the-second-overlapping-shadow-source.md) — fm/neko-panel-shadow-tent, fixing exactly the gap the correction…
- [One constant panel width, and the real top line](docs/knowledge/one-constant-panel-width-and-the-real-top-line.md) — fm/neko-footer-hairline.
- [The top line — AppKit's titled-window rim](docs/knowledge/the-top-line-appkit-s-titled-window-rim.md) — fm/neko-top-line-titled, the third and actual fix for the symptom the…
- [Comet craft pass: floating-layer discipline, a throttled motion catalo](docs/knowledge/comet-craft-pass-floating-layer-discipline-a-throttled-motio.md) — neko-craft-pass, working from a read-only design study of…
- [Menu frost backdrop and the results-list edge fade](docs/knowledge/menu-frost-backdrop-and-the-results-list-edge-fade.md) — fm/neko-frost, requested directly by the captain after seeing…
- [Third-party UI code: evaluated, then narrowly vendored](docs/knowledge/third-party-ui-code-evaluated-then-narrowly-vendored.md) — longbridge/gpui-component (Apache-2.0, crates.io) was evaluated as…
- [Design tokens](docs/knowledge/design-tokens.md) — theme.rs's colour tokens are no longer pub consts — they are fields on…
- [Themes](docs/knowledge/themes.md) — fm/neko-themes, from the captain's own "can we suport like themes…
- [Standing safety rule: an evidence window must never become the key win](docs/knowledge/standing-safety-rule-an-evidence-window-must-never-become-th.md) — doing. A NEKOSHOWSKELETON capture logged key window false (after…
- [Window material](docs/knowledge/window-material.md) — Real native material, not GPUI's own…
- [Dragging the panel: snapping and highlighted guides](docs/knowledge/dragging-the-panel-snapping-and-highlighted-guides.md) — fm/neko-drag-snap. Full account, the pixel measurements, and what…
- [Window placement, disconnection, and Spaces](docs/knowledge/window-placement-disconnection-and-spaces.md) — Three fixes from an exhaustive audit (data/neko-audit/report.md,…
- [v1 simplification: fixed-size window, not dynamic per-keystroke resize](docs/knowledge/v1-simplification-fixed-size-window-not-dynamic-per-keystrok.md) — gpui::Window::resize exists, but this task had no safe way to verify…
- [Summon latency](docs/knowledge/summon-latency.md) — Re-measured after the restructure (release binary, RUSTLOG off —…
- [Hotkey scoping (must never leak into other apps)](docs/knowledge/hotkey-scoping-must-never-leak-into-other-apps.md) — global-hotkey's macOS backend registers through…
- [Licence rule](docs/knowledge/licence-rule.md) — see "Third-party UI code" above (gpui-component's…
- [The GPUI dependency decision](docs/knowledge/the-gpui-dependency-decision.md) — previously chose the opposite route specifically to avoid the cost…
- [A known, upstream-fixed-but-unreleased deadlock in the real summon pat](docs/knowledge/a-known-upstream-fixed-but-unreleased-deadlock-in-the-real-s.md) — neko-leak-confirm set out to confirm a hypothesized ~4GB-per-summon…
- [Codex quick attention loop](docs/knowledge/codex-quick-attention-loop.md) — neko-core::codex is the current Codex integration seam, and it is…
- [Seams for follow-up work](docs/knowledge/seams-for-follow-up-work.md) — the daemon; see "Clipboard history" above.
- [Chat tool authority](docs/knowledge/chat-tool-authority.md) — Chat MCP tools use the selected workspace's lease and current connection catalog;
- [Maintaining this file](docs/knowledge/maintaining-this-file.md) — Keep this file for knowledge useful to almost every future agent session in this project.
