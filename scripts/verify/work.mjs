// Work pages: Tasks (list / board / tree, the open task), Pull requests and the
// Agent tree. Two passes, both with the page already signed in:
//
//  1. Against the TEST daemon's own data (the harness seeds pull-request tasks
//     and recent tasks from a saved snapshot): filters, layouts, j/k, opening a
//     task, a real RenameTask round trip, the destructive dialogs (opened, then
//     cancelled), ⌘K actions, the pull-request verdicts.
//  2. Against a richer snapshot (the same real one with running tasks, a split,
//     a follow-up, a pull request with conflicts and a failing check) served to
//     the page by intercepting its snapshot polls and `TaskChanges`; nothing is
//     written anywhere. This is how the tree, the running states, the thread,
//     the result with its review and the diff get seen.
//
// Every state is screenshotted in dark and light; a11y(...) runs on each.
import { existsSync, readFileSync } from "node:fs";

const SEED = process.env.NEKO_WEB_SEED ?? "/tmp/neko-real-snap.json";

const PATCH = `diff --git a/packages/utilities/src/http/stream-request.ts b/packages/utilities/src/http/stream-request.ts
index 3f2a1c4..9b7e0d2 100644
--- a/packages/utilities/src/http/stream-request.ts
+++ b/packages/utilities/src/http/stream-request.ts
@@ -18,9 +18,14 @@ export async function streamRequest(url: string, options: StreamOptions) {
   const response = await fetch(url, options.init);
   if (!response.ok) {
     throw new HttpError(response.status);
   }
-  for await (const chunk of response.body) {
-    options.onChunk(chunk);
-  }
+  try {
+    for await (const chunk of response.body) {
+      options.onChunk(chunk);
+    }
+  } catch (error) {
+    if (isInterrupted(error)) return options.onInterrupted?.(error);
+    throw error;
+  }
 }
diff --git a/packages/utilities/src/http/stream-request.spec.ts b/packages/utilities/src/http/stream-request.spec.ts
index 0000000..51c8e2a 100644
--- a/packages/utilities/src/http/stream-request.spec.ts
+++ b/packages/utilities/src/http/stream-request.spec.ts
@@ -40,3 +40,12 @@ describe("streamRequest", () => {
     expect(onChunk).toHaveBeenCalledTimes(2);
   });
+
+  it("reports an interrupted stream instead of throwing", async () => {
+    const onInterrupted = jest.fn();
+    mockStream([chunk("a")], new TypeError("network error"));
+    await streamRequest("/x", { onChunk, onInterrupted });
+    expect(onInterrupted).toHaveBeenCalledTimes(1);
+  });
 });
diff --git a/packages/utilities/src/sentry/sentry.constants.js b/packages/utilities/src/sentry/sentry.constants.js
index 7c1d9aa..e41b6f0 100644
--- a/packages/utilities/src/sentry/sentry.constants.js
+++ b/packages/utilities/src/sentry/sentry.constants.js
@@ -3,6 +3,7 @@ export const IGNORE_ERRORS = [
   "Failed to fetch",
   "Load failed",
+  "network error",
 ];
`;

/** A richer snapshot built from the saved real one, with the states the test daemon's data lacks. */
function richSnapshot() {
  const base = JSON.parse(readFileSync(SEED, "utf8"));
  const now = Date.now();
  const s = structuredClone(base);
  s.digest = "work-fixture-1";
  const tasks = s.tasks;
  const where = (status) => tasks.filter((t) => t.status === status);
  const ev = (role, message, ago) => ({ role, message, at_ms: now - ago });
  const receipt = (command, output, code) => `VERIFICATION_COMMAND ${JSON.stringify({ command, output, exit_code: code })}`;

  // Running work: one building (with a failing command), one planning, one queued.
  const [building, planning, queued, parent, childA, childB, follow] = where("AwaitingApproval");
  Object.assign(building, {
    status: "Building", worktree: "/tmp/neko-fixture/worktree", parked: null, updated_at_ms: now - 20_000, created_at_ms: now - 25 * 60_000,
    events: [
      ev("system", "Queued for planning.", 24 * 60_000),
      ev("scout", "Agent started in an isolated task worktree", 23 * 60_000),
      ev("scout", "The crash is in the logger: it serialises every breadcrumb, and a console breadcrumb can hold a circular object.\n\nPlan ready.", 20 * 60_000),
      ev("user", "Approved. Building in its own copy.", 18 * 60_000),
      ev("builder", "Agent started in an isolated task worktree", 17 * 60_000),
      ev("builder", receipt("/bin/zsh -lc 'rg -n GLOBAL_BREADCRUMB packages'", "packages/utilities/src/sentry/sentry.ts:98:  APP.GLOBAL_BREADCRUMB.push(b)\npackages/utilities/src/logger/logger.utilities.ts:17:  breadcrumbs: APP.GLOBAL_BREADCRUMB,", 0), 16 * 60_000),
      ev("builder", receipt("/bin/zsh -lc 'yarn jest packages/utilities'", "FAIL packages/utilities/src/sentry/sentry.spec.ts\n  ● drops console arguments\n    expected undefined, received [Circular]", 1), 9 * 60_000),
      ev("builder", "The first run failed because the breadcrumb copy still carries `data.arguments`. I am stripping it for console breadcrumbs only and re-running.", 8 * 60_000),
      ev("builder", receipt("/bin/zsh -lc 'yarn jest packages/utilities'", "PASS packages/utilities/src/sentry/sentry.spec.ts\nTests: 12 passed", 0), 2 * 60_000),
      ev("builder", receipt("cd \"/Users/nish/Library/Application Support/neko/task-worktrees/abc\" && yarn tsc --noEmit", "", 0), 60_000),
    ],
  });
  Object.assign(planning, { status: "Planning", updated_at_ms: now - 5_000, created_at_ms: now - 3 * 60_000, events: [ev("system", "Queued for planning.", 3 * 60_000), ev("scout", "Agent started in an isolated task worktree", 2 * 60_000)] });
  Object.assign(queued, { status: "Queued", updated_at_ms: now - 40_000, events: [ev("system", "Queued for planning.", 60_000)] });

  // A split: `parent` was split into two children; `follow` was proposed by `childA`.
  const done = where("Completed");
  const sub = (task, title) => ({ title, goal: `${title} in its own task.`, files: ["packages/a/src/index.ts"], tests: ["yarn jest packages/a"], depends_on: [], task_id: task.id, base: null });
  s.splits = [{ parent_id: parent.id, approved: true, integrated: false, subtasks: [sub(done[0], "Guard the missing client"), { ...sub(done[1], "Cover it with a test"), depends_on: [0] }] }];
  s.follow_up_of = { [follow.id]: done[0].id };
  const roots = ["/Users/nish/Documents/hme/athena", "/Users/nish/Documents/hme/navihealth"];
  for (const t of [building, planning, queued]) s.task_roots[t.id] = roots[0];

  // Pull requests: conflicts, a failing check, a handed-back count, a close suggestion, merge when ready.
  const open = tasks.filter((t) => t.pull_request?.status?.state === "OPEN");
  if (open[0]) Object.assign(open[0].pull_request.status, { merge_state: "DIRTY", mergeable: "CONFLICTING" });
  if (open[1]) Object.assign(open[1].pull_request, { status: { ...open[1].pull_request.status, checks: "failing", failing: ["Test", "Lint"], base_failing: true, merge_state: "UNSTABLE" }, follow_up: { count: 2, seen_comments: [] } });
  if (open[2]) open[2].pull_request.close_suggestion = "A fix for this exact crash is already open in #4622, so this one would only conflict with it.";
  if (open[3]) Object.assign(open[3].pull_request, { merge_when_ready: true, follow_up: { merge_note: "its checks are still running", count: 0, seen_comments: [] } });
  if (open[4]) Object.assign(open[4].pull_request.status, { review: "CHANGES_REQUESTED", draft: true });

  // A ready task whose changes the page can show.
  const ready = where("ReadyForReview").find((t) => t.result.includes("Independent review:"));
  if (ready) { ready.worktree = "/tmp/neko-fixture/ready"; ready.parked = null; }
  s.__ids = { building: building.id, planning: planning.id, queued: queued.id, ready: ready?.id, failed: where("Failed")[0]?.id, parent: parent.id, split: parent.id };
  return s;
}

export default async function ({ page, shot, a11y, check, sleep, PORT }) {
  const base = `http://127.0.0.1:${PORT}`;
  const both = async (name, after) => {
    for (const scheme of ["dark", "light"]) {
      await page.emulateMedia({ colorScheme: scheme });
      await page.waitForFunction((s) => document.documentElement.classList.contains(s), scheme, { timeout: 3000 });
      await sleep(350);
      await shot(`${name}-${scheme}`);
      await a11y(`${name} (${scheme})`);
    }
    await page.emulateMedia({ colorScheme: "dark" });
    await sleep(250);
    if (after) await after();
  };
  const rows = () => page.locator("[data-slot='task-row']");

  await page.setViewportSize({ width: Number(process.env.NEKO_WEB_WIDTH ?? 1440), height: 900 });
  await page.emulateMedia({ colorScheme: "dark" });

  // ===== 1. the test daemon's own data =========================================================
  await page.goto(`${base}/#/tasks`);
  await rows().first().waitFor({ timeout: 15000 });
  const total = await rows().count();
  check(total > 5, `Tasks (list) renders ${total} dense rows`);
  check((await page.locator("[data-slot='group-header']").count()) >= 2, "tasks are grouped by stage with counts");
  const subtitle = (await page.locator("[data-slot='work-subtitle']").innerText()).replace(/\s+/g, " ");
  check(/\d+ tasks?/.test(subtitle), `the header counts them: "${subtitle}"`);
  check((await page.locator("[data-slot='nav-item']", { hasText: "Tasks" }).count()) === 1 && (await page.locator("[data-slot='nav-item']", { hasText: "Pull requests" }).count()) === 1, "Tasks and Pull requests are in the sidebar's Work group");
  await both("work-tasks-list");
  await page.getByRole("button", { name: "Work options" }).click();
  await page.getByRole("menuitemcheckbox", { name: /Plan before working/ }).waitFor({ timeout: 3000 });
  check(true, "the Work options menu opens");
  await page.keyboard.press("Escape");

  // filter: search narrows, clear restores
  await page.getByLabel("Search tasks").fill("zzzz-no-such-task");
  await page.getByText(/No tasks match/).waitFor({ timeout: 3000 });
  check(true, "a search with no match says so and offers to clear filters");
  await shot("work-tasks-empty-search");
  await page.getByRole("button", { name: "Clear filters" }).click();
  await rows().first().waitFor({ timeout: 3000 });
  check((await rows().count()) === total, "clearing the filters brings every task back");
  const firstTitle = (await rows().first().getAttribute("aria-label")).split(",")[0];
  await page.getByLabel("Search tasks").fill(firstTitle.slice(0, 12));
  await sleep(300);
  check((await rows().count()) >= 1 && (await rows().count()) <= total, `searching "${firstTitle.slice(0, 12)}" narrows the list to ${await rows().count()}`);
  await page.getByLabel("Clear search").click();

  // stage filter
  const chip = page.getByRole("radio").or(page.locator("[data-slot='toggle-group-item']")).filter({ hasText: /^Needs you/ }).first();
  if (await chip.count()) {
    await chip.click();
    await sleep(300);
    const statuses = await rows().evaluateAll((els) => [...new Set(els.map((e) => e.getAttribute("data-status")))]);
    check(statuses.every((s) => ["AwaitingApproval", "Failed"].includes(s)), `the Needs you filter keeps only waiting or stopped tasks (${statuses.join(", ")})`);
    await chip.click();
    await sleep(200);
  }

  // keyboard: j / k move focus, Enter opens
  await page.locator("[data-slot='tasks-page']").click({ position: { x: 4, y: 4 } });
  await page.keyboard.press("j");
  check(await page.evaluate(() => document.activeElement?.getAttribute("data-slot") === "task-row"), "j focuses the first task");
  const first = await page.evaluate(() => document.activeElement.getAttribute("data-task-id"));
  await page.keyboard.press("j");
  const second = await page.evaluate(() => document.activeElement.getAttribute("data-task-id"));
  await page.keyboard.press("k");
  const back = await page.evaluate(() => document.activeElement.getAttribute("data-task-id"));
  check(first !== second && back === first, "j moves down and k moves back up");
  await page.keyboard.press("Enter");
  await page.locator("[data-slot='task-detail']").waitFor({ timeout: 5000 });
  check(new URL(page.url()).hash.includes(`task=${first}`), "Enter opens the focused task and the link carries its id");
  await sleep(500);
  await both("work-task-detail-side");

  // the side sheet (hushbacks' response details sheet): previous / next, Esc closes
  check(await page.locator("[data-slot='sheet-content'] [data-slot='task-detail']").count() === 1, "the task opens in a side sheet");
  const footer = () => page.locator("[data-slot='sheet-footer']").innerText();
  check(/Task 1 of \d+/.test(await footer()), "the sheet says where the task sits in the list");
  await page.getByRole("button", { name: "Next task" }).click();
  await page.waitForFunction((id) => !location.hash.includes(`task=${id}`), first, { timeout: 3000 });
  await page.waitForFunction(() => /Task 2 of/.test(document.querySelector("[data-slot='sheet-footer']")?.textContent ?? ""), null, { timeout: 3000 }).catch(() => {});
  check(/Task 2 of \d+/.test(await footer()), `Next task moves to the next task in the list (${await footer()})`);
  await page.locator("[data-slot='sheet-footer']").click();
  await page.keyboard.press("ArrowLeft");
  await page.waitForFunction((id) => location.hash.includes(`task=${id}`), first, { timeout: 3000 });
  check(true, "← moves back to the previous task");

  // detail tabs
  await page.getByRole("tab", { name: "Changes" }).click();
  await page.locator("[data-slot='changes'], [data-slot='changes-empty']").first().waitFor({ timeout: 5000 });
  check(true, "the Changes tab opens (a task with no working copy says so)");
  await page.getByRole("tab", { name: "Details" }).click();
  await page.locator("[data-slot='log']").waitFor({ timeout: 5000 });
  check(true, "the Details tab shows the activity log");
  await page.getByRole("tab", { name: "Conversation" }).click();

  // rename: a real RenameTask round trip against the test daemon
  const renamed = `Renamed by web verify ${Date.now() % 100000}`;
  await page.getByRole("button", { name: "More task actions" }).click();
  await page.getByRole("menuitem", { name: /Rename/ }).click();
  await page.getByRole("textbox", { name: "Name" }).fill(renamed);
  await page.getByRole("button", { name: "Rename", exact: true }).click();
  await page.waitForFunction((t) => document.querySelector("[data-slot='task-detail'] [data-slot='task-title']")?.textContent?.includes(t), renamed, { timeout: 8000 });
  check(true, "renaming the task round-trips through the daemon and the heading updates");
  await page.waitForFunction((t) => [...document.querySelectorAll("[data-slot='task-row']")].some((r) => r.getAttribute("aria-label")?.startsWith(t)), renamed, { timeout: 8000 });
  check(true, "the list shows the new name too");
  await page.getByRole("button", { name: "More task actions" }).click();
  await page.getByRole("menuitem", { name: /Rename/ }).click();
  await page.getByRole("textbox", { name: "Name" }).fill("");
  await page.getByRole("button", { name: "Rename", exact: true }).click();
  await page.waitForFunction((t) => !document.querySelector("[data-slot='task-detail'] [data-slot='task-title']")?.textContent?.includes(t), renamed, { timeout: 8000 });
  check(true, "an empty name gives the task back its own title");

  // destructive actions ask first: open the dialog, then cancel
  const finished = page.locator("[data-slot='task-row'][data-status='Completed'], [data-slot='task-row'][data-status='Cancelled'], [data-slot='task-row'][data-status='Failed']").first();
  if (await finished.count()) {
    // the sheet is modal, as in hushbacks: close it before picking another row
    await page.getByRole("button", { name: "Close task" }).click();
    await page.locator("[data-slot='task-detail']").waitFor({ state: "detached", timeout: 3000 });
    await finished.click();
    await page.getByRole("button", { name: "More task actions" }).click();
    await page.getByRole("menuitem", { name: /Delete/ }).click();
    await page.getByRole("alertdialog").waitFor({ timeout: 3000 });
    check(/Delete this task\?/.test(await page.getByRole("alertdialog").innerText()), "Delete asks for confirmation");
    await sleep(450);
    await shot("work-confirm-delete");
    await a11y("confirm dialog");
    await page.getByRole("button", { name: "Keep it" }).click();
    await page.getByRole("alertdialog").waitFor({ state: "detached", timeout: 3000 });
    check((await rows().count()) === total, "cancelling the dialog deletes nothing");
  }

  // ⌘K actions for the open task
  await page.keyboard.press("Meta+k");
  await page.locator("[data-slot='command']").waitFor({ timeout: 3000 });
  const entries = (await page.locator("[data-slot='command-item']").allInnerTexts()).join(" | ");
  check(/Rename selected task/.test(entries) && /Show tasks as a board/.test(entries), "⌘K offers actions for the selected task and the layouts");
  await page.keyboard.press("Escape");
  await sleep(300);
  check(new URL(page.url()).hash.includes("task="), "Esc closes the palette and leaves the task open");
  await page.locator("[data-slot='tasks-page']").click({ position: { x: 4, y: 4 } });
  await page.keyboard.press("Escape");
  await page.waitForFunction(() => !location.hash.includes("task="), null, { timeout: 3000 });
  check(true, "Esc closes the open task");

  // board (with no task open, so it has the whole width)
  await page.evaluate(() => { location.hash = "/tasks"; });
  await sleep(300);
  await page.getByRole("radio", { name: "Board" }).or(page.getByLabel("Board", { exact: true })).first().click();
  await page.locator("[data-slot='task-board']").waitFor({ timeout: 5000 });
  const columns = await page.locator("[data-slot='board-column']").count();
  check(columns === 4 && (await page.locator("[data-slot='done-rail']").count()) === 1, `the board fits ${columns} columns and folds Done into a rail`);
  const overflow = await page.locator("[data-slot='task-board']").evaluate((el) => el.scrollWidth <= el.parentElement.clientWidth + 1);
  check(overflow, "the board's columns fit the width without scrolling sideways");
  await both("work-tasks-board");
  // a drop the daemon would refuse is explained, not attempted: a card cannot be dragged into Ready to review
  const card = page.locator("[data-slot='board-column'][data-stage='needsYou'] [data-slot='task-row']").first();
  await card.dragTo(page.locator("[data-slot='board-column'][data-stage='review']"));
  await page.getByText("Neko moves a task to review once its work and checks are done.").waitFor({ timeout: 4000 });
  check(true, "dragging a card where the daemon will not take it says why");
  await card.click({ button: "right" });
  await page.getByRole("menuitem", { name: "Open" }).waitFor({ timeout: 3000 });
  check((await page.getByRole("menuitem", { name: /^Move to Running/ }).count()) === 1, "a card's context menu offers the moves the daemon allows");
  await page.keyboard.press("Escape");
  await page.locator("[data-slot='done-rail']").click();
  await page.locator("[data-slot='board-column'][data-stage='done']").waitFor({ timeout: 3000 });
  check(true, "the Done rail opens into a column");
  await shot("work-tasks-board-done");
  await page.getByLabel("Hide the Done column").click();

  // tree layout
  await page.getByLabel("Tree", { exact: true }).click();
  await page.locator("[data-slot='agent-tree']").waitFor({ timeout: 5000 });
  check((await page.locator("[data-slot='tree-row']").count()) >= 2, "the tree layout draws Neko and what it started");
  await both("work-tasks-tree");

  await page.getByLabel("List", { exact: true }).click();
  await rows().first().waitFor();

  // ----- pull requests -----------------------------------------------------------------
  await page.goto(`${base}/#/prs`);
  await page.locator("[data-slot='pr-row']").first().waitFor({ timeout: 10000 });
  const prRows = await page.locator("[data-slot='pr-row']").count();
  const verdicts = await page.locator("[data-slot='pr-group']").evaluateAll((els) => els.map((e) => e.getAttribute("data-verdict")));
  check(prRows > 0 && verdicts.length > 0, `Pull requests lists ${prRows} rows in ${verdicts.length} verdict groups (${verdicts.join(", ")})`);
  await both("work-prs");
  const mergeAll = page.getByRole("button", { name: /^Merge \d+ ready/ });
  if (await mergeAll.count()) {
    await mergeAll.click();
    await page.getByRole("alertdialog").waitFor({ timeout: 3000 });
    check(/Each is squashed with an empty body/.test(await page.getByRole("alertdialog").innerText()), "Merge N ready asks first, and says what it does");
    await page.getByRole("button", { name: "Cancel" }).click();
    await page.getByRole("alertdialog").waitFor({ state: "detached", timeout: 3000 });
  }
  const closeButton = page.getByRole("button", { name: /^Close #\d+…/ }).first();
  if (await closeButton.count()) {
    await closeButton.click();
    await page.getByRole("dialog", { name: /^Close #/ }).waitFor({ timeout: 3000 });
    const submit = page.getByRole("button", { name: "Close pull request" });
    await page.getByRole("textbox", { name: "Why (posted as a comment)" }).fill("");
    check(await submit.isDisabled(), "closing a pull request needs a comment");
    await sleep(450);
    await shot("work-prs-close-dialog");
    await a11y("close pull request dialog");
    await page.getByRole("button", { name: "Cancel" }).click();
    await page.getByRole("dialog", { name: /^Close #/ }).waitFor({ state: "detached", timeout: 3000 });
  }
  await page.getByRole("button", { name: "Pull request options" }).click();
  await page.getByRole("menu").waitFor({ timeout: 3000 });
  check((await page.getByRole("menuitemcheckbox", { name: /Show merged and closed/ }).count()) === 1, "the options menu has Show merged and closed");
  await page.keyboard.press("Escape");

  // ----- the tree page ---------------------------------------------------------------------
  await page.goto(`${base}/#/tree`);
  await page.locator("[data-slot='tree-row']").first().waitFor({ timeout: 10000 });
  check(true, "#/tree renders the agent tree");

  // ===== 2. a richer snapshot, served to the page =========================================
  if (!existsSync(SEED)) {
    console.log(`skip rich pass: ${SEED} is missing`);
    return;
  }
  const fixture = richSnapshot();
  const ids = fixture.__ids;
  delete fixture.__ids;
  await page.route("**/api/request", async (route) => {
    let body = null;
    try { body = JSON.parse(route.request().postData() ?? "null"); } catch { /* not JSON */ }
    if (body === null || typeof body !== "object") return route.continue();
    if (body.Workbench === "Snapshot") return route.fulfill({ json: { Workbench: fixture } });
    if (body.WorkbenchIfChanged) {
      if (body.WorkbenchIfChanged.digest === fixture.digest) {
        await sleep(5000);
        return route.fulfill({ json: { WorkbenchUnchanged: true } }).catch(() => {});
      }
      return route.fulfill({ json: { Workbench: fixture } });
    }
    if (body.TaskChanges) {
      return route.fulfill({ json: { TaskChanges: { files: ["packages/utilities/src/http/stream-request.ts", "packages/utilities/src/http/stream-request.spec.ts", "packages/utilities/src/sentry/sentry.constants.js"], patch: PATCH, truncated: false } } });
    }
    return route.continue();
  });
  await page.goto(`${base}/#/tasks`);
  await page.reload();
  await rows().first().waitFor({ timeout: 15000 });
  await page.waitForFunction(() => document.querySelectorAll("[data-slot='task-row']").length > 100, null, { timeout: 15000 });
  check(true, "the rich snapshot loads (106 tasks)");

  // list with running tasks
  await page.getByLabel("List", { exact: true }).click();
  check((await page.locator("[data-slot='group-header'][data-stage='running']").count()) === 1 && (await page.locator("[data-slot='group-header'][data-stage='upNext']").count()) === 1, "Running and Up next groups appear with live and queued work");
  await both("work-rich-list");

  // board
  await page.getByLabel("Board", { exact: true }).click();
  await page.locator("[data-slot='task-board']").waitFor();
  await both("work-rich-board");

  // tree: expand the watch, the split parent, the running task's runs
  await page.getByLabel("Tree", { exact: true }).click();
  await page.locator("[data-slot='agent-tree']").waitFor();
  check((await page.locator("[data-slot='tree-row'][data-kind='run']").count()) >= 1, "a running task is open by default and shows its agent runs");
  const summary = await page.locator("[data-slot='tree-summary']").innerText();
  check(/\d+ tasks · \d+ running · \$[\d.]+ reported/.test(summary), `the tree summarises tasks, running and cost: "${summary}"`);
  const watch = page.locator("[data-slot='tree-row'][data-kind='watch']").first();
  await watch.focus();
  await page.keyboard.press("ArrowRight");
  await sleep(250);
  check((await watch.getAttribute("aria-expanded")) === "true", "Right opens a watch from the keyboard");
  await page.keyboard.press("ArrowDown");
  check(await page.evaluate(() => document.activeElement?.getAttribute("data-slot") === "tree-row"), "Down moves to the next row");
  await page.keyboard.press("ArrowLeft");
  await both("work-rich-tree");
  const splitParent = page.locator("[data-slot='tree-row'][data-kind='task']", { hasText: "Split" });
  void splitParent;
  await page.getByRole("button", { name: "Expand all" }).click();
  await sleep(300);
  check((await page.locator("[data-slot='tree-row']").count()) > 30, "Expand all opens every branch");
  await page.evaluate(() => document.querySelector("[data-slot='agent-tree']").closest(".overflow-y-auto").scrollTo({ top: 0 }));
  await shot("work-rich-tree-expanded");
  await page.getByRole("button", { name: "Collapse all" }).click();

  // the running task, full page
  await page.goto(`${base}/#/tasks?task=${ids.building}`);
  await page.locator("[data-slot='task-detail']").waitFor({ timeout: 8000 });
  await page.locator("[data-slot='working']").waitFor({ timeout: 5000 });
  check((await page.locator("[data-slot='agent-steps']").count()) >= 1, "a running task's commands fold into a work log");
  check((await page.getByRole("toolbar", { name: "Task actions" }).getByRole("button", { name: "Stop", exact: true }).count()) === 1, "a running task offers Stop");
  await page.locator("[data-slot='agent-steps'] button[aria-expanded]").first().waitFor();
  const failing = page.locator("[data-slot='command-row'][data-failed='true'] :is(button, [role='button'])").first();
  if (await failing.count()) {
    await failing.click();
    check(/FAIL packages/.test(await page.locator("[data-slot='thread']").innerText()), "a failed command opens its output in place");
  }
  await both("work-rich-task-running");
  await page.getByRole("toolbar", { name: "Task actions" }).getByRole("button", { name: "Stop", exact: true }).click();
  await page.getByRole("alertdialog").waitFor({ timeout: 3000 });
  check(/Stop “/.test(await page.getByRole("alertdialog").innerText()), "Stop asks before stopping the agent");
  await page.getByRole("button", { name: "Keep working" }).click();
  await page.getByRole("alertdialog").waitFor({ state: "detached", timeout: 3000 });

  // a ticket ready for review: result, independent review, delivery, then changes
  await page.goto(`${base}/#/tasks?task=${ids.ready}&view=full`);
  await page.locator("[data-slot='result']").waitFor({ timeout: 8000 });
  check((await page.locator("[data-slot='review']").count()) === 1, "the result shows the independent review verdict");
  check((await page.locator("[data-slot='delivery']").count()) === 1, "the pull request shows as where the work went");
  check((await page.getByRole("link", { name: /Open pull request/ }).count()) >= 1, "a ready task with an open pull request offers Open pull request");
  await page.locator("[data-slot='result']").scrollIntoViewIfNeeded();
  await page.getByRole("button", { name: "More task actions" }).click();
  await page.getByRole("menu").waitFor({ timeout: 3000 });
  check((await page.getByRole("menuitemcheckbox", { name: /Land on main in athena/ }).count()) === 1, "the ⋯ menu has the land-on-main switch for the task's repository");
  check((await page.getByRole("menuitem", { name: "Rename…" }).count()) === 1 && (await page.getByRole("menuitem", { name: "Cancel task" }).count()) === 1, "the ⋯ menu offers Rename and Cancel task");
  await page.keyboard.press("Escape");
  await both("work-rich-task-ready");
  await page.getByRole("tab", { name: "Changes" }).click();
  await page.locator("[data-slot='changed-file']").first().waitFor({ timeout: 5000 });
  check((await page.locator("[data-slot='changed-file']").count()) === 3, "Changes lists the three changed files");
  check((await page.locator("[data-slot='diff'] tr[data-kind='add']").count()) > 5 && (await page.locator("[data-slot='diff'] tr[data-kind='del']").count()) > 1, "the diff colours added and removed lines");
  await both("work-rich-task-changes");

  // a stopped task: the tray says why
  await page.goto(`${base}/#/tasks?task=${ids.failed}`);
  await page.locator("[data-slot='task-tray']").waitFor({ timeout: 8000 });
  check(/why it stopped/i.test(await page.locator("[data-slot='task-tray']").innerText()), "a stopped task says why it stopped above the composer");
  check((await page.getByRole("button", { name: "Start again" }).count()) === 1, "a stopped task offers Start again");
  await both("work-rich-task-failed");

  // a task with a split: parallel work in Details
  await page.goto(`${base}/#/tasks?task=${ids.parent}&view=full`);
  await page.getByRole("tab", { name: "Details" }).click();
  await page.locator("[data-slot='split']").waitFor({ timeout: 5000 });
  check((await page.getByRole("button", { name: "Open subtask" }).count()) === 2, "a split lists its subtasks with Open subtask");
  await shot("work-rich-task-split");

  // pull requests, richer
  await page.goto(`${base}/#/prs`);
  await page.locator("[data-slot='pr-row']").first().waitFor({ timeout: 8000 });
  const kinds = await page.locator("[data-slot='pr-group']").evaluateAll((els) => els.map((e) => e.getAttribute("data-verdict")));
  check(["ready", "close", "merging"].every((k) => kinds.includes(k)), `verdict groups: ${kinds.join(", ")}`);
  check((await page.getByText(/Conflicts with main/).count()) >= 1 && (await page.getByText(/Checks failing: Test, Lint \(also failing on main\)/).count()) === 1, "conflicts and failing checks (and a failure the base shares) get their own lines");
  check((await page.locator("[data-slot='follow-ups']").count()) >= 1, "a hand-back count is shown");
  await both("work-rich-prs");
  await page.getByRole("button", { name: "Pull request options" }).click();
  await page.getByRole("menu").waitFor({ timeout: 3000 });
  check((await page.getByRole("menuitemcheckbox", { name: "athena" }).count()) === 1, "merge without review is a per-repository switch");
  await shot("work-rich-prs-options");
  await page.keyboard.press("Escape");

  // the tree page, richer
  await page.goto(`${base}/#/tree`);
  await page.locator("[data-slot='tree-row']").first().waitFor();
  await sleep(400);
  await both("work-rich-treepage");
  await page.unroute("**/api/request");
}
