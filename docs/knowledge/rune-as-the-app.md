## Rune as the Mac app: P5a-1 and P5a-2 (2026-11-19)

Code: `apps/neko-rune/src/shell/` (`mod.rs` the gate, the guard and the duties; `daemon.rs`, `status_item.rs`, `app_menu.rs`, `notify.rs`,
`uninstall.rs`, `open_in.rs`; P5a-2: `eventkit.rs`, `schedule.rs`, `calendar.rs`, `reminders.rs`, `paste.rs`), `src/link/shell.rs` (the requests the duties make), `src/deeplink.rs` (`neko_route`). Decision:
`docs/decisions/0002-rune-for-everything.md` (P5). The behaviour spec is the Swift app: `NativeLifecycle.swift`, `MenuBarIcon.swift`,
`StatusSummary.swift`, `NekoApp.swift` (the app menu and `StatusMenu`), `RoutineNotifier.swift`, `Uninstaller.swift`, `WebHome.swift`; P5a-2: `CalendarBridge.swift`, `PhoneBridge.swift`, `PreviousAppContext.swift`, `Palette.swift` (`restoreAndPaste`).

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
overlays were built at start and are attached then (`Overlays::stash`/`attach_pending`). **The guard goes both ways (P5a-2).** Each 10 s answer is compared with the last one. Swift quits: Rune takes the duties over (above). Swift
starts while Rune holds them: `App::set_swift_running` calls `release_duties` and Rune hands every duty back, logging `HANDBACK_LOG` once. The
status item is removed (`status_item::remove`, `removeStatusItem`), the application menu is emptied (`set_menu` with no items, so Rune's own
defaults stay), the summon key and ⌘⇧Esc are unregistered, the pill is hidden, the launcher is hidden and `Overlays::detach` puts what
`install` made back in the stash (the daemon's combo is kept, so `attach_pending` registers the keys again on the way back), the paste
tracker forgets the previous app, and the loops go quiet. The loops are threads that stay for the run and read one `AtomicBool` each
(`Switches`, followed from `duties()`): the daemon watch asks nothing and starts nothing (the daemon it started is left running), the
notifier forgets where it stood so that on return its first poll only records the list (nothing the Swift app showed is shown twice), the
Calendar and Reminders loops drop their timers and cycle at once on return. The read-only Settings row comes back with `swift_holds`.
`duties()` is still the one table; its tests now list every flag and Swift transition (`every_hand_back_transition_by_flag_and_swift`).

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
| Calendar | `calendar.rs`: see below. | `CalendarBridge.swift` |
| Reminders | `reminders.rs`: see below. | `PhoneBridge.swift` |
| Paste back and context | `paste.rs`: see below. | `PreviousAppContext.swift`, `Palette.swift` |
| Open Neko in | `open_in.rs`: Browser (the signed address, `SignedWebUrl`) or Neko window; **Rune's default is the window** (Swift's was the browser); a Settings row (General, after Launch at login) and the status menu's picker; saved in Rune's state (`open-in`). A start at login opens neither (`XPC_SERVICE_NAME=com.neko.launcher` or `NEKO_RUNE_AT_LOGIN=1`) and is windowless; with Browser chosen a start opens the browser once the address is known, the window after 15 s if it never is. | `WebHome.swift` |

Allow-list: `RoutineNotifications` is in `RUNE_EXTRAS` (named for `RoutineNotifier.swift`); `Ping` is the web's; `CancelAllWork` is sent only after the
uninstall confirmation. The saved-state count in `tests/allow_list.rs` is 14. `shell/daemon.rs` may ask whether the sibling binary is
an executable file (the readers test).

### P5a-2: Calendar, Reminders, paste back

All three are duties (`Duties::calendar`, `reminders`, `paste`): only in `--app` mode, never while the Swift app runs (its bridges would be a second
writer, and **Neko's Reminders list must never have two writers**), quiet when handed back. `Allow-list`: `SetCalendarEvents` (`CalendarBridge.swift`'s),
`CalendarStatus` (its switch check), `PhoneRemindersSync` and `PhoneRemindersDone` (`PhoneBridge.swift`'s) are in `RUNE_EXTRAS`, each with its Swift sender; they
are sent only from `src/link/shell.rs`. Neither bridge keeps anything in Rune's saved state (the `Prefs::open` count in `tests/allow_list.rs` is unchanged).

**EventKit and the bundle.** `objc2-event-kit` 0.3 (Zlib OR Apache-2.0 OR MIT, the objc2 family Rune already builds) with only the features the two
bridges call; no other new crate but `block2`, `objc2-application-services` and direct entries for `objc2-core-graphics`/`objc2-core-foundation`, which were already in
the tree. EventKit is touched **only** when the running executable is inside a `.app` whose Info.plist has the usage string
(`NSCalendarsFullAccessUsageDescription`, `NSRemindersFullAccessUsageDescription`; `eventkit::native::bundle_check`, through `NSBundle.mainBundle`, tested as
`usage_ready`). Otherwise one line on stderr (`not an installed app bundle, so Calendar is off`) and no thread; never a crash. Checked on 2026-11-19 with an unbundled
`--app` run against an empty data folder and `NEKO_RUNE_SWIFT_BUNDLE_ID=dev.neko.absent` (the guard looks for that id instead of Swift's; development only, so the
installed Swift app was never touched): all four skip or watch lines printed, and the process kept running until it was stopped. No live EventKit call was made, in a test or
by hand; the owner's real Calendar and Reminders were not read or written.

**One loop for both** (`schedule.rs`): a cycle at once, then after the wait the job names; a change notification starts (and each further one restarts) a 2 s settle, and the job
runs once for the burst; the longest sleep is 5 s so the duty flag is looked at. `Runner::step(now, active, changed)` returns how long to wait and is tested with a fake clock.
Because Rune has one thread per bridge, a change that arrives during a cycle is handled by a settle after it (Swift's `again` flag reruns at once); a difference nobody can see.

**Calendar** (`calendar.rs`). `CalendarSource` is EventKit, `CalendarDaemon` the daemon; `Bridge` is `CalendarBridge`'s methods. While `CalendarStatus.enabled` is false the
switch is looked at every 20 s and nothing is asked or read. Enabled: macOS is asked (only while access is undecided), then the events from local midnight 7 days ago to
35 days ahead (`clock::day_start`/`add_days`, so 23 and 25 hour days are right; at most 2500) go as `SetCalendarEvents { events, source: "macOS Calendar", access }` every 5 minutes and
2 s after the last `EKEventStoreChanged`. Fields as Swift: id `<identifier>@<start seconds>` (repeating events share an identifier), calendar title, colour as `#RRGGBB` through
`NSColor.usingColorSpace(.sRGB)`, times as rounded ms, trimmed location, the link (own `http(s)` URL, else the first meeting link in location then notes, else a location that is a web
link; links are found by `NSDataDetector`, as Swift's). Access: full is `Granted`, undecided `NotAsked`, everything else `Denied` (the daemon then drops its copy); access granted later
resets the store once.

**Reminders** (`reminders.rs`). `ReminderStore` and `PhoneDaemon`; `Bridge::run_cycle` is `runCycle`. Every 20 s and 2 s after a change: `PhoneRemindersSync { access, present,
completed, notes }` (notes only of reminders with a `>` reply line) then the plan is carried out in the "Neko" list (found by title, never made just to look; made on iCloud when there is
one, else the default source, else a local one): delete, create (title, notes, link, a due time and alarm 5 s ahead, a last notes line `neko:<seq>`), finish (the daemon's echo into
the notes, complete, or keep open with the reason), then `PhoneRemindersDone`. A plain completion is reported as completed (the daemon applies the default it promised in the title);
a `>` line is sent as the answer; the daemon decides the rest (`phone-push.md`). **Which reminder is which, across restarts:** Rune keeps no state. The daemon's `phone_state_v1` maps
an item's seq to the reminder's `calendarItemIdentifier` from `PhoneRemindersDone`, and the `neko:<seq>` marker (the last line with the prefix, so an echo after it does not hide it) finds an
open reminder again when the report was lost: before creating, the list is read and a reminder with that marker is reused, so a restart between saving and reporting, or the same item
arriving twice, makes no duplicate. A summary has no marker (seq 0) and is told apart by its `key`. A failed save is reported with an empty id.

**Paste back and the previous app** (`paste.rs`). `Tracker` keeps the last regular app that was not Rune (`NSWorkspace.didActivateApplication`, and the frontmost app at start) as a process id
only, and only while the duty is held. **Context:** `read_context` gives that app's name, the focused window's title and the focused element's selected text through Accessibility
**only when `AXIsProcessTrusted()` is already true**; Rune never prompts (Settings, Permissions, offers it), reads nothing in the background and takes no screenshot. `attach` appends it as
marked, untrusted data (4000 characters of selection, 200 of title, "(selection shortened)"). Used by **⌥Return in Neko's box** (`ComposerView.onSubmitWithContext`): with no previous app
or no Accessibility the draft is sent without context and a notice says why, in Swift's words. **Paste back, ⌥Return in the launcher:** hide the panel, `activate` the previous app, look up to
20 times 25 ms apart for it to be frontmost, post ⌘V (key down and up with the command flag, `CGEventPostToPid`); the three failure sentences are Swift's, shown as a notice. Swift
reaches this only after activating a clipboard row, where the daemon's `Activate` has already put the text on the pasteboard; **Rune's palette has no clipboard rows**, so its ⌥Return pastes
what is on the clipboard now, and nothing is restored afterwards (Swift does not either). Not seen live: a real ⌘V into another app, a real Accessibility read, a prompt.

### Not done here

- **P5b:** the bundle (an `.app` with `CFBundleURLTypes` for `neko://`, the usage strings `NSCalendarsFullAccessUsageDescription` and `NSRemindersFullAccessUsageDescription` that Calendar and Reminders wait for,
  the Accessibility and notification entitlements, signing), a Rune bundle that carries the daemon next to it, the login item that detects a login start from the launch Apple event, the installer, and
  the switch that makes Rune the default and retires the Swift app. Until then notifications, Calendar, Reminders and Uninstall are inert in a development run (no `.app`).
- Not seen live: a notification posting, `neko://` from the system, the browser branch of Open Neko in, the real Trash, EventKit (any call), a posted ⌘V, an Accessibility read, the hand-back with a real Swift app starting.
- Clipboard-history rows in the launcher (the half of paste back that restores the pasteboard).
