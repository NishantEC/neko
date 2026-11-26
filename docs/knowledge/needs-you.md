# Needs you (2026-11-26)

One place that lists what waits for the person, and a Mac notification when something starts waiting. Protocol 6.

## What counts

Three things, and nothing else:

1. A permission request still pending.
2. A run that ended with a question: the task is `asked`.
3. A run that failed for a reason the person can act on: not signed in, the usage limit, the task's folder missing, the agent CLI
   missing. The failure's notice carries its kind (`FailureKind`). A crash, a timeout or a stall is a notice in the task only.

A finished task that needs nothing is done, not "needs you". An interrupted task is not either: a follow-up carries on.

## Where it comes from

The daemon derives the list from what it stores (`crates/neko-core/src/needs.rs`): pending permissions, the newest run of each
`asked` task and its last words, the newest error notice of each `failed` task. Nothing is stored of its own, so the list is the
same after a restart (tested). A notice written before version 6 has no kind; its own words say which it was.

- `list_needs` answers with the list. `needs_changed` pushes it whole when it changes. A thread in the daemon looks again after a
  task's state changes, a request changes or a task is removed (a few at once are taken together), and pushes only a list that
  differs.
- Order: oldest first, grouped by workspace. The workspace whose item waited longest comes first.
- Each item carries its task's cat and the cat's name (`cats.md`); the list leads with them ("miso asks"), and a rename pushes the
  list again.
- An item's id is stable while it waits: `perm:<id>`, `question:<run id>`, `failure:<part id>`.
- A question is the last paragraph of the run's words, without Markdown's marks, cut to the sentences at its end that fit 200
  characters.
- A failure's line to copy is the first code span of what to do next (`claude auth login`).

The app holds no list of its own. It hides an item it answered at once, until the daemon's list drops it.

## Answering

- A request: `answer_permission`, the same request the task's own control sends. A second answer is refused (`already_answered`).
- A question: `follow_up` with `answering` set to the run. The daemon refuses it unless the task is still `asked` on that run and
  no run has started; so a second answer, or an answer to an older question, never reaches the agent.
- A refused answer says "That was already answered." in the list, or in a notification when Neko is not in front.

## The list (⌘2)

A row under Home in the sidebar, with the count. ⌘1 is Home. View has both, with their keys.

- One column. A group per workspace, its name as a small heading. Each item: a mark, the task's title, how long it has waited,
  what is asked in a line or two, and its answers. The row's description says the workspace and the wait.
- A request: Allow once, Always allow, Deny, the words and looks of the task's Confirmation. The destructive form has no Always and
  no default. Confirmation itself is not used: Escape inside it denies, and here Escape goes back.
- A question: a reply box and Send.
- A failure: what happened, what to do next, and the sign-in or install line with Copy.
- Open task, on every item, goes to the task (where the item waits: its newest part).
- Keys: the oldest item has the keyboard when the list opens. Up and Down move between items, across groups, and stop at the ends.
  Return gives the default: Allow once, the reply box, or the task for a failure; nothing for a destructive request. Escape goes
  back to the task the person came from, or Home. Tab reaches each item's own controls. After an answer, the next item takes the
  keyboard.
- Empty: "Nothing needs you right now."
- The rows are `listitem`s with their place in a named `list`. When an item leaves the list, the live region says how many are
  left; a task that starts needing the person is announced as before.

## The sidebar and the Dock

- Each task row keeps its one mark. A workspace section's count is how many items wait there (not what is working).
- The Dock badge is the number of items (`WindowHandle::set_taskbar`), cleared at zero, off with its switch.

## Notifications

`apps/neko/src/notify.rs` is the model as data; it decides, and a `Poster` posts. The real window hands in `System` (Rune's
`rune-notify`, `UNUserNotificationCenter`). Everything else, tests and pictures included, keeps a `Recorder`: nothing there reaches
the system.

When each is posted:

- An item the app has not seen before, while "When a task needs you" is on, unless Neko is in front and the person is in that task
  or in the list. Title: the task's title. Body: the cat and what is asked ("miso asks: …"), at most 140 characters.
  - A request: "Run a command: cargo test", with Allow once (first), Always allow and Deny as actions. A destructive request has no
    actions: its notification only opens the task.
  - A question: "mochi asks: " and the question, with a reply field ("Your answer", Send).
  - A failure: "tofu could not go on: " and its words. No actions.
- A run that finished done (status from working to done), while "When a task finishes" is on, unless Neko is in front on that task:
  the cat's name, " is done: " and the first line of the answer ("tofu is done: …").
- A stale action or reply: "That was already answered."

Rules:

- One notification per task at a time (id `neko.task.<id>`); a newer one replaces it. Never more than one per item. What was
  waiting when the app started is not news.
- Taken back when its item leaves the list (answered anywhere), and when the person opens the task.
- An action or a reply sends the same request the list sends, so every rule above applies unchanged.
- A click on the notification opens Neko at the task, through the same route as the link below.
- No sound: `rune-notify` posts silently. Focus decides what shows; Neko does nothing about it.
- Neko asks for permission the first time it posts (the system asks, once), never at launch. It reads whether it may post with
  `getNotificationSettingsWithCompletionHandler:`, which shows the person nothing: at the first frame, when Settings ▸
  Notifications opens, after a test, and when Neko comes back to the front.

Settings ▸ Notifications: three switches (when a task needs you, when a task finishes, the Dock count), all on by default, kept in
the app's prefs (`notifications`). The system's permission in words, with the one step when it is off (and a button that opens
System Settings at Notifications). Send a test notification.

## The link

`neko://task/<id>` opens Neko at the task (`apps/neko/src/deeplink.rs`). The bundle declares the scheme (`scripts/build-app.sh`,
`CFBundleURLTypes`); Rune hands each link to `on_open_url`, the one that launched the app too. It is read strictly: one scheme, the
`task/` path, an id of the daemon's shape. A link that comes before the task list waits for it; one for a task that is gone goes
Home with a quiet line.

## What `rune-notify` cannot do at this rev

- **No thread identifier**: notifications cannot be grouped by task in Notification Center. Neko keeps one per task instead.
- **No permission status**: it cannot say whether the person allowed notifications. Neko asks the system itself (above).
- **No destructive action style** and no foreground option on actions. Neko puts no destructive action on a notification.
- **Silent only**: no way to ask for the system's default sound. Neko posts none, which the rules allow.
- **Only a bundled app posts.** The development copy that `scripts/show.sh` starts is a bare binary: it posts nothing, gets no
  links, and Settings says it cannot send notifications. Only the installed app does.
- A click that launches a closed app arrives through the delegate Rune installs at launch; Neko installs its handler before the
  window starts, and an action that arrives before the daemon answers is done once it does. Only a real run can confirm this.

## Not here, on purpose

Phone push, digests, quiet hours, dismissing an item without answering it, per-workspace switches.

## Tests

- `crates/neko-core/tests/needs.rs`: each kind and each way it ends, the failures that count and those that do not, the order and
  the groups, a restart.
- `crates/neko-daemon/tests/needs.rs`: the list and its pushes, answering a request and a question once, a stale answer refused, a
  failure kept across a restart, a folder gone before the run.
- `apps/neko/src/notify.rs`: which notifications each change posts, their actions, withdrawal, the front and other-task rule, the
  switches, reading a click.
- `apps/neko/tests/needs_you.rs`: the list's keys and structure, the counts and the badge, the link, and the whole flow against a
  real daemon with the fake CLI, notifications recorded.
- `crates/neko-protocol/tests/round_trip.rs` pins the shapes.
