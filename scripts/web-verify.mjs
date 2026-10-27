#!/usr/bin/env node
// Self-verification loop for the web UI: starts a TEST daemon (never the real
// one), opens the app in the shared headless Chrome and screenshots Home, the
// /#/ui gallery (every section), the ⌘K palette and the PR-board plugin, each in
// dark and light (the system preference is emulated, which is exactly how the
// app picks its theme), to /tmp/neko-web/. It exercises the composer, the
// streaming reply and plugin hot-loading, checks the daemon's access rules, and
// exits non-zero on any console error, failed request or obvious accessibility
// problem (images without alt, unlabeled buttons, inputs and links).
//
//   pnpm --dir web build && node scripts/web-verify.mjs
//
// NEKO_WEB_WIDTH=1100 narrows the viewport; NEKO_WEB_SHOTS sets the output dir.
// An agent can edit web/src, rebuild (`pnpm --dir web build`), run this script
// and read /tmp/neko-web/*.png: the whole edit-and-look loop without a human.
//
// Rules this script follows (see ~/.agents/BROWSER.md): it never launches a
// browser; it acquires the shared Chrome, uses its own context, closes it in
// `finally` and releases the lease.

import { execFileSync, spawn } from "node:child_process";
import { createRequire } from "node:module";
import { cpSync, existsSync, mkdirSync, readdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import http from "node:http";
import net from "node:net";
import os from "node:os";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const require = createRequire(import.meta.url);
const { chromium } = require(process.env.PLAYWRIGHT_CORE ?? "/Users/nish/Documents/loreswipe/node_modules/playwright-core");

const repo = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const PORT = Number(process.env.NEKO_WEB_PORT ?? 4748);
const DATA = process.env.NEKO_WEB_VERIFY_DATA ?? "/tmp/neko-web-data";
const OUT = process.env.NEKO_WEB_SHOTS ?? "/tmp/neko-web";
// The made-up snapshot in scripts/fixtures/web-seed.mjs unless NEKO_WEB_SEED
// names another (a saved live snapshot, unwrapped). Every area reads the same.
const SEED = process.env.NEKO_WEB_SEED ?? path.join(DATA + "-seed.json");
if (!process.env.NEKO_WEB_SEED) {
  const { webSeed } = await import(pathToFileURL(path.join(repo, "scripts/fixtures/web-seed.mjs")).href);
  writeFileSync(SEED, JSON.stringify(webSeed()));
  process.env.NEKO_WEB_SEED = SEED;
}
const TARGET = process.env.CARGO_TARGET_DIR ?? "/Users/nish/Documents/neko/target";
const HARNESS = process.env.NEKO_WEB_HARNESS ?? path.join(TARGET, "debug/web_harness");
const LEASE = process.env.NEKO_WEB_LEASE ?? "neko-web-foundation";
const CHROME = "http://127.0.0.1:9333";
const sharedChrome = path.join(os.homedir(), ".agents/bin/shared-chrome");

const failures = [];
const check = (ok, label) => {
  console.log(`${ok ? "ok  " : "FAIL"} ${label}`);
  if (!ok) failures.push(label);
};
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

function raw(headers, method = "GET", target = "/api/plugins") {
  return new Promise((resolve, reject) => {
    const req = http.request({ host: "127.0.0.1", port: PORT, method, path: target, headers, setHost: false }, (res) => {
      res.resume();
      res.on("end", () => resolve(res.statusCode));
    });
    req.on("error", reject);
    req.end();
  });
}

async function waitForPort(port, ms = 30000) {
  const end = Date.now() + ms;
  while (Date.now() < end) {
    const open = await new Promise((r) => {
      const s = net.connect(port, "127.0.0.1", () => { s.destroy(); r(true); });
      s.on("error", () => r(false));
    });
    if (open) return;
    await sleep(200);
  }
  throw new Error(`nothing listened on ${port}`);
}

function startDaemon() {
  if (!existsSync(HARNESS)) throw new Error(`${HARNESS} is missing: cargo build -p neko-daemon --bin web_harness`);
  if (!existsSync(path.join(repo, "web/dist/index.html"))) throw new Error("web/dist is missing: pnpm --dir web build");
  rmSync(DATA, { recursive: true, force: true });
  mkdirSync(DATA, { recursive: true });
  const child = spawn(HARNESS, [], {
    cwd: DATA,
    env: {
      PATH: process.env.PATH, HOME: DATA,
      NEKO_DATA_DIR: DATA, NEKO_WEB_PORT: String(PORT),
      NEKO_WEB_DIR: path.join(repo, "web/dist"), NEKO_WEB_STREAM: "1",
      NEKO_WEB_SEED: SEED,
    },
    stdio: ["ignore", "inherit", "inherit"],
  });
  return child;
}

function stopDaemon(child) {
  if (!child || child.exitCode !== null) return;
  // By pid, and only after confirming it is the harness this script started.
  const command = execFileSync("ps", ["-p", String(child.pid), "-o", "command="]).toString();
  if (command.includes("web_harness")) process.kill(child.pid, "SIGTERM");
}

mkdirSync(OUT, { recursive: true });

/**
 * "Axe-lite": the obvious problems, found in the live DOM. Images without alt,
 * buttons and links with no accessible name, form controls with no label.
 * Returns a list of strings; empty means clean.
 */
async function obviousProblems(page) {
  return page.evaluate(() => {
    const out = [];
    const visible = (el) => {
      const r = el.getBoundingClientRect();
      const s = getComputedStyle(el);
      return s.visibility !== "hidden" && s.display !== "none" && (r.width > 0 || r.height > 0);
    };
    const describe = (el) => `${el.tagName.toLowerCase()}${el.getAttribute("data-slot") ? `[${el.getAttribute("data-slot")}]` : ""}${el.className && typeof el.className === "string" ? "." + el.className.split(/\s+/).slice(0, 2).join(".") : ""}`;
    const named = (el) => {
      if (el.getAttribute("aria-label")?.trim()) return true;
      const by = el.getAttribute("aria-labelledby");
      if (by && by.split(/\s+/).some((id) => document.getElementById(id)?.textContent?.trim())) return true;
      if (el.getAttribute("title")?.trim()) return true;
      if (el.labels && el.labels.length && [...el.labels].some((l) => l.textContent.trim())) return true;
      if ((el.textContent ?? "").trim()) return true;
      if ([...el.querySelectorAll("img")].some((i) => i.getAttribute("alt")?.trim())) return true;
      if ([...el.querySelectorAll("[aria-label]")].some((i) => i.getAttribute("aria-label").trim())) return true;
      return false;
    };
    for (const img of document.querySelectorAll("img")) {
      if (!img.hasAttribute("alt")) out.push(`image without alt: ${img.getAttribute("src")?.slice(0, 60)}`);
    }
    for (const el of document.querySelectorAll("button, [role=button], [role=switch], [role=checkbox], [role=tab], summary")) {
      if (visible(el) && !named(el)) out.push(`unlabeled ${describe(el)}`);
    }
    for (const el of document.querySelectorAll("a[href]")) {
      if (visible(el) && !named(el)) out.push(`link without a name: ${el.getAttribute("href")}`);
    }
    for (const el of document.querySelectorAll("input:not([type=hidden]), textarea, select")) {
      // Primitives keep a hidden twin input for forms; it is not a control anyone can reach.
      if (el.closest("[aria-hidden=true]") || el.tabIndex === -1) continue;
      if (visible(el) && !named(el)) out.push(`form control without a label: ${describe(el)}`);
    }
    return [...new Set(out)];
  });
}

const daemon = startDaemon();
let browser, context, leased = false;
try {
  await waitForPort(PORT);
  const token = readFileSync(path.join(DATA, "web-token"), "utf8").trim();

  // --- access rules, against the live daemon -------------------------------
  const host = `127.0.0.1:${PORT}`;
  check((await raw({ host })) === 401, "no token is refused (401)");
  check((await raw({ host: `evil.example:${PORT}`, authorization: `Bearer ${token}` })) === 403, "a foreign Host is refused (403)");
  check((await raw({ host, origin: "https://evil.example", authorization: `Bearer ${token}`, "content-type": "application/json" }, "POST", "/api/request")) === 403, "a cross-origin POST is refused (403)");
  check((await raw({ host, authorization: `Bearer ${token}` })) === 200, "the right token is accepted (200)");

  // --- browser ---------------------------------------------------------------
  execFileSync(sharedChrome, ["acquire", LEASE], { stdio: "inherit" });
  leased = true;
  browser = await chromium.connectOverCDP(CHROME);
  context = await browser.newContext({ viewport: { width: Number(process.env.NEKO_WEB_WIDTH ?? 1440), height: 900 }, deviceScaleFactor: 1, colorScheme: "dark" });
  const page = await context.newPage();
  const problems = [];
  page.on("console", (m) => { if (m.type() === "error") problems.push(`console: ${JSON.stringify(m.text())} @${m.location().url}`); });
  page.on("pageerror", (e) => problems.push(`pageerror: ${e.message}`));
  page.on("requestfailed", (r) => { if (!r.url().includes("/api/events") && !r.url().includes("/api/request")) problems.push(`request failed: ${r.url()} (${r.failure()?.errorText ?? "?"})`); });
  page.on("response", (r) => { if (r.status() >= 400) problems.push(`HTTP ${r.status()}: ${r.url()}`); });

  const shot = (name, options = {}) => page.screenshot({ path: path.join(OUT, `${name}.png`), ...options });
  const a11y = async (label) => {
    const found = await obviousProblems(page);
    check(found.length === 0, found.length ? `${label}: ${found.join("; ")}` : `${label}: no obvious accessibility problems`);
  };
  const scroller = () => page.locator("[role='log'] > div").first();
  const scrollTo = (top) => scroller().evaluate((el, t) => el.scrollTo({ top: t }), top);

  await page.goto(`http://127.0.0.1:${PORT}/?token=${token}`, { waitUntil: "domcontentloaded" });
  check(new URL(page.url()).search === "", "the token is gone from the URL after sign-in");
  await page.waitForSelector("[data-slot='user-turn']", { timeout: 20000 });
  await page.waitForSelector("[data-slot='work-log'][data-running='true']", { timeout: 10000 });
  await sleep(800);
  check(await page.evaluate(() => document.documentElement.classList.contains("dark")), "the dark system preference gives <html class=dark>");

  // Live turn: "Working for Ns" with the newest step, and the answer growing.
  const header = (await page.locator("[data-slot='work-log'][data-running='true'] [data-slot='work-head']").innerText()).trim();
  check(/^Working for \d+s/.test(header), `live work log reads "${header.replace(/\s+/g, " ")}"`);
  check((await page.locator("[data-slot='work-log'][data-running='true'] [data-slot='work-live']").count()) === 1, "the newest step is shown while working");
  const pendingProse = "[data-slot='neko-turn'][data-pending='true'] [data-slot='prose']";
  const grow = async () => (await page.locator(pendingProse).innerText().catch(() => "")).length;
  const before = await grow();
  // Finished paragraphs arrive as whole blocks, so wait for the next one.
  await page.waitForFunction(([sel, n]) => (document.querySelector(sel)?.textContent ?? "").length > n, [pendingProse, before], { timeout: 20000 }).catch(() => {});
  const after = await grow();
  check(after > before, `the pending reply streams (${before} -> ${after} characters)`);
  await shot("home-live");

  // Finished turn: scroll to its log, open it.
  const finished = page.locator("[data-slot='neko-turn'][data-pending='false']").filter({ has: page.locator("[data-slot='work-log']") }).last();
  await finished.scrollIntoViewIfNeeded();
  const done = (await finished.locator("[data-slot='work-head']").innerText()).replace(/\s+/g, " ").trim();
  check(/^Worked for 1m 12s/.test(done), `finished work log reads "${done}"`);
  await finished.locator("[data-slot='work-head']").click();
  check((await finished.locator("[data-slot='work-step']").count()) > 0, "the work log expands to its steps");
  await finished.evaluate((el) => el.scrollIntoView({ block: "start" }));
  await sleep(300);
  await shot("home-worklog");
  check((await page.locator("[data-slot='prose'] table").count()) > 0, "a GFM table renders");

  // Decision dock: the exact thing being approved, above a composer that stays usable.
  await scrollTo(1e9);
  await page.waitForSelector("[data-slot='decision']", { timeout: 5000 });
  const decision = (await page.locator("[data-slot='decision']").textContent()).replace(/\s+/g, " ");
  check(/Tool approval/i.test(decision) && /save_issue/.test(decision) && /Approve/.test(decision) && /Decline/.test(decision) && /1 of \d+/.test(decision), `a pending decision docks above the composer: "${decision.slice(0, 90)}…"`);
  check(await page.locator("textarea").isEnabled(), "the composer stays usable while a decision is pending");
  check((await page.locator("textarea").getAttribute("placeholder")).startsWith("Queue a message"), "the composer says it can queue");
  check((await page.locator("[data-slot='send'][data-mode='stop']").count()) === 1, "the send button is a stop square while a reply runs");
  await a11y("Home (dark)");
  await shot("home-decision");

  // Queue: send while the reply runs (same SendMessage the native app sends; the test daemon never starts a model).
  const message = `web verify ${Date.now()}`;
  await page.locator("textarea").fill(message);
  check((await page.locator("[data-slot='send']:not([data-mode='stop'])").count()) === 1, "typing turns the stop square back into a send arrow");
  await page.locator("textarea").press("Enter");
  await page.locator("[data-slot='queue-row']", { hasText: message }).waitFor({ timeout: 10000 });
  check(true, "a message sent mid-reply waits in the queue tray");
  check((await page.locator("[data-slot='user-turn']", { hasText: message }).count()) === 0, "a queued message is not drawn in the transcript yet");
  await shot("home-queue");
  await page.locator("[data-slot='queue-row'] button[aria-label='Edit queued message']").click();
  await page.waitForFunction((t) => document.querySelector("textarea").value === t, message, { timeout: 5000 });
  await page.waitForFunction(() => document.querySelectorAll("[data-slot='queue-row']").length === 0, null, { timeout: 5000 }).catch(() => {});
  check((await page.locator("[data-slot='queue-row']").count()) === 0, "editing takes the message back out of the queue into the composer");
  await page.locator("textarea").press("Enter");
  await page.locator("[data-slot='queue-row']", { hasText: message }).waitFor({ timeout: 10000 });
  await page.locator("[data-slot='queue-row'] button[aria-label='Remove queued message']").click();
  await page.waitForFunction(() => document.querySelectorAll("[data-slot='queue-row']").length === 0, null, { timeout: 5000 });
  check(true, "a queued message can be removed");

  // Decline with the keyboard (composer empty): the next decision moves up.
  await page.locator("textarea").press("Escape");
  await page.waitForFunction(() => /Tell its agent/.test(document.querySelector("[data-slot='decision']")?.textContent ?? ""), null, { timeout: 5000 });
  check(true, "Esc declines the card in front and the next decision appears");
  await shot("home-decision-tool");

  // Scroll to end pill: only when scrolled away from the bottom.
  await scrollTo(1e9);
  await sleep(500);
  check((await page.locator("[data-slot='to-end']").count()) === 0, "no scroll-to-end pill while at the bottom");
  await scrollTo(0);
  await page.locator("[data-slot='to-end']").waitFor({ timeout: 3000 });
  await shot("home-top");
  await page.locator("[data-slot='to-end']").click();
  await page.waitForFunction(() => { const e = document.querySelector("[role='log'] > div"); return e.scrollHeight - e.scrollTop - e.clientHeight < 90; }, null, { timeout: 5000 });
  check(true, "the pill returns to the end of the conversation");
  check((await page.locator(".chat-markdown-table-container button[aria-label='Copy table']").count()) > 0, "each table has a copy button");

  // --- Home in light: the same page, the system preference flipped ------------
  await page.emulateMedia({ colorScheme: "light" });
  await page.waitForFunction(() => document.documentElement.classList.contains("light"), null, { timeout: 3000 });
  check(true, "the light system preference gives <html class=light> without a reload");
  await finished.evaluate((el) => el.scrollIntoView({ block: "start" }));
  await sleep(300);
  await shot("home-worklog-light");
  await scrollTo(1e9);
  await sleep(500);
  await shot("home-light");
  await a11y("Home (light)");

  // --- ⌘K palette -----------------------------------------------------------------
  await page.keyboard.press("Meta+k");
  await page.locator("[data-slot='command']").waitFor({ timeout: 3000 });
  await sleep(400);
  const entries = await page.locator("[data-slot='command-item']").allInnerTexts();
  check(entries.some((e) => /Go to Home/.test(e)) && entries.some((e) => /Go to UI gallery/.test(e)), `⌘K lists "Go to" entries: ${entries.slice(0, 3).map((e) => e.trim()).join(", ")}…`);
  check(entries.some((e) => /Focus the composer/.test(e)) && entries.some((e) => /Stop the current reply/.test(e)), "⌘K lists the actions Home contributes");
  await shot("palette-light");
  await page.emulateMedia({ colorScheme: "dark" });
  await sleep(300);
  await shot("palette-dark");
  await a11y("palette");
  await page.keyboard.type("gallery");
  await sleep(200);
  await page.keyboard.press("Enter");
  await page.waitForSelector("[data-gallery-section]", { timeout: 10000 });
  check(new URL(page.url()).hash === "#/ui", "choosing “Go to UI gallery” in ⌘K routes to #/ui");

  // --- gallery: every section, dark then light -----------------------------------------
  const sections = await page.locator("[data-gallery-section]").evaluateAll((els) => els.map((e) => e.getAttribute("data-gallery-section")));
  check(sections.length >= 7, `the gallery renders ${sections.length} sections: ${sections.join(", ")}`);
  await page.setViewportSize({ width: Number(process.env.NEKO_WEB_WIDTH ?? 1440), height: 2600 });
  for (const theme of ["dark", "light"]) {
    await page.emulateMedia({ colorScheme: theme });
    await sleep(700);
    for (const id of sections) {
      await page.locator(`[data-gallery-section='${id}']`).screenshot({ path: path.join(OUT, `gallery-${id}-${theme}.png`) });
    }
    await a11y(`gallery (${theme})`);
  }
  check((await page.locator("[data-gallery-section='workspace'] .dv-tab").count()) >= 4, "the gallery's Workspace section shows the dock with its four tabs");
  check((await page.locator("[data-gallery-section='sidebar'] [data-sidebar-option]").count()) >= 3, "the gallery's Sidebar section shows the sidebar options side by side");
  {
    const option = (id) => page.locator(`[data-gallery-section='sidebar'] [data-sidebar-option='${id}']`);
    await option("setup-folds").locator("[data-slot='mock-setup']").click();
    await sleep(450);
    check(await option("setup-folds").getByRole("button", { name: "Schedules" }).isVisible(), "the gallery's option D: clicking Setup reveals Schedules under it");
    await option("top-and-bottom").getByRole("button", { name: "Memory" }).click();
    check((await option("top-and-bottom").getByRole("button", { name: "Memory" }).getAttribute("aria-current")) === "page", "the gallery's option E: clicking a page makes it the current one");
    const switcher = option("top-and-bottom").locator("[data-slot='mock-workspace-switcher']");
    check(/Nishant/.test(await switcher.innerText()), "the gallery's options show the current workspace next to the mark");
    await switcher.click();
    await page.getByRole("menuitem", { name: /athena/ }).waitFor({ timeout: 3000 });
    check((await page.getByRole("menuitem", { name: /All workspaces/ }).count()) === 1 && (await page.getByRole("menuitem", { name: /Manage workspaces/ }).count()) === 1, "the switcher lists the workspaces, All workspaces and Manage workspaces…");
    await sleep(300);
    await page.locator("[data-gallery-section='sidebar']").screenshot({ path: path.join(OUT, "gallery-sidebar-switcher.png") });
    await page.getByRole("menuitem", { name: /athena/ }).click();
    await sleep(200);
    check(/athena/.test(await switcher.innerText()), "choosing a workspace in the switcher shows its name");
    await option("quiet").getByRole("button", { name: "Collapse sidebar" }).click();
    await sleep(450);
    await page.locator("[data-gallery-section='sidebar']").screenshot({ path: path.join(OUT, "gallery-sidebar-interactive.png") });
    await option("quiet").getByRole("button", { name: "Expand sidebar" }).click();
    await option("setup-folds").locator("[data-slot='mock-setup']").click();
  }
  const blocks = await page.locator("[data-slot='reply-blocks'] [data-slot='reply-block']").evaluateAll((els) => els.map((e) => e.getAttribute("data-kind")));
  check(["choices", "plan", "chart", "form"].every((k) => blocks.includes(k)), `a reply's neko-* blocks are drawn, not shown as code (${blocks.join(", ") || "none"})`);
  check((await page.locator("[data-slot='reply-blocks'] [data-slot='reply-block'][data-kind='choices'] [role='radio']").count()) === 2, "the question offers its two options as radio choices");
  await page.setViewportSize({ width: Number(process.env.NEKO_WEB_WIDTH ?? 1440), height: 900 });
  for (const theme of ["dark", "light"]) {
    await page.emulateMedia({ colorScheme: theme });
    await sleep(500);
    await page.evaluate(() => document.querySelector("[data-slot='gallery']").scrollTo({ top: 0 }));
    await shot(`gallery-${theme}`);
  }
  // Overlays open: a dropdown, then a dialog.
  await page.emulateMedia({ colorScheme: "dark" });
  await sleep(400);
  await page.locator("[data-gallery-section='overlays']").scrollIntoViewIfNeeded();
  await page.getByRole("button", { name: /^Actions/ }).click();
  await page.locator("[data-slot='dropdown-menu-content']").waitFor({ timeout: 3000 });
  await sleep(300);
  await shot("gallery-dropdown-dark");
  await page.keyboard.press("Escape");
  await page.getByRole("button", { name: "Dialog", exact: true }).click();
  await page.locator("[data-slot='dialog-content']").waitFor({ timeout: 3000 });
  await sleep(300);
  await shot("gallery-dialog-dark");
  await page.keyboard.press("Escape");
  await sleep(300);

  // --- plugin: installed while the app is open, no rebuild ---------------------------------
  await page.emulateMedia({ colorScheme: "dark" });
  await page.waitForFunction(() => document.documentElement.classList.contains("dark"), null, { timeout: 3000 });
  await page.goto(`http://127.0.0.1:${PORT}/#/`);
  await sleep(600);
  const nav = (text) => page.locator("[data-slot='nav-item']", { hasText: text });
  const pluginDir = path.join(DATA, "plugins/pr-board");
  check((await nav("PR board").count()) === 0, "no plugin page before the plugin is installed");
  cpSync(path.join(repo, "web/plugins/pr-board"), pluginDir, { recursive: true });
  await nav("PR board").waitFor({ timeout: 10000 });
  check(true, "the plugin appears in the sidebar without rebuilding the app");
  await nav("PR board").click();
  await page.waitForSelector(".pr-card", { timeout: 10000 });
  const cards = await page.locator(".pr-card").count();
  check(cards > 0, `the PR board lists ${cards} pull requests`);
  const groups = await page.locator(".pr-board h2").allInnerTexts();
  check(groups.length > 0, `grouped by status: ${groups.map((g) => g.replace(/\s+/g, " ")).join(", ")}`);
  await shot("pr-board");
  await a11y("PR board (dark)");
  await page.emulateMedia({ colorScheme: "light" });
  await sleep(500);
  await shot("pr-board-light");
  await a11y("PR board (light)");
  check((await page.locator("[data-slot='connection'][data-connected='true']").count()) === 1, "the sidebar shows Connected");

  // Hot edit: change the plugin file; the page picks it up.
  const entry = path.join(pluginDir, "index.js");
  writeFileSync(entry, readFileSync(entry, "utf8").replaceAll('"PR board"', '"PR board (edited)"'));
  await nav("PR board (edited)").waitFor({ timeout: 10000 });
  check(true, "editing the plugin file reloads it live");

  // --- per-area checks: scripts/verify/*.mjs, one file per area -------------------------------
  // Each file default-exports `async ({ page, shot, a11y, check, sleep, PORT, DATA, OUT })`
  // and opens its own pages. Optional filter: NEKO_WEB_ONLY=tasks,prs runs matching files only.
  const only = (process.env.NEKO_WEB_ONLY ?? "").split(",").filter(Boolean);
  const dir = path.join(repo, "scripts/verify");
  for (const file of existsSync(dir) ? readdirSync(dir).filter((f) => f.endsWith(".mjs")).sort() : []) {
    if (only.length && !only.some((name) => file.startsWith(name))) continue;
    console.log(`\n== ${file}`);
    const area = (await import(pathToFileURL(path.join(dir, file)).href)).default;
    try {
      await area({ page, shot, a11y, check, sleep, PORT, DATA, OUT });
    } catch (error) {
      // An area that throws stops the run; say what the page logged first.
      if (problems.length) console.error(`console/network problems so far:\n  ${problems.join("\n  ")}`);
      throw error;
    }
  }

  check(problems.length === 0, problems.length ? `console/network problems:\n  ${problems.join("\n  ")}` : "no console errors or failed requests");
} catch (error) {
  failures.push(String(error?.stack ?? error));
  console.error(error);
} finally {
  try { await context?.close(); } catch {}
  try { await browser?.close(); } catch {}
  if (leased) try { execFileSync(sharedChrome, ["release", LEASE], { stdio: "inherit" }); } catch {}
  stopDaemon(daemon);
}

console.log(failures.length ? `\n${failures.length} failed` : `\nall passed; screenshots in ${OUT}`);
process.exit(failures.length ? 1 : 0);
