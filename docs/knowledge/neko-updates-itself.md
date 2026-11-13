## Neko updates itself (2026-10-22, rewritten 2026-11-13)

Owner (2026-11-13, verbatim): "this repo is supposed to be something that can
publicly exist and can be used by anyone… we will have 2, one that is to develop
and one that is to use ourselves. And the one that we use ourselves will be
continuously updated from the one that is publicly available. We are just using
it. Do not introduce other stuff into it."

**Before (2026-10-22 to 2026-11-12), one paragraph.** The installed app built
itself from Neko's own development checkout (`land_on_main` /
`NEKO_SELF_CHECKOUT`) whenever a Neko task's `landed_commit` appeared there; a
range touching `SENSITIVE` paths waited for a click, and a hand commit was never
applied (hence `ship-web.sh`). That mixed the copy people use with the copy
people edit, and it could not work for anyone without a checkout.

### The two copies

- **Use copy**: the installed app, built from a clean clone **the updater owns**
  at `<data dir>/source`, cloned from the configured public repository over https
  with no credentials. Nothing else in Neko writes there (the only writers are
  `sync_clone`, `reset_clone` and the build tools run from it; it is not a
  workspace, and the agent prompt says never to edit it), so the updater resets
  it freely and **never commits or pushes** in it.
- **Development copy**: whatever checkout the owner works in. The updater never
  writes to it (a test asserts its HEAD, status and log are unchanged). The only
  reads are `git rev-parse <commit>^{tree}` there and in the app's recorded
  `NekoSourcePath`, to tell whether an app built from unpublished history matches
  the published tree (below).

### How the use copy updates

Decisions are pure in `neko_core::self_update` (tested); the I/O is
`crates/neko-daemon/src/self_update.rs`, one thread woken by the update commands
and otherwise every 10 s. State is `Snapshot::self_update`.

1. **Look** (`refresh`): at most every 10 minutes (`CHECK_EVERY_MS`), at once for
   **Update now** or a missing clone. One `git ls-remote` (30 s limit); only when
   `main` moved, `git fetch` (5 min) and `reset_clone`: `checkout -f -B main
   origin/main`, `reset --hard`, `clean -fdx -e node_modules -e .build` (the
   dependency folders are the only thing worth keeping: pnpm's `node_modules`
   stays inside the clone, Swift's `.build` too). A full single-branch clone
   (`--no-tags`), not shallow: the range logic diffs from the app's own commit,
   which a shallow history may not hold, and the repository is ~50 MB. **No work
   while the Mac says WAIT** (`machine-ok`, or the steward's hold) unless the
   owner clicked Update now. An unreachable source is the last check's `error`
   (offline is normal), never a failed update; with no clone yet it retries after
   a minute, not every tick. The network git calls ignore the person's own Git
   configuration (`GIT_CONFIG_GLOBAL=/dev/null`: no ssh URL rewrites, no
   credential helpers) and never prompt. The clone folder is refused when it is
   the development checkout or a real worktree.
2. **Starting point** (`starting_point`): the recorded `applied_through` if it is
   in the published history; else the running app's `NekoGitCommit`; else, when
   the app's commit is not public (rewritten history), the published tip **if the
   app's tree equals the tip's tree** (adopted, no reinstall); else the tip is
   applied once. An app built from commits *ahead* of the published tip (found in
   the development checkout) is newer, not older, and is left alone. No installed app (a development run): the tip is the start.
3. **Decide** (`decide`): docs-only (`.md`) ranges do nothing; web-only ranges
   rebuild the UI; anything under `crates/ native/ scripts/ patches/` or the
   Cargo files reinstalls. **Updates apply automatically by default, `SENSITIVE`
   paths included**: the source is the published `main`. The list stays, as
   information (the plan names those paths and the Settings row shows them).
   Settings → Neko updates keeps the two switches; with one off, a plan that
   needs that part becomes `pending` and waits for **Apply update** (or Update
   now); switching it back on releases it.
4. **Apply from the clone**. A queued plan is retargeted to the clone's current
   tip if the clone moved while it waited, and a dirty clone is reset first.
   - *Web*: `pnpm run build --outDir <data>/web-live.next` in the clone
     (`pnpm install` first when the lockfile changed; waits for `machine-ok`),
     smoke check (index.html loads a script and every `/assets/…` exists), then
     `web-live`, a symlink into `<data>/web-builds/`, is replaced by one
     `rename`, the old target kept as `web-live.prev`. `web::web_roots` reads
     per request: live, prev (a tab that loaded the old page can still fetch its
     chunks), then the bundle. A failed build keeps the current UI and records
     why. `Event::WebUpdated` reloads open pages when nothing was typed for 5 s.
     A newer installed app outranks an older live UI.
   - *Native*, when idle: no task worker, publish or chat reply, `machine-ok`
     fine, clone at the target commit. The daemon cannot replace itself, so it
     starts `scripts/self-update.sh` of the clone with `/bin/bash` in **its own
     process group**, stdout to `<data>/self-update/install.log`, with
     `CARGO_TARGET_DIR` set, and does not wait. `install-native.sh` (checks
     first, `CARGO_BUILD_JOBS=2`) stops the app, installs, the script relaunches
     `/Applications/Neko.app` and health-checks it (web `Ping` → `Pong`, the
     snapshot loads, `GET /` is 200, the installed `NekoGitCommit` is the
     target), restores the newest `/Applications/.neko-native-backup.*` on
     failure, keeps one backup on success, and writes
     `<data>/self-update/result.json`, which whichever daemon runs reads once
     (`take_result`): "updated", "rolled back", or "failed". No report within an
     hour is a failure. A failed web build, a failed install or a rollback is
     recorded and that commit is not retried until `main` moves or the owner
     clicks Update now; after a native failure the next range is computed from
     the running app, so the UI never outruns the daemon.
5. **Where things live.** Clone: `<data dir>/source`. Cargo's output
   (`NEKO_UPDATE_TARGET_DIR` overrides): `~/Library/Caches/neko/target`, which
   `build-native.sh` and `install-native.sh` honour through `CARGO_TARGET_DIR`
   (the bundle is `$CARGO_TARGET_DIR/native/Neko.app`); never in the data folder.
   The last check (`workbench_self_update_check_v1`, deferred) is a timer value,
   so it has a row of its own and `encode` strips it from the snapshot row.
   Disk: one clone, one target, no per-update copies.
6. **Settings**: Settings → About → "Updates come from" (UserDefaults
   `neko.updates.repository`, default `Updates.defaultRepository`) is the one
   source setting. The app sends it to the daemon (`SetUpdateSource`, on every
   connection and on submit; `Updates.normalizedRepository` and
   `neko_core::self_update::normalize_repository` agree), the daemon reads
   `<owner/name>` → `https://github.com/<owner/name>.git`, and a changed source
   re-points the clone's `origin` and fetches. The native "Update now" button
   sends `UpdateNekoNow` instead of pulling anything itself.
   `NEKO_UPDATE_REMOTE` and `NEKO_UPDATE_SOURCE_DIR` override the remote and the
   clone path for tests and development; `NEKO_SELF_UPDATE=0` switches the
   thread off.

Not a security boundary: `SetUpdateSource` is an ordinary daemon command like
the rest, so a full-access agent or page that can reach the daemon could point
the app at another repository (the app puts its own setting back on its next
launch). The old click on safety-critical paths had the same limit.

### Neko tasks that change Neko

They work in the **development checkout** (Settings → Neko updates →
"Development checkout", `SelfUpdate::dev_checkout`, `SetDevCheckout`; empty by
default, so a friend's install has none). A task whose repository is that
checkout lands on its local `main` after review (as `land_on_main` always did)
and then `native_runner::publish::push_to_update_source` pushes `main` to
`origin` as a plain fast-forward, only when `origin` is the repository the app
updates from and the checkout's `main` held nothing unpublished before the
landing (it fetches first; otherwise it says to push those commits yourself). The ticket says which happened: pushed ("the installed app picks
it up within about 10 minutes") or not pushed with the reason (no `origin`, a
different origin, no credentials, a non-fast-forward), in which case the change
stays local and the use copy is unaffected. The change returns to the use copy
through the updater like any other commit. Landed commits no longer trigger an
update. The agent prompt for "Change Neko" names the development checkout and
tells agents never to edit the clone. **Migration**: the first pass after the
update moves the old `self_update.checkout` (it was the development checkout)
to `dev_checkout` once, and `checkout` becomes the clone.

### Shipping a web change by hand (2026-10-27, now secondary)

`bash scripts/ship-web.sh` (after committing `web/` in the development
checkout) is a ~1 min preview of unpublished web work: it builds into
`<data>/web-live.ship`, smoke checks, swaps `web-live` by one rename (`mv -fh`)
keeping `web-live.prev`, and `curl`s `/` with the token cookie. `--rollback`
points `web-live` back. The daemon adopts a build whose commit the clone knows
(`live_web_label` → `resolve_commit`: only published commits resolve), records
"went live (shipped by hand)", and the newer-app-outranks-older-UI rule still
applies. An unpublished label is simply not adopted. The updater swaps in the
published build when `main` next changes, so push first and let the updater
ship when the aim is the use copy.

### Tests

`neko_core::self_update`: classification, list, decisions (sensitive paths apply,
switches pend), idle gate, smoke, swap/rollback, plist and report parsing,
`normalize_repository`, `Source::configured`, clone/reset/clean against a local
bare repository (keeps `node_modules`, undoes a stray commit and edits, refuses a
folder that is not a clone), `starting_point`. Daemon `self_update::tests` drive
the whole loop against a local bare "public" repository with stubbed build,
installer and `machine-ok`: clone, a new commit not seen before the next check,
a web-only apply, a docs-only no-op, a native range making the install call, a
sensitive path applying, auto-update off then Apply, retargeting a waiting
update and resetting a dirty clone, WAIT skipping the check, adoption of an app
with the same tree, apply-once for an app that matches nothing, the migration,
an unreachable source, and the development checkout's HEAD, status and log
unchanged throughout. Tests never fetch from GitHub.
`scripts/verify/updates.mjs` covers Settings (the harness seeds sample history;
`NEKO_WEB_SELF_UPDATE=1` runs the real thread). The install, relaunch and
rollback were never run for real here (they replace `/Applications/Neko.app`);
only the script's refusal path was exercised.
