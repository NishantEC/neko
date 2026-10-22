// Setup pages: Workspaces, Watching, Schedules, Tools & skills, Memory, Profiles,
// Settings (with Rulebook) and Usage & machine. Runs against the TEST daemon only.
//
// The data dir starts empty, so this seeds what the pages show (two workspaces, a
// fixture MCP server that really answers, a broken connection, a watch, a
// schedule, memories, a profile) through the same requests the pages send, then
// opens each page, does something harmless (toggle a switch and revert, open a
// dialog and cancel, search settings), and screenshots dark and light.
//
//   NEKO_WEB_ONLY=setup node scripts/web-verify.mjs   (see scripts/verify/README.md)
import { mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";

const FIXTURE = `// A tiny MCP server over stdio (newline-delimited JSON-RPC) for verification.
import readline from "node:readline";
const rl = readline.createInterface({ input: process.stdin });
const send = (m) => process.stdout.write(JSON.stringify(m) + "\\n");
const tools = [
  { name: "search_issues", description: "Search issues by text, assignee or state.", inputSchema: { type: "object", properties: { query: { type: "string", description: "Free text" }, state: { type: "string", enum: ["open", "closed"] } }, required: ["query"] }, annotations: { readOnlyHint: true } },
  { name: "list_projects", description: "List the projects you can see.", inputSchema: { type: "object", properties: {} }, annotations: { readOnlyHint: true } },
  { name: "create_issue", description: "Create an issue in a project.", inputSchema: { type: "object", properties: { title: { type: "string" }, body: { type: "string" } }, required: ["title"] } },
];
rl.on("line", (line) => {
  let m;
  try { m = JSON.parse(line); } catch { return; }
  if (m.method === "initialize") send({ jsonrpc: "2.0", id: m.id, result: { protocolVersion: m.params?.protocolVersion ?? "2025-03-26", capabilities: { tools: {} }, serverInfo: { name: "fixture", version: "1.0.0" } } });
  else if (m.method === "tools/list") send({ jsonrpc: "2.0", id: m.id, result: { tools } });
  else if (m.id !== undefined) send({ jsonrpc: "2.0", id: m.id, result: {} });
});
`;

// A reading the way the steward reports it. The test harness runs no steward, so
// the daemon answers `null`; the page is shown a realistic reading instead.
const MACHINE = {
  at_ms: 0, load: 11.9, cores: 14, free_disk_gb: 212, compressed_gb: 3.2, swap_used_gb: 1.4, swap_total_gb: 6,
  pressure: "warning", kernel_task_cpu: 12, night: false, holding: null, cap: 1, slowed: 2,
  agents: [
    { name: "Claude Code", processes: 3, mb: 2140 }, { name: "Codex", processes: 2, mb: 1310 },
    { name: "Chrome (shared)", processes: 9, mb: 940 }, { name: "OpenCode", processes: 1, mb: 310 },
  ],
};

export default async function ({ page, shot, a11y, check, sleep, PORT, DATA }) {
  const origin = `http://127.0.0.1:${PORT}`;
  await page.goto(`${origin}/#/`);
  await sleep(500);

  const api = (body) =>
    page.evaluate(async (b) => {
      const r = await fetch("/api/request", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(b) });
      return r.json();
    }, body);
  const wb = async (name, args) => {
    const reply = await api({ Workbench: args === undefined ? name : { [name]: args } });
    if (reply.Error) throw new Error(`${name}: ${reply.Error.message}`);
    return reply.Workbench;
  };
  const snapshot = async () => (await api({ Workbench: "Snapshot" })).Workbench;

  // ---- seed ---------------------------------------------------------------------
  const athena = path.join(DATA, "athena");
  const riavox = path.join(DATA, "riavox");
  mkdirSync(athena, { recursive: true });
  mkdirSync(riavox, { recursive: true });
  const fixture = path.join(DATA, "mcp-fixture.mjs");
  writeFileSync(fixture, FIXTURE);

  const save = (name, dir, instructions) =>
    wb("SaveWorkspaceWithFolders", { workspace: { id: "", name, repository: dir, instructions, away_enabled: false }, folders: [dir] });
  await save("athena", athena, "Prefer small pull requests. Run the unit tests before asking for review.");
  await save("riavox", riavox, "");
  let snap = await snapshot();
  const athenaId = snap.workspaces.find((w) => w.name === "athena").id;
  const riavoxId = snap.workspaces.find((w) => w.name === "riavox").id;

  await wb("Mcp", { AddConnection: { workspace_id: "", label: "Sentry", config: { transport: "http", url: "https://mcp.example.invalid/mcp" }, trust_local_process: false, credentials: null } });
  await wb("Mcp", { AddConnection: { workspace_id: "", label: "Linear (fixture)", config: { transport: "stdio", command: process.execPath, args: [fixture], cwd: null }, trust_local_process: true, credentials: null } });
  snap = await snapshot();
  for (const c of snap.mcp.connections) await wb("Mcp", { Discover: { connection_id: c.id } });
  await sleep(1500);
  snap = await snapshot();
  const linear = snap.mcp.connections.find((c) => c.label.startsWith("Linear"));
  const sentry = snap.mcp.connections.find((c) => c.label === "Sentry");
  check(linear.tools.length === 3, `the fixture MCP server answered (${linear.tools.length} tools)`);
  check(!!sentry.error, `the unreachable connection reports an error ("${sentry.error}")`);

  const watch = (instruction, ids, enabled) => ({
    id: "", workspace_id: athenaId, instruction, connection_ids: ids, enabled, prepare_low_risk: false,
    next_due_ms: Date.now(), last_attempt_ms: null, last_result: "", failures: 0,
  });
  await wb("Mcp", { SaveResponsibility: { responsibility: watch("New Linear issues assigned to me, and anything of mine that becomes blocked", [linear.id], true) } });
  await wb("Mcp", { SaveResponsibility: { responsibility: watch("New or spiking Sentry errors in my projects", [sentry.id], false) } });
  await wb("Schedules", { Save: { schedule: { id: "", name: "Morning brief", prompt: "Summarise what changed overnight in athena and what needs me today.", workspace_id: athenaId, rule: "FREQ=WEEKLY;BYDAY=MO,TU,WE,TH,FR;BYHOUR=9;BYMINUTE=0", timezone: "Asia/Kolkata", anchor_ms: Date.now(), enabled: false } } });
  await wb("Schedules", { Save: { schedule: { id: "", name: "Friday review", prompt: "List open pull requests older than three days.", workspace_id: riavoxId, rule: "FREQ=WEEKLY;BYDAY=FR;BYHOUR=16;BYMINUTE=30", timezone: "Asia/Kolkata", anchor_ms: Date.now(), enabled: false } } });
  const memory = (kind, workspace, text) => wb("SaveMemory", { entry: { id: "", agent_profile_id: "default", kind, workspace_id: workspace, text, source: "user", created_at_ms: Date.now(), updated_at_ms: Date.now() } });
  await memory("profile", null, "I prefer small pull requests, with the failing test written first.");
  await memory("workspace", athenaId, "athena: run `pnpm test --filter api` before pushing; CI is slow.");
  await memory("decision", athenaId, "Never auto-merge changes under billing/. Always ask.");
  await wb("AgentProfiles", { Save: { profile: { id: "", name: "Reviewer", instructions: "Review like a staff engineer: terse, specific, and kind." } } });
  snap = await snapshot();
  check(snap.mcp.responsibilities.length === 2 && snap.schedules.length === 2 && snap.memory.length === 3, "seeded watches, schedules and memories");

  // ---- helpers ------------------------------------------------------------------
  const both = async (name, selector, label, after) => {
    for (const scheme of ["dark", "light"]) {
      await page.emulateMedia({ colorScheme: scheme });
      await page.waitForSelector(selector, { timeout: 15000 });
      await sleep(400);
      if (after) await after(scheme);
      await shot(`setup-${name}-${scheme}`);
      await a11y(`${label} (${scheme})`);
    }
    await page.emulateMedia({ colorScheme: "dark" });
  };
  const open = async (route, selector) => {
    await page.goto(`${origin}/#/${route}`);
    try {
      await page.waitForSelector(selector, { timeout: 15000 });
    } catch (error) {
      await shot(`setup-failed-${route}`);
      throw error;
    }
    await sleep(300);
  };
  const until = async (test, what) => {
    for (let i = 0; i < 40; i++) {
      if (await test(await snapshot())) return;
      await sleep(200);
    }
    throw new Error(`timed out waiting for ${what}`);
  };
  const dialog = () => page.getByRole("dialog").last();
  const cancel = async () => {
    await dialog().getByRole("button", { name: "Cancel" }).click();
    await dialog().waitFor({ state: "detached", timeout: 5000 });
  };

  // ---- Workspaces ---------------------------------------------------------------
  await open("workspaces", "[data-slot='workspace']");
  const workspaceCount = await page.locator("[data-slot='workspace']").count();
  check(workspaceCount >= 2 && (await page.locator("[data-slot='workspace'][data-workspace='riavox']").count()) === 1, `Workspaces lists the seeded workspaces (${workspaceCount})`);
  const athenaPanel = page.locator("[data-slot='workspace'][data-workspace='athena']");
  check((await athenaPanel.innerText()).includes("1 active"), "athena shows its watch summary (1 active, 1 paused)");
  await both("workspaces", "[data-slot='workspace']", "Workspaces");
  await page.getByRole("button", { name: "Add workspace" }).first().click();
  await dialog().waitFor();
  await dialog().getByLabel("Name", { exact: true }).fill("scratch");
  await dialog().getByLabel("Source folders").fill("/nope/does-not-exist");
  await dialog().getByRole("button", { name: "Add folder" }).click();
  await dialog().getByRole("button", { name: "Save workspace" }).click();
  await page.waitForSelector("[data-slot='error-note']", { timeout: 10000 });
  const wsError = await dialog().locator("[data-slot='error-note']").innerText();
  check(/does not exist/i.test(wsError), `a failed save shows the daemon's own words (“${wsError.replace(/\s+/g, " ").slice(0, 70)}”)`);
  await shot("setup-workspaces-add-error");
  await a11y("Workspaces add dialog");
  await cancel();
  check((await page.locator("[data-slot='workspace']").count()) === workspaceCount, "cancelling the dialog added nothing");

  // ---- Watching -----------------------------------------------------------------
  await open("watching", "[data-slot='watch']");
  check((await page.locator("[data-slot='watch']").count()) >= 1, "Watching lists the active watch");
  await both("watching", "[data-slot='new-watch']", "Watching");
  // Pause the watch and turn it back on (which asks first), then leave it as found.
  const watchSwitch = page.getByRole("switch", { name: /^Watch New Linear issues/ });
  check((await watchSwitch.getAttribute("aria-checked")) === "true", "the Linear watch starts on");
  await watchSwitch.click();
  await until((s) => s.mcp.responsibilities.find((r) => r.instruction.startsWith("New Linear"))?.enabled === false, "the watch to pause");
  check(true, "turning a watch off pauses it");
  await page.getByRole("button", { name: "Show", exact: true }).click();
  await sleep(300);
  await shot("setup-watching-paused");
  await page.getByRole("switch", { name: /^Watch New Linear issues/ }).click();
  await page.getByRole("alertdialog").waitFor({ timeout: 5000 });
  check((await page.getByRole("alertdialog").innerText()).includes("unattended checks"), "turning it on asks first, and says what it allows");
  await sleep(500);
  await shot("setup-watching-confirm");
  await page.getByRole("alertdialog").getByRole("button", { name: "Turn on watch" }).click();
  await until((s) => s.mcp.responsibilities.find((r) => r.instruction.startsWith("New Linear"))?.enabled === true, "the watch to turn back on");
  check(true, "confirming turns the watch back on");
  await page.getByRole("button", { name: /^Show details for New Linear/ }).first().click();
  await sleep(300);
  await shot("setup-watching-expanded");
  await a11y("Watching (expanded)");
  await page.getByRole("button", { name: "Edit watch" }).first().click();
  await dialog().waitFor();
  check(/connected tools/i.test(await dialog().innerText()) && (await dialog().getByRole("checkbox", { name: "Linear (fixture)" }).count()) === 1, "the watch editor lists connected tools");
  await a11y("Watching editor");
  await cancel();

  // ---- Schedules ----------------------------------------------------------------
  await open("schedules", "[data-slot='schedule']");
  check((await page.locator("[data-slot='schedule']").count()) === 2, "Schedules lists both schedules");
  await both("schedules", "[data-slot='schedule']", "Schedules");
  const brief = page.getByRole("switch", { name: /Morning brief/ });
  await brief.click();
  await page.waitForFunction(() => [...document.querySelectorAll("[data-slot='schedule']")].some((s) => s.innerText.includes("Morning brief") && s.dataset.enabled === "true"), null, { timeout: 8000 });
  check(true, "enabling a schedule works");
  await page.getByRole("switch", { name: /Morning brief/ }).click();
  await page.waitForFunction(() => [...document.querySelectorAll("[data-slot='schedule']")].every((s) => s.dataset.enabled === "false"), null, { timeout: 8000 });
  check(true, "…and pausing it again leaves it as found");
  await page.getByRole("button", { name: "Edit schedule" }).first().click();
  await dialog().waitFor();
  await shot("setup-schedules-editor");
  await a11y("Schedules editor");
  await cancel();

  // ---- Tools & skills -----------------------------------------------------------
  await open("tools", "[data-slot='mcp-connection']");
  const order = await page.locator("[data-slot='mcp-connection']").evaluateAll((els) => els.map((e) => e.dataset.status));
  check(order[0] === "Needs attention" && order.includes("Available"), `a broken connection is listed first (${order.join(", ")})`);
  check((await page.locator("[data-slot='attention']").count()) === 1, "an attention banner explains what is broken");
  await both("tools", "[data-slot='mcp-connection']", "Tools & skills");
  await page.getByRole("button", { name: "Add connection" }).first().click();
  await page.getByRole("menuitem", { name: /Add connection manually/ }).click();
  await dialog().waitFor();
  await dialog().getByLabel("Connection name").fill("Example");
  await dialog().getByLabel("Server URL").fill("not a url");
  await dialog().getByRole("button", { name: "Add connection" }).click();
  await page.waitForSelector("[data-slot='error-note']");
  check(/complete HTTP or HTTPS/.test(await dialog().innerText()), "a bad URL is refused before anything is sent");
  await shot("setup-tools-add-dialog");
  await a11y("Tools add dialog");
  await cancel();
  check((await page.locator("[data-slot='mcp-connection']").count()) === 2, "cancelling added no connection (still 2)");
  await page.getByRole("button", { name: /^Sentry, All workspaces, Needs attention/ }).first().click();
  await page.waitForSelector("[data-slot='connection-detail']");
  check((await page.locator("[data-slot='connection-detail']").innerText()).includes("Reconnect"), "a broken connection offers Reconnect");
  await shot("setup-tools-detail-broken");
  await a11y("Tools connection detail (broken)");
  await page.keyboard.press("Escape");
  await page.waitForSelector("[data-slot='connection-detail']", { state: "detached" });
  await page.getByRole("button", { name: /^Linear \(fixture\), All workspaces, Available/ }).click();
  await page.waitForSelector("[data-slot='connection-detail']");
  check((await page.locator("[data-slot='connection-detail']").innerText()).includes("search_issues"), "an available connection lists its tools");
  await page.getByRole("button", { name: /^search_issues/ }).click();
  await sleep(300);
  await shot("setup-tools-detail-tools");
  await page.keyboard.press("Escape");
  await page.waitForSelector("[data-slot='connection-detail']", { state: "detached" });
  await page.getByRole("button", { name: "Add connection" }).first().click();
  await page.getByRole("menuitem", { name: /Browse MCP servers/ }).click();
  await dialog().waitFor();
  check((await dialog().innerText()).includes("sentry-mcp"), "the registry browser lists a featured server");
  await shot("setup-tools-registry");
  await a11y("Tools registry");
  await page.keyboard.press("Escape");
  await page.getByRole("tab", { name: "Skills" }).click();
  await sleep(500);
  await shot("setup-tools-skills");
  await a11y("Tools skills tab");

  // ---- Memory -------------------------------------------------------------------
  await open("memory", "[data-slot='memory']");
  check((await page.locator("[data-slot='memory']").count()) === 3, "Memory lists three entries by scope");
  await both("memory", "[data-slot='memory']", "Memory");
  const useMemory = page.getByRole("switch", { name: "Use saved memory" });
  const was = await useMemory.getAttribute("aria-checked");
  await useMemory.click();
  await page.waitForFunction((w) => document.querySelector("[role='switch'][aria-label='Use saved memory']")?.getAttribute("aria-checked") !== w, was, { timeout: 8000 });
  await page.getByRole("switch", { name: "Use saved memory" }).click();
  await page.waitForFunction((w) => document.querySelector("[role='switch'][aria-label='Use saved memory']")?.getAttribute("aria-checked") === w, was, { timeout: 8000 });
  check(true, "a memory option toggles and reverts");
  await page.getByRole("button", { name: "Add memory" }).first().click();
  await dialog().waitFor();
  await a11y("Memory add dialog");
  await cancel();
  await page.getByRole("button", { name: "Memory actions" }).first().click();
  await page.getByRole("menuitem", { name: /Forget memory/ }).click();
  await page.getByRole("alertdialog").waitFor();
  check(true, "forgetting a memory asks first");
  await shot("setup-memory-forget");
  await page.getByRole("alertdialog").getByRole("button", { name: "Cancel" }).click();
  await page.getByRole("tab", { name: "Working style" }).click();
  await sleep(400);
  await shot("setup-memory-style");
  await a11y("Memory working style");

  // ---- Profiles -----------------------------------------------------------------
  await open("profiles", "[data-slot='profile']");
  check((await page.locator("[data-slot='profile']").count()) === 2, "Profiles lists Neko and Reviewer");
  await both("profiles", "[data-slot='profile']", "Profiles");
  await page.getByRole("button", { name: "Edit" }).nth(1).click();
  await dialog().waitFor();
  check((await dialog().getByLabel("Name").inputValue()) === "Reviewer", "the profile editor opens on the chosen profile");
  await cancel();

  // ---- Settings -----------------------------------------------------------------
  await open("settings", "[data-slot='settings-nav']");
  await both("settings-general", "[data-slot='settings-section'][data-section='general']", "Settings · General");
  await page.keyboard.press("Meta+f");
  check(await page.evaluate(() => document.activeElement?.getAttribute("aria-label") === "Find a setting"), "⌘F focuses the settings search");
  await page.getByLabel("Find a setting").fill("budget");
  await page.waitForSelector("[data-slot='settings-results']");
  const results = await page.locator("[data-slot='settings-results']").innerText();
  check(/Task budget/.test(results) && /AI & agents/.test(results), "searching finds a setting and its section");
  await shot("setup-settings-search");
  await a11y("Settings search");
  await page.getByRole("button", { name: /^Task budget in AI & agents/ }).click();
  await page.waitForSelector("#setting-budget");
  check(await page.evaluate(() => document.getElementById("setting-budget")?.hasAttribute("data-highlighted")), "choosing a result jumps to the setting and highlights it");
  await sleep(900);
  await shot("setup-settings-jump");
  await page.getByLabel("Find a setting").fill("zzzzqq");
  check((await page.locator("[data-slot='settings-results']").innerText()).includes("No setting matches"), "a query with no match says so");
  await page.getByLabel("Find a setting").fill("");
  await sleep(300);
  await page.waitForSelector("[data-slot='runtime']");
  await both("settings-ai", "[data-slot='settings-section'][data-section='ai']", "Settings · AI & agents", async () => { await page.waitForSelector("[data-slot='pins']"); });
  const nav = (name) => page.locator("[data-slot='settings-nav']").getByRole("button", { name, exact: true }).click();
  // Rulebook: edit, save, then restore the default.
  await nav("Rulebook");
  await page.waitForSelector("[data-slot='rulebook']");
  const original = await page.getByLabel("Rulebook", { exact: true }).inputValue();
  check(original.length > 100, "the rulebook shows the built-in default");
  await both("settings-rulebook", "[data-slot='rulebook']", "Settings · Rulebook");
  await page.getByLabel("Rulebook", { exact: true }).fill(`${original}\n\n## Verification note\nShip small.`);
  await page.getByRole("button", { name: "Save rulebook" }).click();
  await page.getByText("Using your own rules").waitFor({ timeout: 8000 });
  check(true, "saving the rulebook switches to your own rules");
  await page.getByRole("button", { name: "Restore default" }).click();
  await page.getByRole("alertdialog").getByRole("button", { name: "Restore default" }).click();
  await page.getByText("Using Neko’s default rules").waitFor({ timeout: 8000 });
  check(true, "restoring the default puts the built-in rulebook back");
  await nav("Quick panel");
  await sleep(500);
  await shot("setup-settings-quick-panel");
  await nav("Permissions");
  await page.getByRole("switch", { name: "Clipboard history" }).waitFor();
  const clip = page.getByRole("switch", { name: "Clipboard history" });
  const clipWas = await clip.getAttribute("aria-checked");
  await clip.click();
  await page.waitForFunction((w) => document.querySelector("[role='switch'][aria-label='Clipboard history']")?.getAttribute("aria-checked") !== w, clipWas, { timeout: 8000 });
  await page.getByRole("switch", { name: "Clipboard history" }).click();
  await page.waitForFunction((w) => document.querySelector("[role='switch'][aria-label='Clipboard history']")?.getAttribute("aria-checked") === w, clipWas, { timeout: 8000 });
  check(true, "clipboard history toggles and reverts");
  await both("settings-permissions", "[data-slot='settings-section'][data-section='permissions']", "Settings · Permissions");
  await nav("Diagnostics");
  await page.getByRole("button", { name: /Run diagnostics/ }).click();
  await page.waitForSelector("[data-slot='diagnostic-checks'] > *", { timeout: 60000 });
  check(true, "diagnostics lists its checks");
  await both("settings-diagnostics", "[data-slot='settings-section'][data-section='diagnostics']", "Settings · Diagnostics");
  await nav("About");
  await sleep(300);
  await shot("setup-settings-about");
  await a11y("Settings · About");

  // ---- Usage & machine ----------------------------------------------------------
  await open("usage", "[data-slot='setup-page']");
  await page.waitForSelector("text=No reading yet", { timeout: 15000 });
  check(true, "with no machine reading the page says so instead of showing zeros");
  await shot("setup-usage-no-reading");
  await page.route("**/api/request", async (route) => {
    if (route.request().postData() === '"MachineHealth"') {
      await route.fulfill({ contentType: "application/json", body: JSON.stringify({ MachineHealth: { ...MACHINE, at_ms: Date.now() } }) });
    } else await route.continue();
  });
  await page.reload();
  await page.waitForSelector("[data-slot='machine-wells']", { timeout: 15000 });
  const wells = await page.locator("[data-slot='machine-wells'] [data-slot='well']").count();
  check(wells === 4, "the machine strip shows load, memory, disk and cooling wells");
  check((await page.locator("[data-slot='machine-state']").innerText()).includes("Memory tight"), "a capped Mac is called out in words");
  await both("usage", "[data-slot='machine-wells']", "Usage & machine");
  await page.locator("[data-slot='setup-page']").evaluate((el) => el.scrollTo({ top: el.scrollHeight }));
  await sleep(600);
  await shot("setup-usage-spend");
  check((await page.locator("[data-slot='spend-wells']").count()) === 1, "the spend wells render from what agents reported");
  await page.unroute("**/api/request");

  // ---- ⌘K -----------------------------------------------------------------------
  await open("usage", "[data-slot='setup-page']");
  const palette = async (text) => {
    await page.keyboard.press("Meta+k");
    await page.locator("[data-slot='command']").waitFor({ timeout: 5000 });
    await page.keyboard.press("Meta+a");
    await page.keyboard.type(text);
    await sleep(300);
    return page.locator("[data-slot='command-item']").allInnerTexts();
  };
  check((await palette("rulebook")).some((e) => /Open Rulebook/.test(e)), "⌘K offers “Open Rulebook” on a setup page");
  await page.locator("[data-slot='command-item']", { hasText: "Open Rulebook" }).click();
  await page.waitForSelector("[data-slot='rulebook']", { timeout: 8000 });
  check(true, "“Open Rulebook” lands on the Rulebook section");
  check((await palette("check all")).some((e) => /Check all watches now/.test(e)), "⌘K offers “Check all watches now”");
  await page.keyboard.press("Escape");
  check((await palette("ai &")).some((e) => /Settings: AI & agents/.test(e)), "⌘K lists the Settings sections");
  await page.keyboard.press("Escape");
}
