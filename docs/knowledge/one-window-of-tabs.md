## One window of tabs (2026-11-06)

`DESIGN.md` §1, §3, §4, §5 in the web app. Owner, same day: no Canvas gallery
page, and Neko's own right-click menu everywhere.

- **Tabs ride on the hash.** `web/src/shell/tabs.ts` watches `hashchange`; the
  router, `navigate(route)`, `host.navigation.navigate` and plain `<a href>`
  are unchanged, so plugins need nothing new. Each location has a key: a page
  is its route; `#/tasks?task=<id>&view=full` is `task:<id>`,
  `#/workspaces?workspace=<id>` is `workspace:<id>`, `#/artifact?id=<id>` is
  `artifact:<id>`, `#/new` is the launcher. Other query keys belong to the
  page's tab (`#/tasks?task=<id>` is the sheet beside the list, in the Tasks
  tab). A new key opens a tab right of the current one; an open key focuses
  its tab (the active tab wins when duplicates share a key); leaving the
  launcher replaces it. `taskHref` now points at the task's own tab, so the
  palette, attention toasts, PR rows and chat links open tabs.
- **Back/forward are browser history.** Each entry is stamped with
  `nekoIndex` in `history.state` (sessionStorage keeps the max) so the buttons
  know when they can go. A closed tab's place reopens it. Clicking a tab sets
  the hash, so it is a history step too.
- **Only the active tab renders.** Pages read the global hash and register
  window key handlers, so keeping hidden tabs mounted would make them react to
  another tab's address. State that must survive lives in stores (chat,
  drafts, canvas target) already.
- **Persisted** in localStorage `neko.tabs.v1` (ids, hrefs, pinned). Home is
  pinned first and cannot close. The address the app opens at always wins.
- **Keys:** ⌘T launcher, ⌘W close, ⌘1–8 / ⌘9 last, ⌘[ ⌘] back/forward,
  ⌘⇧[ ⌘⇧] neighbour; Ctrl works too (the verify uses it: ⌘W/⌘T would hit
  the shared Chrome). The strip is a tablist with roving tabindex: ← → Home
  End move focus, Enter/Space opens, Delete closes; handled keys stop
  propagating so page shortcuts never see them. Drag reorders (HTML5 DnD).
  In the native app WKWebView offers key equivalents to the page first.
- **Sidebar is a launcher**, collapsed to its icon rail by default
  (`neko.sidebar.open`). Its rows' right-click: Open / Open in a new tab.
- **Right-click menu** (`shell/context-menu.tsx`): one Base UI ContextMenu
  wraps the app; a capture handler picks the items of the nearest element
  carrying `data-context-menu` (`useContextMenu(() => items)` or
  `<WithContextMenu>`). Selected text adds Copy / Ask Neko about this (Home's
  composer prefilled with a quote, `askNeko` in `shell/actions.ts`) / Search
  (⌘K prefilled). Nothing offered: back, forward, reload, new tab, ⌘K. Text
  fields keep the browser menu unless the field offers items. Shift+F10 and
  the context-menu key dispatch a `contextmenu` on the focused element.
  Duplicate labels are dropped. Items must call the visible buttons' code.
  Not reachable: inside an artifact frame (a separate sandboxed document).
- **One status language** (`web/src/ui/status-glyph.tsx`, mapping documented
  there): Queued to do; Planning/Building/Reviewing/AwaitingApproval in
  progress (amber; "Needs you" in words for AwaitingApproval); ReadyForReview
  review (sky); Completed done (mint); Failed failed (coral); Cancelled a
  faint cross; anything else backlog. `IdLabel`, `ProgressRing`. Used by task
  list/board groups and cards, tabs, Inbox routines and items, Workspaces'
  routine summary (`setup/routine-status.ts`), the crew header.
- **Crew in chat** (`chat/crew.tsx`): "Neko · <model> · Working on NEK-… · n in
  crew" from `runtime_selections`/`conversation_runtime` and running tasks;
  handoff lines from `metadata.tickets`, `neko_task_action` parts and routine
  briefs; "Hide crew" (`neko.chat.hideCrew`). Roles are inferred from status
  (Planning Scout, Building Builder, Reviewing Reviewer): the daemon records
  no explicit handoff event yet. The snapshot carries no routines, so running
  routines are not in the count.
- **References** (`chat/references.ts`, one recogniser for the markdown and the
  message menu): `NEK-XXXX` when exactly one task has that prefix, full task
  ids in the snapshot, `artifact <12-hex> [v<n>]`; code and existing links are
  skipped. They render as Iris links to the tab.
- Verify: `NEKO_WEB_ONLY=tabs node scripts/web-verify.mjs` (and `canvas`). An
  old `web_harness` without `SaveArtifact` fails the canvas area; point
  `NEKO_WEB_HARNESS` at a current build.
- **Done, not yet seen** (2026-11-08, herdr's unseen `done`): `neko_core::task_seen`
  keeps, per task, the status the person last saw it in, in one quiet row
  (`task_seen_v1`, `Db::set_setting_quiet`, never the snapshot). A finished task
  (ReadyForReview, Completed, Failed) is unseen when there is no record or it
  names another status. The first read records every finished task as seen (no
  flood on rollout); seen ReadyForReview covers Completed; a record for a task
  that runs again is dropped on the next read, so fail → restart → fail flags
  again (only if some page asked while it ran). Requests `UnseenTasks`,
  `MarkTaskSeen { task_id, seen }` (false = Mark as unread); the mark broadcasts
  `Event::UnseenTasksChanged` so other windows agree. Web: `work/seen.tsx`
  (`useUnseen`, `useMarkSeen` in TaskSheet and TaskDetail, `UnseenDot`), asked
  again 250 ms after each new snapshot and on window focus. Verify:
  `NEKO_WEB_ONLY=unseen`.
- **Toasts are dialogs.** Base UI toasts are `role=dialog` (non-modal) and carry
  `data-slot=toast`. A key handler that bails while a dialog is open must
  exclude them (Tasks' j/k did not, and a toast left open by one verify area
  broke the next one's "j focuses the first task"). The verify runner closes
  leftover toasts between areas.
