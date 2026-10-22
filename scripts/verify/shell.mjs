// The shell, the composer, the plugins page and first run, checked in a real
// browser against the test daemon. Run alone with NEKO_WEB_ONLY=shell (see
// scripts/verify/README.md). Screenshots land in $NEKO_WEB_SHOTS.
//
// What is faked, and why: the test daemon never starts a model, so the model
// catalog (`AgentModels`) is answered by a fixture, and a few requests are
// answered by the page itself when the point is to see how the UI shows what the
// daemon would say (an error, a snapshot with tasks in every stage and a Mac
// under load). Everything else reaches the real daemon.
import { cpSync, existsSync, mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import path from "node:path";

const CATALOG = {
  sources: [
    {
      provider: "codex", label: "Codex", connection: "ChatGPT", status: "ready", default_model: "gpt-6-astra",
      models: [
        {
          id: "gpt-6-astra", label: "GPT-6 Astra", description: "Best for hard problems", recommended: true, access: "listed",
          effort_options: [{ id: "low", label: "Low" }, { id: "medium", label: "Medium" }, { id: "high", label: "High", description: "Thinks longer before it answers." }],
          default_effort: "medium",
          speed_options: [{ id: "default", label: "Normal" }, { id: "fast", label: "Fast", description: "About 1.5x faster; uses more allowance." }],
          default_speed: "default",
        },
        { id: "gpt-6-luna", label: "GPT-6 Luna", description: "Quick and light", access: "listed", effort_options: [{ id: "low", label: "Low" }, { id: "medium", label: "Medium" }], speed_options: [] },
        { id: "gpt-5-sol", label: "GPT-5 Sol", description: "Older", access: "unavailable", reason: "Not on this plan.", effort_options: [], speed_options: [] },
      ],
    },
    { provider: "claude", label: "Claude Code", connection: "", status: "sign_in_required", note: "Sign in with `claude login` to use Claude models.", models: [] },
    { provider: "ollama", label: "Ollama", connection: "", status: "not_running", note: "Ollama is not running on this Mac.", models: [] },
  ],
};

// A 1x1 PNG.
const PNG = Buffer.from("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==", "base64");

export default async function ({ page, shot, a11y, check, sleep, PORT, DATA, OUT }) {
  const base = `http://127.0.0.1:${PORT}`;
  const repo = path.resolve(path.dirname(new URL(import.meta.url).pathname), "../..");
  const themes = ["dark", "light"];
  const setTheme = async (theme) => {
    await page.emulateMedia({ colorScheme: theme });
    await page.waitForFunction((t) => document.documentElement.classList.contains(t), theme, { timeout: 3000 });
    await sleep(250);
  };

  // ---- one route for everything this area fakes ----------------------------------------------
  const mock = { runtimeError: null, captured: [], intercept: [], patchSnapshot: null, models: true };
  await page.route("**/api/request", async (route) => {
    const body = route.request().postData() ?? "";
    if (mock.models && body.includes('"AgentModels"')) return route.fulfill({ json: { AgentModels: CATALOG } });
    if (body.includes('"SetConversationRuntime"')) {
      if (mock.runtimeError) {
        const message = mock.runtimeError;
        mock.runtimeError = null;
        return route.fulfill({ json: { Error: { message } } });
      }
      // The real daemon runs one short model check before it saves a pin; the test daemon has no model,
      // so the save is answered here the way the daemon answers a passing check.
      const { conversation_id: id, preferences } = JSON.parse(body).Workbench.SetConversationRuntime;
      const real = await (await route.fetch({ postData: JSON.stringify({ Workbench: "Snapshot" }) })).json();
      const snap = real.Workbench;
      mock.revision = (mock.revision ?? 0) + 1; // the stand-in daemon forgets nothing: revisions only go up
      snap.conversation_runtime_revisions = { ...snap.conversation_runtime_revisions, [id]: mock.revision };
      snap.conversation_runtime = { ...snap.conversation_runtime };
      if (Object.values({ ...preferences, allow_paid_speed: null }).every((v) => v === null)) delete snap.conversation_runtime[id];
      else snap.conversation_runtime[id] = preferences;
      mock.patchSnapshot?.(snap);
      return route.fulfill({ json: { Workbench: snap } });
    }
    for (const name of mock.intercept) {
      if (body.includes(`"${name}"`)) {
        mock.captured.push(JSON.parse(body));
        return route.fulfill({ json: { Error: { message: `verify: ${name} was intercepted, not sent` } } });
      }
    }
    const patch = mock.patchSnapshot;
    if (patch && body.includes('Workbench')) {
      const response = await route.fetch();
      const json = await response.json();
      if (json.Workbench) patch(json.Workbench);
      return route.fulfill({ response, json });
    }
    return route.fallback();
  });

  await setTheme("dark");
  const composerBox = () => page.locator("[data-slot='composer']").boundingBox();
  const textarea = page.locator("[data-slot='composer'] textarea");
  const goHome = async () => {
    await page.goto(`${base}/#/`);
    await page.waitForSelector("[data-slot='composer'] textarea", { timeout: 15000 });
    await sleep(500);
  };
  const clearDraft = async () => {
    await textarea.fill("");
    await sleep(100);
  };

  // ===========================================================================================
  // Shell state: sidebar chips, tasks, title, status bar
  // ===========================================================================================
  const now = Date.now();
  const task = (id, title, status, ago) => ({
    id, workspace_id: "w", issue_id: null, title, display_title: title, goal: title, status, plan: "", result: "", worktree: null,
    events: [], created_at_ms: now - ago - 60000, updated_at_ms: now - ago,
  });
  mock.withNew = false;
  mock.patchSnapshot = (snap) => {
    snap.tasks = [
      ...(mock.withNew ? [task("t-new", "Tighten the signup copy", "ReadyForReview", 100)] : []),
      task("t-needs", "Fix the flaky login test", "AwaitingApproval", 4000),
      task("t-failed", "Migrate billing webhooks", "Failed", 9000),
      task("t-review", "Add retry to the sync job", "ReadyForReview", 20000),
      task("t-run", "Refactor the settings store", "Building", 1000),
      task("t-next", "Update the CLI help text", "Queued", 30000),
      ...(snap.tasks ?? []),
    ];
    snap.machine = {
      at_ms: Date.now(), load: 17.4, cores: 14, free_disk_gb: 96, compressed_gb: 2.3, swap_used_gb: 1.2, swap_total_gb: 4,
      pressure: "warning", kernel_task_cpu: 12, night: false, holding: "Memory pressure is high: no new agents start.", cap: 1, slowed: 2, agents: [],
    };
  };
  await page.goto(`${base}/#/`);
  await page.reload();
  await page.waitForSelector("[data-slot='composer'] textarea", { timeout: 15000 });
  await page.waitForSelector("[data-slot='task-row']", { timeout: 10000 });
  await sleep(600);

  const chips = await page.locator("[data-slot='nav-item'] [data-slot='state-chip']").allInnerTexts();
  check(chips.some((c) => /^Working \d+s/.test(c.trim())), `Home's sidebar row shows a live "Working Ns" chip (${chips.join(" | ")})`);
  const tasksChip = await page.locator("[data-slot='nav-item']", { hasText: "Home" }).first().innerText();
  check(/Working/.test(tasksChip), "the chip sits in the row's right-hand slot");
  const rows = await page.locator("[data-slot='task-row']").evaluateAll((els) => els.map((e) => ({ stage: e.getAttribute("data-stage"), text: e.textContent.trim() })));
  check(rows.length >= 4, `the sidebar lists live tasks (${rows.length}): ${rows.map((r) => r.stage).join(", ")}`);
  check(rows[0].stage === "needsYou" && /Needs you/.test(rows[0].text), `a task that needs you leads, with an amber "Needs you" chip: "${rows[0].text}"`);
  const amber = await page.locator("[data-slot='task-row'][data-stage='needsYou'] [data-slot='state-chip']").first().getAttribute("data-tone");
  check(amber === "amber", "the Needs-you chip is the amber tone");
  check(rows.some((r) => r.stage === "review" && /Review/.test(r.text)) && rows.some((r) => r.stage === "running" && /Working/.test(r.text)), "review and running tasks carry their own chips");
  const title = await page.title();
  check(/^\(\d+\) Neko$/.test(title), `the document title carries the count when something needs you: "${title}"`);
  const bar = page.locator("[data-slot='status-bar']");
  const barText = (await bar.innerText()).replace(/\s+/g, " ");
  check(/New work is on hold/.test(barText) && /waiting on you|Working/.test(barText), `the status bar shows the machine hold and what waits on you: "${barText}"`);
  check((await page.locator("[data-slot='machine']").getAttribute("data-level")) === "holding", "the machine entry reports the hold level");
  await page.locator("[data-slot='machine']").click();
  await page.waitForSelector("[data-slot='popover-content']", { timeout: 3000 });
  await sleep(450);
  const facts = await page.locator("[data-slot='popover-content']").innerText();
  check(/At most 1 right now/.test(facts) && /Memory/.test(facts) && /Disk/.test(facts), "the machine popover lists the cap and the Mac's facts");
  await shot("shell-machine-popover-dark");
  await page.keyboard.press("Escape");
  for (const theme of themes) {
    await setTheme(theme);
    await shot(`shell-sidebar-${theme}`);
    await a11y(`shell sidebar + status bar (${theme})`);
  }
  await setTheme("dark");

  // Something that newly needs you raises a toast (nothing toasts for what was already waiting at load).
  check((await page.getByText("A task is ready to review").count()) === 0, "no toast for what was already waiting when the page opened");
  mock.withNew = true;
  await textarea.fill("ping");
  await page.keyboard.press("Enter"); // a real change in the daemon: the next snapshot (patched here) has one more task to review
  await page.getByText("A task is ready to review").first().waitFor({ timeout: 8000 });
  check((await page.getByText("Tighten the signup copy").count()) > 0 && (await page.getByRole("button", { name: "Open" }).count()) > 0, "a task that newly needs you raises a toast naming it, with an Open action");
  check(/^\(\d+\) Neko$/.test(await page.title()), "and the title count follows");
  await sleep(500);
  await shot("shell-toast-dark");
  while ((await page.locator("button[aria-label='Close']").count()) > 0) await page.locator("button[aria-label='Close']").first().click().catch(() => {});
  await page.locator("[data-slot='queue-row'] button[aria-label='Remove queued message']").first().click();
  mock.withNew = false;

  // ===========================================================================================
  // Composer: / menu
  // ===========================================================================================
  mock.patchSnapshot = null;
  await goHome();
  await textarea.click();
  await page.keyboard.type("/");
  const menu = page.locator("[data-slot='composer-menu']");
  await menu.waitFor({ timeout: 3000 });
  const geometry = await page.evaluate(() => {
    const m = document.querySelector("[data-slot='composer-menu']").getBoundingClientRect();
    const f = document.querySelector("[data-slot='composer']").getBoundingClientRect();
    const list = document.querySelector("[data-slot='composer-menu'] [role='listbox']");
    const style = getComputedStyle(document.querySelector("[data-slot='composer-menu']"));
    return { menuBottom: m.bottom, composerTop: f.top, menuTop: m.top, rows: list.children.length, client: list.clientHeight, scroll: list.scrollHeight, bg: style.backgroundColor, opacity: style.opacity };
  });
  check(geometry.menuBottom <= geometry.composerTop + 1 && geometry.menuTop >= 0, `the / menu sits above the composer (bottom ${Math.round(geometry.menuBottom)} <= top ${Math.round(geometry.composerTop)})`);
  const alpha = /\/\s*([\d.]+)(%?)\s*\)/.exec(geometry.bg);
  const opaque = geometry.opacity === "1" && !/transparent/.test(geometry.bg) && (!alpha || Number(alpha[1]) / (alpha[2] ? 100 : 1) === 1);
  check(opaque, `the menu background is opaque (${geometry.bg})`);
  check(geometry.rows === 12 && geometry.client <= 7 * 40 + 1 && geometry.scroll > geometry.client, `12 commands, 7 rows tall (${geometry.client}px of ${geometry.scroll}px), the rest scrolls`);
  const first = await page.locator("[data-slot='composer-menu-row']").first().innerText();
  check(/\/plan/.test(first) && /Plan first/.test(first), `/plan leads the list: "${first.replace(/\s+/g, " ")}"`);
  check((await page.locator("[data-slot='composer-menu-row']").allInnerTexts()).some((t) => /\/stop/.test(t) && /⌘⇧Esc/.test(t)), "/stop shows its shortcut");
  await shot("composer-slash-dark");
  await a11y("composer / menu (dark)");
  await setTheme("light");
  await shot("composer-slash-light");
  await a11y("composer / menu (light)");
  await setTheme("dark");
  for (let i = 0; i < 9; i++) await page.keyboard.press("ArrowDown");
  const scrolled = await page.evaluate(() => document.querySelector("[data-slot='composer-menu'] [role='listbox']").scrollTop);
  check(scrolled > 0, `ArrowDown past the seventh row scrolls the list (scrollTop ${scrolled})`);
  check((await page.locator("[data-slot='composer-menu-row'][data-active='true']").innerText()).includes("/tickets") === false, "the highlight moved with the arrows");
  for (let i = 0; i < 20; i++) await page.keyboard.press("ArrowUp");
  await page.keyboard.type("re");
  const filtered = await page.locator("[data-slot='composer-menu-row']").allInnerTexts();
  check(filtered.length === 2 && filtered.every((t) => /\/re/.test(t)), `typing narrows the menu: ${filtered.map((t) => t.split("\n")[0]).join(", ")}`);
  await page.keyboard.press("Escape");
  check((await menu.count()) === 0 && (await textarea.inputValue()) === "/re", "Esc closes the menu and keeps the draft");
  await clearDraft();
  await page.keyboard.type("/rem");
  await page.keyboard.press("Enter");
  check((await textarea.inputValue()) === "/remember ", "Enter on /remember fills it in and waits for the fact");
  await page.keyboard.type("tests use pnpm");
  mock.intercept = ["SendMessage"];
  await page.keyboard.press("Enter");
  await sleep(400);
  const sent = mock.captured.at(-1)?.Workbench?.SendMessage;
  check(sent?.text === "Remember that tests use pnpm", `/remember becomes the chat message the daemon answers in code: "${sent?.text}"`);
  check((await page.locator("[data-slot='composer-error']").innerText()).includes("verify: SendMessage was intercepted"), "the daemon's own error text is shown verbatim above the composer");
  mock.intercept = [];
  await clearDraft();
  // /plan switches the mode and clears the draft.
  await page.keyboard.type("/pl");
  await page.keyboard.press("Enter");
  check((await page.locator("[data-slot='mode-switch'] [role='radio'][aria-checked='true']").innerText()) === "Plan" && (await textarea.inputValue()) === "", "choosing /plan switches to Plan and clears the draft");
  await page.locator("[data-slot='mode-switch'] [role='radio']", { hasText: "Ask" }).click();

  // ===========================================================================================
  // Composer: @ menu
  // ===========================================================================================
  await page.route("**/api/files**", (route) => {
    const q = new URL(route.request().url()).searchParams.get("q");
    return route.fulfill({ json: { files: q && q.length >= 2 ? [{ path: "/Users/you/Documents/neko/crates/neko-daemon/src/web.rs", name: "web.rs", location: "neko › src" }, { path: "/Users/you/Documents/neko/web/src/Home.tsx", name: "Home.tsx", location: "neko › src" }] : [] } });
  });
  await clearDraft();
  await page.keyboard.type("look at @");
  await menu.waitFor({ timeout: 3000 });
  const mentionRows = await page.locator("[data-slot='composer-menu-row']").allInnerTexts();
  check(mentionRows.length > 0 && mentionRows.every((t) => /Workspace|tool/.test(t)), `@ lists workspaces and tools: ${mentionRows.map((t) => t.replace(/\s+/g, " ")).slice(0, 3).join(" | ")}`);
  await page.keyboard.type("we");
  await page.locator("[data-slot='composer-menu-row']", { hasText: "web.rs" }).waitFor({ timeout: 4000 });
  check(true, "@we finds files by name through the daemon (web.rs, Home.tsx)");
  const mBox = await page.locator("[data-slot='composer-menu']").boundingBox();
  const cBox = await composerBox();
  check(mBox.y + mBox.height <= cBox.y + 1, "the @ menu is above the composer too");
  await shot("composer-mention-dark");
  for (let i = 0; i < 12 && !(await page.locator("[data-slot='composer-menu-row'][data-active='true']").innerText()).includes("Home.tsx"); i++) await page.keyboard.press("ArrowDown");
  await page.keyboard.press("Tab");
  check((await textarea.inputValue()) === "look at /Users/you/Documents/neko/web/src/Home.tsx ", `Tab accepts a file and puts its path in the message: "${await textarea.inputValue()}"`);
  await clearDraft();
  await page.unroute("**/api/files**");

  // ===========================================================================================
  // Composer: Ask / Plan, ⌘Return, drafts per scope
  // ===========================================================================================
  await page.locator("[data-slot='mode-switch'] [role='radio']", { hasText: "Plan" }).click();
  await textarea.fill("add dark mode to settings");
  mock.intercept = ["SendMessage"];
  await page.keyboard.press("Enter");
  await sleep(400);
  const plan = mock.captured.at(-1)?.Workbench?.SendMessage?.text ?? "";
  check(plan.startsWith("Plan only, and change nothing yet:") && plan.endsWith("add dark mode to settings"), "Plan mode wraps the message exactly as the native composer does");
  mock.intercept = [];
  await page.locator("[data-slot='mode-switch'] [role='radio']", { hasText: "Ask" }).click();
  await textarea.fill("interrupt me");
  mock.intercept = ["InterruptAndSendMessage"];
  await page.keyboard.press("Meta+Enter");
  await sleep(400);
  const interrupt = mock.captured.at(-1)?.Workbench?.InterruptAndSendMessage;
  check(interrupt?.text === "interrupt me", "⌘Return sends InterruptAndSendMessage");
  mock.intercept = [];
  // A draft is kept per scope while you move around.
  await textarea.fill("a draft that survives");
  await page.goto(`${base}/#/plugins`);
  await page.waitForSelector("[data-slot='plugins-page']", { timeout: 10000 });
  await page.goto(`${base}/#/`);
  await page.waitForSelector("[data-slot='composer'] textarea");
  check((await textarea.inputValue()) === "a draft that survives", "the draft is still there after visiting another page");
  await clearDraft();

  // Shift+Return is a newline, Return sends.
  await textarea.fill("line one");
  await page.keyboard.press("Shift+Enter");
  await page.keyboard.type("line two");
  check((await textarea.inputValue()) === "line one\nline two", "Shift+Return adds a line");
  await clearDraft();

  // ===========================================================================================
  // Composer: attachments
  // ===========================================================================================
  await page.locator("input[type='file']").setInputFiles([
    { name: "screenshot.png", mimeType: "image/png", buffer: PNG },
    { name: "notes.txt", mimeType: "text/plain", buffer: Buffer.from("remember the milk") },
  ]);
  await page.waitForFunction(() => document.querySelectorAll("[data-slot='attachment'][data-status='ready']").length === 2, null, { timeout: 8000 });
  const chipsText = await page.locator("[data-slot='attachment']").allInnerTexts();
  check(chipsText.length === 2 && /screenshot\.png/.test(chipsText[0]) && /Image/.test(chipsText[0]) && /notes\.txt/.test(chipsText[1]) && /File/.test(chipsText[1]), `two chips: ${chipsText.map((t) => t.replace(/\s+/g, " ")).join(" | ")}`);
  check((await page.locator("[data-slot='attachment'] img").count()) === 1, "an image chip shows its thumbnail");
  const kept = existsSync(path.join(DATA, "attachments")) ? readdirSync(path.join(DATA, "attachments")) : [];
  check(kept.length === 2 && kept.some((f) => f.endsWith(".png")) && kept.some((f) => f.endsWith(".txt")), `the daemon kept both files under attachments/ (${kept.map((f) => f.slice(-8)).join(", ")})`);
  await shot("composer-attachments-dark");
  await a11y("composer with attachments");
  await page.locator("[data-slot='attachment'] button[aria-label='Remove notes.txt']").click();
  check((await page.locator("[data-slot='attachment']").count()) === 1, "an attachment chip can be removed");
  await textarea.fill("see the picture");
  mock.intercept = ["SendMessage"];
  await page.keyboard.press("Enter");
  await sleep(400);
  const withImage = mock.captured.at(-1)?.Workbench?.SendMessage?.text ?? "";
  check(/^see the picture\n!\[Image\]\(file:\/\/.+\.png\)$/.test(withImage), `the message carries the image reference the native composer writes: "${withImage.replace(DATA, "<data>")}"`);
  mock.intercept = [];
  // The same file dropped on the composer.
  await page.evaluate(async () => {
    const dt = new DataTransfer();
    dt.items.add(new File(["dropped"], "dropped.md", { type: "text/markdown" }));
    document.querySelector("[data-slot='composer']").dispatchEvent(new DragEvent("drop", { dataTransfer: dt, bubbles: true, cancelable: true }));
  });
  await page.waitForFunction(() => document.querySelectorAll("[data-slot='attachment']").length === 2, null, { timeout: 5000 });
  check(true, "dropping a file on the composer attaches it");
  await clearDraft();
  while ((await page.locator("[data-slot='attachment'] button").count()) > 0) await page.locator("[data-slot='attachment'] button").first().click();
  // A message that is too large says so.
  await textarea.fill("x".repeat(4200));
  check(/limit is 4096/.test(await page.locator("[data-slot='composer-error']").innerText()) && (await page.locator("[data-slot='send']").isDisabled()), "a message over the daemon's 4096-byte limit is refused with the reason, and Send is off");
  await clearDraft();

  // Send / Queue / Stop morph.
  check((await page.locator("[data-slot='send'][data-mode='stop']").count()) === 1, "with a reply running and nothing typed, the button is a stop square");
  await textarea.fill("queue this");
  check((await page.locator("[data-slot='send'][data-mode='queue']").count()) === 1, "with a reply running and a draft, it is Queue");
  await clearDraft();

  // ===========================================================================================
  // Runtime picker
  // ===========================================================================================
  const trigger = page.locator("[data-slot='runtime-trigger']");
  check(/Neko decides/.test(await trigger.innerText()), "the picker starts at \"Neko decides\"");
  await trigger.click();
  await page.waitForSelector("[data-slot='runtime-panel']", { timeout: 4000 });
  await page.locator("[data-slot='model-row']", { hasText: "GPT-6 Astra" }).waitFor({ timeout: 4000 });
  await sleep(450);
  const panel = await page.locator("[data-slot='runtime-panel']").innerText();
  check(/codex/i.test(panel) && /claude code/i.test(panel) && /Sign in with `claude login`/.test(panel) && /Ollama is not running/.test(panel), "the catalog lists each source, with why an unready one cannot be used");
  check(/Unavailable/.test(panel) && /Recommended/.test(panel), "an unavailable model says so; the recommended one is marked");
  const pBox = await page.locator("[data-slot='popover-content']").boundingBox();
  const tBox = await trigger.boundingBox();
  check(pBox.y + pBox.height <= tBox.y + 12, "the picker opens above the composer");
  await shot("composer-runtime-dark");
  await a11y("runtime picker (dark)");
  await setTheme("light");
  await shot("composer-runtime-light");
  await setTheme("dark");
  await page.locator("[data-slot='model-row']", { hasText: "GPT-6 Astra" }).click();
  await page.getByRole("radio", { name: "High" }).click();
  await page.getByRole("radio", { name: "Fast" }).click();
  check((await page.getByRole("button", { name: "Check & apply" }).isEnabled()), "choosing a model, effort and speed enables Check & apply");
  await page.getByRole("button", { name: "Check & apply" }).click();
  await page.waitForFunction(() => /Astra · High · Fast/.test(document.querySelector("[data-slot='runtime-trigger']")?.getAttribute("aria-label") ?? ""), null, { timeout: 8000 });
  check(true, "applying saves it for this conversation (SetConversationRuntime) and the chip reads \"Astra · High · Fast\"");
  check(/Settings applied/.test(await page.locator("[data-slot='runtime-panel']").innerText()), "the panel confirms the daemon accepted it");
  // Changing the model drops a pin it cannot honour, and says so.
  await page.locator("[data-slot='model-row']", { hasText: "GPT-6 Luna" }).click();
  const note = await page.locator("[data-slot='runtime-panel']").innerText();
  check(/Speed reset to Auto: GPT-6 Luna does not advertise fast/.test(note), "picking a model that lacks a pinned speed resets it and explains");
  // The daemon's refusal is shown word for word.
  mock.runtimeError = "GPT-6 Luna is not available on this account. Choose another model.";
  await page.getByRole("button", { name: "Check & apply" }).click();
  await page.locator("[data-slot='runtime-error']").waitFor({ timeout: 5000 });
  check((await page.locator("[data-slot='runtime-error']").innerText()).includes("GPT-6 Luna is not available on this account. Choose another model."), "a refusal from the daemon is shown verbatim");
  await shot("composer-runtime-error-dark");
  await page.getByRole("button", { name: "Reset to Neko decides" }).click();
  await page.getByRole("button", { name: "Check & apply" }).click();
  await page.waitForFunction(() => /Neko decides/.test(document.querySelector("[data-slot='runtime-trigger']")?.textContent ?? ""), null, { timeout: 8000 });
  check(true, "Reset to Neko decides clears every pin");
  await page.keyboard.press("Escape");
  // /models opens it.
  await textarea.fill("/models");
  await page.keyboard.press("Enter");
  await page.waitForSelector("[data-slot='runtime-panel']", { timeout: 3000 });
  check(true, "/models opens the runtime picker");
  await page.keyboard.press("Escape");
  await clearDraft();

  // ===========================================================================================
  // ⌘K palette and the shortcut overlay
  // ===========================================================================================
  mock.patchSnapshot = (snap) => {
    snap.tasks = [task("t-copy", "Rename the export button", "Building", 500), ...(snap.tasks ?? [])];
  };
  await page.reload();
  await page.waitForSelector("[data-slot='composer'] textarea", { timeout: 15000 });
  await sleep(800);
  await page.keyboard.press("Meta+k");
  await page.locator("[data-slot='command']").waitFor({ timeout: 3000 });
  await sleep(300);
  const palette = (await page.locator("[data-slot='command-item']").allInnerTexts()).map((t) => t.replace(/\s+/g, " ").trim());
  const has = (re) => palette.some((t) => re.test(t));
  check(has(/^New task/) && has(/^Stop all work/) && has(/^Approve what is waiting/) && has(/^Keyboard shortcuts/) && has(/first-run tour/) && has(/Reload plugins/), `⌘K lists the shell's actions: ${palette.slice(0, 6).join(" | ")}`);
  check(has(/Go to Plugins/) && has(/Go to Home/) && has(/Go to UI gallery/), "⌘K lists Go to every page, Plugins included");
  check(has(/Rename the export button/) && (await page.locator("[data-slot='command-item'][data-task-id]").count()) > 0, "⌘K lists recent tasks");
  check(has(/Focus the composer/) && has(/Stop the current reply/), "⌘K still lists what Home registers");
  check(has(/^New task.*⌘N/) && has(/Stop all work.*⌘⇧Esc/), "actions show their shortcut hints");
  await shot("palette-full-dark");
  await a11y("palette (dark)");
  await setTheme("light");
  await shot("palette-full-light");
  await setTheme("dark");
  await page.keyboard.type("rename export");
  await sleep(250);
  await page.keyboard.press("Meta+c");
  await page.locator("text=Task ID copied").waitFor({ timeout: 3000 }).catch(() => {});
  check((await page.locator("text=Task ID copied").count()) > 0 || (await page.locator("text=Could not copy").count()) > 0, "⌘C on a task in the palette copies its ID (and says so)");
  // (copying closed the palette; an Esc here would reach the decision dock and decline it)
  await page.waitForFunction(() => !document.querySelector("[data-slot='command']"), null, { timeout: 3000 });
  await sleep(400);
  await page.keyboard.press("Meta+k");
  await page.locator("[data-slot='command']").waitFor({ timeout: 3000 });
  await sleep(500);
  await page.keyboard.type("stop all");
  await sleep(300);
  await shot("palette-stop-all-dark");
  await page.keyboard.press("Enter");
  await page.locator("[data-slot='alert-dialog-content']").waitFor({ timeout: 4000 }).catch(async () => { await shot("debug-stop-all"); throw new Error("no confirmation dialog for Stop all work"); });
  check(/Stop all work\?/.test(await page.locator("[data-slot='alert-dialog-content']").innerText()), "Stop all work asks before it does anything");
  await sleep(450);
  await shot("confirm-stop-all-dark");
  await page.keyboard.press("Escape");
  await page.waitForFunction(() => !document.querySelector("[data-slot='alert-dialog-content']"), null, { timeout: 3000 });
  check(true, "Esc declines the confirmation");
  mock.patchSnapshot = null;

  await page.locator("body").click({ position: { x: 700, y: 400 } });
  await page.keyboard.press("?");
  await page.locator("[data-slot='shortcuts']").waitFor({ timeout: 3000 });
  await sleep(450);
  const keys = await page.locator("[data-slot='shortcuts']").innerText();
  check(/Keyboard shortcuts/.test(keys) && /Open the command palette/.test(keys) && /Stop the reply and send this first/.test(keys) && /Mention a workspace/.test(keys), "? opens the shortcut overlay with the shell's and the composer's keys");
  await shot("shortcuts-dark");
  await a11y("shortcuts overlay (dark)");
  await setTheme("light");
  await shot("shortcuts-light");
  await setTheme("dark");
  await page.keyboard.press("Escape");
  await page.waitForFunction(() => !document.querySelector("[data-slot='shortcuts']"), null, { timeout: 3000 });
  await textarea.click();
  await page.keyboard.type("?");
  check((await page.locator("[data-slot='shortcuts']").count()) === 0 && (await textarea.inputValue()) === "?", "typing ? in the composer does not open the overlay");
  await clearDraft();

  // ===========================================================================================
  // Plugins page
  // ===========================================================================================
  const pluginsDir = path.join(DATA, "plugins");
  rmSync(pluginsDir, { recursive: true, force: true });
  await page.goto(`${base}/#/plugins`);
  await page.waitForSelector("[data-slot='plugins-page']", { timeout: 10000 });
  await page.locator("[data-slot='plugins-empty']").waitFor({ timeout: 8000 });
  check(/No plugins yet/.test(await page.locator("[data-slot='plugins-empty']").innerText()), "with no plugins the page says so and shows the manifest format");
  check((await page.locator("[data-slot='plugin-folder']").innerText()).trim() === pluginsDir, "the plugin folder path is shown");
  await shot("plugins-empty-dark");
  await a11y("plugins (empty)");
  cpSync(path.join(repo, "web/plugins/pr-board"), path.join(pluginsDir, "pr-board"), { recursive: true });
  writeFileSync(path.join(pluginsDir, "pr-board/manifest.json"), JSON.stringify({ title: "PR board", description: "Open pull requests by stage", version: "0.3.1", entry: "index.js" }));
  mkdirSync(path.join(pluginsDir, "broken"), { recursive: true });
  writeFileSync(path.join(pluginsDir, "broken/manifest.json"), "{ not json");
  mkdirSync(path.join(pluginsDir, "crashy"), { recursive: true });
  writeFileSync(path.join(pluginsDir, "crashy/manifest.json"), JSON.stringify({ title: "Crashy", entry: "index.js" }));
  writeFileSync(path.join(pluginsDir, "crashy/index.js"), "export default function register() { throw new Error('boom from the register function'); }");
  const row = (id) => page.locator(`[data-slot='plugin-row'][data-plugin='${id}']`);
  await row("pr-board").waitFor({ timeout: 10000 });
  await sleep(1200);
  const prRow = (await row("pr-board").innerText()).replace(/\s+/g, " ");
  check(/PR board/.test(prRow) && /v0\.3\.1/.test(prRow) && /Running/.test(prRow) && /index\.js/.test(prRow) && /Open pull requests by stage/.test(prRow), `a running plugin shows name, version, entry and status: "${prRow.slice(0, 140)}"`);
  check((await row("broken").getAttribute("data-status")) === "error" && /not valid JSON/.test(await row("broken").locator("[data-slot='plugin-error']").innerText()), "a plugin whose manifest is broken shows the daemon's error");
  check(/boom from the register function/.test(await row("crashy").locator("[data-slot='plugin-error']").innerText()), "a plugin that throws while loading shows what it threw");
  for (const theme of themes) {
    await setTheme(theme);
    await shot(`plugins-list-${theme}`);
    await a11y(`plugins (${theme})`);
  }
  await setTheme("dark");
  // Disable / enable.
  const nav = (text) => page.locator("[data-slot='nav-item']", { hasText: text });
  check((await nav("PR board").count()) > 0, "the plugin's page is in the sidebar while it is on");
  await row("pr-board").getByRole("switch", { name: /Disable PR board/ }).click();
  await page.waitForFunction(() => document.querySelector("[data-slot='plugin-row'][data-plugin='pr-board']")?.getAttribute("data-status") === "off", null, { timeout: 8000 });
  check(existsSync(path.join(pluginsDir, "pr-board/.disabled")), "disabling writes a .disabled marker into the plugin's folder");
  await page.waitForFunction(() => ![...document.querySelectorAll("[data-slot='nav-item']")].some((e) => /PR board/.test(e.textContent)), null, { timeout: 8000 });
  check(/Off/.test(await row("pr-board").locator("[data-slot='plugin-status']").innerText()), "a disabled plugin reads Off, and its sidebar entry is gone");
  await shot("plugins-disabled-dark");
  await row("pr-board").getByRole("switch", { name: /Enable PR board/ }).click();
  await page.waitForFunction(() => /Running/.test(document.querySelector("[data-slot='plugin-row'][data-plugin='pr-board'] [data-slot='plugin-status']")?.textContent ?? ""), null, { timeout: 10000 });
  check(!existsSync(path.join(pluginsDir, "pr-board/.disabled")) && (await nav("PR board").count()) > 0, "enabling removes the marker and the page comes back");
  // Details in the inspector.
  await row("pr-board").getByRole("button", { name: /Details of PR board/ }).click();
  await page.locator("[data-slot='inspector']").waitFor({ timeout: 3000 });
  await sleep(400);
  check(/"version":\s*"0\.3\.1"/.test(await page.locator("[data-slot='plugin-details']").innerText()), "Details opens the inspector with the manifest");
  const handle = await page.locator("[data-slot='resizable-handle']").boundingBox();
  const inspectorBefore = (await page.locator("[data-slot='inspector']").boundingBox()).width;
  await page.mouse.move(handle.x + handle.width / 2, handle.y + 200);
  await page.mouse.down();
  await page.mouse.move(handle.x - 140, handle.y + 200, { steps: 6 });
  await page.mouse.up();
  const inspectorAfter = (await page.locator("[data-slot='inspector']").boundingBox()).width;
  check(inspectorAfter > inspectorBefore + 60, `dragging the handle resizes the inspector (${Math.round(inspectorBefore)} -> ${Math.round(inspectorAfter)}px)`);
  await shot("inspector-dark");
  await a11y("inspector");
  await setTheme("light");
  await shot("inspector-light");
  await setTheme("dark");
  await page.locator("[data-slot='inspector'] button[aria-label='Close the inspector']").click();
  check((await page.locator("[data-slot='inspector']").count()) === 0, "the inspector closes");
  await row("pr-board").getByRole("button", { name: /Details of PR board/ }).click();
  await page.locator("[data-slot='inspector']").waitFor();
  await page.locator("[data-slot='inspector'] button[aria-label='Close the inspector']").focus();
  await page.keyboard.press("Escape");
  check((await page.locator("[data-slot='inspector']").count()) === 0, "Esc closes the inspector from inside it");
  // Remove, with a confirmation.
  await row("broken").getByRole("button", { name: /Remove broken/ }).click();
  await page.locator("[data-slot='alert-dialog-content']").waitFor({ timeout: 3000 });
  check(/cannot be undone/.test(await page.locator("[data-slot='alert-dialog-content']").innerText()), "removing a plugin asks first and says it cannot be undone");
  await sleep(450);
  await shot("plugins-remove-confirm-dark");
  await page.getByRole("button", { name: "Cancel" }).click();
  check(existsSync(path.join(pluginsDir, "broken")), "Cancel keeps the plugin");
  await row("broken").getByRole("button", { name: /Remove broken/ }).click();
  await page.locator("[data-slot='confirm-action']").click();
  await page.waitForFunction(() => !document.querySelector("[data-slot='plugin-row'][data-plugin='broken']"), null, { timeout: 8000 });
  check(!existsSync(path.join(pluginsDir, "broken")), "confirming removes the plugin's folder");
  // Build with Neko.
  await page.getByRole("button", { name: "Build a plugin with Neko" }).first().click();
  await page.locator("[data-slot='build-dialog']").waitFor({ timeout: 3000 });
  await sleep(450);
  check((await page.locator("[data-slot='build-send']").isDisabled()), "Ask Neko is off until you say what it should do");
  await page.locator("#plugin-wish").fill("A page that lists the open pull requests with their checks");
  await page.locator("[data-slot='build-dialog'] summary").click();
  const prompt = await page.locator("[data-slot='build-prompt']").innerText();
  check(prompt.includes(pluginsDir) && prompt.includes("A page that lists the open pull requests") && /"entry": "index\.js"/.test(prompt) && /registerPage/.test(prompt), "the prompt shown includes the plugins folder, the request, the manifest format and a minimal example");
  await shot("plugins-build-dialog-dark");
  await a11y("build dialog");
  mock.intercept = ["SendMessage"];
  await page.locator("[data-slot='build-send']").click();
  await page.locator("[data-slot='build-error']").waitFor({ timeout: 4000 });
  const buildBody = mock.captured.at(-1)?.Workbench?.SendMessage;
  check(buildBody?.text.startsWith("Open a task that builds a plugin for Neko's web UI.") && buildBody.text.includes(`${pluginsDir}/<name>/`) && !buildBody.text.includes("{{") && buildBody.text.length < 4096, "Ask Neko sends one well-formed chat message (SendMessage) that opens a task");
  check(/verify: SendMessage was intercepted/.test(await page.locator("[data-slot='build-error']").innerText()), "a failure is shown in the dialog, word for word, and the dialog stays open");
  mock.intercept = [];
  await page.locator("[data-slot='build-send']").click();
  await page.waitForFunction(() => location.hash === "#/" || location.hash === "", null, { timeout: 6000 });
  check(true, "on success the dialog closes and takes you to Home, where the task will appear");
  await page.locator("[data-slot='queue-row']", { hasText: "Open a task that builds a plugin" }).waitFor({ timeout: 6000 });
  await page.locator("[data-slot='queue-row'] button[aria-label='Remove queued message']").first().click();
  await page.waitForFunction(() => document.querySelectorAll("[data-slot='queue-row']").length === 0, null, { timeout: 5000 });
  // A plugin can open the inspector and add to ⌘K.
  writeFileSync(path.join(pluginsDir, "pr-board/index.js"), `export default function register(host) {
    const { React, registerPage, registerCommands, inspector } = host;
    registerCommands([{ id: "peek", title: "Peek from the plugin", run: () => inspector.open({ title: "Peek", subtitle: "Plugin", children: React.createElement("p", { style: { padding: 16 } }, "hello from a plugin") }) }]);
    registerPage({ id: "board", title: "PR board", component: () => React.createElement("div", { style: { padding: 24 } }, "board") });
  }`);
  await sleep(4500);
  await page.keyboard.press("Meta+k");
  await page.locator("[data-slot='command']").waitFor({ timeout: 3000 });
  await sleep(500);
  await page.keyboard.type("peek from");
  await sleep(300);
  await shot("palette-plugin-command-dark");
  await page.keyboard.press("Enter");
  await page.locator("[data-slot='inspector']").waitFor({ timeout: 4000 });
  check(/hello from a plugin/.test(await page.locator("[data-slot='inspector']").innerText()), "a plugin can add a ⌘K command and open the inspector (host.registerCommands, host.inspector)");
  await page.locator("[data-slot='inspector'] button[aria-label='Close the inspector']").focus();
  await page.keyboard.press("Escape");
  rmSync(pluginsDir, { recursive: true, force: true });

  // ===========================================================================================
  // Unknown page
  // ===========================================================================================
  await page.goto(`${base}/#/nowhere`);
  await page.locator("[data-slot='missing']").waitFor({ timeout: 5000 });
  check(/not in the web app yet/.test(await page.locator("[data-slot='missing']").innerText()), "an unknown page says so and offers the way back");
  await shot("missing-page-dark");
  await a11y("missing page");

  // ===========================================================================================
  // First run
  // ===========================================================================================
  mock.models = true;
  await page.evaluate(() => fetch("/api/request", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ SetOnboardingComplete: { completed: false } }) }));
  await page.goto(`${base}/#/`);
  await page.reload();
  const onboarding = page.locator("[data-slot='onboarding']");
  await onboarding.waitFor({ timeout: 10000 });
  check(/A little more\s+off your mind/.test(await onboarding.innerText()), "when first run is not finished, the welcome step opens by itself");
  check((await onboarding.getByRole("button", { name: "Skip setup" }).count()) === 0, "there is nothing to skip on the welcome screen");
  for (const theme of themes) {
    await setTheme(theme);
    await shot(`onboarding-welcome-${theme}`);
    await a11y(`onboarding welcome (${theme})`);
  }
  await setTheme("dark");
  await onboarding.getByRole("button", { name: /Get started/ }).click();
  await onboarding.getByText("Full Disk Access").first().waitFor();
  const perms = await onboarding.innerText();
  check(/Privacy & Security → Full Disk Access/.test(perms) && /Accessibility/.test(perms) && /Clipboard history/.test(perms), "permissions are explained, with Full Disk Access guidance");
  check((await onboarding.locator("a[href^='x-apple.systempreferences:']").count()) === 2, "each permission links to its System Settings pane");
  const clip = onboarding.getByRole("switch", { name: "Clipboard history" });
  await page.waitForFunction(() => { const el = document.querySelector("[role='switch'][aria-label='Clipboard history']"); return el && el.getAttribute("aria-disabled") !== "true" && !el.hasAttribute("data-disabled"); }, null, { timeout: 5000 });
  const wasOn = (await clip.getAttribute("aria-checked")) === "true";
  await clip.click();
  await page.waitForFunction((was) => (document.querySelector("[role='switch'][aria-label='Clipboard history']")?.getAttribute("aria-checked") === "true") !== was, wasOn, { timeout: 5000 });
  check(true, "the clipboard switch really changes the daemon's setting");
  await clip.click();
  for (const theme of themes) {
    await setTheme(theme);
    await shot(`onboarding-permissions-${theme}`);
    await a11y(`onboarding permissions (${theme})`);
  }
  await setTheme("dark");
  await page.keyboard.press("Escape");
  check(/A little more\s+off your mind/.test(await onboarding.innerText()), "Esc steps back");
  await onboarding.getByRole("button", { name: /Get started/ }).click();
  await onboarding.getByRole("button", { name: /Continue/ }).click();
  await onboarding.getByText("Where do").waitFor();
  await onboarding.getByLabel("Folder path").fill("not a path");
  await onboarding.getByRole("button", { name: "Add workspace" }).click();
  await onboarding.locator("[data-slot='onboarding-error']").waitFor();
  check(/starting with \//.test(await onboarding.locator("[data-slot='onboarding-error']").innerText()), "a folder path that is not a path is explained");
  await onboarding.getByLabel("Folder path").fill(DATA);
  await onboarding.getByLabel("Name (optional)").fill("Test workspace");
  await onboarding.getByRole("button", { name: "Add workspace" }).click();
  await onboarding.locator("[data-slot='workspace-list']").getByText("Test workspace").waitFor({ timeout: 6000 });
  check(true, "a workspace folder can be connected from the tour (SaveWorkspaceWithFolders)");
  for (const theme of themes) {
    await setTheme(theme);
    await shot(`onboarding-workspace-${theme}`);
  }
  await setTheme("dark");
  await onboarding.getByRole("button", { name: /Continue/ }).click();
  await onboarding.locator("[data-slot='runtime-choices']").getByText("GPT-6 Astra").waitFor({ timeout: 6000 });
  check(/Neko decides/.test(await onboarding.innerText()) && /Sign in with `claude login`/.test(await onboarding.innerText()), "the runtime step lists what is ready and why the rest is not");
  await onboarding.getByRole("radio", { name: /GPT-6 Astra/ }).click();
  for (const theme of themes) {
    await setTheme(theme);
    await shot(`onboarding-runtime-${theme}`);
    await a11y(`onboarding runtime (${theme})`);
  }
  await setTheme("dark");
  await onboarding.getByRole("button", { name: /Continue/ }).click();
  await onboarding.getByText("existing setup").first().waitFor();
  await onboarding.getByRole("button", { name: "Look", exact: true }).click();
  await onboarding.locator("[data-slot='import-sources']").waitFor({ timeout: 6000 });
  check(true, "the optional import step looks for setups (SetupImport ListSources)");
  await shot("onboarding-import-dark");
  await page.keyboard.press("Enter"); // Enter continues (focus is not on a control)
  await sleep(300);
  if (!/Neko is\s+ready/.test(await onboarding.innerText())) await onboarding.getByRole("button", { name: /Continue/ }).click();
  await onboarding.getByText("ready.").waitFor();
  check(/GPT-6 Astra|gpt-6-astra/.test(await onboarding.innerText()), "the last step summarises what was chosen");
  await shot("onboarding-finish-dark");
  await onboarding.getByRole("button", { name: "Open Neko" }).click();
  await onboarding.waitFor({ state: "detached", timeout: 6000 });
  const state = await page.evaluate(() => fetch("/api/request", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify("GetOnboardingState") }).then((r) => r.json()));
  check(state.OnboardingState.completed === true, "finishing marks first run complete in the daemon");
  const runtime = await page.evaluate(() => fetch("/api/request", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ Workbench: "Snapshot" }) }).then((r) => r.json()));
  check(runtime.Workbench.agent_runtime.model === "gpt-6-astra", "the runtime picked in the tour was saved (SetAgentRuntime)");
  // Skip from the middle.
  await page.evaluate(() => fetch("/api/request", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ SetOnboardingComplete: { completed: false } }) }));
  await page.reload();
  await onboarding.waitFor({ timeout: 10000 });
  await onboarding.getByRole("button", { name: /Get started/ }).click();
  await onboarding.getByRole("button", { name: "Skip setup" }).click();
  await onboarding.waitFor({ state: "detached", timeout: 6000 });
  check((await page.evaluate(() => fetch("/api/request", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify("GetOnboardingState") }).then((r) => r.json()))).OnboardingState.completed === true, "Skip setup finishes first run");
  // /setup shows it again; Esc on the first step closes a re-opened tour.
  await page.goto(`${base}/#/`);
  await page.waitForSelector("[data-slot='composer'] textarea");
  await page.locator("[data-slot='composer'] textarea").fill("/setup");
  await page.keyboard.press("Enter");
  await onboarding.waitFor({ timeout: 4000 });
  await page.keyboard.press("Escape");
  await onboarding.waitFor({ state: "detached", timeout: 4000 });
  check(true, "/setup shows the tour again, and Esc closes it");
  await page.locator("[data-slot='composer'] textarea").fill("/permissions");
  await page.keyboard.press("Enter");
  await onboarding.getByText("Full Disk Access").first().waitFor({ timeout: 4000 });
  check(true, "/permissions opens the tour at the permissions step");
  await page.keyboard.press("Escape");
  await page.keyboard.press("Escape");
  await onboarding.waitFor({ state: "detached", timeout: 4000 });

  await page.unroute("**/api/request");
  await goHome();
}
