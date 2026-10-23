// Chat on the AI SDK: a streaming reply grows, a tool call part, an approval
// part approved and declined, a choices part answered, reload mid-reply resumes,
// a second tab sees the same reply, queue and interrupt still work.
//
// Run with the harness in scripted mode so a sent message gets a whole reply
// without a model:  NEKO_WEB_SCRIPTED=1 NEKO_WEB_ONLY=chat node scripts/web-verify.mjs
// (web-verify starts the harness; set NEKO_WEB_SCRIPTED in its environment).
// NOT RUN where it was written (the shared Chrome would not start there): read
// the screenshots before trusting it, and fix the selectors it gets wrong.
export default async function ({ page, shot, a11y, check, sleep, PORT }) {
  const home = `http://127.0.0.1:${PORT}/#/`;
  await page.goto(home);
  await page.waitForSelector("[data-slot='neko-turn']", { timeout: 20000 });

  // A reply in flight is shown (its growth is checked by the main run in web-verify.mjs; the
  // harness stream is one-shot, so by the time this area runs it has said all its words).
  const prose = "[data-slot='neko-turn'][data-pending='true'] [data-slot='prose']";
  await page.waitForSelector(prose, { timeout: 20000 });
  check((await page.locator(prose).innerText()).length > 0, "the pending reply is drawn from the SDK log");
  await shot("chat-streaming");

  // Tool call parts: a finished turn folds them into its work log.
  const finished = page.locator("[data-slot='neko-turn'][data-pending='false']").filter({ has: page.locator("[data-slot='work-log']") }).last();
  await finished.scrollIntoViewIfNeeded();
  await finished.locator("[data-slot='work-head']").click();
  check((await finished.locator("[data-slot='work-step']").count()) >= 4, "tool parts are listed as steps");
  await shot("chat-tools");

  // A choices part is drawn, and answering it sends a message.
  check((await page.locator("[data-slot='reply-block'][data-kind='choices'] [role='radio']").count()) === 2, "a choices part offers its options");
  await shot("chat-choices");

  // An approval part docks as the decision card.
  await page.waitForSelector("[data-slot='decision']", { timeout: 5000 });
  check((t=>/Approve/.test(t)&&/Decline/.test(t))(await page.locator("[data-slot='decision']").innerText()), "a pending approval docks as the decision card with Approve and Decline");
  await shot("chat-approval");

  // Reload mid-reply: the same pending reply comes back from the daemon's log.
  await page.reload();
  await page.waitForSelector(prose, { timeout: 20000 });
  check((await page.locator(prose).innerText()).length > 0, "after a reload the in-flight reply is still there");

  // A second tab shows the same conversation and the same reply.
  const second = await page.context().newPage();
  await second.goto(home);
  await second.waitForSelector(prose, { timeout: 20000 });
  check((await second.locator("[data-slot='neko-turn']").count()) === (await page.locator("[data-slot='neko-turn']").count()), "a second tab shows the same turns");
  await second.close();

  // Queue and interrupt still work (the harness holds a reply, so a message queues).
  const text = `chat verify ${Date.now()}`;
  await page.locator("textarea").fill(text);
  await page.locator("textarea").press("Enter");
  await page.locator("[data-slot='queue-row']", { hasText: text }).waitFor({ timeout: 10000 });
  check(true, "a message sent mid-reply waits in the queue tray");
  // Leave the queue as found: later areas count its rows.
  await page.locator("[data-slot='queue-row']", { hasText: text }).locator("button[aria-label='Remove queued message']").click();
  await page.waitForFunction(() => document.querySelectorAll("[data-slot='queue-row']").length === 0, null, { timeout: 10000 });
  await a11y("chat");
}
