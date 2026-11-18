## Rune as the Mac app: P5a-1 (2026-11-19)

Code: `apps/neko-rune/src/shell/` (`mod.rs` the gate, the guard and the duties; `daemon.rs`, `status_item.rs`, `app_menu.rs`, `notify.rs`,
`uninstall.rs`, `open_in.rs`), `src/link/shell.rs` (the three requests the duties make), `src/deeplink.rs` (`neko_route`). Decision:
`docs/decisions/0002-rune-for-everything.md` (P5). The behaviour spec is the Swift app: `NativeLifecycle.swift`, `MenuBarIcon.swift`,
`StatusSummary.swift`, `NekoApp.swift` (the app menu and `StatusMenu`), `RoutineNotifier.swift`, `Uninstaller.swift`, `WebHome.swift`.

### The gate

`--app` (or `NEKO_RUNE_APP=1`) makes Rune act as the Mac app (`shell::gate`). It implies `--overlays`. Without it Rune is the plain
client it was: nothing in `shell/` is created, no thread starts, no menu item is added (`shell::tests`). The Swift app stays the default
until P5b.

### The Swift-running guard

Two apps must not hold one duty (two menu bar icons, two pills, two banners per item). In `--app` mode Rune asks
`NSRunningApplication` for bundle id `dev.neko.launcher` (not this process, so a bundled Rune may carry the same id) at start and every
10 s (`shell::spawn_guard`, `swift_running`). `shell::duties(app_flag, swift_running)` is the one table: no flag, nothing; the flag with
the Swift app running, nothing, and `swift_holds` shows a read-only row in Settings, Neko updates ("The Swift Neko app is running. It
keeps the menu bar, notifications and hotkeys until it quits."); the flag with no Swift app, everything. One log line each way on
stderr. When the Swift app quits Rune takes the duties over without a restart (`Shell::observe_swift`, `App::start_duties`); the
overlays were built at start and are attached then (`Overlays::stash`/`attach_pending`). **The takeover is one way**: a Swift app that
starts later does not take the duties back, so the two icons can coexist then (P5b makes it moot: one bundle).

Checked for real on 2026-11-19 with a stand-in app of that bundle id (the real Swift app was not running; it was never touched): while it
ran, `--app` printed the guard line, the menu bar had only the default menus and no status item (System Events: one menu bar); 10 s after
the stand-in quit the takeover line printed, the menu bar gained File, View and Work and a second menu bar (the status item) with
Swift's items. A start with `NEKO_RUNE_AT_LOGIN=1` opened no window and still held every duty; "Open Neko" in the status menu opened it.

### The duties

| Duty | Rune | Swift source |
| --- | --- | --- |
| Daemon watch | `daemon.rs`: `Ping` at start and every 5 s; if nothing answers and the child it started is not running, spawn the sibling `neko-daemon` (or `NEKO_DAEMON_PATH`); backoff `min(60, 2^failures)` s for failures 1 to 6, reset by an answer; never spawns while one answers, never kills, no sibling (a development run) means one log line and nothing. `Supervisor` over an `Io` trait is tested without a process. | `NativeLifecycle.swift` |
| Menu bar item | `status_item.rs`: `NSStatusItem` through `objc2-app-kit` (Rune has no macOS tray); the icon is an SVG cat head drawn in code and rasterised with `resvg` at 2x into a template `NSImage` (a filled dot when something needs you, a ring while work runs, in Swift's corner); the menu is a pure `menu()` model (two info lines, Quick panel, Open Neko, Settings…, "Open Neko in" radio submenu, Stop all work (⌘⇧Esc), Uninstall Neko… from an installed app only, Quit Neko ⌘Q), kept equal to an `NSMenu` in place. Counts follow `TaskStage` (waiting = AwaitingApproval + Failed + ReadyForReview). Stop all asks first (`stop_all_key`, the overlay's `confirm(` path). | `MenuBarIcon.swift`, `StatusSummary.swift`, `NekoApp.swift` |
| App menu | `app_menu.rs` through `AppHandle::set_menu`: File ▸ Quick panel ⌘K, the app menu's Settings… ⌘,, View ▸ "Appearance: System / Light / Dark" (Rune's menus have no submenus), Work ▸ Stop all work ⌘⇧Esc and Clear finished tickets (both ask first). | `NekoApp.swift` `.commands` |
| Notifications | `notify.rs` with `rune-notify` (same pinned rev): `RoutineNotifications { after }` every 30 s, at most 5 a poll then "N more things need you. Open the Inbox.", title `<scope> · <title>`, body the detail, no actions; the first poll only records where the list stands; `after` is kept in Rune's saved state (`notify-seq`); a click opens the Inbox; only from an installed bundle (`rune_notify::available`). `rune-notify` posts silently. | `RoutineNotifier.swift` |
| `neko://` links | `App::on_open_url` into `open_route`, the router `--open` uses; only the known heads (`deeplink::KNOWN_ROUTES`), anything else is ignored with one log line. A link that arrives before the guard has answered waits for the first daemon answer. | none (Swift has no scheme) |
| Uninstall | `uninstall.rs`: Swift's plan and words, its checkbox ("Also move Neko’s data to the Trash …", `Ask::toggle`), a coral button through `confirm(`; yes stops all work, unloads and trashes the login item, `pkill`s the bundle's own daemon by full path, resets Accessibility (`tccutil`), trashes the bundle and, if ticked, `~/Library/Application Support/neko`, then quits (or says what failed). `Runner` and `Trasher` are injected; every test runs on a temp folder with fakes. | `Uninstaller.swift` |
| Open Neko in | `open_in.rs`: Browser (the signed address, `SignedWebUrl`) or Neko window; **Rune's default is the window** (Swift's was the browser); a Settings row (General, after Launch at login) and the status menu's picker; saved in Rune's state (`open-in`). A start at login opens neither (`XPC_SERVICE_NAME=com.neko.launcher` or `NEKO_RUNE_AT_LOGIN=1`) and is windowless; with Browser chosen a start opens the browser once the address is known, the window after 15 s if it never is. | `WebHome.swift` |

Allow-list: `RoutineNotifications` is in `RUNE_EXTRAS` (named for `RoutineNotifier.swift`); `Ping` is the web's; `CancelAllWork` is sent only after the
uninstall confirmation. The saved-state count in `tests/allow_list.rs` is 14. `shell/daemon.rs` may ask whether the sibling binary is
an executable file (the readers test).

### Not done here

- **P5a-2:** retiring the Swift pill, palette hotkey and ⌘⇧Esc (the overlays are still two sets of keys while both run, the guard is what keeps them apart), making Rune the default at login.
- **P5b:** the bundle: `CFBundleURLTypes` for `neko://` (the handler is ready), the login item that detects a login start from the launch Apple event, the installer, and a Rune bundle that carries the daemon next to it. Until then notifications and Uninstall are inert in a development run (no `.app`).
- A Swift app launched after Rune took over is not handed the duties back.
- Not seen live: a notification posting (needs a bundle), `neko://` from the system, the browser branch of Open Neko in, the real Trash.
