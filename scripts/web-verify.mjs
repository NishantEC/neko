#!/usr/bin/env node
// Self-verification loop for the web UI: starts a TEST daemon (never the real
// one), opens the app in the shared headless Chrome, screenshots Home and the
// PR-board plugin to /tmp/neko-web/, exercises the composer, the streaming
// reply and plugin hot-loading, checks the daemon's access rules, and exits
// non-zero on any console error or failed check.
//
//   pnpm --dir web build && node scripts/web-verify.mjs
//
// An agent can edit web/src, rebuild (`pnpm --dir web build`), run this script
// and read /tmp/neko-web/*.png: the whole edit-and-look loop without a human.
//
// Rules this script follows (see ~/.agents/BROWSER.md): it never launches a
// browser; it acquires the shared Chrome, uses its own context, closes it in
// `finally` and releases the lease.

import { execFileSync, spawn } from "node:child_process";
import { createRequire } from "node:module";
import { cpSync, existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import http from "node:http";
import net from "node:net";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const require = createRequire(import.meta.url);
const { chromium } = require(process.env.PLAYWRIGHT_CORE ?? "/Users/nish/Documents/loreswipe/node_modules/playwright-core");

const repo = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const PORT = Number(process.env.NEKO_WEB_PORT ?? 4748);
const DATA = process.env.NEKO_WEB_VERIFY_DATA ?? "/tmp/neko-web-data";
const OUT = "/tmp/neko-web";
const SEED = process.env.NEKO_WEB_SEED ?? "/tmp/neko-real-snap.json";
const TARGET = process.env.CARGO_TARGET_DIR ?? "/Users/nish/Documents/neko/target";
const HARNESS = process.env.NEKO_WEB_HARNESS ?? path.join(TARGET, "debug/web_harness");
const LEASE = "neko-web";
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
      ...(existsSync(SEED) ? { NEKO_WEB_SEED: SEED } : {}),
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
  context = await browser.newContext({ viewport: { width: 1440, height: 900 }, deviceScaleFactor: 1 });
  const page = await context.newPage();
  const problems = [];
  page.on("console", (m) => { if (m.type() === "error") problems.push(`console: ${m.text()}`); });
  page.on("pageerror", (e) => problems.push(`pageerror: ${e.message}`));
  page.on("requestfailed", (r) => { if (!r.url().includes("/api/events") && !r.url().includes("/api/request")) problems.push(`request failed: ${r.url()}`); });
  page.on("response", (r) => { if (r.status() >= 400) problems.push(`HTTP ${r.status()}: ${r.url()}`); });

  await page.goto(`http://127.0.0.1:${PORT}/?token=${token}`, { waitUntil: "domcontentloaded" });
  check(new URL(page.url()).search === "", "the token is gone from the URL after sign-in");
  await page.waitForSelector(".turn.user", { timeout: 20000 });
  await page.waitForSelector("[data-running='true']", { timeout: 10000 });
  await sleep(600);

  // Live turn: "Working for Ns" with the newest step, and the answer growing.
  const header = (await page.locator("[data-running='true'] .work-head").innerText()).trim();
  check(/^Working for \d+s/.test(header), `live work log reads "${header.replace(/\s+/g, " ")}"`);
  check((await page.locator("[data-running='true'] .work-row.live").count()) === 1, "the newest step is shown while working");
  const grow = async () => (await page.locator(".turn.neko[data-pending='true'] .prose").innerText().catch(() => "")).length;
  const before = await grow();
  // Finished paragraphs arrive as whole blocks, so wait for the next one.
  await page.waitForFunction((n) => (document.querySelector(".turn.neko[data-pending='true'] .prose")?.textContent ?? "").length > n, before, { timeout: 20000 }).catch(() => {});
  const after = await grow();
  check(after > before, `the pending reply streams (${before} -> ${after} characters)`);
  await page.screenshot({ path: path.join(OUT, "home-live.png") });

  // Finished turn: scroll to its log, open it.
  const finished = page.locator(".turn.neko[data-pending='false']").filter({ has: page.locator(".work") }).last();
  await finished.scrollIntoViewIfNeeded();
  const done = (await finished.locator(".work-head").innerText()).replace(/\s+/g, " ").trim();
  check(/^Worked for 1m 12s/.test(done), `finished work log reads "${done}"`);
  await finished.locator(".work-head").click();
  check((await finished.locator(".work-steps li").count()) > 0, "the work log expands to its steps");
  await finished.evaluate((el) => el.scrollIntoView({ block: "start" }));
  await page.screenshot({ path: path.join(OUT, "home-worklog.png") });
  check((await page.locator(".prose table").count()) > 0, "a GFM table renders");

  // Decision dock: the exact thing being approved, above a composer that stays usable.
  await page.evaluate(() => document.querySelector(".scroll").scrollTo({ top: 1e9 }));
  await page.waitForSelector(".decision", { timeout: 5000 });
  const decision = (await page.locator(".decision").innerText()).replace(/\s+/g, " ");
  check(/Tool approval/.test(decision) && /save_issue/.test(decision) && /Approve/.test(decision) && /Decline/.test(decision) && /1 of \d+/.test(decision), `a pending decision docks above the composer: "${decision.slice(0, 90)}…"`);
  check(await page.locator("textarea").isEnabled(), "the composer stays usable while a decision is pending");
  check((await page.locator("textarea").getAttribute("placeholder")).startsWith("Queue a message"), "the composer says it can queue");
  check((await page.locator(".send.stop").count()) === 1, "the send button is a stop square while a reply runs");
  await page.screenshot({ path: path.join(OUT, "home-decision.png") });

  // Queue: send while the reply runs (same SendMessage the native app sends; the test daemon never starts a model).
  const message = `web verify ${Date.now()}`;
  await page.locator("textarea").fill(message);
  check((await page.locator(".send:not(.stop)").count()) === 1, "typing turns the stop square back into a send arrow");
  await page.locator("textarea").press("Enter");
  await page.locator(".tray-row", { hasText: message }).waitFor({ timeout: 10000 });
  check(true, "a message sent mid-reply waits in the queue tray");
  check((await page.locator(".bubble", { hasText: message }).count()) === 0, "a queued message is not drawn in the transcript yet");
  await page.screenshot({ path: path.join(OUT, "home-queue.png") });
  await page.locator(".tray-row button[aria-label='Edit queued message']").click();
  await page.waitForFunction((t) => document.querySelector("textarea").value === t, message, { timeout: 5000 });
  check((await page.locator(".tray-row").count()) === 0, "editing takes the message back out of the queue into the composer");
  await page.locator("textarea").press("Enter");
  await page.locator(".tray-row", { hasText: message }).waitFor({ timeout: 10000 });
  await page.locator(".tray-row button[aria-label='Remove queued message']").click();
  await page.waitForFunction(() => document.querySelectorAll(".tray-row").length === 0, null, { timeout: 5000 });
  check(true, "a queued message can be removed");

  // Decline with the keyboard (composer empty): the next decision moves up.
  await page.locator("textarea").press("Escape");
  await page.waitForFunction(() => /Tell its agent/.test(document.querySelector(".decision")?.textContent ?? ""), null, { timeout: 5000 });
  check(true, "Esc declines the card in front and the next decision appears");
  await page.screenshot({ path: path.join(OUT, "home-decision-tool.png") });

  // Scroll to end pill: only when scrolled away from the bottom.
  check((await page.locator(".to-end").count()) === 0, "no scroll-to-end pill while at the bottom");
  await page.evaluate(() => document.querySelector(".scroll").scrollTo({ top: 0 }));
  await page.locator(".to-end").waitFor({ timeout: 3000 });
  await page.screenshot({ path: path.join(OUT, "home-top.png") });
  await page.locator(".to-end").click();
  await page.waitForFunction(() => { const e = document.querySelector(".scroll"); return e.scrollHeight - e.scrollTop - e.clientHeight < 90; }, null, { timeout: 5000 });
  check(true, "the pill returns to the end of the conversation");
  check((await page.locator(".table-foot button").count()) > 0, "each table has a copy button");

  // Plugin: installed while the app is open, no rebuild.
  const pluginDir = path.join(DATA, "plugins/pr-board");
  check((await page.locator(".nav", { hasText: "PR board" }).count()) === 0, "no plugin page before the plugin is installed");
  cpSync(path.join(repo, "web/plugins/pr-board"), pluginDir, { recursive: true });
  await page.locator(".nav", { hasText: "PR board" }).waitFor({ timeout: 10000 });
  check(true, "the plugin appears in the sidebar without rebuilding the app");
  await page.locator(".nav", { hasText: "PR board" }).click();
  await page.waitForSelector(".pr-card", { timeout: 10000 });
  const cards = await page.locator(".pr-card").count();
  check(cards > 0, `the PR board lists ${cards} pull requests`);
  const groups = await page.locator(".pr-board h2").allInnerTexts();
  check(groups.length > 0, `grouped by status: ${groups.map((g) => g.replace(/\s+/g, " ")).join(", ")}`);
  await page.screenshot({ path: path.join(OUT, "pr-board.png") });

  // Hot edit: change the plugin file; the page picks it up.
  const entry = path.join(pluginDir, "index.js");
  writeFileSync(entry, readFileSync(entry, "utf8").replaceAll('"PR board"', '"PR board (edited)"'));
  await page.locator(".nav", { hasText: "PR board (edited)" }).waitFor({ timeout: 10000 });
  check(true, "editing the plugin file reloads it live");

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
