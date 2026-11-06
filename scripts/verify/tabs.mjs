// One window of tabs (DESIGN.md §1, docs/knowledge/one-window-of-tabs.md) and
// Neko's own right-click menu: links open or focus a tab, a task has a tab of
// its own, back and forward walk the places visited, the launcher replaces
// itself, tabs survive a reload, the strip is a keyboard tablist, and the
// right-click menu shows each surface's items (the browser's own stays in
// text fields).
//
//   NEKO_WEB_ONLY=tabs node scripts/web-verify.mjs
//
// Shortcuts are pressed with Control: the handlers take ⌘ or Ctrl, and ⌘W / ⌘T
// would reach the shared Chrome itself.
export default async function ({ page, shot, a11y, check, sleep, PORT }) {
  const base = `http://127.0.0.1:${PORT}`;
  const tabs = page.locator("[role=tablist][aria-label='Open tabs'] [role=tab]");
  const strip = page.locator("[role=tablist][aria-label='Open tabs']");
  const selected = () => strip.locator("[role=tab][aria-selected=true]").innerText().catch(() => "");
  const count = () => tabs.count();
  const menu = page.locator("[data-slot='neko-context-menu']");
  const closeMenu = async () => { await page.keyboard.press("Escape"); await menu.waitFor({ state: "hidden", timeout: 3000 }).catch(() => {}); };

  await page.goto(`${base}/#/`);
  await page.evaluate(() => { localStorage.removeItem("neko.tabs.v1"); localStorage.removeItem("neko.sidebar.open"); });
  await page.reload();
  await page.waitForSelector("[role=tablist][aria-label='Open tabs']");
  check((await count()) === 1 && /Home/.test(await selected()), "a fresh window has one tab, Home, selected");
  check((await page.locator("[role=tabpanel]").getAttribute("aria-labelledby"))?.startsWith("neko-tab-"), "the page is the tabpanel of the selected tab");
  check((await page.locator("[data-slot='sidebar']").first().getAttribute("data-state").catch(() => null)) !== "expanded", "the sidebar starts collapsed to its launcher rail");

  // Links open tabs; opening an open place focuses its tab.
  await page.evaluate(() => { location.hash = "/tasks"; });
  await page.waitForSelector("[data-slot='tasks-page']");
  await page.evaluate(() => { location.hash = "/inbox"; });
  await sleep(400);
  check((await count()) === 3 && /Inbox/.test(await selected()), "navigating opens Tasks and Inbox as tabs");
  await page.evaluate(() => { location.hash = "/tasks"; });
  await sleep(300);
  check((await count()) === 3 && /Tasks/.test(await selected()), `a link to an open page focuses its tab instead of adding one (${await count()} tabs, on “${await selected()}”)`);

  // A task gets a tab of its own; the sheet beside the list stays in the Tasks tab.
  const row = page.locator("[data-slot='task-row']").first();
  await row.waitFor({ timeout: 10000 });
  const taskId = await row.getAttribute("data-task-id");
  await row.click();
  await sleep(300);
  check((await count()) === 3, "opening a task beside the list stays in the Tasks tab");
  await page.evaluate((id) => { location.hash = `/tasks?task=${encodeURIComponent(id)}&view=full`; }, taskId);
  await sleep(500);
  check((await count()) === 4 && !/^Tasks$/.test((await selected()).trim()), "a task link opens the task's own tab");
  await shot("tabs-task");

  // Back and forward walk the places visited, across tabs.
  await page.locator("button[aria-label='Back']").click();
  await sleep(400);
  check(/Tasks/.test(await selected()), `Back returns to the Tasks tab (on “${await selected()}”, ${await page.evaluate(() => location.hash)})`);
  await page.keyboard.press("Control+BracketRight");
  await sleep(400);
  check((await count()) === 4 && !/^Tasks$/.test((await selected()).trim()), "Ctrl/⌘] goes forward to the task again");

  // ⌘1–9 and ⌘W.
  await page.keyboard.press("Control+1");
  await sleep(300);
  check(/Home/.test(await selected()), "Ctrl/⌘1 selects the first tab");
  await page.keyboard.press("Control+9");
  await sleep(300);
  await page.keyboard.press("Control+w");
  await sleep(400);
  check((await count()) === 3, "Ctrl/⌘W closes the tab");

  // ⌘T opens the launcher; picking a page replaces it (or focuses the open one).
  await page.keyboard.press("Control+t");
  await page.waitForSelector("[data-slot='launcher']");
  check((await count()) === 4 && /New tab/.test(await selected()), "Ctrl/⌘T opens a New tab launcher");
  await shot("tabs-launcher");
  await a11y("tab launcher");
  await page.locator("[data-slot='launcher-page']", { hasText: "Pull requests" }).click();
  await sleep(500);
  check((await count()) === 4 && /Pull requests/.test(await selected()) && (await page.locator("[role=tab]", { hasText: "New tab" }).count()) === 0, "picking a page replaces the launcher");

  // The set survives a reload.
  const before = await count();
  await page.reload();
  await page.waitForSelector("[role=tablist][aria-label='Open tabs']");
  await sleep(400);
  check((await count()) === before, `the tabs survive a reload (${before})`);

  // The strip is a keyboard tablist.
  await strip.locator("[role=tab][aria-selected=true]").focus();
  await page.keyboard.press("Home");
  check(await page.evaluate(() => document.activeElement?.getAttribute("role") === "tab" && /Home/.test(document.activeElement.textContent ?? "")), "Home moves focus to the first tab");
  await page.keyboard.press("ArrowRight");
  const focusedTitle = await page.evaluate(() => document.activeElement?.textContent ?? "");
  check(!/Home/.test(focusedTitle), "→ moves focus to the next tab");
  await page.keyboard.press("Enter");
  await sleep(300);
  check((await selected()) === focusedTitle, "Enter opens the focused tab");
  const n = await count();
  await strip.locator("[role=tab][aria-selected=true]").focus(); // the page it opened may take the focus
  await page.keyboard.press("End");
  await page.keyboard.press("Delete");
  await sleep(300);
  check((await count()) === n - 1, "Delete closes the focused tab");

  await shot("tabs-after-keyboard");
  // The Tasks sheet is modal (its backdrop takes the first click); start from Home.
  await page.evaluate(() => { location.hash = "/"; });
  await page.waitForSelector("[data-slot='composer'] textarea");
  // Right-click menus: a tab's own, the default on a blank area, none in a text field.
  await tabs.nth(1).click({ button: "right" });
  await menu.waitFor({ timeout: 3000 }).then(() => check(true, "right-click on a tab opens Neko's menu"), () => check(false, "right-click on a tab opens Neko's menu"));
  check((await menu.locator("[role=menuitem]", { hasText: "Close other tabs" }).count()) === 1, "the tab menu offers Close other tabs");
  await sleep(300);
  await shot("menu-tab");
  await a11y("tab menu");
  await page.keyboard.press("ArrowDown");
  check(await page.evaluate(() => document.activeElement?.getAttribute("role") === "menuitem"), "arrow keys move through the menu");
  await closeMenu();

  await page.locator("[data-slot='tab-strip']").click({ button: "right", position: { x: 2, y: 2 } });
  await menu.waitFor({ timeout: 3000 }).catch(() => {});
  check((await menu.locator("[role=menuitem]", { hasText: "New tab" }).count()) === 1, "a blank area gets the default menu (back, forward, reload, new tab, ⌘K)");
  await closeMenu();
  await page.locator("[data-slot='composer'] textarea").click({ button: "right" });
  await sleep(300);
  check(!(await menu.isVisible().catch(() => false)), "a text field keeps the browser's own menu");
  await page.keyboard.press("Escape");

  // Shift+F10 opens the menu on the focused element.
  await page.evaluate(() => { location.hash = "/tasks"; });
  const taskRow = page.locator("[data-slot='task-row']").first();
  await taskRow.waitFor({ timeout: 10000 });
  await taskRow.focus();
  await page.keyboard.press("Shift+F10");
  await menu.waitFor({ timeout: 3000 }).then(() => check(true, "Shift+F10 opens the menu on the focused task"), () => check(false, "Shift+F10 opens the menu on the focused task"));
  check((await menu.locator("[role=menuitem]", { hasText: "Open in a tab" }).count()) >= 1, "a task's menu offers Open in a tab");
  await sleep(300);
  await shot("menu-task");
  await closeMenu();

  // Selected text: Copy, Ask Neko about this, Search.
  const title = page.locator("[data-slot='task-row']").first();
  await title.evaluate((el) => {
    const range = document.createRange();
    range.selectNodeContents(el.querySelector("span:not([data-slot])") ?? el);
    const sel = getSelection();
    sel.removeAllRanges();
    sel.addRange(range);
  });
  await title.click({ button: "right" });
  await menu.waitFor({ timeout: 3000 }).catch(() => {});
  check((await menu.locator("[role=menuitem]", { hasText: "Ask Neko about this" }).count()) === 1, "selected text offers Ask Neko about this");
  await menu.locator("[role=menuitem]", { hasText: "Ask Neko about this" }).click().catch(() => {});
  await page.waitForFunction(() => (document.querySelector("[data-slot='composer'] textarea")?.value ?? "").startsWith("> "), null, { timeout: 5000 })
    .then(() => check(true, "Ask Neko about this opens Home with the quote in the composer"), () => check(false, "Ask Neko about this opens Home with the quote in the composer"));
  await page.locator("[data-slot='composer'] textarea").fill("");

  for (const theme of ["dark", "light"]) {
    await page.emulateMedia({ colorScheme: theme });
    await sleep(300);
    await shot(`tabs-strip-${theme}`);
  }
  await page.emulateMedia({ colorScheme: "dark" });
  await a11y("tab strip");
}
