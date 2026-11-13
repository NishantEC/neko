// First run (docs/knowledge/first-run.md): the six-step setup a person sees on a
// new Mac. Starts from "nothing chosen" (routines, Calendar and phone reminders
// off), walks every step against the real harness daemon, and checks that each
// choice reaches the daemon and that skipping finishes it for good.
//
// The agent catalog is stood in for (Claude Code ready, Codex signed out,
// OpenCode missing) so the result does not depend on this Mac's own installs.
// A scope with two MCP servers is made under the harness's own HOME.
//
//   NEKO_WEB_ONLY=firstrun node scripts/web-verify.mjs
import { mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";

const CATALOG = {
  read_at_ms: 1,
  sources: [
    { provider: "claude", label: "Claude Code", connection: "Claude Code · Max", status: "ready", models: [] },
    { provider: "codex", label: "Codex", connection: "Codex · sign-in required", status: "sign_in_required", note: "Sign in to Codex, then refresh.", models: [] },
    { provider: "opencode", label: "OpenCode", connection: "OpenCode · not installed", status: "not_installed", models: [] },
  ],
};

export default async function ({ page, shot, a11y, check, sleep, PORT, DATA }) {
  const base = `http://127.0.0.1:${PORT}`;
  const call = (body) => page.evaluate((b) => fetch("/api/request", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(b) }).then((r) => r.json()), body);

  // A scope on disk: a top folder under ~/Documents with two servers.
  const scope = path.join(DATA, "Documents", "acme");
  mkdirSync(scope, { recursive: true });
  writeFileSync(path.join(scope, ".mcp.json"), JSON.stringify({ mcpServers: { "slack-work": { command: "true" }, "ops-mcp": { command: "true" } } }));

  await page.route("**/api/request", async (route) => {
    const body = route.request().postData() ?? "";
    if (body.includes('"AgentModels"')) return route.fulfill({ json: { AgentModels: CATALOG } });
    return route.continue();
  });

  // Start from nothing chosen.
  await page.goto(`${base}/#/`);
  await call({ SetRoutinesOn: { enabled: false } });
  await call({ SetCalendarEnabled: { enabled: false } });
  const phone = (await call("PhoneSettings")).Phone.settings;
  await call({ SetPhoneSettings: { settings: { ...phone, reminders: false, ntfy: false } } });
  await call({ SetOnboardingComplete: { completed: false } });
  await page.reload();

  const dialog = page.locator("[data-slot='onboarding']");
  await dialog.waitFor({ timeout: 10000 });
  const welcome = await dialog.innerText();
  check(/^Hi \S+\.\s*A little more\s+off your mind/m.test(welcome), "the welcome greets the person by the Mac account's first name");
  check(/Nothing runs on its own until you say so/.test(welcome), "the welcome says nothing runs until you say so");
  check((await dialog.getByRole("button", { name: "Skip setup" }).count()) === 1, "the welcome can be skipped");
  for (const theme of ["dark", "light"]) {
    await page.emulateMedia({ colorScheme: theme });
    await shot(`firstrun-welcome-${theme}`);
    await a11y(`first run welcome (${theme})`);
  }
  await page.emulateMedia({ colorScheme: "dark" });
  const owner = (await call("Routines")).Routines;
  check(owner.routines_on === false, "routines are off on a fresh data folder");
  check(owner.scopes.some((s) => s.name === "acme" && !s.workspace_id), "the scope on disk is found but no workspace is made for it");

  // 2. Agents
  await dialog.getByRole("button", { name: /Get started/ }).click();
  const list = dialog.locator("[data-slot='agent-list']");
  await list.getByText("Claude Code", { exact: true }).waitFor({ timeout: 6000 });
  const agents = await list.innerText();
  check(/Claude Code[\s\S]*Ready/.test(agents), "a signed-in agent shows as ready");
  check(/Codex[\s\S]*not signed in/.test(agents) && /codex login/.test(agents), "a signed-out agent shows the one command to sign in");
  check(/OpenCode[\s\S]*Not installed/.test(agents) && /npm install -g opencode-ai/.test(agents), "a missing agent says so and shows how to install it");
  check((await dialog.locator("[data-slot='agents-none']").count()) === 0, "no 'needs one agent' warning while one is ready");
  await shot("firstrun-agents-dark");
  await a11y("first run agents");

  // 3. Workspaces
  await dialog.getByRole("button", { name: /Continue/ }).click();
  const workspaces = dialog.locator("[data-slot='workspace-list']");
  await workspaces.getByText("acme", { exact: true }).waitFor({ timeout: 6000 });
  const text = await workspaces.innerText();
  check(/2 tool servers: .*slack-work/.test(text) && /ops-mcp/.test(text), "a scope lists its MCP servers");
  check(/Personal/.test(text), "the home folder is offered as Personal");
  const acme = workspaces.locator("[data-slot='setup-row']", { hasText: "2 tool servers" });
  await acme.getByRole("button", { name: "Add" }).click();
  await acme.getByText("Added").waitFor({ timeout: 6000 });
  check(true, "adding a scope makes its workspace");
  await dialog.getByLabel("Folder path").fill("not a path");
  await dialog.getByRole("button", { name: "Add workspace" }).click();
  await dialog.locator("[data-slot='onboarding-error']").waitFor();
  check(/starting with \//.test(await dialog.locator("[data-slot='onboarding-error']").innerText()), "a folder that is not a path is explained");
  const extra = path.join(DATA, "Documents", "side-project");
  mkdirSync(extra, { recursive: true });
  await dialog.getByLabel("Folder path").fill(extra);
  await dialog.getByRole("button", { name: "Add workspace" }).click();
  await workspaces.getByText("side-project", { exact: true }).waitFor({ timeout: 6000 });
  check(true, "any other folder can be added");
  await shot("firstrun-workspaces-dark");
  await a11y("first run workspaces");

  // 4. Routines (off until switched on)
  await dialog.getByRole("button", { name: /Continue/ }).click();
  const routines = dialog.getByRole("switch", { name: "Run routines" });
  await routines.waitFor();
  await page.waitForFunction(() => document.querySelector("[role='switch'][aria-label='Run routines']")?.getAttribute("aria-disabled") !== "true");
  check((await routines.getAttribute("aria-checked")) === "false", "the routines switch starts off");
  await shot("firstrun-routines-off-dark");
  await routines.click();
  await page.waitForFunction(() => document.querySelector("[role='switch'][aria-label='Run routines']")?.getAttribute("aria-checked") === "true");
  check((await call("Routines")).Routines.routines_on === true, "the switch turns routines on in the daemon");
  check((await call("Routines")).Routines.scopes.every((s) => s.workspace_id), "turning routines on gives every scope its workspace");
  await a11y("first run routines");

  // 5. This Mac
  await dialog.getByRole("button", { name: /Continue/ }).click();
  const calendar = dialog.getByRole("switch", { name: "Calendar", exact: true });
  await calendar.waitFor();
  await page.waitForFunction(() => { const el = document.querySelector("[role='switch'][aria-label='Calendar']"); return el && el.getAttribute("aria-disabled") !== "true"; });
  check((await calendar.getAttribute("aria-checked")) === "false", "Calendar starts off");
  await calendar.click();
  await page.waitForFunction(() => document.querySelector("[role='switch'][aria-label='Calendar']")?.getAttribute("aria-checked") === "true");
  check((await call("CalendarStatus")).CalendarStatus.enabled === true, "the Calendar switch is saved in the daemon");
  const reminders = dialog.getByRole("switch", { name: "Phone reminders" });
  check((await reminders.getAttribute("aria-checked")) === "false", "phone reminders start off");
  await reminders.click();
  await page.waitForFunction(() => document.querySelector("[role='switch'][aria-label='Phone reminders']")?.getAttribute("aria-checked") === "true");
  check((await call("PhoneSettings")).Phone.settings.reminders === true, "the phone switch is saved in the daemon");
  check((await dialog.getByRole("switch", { name: "Clipboard history" }).count()) === 1, "clipboard history is one more optional switch");
  check((await dialog.locator("a[href^='x-apple.systempreferences:']").count()) === 1, "Accessibility links to its System Settings pane");
  await shot("firstrun-mac-dark");
  await a11y("first run this mac");

  // 6. Done
  await dialog.getByRole("button", { name: /Continue/ }).click();
  const summary = dialog.locator("[data-slot='setup-summary']");
  await summary.waitFor();
  await page.waitForFunction(() => { const s = document.querySelector("[data-slot='setup-summary']")?.textContent ?? ""; return /Routines\s*On/.test(s) && !s.includes("…"); });
  const done = (await summary.innerText()).replace(/\s+/g, " ");
  check(/Agents ready 1/.test(done) && /Routines On/.test(done) && /Calendar On/.test(done) && /Phone reminders On/.test(done), "the last step summarises what was chosen");
  await shot("firstrun-done-dark");
  await dialog.getByRole("button", { name: "Open Neko" }).click();
  await dialog.waitFor({ state: "detached", timeout: 6000 });
  check((await call("GetOnboardingState")).OnboardingState.completed === true, "finishing marks first run complete");
  await page.reload();
  await sleep(1500);
  check((await dialog.count()) === 0, "once done, the setup never shows again");

  // Skip: from the welcome, and nothing is switched on.
  await call({ SetRoutinesOn: { enabled: false } });
  await call({ SetCalendarEnabled: { enabled: false } });
  await call({ SetOnboardingComplete: { completed: false } });
  await page.reload();
  await dialog.waitFor({ timeout: 10000 });
  await dialog.getByRole("button", { name: "Skip setup" }).click();
  await dialog.waitFor({ state: "detached", timeout: 6000 });
  check((await call("GetOnboardingState")).OnboardingState.completed === true, "Skip setup finishes first run");
  check((await call("Routines")).Routines.routines_on === false && (await call("CalendarStatus")).CalendarStatus.enabled === false, "skipping switches nothing on");

  // Re-openable: Settings → General, and the native app's #/setup link.
  await page.goto(`${base}/#/settings?section=general`);
  await page.getByRole("button", { name: "Show again" }).click();
  await dialog.waitFor({ timeout: 4000 });
  check(/A little more\s+off your mind/.test(await dialog.innerText()), "Settings → General shows the setup again");
  await page.keyboard.press("Escape");
  await dialog.waitFor({ state: "detached", timeout: 4000 });
  await page.evaluate(() => { location.hash = "#/setup"; });
  await dialog.waitFor({ timeout: 4000 });
  check(true, "the #/setup link (used by the native app) opens it");
  await page.keyboard.press("Escape");
  await dialog.waitFor({ state: "detached", timeout: 4000 });

  // The two places that were quiet because the switch was off say so.
  await page.goto(`${base}/#/inbox`);
  await page.locator("[data-slot='routines-off']").waitFor({ timeout: 6000 });
  check(true, "the Inbox says routines are off, with a Turn on button");
  await page.goto(`${base}/#/calendar`);
  await page.locator("[data-slot='calendar-off']").waitFor({ timeout: 6000 });
  check(true, "the Calendar says it is off, with a Turn on button");

  await page.unroute("**/api/request");
  // Leave the harness the way the other areas expect it.
  await call({ SetRoutinesOn: { enabled: true } });
  await call({ SetOnboardingComplete: { completed: true } });
}
