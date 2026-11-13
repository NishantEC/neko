// The Inbox stays small by itself (DESIGN.md §8, docs/knowledge/neko-runs-the-day.md):
// open items on top, a collapsed "Closed recently" group with each item's reason,
// Reopen in the row actions and the right-click menu, and the age-out setting in
// Settings. The harness seeds three open items and closed ones for each reason
// (`seed_inbox` in web_harness.rs), one of them closed long ago.
//
//   NEKO_WEB_ONLY=inbox node scripts/web-verify.mjs
export default async function ({ page, shot, a11y, check, sleep, PORT }) {
  const base = `http://127.0.0.1:${PORT}`;
  await page.goto(`${base}/#/inbox`);
  await page.waitForSelector("[data-slot='needs-you-item']:not([data-goal])");
  check((await page.locator("[data-slot='needs-you-item']:not([data-goal])").count()) === 3, "three items are open");

  const group = page.getByRole("button", { name: /Closed recently/ });
  check((await group.getAttribute("aria-expanded")) === "false", "Closed recently starts collapsed");
  check((await page.locator("[data-slot='closed-item']").count()) === 0, "closed items are hidden while it is collapsed");
  await group.click();
  const rows = page.locator("[data-slot='closed-item']");
  await rows.first().waitFor();
  check((await rows.count()) === 5, `the last few days are listed, the one closed long ago is not (${await rows.count()})`);
  const text = await rows.allInnerTexts();
  for (const reason of ["PR merged", "Expired", "Aged out", "Resolved by routine", "Done by you"]) {
    check(text.some((t) => t.includes(reason)), `a closed item says why: ${reason}`);
  }
  check(!text.some((t) => t.includes("Closed long ago")), "an item closed more than three days ago is gone");
  await shot("inbox-closed");
  await a11y("inbox with closed items");

  // Reopen from the right-click menu.
  await rows.first().click({ button: "right" });
  const menu = page.locator("[data-slot='neko-context-menu']");
  await menu.waitFor();
  check(/Reopen/.test(await menu.innerText()), "the right-click menu offers Reopen");
  await menu.getByText("Reopen", { exact: true }).click();
  await page.waitForFunction(() => document.querySelectorAll("[data-slot='needs-you-item']:not([data-goal])").length === 4);
  check((await rows.count()) === 4, "a reopened item leaves Closed recently and returns to Needs you");

  // Reopen from the row action.
  await rows.first().hover();
  await rows.first().getByRole("button", { name: /^Reopen/ }).click();
  await page.waitForFunction(() => document.querySelectorAll("[data-slot='needs-you-item']:not([data-goal])").length === 5);
  check((await rows.count()) === 3, "the row action reopens too");

  // Marking one done closes it with "Done by you".
  await page.locator("[data-slot='needs-you-item']:not([data-goal])").first().getByRole("button", { name: /Mark .* done/ }).first().click();
  await rows.first().waitFor();
  await page.waitForFunction(() => [...document.querySelectorAll("[data-slot='closed-item']")].some((r) => r.textContent.includes("Done by you")));
  check(true, "marking an item done lands it in Closed recently");

  // The setting.
  await page.goto(`${base}/#/settings?section=ai`);
  await page.getByText("Inbox items age out after").first().waitFor();
  await shot("inbox-setting");
  check(true, "Settings shows how long an item may sit before it ages out");
}
