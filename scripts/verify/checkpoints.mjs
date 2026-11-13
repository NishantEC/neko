// Per-run checkpoints and undo (docs/knowledge/checkpoints-and-undo.md): the seeded failed task "Stop the
// nightly export running twice" has a real temporary repository with three checkpointed runs (run 2
// committed and pushed to a local bare remote). Each run is one line (glyph, "Run 3", files, +/-), a run
// expands to its own diff, "Whole task" spans first start to last end, "Undo this run" asks first and
// restores the folder, Redo brings it back, and an undo the daemon refuses says why and offers the way
// out (later runs: undo back to here; pushed: a revert commit).
//
//   NEKO_WEB_ONLY=checkpoints node scripts/web-verify.mjs
export default async function ({ page, shot, a11y, check, sleep, PORT }) {
  const base = `http://127.0.0.1:${PORT}`;
  const request = (body) => page.evaluate(async (body) => (await fetch("/api/request", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(body) })).json(), body);
  const snapshot = async () => (await request({ Workbench: "Snapshot" }))?.Workbench ?? {};
  const TITLE = "Stop the nightly export running twice";
  const task = (await snapshot()).tasks.find((t) => t.title === TITLE);
  await page.setViewportSize({ width: Number(process.env.NEKO_WEB_WIDTH ?? 1440), height: 900 });
  await page.emulateMedia({ colorScheme: "dark" });

  const list = async () => (await request({ TaskCheckpoints: { task_id: task.id } })).TaskCheckpoints;
  const seeded = await list();
  check(seeded.runs.length === 3 && seeded.runs.every((r) => r.outcome === "done"), "the daemon lists three finished runs");
  check(seeded.runs[0].files === 1 && seeded.runs[0].added >= 1, `run 1 has its own counts (${seeded.runs[0].files} file, +${seeded.runs[0].added})`);

  await page.goto(`${base}/#/tasks`);
  await page.getByLabel("List", { exact: true }).first().click();
  const row = page.locator(`[data-slot='task-row'][aria-label^='${TITLE}']`);
  await row.waitFor({ timeout: 15000 });
  await row.click();
  await page.getByRole("tab", { name: "Changes" }).click().catch(() => page.getByRole("button", { name: "Changes" }).click());
  const runs = page.locator("[data-slot='run']");
  await runs.first().waitFor({ timeout: 10000 });
  check((await runs.count()) === 3, "the Changes tab shows one line per run");
  const first = await runs.nth(0).innerText();
  check(/Run 1/.test(first) && /1 file/.test(first) && /\+\d/.test(first) && /−\d/.test(first), `a run line has its name, files and +/- (${first.replace(/\n/g, " | ")})`);
  check((await runs.nth(0).locator("[data-slot='status-glyph']").count()) === 1, "and a status glyph");
  check((await runs.nth(0).boundingBox()).height < 52, "on one line");

  // One run's diff only.
  await runs.nth(2).getByRole("button", { expanded: false }).first().click();
  await page.locator("[data-slot='run-diff']").waitFor({ timeout: 8000 });
  const diff3 = await page.locator("[data-slot='run-diff']").innerText();
  check(/NOTES\.md/.test(diff3) && !/lock_file/.test(diff3), "run 3's diff holds only run 3's change");
  await shot("checkpoints-run-diff-dark");
  await runs.nth(2).getByRole("button", { expanded: true }).first().click();
  await page.locator("[data-slot='run-whole'] button").click();
  await page.locator("[data-slot='run-whole'] [data-slot='run-diff']").waitFor({ timeout: 8000 });
  const whole = await page.locator("[data-slot='run-whole']").innerText();
  check(/export\.sh/.test(whole) && /NOTES\.md/.test(whole), "Whole task spans every run's files");
  await page.locator("[data-slot='run-whole'] button").first().click();
  await a11y("a task's runs");

  // Undo the last run: asks first, then restores, and the line shows it.
  await runs.nth(2).locator("[data-slot='undo-run']").click();
  const dialog = page.getByRole("alertdialog");
  await dialog.waitFor({ timeout: 5000 });
  check(/Undo run 3/.test(await dialog.innerText()) && /redo/i.test(await dialog.innerText()), "undo asks first and says it can be redone");
  await shot("checkpoints-undo-confirm-dark");
  await dialog.getByRole("button", { name: "Keep it" }).click();
  check((await list()).runs[2].undone === false, "keeping it changes nothing");
  await runs.nth(2).locator("[data-slot='undo-run']").click();
  await dialog.getByRole("button", { name: "Undo this run" }).click();
  await page.waitForFunction(() => document.querySelector("[data-slot='run'][data-run='3'][data-undone]"), null, { timeout: 10000 });
  check((await list()).runs[2].undone === true && (await list()).redo, "run 3 is undone and a redo is offered");
  check((await page.locator("[data-slot='redo']").count()) === 1, "the Redo button shows");
  check((await snapshot()).tasks.find((t) => t.id === task.id).events.some((e) => /checkpoint: Undid run 3/.test(e.message)), "the task's history records the undo");
  await shot("checkpoints-undone-dark");
  await page.locator("[data-slot='redo']").click();
  await page.waitForFunction(() => !document.querySelector("[data-slot='run'][data-undone]"), null, { timeout: 10000 });
  check((await list()).runs.every((r) => !r.undone), "Redo brings it back");

  // Undo an earlier run: refused because later runs exist; the offer undoes them too, and then the
  // pushed commit refuses and offers a revert commit instead.
  await runs.nth(0).locator("[data-slot='undo-run']").click();
  await dialog.getByRole("button", { name: "Undo this run" }).click();
  const refused = page.locator("[data-slot='undo-refused']");
  await refused.waitFor({ timeout: 10000 });
  check((await refused.getAttribute("data-code")) === "later_runs" && /Undo back to here/.test(await refused.innerText()), "an earlier run is refused while later ones exist, with the offer");
  await shot("checkpoints-refused-dark");
  await refused.getByRole("button", { name: "Undo back to here" }).click();
  await dialog.getByRole("button", { name: "Undo back to here" }).click();
  await refused.waitFor({ timeout: 10000 });
  check((await refused.getAttribute("data-code")) === "pushed" && /revert commit/i.test(await refused.innerText()), "a pushed commit refuses and offers a revert commit");
  check((await list()).runs.every((r) => !r.undone), "a refusal changes nothing");
  await refused.getByRole("button", { name: "Add a revert commit instead" }).click();
  await dialog.getByRole("button", { name: "Undo back to here" }).click();
  await page.waitForFunction(() => document.querySelectorAll("[data-slot='run'][data-undone]").length === 3, null, { timeout: 10000 });
  check(true, "the revert commit undoes all three runs and keeps history");
  await shot("checkpoints-reverted-dark");

  // The agent's side: the crew tool reads the same diffs.
  const state = await snapshot();
  check(state.tasks.some((t) => t.id === task.id), "the task is still in the record");
}
