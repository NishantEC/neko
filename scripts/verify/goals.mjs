// Goals that keep working (docs/knowledge/goals-that-keep-working.md; DESIGN.md §8): a goal is
// one line on its task and in Home's chat (status glyph, objective, "turn 4/20", the last check),
// opens to its evidence ledger and journal, offers Pause, Resume, Extend, Mark done and Stop as
// row actions and in the right-click menu, and the Inbox carries its question and its
// "Extend by N turns?" with a button that answers it. Home's crew line counts the active goals.
// The harness seeds three goals (scripts/fixtures/web-seed.mjs): one running with a failing
// check, one blocked on a question and set in Home's chat, one paused at its turn cap.
//
//   NEKO_WEB_ONLY=goals node scripts/web-verify.mjs
export default async function ({ page, shot, a11y, check, sleep, PORT }) {
  const base = `http://127.0.0.1:${PORT}`;
  const request = (body) => page.evaluate(async (body) => (await fetch("/api/request", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(body) })).json(), body);
  const goals = async () => (await request({ Workbench: "Snapshot" }))?.Workbench?.goals ?? [];
  const goalOf = async (text) => (await goals()).find((g) => g.objective.includes(text));
  const menu = page.locator("[data-slot='neko-context-menu']");
  const RUNNING = "Retry timed-out webhook sends";
  await page.setViewportSize({ width: Number(process.env.NEKO_WEB_WIDTH ?? 1440), height: 900 });
  await page.emulateMedia({ colorScheme: "dark" });

  // Home: the goal set in the chat shows under the crew line, which counts active goals.
  await page.goto(`${base}/#/`);
  const home = page.locator("[data-slot='home-goals'] [data-slot='goal']");
  await home.first().waitFor({ timeout: 15000 });
  check((await home.count()) === 1, "Home shows the one goal set in its chat");
  check((await home.first().getAttribute("data-status")) === "Blocked", "and it is the blocked one");
  check(/Which database should the reports job tests run against/.test(await home.first().innerText()), "its one question is on the line, with a way to answer");
  check((await home.first().getByRole("button", { name: "Use the default" }).count()) === 1, "Use the default is offered");
  const crew = (await page.locator("[data-slot='crew-goals']").innerText()).trim();
  check(crew === "2 goals", `Home's crew line counts the active goals (${crew}); the paused one is not active`);
  await shot("goals-home-dark");
  await a11y("Home with a goal");

  // The task: one line with the glyph, objective, turn and the last check.
  await page.goto(`${base}/#/tasks`);
  await page.getByLabel("List", { exact: true }).first().click();
  const row = page.locator("[data-slot='task-row'][aria-label^='Add a retry to the webhook sender']");
  await row.waitFor({ timeout: 15000 });
  await row.click();
  const line = page.locator("[data-slot='goal']", { hasText: RUNNING });
  await line.waitFor({ timeout: 10000 });
  const text = await line.first().innerText();
  check(/turn 4\/20/.test(text) && /check failing \(exit 101\)/.test(text), `the line says "turn 4/20" and the last check (${text.replace(/\n/g, " | ")})`);
  check((await line.first().locator("[data-slot='status-glyph']").getAttribute("aria-label")) === "Goal: Working", "the glyph is named for the goal's state");
  const height = (await line.first().locator("> div").first().boundingBox()).height;
  check(height < 44, `it is one line (${Math.round(height)}px)`);

  // Expanded: the evidence ledger and the journal.
  const toggle = line.first().getByRole("button", { name: new RegExp(RUNNING) });
  check((await toggle.getAttribute("aria-expanded")) === "false", "it starts collapsed");
  await toggle.click();
  const detail = line.first().locator("[data-slot='goal-detail']");
  await detail.waitFor();
  const body = await detail.innerText();
  check(/pnpm test src\/http/.test(body) && /exit 101/.test(body) && /expected 3 calls, received 2/.test(body), "the ledger names the command, the exit code and the end of its output");
  check(/Wrote the back-off/.test(body) && /Stalled with no output for ten minutes/.test(body) && /Next: Fix the off-by-one/.test(body), "the journal says what each turn did, what is next and the stall");
  for (const scheme of ["dark", "light"]) {
    await page.emulateMedia({ colorScheme: scheme });
    await page.waitForFunction((s) => document.documentElement.classList.contains(s), scheme, { timeout: 3000 });
    await page.mouse.move(0, 0);
    await sleep(600);
    await shot(`goals-task-${scheme}`);
  }
  await page.emulateMedia({ colorScheme: "dark" });
  await a11y("a task with its goal expanded");

  // Actions: named buttons on hover or focus, and every one in the right-click menu.
  const target = line.first().locator("> div").first();
  await target.hover();
  for (const name of ["Pause", "Extend by 10 turns", "Mark done", "Stop"]) {
    check((await target.getByRole("button", { name, exact: true }).count()) === 1, `the row has a ${name} action`);
  }
  await target.click({ button: "right" });
  for (const name of ["Pause", "Extend by 10 turns", "Mark done", "Stop"]) {
    check((await menu.getByRole("menuitem", { name, exact: true }).count()) === 1, `the right-click menu has ${name}`);
  }

  // Pause from the right-click menu, resume from the row: the daemon's goal row follows.
  await menu.getByRole("menuitem", { name: "Pause", exact: true }).click();
  await page.waitForFunction(() => document.querySelector("[data-slot='goal'][data-status='Paused']"), null, { timeout: 8000 })
    .then(() => check(true, "Pause from the right-click menu pauses the goal"), () => check(false, "Pause from the right-click menu pauses the goal"));
  check((await goalOf(RUNNING))?.status === "Paused", "the daemon's goal row says paused");
  await target.hover();
  await target.getByRole("button", { name: "Resume", exact: true }).click();
  await page.waitForFunction(() => document.querySelector("[data-slot='goal'][data-status='Running']"), null, { timeout: 8000 })
    .then(() => check(true, "Resume from the row runs it again"), () => check(false, "Resume from the row runs it again"));

  // Extend adds turns.
  await target.hover();
  await target.getByRole("button", { name: "Extend by 10 turns", exact: true }).click();
  await sleep(500);
  check((await goalOf(RUNNING))?.budget?.max_turns === 30, "Extend by 10 turns moves the cap to 30");

  // Mark done finishes a goal; a finished goal offers no actions.
  await target.hover();
  await target.getByRole("button", { name: "Mark done", exact: true }).click();
  await page.waitForFunction(() => document.querySelector("[data-slot='goal'][data-status='Done']"), null, { timeout: 8000 })
    .then(() => check(true, "Mark done finishes the goal"), () => check(false, "Mark done finishes the goal"));
  check((await page.locator("[data-slot='goal'][data-status='Done'] [data-slot='row-actions'] button").count()) === 0, "a finished goal offers no actions");

  // Inbox: the question and "Extend by N turns?" are items; their buttons answer them. The
  // harness raises them only when this area is asked for alone (the Inbox area counts its items).
  if (!(process.env.NEKO_WEB_ONLY ?? "").split(",").includes("goals")) return;
  await page.goto(`${base}/#/inbox`);
  const items = page.locator("[data-slot='needs-you-item'][data-goal]");
  await items.first().waitFor({ timeout: 15000 });
  check((await items.count()) === 2, "the Inbox carries the goal's question and its extend offer");
  const html = await page.locator("main").first().innerText().catch(() => "");
  check(!/Goals that keep working report here/.test(html), "the hidden routine that holds goal items is not a routine in the list");
  const extend = page.locator("[data-slot='goal-item-actions']").getByRole("button", { name: /^Extend by \d+ turns$/ });
  await extend.first().waitFor({ timeout: 5000 });
  await shot("goals-inbox-dark");
  await a11y("Inbox with goal items");
  await extend.first().click();
  await sleep(600);
  const documented = await goalOf("Document the reports job");
  check(documented?.status === "Running" && documented.budget.max_turns === 30, `Extend from the Inbox resumes the paused goal with 10 more turns (${documented?.status}, ${documented?.budget?.max_turns})`);
  const answer = page.locator("[data-slot='goal-answer']").first();
  await answer.getByRole("button", { name: "Use the default" }).click();
  await sleep(600);
  const reports = await goalOf("Keep the reports job");
  check(reports?.status === "Running" && reports.in_turn === false, `Use the default answers a blocked goal and it runs again (${reports?.status})`);
}
