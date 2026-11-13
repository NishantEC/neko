// The chat directs the crew (docs/knowledge/the-chat-directs-the-crew.md): a message typed into a
// running task's composer queues, one line each with Cancel; the composer offers the three delivery
// modes (after the turn is the default, ⌘↩ steers now); a retried send is one message. (A
// delegated task's crew line is the existing handoff row; its link to the reply is a daemon test.) The seeded "Rework the export batching" task is building
// with no worker in the test daemon, so its queue holds what is sent.
//
//   NEKO_WEB_ONLY=crew node scripts/web-verify.mjs
export default async function ({ page, shot, a11y, check, sleep, PORT }) {
  const base = `http://127.0.0.1:${PORT}`;
  const request = (body) => page.evaluate(async (body) => (await fetch("/api/request", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(body) })).json(), body);
  const snapshot = async () => (await request({ Workbench: "Snapshot" }))?.Workbench ?? {};
  const TITLE = "Rework the export batching";
  const task = (await snapshot()).tasks.find((t) => t.title === TITLE);
  const queued = async () => ((await snapshot()).queued_messages ?? []).filter((m) => m.task_id === task.id && m.state === "queued");
  await page.setViewportSize({ width: Number(process.env.NEKO_WEB_WIDTH ?? 1440), height: 900 });
  await page.emulateMedia({ colorScheme: "dark" });

  // The daemon's side: a retried command id is one message; a message to a missing task is refused.
  const enqueue = (command_id, text, mode = "after_turn") => request({ Workbench: { Crew: { Enqueue: { task_id: task.id, command_id, text, mode, kind: "send" } } } });
  await enqueue("retry-1", "Use the smaller batch size");
  await enqueue("retry-1", "Use the smaller batch size");
  check((await queued()).length === 1, "a retried send (same command id) queues one message");
  check((await request({ Workbench: { Crew: { Enqueue: { task_id: "missing", text: "hi" } } } })).Error !== undefined, "a message to a task that is gone is refused");

  await page.goto(`${base}/#/tasks`);
  await page.getByLabel("List", { exact: true }).first().click();
  const row = page.locator(`[data-slot='task-row'][aria-label^='${TITLE}']`);
  await row.waitFor({ timeout: 15000 });
  await row.click();

  // The queue: one line, with its mode and a Cancel.
  const lines = page.locator("[data-slot='queued-message']");
  await lines.first().waitFor({ timeout: 10000 });
  check((await lines.count()) === 1, "the task shows its one waiting message");
  const line = await lines.first().innerText();
  check(/Queued · after the turn/.test(line) && /Use the smaller batch size/.test(line), `on one line with its mode (${line.replace(/\n/g, " | ")})`);
  check((await lines.first().getByRole("button", { name: /Cancel queued message/ }).count()) === 1, "with a Cancel");
  const height = (await lines.first().boundingBox()).height;
  check(height < 44, `one line (${Math.round(height)}px)`);

  // The composer offers the three modes; after the turn is the default.
  const modes = page.locator("[data-slot='queue-modes'] [role='radio']");
  check((await modes.count()) === 3, "the composer offers three delivery modes");
  check((await page.locator("[data-slot='queue-modes'] [data-mode='after_turn']").getAttribute("aria-checked")) === "true", "after this turn is the default");
  const box = page.locator("textarea").last();
  await box.fill("Also log the batch counts");
  await box.press("Enter");
  await page.waitForFunction(() => document.querySelectorAll("[data-slot='queued-message']").length === 2, null, { timeout: 8000 });
  check((await queued()).at(-1)?.mode === "after_turn", "Enter queues it after the turn");
  await box.fill("Skip the dry run");
  await box.press("Meta+Enter");
  await page.waitForFunction(() => document.querySelectorAll("[data-slot='queued-message']").length === 3, null, { timeout: 8000 });
  check((await queued()).at(-1)?.mode === "steer_now", "⌘↩ steers now");
  await page.locator("[data-slot='queue-modes'] [data-mode='interrupt_restart']").click();
  await box.fill("Stop and start over from the schema");
  await box.press("Enter");
  await page.waitForFunction(() => document.querySelectorAll("[data-slot='queued-message']").length === 4, null, { timeout: 8000 });
  check((await queued()).at(-1)?.mode === "interrupt_restart", "the chosen mode interrupts and restarts");
  await shot("crew-queue-dark");
  await a11y("a running task with queued messages");

  // Cancel takes one back.
  await lines.first().getByRole("button", { name: /Cancel queued message/ }).click();
  await page.waitForFunction(() => document.querySelectorAll("[data-slot='queued-message']").length === 3, null, { timeout: 8000 });
  check((await queued()).every((m) => m.text !== "Use the smaller batch size"), "Cancel takes the message back (the daemon no longer holds it)");
}
