# 0001. One interface: the web UI, a thin Swift shell, the Rust daemon

Status: **accepted 2026-11-16 (shape); execution pending the owner's choices. Superseded in part by
[0002](0002-rune-for-everything.md) the same day: the one interface becomes Rune, not the web UI
(the web UI stays the default until each Rune screen reaches parity). The inventory, the retirement
of GPUI and of the duplicate Swift screens, and the Swift shell's duty list still stand.**
Phase 1 (this record and the inventory) is done. Phases 2 to 5 below are
**pending the owner's choices**: nothing has been ported or deleted.

## Context

Neko grew three user interfaces over the same daemon:

| Interface | Where | Lines | State |
| --- | --- | --- | --- |
| React web UI | `web/` (served by the daemon at 127.0.0.1) | has every recent feature | installed, used daily |
| SwiftUI/AppKit app | `native/NekoKit` | 21,032 Swift (about 15k of it screens that duplicate the web) | installed as `/Applications/Neko.app` |
| GPUI client | `crates/neko` (own Cargo workspace, patched `gpui` fork) | 33,781 Rust | not installed; needs `scripts/setup-gpui-patch.sh` |

Every feature had to be built three times or was missing from one of them, and
each switch of the "main" interface broke something (GPUI to SwiftUI, then
SwiftUI to browser-first, which left a "Home (web)" trial page, a "Neko window"
fallback and a browser default side by side). Settings that exist twice already
disagree: the web's Permissions and Summon shortcut rows say "managed in the
native app", and the native Settings has no row for phone push, themes or the
update source that the web owns.

## Decision (owner, final, 2026-11-16)

- The **Rust daemon is the brain.**
- **The React web UI (`web/`) is the only interface.**
- A **thin Swift shell** does only what needs macOS: the window hosting the web
  UI (WKWebView, `WebHome.swift`), the menu bar item, the global hotkey, launch
  at login, EventKit (Calendar and Reminders bridges), notifications,
  permissions (Accessibility, Full Disk Access) and the update check.
- The GPUI client and the SwiftUI screens that duplicate the web are retired.

**The rule: a new screen goes in `web/`. Swift only for OS duties. No GPUI.** (Per 0002 this holds until
the Rune app reaches parity and becomes the default in its P5; the destination changes, the other two
parts of the rule do not.)

## Consequences

- One place to build and test each screen; `web-verify` covers all of it.
- The shell shrinks from 21k lines to what the duty list needs (estimate 4 to 5k).
- Anything the web cannot reach (global key, paste into another app, a folder
  chooser, permission status) goes through one small message bridge from the
  page to the shell; in a plain browser the bridge is absent and those rows
  say so.
- The daemon becomes the only owner of settings both UIs used to keep (for
  example the update source, today mirrored from a Swift `UserDefaults` value
  to the daemon on every connect).
- Features that exist only in Swift or GPUI are lost unless ported; the
  inventory says which and what each costs, so the owner decides.
- Licence: dropping `crates/neko` removes the only GPL-3.0-or-later dependency
  chain (`gpui -> sum_tree -> ztracing`) from the repository.

## Inventory

Layers: **S** = Swift app, **G** = GPUI client, **W** = web. Marks:
**(a)** the web already has it, **(b)** a shell duty that stays in Swift,
**(c)** only in Swift/GPUI, must be ported or dropped, **(d)** obsolete.
"Used?" is read-only evidence from the owner's data
(`~/Library/Application Support/neko/neko.db`, `defaults read NekoNative`,
`/Applications`) taken 2026-11-16; nothing was modified.

### (c) Only in Swift/GPUI: the owner decides keep or drop

| Feature | Layer | What it does | Size today | Used? (evidence) | Port cost |
| --- | --- | --- | --- | --- | --- |
| Hotkey quick panel (the "launcher"): app search, files, clipboard history, commands, modes (themes, usage meters, agents, terminals, schedules, ask Neko, new agent, settings rows) | S `Palette.swift` (642 + 50), G `panel.rs`, `modes.rs` | The summon key opens a floating panel; every row comes from the daemon's providers (`Search`, `Activate`), so the logic is already daemon-side | S about 700 lines; G about 9k | Weak. Hotkey never changed (no `hotkey*` row in settings). Daemon `launches` table: 3 apps, 4 launches in total, last 2026-11-14. Clipboard capture runs (200 entries, newest 2026-11-15) but nothing records pastes. The web's ⌘K already reaches pages, tasks and actions | **L**: a `#/palette` web route (about 700 lines), a second WKWebView panel in the shell (about 250 Swift), a daemon route serving app icons and clipboard images, a paste bridge |
| Paste into the previous app (CGEvent ⌘V) | S `Palette.swift` | After choosing a clipboard row, re-activates the previous app and pastes | 30 lines | Only reachable through the panel; no pastes recorded | **S** with the panel, pointless without it |
| Clipboard history browser (search, preview, delete) | S/G, inside the panel | Browses the saved entries | in the panel | Capture is on (`clipboard_history_enabled=true`, 200 entries); the web only has the on/off switch and cannot read the history | **M** as a stand-alone web page (about 250 lines, no paste) |
| Summon shortcut recorder | S `HotkeySettingsView` (about 60), G `hotkey_client.rs` | Records a combo and registers it system-wide | about 60 | Never changed (default combo, no stored row) | **S**: bridge call plus a web recorder |
| Permission status and buttons (Full Disk Access, Accessibility, "Allow…", open System Settings) | S `PermissionsView`, `FullDiskAccess` (about 180) | Reads TCC state, shows Allowed or Not allowed, opens the pane, triggers the Accessibility prompt | about 180 | The web row says "Managed in the native app"; dropping it leaves no in-app way to see or fix these | **S**: bridge returns two booleans and opens panes |
| First-run Full Disk Access banner | S `FullDiskAccessBanner` | One-time nudge above every page | 40 | `neko.fullDiskAccess.dismissed` not set in the app's defaults | **S** (a web banner reading the same bridge) |
| "Show activity while Neko works" toggle | S `PreferencesView` | Turns the floating status capsule on or off (the capsule stays, see (b)) | 10 | `neko.presence.enabled` not set: default (on) | **S**: bridge flag plus one web row |
| Folder chooser (NSOpenPanel) | S `FolderPicker`, 3 call sites | "Choose folder…" for workspaces, search folders, connections | about 36 | The web takes typed paths; chooser use not recorded | **S** |
| Send with context (⌥Return): attaches the previous app's name, window title and selected text | S `PreviousAppContext` (62), `TodayView` | Reads the selection through Accessibility on an explicit keypress | about 80 | Not recorded | **S**: bridge call plus a modifier in the web composer |
| "Open Neko in: Browser or Neko window" | S `OpenNeko` | Chooses where "Open Neko" goes | about 60 | `neko.openIn` not set: default (browser) | **S**, and moot once the window hosts the web UI |

If everything is ported: **1 L + 1 M + 8 S**, roughly 3 days. Dropping the
quick panel and clipboard history leaves **7 S**, about half a day.

### (b) Shell duties that stay in Swift

| Feature | Layer | Used? |
| --- | --- | --- |
| Window hosting the web UI (WKWebView, token sign-in, link policy) | S `WebHome` | yes (window frame saved) |
| Menu bar item, attention icon, status text, Quit | S `NekoApp`, `MenuBarIcon`, `StatusSummary` | yes |
| Global summon hotkey registration (default ⌥Space) | S `Palette.swift` | yes (default) |
| Global ⌘⇧Esc "stop all work" | S `StopAllWork` | not recorded |
| Calendar bridge (EventKit to daemon) | S `CalendarBridge` | yes (`calendar_events_v1`, `calendar_status_v1` rows) |
| Reminders and phone push bridge | S `PhoneBridge` | yes (`phone_state_v1`) |
| Routine and attention notifications | S `RoutineNotifier` | yes (`routine_seq_v1`, many `routine_runs_v1`) |
| Daemon launch and recovery | S `NativeLifecycle` | yes |
| Accessibility and Full Disk Access checks | S `FullDiskAccess` | yes |
| Update check, build stamp, Uninstall menu item | S `Updates`, `Uninstaller` | not recorded |
| Floating activity capsule while tasks run | S `Presence`, `Activity/` (about 520) | on by default |
| Previous-app tracking | S `PreviousAppContext` | yes (runs at startup) |

### (a) The web already has it (about 25 features)

Home chat and composer (slash commands, runtime and model pickers); Tasks
list, board, detail and thread; Pull requests; Agent tree; Workspaces;
Watching; Schedules; Tools, skills and the public MCP registry browser; Memory
and working style; Profiles; Rulebook; Settings (appearance and themes, AI and
agents, budget and limits, phone, quick-panel folders, updates and update
source, diagnostics, launch at login, clipboard on/off); first-run onboarding
(already web); Inbox and morning digest; Calendar page; canvas and artifacts;
usage and machine health; chat tool cards, reply views and rich text; stop all
work (action and ⌘K entry); in-app ⌘K palette for pages, tasks and actions.
Their Swift counterparts (about 15k lines: `TodayView`, `Tickets*`, `Ticket*`,
`PullRequestsView`, `AgentTree*`, `Composer*`, `SharedComposer`, `Chat*`,
`Reply*`, `Settings*`, `Tools*`, `Management*`, `PreferencesView`, `Watching/`,
`WorkingStyleViews`, `DiagnosticsView`, `MCPRegistryBrowser`, `SidePanel`,
`MachineStrip`, and the DesignSystem shells they use) duplicate these.

### (d) Obsolete

| Item | Why |
| --- | --- |
| The GPUI client `crates/neko` (33.8k lines, patched `gpui` fork, own lockfile, `scripts/setup-gpui-patch.sh`, `patches/gpui-*`) | Not installed: `/Applications` holds the Swift app and no GPUI binary exists. Its modes are the same daemon providers the (c) quick panel renders |
| GPUI-only window craft: panel drag and snap, edge fades, window material, comet motion | Exists only inside the uninstalled client |
| Swift Design lab, Activity gallery, Reply gallery | Developer previews (the last two only by `NEKO_START_PAGE`); the web has a UI gallery |
| "Home (web)" trial page and the Browser/Neko-window duality | Superseded by this decision |
| Native appearance "look" and View-menu appearance picker | The web owns themes (`appearance.mode=dark` is stored by the web) |
| Legacy "Agents" provider tab in native Settings | Not reachable from the Settings section list |
| Swift mirror of the update source (`neko.updates.repository`) | The web's "Updates come from" row and the daemon own it |

Counts: **(a) about 25, (b) 12, (c) 10, (d) 7.**

## Phases

1. **Decision record and inventory.** Done with this file.
2. **Parity** (port the (c) items the owner keeps). Pending the owner's choices.
3. **Retire** `crates/neko`, the gpui patch tooling, duplicate Swift screens and
   their tests; mark knowledge entries historical; update installers, the check
   gate and NOTICE. Pending the owner's choices (never before parity for a kept
   feature is committed).
4. **Wire contract cannot drift** (generated TypeScript types from
   `neko-protocol`, a stale check in `scripts/check.sh`). Pending the owner's choices.
5. **Web performance budget** (500-task list, long chat, heap bound in
   web-verify). Pending the owner's choices.
