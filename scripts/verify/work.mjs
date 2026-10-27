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


/** Carries a card with the pointer in small steps (dnd-kit wants a real gesture). `hold` leaves the button down. */
async function mouseDrag(page, from, to, { hold = false } = {}) {
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  await from.scrollIntoViewIfNeeded();
  const a = await from.boundingBox();
  const b = await to.boundingBox();
  await page.mouse.move(a.x + a.width / 2, a.y + 24);
  await page.mouse.down();
  await page.mouse.move(a.x + a.width / 2 + 12, a.y + 40, { steps: 4 });
  await page.mouse.move(b.x + b.width / 2, b.y + Math.min(140, b.height / 2), { steps: 14 });
  await sleep(120);
  if (!hold) await page.mouse.up();
}

/**
 * A richer snapshot built from the seed, with the states the test daemon's data
 * lacks. `daemon` is the test daemon's own snapshot: it supplies every field a
 * seed leaves out (the made-up seed holds only workspaces, tasks and chat).
 */
function richSnapshot(daemon) {
  const seed = JSON.parse(readFileSync(SEED, "utf8"));
  const { fixture_mcp: watches, ...rest } = seed;
  const base = { ...daemon, ...rest, mcp: { ...daemon.mcp, ...rest.mcp, ...watches } };
  base.task_roots ??= {};
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

  // the side sheet: title is the heading, actions beside it, no previous/next buttons or footer;
  // it spans the whole page (header included); ← → still move between tasks
  check(await page.locator("[data-slot='sheet-content'] [data-slot='task-detail']").count() === 1, "the task opens in a side sheet");
  check((await page.getByRole("button", { name: "Next task" }).count()) === 0 && (await page.locator("[data-slot='sheet-footer']").count()) === 0 && !(await page.locator("[data-slot='sheet-content']").innerText()).includes("Task details"), "no previous/next buttons, no footer and no “Task details” heading");
  const sheetBox = await page.locator("[data-slot='sheet-content']").boundingBox();
  const headerBox = await page.locator("[data-slot='work-header']").boundingBox();
  check(sheetBox && headerBox && sheetBox.y <= headerBox.y + 16, `the sheet spans the page from the top (sheet y ${Math.round(sheetBox?.y ?? -1)}, header y ${Math.round(headerBox?.y ?? -1)})`);
  const titleRow = await page.locator("[data-slot='task-detail'] [data-slot='task-title']").boundingBox();
  const actionsBox = await page.getByRole("toolbar", { name: "Task actions" }).boundingBox();
  check(titleRow && actionsBox && Math.abs(actionsBox.y - titleRow.y) < 24, "the task's actions sit beside its title");
  const tabsBox = await page.getByRole("tablist", { name: "Task sections" }).boundingBox();
  const statusBox = await page.locator("[data-slot='task-detail'] [data-slot='task-status']").boundingBox();
  check(tabsBox && statusBox && Math.abs(tabsBox.y + tabsBox.height / 2 - (statusBox.y + statusBox.height / 2)) < 8 && statusBox.x > tabsBox.x + tabsBox.width, "the tabs are on the left and the status on the right of one row");
  await page.locator("[data-slot='task-detail'] [data-slot='task-title']").click();
  await page.keyboard.press("ArrowRight");
  await page.waitForFunction((id) => !location.hash.includes(`task=${id}`), first, { timeout: 3000 });
  check(true, "→ moves to the next task");
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
  // Escape belongs to the palette once its input has focus; wait for that, as a person would.
  await page.waitForFunction(() => document.activeElement?.getAttribute("data-slot") === "command-input", null, { timeout: 3000 });
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
  await page.getByLabel("Board", { exact: true }).first().click();
  await page.locator("[data-slot='task-board']").waitFor({ timeout: 5000 });
  const columns = await page.locator("[data-slot='board-column']").count();
  check(columns === 4 && (await page.locator("[data-slot='done-rail']").count()) === 1, `the board fits ${columns} columns and folds Done into a rail`);
  const counts = await page.locator("[data-slot='board-column']").evaluateAll((els) => els.map((e) => [e.dataset.stage, Number(e.querySelector("[data-slot='column-count']").textContent), e.querySelectorAll("[data-card='board-card']").length]));
  check(counts.every(([, n, cards]) => n === cards), `each column's count matches its cards (${counts.map(([k, n]) => `${k} ${n}`).join(", ")})`);
  const overflow = await page.locator("[data-slot='task-board']").evaluate((el) => el.scrollWidth <= el.parentElement.clientWidth + 1);
  check(overflow || Number(process.env.NEKO_WEB_WIDTH ?? 1440) < 1300, "the board's columns fit the width without scrolling sideways (at desktop width)");
  await both("work-tasks-board");
  const hasCard = await page.locator("[data-slot='board-column'][data-stage='needsYou'] [data-slot='task-row']").count();
  if (hasCard) {
    // a drop the daemon would refuse is explained inline, not attempted: a card cannot be dragged into Ready to review
    const card = page.locator("[data-slot='board-column'][data-stage='needsYou'] [data-slot='task-row']").first();
    await mouseDrag(page, card, page.locator("[data-slot='board-column'][data-stage='review']"), { hold: true });
    await page.locator("[data-slot='board-column'][data-stage='review'][data-drop='refuse']").waitFor({ timeout: 3000 });
    check((await page.locator("[data-slot='board-column'][data-stage='review'] [data-slot='drop-slot']").innerText()).includes("Neko moves a task to review"), "while carrying a card over a column that will not take it, the column says why");
    await sleep(250);
    await shot("work-tasks-board-refuse-hover");
    await page.mouse.up();
    const notice = page.locator("[data-slot='board-notice']");
    await notice.waitFor({ timeout: 4000 });
    check(/Neko moves a task to review once its work and checks are done/.test(await notice.innerText()), "dropping a card where the daemon will not take it says why, inline");
    await sleep(300);
    await shot("work-tasks-board-notice");
    await a11y("board notice");
    await notice.getByRole("button", { name: "Dismiss" }).click();
    await card.click({ button: "right" });
    await page.getByRole("menuitem", { name: "Open" }).waitFor({ timeout: 3000 });
    check((await page.getByRole("menuitem", { name: /^Move to Running/ }).count()) === 1, "a card's context menu offers the moves the daemon allows");
    await page.keyboard.press("Escape");
    // Esc cancels a drag and the card stays where it was
    const before = await page.locator("[data-slot='board-column'][data-stage='needsYou'] [data-slot='task-row']").count();
    await mouseDrag(page, card, page.locator("[data-slot='board-column'][data-stage='upNext']"), { hold: true });
    await page.locator("[data-slot='board-card-overlay']").waitFor({ timeout: 3000 });
    await page.keyboard.press("Escape");
    await page.locator("[data-slot='board-card-overlay']").waitFor({ state: "detached", timeout: 3000 });
    await page.mouse.up();
    check((await page.locator("[data-slot='board-column'][data-stage='needsYou'] [data-slot='task-row']").count()) === before, "Esc during a drag puts the card back");
    // a click opens the sheet
    await card.click();
    await page.waitForFunction(() => location.hash.includes("task="), null, { timeout: 3000 });
    check(true, "clicking a card opens the task sheet");
    await sleep(500);
    await page.keyboard.press("Escape");
    await page.waitForFunction(() => !location.hash.includes("task="), null, { timeout: 3000 });
  }
  await page.locator("[data-slot='done-rail']").click();
  await page.locator("[data-slot='board-column'][data-stage='done']").waitFor({ timeout: 3000 });
  check(true, "the Done rail opens into a column");
  await shot("work-tasks-board-done");
  await page.getByRole("button", { name: "Done options" }).click();
  await page.getByRole("menuitem", { name: "Fold into a rail" }).click();

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
  const daemon = (await page.evaluate(async () => (await fetch("/api/request", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ Workbench: "Snapshot" }) })).json())).Workbench;
  const fixture = richSnapshot(daemon);
  const ids = fixture.__ids;
  delete fixture.__ids;
  const commands = [];
  await page.route("**/api/request", async (route) => {
    let body = null;
    try { body = JSON.parse(route.request().postData() ?? "null"); } catch { /* not JSON */ }
    if (body === null || typeof body !== "object") return route.continue();
    if (body.Workbench === "Snapshot") return route.fulfill({ json: { Workbench: fixture } });
    // a command: record it (nothing is written anywhere) and answer with the unchanged snapshot
    if (body.Workbench && typeof body.Workbench === "object") { commands.push(body.Workbench); return route.fulfill({ json: { Workbench: fixture } }); }
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
  // The daemon serves a task's conversation from its own (unpatched) snapshot, so the fixture's
  // events are folded the way `neko_core::task_chat` does: the brief, then each agent's stretch.
  await page.route(/\/api\/chat\/task(:|%3A)[^/]+$/, async (route) => {
    const id = decodeURIComponent(route.request().url().split("/").pop()).slice(5);
    const task = (fixture.tasks ?? []).find((t) => t.id === id);
    if (!task) return route.continue();
    const messages = [{ id: "brief", role: "user", parts: [{ type: "text", text: task.goal }], metadata: { atMs: task.created_at_ms, state: "done", kind: "brief" } }];
    let stretch = null;
    task.events.forEach((e, i) => {
      if (!["scout", "builder", "reviewer"].includes(e.role) || /^Agent started|^Command running/.test(e.message)) return;
      const raw = e.message.startsWith("VERIFICATION_COMMAND ") ? JSON.parse(e.message.slice(21)) : null;
      const part = raw
        ? { type: "dynamic-tool", toolName: "Bash", toolCallId: `receipt-${i}`, state: "output-available", input: { command: raw.command }, output: { command: raw.command, output: raw.output, exit_code: raw.exit_code, incomplete: false, failed: raw.exit_code !== 0 } }
        : { type: "text", text: e.message, state: "done" };
      if (!stretch || stretch.metadata.authorRole !== e.role) {
        stretch = { id: `a${i}`, role: "assistant", parts: [], metadata: { atMs: e.at_ms, state: "done", author: e.role, authorRole: e.role } };
        messages.push(stretch);
      }
      stretch.parts.push(part);
    });
    return route.fulfill({ json: { messages, active: null } });
  });
  await page.goto(`${base}/#/tasks`);
  await page.reload();
  await rows().first().waitFor({ timeout: 15000 });
  // Only the rich snapshot has a task that is building.
  await page.waitForFunction(() => document.querySelector("[data-slot='task-row'][data-status='Building']"), null, { timeout: 15000 });
  check(true, `the rich snapshot loads (${await rows().count()} task rows)`);

  // list with running tasks
  await page.getByLabel("List", { exact: true }).click();
  check((await page.locator("[data-slot='group-header'][data-stage='running']").count()) === 1 && (await page.locator("[data-slot='group-header'][data-stage='upNext']").count()) === 1, "Running and Up next groups appear with live and queued work");
  await both("work-rich-list");

  // board
  await page.getByLabel("Board", { exact: true }).click();
  await page.locator("[data-slot='task-board']").waitFor();
  await both("work-rich-board");

  // dragging between columns sends the daemon's own commands, and only those
  const col = (stage) => page.locator(`[data-slot='board-column'][data-stage='${stage}']`);
  const cardIn = (stage, status) => col(stage).locator(`[data-slot='task-row'][data-status='${status}']`).first();
  const approval = cardIn("needsYou", "AwaitingApproval");
  const approvalId = await approval.getAttribute("data-task-id");
  commands.length = 0;
  await mouseDrag(page, approval, col("running"), { hold: true });
  await page.locator("[data-slot='board-column'][data-stage='running'][data-drop='ok']").waitFor({ timeout: 3000 });
  check((await col("running").locator("[data-slot='drop-slot']").innerText()).includes("Start work"), "a column that will take the card says what the drop does");
  await sleep(250);
  await shot("work-rich-board-dragging");
  await page.mouse.up();
  await sleep(400);
  check(commands.length === 1 && commands[0].StartTask?.task_id === approvalId, `Needs you → Running sends StartTask for that task (${JSON.stringify(commands)})`);

  // Ready to review → Done marks it done (and Done opens first, as a drop target)
  await page.locator("[data-slot='done-rail']").click();
  await col("done").waitFor({ timeout: 3000 });
  const ready = cardIn("review", "ReadyForReview");
  const readyId = await ready.getAttribute("data-task-id");
  commands.length = 0;
  await mouseDrag(page, ready, col("done"));
  await sleep(400);
  check(commands.length === 1 && commands[0].CompleteTask?.task_id === readyId, `Ready to review → Done sends CompleteTask (${JSON.stringify(commands)})`);

  // Running → Done is a cancel, so it asks first and sends nothing until confirmed
  commands.length = 0;
  await mouseDrag(page, cardIn("running", "Building"), col("done"));
  await page.getByRole("alertdialog").waitFor({ timeout: 3000 });
  check(commands.length === 0 && /Cancel/.test(await page.getByRole("alertdialog").innerText()), "Running → Done asks before cancelling");
  await page.getByRole("button", { name: "Keep working" }).click();
  await page.getByRole("alertdialog").waitFor({ state: "detached", timeout: 3000 });
  check(commands.length === 0, "declining the confirmation sends nothing");

  // Running → Needs you, Completed → Up next: refused with a reason, nothing sent
  await mouseDrag(page, cardIn("running", "Building"), col("needsYou"));
  const rn = page.locator("[data-slot='board-notice']");
  await rn.waitFor({ timeout: 3000 });
  check(commands.length === 0 && /can’t go back to waiting/.test(await rn.innerText()), "Running → Needs you is refused with its reason");
  await rn.getByRole("button", { name: "Dismiss" }).click();
  await mouseDrag(page, cardIn("done", "Completed"), col("upNext"));
  await rn.waitFor({ timeout: 3000 });
  check(commands.length === 0 && /Finished work stays finished/.test(await rn.innerText()), "Done → Up next is refused with its reason");
  await rn.getByRole("button", { name: "Dismiss" }).click();
  await shot("work-rich-board-after");

  // keyboard: Space picks a card up, arrows choose a column, Space drops, Enter on a card still opens it
  const kb = cardIn("needsYou", "AwaitingApproval");
  const kbId = await kb.getAttribute("data-task-id");
  commands.length = 0;
  await kb.focus();
  await page.keyboard.press("Space");
  const live = page.locator("[id^='DndLiveRegion']");
  await page.waitForFunction(() => /Picked up|where it already is|Move .* to/.test(document.querySelector("[id^='DndLiveRegion']")?.textContent ?? ""), null, { timeout: 3000 });
  check(true, "picking a card up with Space is announced (the live region says where it is)");
  await sleep(500); // a person takes longer than this between Space and an arrow
  await page.keyboard.press("ArrowRight");
  await page.waitForFunction(() => /Ready to review/.test(document.querySelector("[id^='DndLiveRegion']")?.textContent ?? ""), null, { timeout: 3000 });
  check(/can’t go here/.test(await live.textContent()), "moving over a column that refuses the card announces why");
  await page.keyboard.press("ArrowRight");
  await page.waitForFunction(() => /over Running/.test(document.querySelector("[id^='DndLiveRegion']")?.textContent ?? ""), null, { timeout: 3000 });
  check(/Start work/.test(await live.textContent()), "moving over a column that takes the card announces what the drop does");
  await shot("work-rich-board-keyboard");
  await page.keyboard.press("Space");
  await sleep(400);
  check(commands.length === 1 && commands[0].StartTask?.task_id === kbId, `keyboard drop onto Running sends StartTask (${JSON.stringify(commands)})`);
  check(!new URL(page.url()).hash.includes("task="), "Space and the arrows never open the task");
  // Esc puts a keyboard-carried card back and leaves the open task alone
  const kb2 = cardIn("needsYou", "AwaitingApproval");
  commands.length = 0;
  await kb2.focus();
  await page.keyboard.press("Space");
  await sleep(500);
  await page.keyboard.press("ArrowRight");
  await sleep(300);
  await page.keyboard.press("Escape");
  await sleep(300);
  check(commands.length === 0 && (await page.locator("[data-slot='board-card-overlay']").count()) === 0, "Esc cancels a keyboard drag");
  await kb2.focus();
  await page.keyboard.press("Enter");
  await page.waitForFunction(() => location.hash.includes("task="), null, { timeout: 3000 });
  check(true, "Enter on a card opens the task");
  await sleep(500);
  await shot("work-rich-board-sheet");
  await a11y("board with the sheet open");
  await page.keyboard.press("Escape");
  await page.waitForFunction(() => !location.hash.includes("task="), null, { timeout: 3000 });
  await page.getByRole("button", { name: "Done options" }).click();
  await page.getByRole("menuitem", { name: "Fold into a rail" }).click();

  // the notched card: the state in the notch, the action on the card, running live, done plain
  {
    const card = (status) => page.locator(`[data-slot='task-board'] [data-slot='board-card'][data-status='${status}']`).first();
    const notch = async (status) => (await card(status).locator("[data-notch]").innerText()).replace(/\s+/g, " ").trim();
    check((await card("Building").getAttribute("data-density")) === "compact", "board cards are the compact notched card");
    const live = await notch("Building");
    check(/^Preparing a fix · \d+m \d\ds$/.test(live), `a running card's notch is its live step and clock (${live})`);
    check((await card("Building").locator("[data-slot='card-action']").count()) === 0, "a running card has no action");
    const ready = await notch("ReadyForReview");
    check(/^Ready to review/.test(ready) && (await card("ReadyForReview").locator("[data-slot='card-action']").count()) === 0, `a ready card says so in its notch and has no action (${ready})`);
    const waiting = page.locator("[data-slot='task-board'] [data-slot='board-card'][data-status='AwaitingApproval']");
    const states = await waiting.evaluateAll((els) => els.map((e) => [e.querySelector("[data-notch]")?.innerText.split(/\s/)[0] ?? "", e.querySelector("[data-slot='card-action']")?.innerText.trim() ?? ""]));
    check(states.length > 0 && states.every(([s, a]) => (s === "Asks" && a === "Answer") || (s === "Needs" && a === "Approve")), `waiting cards say Asks you with Answer, or Needs approval with Approve (${states.map(([s, a]) => `${s}/${a}`).join(", ")})`);
    const approve = waiting.filter({ has: page.locator("[data-slot='card-action']", { hasText: "Approve" }) }).first();
    if (await approve.count()) {
      const id = await approve.locator("[data-slot='task-row']").getAttribute("data-task-id");
      commands.length = 0;
      await approve.locator("[data-slot='card-action']").click();
      await sleep(400);
      check(commands.length === 1 && commands[0].ApproveTask?.task_id === id && !new URL(page.url()).hash.includes("task="), `a card's Approve sends ApproveTask for that task and does not open it (${JSON.stringify(commands)})`);
    }
    const failed = card("Failed");
    if (await failed.count()) {
      check(/^Stopped/.test(await notch("Failed")) && (await failed.locator("[data-slot='card-action']").innerText()).includes("Start again"), "a stopped card says Stopped in its notch and offers Start again");
      const id = await failed.locator("[data-slot='task-row']").getAttribute("data-task-id");
      commands.length = 0;
      await failed.locator("[data-slot='card-action']").click();
      await sleep(400);
      check(commands.length === 1 && commands[0].StartTask?.task_id === id, `a card's Start again sends StartTask (${JSON.stringify(commands)})`);
    }
    await page.locator("[data-slot='done-rail']").click();
    await page.locator("[data-slot='board-column'][data-stage='done']").waitFor({ timeout: 3000 });
    const doneCards = page.locator("[data-slot='board-column'][data-stage='done'] [data-slot='board-card']");
    check((await doneCards.count()) > 0 && (await doneCards.locator("[data-notch]").count()) === 0, "done cards have no notch");
    await both("work-rich-board-cards");
    await page.getByRole("button", { name: "Done options" }).click();
    await page.getByRole("menuitem", { name: "Fold into a rail" }).click();
  }


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
  const collapsedRows = await page.locator("[data-slot='tree-row']").count();
  await page.getByRole("button", { name: "Expand all" }).click();
  await sleep(300);
  const expandedRows = await page.locator("[data-slot='tree-row']").count();
  check(expandedRows > collapsedRows, `Expand all opens every branch (${collapsedRows} -> ${expandedRows} rows)`);
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
  await page.getByRole("tab", { name: "Result" }).click(); // the result is its own pane
  await page.locator("[data-slot='result']").waitFor({ timeout: 8000 });
  check((await page.getByRole("tab").evaluateAll((els) => els.map((e) => e.getAttribute("aria-label")))).filter((n) => ["Chat", "Result", "Changes", "Plan", "Details"].includes(n)).length >= 5, "the full page is a dock: Chat, Changes, Plan and Details are tabs");
  check((await page.locator("[data-slot='review']").count()) === 1, "the result shows the independent review verdict");
  check((await page.locator("[data-slot='delivery']").count()) === 1, "the pull request shows as where the work went");
  check((await page.getByRole("link", { name: /Open pull request/ }).count()) >= 1, "a ready task with an open pull request offers Open pull request");
  await page.locator("[data-slot='result']").scrollIntoViewIfNeeded();
  await page.getByRole("button", { name: "More task actions" }).click();
  await page.getByRole("menu").waitFor({ timeout: 3000 });
  const readyRepo = String(fixture.task_roots?.[ids.ready] ?? "").split("/").pop();
  check((await page.getByRole("menuitemcheckbox", { name: `Land on main in ${readyRepo}` }).count()) === 1, "the ⋯ menu has the land-on-main switch for the task's repository");
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
  const prRepo = String(fixture.task_roots?.[fixture.tasks.find((t) => t.pull_request)?.id] ?? "").split("/").pop();
  check((await page.getByRole("menuitemcheckbox", { name: prRepo, exact: true }).count()) === 1, "merge without review is a per-repository switch");
  await shot("work-rich-prs-options");
  await page.keyboard.press("Escape");

  // the tree page, richer
  await page.goto(`${base}/#/tree`);
  await page.locator("[data-slot='tree-row']").first().waitFor();
  await sleep(400);
  await both("work-rich-treepage");
  await page.unroute("**/api/request");
}
