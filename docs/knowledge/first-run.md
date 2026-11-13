## A friend can install it: one command, one calm first run (2026-11-13)

Owner: "whatever we put in Neko is agnostic to our system and us… if any of my
friends install it, they should be able to adopt it. They shouldn't need too
much configuration." Everything below serves that. Writing for the person
installing: `README.md` (five steps) and `docs/getting-started.md`.

**The clean-clone test is the proof.** Before this, a fresh `git clone` could
not run any cargo command: the root `Cargo.toml` had a `[patch]` pointing at the
untracked `.gpui-fork-patched/` (the patched GPUI fork, needed only by the
optional GPUI client, which the installer never links). Fix: `crates/neko` is
its **own Cargo workspace** (own `[workspace]`, `[patch]` and `Cargo.lock`,
path dependencies on `neko-protocol` and `neko-client`); the root workspace
has no GPUI in its lock (7,000 lines fewer). Build it with
`cargo build --manifest-path crates/neko/Cargo.toml --target-dir target` after
`scripts/setup-gpui-patch.sh`; `scripts/check.sh` checks it only when
`.gpui-fork-patched` exists. `--target-dir target` is needed because cargo
looks for config in the working directory, not the manifest's.

**`scripts/install.sh`** (the friend's command): checks macOS 14+, Swift (Xcode
or CLT), Rust 1.88+ (let-chains), Node 20+ and pnpm, prints the exact command for
anything missing and stops before changing anything; `gh` and agent CLIs are
"skipped" lines, never errors. Then `install-native.sh --skip-checks` (the
test suites are for contributors) and `open /Applications/Neko.app`.
`--check` only checks; `--build-only` builds `target/native/Neko.app` without
installing (what the clean-clone test runs). Two build bugs a fresh Mac hit and
an owner's Mac never did: `codesign -dvv /Applications/Neko.app` failing under
`pipefail` when no Neko was installed yet, and `xcrun metal` missing without
full Xcode (the shader is optional: `GemShaders.library` is nil-safe, so the
build now skips it with one line).

**Owner-machine assumptions, audited.** Already optional and detected, left as
is: `~/.agents/bin/machine-ok` (no script, no hold; `core::machine`,
`core::self_update`), Paseo (read only when present; a routine pauses a Paseo
schedule only when it names one, and seeded routines never do), Peekaboo
(`scopes::peekaboo`, config simply lacks it), `~/.claude.json` (optional;
discovery runs without it), `gh` (a clear "install and `gh auth login`" error
only when a PR is opened), Codex in ChatGPT.app (one candidate among several,
`NEKO_CODEX_PATH` overrides), `refs/` (comments only), self-update (dormant
unless a `land_on_main` repository is Neko's own checkout). Fixed:
- **Routines were on from the first tick.** A fresh data folder made a
  workspace for every scope on disk, seeded routines `enabled: true` and ran
  Claude Code on them within minutes. Now `routines_master_v1` (`on`/`off`) gates
  both seeding and running (`core::master_on`): absent means "on only if a
  previous build already seeded routines" (`Store::seeded`), so existing installs
  change nothing and a fresh one is off until the person switches it on.
  `SetRoutinesOn` answers with `Routines`, which now carries `routines_on`.
  Asking for `Routines` on a fresh folder creates nothing (test:
  `a_fresh_data_folder_makes_no_workspace_and_no_routine_until_asked`); the home
  folder is offered as a "Personal" scope with the global MCP servers, not made.
- **Calendar asked macOS at launch.** `CalendarBridge` now asks (and reads)
  only while `calendar_enabled_v1` is on (`SetCalendarEnabled`,
  `CalendarStatus.enabled`); absent means "on if an earlier build was already
  granted", so the owner's calendar keeps working. It polls the switch every 20 s
  while off. Turning it off empties the stored events. Reminders were already
  asked only when the person turned the phone channel on.
- **The update source** is a setting (Settings → About, "Updates come from",
  `neko.updates.repository` in UserDefaults), defaulting to the project's public
  repository; `Updates.normalizedRepository` accepts `owner/name` or a GitHub URL
  and falls back to the default for anything else (a fork's friend points it at
  their fork).
- **`web-verify.mjs`** found `playwright-core` through another project's path;
  it is now a devDependency of `web/`, and the browser is `shared-chrome` when
  that exists, else any Chrome on `NEKO_WEB_CHROME` (default `:9333`).

**The first run** is one web setup (`web/src/onboarding/`), shown when
`GetOnboardingState` says not completed (an empty data folder), and the native
window shows that same page (`WebHomeView`, polling `GetOnboardingState` every
2 s until it completes) instead of its old SwiftUI copy; by default Neko opens in
the browser anyway. Steps, DESIGN.md §8 (title, one line, rows with no lines):
welcome (greets by the first name in `getpwuid`, `core::routines::owner_name`),
**agents** (the catalog's `status` for Claude Code, Codex and OpenCode: ready, or the
sign-in or install command with Copy, and Check again), **workspaces** (the
scopes from `Routines` with their MCP servers and an Add button each, plus an
add-a-folder box), **routines** (the one master switch, off), **this Mac**
(Calendar, phone reminders, clipboard history, Accessibility link), **done** (a
summary). Skip setup is on every screen; the switches save themselves, so
leaving halfway keeps what was chosen, and skipping switches nothing on.
Reopen from Settings → General ("Show again"), ⌘K, `/setup`, `/permissions`
(the This Mac step) or the hash `#/setup` (the native Home's Setup action).
Routines and Calendar also have a switch in Settings → General, and the Inbox
and Calendar pages say "off" with a Turn on button until then.
No owner names: the greeting and `{owner}` come from the Mac account.

**Not done.** The Mac steps are untested on a Mac that never ran Neko (the
Reminders and Calendar prompts need a real app and a person); the sign-in
commands are shown, not run; the native window's old Accessibility prompt
(`AXIsProcessTrustedWithOptions`) is gone, replaced by a link to the pane;
`install.sh` was proven to build in a clean clone, not to copy over a real
`/Applications`.

Check: `NEKO_WEB_ONLY=firstrun node scripts/web-verify.mjs`
(`scripts/verify/firstrun.mjs`; the harness switches routines on for its own
Inbox seed and the area switches them off to start fresh).
