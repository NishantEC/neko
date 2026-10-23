// The docking layout on the full-page task view (web/src/work/dock, dockview):
// the default layout, moving a tab into another group (real mouse drag), splitting
// by dropping on an edge, closing and reopening from the View menu, ⌘K actions,
// the layout surviving a reload, Reset layout, a corrupted saved layout, and the
// narrow single-group fallback. Screenshots in dark and light.
const KEY = "neko:dock:task";

export default async function ({ page, shot, a11y, check, sleep, PORT }) {
  const base = `http://127.0.0.1:${PORT}`;
  const both = async (name) => {
    for (const scheme of ["dark", "light"]) {
      await page.emulateMedia({ colorScheme: scheme });
      await page.waitForFunction((s) => document.documentElement.classList.contains(s), scheme, { timeout: 3000 });
      await sleep(300);
      await shot(`${name}-${scheme}`);
      await a11y(`${name} (${scheme})`);
    }
    await page.emulateMedia({ colorScheme: "dark" });
    await sleep(200);
  };

  await page.setViewportSize({ width: 1440, height: 900 });
  await page.emulateMedia({ colorScheme: "dark" });
  await page.goto(`${base}/#/tasks`);
  await page.locator("[data-slot='task-row']").first().waitFor({ timeout: 15000 });
  const taskId = await page.locator("[data-slot='task-row']").first().getAttribute("data-task-id");

  const openFull = async () => {
    await page.goto(`${base}/#/tasks?task=${taskId}&view=full`);
    await page.reload();
    await page.locator("[data-slot='dock'] .dv-groupview").first().waitFor({ timeout: 10000 });
    await page.waitForFunction((k) => window.__nekoDock?.[k]?.api?.panels.length > 0, KEY, { timeout: 5000 });
    await sleep(500);
  };
  const groups = () =>
    page.evaluate((k) => {
      const api = window.__nekoDock[k].api;
      return api.groups
        .map((g) => { const r = g.element.getBoundingClientRect(); return { x: Math.round(r.x), y: Math.round(r.y), panels: g.panels.map((p) => p.id), active: g.activePanel?.id }; })
        .sort((a, b) => a.x - b.x || a.y - b.y)
        .map(({ panels, active }) => ({ panels, active }));
    }, KEY);
  const saved = () => page.evaluate((k) => localStorage.getItem(k), KEY);
  const same = (a, b) => JSON.stringify(a) === JSON.stringify(b);
  const DEFAULT = [{ panels: ["chat"], active: "chat" }, { panels: ["result", "changes", "plan", "details"], active: "changes" }];
  const tab = (id) => page.locator(`[data-slot='dock'] .dv-tab:has([data-panel='${id}'])`);
  const box = async (loc) => (await loc.boundingBox());
  /** A real mouse drag: down, a few steps so dockview sees the drag begin, up. */
  const drag = async (from, to, label) => {
    const a = await box(from);
    await page.mouse.move(a.x + a.width / 2, a.y + a.height / 2);
    await page.mouse.down();
    await page.mouse.move(a.x + a.width / 2 + 12, a.y + a.height / 2 + 12, { steps: 4 });
    await page.mouse.move(to.x, to.y, { steps: 14 });
    await sleep(250);
    await page.mouse.move(to.x + 1, to.y + 1, { steps: 2 });
    await sleep(150);
    if (label) await shot(label);
    await page.mouse.up();
    await sleep(500);
  };

  // ===== (a) the default layout ==========================================================
  await page.evaluate((k) => localStorage.removeItem(k), KEY);
  await openFull();
  check(same(await groups(), DEFAULT), `default layout: Chat on the left, Changes / Plan / Details grouped on the right (${JSON.stringify(await groups())})`);
  const chatBox = await box(tab("chat"));
  const changesBox = await box(tab("changes"));
  check(chatBox && changesBox && chatBox.x < changesBox.x && Math.abs(chatBox.y - changesBox.y) < 4, "Chat and Changes sit side by side");
  check((await page.locator("[data-slot='dock'] .dv-groupview").count()) === 2, "two panes");
  check((await page.locator("[data-slot='dock-chat']").count()) === 1 && (await page.locator("[data-slot='dock-chat'] textarea, [data-slot='dock-chat'] [contenteditable]").count()) >= 1, "the conversation and its composer are in the Chat pane");
  const dockBox = await box(page.locator("[data-slot='dock']"));
  const paneWidth = (await box(page.locator("[data-slot='dock'] .dv-groupview").first())).width;
  check(Math.abs(paneWidth / dockBox.width - 0.35) < 0.06, `Chat takes about 35% of the width (${Math.round((paneWidth / dockBox.width) * 100)}%)`);
  // fused tabs: each group has its OWN indicator, the pane's surface, 40px tall with concave feet,
  // sitting exactly under the active tab; idle tabs are recessed 36px icon-only pills
  const fused = await page.evaluate(() => {
    const out = [];
    for (const g of document.querySelectorAll("[data-slot='dock'] .dv-groupview")) {
      const act = g.querySelector(".dv-tab.dv-active-tab");
      const idle = g.querySelector(".dv-tab.dv-inactive-tab");
      const ind = g.querySelector(".dock-indicator");
      const pane = getComputedStyle(g.querySelector(".dv-content-container")).backgroundColor;
      const ir = ind?.getBoundingClientRect();
      const ar = act.getBoundingClientRect();
      out.push({
        n: g.querySelectorAll(".dock-indicator").length,
        same: !!ind && getComputedStyle(ind).backgroundColor === pane,
        h: ind ? Math.round(ir.height) : 0,
        aligned: !!ind && Math.abs(ir.left - ar.left) < 1.5 && Math.abs(ir.width - ar.width) < 1.5 && Math.abs(ir.bottom - ar.bottom) < 1.5,
        strip: Math.round(g.querySelector(".dv-tabs-and-actions-container").getBoundingClientRect().height),
        idleH: idle ? Math.round(idle.getBoundingClientRect().height) : 36,
        feet: !!ind && getComputedStyle(ind, "::before").backgroundImage.includes("radial-gradient") && getComputedStyle(ind, "::after").backgroundImage.includes("radial-gradient"),
      });
    }
    return out;
  });
  check(fused.length > 0 && fused.every((f) => f.n === 1 && f.same && f.h === 41 && f.aligned && f.strip === 52 && f.idleH === 36 && f.feet), `every group has its own fused indicator under its active tab (card colour, 40px, feet; 52px strip; idle tabs 36px) ${JSON.stringify(fused)}`);

  // the indicator SLIDES: click another tab in the group and sample its box twice inside the transition
  const ind = page.locator("[data-slot='dock'] .dv-groupview").nth(1).locator(".dock-indicator");
  const startX = (await box(ind)).x;
  await tab("plan").click();
  await sleep(90);
  const mid1 = (await box(ind)).x;
  await sleep(60);
  const mid2 = (await box(ind)).x;
  await sleep(600);
  const endBox = await box(ind);
  const planBox = await box(tab("plan"));
  check(mid1 !== startX && mid2 !== mid1 && Math.abs(mid1 - startX) < Math.abs(endBox.x - startX), `the indicator animates between tabs (${startX.toFixed(1)} -> ${mid1.toFixed(1)} -> ${mid2.toFixed(1)} -> ${endBox.x.toFixed(1)})`);
  check(Math.abs(endBox.x - planBox.x) < 1.5 && Math.abs(endBox.width - planBox.width) < 1.5, `…and ends exactly under the new active tab (${JSON.stringify([endBox, planBox])})`);
  await tab("plan").click(); // already active: nothing moves
  // reduced motion: a click jumps at once
  await page.emulateMedia({ reducedMotion: "reduce" });
  await sleep(100);
  const rStart = (await box(ind)).x;
  await tab("changes").click();
  await sleep(40);
  const rNow = (await box(ind)).x;
  const rEnd = (await box(tab("changes"))).x;
  check(Math.abs(rNow - rEnd) < 2 && Math.abs(rNow - rStart) > 5, `with reduced motion the indicator jumps instantly (${rStart.toFixed(1)} -> ${rNow.toFixed(1)})`);
  await page.emulateMedia({ reducedMotion: "no-preference" });
  await sleep(100);
  // settled mid-animation shots
  await tab("plan").click();
  await sleep(110);
  await shot("dock-indicator-mid-slide");
  await sleep(500);
  await tab("changes").click();
  await sleep(500);
  await both("dock-default");

  // keyboard: arrow keys move between the tabs of a group, Enter activates
  await tab("changes").focus();
  await page.keyboard.press("ArrowRight");
  const focusedTab = await page.evaluate(() => document.activeElement?.getAttribute("aria-label"));
  check(focusedTab === "Plan", `ArrowRight moves focus to the next tab (${focusedTab})`);
  await page.keyboard.press("Enter");
  await sleep(200);
  check((await groups())[1].active === "plan", "Enter activates the focused tab");
  check((await page.getByRole("button", { name: "Close Plan" }).count()) === 1, "close buttons are labelled");
  await tab("changes").click();

  // ===== (b) move Changes into the Chat group (real drag onto the middle of the Chat pane) ===========
  const composer = page.locator("[data-slot='dock-chat'] textarea, [data-slot='dock-chat'] [contenteditable='true']").first();
  await composer.fill("a draft that must survive");
  const draft = () => composer.evaluate((el) => ("value" in el ? el.value : el.innerText));
  const chatBody = await box(page.locator("[data-slot='dock'] .dv-groupview").first().locator(".dv-content-container"));
  await drag(tab("changes"), { x: chatBody.x + chatBody.width / 2, y: chatBody.y + chatBody.height / 2 }, "dock-dragging-to-centre");
  let now = await groups();
  const together = now.some((g) => g.panels.includes("chat") && g.panels.includes("changes"));
  check(together, `dragging Changes onto the middle of the Chat pane puts both in one group (${JSON.stringify(now)})`);
  if (together) {
    const a = await box(tab("chat"));
    const b = await box(tab("changes"));
    check(Math.abs(a.y - b.y) < 4, "…and they share one tab strip");
    await both("dock-grouped");
    // Changes is in front now, so Chat is hidden behind it: it keeps its draft and comes back as it was.
    await tab("chat").click();
    await sleep(300);
    check((await draft()).includes("a draft that must survive"), "a hidden Chat keeps its composer draft when its tab comes back to the front");
    await tab("changes").click();
    await sleep(200);
    check((await composer.count()) === 1, "…and stays mounted behind another tab (live updates keep arriving)");
  }

  // ===== (c) split by dropping on an edge ===============================================
  // Plan onto the left edge of the group that now holds Chat: a new pane appears beside it.
  const before = (await groups()).length;
  const target = await box(page.locator("[data-slot='dock'] .dv-groupview").first().locator(".dv-content-container"));
  await drag(tab("plan"), { x: target.x + 8, y: target.y + target.height / 2 });
  now = await groups();
  check(now.length === before + 1 && now.some((g) => g.panels.length === 1 && g.panels[0] === "plan"), `dropping a tab on a pane's edge splits it (${JSON.stringify(now)})`);
  check(await page.evaluate(() => [...document.querySelectorAll("[data-slot='dock'] .dv-tabs-container")].filter((e) => e.clientWidth > 330).every((e) => e.scrollWidth <= e.clientWidth + 1)), "the tab strips of wide panes do not overflow when their tabs fit (the concave feet do not start a scrollbar)")
  const splitInd = await page.evaluate(() => [...document.querySelectorAll("[data-slot='dock'] .dv-groupview")].map((g) => { const i = g.querySelector(".dock-indicator"), a = g.querySelector(".dv-tab.dv-active-tab"); if (!i || !a) return false; const x = i.getBoundingClientRect(), y = a.getBoundingClientRect(); return Math.abs(x.left - y.left) < 1.5 && Math.abs(x.width - y.width) < 1.5 ? true : [x.left, x.width, y.left, y.width].map(Math.round); }));
  check(splitInd.length >= 3 && splitInd.every((v) => v === true), `in a split layout every group has an indicator under its active tab (${JSON.stringify(splitInd)})`);
  await both("dock-split");

  // ===== (d) the layout survives a reload ================================================
  await sleep(400);
  const beforeReload = await groups();
  await page.reload();
  await page.locator("[data-slot='dock'] .dv-groupview").first().waitFor({ timeout: 10000 });
  await page.waitForFunction((k) => window.__nekoDock?.[k]?.api?.panels.length > 0, KEY, { timeout: 5000 });
  await sleep(500);
  check(same(await groups(), beforeReload), `the layout survives a reload (${JSON.stringify(await groups())})`);
  const persisted = JSON.parse(await saved());
  check(persisted.v === 1 && typeof persisted.layout === "object" && Array.isArray(persisted.closed), "one layout is saved for the person, not per task");

  // ===== close and reopen from the View menu; ⌘K ==========================================
  await tab("details").click();
  await sleep(450);
  await page.getByRole("button", { name: "Close Details" }).click();
  await sleep(300);
  check(!(await groups()).some((g) => g.panels.includes("details")), "closing a tab removes the panel");
  await page.getByRole("button", { name: "View: choose panels" }).click();
  await page.getByRole("menuitemcheckbox", { name: "Details" }).waitFor({ timeout: 3000 });
  check((await page.getByRole("menuitemcheckbox", { name: "Details", checked: true }).count()) === 0 && (await page.getByRole("menuitemcheckbox", { name: "Chat", checked: true }).count()) === 1, "the View menu ticks the open panels and not the closed one");
  await sleep(350);
  await shot("dock-view-menu");
  await page.getByRole("menuitemcheckbox", { name: "Details" }).click();
  await page.keyboard.press("Escape");
  await sleep(600);
  check((await groups()).some((g) => g.panels.includes("details")), "choosing a closed panel in the View menu reopens it");
  await page.getByRole("button", { name: "Close Details" }).click();
  await sleep(300);
  await page.keyboard.press("Meta+k");
  await page.locator("input[aria-label='Command palette']").fill("View: Details");
  await sleep(300);
  await page.keyboard.press("Enter");
  await sleep(500);
  check((await groups()).some((g) => g.panels.includes("details")), "⌘K “View: Details” reopens a closed panel");
  await page.keyboard.press("Meta+k");
  await page.locator("input[aria-label='Command palette']").fill("Split right");
  await sleep(300);
  const hasSplit = (await page.getByRole("option", { name: /Split right/ }).count()) >= 1;
  check(hasSplit, "⌘K lists “Split right”");
  await page.keyboard.press("Escape");

  // maximise a group by double-clicking its tab
  await tab("details").dblclick();
  await sleep(400);
  check(await page.evaluate((k) => window.__nekoDock[k].api.hasMaximizedGroup(), KEY), "double-clicking a tab maximises its pane");
  await shot("dock-maximised");
  await tab("details").dblclick();
  await sleep(300);
  check(!(await page.evaluate((k) => window.__nekoDock[k].api.hasMaximizedGroup(), KEY)), "double-clicking again restores it");

  // the tab context menu (close, maximise, float) and a floating pane
  await tab("plan").click({ button: "right" });
  await sleep(300);
  const menuText = (await page.locator("[role=menu]").allInnerTexts()).join(" | ").replace(/\s+/g, " ");
  check(/Close/.test(menuText) && /Float/i.test(menuText), `right-clicking a tab offers close / maximise / float (${menuText})`);
  await shot("dock-tab-menu");
  const floatItem = page.locator("[role=menuitem]").filter({ hasText: /Float/i }).first();
  if (await floatItem.count()) {
    await floatItem.click();
    await sleep(500);
    check((await page.locator("[data-slot='dock'] .dv-resize-container").count()) >= 1, "Float moves a pane into a floating window");
    await shot("dock-floating");
  } else await page.keyboard.press("Escape");

  // ===== (e) Reset layout ===============================================================
  await page.getByRole("button", { name: "View: choose panels" }).click();
  await page.getByRole("menuitem", { name: "Reset layout" }).click();
  await sleep(600);
  check(same(await groups(), DEFAULT), `Reset layout restores the default (${JSON.stringify(await groups())})`);

  // ===== (f) a corrupted saved layout falls back to the default =========================
  const bad = [
    "not json at all",
    JSON.stringify({ v: 1, layout: { grid: { root: { type: "branch", data: [] }, width: 10, height: 10, orientation: "HORIZONTAL" }, panels: { chat: { id: "chat", contentComponent: "panel" } } }, closed: [] }),
    JSON.stringify({ v: 1, layout: { grid: { root: { type: "leaf", data: { views: ["nope"], id: "1" }, size: 100 }, width: 100, height: 100, orientation: "HORIZONTAL" }, panels: { nope: { id: "nope", contentComponent: "panel" } } }, closed: ["chat", "changes", "plan", "details"] }),
    JSON.stringify({ v: 1, layout: { panels: [] }, closed: [] }),
    JSON.stringify({ v: 1, layout: { grid: { root: { type: "leaf", data: { views: ["chat"], id: "1" }, size: 100 }, width: 100, height: 100, orientation: "HORIZONTAL" }, panels: { chat: { id: "chat", contentComponent: "evil" } } }, closed: [] }),
  ];
  for (const [i, value] of bad.entries()) {
    await page.evaluate(([k, v]) => localStorage.setItem(k, v), [KEY, value]);
    await openFull();
    const g = await groups();
    const all = g.flatMap((x) => x.panels).sort().join(",");
    check(all === "changes,chat,details,plan,result" && g.length >= 1, `a corrupted saved layout (#${i + 1}) never leaves the view empty (${JSON.stringify(g)})`);
    check(same(g, DEFAULT) || i === 2, `…and falls back to the default (#${i + 1})`);
  }
  // an unknown panel in an otherwise valid layout is dropped; a missing one is put back
  await page.evaluate((k) => localStorage.removeItem(k), KEY);
  await openFull();
  await page.evaluate((k) => {
    const s = JSON.parse(localStorage.getItem(k));
    s.layout.panels.ghost = { ...s.layout.panels.plan, id: "ghost" };
    const leaf = (n) => (n.type === "leaf" ? n : n.data.map(leaf).flat());
    const leaves = [leaf(s.layout.grid.root)].flat(Infinity);
    leaves[1].data.views.push("ghost");
    leaves[1].data.views = leaves[1].data.views.filter((v) => v !== "plan");
    delete s.layout.panels.plan;
    localStorage.setItem(k, JSON.stringify(s));
  }, KEY);
  await openFull();
  const healed = (await groups()).flatMap((x) => x.panels).sort().join(",");
  check(healed === "changes,chat,details,plan,result", `an unknown panel id is dropped and a missing one is put back (${healed})`);

  // ===== narrow: one tabbed group, no splitting ===========================================
  await page.evaluate((k) => localStorage.removeItem(k), KEY);
  await page.setViewportSize({ width: 760, height: 900 });
  await openFull();
  const narrow = await groups();
  check(narrow.length === 1 && narrow[0].panels.length === 5, `below 900px the dock is one tabbed group (${JSON.stringify(narrow)})`);
  await both("dock-narrow");
  await page.setViewportSize({ width: 1440, height: 900 });
  await sleep(800);
  check(same(await groups(), DEFAULT), "growing the window brings the wide layout back");
  await sleep(200);
  await page.evaluate((k) => localStorage.removeItem(k), KEY);
}
