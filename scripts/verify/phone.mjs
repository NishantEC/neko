// Settings → Phone (docs/knowledge/phone-push.md): two channel switches, the ntfy
// rows only while ntfy is on, a topic that is long and random, quiet hours, a
// test button and one status line each. The server is pointed at a closed local
// port before the test is sent, so this check never reaches a real ntfy server.
//
//   NEKO_WEB_ONLY=phone node scripts/web-verify.mjs
export default async function ({ page, shot, a11y, check }) {
  await page.evaluate(() => { location.hash = "#/settings?section=phone"; });
  await page.waitForSelector("[data-slot='phone-settings']");
  const section = page.locator("[data-slot='phone-settings']");

  check((await section.locator("[data-slot='phone-ntfy']").count()) === 0, "the ntfy rows stay hidden while ntfy is off");
  check(/not asked yet/i.test(await page.locator("[data-slot='phone-access']").innerText()), "the Reminders line says it has not asked yet");
  check(/No push sent yet/.test(await page.locator("[data-slot='phone-last-push']").innerText()), "nothing has been sent yet");
  check(await page.getByRole("button", { name: "Send a test" }).isDisabled(), "Send a test waits for a channel");
  await shot("phone-off");

  await page.getByRole("switch", { name: "ntfy", exact: true }).click();
  await page.waitForSelector("[data-slot='phone-ntfy']");
  const topic = (await page.locator("[data-slot='phone-topic']").innerText()).trim();
  check(topic.length >= 24 && /^[a-z0-9_-]+$/i.test(topic), `the topic is long and unguessable (${topic.length} characters)`);
  const link = (await page.locator("[data-slot='phone-link']").innerText()).trim();
  check(link === `ntfy://ntfy.sh/${topic}`, "the subscribe link names the server and the topic");
  check((await page.getByRole("switch", { name: "Send titles only" }).getAttribute("aria-checked")) === "true", "titles only is on by default");

  await page.getByRole("button", { name: "New topic" }).click();
  await page.waitForFunction((old) => document.querySelector("[data-slot='phone-topic']")?.textContent?.trim() !== old, topic);
  check(true, "New topic makes another topic");

  const server = page.getByLabel("ntfy server");
  await server.fill("http://127.0.0.1:9");
  await server.press("Enter");
  await page.waitForFunction(() => /ntfy:\/\/127\.0\.0\.1:9\//.test(document.querySelector("[data-slot='phone-link']")?.textContent ?? ""));
  check(true, "a server of your own changes the subscribe link");

  await page.getByRole("button", { name: "Send a test" }).click();
  await page.waitForSelector("[data-slot='phone-test-line']");
  check(/ntfy: /.test(await page.locator("[data-slot='phone-test-line']").first().innerText()), "the test says what happened on ntfy");
  await page.waitForSelector("[data-slot='phone-error']");
  check(/Last error/.test(await page.locator("[data-slot='phone-error']").innerText()), "a failed push shows its error on one line");
  await shot("phone-ntfy-error");

  await page.getByRole("switch", { name: "Quiet hours", exact: true }).click();
  await page.waitForFunction(() => document.querySelector("button[aria-label='Quiet hours start']")?.hasAttribute("disabled"));
  check(true, "turning quiet hours off disables the hours");
  await a11y("settings · phone");
}
