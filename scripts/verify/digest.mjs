// The morning digest (docs/knowledge/morning-digest.md): composed once a day from data Neko
// already has, shown as one calm block at the top of Home (status, bottleneck, what needs
// you, with defaults), hidden once read, and its time is a setting in Settings → Phone.
// Counts are not asserted: other areas seed their own goals and items, so the check looks
// for what the harness's Inbox seed always has (an item with a default, items without).
//
//   NEKO_WEB_ONLY=digest node scripts/web-verify.mjs
export default async function ({ page, shot, a11y, check, PORT }) {
  const base = `http://127.0.0.1:${PORT}`;
  await page.goto(`${base}/#/settings?section=phone`);
  await page.waitForSelector("[data-slot='phone-digest']");
  check((await page.getByRole("switch", { name: "Morning digest", exact: true }).getAttribute("aria-checked")) === "true", "the digest is on by default");
  const time = page.getByLabel("Digest time");
  check((await time.inputValue()) === "08:30", "the digest time defaults to 08:30");
  check(/Replies need Apple Reminders/.test(await page.locator("[data-slot='phone-replies']").innerText()), "Settings says replies need Reminders");

  // A time that has just passed makes it due: the first Home that opens composes it.
  const hour = String(new Date().getHours()).padStart(2, "0");
  await time.fill(`${hour}:00`);
  await time.blur();
  await page.waitForFunction((h) => document.querySelector("[aria-label='Digest time']")?.value === `${h}:00`, hour);

  await page.goto(`${base}/#/`);
  const block = page.locator("[data-slot='home-digest']");
  await block.waitFor({ timeout: 15000 });
  const headline = await page.locator("[data-slot='digest-headline']").innerText();
  check(/need you/.test(headline) || /Nothing needs you/.test(headline), `the headline says what needs you (${headline})`);
  check(/finished/.test(headline) && /stuck/.test(headline), "it leads with the status and the bottleneck");
  check((await page.locator("[data-slot='digest-details']").count()) === 0, "the details start closed");
  await block.getByRole("button", { name: /Morning digest/ }).click();
  const decisions = page.locator("[data-slot='digest-decisions']");
  await decisions.waitFor();
  const text = await decisions.innerText();
  check(/default: Approve it with a comment/.test(text), "a decision shows its default");
  check(/No default/.test(text), "a decision without one says so");
  await shot("home-digest");
  await a11y("home with the morning digest");

  // Reading it hides it, and it is not composed again the same day.
  await block.hover();
  await block.getByRole("button", { name: "Mark the digest read" }).click();
  await block.waitFor({ state: "detached" });
  await page.goto(`${base}/#/inbox`);
  await page.goto(`${base}/#/`);
  await page.waitForSelector("[data-slot='home-goals'], textarea, [data-slot='conversation']", { timeout: 15000 }).catch(() => {});
  check((await page.locator("[data-slot='home-digest']").count()) === 0, "a read digest stays hidden for the day");

  // The Inbox shows each escalation's default and takes it with one click.
  await page.goto(`${base}/#/inbox`);
  await page.waitForSelector("[data-slot='needs-you-item']:not([data-goal])");
  const defaults = await page.locator("[data-slot='item-default']").allInnerTexts();
  check(defaults.some((t) => /Default: Approve it with a comment/.test(t)), "an Inbox item shows its default");
  check(defaults.some((t) => /No default/.test(t)), "an Inbox item without one says No default");
}
