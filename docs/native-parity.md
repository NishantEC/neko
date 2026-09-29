# Native client parity acceptance matrix

Baseline: source inspection of `crates/neko/src` on 2026-09-29. This is a migration checklist, not evidence that the replacement implements these behaviors. Every row is **not yet verified in the native client**. Change a row only with a command receipt or real running-window evidence; a compiling Swift view is not interaction proof.

The current source is authoritative: onboarding has **three** visible steps (`Welcome`, `Mac`, `Workspace`), not the six/14 steps described by historical documentation. Default workspace navigation is Today, Tickets, Responsibilities, Memory, Workspaces, Tools, and Profiles. Skills and schedules are inside Tools. Legacy provider modes are distinct from this default workspace.

## Default workspace

All commands below are wrapped in `Request::Workbench`. Snapshot data remains daemon owned; native views must not open the SQLite database.

| Surface | Required behavior / acceptance | Commands | Native status |
| --- | --- | --- | --- |
| Window / navigation | Persistent regular window; dock reopen/menu reopen reuse it; selected workspace and selected ticket stay coherent; no accidental cross-workspace ticket selection | `Snapshot` | Not verified |
| Refresh / errors | Initial loading, periodic/event refresh, stale/disconnected banner, actionable daemon errors, busy state; preserve input on failed mutation; queue user actions during polling without duplicate dispatch | `Snapshot`, all mutation responses | Not verified |
| Today | Greeting, scoped brief/counts, attention/working/completed groups, ticket shortcuts, schedule countdowns and empty states | `Snapshot` | Not verified |
| Chat | Profile/workspace-scoped history, markdown replies, composer/send, running state, tool-call status and results, explicit allow/deny, cancellation; notes must never grant tools | `SendMessage`, `DecideChatTool`, `CancelChat` | Not verified |
| Tickets | Status filters; selected ticket goal, plan, risk/assessment, result, reviewer outcome/evidence, timeline/events, worktree information and notes | `Snapshot`, `AddTicketNote` | Not verified |
| Ticket actions | Status-gated plan approval, cancel, retry, mark complete; show failure without inventing completion | `ApproveTask`, `CancelTask`, `RetryTask`, `CompleteTask` | Not verified |
| Parallel proposal | Read-only proposal at awaiting approval; display proposed/approved children, file scope, child statuses; approval stays explicit through normal task approval | `ProposeSplit`, `ApproveTask` | Not verified |
| Responsibilities | List by workspace, instruction/scope, enabled state, source/eligibility/status information, wake now, edit; prepare-local-fix is explicit and defaults off | `Mcp(SaveResponsibility)`, `Mcp(Wake)` | Not verified |
| Memory | Profile/workspace/decision scopes, create/edit/delete, accept/reject proposed memories; edit preserves original entry identity and scope | `SaveMemory`, `DeleteMemory`, `DecideMemoryProposal` | Not verified |
| Workspaces | Native folder chooser; add/select existing workspace; editable name/instructions; attach multiple folders without duplicates; advanced settings and scope stay attached to original workspace | `SaveWorkspace`, `SaveWorkspaceWithFolders` | Not verified |
| Profiles | Create/edit profile name and instructions, set active profile, assign workspace, explicit cross-profile read grants | `AgentProfiles(Save/SetActive/AssignWorkspace/SetReadGrant)` | Not verified |

## Tools, skills and schedules

| Surface | Required behavior / acceptance | Commands | Native status |
| --- | --- | --- | --- |
| Connections | Local executable plus JSON argument array, or remote HTTP URL; label, workspace/global scope, optional credential JSON/OAuth client ID; explicit local process trust | `Mcp(AddConnection)` | Not verified |
| Discovery/auth | Connection health/error, discover tools, authenticate, enable/disable; OAuth opens the real browser flow and credentials remain daemon/Keychain owned | `Mcp(Discover/Authenticate/SetEnabled)` | Not verified |
| Tool grants | Display schema/tool details; per-workspace tool permission and explicit unattended authorization; no grant inferred from server annotation | `Mcp(SetWorkspaceToolGrant)` | Not verified |
| Found connections | Read-only discovery once per selected workspace, refresh, source grouping, global/current-workspace filtering; local process consent before linking; report unavailable sources | `SetupImport(Discover)`, `Mcp(LinkSource)` | Not verified |
| Skills library | Refresh; search by name/description/source; enabled-first/collapsed list and browse more; scope-aware enable/disable; unavailable skill recovery | `Skills(Refresh/SetEnabled)` | Not verified |
| Skill repository | Preview source repository; show exact proposed content and audit evidence; record required audit review before accept; hash-bound accept/reject | `Skills(PreviewRepository/ConfirmAuditReview/DecideProposal)` | Not verified |
| Schedules | List, create/edit, enable/pause, run now, remove, next run/error details; editing must retain original workspace even if selection changes | `Schedules(Save/SetEnabled/RunNow/Remove)` | Not verified |

## Onboarding and preferences

| Surface | Required behavior / acceptance | Commands / ownership | Native status |
| --- | --- | --- | --- |
| Setup lifecycle | Welcome → Your Mac → Workspace; back, skip/setup later; completion only after daemon acknowledgment; failed completion leaves window recoverable | `GetOnboardingState`, `SetOnboardingComplete` | Not verified |
| Accessibility | Optional real macOS ask, poll current grant, explain denied state; recorder stops when leaving Mac step | Native AX APIs; `DismissAccessibilityBanner` for palette banner | Not verified |
| Clipboard consent | Read current value first; off by default; wait for exact acknowledged value; suppress double activation | `GetClipboardHistoryEnabled`, `SetClipboardHistoryEnabled` | Not verified |
| Setup workspace | Folder picker validates directory; existing folder selects existing workspace; new selection registers only workspace; optional skip | `Workbench(Snapshot/SaveWorkspace)` | Not verified |
| Hotkey recorder | Canonicalize actual key/modifiers; ignore bare modifiers; Escape cancels; reject invalid/reserved combinations; register candidate before unregistering old binding; persist only successful binding | `GetHotkey`, `CheckHotkeyConflict`, `CommitHotkey`; native global registration | Not verified |
| Preferences General | Hotkey change and launch at login; real state readback and inline failures | `Search(provider: preference)`, `Activate(kind: preference, id: launch-at-login)` | Not verified |
| Preferences Search | Show folder scopes, add typed folder, remove existing scope, report errors | `Search(provider: folder-scope)`, `Activate(kind: folder-scope, id: path, action: add or null)` | Not verified |
| Preferences Agents | Source/count information, show-agents and include-idle toggles; legacy availability remains explicit | `Search(provider: agent/preference)`, `Activate(kind: preference, id: agents-enabled/agents-include-idle)` | Not verified |
| Preferences About | Product/version/about content; keyboard tab traversal, arrow tab switching, Enter/Space controls, Escape/window close | Native views | Not verified |

## Search palette and modes

`Search` includes `query`, `limit`, optional `provider`; consume streaming fast and full result frames. `Activate` includes exact `kind`, `id`, optional `action`, and current `query`. Never substitute display text for opaque IDs.

| Surface | Required behavior / acceptance | Commands / ownership | Native status |
| --- | --- | --- | --- |
| Summon/dismiss | Global hotkey toggles palette; hide with window `orderOut` only, never hide whole app; outside click and Escape; restore previous app for paste; active-display placement and Spaces/full-screen behavior | Native AppKit/global hotkey | Not verified |
| Root search | Applications/files/folders/settings/clipboard/commands; section headers, icons, subtitles/accessories, limits, no-match and delayed-loading state; slash command scope | Streaming `Search` | Not verified |
| Streaming selection | Superseded queries ignored/cancelled; fast results immediate; late results preserve existing row order/selection; no stale activation | Streaming `Search`, `Activate` | Not verified |
| Selection/actions | Arrow/page/home/end navigation, mouse hover/click, Enter, contextual primary/secondary actions, Cmd-K popover, action keyboard hints; activation error stays visible | `Activate`; `SearchItem.actions` and result dispositions | Not verified |
| Mode navigation | Respect `enters_mode`; back/Escape restore pre-mode query; maintain item/backend scope; unknown mode fails safely | Scoped `Search`; client mode state | Not verified |
| Clipboard mode | Filter history; selected text/image preview and application/content type/copied fields; restore/copy/paste behavior and keep-open/dismiss disposition | `Search(provider: clipboard)`, `Activate`; native paste lifecycle | Not verified |
| Themes | Filter palettes; selection live-previews complete palette and native light/dark material; Escape restores original; Enter persists; broadcast updates every window | `GetTheme`, `Search(provider: theme)`, `Activate(kind: theme)`, `ThemeChanged` | Not verified |
| Render content | App/file icon updates, selectable/readable markdown, code/tool/image content where supplied; scrollbars, detail panes, edge fades, skeletons; keyboard focus visible | Native views; `IconsUpdated` | Not verified |
| Disconnection | Clear stale-results banner; reconnect/refresh automatically, no frozen UI or false successful activation | Transport connection state | Not verified |

## Native ownership without a daemon command

| Capability | Acceptance boundary | Native status |
| --- | --- | --- |
| Text editing | Native IME/composition, UTF-8-safe selection, mouse caret/selection, Cmd-A/C/X/V, word/line deletion and movement, shift-selection, multiline composer behavior | Not verified |
| Paste bridge | Capture frontmost external app before summon, restore focus before synthetic paste, honor Accessibility denial, never paste into Neko itself | Not verified |
| Image attachments | Image paste persists content-addressed attachment and inserts a readable file reference; cache lifetime survives later transcript use; do not silently reduce to plain text | Not verified |
| Windows | Persistent workspace versus transient palette, singleton reopening, correct traffic lights, menu/keyboard shortcuts, draggable surfaces, screen-edge/center snapping and guides where palette supports them | Not verified |
| Appearance | Native materials/fallback, theme appearance, readable contrast, reduce-motion behavior, consistent focus and accessibility labels; no doubled shadow/rim regression | Not verified |
| Status menu | Summon, Open Neko Workspace, Preferences, Quit; attention badge and quota warning/menu readings update from daemon events | Not verified |
| Lifecycle | Launch bundled daemon safely, single app instance, reconnect after daemon restart, quit leaves daemon persistence semantics intact; development uses `NEKO_DATA_DIR` | Not verified |

## Hidden/opt-in compatibility surfaces

Daemon registration is gated by presence of `NEKO_LEGACY_AGENTS`; do not expose these as default capabilities simply because `modes.rs` still defines their chrome. Validate against actual registered providers.

| Mode / surface | Required compatibility if retained | Native status |
| --- | --- | --- |
| `codex-task`, `new-codex-task` | Backend-qualified task ID, actual available transcript or explicit unavailability, project selection, task prompt/create and backend-specific actions | Not verified |
| `agent`, `new-agent`, `conversation` | Paseo agent tiles/list, state/attention, project selection, prompt send, transcript/tool cards/images, approval/review actions, open-in-Paseo; never route Codex IDs to Paseo | Not verified |
| `terminal`, `ask`, `schedule`, `usage` | Terminal output details, explicit confirmation of proposed tool calls, legacy schedule toggle, actual quota meters/status; generic provider `Search`/`Activate` contracts | Not verified |
| Legacy Linear protocol | `ConnectLinear`, `SyncLinear`, `SetConnectionEnabled`, `PlanIssue` remain protocol compatibility, not current default UI; do not restore built-in Linear connector | Not a migration target |

`CreateTask`/`CreateTaskInFolder` are protocol-supported and handled in workspace response bookkeeping, but current home flow primarily creates tickets through chat. Do not confuse an enum variant with a visible button. Similarly, `Schedules(List)`, `Mcp(SetToolGrant)`, and older import apply commands are not a reason to invent new surfaces.

## Automated evidence annotations

The original row statuses mean **end-to-end acceptance is unverified**. The annotations below add narrower automated coverage; they do not promote those rows to complete. Evidence: [native-swift-verification.md](evidence/native-swift-verification.md), command `NEKO_TEST_DAEMON=/Users/nish/Documents/neko/target/debug/neko-daemon swift test --package-path native/NekoKit`, 2026-09-28 21:28:36 UTC. Later focused AppModel run after the Kanban change passed 13 cases (2026-09-28 21:31:11 UTC).

| Matrix row | Automated boundary established | Still outside this evidence |
| --- | --- | --- |
| Refresh / errors | Mutation-specific Bool acknowledgment; busy rejection; old refresh success/error cannot replace newer mutation; overlapping refresh sequence ordering | Live disconnect/recovery UI, all mutation forms |
| Workspaces | Actual isolated daemon accepts multiple non-Git folders and retains them after restart | Every native picker/edit/cancel path |
| Profiles | Daemon create/assign/read-grant persistence; name required but instructions may be empty | All grant controls and profile switching in live windows |
| Memory | Daemon memory save and restart persistence | Edit/delete/proposal controls and scope changes |
| Skills library | Pure scope-aware missing-skill detection and changed-grant retention tests | Real enable/disable of changed/missing files through UI |
| Schedules | Pure common-recurrence/custom-preservation tests; daemon save enforces paused state and survives restart | Live enable/run/remove and time-zone behavior |
| Setup lifecycle | Initial state acknowledgment and positive completion response required; false/malformed responses fail closed | Permission prompts and every step/back/skip interaction |
| Parallel proposal | Pure guard excludes already-split parents/children and non-awaiting tasks | Live proposal/approval/child navigation/integration review |
| Streaming selection | Socket fixtures deliver partial/complete responses, ignore events and handle fragmented frames | Current selection/activation behavior under live delayed results |
| Text editing | Keyboard routing leaves composition and modified editing keys to AppKit | Actual IME, selection, clipboard and undo interaction |
| Image attachments | TIFF normalization, content deduplication, file retention, invalid-image rejection | Actual paste and later model attachment consumption |
| Lifecycle | Bounded daemon recovery backoff and native owned-window-family helpers | Bundle launch/reopen, daemon crash/restart, OS focus and installed-app behavior |
| Status menu | Pure attention/working counts across all workspaces | Live menu updates; quota events remain a separate feature |

## Verification gates

### Native migration checkpoint, 2026-09-29

The Swift client now builds under `native/NekoKit`; `scripts/build-native.sh` packages it without replacing the installed app. A distinct preview bundle can use isolated `NEKO_DATA_DIR`.

Verified live in the preview: three-step onboarding with one window; native folder selection; creation of a non-Git workspace and sidebar readback; profile creation; memory creation; schedule creation saved paused. The initial nested modal folder picker failed live; the shared asynchronous sheet picker subsequently accepted the same directory. Management controls were changed to expose individual AX buttons, then profile creation was exercised successfully.

Automated transport and real-daemon fixtures cover framing, fragmentation, cancellation, deadlines, partial search, non-Git multiple folders, profile assignment, memory, paused schedules and restart persistence. AppModel tests cover mutation acknowledgments and stale response ordering. These checks are not complete UI parity, live OAuth/model proof, or a replacement-install acceptance.

New visual reference: see `native-visual-direction.md`. Remaining matrix rows above deliberately stay unverified until exercised. Palette IME, owned child-window focus, full hotkey vocabulary, native permission/paste and rendering details require further review.

1. Exercise framed socket transport against isolated real daemon: unsolicited events, multi-frame search, reconnect, error responses and concurrent request correlation. Preserve secrets outside logs.
2. Exercise every default matrix mutation with isolated data and confirm daemon readback after app restart; include failed mutation, duplicate clicks and scope changes while request is pending.
3. Capture real native windows for workspace, tickets, approval, tools, skills, schedules, memory, profiles, setup and palette; test actual keyboard/mouse behavior, not screenshots alone.
4. Test native-only behavior on macOS: two displays, full-screen Space, permission denied/granted, hotkey conflict, external-app clipboard paste, menu reopen and daemon restart.
5. Record each verified row with evidence path, exact build revision and limitations. Keep all untested rows explicitly unverified; compilation and deterministic daemon fixtures do not prove live OAuth/model/provider behavior.
