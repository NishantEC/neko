// Embed mode (`?embed=1`, web/src/shell/embed.ts): the web UI as a piece of the Rune client's panel.
// An artifact route and a plugin page render only their content, in a transparent page that follows the
// host's theme, with no tab strip, rail, sidebar or status chrome; the same routes without the flag are
// the app they always were.
//
//   NEKO_WEB_ONLY=embed node scripts/web-verify.mjs
import { cpSync, existsSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const repo = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");

const PAGE = `<!doctype html><html><body style="font-family:sans-serif;margin:0;padding:24px">
<h1 data-neko-id="title">Embedded page</h1>
<button id="offer" data-neko-send="Make the heading blue">Ask for blue</button>
</body></html>`;

const SKETCH = JSON.stringify({
  type: "excalidraw", version: 2, source: "neko-verify",
  elements: [{ type: "rectangle", id: "box-1", x: 80, y: 80, width: 220, height: 110, angle: 0, strokeColor: "#1e1e1e", backgroundColor: "#a5d8ff", fillStyle: "solid", strokeWidth: 2, strokeStyle: "solid", roughness: 1, opacity: 100, groupIds: [], frameId: null, roundness: { type: 3 }, seed: 11, version: 1, versionNonce: 1, isDeleted: false, boundElements: null, updated: 1, link: null, locked: false }],
  appState: { viewBackgroundColor: "#ffffff" },
  files: {},
});

export default async function ({ page, shot, a11y, check, sleep, PORT, DATA }) {
  const base = `http://127.0.0.1:${PORT}`;
  await page.goto(`${base}/#/`);
  const request = (body) => page.evaluate(async (body) => (await fetch("/api/request", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(body) })).json(), body);
  const save = async (name, body, title) => {
    const file = path.join(DATA, name);
    writeFileSync(file, body);
    const reply = await request({ SaveArtifact: { title, file, workspace_id: null, artifact_id: null, note: "", show: false } });
    return reply?.ArtifactSaved?.artifact;
  };
  const html = await save("embed-page.html", PAGE, "Embedded page");
  const sketch = await save("embed-board.excalidraw", SKETCH, "Embedded sketch");
  check(!!html && !!sketch, "an html artifact and a sketch were kept");

  // An artifact route in embed mode: only the artifact.
  await page.goto(`${base}/#/artifact/${html.id}/1?embed=1&theme=dark`);
  await page.reload();
  await page.waitForSelector("[data-slot='embed']", { timeout: 15000 });
  await page.waitForSelector(`[data-slot='artifact-panel'][data-artifact='${html.id}'] iframe`, { timeout: 15000 })
    .then(() => check(true, "an html artifact is drawn in its viewer in embed mode"), () => check(false, "an html artifact is drawn in its viewer in embed mode"));
  const chrome = await page.evaluate(() => ({
    tabs: document.querySelectorAll("[data-slot='tab-strip']").length,
    sidebar: document.querySelectorAll("[data-slot='sidebar'], [data-slot='sidebar-wrapper'], [data-slot='status-bar'], [data-slot='workspace-switcher']").length,
    heading: document.querySelectorAll("[data-slot='artifact-tab']").length,
    body: getComputedStyle(document.body).backgroundColor,
    html: getComputedStyle(document.documentElement).backgroundColor,
    root: getComputedStyle(document.getElementById("root")).backgroundColor,
    flag: document.documentElement.hasAttribute("data-embed"),
  }));
  check(chrome.tabs === 0 && chrome.sidebar === 0, `no tab strip, rail, sidebar or status chrome (${JSON.stringify({ tabs: chrome.tabs, sidebar: chrome.sidebar })})`);
  check(chrome.heading === 0, "no tab header around the artifact");
  check(chrome.flag && chrome.body === "rgba(0, 0, 0, 0)" && chrome.html === "rgba(0, 0, 0, 0)" && chrome.root === "rgba(0, 0, 0, 0)", `the page background is transparent (${chrome.body})`);
  check(await page.evaluate(() => document.documentElement.classList.contains("dark") && document.documentElement.style.colorScheme === "dark"), "theme=dark paints the dark theme");
  await sleep(500);
  await shot("embed-artifact-html-dark");
  // The host changes the appearance by changing the address; the page follows without reloading.
  await page.evaluate(() => { location.hash = location.hash.replace("theme=dark", "theme=light"); });
  await page.waitForFunction(() => document.documentElement.classList.contains("light") && !document.documentElement.classList.contains("dark"), null, { timeout: 5000 })
    .then(() => check(true, "changing theme in the address changes the page's theme"), () => check(false, "changing theme in the address changes the page's theme"));
  await shot("embed-artifact-html-light");
  await a11y("embedded html artifact");
  check((await page.getByRole("link", { name: "Open in browser" }).count()) === 0, "no \"Open in browser\" link (it would open without the sign-in)");

  // Comment mode and data-neko-send work: they dock a composer, and nothing is sent.
  const frame = page.frameLocator(`[data-slot='artifact-panel'][data-artifact='${html.id}'] iframe`);
  await page.getByRole("button", { name: "Comment" }).click();
  await page.getByText("Click an element in the design to comment on it").waitFor({ timeout: 5000 })
    .then(() => check(true, "Comment mode starts in embed mode"), () => check(false, "Comment mode starts in embed mode"));
  await frame.locator("h1").click({ timeout: 5000 }).catch(() => {});
  await page.locator("[data-slot='embed-composer']").waitFor({ timeout: 8000 })
    .then(() => check(true, "an element picked in comment mode docks the composer with its reference"), () => check(false, "an element picked in comment mode docks the composer with its reference"));
  const hashBefore = await page.evaluate(() => location.hash);
  await frame.locator("#offer").click({ timeout: 5000 }).catch(() => {});
  await sleep(600);
  check((await page.evaluate(() => location.hash)) === hashBefore, "data-neko-send does not carry the embedded page away from its route");
  await shot("embed-artifact-comment");

  // A sketch, and an unknown artifact in embed mode.
  await page.goto(`${base}/#/artifact/${sketch.id}/1?embed=1&theme=dark`);
  await page.waitForSelector(`[data-slot='artifact-panel'][data-artifact='${sketch.id}'] [data-slot='sketch-board'] canvas`, { timeout: 20000 })
    .then(() => check(true, "an Excalidraw sketch is drawn in embed mode"), () => check(false, "an Excalidraw sketch is drawn in embed mode"));
  await sleep(800);
  await shot("embed-artifact-sketch");

  // A plugin page in embed mode: the page, alone.
  const pluginDir = path.join(DATA, "plugins/pr-board");
  if (!existsSync(pluginDir)) cpSync(path.join(repo, "web/plugins/pr-board"), pluginDir, { recursive: true });
  await page.goto(`${base}/#/pr-board:board?embed=1&theme=dark`);
  await page.waitForSelector("[data-slot='embed'] .pr-card", { timeout: 20000 })
    .then(() => check(true, "a plugin page is drawn in embed mode"), () => check(false, "a plugin page is drawn in embed mode"));
  check(await page.evaluate(() => document.querySelectorAll("[data-slot='tab-strip'], [data-slot='sidebar'], [data-slot='status-bar']").length === 0), "the plugin page has no tab strip, rail or sidebar");
  await shot("embed-plugin-page");
  await a11y("embedded plugin page");

  // The daemon learned the plugin's pages from this window (PluginPages), and the viewers it declares.
  await page.waitForFunction(async () => {
    const r = await (await fetch("/api/request", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify("PluginPages") })).json();
    return (r?.PluginPages?.pages ?? []).some((p) => p.id === "pr-board:board");
  }, null, { timeout: 8000 })
    .then(() => check(true, "the page reported the plugin's pages to the daemon (PluginPages)"), () => check(false, "the page reported the plugin's pages to the daemon (PluginPages)"));
  const viewers = await request("PluginViewers");
  check(Array.isArray(viewers?.PluginViewers?.viewers), "PluginViewers answers with a list");
  const signed = await request({ SignedWebUrl: { route: "tasks" } });
  check(!!signed?.Error && !/token=/.test(JSON.stringify(signed)), "the web bridge does not give a page the signed address");

  // Without the flag the same routes are the app: tab strip, header, the lot.
  await page.goto(`${base}/#/artifact?id=${html.id}&v=1`);
  await page.reload();
  await page.waitForSelector("[data-slot='artifact-tab']", { timeout: 15000 })
    .then(() => check(true, "without ?embed=1 the artifact route is its tab as before"), () => check(false, "without ?embed=1 the artifact route is its tab as before"));
  check(await page.evaluate(() => document.querySelectorAll("[data-slot='tab-strip']").length === 1 && !document.documentElement.hasAttribute("data-embed")), "and the tab strip is there");
}
