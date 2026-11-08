// "Done, not yet seen" (docs/research/herdr.md; web/src/work/seen.tsx): a finished task the
// person has not opened since it finished carries a small Iris dot in the task list, on its
// board card, on its tab and in Home's crew line, and opening it marks it seen. The record is
// the daemon's (`UnseenTasks` / `MarkTaskSeen`), so it survives a reload. Runs against the
// test daemon's own tasks: the first read counts every finished task as seen, so the check
// flags one with the right-click menu's "Mark as unread".
//
//   NEKO_WEB_ONLY=unseen node scripts/web-verify.mjs
export default async function ({ page, shot, a11y, check, sleep, PORT }) {
  const base = `http://127.0.0.1:${PORT}`;
  const request = (body) => page.evaluate(async (body) => (await fetch("/api/request", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(body) })).json(), body);
  const unseen = async () => (await request("UnseenTasks"))?.UnseenTasks?.ids ?? null;
  const menu = page.locator("[data-slot='neko-context-menu']");
  const tabs = page.locator("[role=tablist][aria-label='Open tabs'] [role=tab]");
  const TITLE = "Add a retry to the webhook sender";

  await page.setViewportSize({ width: Number(process.env.NEKO_WEB_WIDTH ?? 1440), height: 900 });
  await page.emulateMedia({ colorScheme: "dark" });
  await page.goto(`${base}/#/tasks`);
  const row = page.locator("[data-slot='task-line']", { has: page.locator(`[data-slot='task-row'][aria-label^='${TITLE}']`) });
  await page.getByLabel("List", { exact: true }).first().click();
  await row.waitFor({ timeout: 15000 });
  const id = await row.locator("[data-slot='task-row']").getAttribute("data-task-id");
  const first = await unseen();
  check(Array.isArray(first) && !first.includes(id), `the daemon answers UnseenTasks, and what finished before the first read counts as seen (${first?.length ?? "no answer"} unseen)`);
  check((await page.locator("[data-slot='unseen-dot']").count()) === 0, "so no task carries the dot yet");

  // Mark as unread: the dot on the row, the daemon keeps it, a reload keeps it.
  await row.locator("[data-slot='task-row']").click({ button: "right" });
  await menu.getByRole("menuitem", { name: "Mark as unread" }).click();
  await row.locator("[data-slot='unseen-dot']").waitFor({ timeout: 5000 })
    .then(() => check(true, "Mark as unread puts the Iris dot on the row"), () => check(false, "Mark as unread puts the Iris dot on the row"));
  check((await unseen())?.includes(id), "the daemon records it");
  check(/not yet seen/.test(await row.locator("[data-slot='task-row']").getAttribute("aria-label")), "the row's name says it is not yet seen");
  const dot = await row.locator("[data-slot='unseen-dot']").evaluate((e) => ({ bg: getComputedStyle(e).backgroundColor, w: e.getBoundingClientRect().width, label: e.getAttribute("aria-label") }));
  const primary = await page.evaluate(() => { const p = document.createElement("span"); p.className = "bg-primary"; document.body.append(p); const c = getComputedStyle(p).backgroundColor; p.remove(); return c; });
  check(dot.bg === primary && dot.w <= 8 && dot.label === "Not yet seen", `the dot is small, Iris (the primary) and named (${dot.bg}, ${dot.w}px)`);
  await page.reload();
  await row.locator("[data-slot='unseen-dot']").waitFor({ timeout: 8000 })
    .then(() => check(true, "the dot survives a reload (the daemon keeps it, not the page)"), () => check(false, "the dot survives a reload"));
  for (const scheme of ["dark", "light"]) {
    await page.emulateMedia({ colorScheme: scheme });
    await page.waitForFunction((s) => document.documentElement.classList.contains(s), scheme, { timeout: 3000 });
    await page.mouse.move(0, 0);
    await sleep(700);
    await shot(`unseen-list-${scheme}`);
  }
  await page.emulateMedia({ colorScheme: "dark" });
  await a11y("task list with an unseen dot");

  // The board card carries it too.
  await page.getByLabel("Board", { exact: true }).first().click();
  const card = page.locator(`[data-slot='board-card'][data-unseen]`, { has: page.locator(`[data-task-id='${id}']`) });
  await card.waitFor({ timeout: 5000 })
    .then(() => check(true, "the board card carries the dot"), () => check(false, "the board card carries the dot"));
  for (const scheme of ["dark", "light"]) {
    await page.emulateMedia({ colorScheme: scheme });
    await page.waitForFunction((s) => document.documentElement.classList.contains(s), scheme, { timeout: 3000 });
    await page.mouse.move(0, 0);
    await sleep(700);
    await shot(`unseen-board-${scheme}`);
  }
  await page.emulateMedia({ colorScheme: "dark" });
  await page.getByLabel("List", { exact: true }).first().click();

  // Home's crew line counts it and links to it.
  await page.goto(`${base}/#/`);
  const crew = page.locator("[data-slot='crew-unseen']");
  await crew.waitFor({ timeout: 10000 })
    .then(() => check(true, "Home's crew line counts the finished task not seen"), () => check(false, "Home's crew line counts the finished task not seen"));
  const said = (await crew.innerText().catch(() => "")).trim();
  check(said === "1 finished, not seen" && /task/.test((await crew.getAttribute("href")) ?? ""), `it reads “${said}” and links to the task`);
  await shot("unseen-home-crew");

  // Its tab: open the task in a tab, step back to Tasks, flag it again; the tab shows the dot
  // while another tab is chosen, and choosing the tab marks it seen.
  await page.goto(`${base}/#/tasks`);
  await row.waitFor({ timeout: 10000 });
  await row.locator("[data-slot='task-row']").click({ button: "right" });
  await menu.getByRole("menuitem", { name: "Open in a tab" }).click();
  await page.locator("[data-slot='task-detail']").waitFor({ timeout: 8000 });
  await sleep(500);
  check(!(await unseen())?.includes(id), "opening the task in its tab marks it seen");
  const taskTab = tabs.filter({ hasText: TITLE }).first();
  await tabs.filter({ hasText: /^Tasks$/ }).first().click();
  await row.waitFor({ timeout: 8000 });
  await row.locator("[data-slot='task-row']").click({ button: "right" });
  await menu.getByRole("menuitem", { name: "Mark as unread" }).click();
  await taskTab.locator("[data-slot='unseen-dot']").waitFor({ timeout: 5000 })
    .then(() => check(true, "the task's tab shows the dot while another tab is chosen"), () => check(false, "the task's tab shows the dot while another tab is chosen"));
  check(/not yet seen/.test(await taskTab.getAttribute("aria-label")), "the tab's name says so");
  for (const scheme of ["dark", "light"]) {
    await page.emulateMedia({ colorScheme: scheme });
    await page.waitForFunction((s) => document.documentElement.classList.contains(s), scheme, { timeout: 3000 });
    await page.mouse.move(0, 0);
    await sleep(700);
    await shot(`unseen-tab-${scheme}`, { clip: { x: 0, y: 0, width: 900, height: 120 } });
  }
  await page.emulateMedia({ colorScheme: "dark" });
  await taskTab.click();
  await page.locator("[data-slot='task-detail']").waitFor({ timeout: 8000 });
  await sleep(500);
  check(!(await unseen())?.includes(id) && (await taskTab.locator("[data-slot='unseen-dot']").count()) === 0, "choosing the tab marks it seen and the dot goes");

  // Opening it beside the list (the sheet) marks it seen too.
  await page.keyboard.press("Control+w");
  await page.goto(`${base}/#/tasks`);
  await row.waitFor({ timeout: 8000 });
  await row.locator("[data-slot='task-row']").click({ button: "right" });
  await menu.getByRole("menuitem", { name: "Mark as unread" }).click();
  await row.locator("[data-slot='unseen-dot']").waitFor({ timeout: 5000 });
  await row.locator("[data-slot='task-row']").click();
  await page.locator("[data-slot='task-detail']").waitFor({ timeout: 8000 });
  await page.waitForFunction(() => !document.querySelector("[data-slot='task-line'] [data-slot='unseen-dot']"), null, { timeout: 5000 })
    .then(() => check(true, "opening the task in the sheet marks it seen"), () => check(false, "opening the task in the sheet marks it seen"));
  check(!(await unseen())?.includes(id), "and the daemon agrees");
  await page.keyboard.press("Escape");
  await page.waitForFunction(() => !location.hash.includes("task="), null, { timeout: 3000 }).catch(() => {});
}
