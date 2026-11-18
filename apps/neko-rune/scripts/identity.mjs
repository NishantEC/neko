// Visual identity of the Rune client against the web UI, measured (docs/knowledge/rune-identity.md). Run by `identity.sh`.
//
// For every pair in `identity-pairs.json`: Rune's content region headless (`--snap` + `--tree`, 2x), the web's page in embed mode in
// the shared headless Chrome at that region's exact size and scale, a pixelmatch diff (ISC) of the two, and a side-by-side picture.
// Writes REPORT.md and results.json in $OUT (default /tmp/neko-rune-identity; real-data pictures are never committed).
//
// Env: RUNE_BIN, DATA (the harness data folder), PORT (the harness web port), OUT, TOOLS (a folder with node_modules of
// pngjs, pixelmatch and playwright-core), ONLY (a comma list of pair names), THEME (dark|light).
import { createRequire } from "node:module";
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const OUT = process.env.OUT ?? "/tmp/neko-rune-identity";
const TOOLS = process.env.TOOLS ?? `${OUT}/tools`;
const DATA = process.env.DATA ?? `${OUT}/data`;
const PORT = process.env.PORT ?? "4771";
const THEME = process.env.THEME ?? "dark";
const RUNE = process.env.RUNE_BIN ?? `${process.env.HOME}/Library/Caches/neko/rune-target/debug/neko-rune`;
const ONLY = process.env.ONLY ? new Set(process.env.ONLY.split(",")) : null;
const THRESHOLD = 2.5;
const S = 2;

const require = createRequire(`${TOOLS}/package.json`);
const { PNG } = require("pngjs");
const pixelmatch = require("pixelmatch").default ?? require("pixelmatch");
const { chromium } = require("playwright-core");

const pairs = JSON.parse(fs.readFileSync(path.join(here, "identity-pairs.json"), "utf8")).filter((p) => !ONLY || ONLY.has(p.name));
fs.mkdirSync(`${OUT}/pairs`, { recursive: true });
const size = process.env.WINDOW ?? "1440x900";

/** Rune's half: the picture and the tree of one state. */
function rune(pair) {
  const env = { ...process.env, NEKO_RUNE_SOCKET: `${DATA}/neko.sock`, NEKO_DATA_DIR: DATA, NEKO_RUNE_ACCOUNT: "1", ...(pair.rune ?? {}) };
  const png = `${OUT}/pairs/${pair.name}-rune.png`;
  const tree = `${OUT}/pairs/${pair.name}-rune.tree.json`;
  for (const [flag, file] of [["--snap", png], ["--tree", tree]]) {
    const args = [flag, file, "--scene", "default", "--theme", THEME, "--size", size];
    if (flag === "--snap") args.push("--scale", String(S));
    const r = spawnSync(RUNE, args, { env, encoding: "utf8" });
    if (r.status !== 0) throw new Error(`rune ${flag} ${pair.name}: ${r.stderr.slice(-400)}`);
  }
  const t = JSON.parse(fs.readFileSync(tree, "utf8"));
  const els = t.elements;
  const find = (k) => els.find((e) => e.key === k);
  const region = find(pair.region ?? "page-scroll");
  if (!region) throw new Error(`${pair.name}: no element keyed ${pair.region ?? "page-scroll"} in Rune's tree`);
  let rect = region.rect.slice();
  // The part inside the window (the card's bottom edge is the window's).
  const [vw, vh] = t.viewport;
  rect[2] = Math.min(rect[2], vw - rect[0]);
  rect[3] = Math.min(rect[3], vh - rect[1]);
  if (pair.crop) rect = [rect[0] + pair.crop[0], rect[1] + pair.crop[1], pair.crop[2] ?? rect[2] - pair.crop[0], pair.crop[3] ?? rect[3] - pair.crop[1]];
  // Words and numbers that change between two runs are masked: elements Rune names (by key prefix) or any text matching a pattern.
  const masks = [];
  const patterns = [/\b\d+\s*(s|sec|seconds?|m|min|minutes?|h|hours?|d|days?)\b.*\bago\b/i, /\bago\b/i, /^\d{1,2}:\d{2}/, /\bjust now\b/i, /^Synced /i].concat((pair.maskText ?? []).map((p) => new RegExp(p)));
  for (const e of els) {
    if (!e.text || !patterns.some((p) => p.test(e.text))) continue;
    const [x, y, w, h] = e.rect;
    if (x >= rect[0] && y >= rect[1] && x + w <= rect[0] + rect[2] + 1 && y + h <= rect[1] + rect[3] + 1) masks.push([x - rect[0], y - rect[1], w, h]);
  }
  return { png, rect, masks };
}

function crop(img, x, y, W, H) {
  const out = new PNG({ width: W, height: H });
  PNG.bitblt(img, out, Math.round(x * S), Math.round(y * S), Math.min(W, img.width - Math.round(x * S)), Math.min(H, img.height - Math.round(y * S)), 0, 0);
  return out;
}

function paint(img, masks, W, H) {
  for (const [mx, my, mw, mh] of masks) {
    for (let y = Math.max(0, Math.floor(my * S)); y < Math.min(H, Math.ceil((my + mh) * S)); y++) {
      for (let x = Math.max(0, Math.floor(mx * S)); x < Math.min(W, Math.ceil((mx + mw) * S)); x++) {
        const i = (y * W + x) * 4;
        img.data[i] = 255; img.data[i + 1] = 0; img.data[i + 2] = 255; img.data[i + 3] = 255;
      }
    }
  }
}

const token = fs.readFileSync(`${DATA}/web-token`, "utf8").trim();
const browser = await chromium.connectOverCDP("http://127.0.0.1:9333");
const context = await browser.newContext({ viewport: { width: 1440, height: 900 }, deviceScaleFactor: S, colorScheme: THEME });
const results = [];
try {
  const page = await context.newPage();
  await page.goto(`http://127.0.0.1:${PORT}/?token=${token}#/inbox?embed=1&theme=${THEME}`, { waitUntil: "load" });
  await page.waitForTimeout(1500);
  for (const pair of pairs) {
    const row = { name: pair.name, note: pair.note ?? "" };
    try {
      const r = rune(pair);
      const [, , rw, rh] = r.rect;
      const W = Math.round(rw * S), H = Math.round(rh * S);
      await page.setViewportSize({ width: Math.round(rw), height: Math.round(rh) });
      for (const [k, v] of Object.entries(pair.storage ?? {})) await page.evaluate(([a, b]) => localStorage.setItem(a, b), [k, v]);
      const route = pair.web;
      await page.goto(`http://127.0.0.1:${PORT}/?token=${token}#/${route}${route.includes("?") ? "&" : "?"}embed=1&theme=${THEME}`, { waitUntil: "load" });
      await page.reload({ waitUntil: "load" });
      await page.waitForTimeout(pair.wait ?? 3000);
      for (const step of pair.steps ?? []) {
        if (step.click) await page.getByText(step.click, { exact: true }).first().click({ timeout: 4000 }).catch(() => {});
        if (step.clickRole) await page.getByRole(step.clickRole[0], { name: step.clickRole[1] }).first().click({ timeout: 4000 }).catch(() => {});
        await page.waitForTimeout(step.wait ?? 600);
      }
      const webPath = `${OUT}/pairs/${pair.name}-web.png`;
      await page.screenshot({ path: webPath });
      // Where the web puts each text (its words, rect in points), for the second measure below.
      const webTexts = await page.evaluate(() => {
        const found = [];
        const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
        for (let n = walker.nextNode(); n; n = walker.nextNode()) {
          const words = n.textContent.replace(/\s+/g, " ").trim();
          const el = n.parentElement;
          if (!words || getComputedStyle(el).visibility === "hidden") continue;
          const range = document.createRange();
          range.selectNodeContents(n);
          const b = range.getBoundingClientRect();
          if (b.width >= 1 && b.height >= 1) found.push({ text: words, x: b.x, y: b.y + b.height / 2, w: b.width });
        }
        return found;
      });
      const a = crop(PNG.sync.read(fs.readFileSync(r.png)), r.rect[0], r.rect[1], W, H);
      const b = crop(PNG.sync.read(fs.readFileSync(webPath)), 0, 0, W, H);
      paint(a, r.masks, W, H);
      paint(b, r.masks, W, H);
      const diff = new PNG({ width: W, height: H });
      const n = pixelmatch(a.data, b.data, diff.data, W, H, { threshold: 0.1, includeAA: false, diffColor: [255, 60, 60], alpha: 0.3 });
      const sheet = new PNG({ width: W * 3 + 16, height: H });
      for (let i = 0; i < sheet.data.length; i += 4) { sheet.data[i] = 70; sheet.data[i + 1] = 70; sheet.data[i + 2] = 70; sheet.data[i + 3] = 255; }
      PNG.bitblt(a, sheet, 0, 0, W, H, 0, 0);
      PNG.bitblt(b, sheet, 0, 0, W, H, W + 8, 0);
      PNG.bitblt(diff, sheet, 0, 0, W, H, 2 * W + 16, 0);
      const pairPath = `${OUT}/pairs/${pair.name}-pair.png`;
      fs.writeFileSync(pairPath, PNG.sync.write(sheet));
      // The second measure: of the words the web shows, how many Rune shows within 2 points of the same place (left edge and vertical centre).
      // Pixels alone pass a sparse page whose rows are all in the wrong place; this does not.
      const runeTexts = JSON.parse(fs.readFileSync(`${OUT}/pairs/${pair.name}-rune.tree.json`, "utf8")).elements.filter((e) => (e.role === "text" || e.role === "heading") && e.text?.trim());
      let placed = 0;
      const used = new Set();
      // Only the words the picture shows: a long page's text below the window's bottom edge is in neither screenshot.
      const visibleTexts = webTexts.filter((w) => w.y < rh);
      for (const w of visibleTexts) {
        const i = runeTexts.findIndex((e, k) => !used.has(k) && e.text.replace(/\s+/g, " ").trim() === w.text && Math.abs(e.rect[0] - r.rect[0] - w.x) <= 2 && Math.abs(e.rect[1] + e.rect[3] / 2 - r.rect[1] - w.y) <= 2.5);
        if (i >= 0) { used.add(i); placed++; }
      }
      const placedPct = visibleTexts.length ? Math.round((1000 * placed) / visibleTexts.length) / 10 : 100;
      Object.assign(row, { placed: placedPct, words: visibleTexts.length, pct: Math.round((10000 * n) / (W * H)) / 100, size: `${Math.round(rw)}x${Math.round(rh)}`, masks: r.masks.length, pair: pairPath });
    } catch (e) {
      row.error = String(e.message ?? e).slice(0, 200);
    }
    results.push(row);
    console.log(row.error ? `ERR  ${row.name}: ${row.error}` : `${row.pct.toFixed(2).padStart(6)}%  words ${String(row.placed).padStart(5)}%  ${row.name}`);
  }
} finally {
  await context.close();
  await browser.close();
}

const ok = results.filter((r) => r.pct !== undefined);
const pass = ok.filter((r) => r.pct <= THRESHOLD);
const lines = [`# Identity report (${THEME}, ${size} window, 2x)`, "", `Threshold ${THRESHOLD}% differing pixels per pair (pixelmatch threshold 0.1, anti-aliasing ignored). ${pass.length} of ${results.length} pairs pass; ${results.length - ok.length} could not be rendered.`, "", "| Pair | Differing | Words placed | Size | Masked | Side by side |", "| --- | ---: | ---: | --- | ---: | --- |"];
for (const r of [...results].sort((a, b) => (b.pct ?? 999) - (a.pct ?? 999))) {
  lines.push(r.error ? `| ${r.name} | error: ${r.error.replace(/\|/g, "/")} | | | | |` : `| ${r.name} | ${r.pct.toFixed(2)}% ${r.pct <= THRESHOLD ? "" : "FAIL"} | ${r.placed}% of ${r.words} | ${r.size} | ${r.masks} | ${r.pair} |`);
}
fs.writeFileSync(`${OUT}/REPORT.md`, lines.join("\n") + "\n");
fs.writeFileSync(`${OUT}/results.json`, JSON.stringify(results, null, 1));
console.log(`${pass.length}/${results.length} pass; report: ${OUT}/REPORT.md`);
