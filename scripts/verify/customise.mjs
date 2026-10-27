// One registry, one host: a page of yours replacing a built-in (and being reset),
// wrapping one, host.storage, host.addSection, custom.css applied live, the
// Customisations page, and the Roll back button. Run alone with
// NEKO_WEB_ONLY=customise (see scripts/verify/README.md).
//
// Everything reaches the real test daemon except RollBackWebUi, which is
// captured so the check does not move the test daemon's web link.
import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import path from "node:path";

export default async function ({ page, shot, a11y, check, sleep, PORT, DATA }) {
  const base = `http://127.0.0.1:${PORT}`;
  const plugins = path.join(DATA, "plugins");
  const css = path.join(DATA, "custom.css");
  const storageDir = path.join(DATA, "plugin-data");
  const clean = () => {
    rmSync(plugins, { recursive: true, force: true });
    rmSync(css, { force: true });
    rmSync(storageDir, { recursive: true, force: true });
  };
  clean();
  const write = (name, manifest, source) => {
    mkdirSync(path.join(plugins, name), { recursive: true });
    writeFileSync(path.join(plugins, name, "manifest.json"), JSON.stringify({ entry: "index.js", ...manifest }));
    writeFileSync(path.join(plugins, name, "index.js"), source);
  };
  const nav = (text) => page.locator("[data-slot='nav-item']", { hasText: text });

  // ---- the redirect, and the sidebar without a Plugins group ------------------------------
  await page.goto(`${base}/#/plugins`);
  await page.waitForSelector("[data-slot='plugins-page']", { timeout: 10000 });
  check(await page.evaluate(() => location.hash) === "#/customisations", "#/plugins redirects to #/customisations");
  check(/Customisations/.test(await page.locator("[data-slot='plugins-page'] h1").first().innerText()), "the page is titled Customisations");
  check((await nav("Customisations").count()) === 1 && (await nav("Plugins").count()) === 0, "the sidebar lists Customisations and no Plugins entry");
  const labels = await page.locator("aside, [data-slot='sidebar']").first().innerText();
  check(!/^\s*Plugins\s*$/m.test(labels), "there is no Plugins group in the sidebar");

  // ---- user pages in the normal groups, marked "yours" ------------------------------------
  write("mine", { title: "Mine", description: "Overrides Usage; checks storage", version: "1.0.0", group: "work" }, `
export default function register(host) {
  const { React, storage, ui } = host;
  const h = React.createElement;
  function Usage() {
    return h("div", { "data-testid": "my-usage", style: { padding: 24 } }, "MY OWN USAGE PAGE");
  }
  function Notes() {
    const [state, setState] = React.useState("working");
    React.useEffect(() => {
      (async () => {
        await storage.set("note", { n: 41, text: "kept by the daemon" });
        const back = await storage.get("note");
        const missing = await storage.get("nothing", "fallback");
        const keys = await storage.list();
        await storage.set("gone", 1);
        await storage.delete("gone");
        setState("storage:" + back.n + ":" + missing + ":" + keys.join(","));
      })().catch((e) => setState("storage-error:" + e.message));
    }, []);
    return h("div", { style: { padding: 24 }, "data-testid": "notes" }, state);
  }
  host.registerPage({ id: "usage", component: Usage });
  host.registerPage({ id: "notes", title: "My notes", icon: "Heart", component: Notes });
  host.addSection({ id: "hello", title: "Hello section", component: () => h("div", { "data-testid": "hello-section" }, "section from a page") });
}`);
  await nav("My notes").waitFor({ timeout: 12000 });
  check(!(await nav("My notes").innerText()).includes("yours"), "a page of yours sits in the normal sidebar, unmarked: one registry");
  const workGroup = await page.locator("[data-slot='nav-item']").evaluateAll((els) => els.map((e) => e.textContent.replace(/\s+/g, " ").trim()));
  check(workGroup.indexOf("My notes") > workGroup.findIndex((t) => /^Watching/.test(t)) && workGroup.indexOf("My notes") < workGroup.findIndex((t) => /^Settings/.test(t)), `the manifest's group puts it with the Work pages: ${workGroup.join(" | ")}`);

  // ---- host.storage round trip -------------------------------------------------------------
  await nav("My notes").click();
  await page.waitForFunction(() => /^storage:41:fallback:/.test(document.querySelector("[data-testid='notes']")?.textContent ?? ""), null, { timeout: 8000 });
  const stored = path.join(storageDir, "mine/note.json");
  check(existsSync(stored) && JSON.parse(readFileSync(stored, "utf8")).n === 41, "host.storage.set kept the value as a file under plugin-data/<id>/");
  check(!existsSync(path.join(storageDir, "mine/gone.json")), "host.storage.delete removed its file");
  check((await page.locator("[data-testid='notes']").innerText()).endsWith(":note"), "host.storage.get(fallback) and list() agree with what was stored");
  await page.reload();
  await page.waitForFunction(() => /^storage:41:/.test(document.querySelector("[data-testid='notes']")?.textContent ?? ""), null, { timeout: 8000 });
  check(true, "the value is read back after a full reload (kept by the daemon, not the browser)");
  const bad = (await page.request.put(`${base}/api/storage/mine/..%2Fx`, { data: "1", headers: { "content-type": "application/json" } })).status();
  check(bad === 400, "a storage key that tries to leave its folder is refused (400)");

  // ---- a user page with a built-in's id replaces it, visibly, and can be reset --------------
  await nav("Usage").waitFor();
  check(!(await nav("Usage").innerText()).includes("Customised"), "the replaced built-in keeps its plain sidebar row (the page itself says Customised)");
  await page.goto(`${base}/#/usage`);
  await page.locator("[data-testid='my-usage']").waitFor({ timeout: 8000 });
  await page.locator("[data-slot='customised-strip']").waitFor();
  check(/Customised/.test(await page.locator("[data-slot='customised-strip']").innerText()), "the replacing page shows a Customised strip");
  await shot("customised-strip-dark");
  await a11y("customised page");
  await page.getByRole("button", { name: /Reset to original/ }).first().click();
  await page.waitForFunction(() => !document.querySelector("[data-testid='my-usage']"), null, { timeout: 6000 });
  check((await page.locator("[data-slot='customised-strip']").count()) === 0, "Reset to original brings the built-in page back and the strip goes");
  check(existsSync(path.join(storageDir, "_neko/overrides-off.json")) && JSON.parse(readFileSync(path.join(storageDir, "_neko/overrides-off.json"), "utf8")).includes("usage"), "the reset is kept by the daemon");
  await page.reload();
  await sleep(1500);
  check((await page.locator("[data-testid='my-usage']").count()) === 0 && !(await nav("Usage").innerText()).includes("Customised"), "the reset survives a reload; the original is still shown");

  // ---- the Customisations page lists it, and can switch it back on --------------------------
  await page.goto(`${base}/#/customisations`);
  const row = page.locator("[data-slot='plugin-row'][data-plugin='mine']");
  await row.waitFor({ timeout: 8000 });
  const override = row.locator("[data-slot='override']");
  check((await override.getAttribute("data-off")) === "true" && /Original Usage/.test(await override.innerText()), "the page lists the override as reset");
  check(/My notes/.test(await row.innerText()), "the page lists the pages of yours too");
  await shot("customisations-dark");
  await a11y("customisations");
  await override.getByRole("button", { name: /Use my version/ }).click();
  await page.waitForFunction(() => document.querySelector("[data-slot='override']")?.getAttribute("data-off") === "false", null, { timeout: 6000 });
  await page.goto(`${base}/#/usage`);
  await page.locator("[data-testid='my-usage']").waitFor({ timeout: 8000 });
  check(true, "Use my version switches the override back on");

  // ---- wrap(Original) ------------------------------------------------------------------------
  write("wrapper", { title: "Wrapper", version: "0.1.0" }, `
export default function register(host) {
  const h = host.React.createElement;
  host.registerPage({ id: "memory", wrap: (Original) => function Wrapped(props) {
    return h(host.React.Fragment, null, h("div", { "data-testid": "wrap-banner", style: { padding: 8 } }, "WRAPPED BY ME"), h("div", { className: "flex min-h-0 flex-1 flex-col" }, h(Original, props)));
  } });
}`);
  await page.goto(`${base}/#/memory`);
  await page.locator("[data-testid='wrap-banner']").waitFor({ timeout: 12000 });
  await page.locator("[data-slot='customised-strip']").waitFor();
  check((await page.locator("[data-slot='page'] h1").count()) > 0 || (await page.locator("[data-slot='page']").innerText()).length > 80, "wrap() renders the original page inside the wrapper");

  // ---- addSection --------------------------------------------------------------------------
  await page.goto(`${base}/#/settings`);
  await page.waitForSelector("[data-slot='settings-nav']", { timeout: 10000 });
  await page.locator("[data-slot='settings-nav'] button", { hasText: "Hello section" }).click();
  await page.locator("[data-testid='hello-section']").waitFor({ timeout: 4000 });
  check(true, "host.addSection adds a section to Settings, in its rail");

  // ---- Roll back the web UI ------------------------------------------------------------------
  const captured = [];
  await page.route("**/api/request", async (route) => {
    const body = route.request().postData() ?? "";
    if (body.includes('"RollBackWebUi"')) {
      captured.push(body);
      return route.fulfill({ json: { Error: { message: "verify: RollBackWebUi was captured, not sent" } } });
    }
    return route.continue();
  });
  await page.locator("[data-slot='settings-nav'] button", { hasText: "Neko updates" }).click();
  await page.locator("#setting-roll-back-web").waitFor({ timeout: 6000 });
  await page.locator("#setting-roll-back-web").getByRole("button", { name: /Roll back/ }).click();
  await page.locator("[data-slot='confirm-action']").click();
  await sleep(500);
  check(captured.length === 1 && captured[0].includes("RollBackWebUi"), "Settings, Neko updates has Roll back the web UI and it sends RollBackWebUi after a confirmation");
  await shot("settings-rollback-dark");
  await page.unroute("**/api/request");

  // ---- custom.css, applied live ------------------------------------------------------------
  const outline = () => page.evaluate(() => getComputedStyle(document.body).outlineWidth);
  const resting = await outline();
  check(resting !== "7px" && resting !== "9px", `no custom.css: nothing changed (outline ${resting})`);
  writeFileSync(css, "body { outline: 7px solid rgb(1, 2, 3); }");
  await page.waitForFunction(() => getComputedStyle(document.body).outlineWidth === "7px", null, { timeout: 10000 });
  check(true, "custom.css is applied live, without reloading");
  writeFileSync(css, "body { outline: 9px solid rgb(1, 2, 3); }");
  await page.waitForFunction(() => getComputedStyle(document.body).outlineWidth === "9px", null, { timeout: 10000 });
  check(true, "editing custom.css updates it live");
  const linkLast = await page.evaluate(() => {
    const links = [...document.head.querySelectorAll("link[rel=stylesheet]")];
    return links.at(-1)?.id === "neko-custom-css";
  });
  check(linkLast, "the stylesheet link comes after the app's own, so it wins");
  writeFileSync(css, ":root, .dark { --background: rgb(10, 40, 20); } body { background: var(--background); }");
  await page.waitForFunction(() => getComputedStyle(document.body).backgroundColor === "rgb(10, 40, 20)", null, { timeout: 10000 });
  check(true, "the app's CSS variables can be restyled from custom.css");
  await shot("custom-css-dark");
  rmSync(css, { force: true });
  await page.waitForFunction(() => getComputedStyle(document.body).outlineWidth !== "9px" && getComputedStyle(document.body).backgroundColor !== "rgb(10, 40, 20)", null, { timeout: 10000 });
  check(true, "removing custom.css puts the app back");

  // ---- clean up so the next area starts from nothing -----------------------------------------
  clean();
  await page.evaluate(() => localStorage.removeItem("neko.web.settings.section")); // Settings remembers its section
  await page.goto(`${base}/#/`);
  await sleep(4000);
}
