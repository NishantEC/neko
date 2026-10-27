## Neko updates itself (2026-10-22)

Owner: Neko "should be able to write itself over". When a Neko task lands a
commit on Neko's own checkout (the `land_on_main` repository holding
`crates/neko-daemon` and `web/`; `NEKO_SELF_CHECKOUT` overrides), the change
goes live on its own. Decisions are pure in `neko_core::self_update` (tested);
the I/O is `crates/neko-daemon/src/self_update.rs`, one thread woken by the
update commands and otherwise every 10 s. State is `Snapshot::self_update`.

- **Trigger**: a task's `landed_commit` newer than `applied_through` on the
  checkout's `main`, or the owner's **Update Neko now** (`UpdateNekoNow`). A
  commit made by hand is not applied on its own (it rides along with the next
  landed one). The first pass takes the running app's commit (`NekoGitCommit`
  in the bundle's Info.plist) as the starting point, so history is not
  replayed. Docs-only changes do nothing.
- **Guardrail**: `self_update::SENSITIVE` is the one list of safety-critical
  paths (rulebook defaults, approval/authority/review/publish/merge code, MCP
  tool authority, `web.rs` access control, the updater and the scripts it
  runs, `patches/`); a test asserts every entry exists. A range touching any
  of them becomes `pending` and waits for a click: **Apply update** in
  Settings → Neko updates, or the chat action `apply_update` Neko posts on the
  task that brought it (`neko_chat::post_notice`). Switching an auto-update
  off also makes updates pending; switching it on releases them (never a
  safety-critical one). The click is a guard against accidental self-change,
  not a security boundary: full-access agents can reach the daemon socket.
- **Web UI** (auto by default): `pnpm run build --outDir <data>/web-live.next`
  in the checkout (installs first when the lockfile changed; waits for
  `machine-ok` and a clean `web/`), smoke check (index.html loads a script and
  every `/assets/…` it names exists), then `web-live` — a symlink into
  `<data>/web-builds/` — is replaced by one `rename`, the old target kept as
  `web-live.prev`. `web::web_roots` reads per request: live, then prev (a tab
  that loaded the old page can still fetch its lazy chunks), then the bundle.
  A failed build keeps the current UI and records why. `Event::WebUpdated`
  goes to open pages; `api.ts` reloads only when nothing was typed for 5 s and
  no focused field holds text. A newer app (installed by hand) outranks an
  older live UI: `web-live` is dropped when the bundle's commit is not older.
- **Native app** (auto by default, when idle): waits until `Controller::busy`
  shows no task worker, no publish in flight and no chat reply, `machine-ok`
  exits 0 (and the steward is not holding), and the checkout is exactly at the
  commit with no uncommitted tracked changes. Skipped when the app already
  runs that commit, or when the daemon is not running from a bundle.
- **How the daemon replaces itself — the choice**: it cannot, so it does not
  try. It starts `scripts/self-update.sh` with `/bin/bash` in **its own process
  group** (`process_group(0)`), stdout to `<data>/self-update/install.log`,
  and does not wait. `install-native.sh` (checks first, `CARGO_BUILD_JOBS=2`)
  stops the installed app and daemon by pid, which leaves the script alive;
  the script relaunches `/Applications/Neko.app` (the app starts the daemon),
  health-checks it (web `Ping` → `Pong`, `{"Workbench":"Snapshot"}` loads,
  `GET /` is 200, installed `NekoGitCommit` is the target), restores the
  newest `/Applications/.neko-native-backup.*/previous-Neko.app` and
  relaunches on failure, keeps exactly one backup on success, and writes
  `<data>/self-update/result.json`. Whichever daemon is running reads it
  (`take_result`) and records "Neko updated itself to <sha> (app
  reinstalled)" or "Rolled back …" on the task and in chat. Rejected: a
  LaunchAgent (persistent state outside the data dir for a one-shot job) and
  the native app doing the swap (it would need a second updater in Swift and
  the daemon is what knows when Neko is idle). No report within an hour is
  recorded as a failure.
- **Tests**: `neko_core::self_update` (classification, list, decisions, idle
  gate, smoke, swap/rollback on temp dirs, Info.plist and report parsing),
  `self_update::tests` in the daemon (a temp git checkout through plan, pending,
  chat action, apply and the installer report), `web::tests::a_live_web_ui…`,
  and `scripts/verify/updates.mjs` (Settings section). The web harness seeds
  sample update history; `NEKO_WEB_SELF_UPDATE=1` + `NEKO_SELF_CHECKOUT` runs
  the real thread (used to verify build → swap → `WebUpdated` → failed build
  keeps the UI). `NEKO_SELF_UPDATE=0` switches the thread off. The install,
  relaunch and rollback were never run for real here (they replace
  `/Applications/Neko.app`); only the script's refusal path was exercised.

### Shipping a web change by hand (2026-10-27)

Why a web-only commit never went live on its own: the updater acts only on a
task's `landed_commit` or the owner's **Update Neko now**; a commit made by
hand (how the owner and his agents work on Neko) is never applied by itself,
and "Update now" is the owner's approval (it lets safety-critical paths
through and also reinstalls an app that is behind), so a script must not send
it. Each `install-native.sh` then dropped `web-live` (the app it installs is
newer), so after a week of hand commits `web-live` did not exist and every CSS
fix cost a ~10-minute reinstall.

`bash scripts/ship-web.sh` (after committing `web/`) is the path now, ~1 min:
it refuses unless everything changed since the installed app
(`NekoGitCommit`) is under `web/` or docs (`--force` overrides when the UI is
known not to need the new daemon code), waits for `machine-ok`, builds into
`<data>/web-live.ship`, smoke checks it like `smoke_check`, moves it to
`web-builds/<short sha>` and swaps `web-live` by one rename (`mv -fh`),
keeping the old target as `web-live.prev`; then it `curl`s `/` with the token
cookie and fails if the served script is not the new one. `--rollback` points
`web-live` back at `web-live.prev`. The daemon serves `web-live` per request,
so no restart; its self-update tick (≤10 s) **adopts** a build it did not make
(`core::live_web_label` → `resolve_commit`), records it as `web_commit` and
"went live (shipped by hand)", and sends `Event::WebUpdated` so open pages
reload. Adoption is what keeps the "newer app outranks the live UI" rule
working for hand-shipped builds. Nothing here touches `SENSITIVE` paths: the
script ships only `web/`, which has none, and changes to the daemon still
need an install (or the owner's "Apply update").
