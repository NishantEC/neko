// Settings → Neko updates: Neko applying its own landed changes. The test
// harness seeds a last update, a rollback and an update that waits for a click
// (it changes safety-critical code); this checks the section shows them, that
// the auto-update switches toggle and revert, and that Apply takes the pending
// update off the list.
//
//   NEKO_WEB_ONLY=updates node scripts/web-verify.mjs   (see scripts/verify/README.md)

export default async function ({ page, shot, a11y, check, sleep, PORT }) {
  await page.goto(`http://127.0.0.1:${PORT}/#/settings`);
  await page.waitForSelector("[data-slot='settings-nav']");
  await page.locator("[data-slot='settings-nav']").getByRole("button", { name: "Neko updates", exact: true }).click();
  const section = page.locator("[data-slot='neko-updates']");
  await section.waitFor();
  const text = await section.innerText();
  check(text.includes("App 9acc5352") && text.includes("UI c7b6c58d"), "the current app and UI versions are shown");
  check(text.includes("Update waiting: d03c977a") && text.includes("safety-critical"), "the pending update says why it waits");
  check(text.includes("Neko updated itself to c7b6c58d (UI live)."), "the last update is shown");
  check(text.includes("Rolled back the update to 5dc59a5b"), "the last rollback is shown");

  await sleep(300);
  await shot("updates-settings");
  await a11y("Settings · Neko updates (dark)");
  await page.emulateMedia({ colorScheme: "light" });
  await sleep(400);
  await shot("updates-settings-light");
  await a11y("Settings · Neko updates (light)");
  await page.emulateMedia({ colorScheme: "dark" });

  for (const name of ["Update the web UI automatically", "Reinstall the app automatically"]) {
    const toggle = page.getByRole("switch", { name });
    const was = await toggle.getAttribute("aria-checked");
    check(was === "true", `${name} is on by default`);
    await toggle.click();
    await page.waitForFunction(([n, w]) => document.querySelector(`[role='switch'][aria-label='${n}']`)?.getAttribute("aria-checked") !== w, [name, was], { timeout: 8000 });
    await page.getByRole("switch", { name }).click();
    await page.waitForFunction(([n, w]) => document.querySelector(`[role='switch'][aria-label='${n}']`)?.getAttribute("aria-checked") === w, [name, was], { timeout: 8000 });
  }
  check(true, "both auto-update switches toggle and revert");

  // Search finds it.
  await page.keyboard.press("Meta+f");
  await page.getByLabel("Find a setting").fill("rollback");
  await page.waitForSelector("[data-slot='settings-results']");
  check((await page.locator("[data-slot='settings-results']").innerText()).includes("Neko updates"), "searching settings for rollback finds Neko updates");
  await page.getByLabel("Find a setting").fill("");

  await page.getByRole("button", { name: "Apply update" }).click();
  await page.waitForFunction(() => !document.querySelector("[data-slot='neko-updates']")?.textContent?.includes("Update waiting"), null, { timeout: 8000 });
  const after = await section.innerText();
  check(after.includes("Applying the update you approved"), "Apply hands the update to Neko and says so");
  await shot("updates-settings-applied");
  check(await page.evaluate(() => !window.__nekoReloadPending), "no reload is pending while nothing was rebuilt");
}
