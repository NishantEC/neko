// Appearance: the mode, the light and dark themes, the themes a person (or Neko)
// adds as files, and the first paint. Run alone with NEKO_WEB_ONLY=appearance.
// Runs first (the files are read in order) because the legacy `neko-theme`
// migration has to meet a daemon that has never been given a choice.
import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import path from "node:path";

const OCEAN = { id: "ocean", name: "Ocean", appearance: "dark", family: "Mine", colors: { background: "#0b1f2a", foreground: "#d7e9f0", primary: "#3ab0d0", mutedForeground: "#9ab8c4" } };
const LOW = { id: "washed-out", name: "Washed Out", appearance: "light", colors: { background: "#ffffff", foreground: "#bbbbbb", primary: "#cccccc" } };
const NORD_MINE = { id: "nord", name: "Nord (mine)", appearance: "dark", colors: { background: "#101820", foreground: "#e6edf3", primary: "#ff8800" } };

export default async function ({ page, shot, a11y, check, sleep, PORT, DATA, OUT }) {
  const base = `http://127.0.0.1:${PORT}`;
  const themesDir = path.join(DATA, "themes");
  const file = (id) => path.join(themesDir, `${id}.json`);
  const write = (theme, id = theme.id) => {
    mkdirSync(themesDir, { recursive: true });
    writeFileSync(file(id), typeof theme === "string" ? theme : JSON.stringify(theme));
  };
  const root = (attr) => page.evaluate((a) => document.documentElement.getAttribute(a), attr);
  const token = (name) => page.evaluate((n) => getComputedStyle(document.documentElement).getPropertyValue(n).trim(), name);
  const theme = () => root("data-neko-theme");
  const waitTheme = (id, timeout = 4000) => page.waitForFunction((t) => document.documentElement.getAttribute("data-neko-theme") === t, id, { timeout });
  const daemon = (body) => page.evaluate(async (b) => (await fetch("/api/request", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(b) })).json(), body);
  const settle = () => sleep(350);
  const palette = async (text) => {
    await page.keyboard.press("Meta+k");
    await page.locator("[data-slot='command']").waitFor({ timeout: 3000 });
    await sleep(300);
    await page.keyboard.press("Meta+a");
    await page.keyboard.type(text);
    await sleep(300);
    await page.keyboard.press("Enter");
  };
  const card = (id) => page.locator(`[data-slot='theme-card'][data-theme-id='${id}']`);
  const open = async () => {
    await page.goto(`${base}/#/settings?section=appearance`);
    await page.waitForSelector("[data-slot='appearance']");
    await settle();
  };

  // ---- the old single setting becomes the new ones, once --------------------------------------
  await page.emulateMedia({ colorScheme: "dark" });
  await page.goto(`${base}/#/`, { waitUntil: "domcontentloaded" });
  await page.evaluate(() => { localStorage.removeItem("neko-appearance"); localStorage.removeItem("neko-theme-boot"); localStorage.setItem("neko-theme", "glass"); });
  await page.reload({ waitUntil: "domcontentloaded" });
  await waitTheme("glass");
  check(await page.evaluate(() => document.documentElement.hasAttribute("data-backdrop") && !document.documentElement.classList.contains("glass")), "the old value “glass” is migrated: Glass is a theme with a backdrop, not a class");
  const migrated = await page.evaluate(() => ({ appearance: JSON.parse(localStorage.getItem("neko-appearance")), old: localStorage.getItem("neko-theme") }));
  check(migrated.old === null && migrated.appearance.mode === "dark" && migrated.appearance.dark === "glass" && migrated.appearance.light === "neutral-light", "the migration keeps the dark choice, picks the default light, and removes neko-theme");
  await sleep(800);
  const seeded = (await daemon("GetAppearance")).Appearance;
  check(seeded?.stored === true && seeded.mode === "dark" && seeded.dark === "glass", "the first browser hands its old choice to the daemon");
  const wash = await page.evaluate(() => getComputedStyle(document.body).backgroundImage);
  check(/gradient/.test(wash), "a theme with a backdrop paints its wash behind the page");
  await daemon({ SetAppearance: { mode: "system", light: "neutral-light", dark: "neutral" } });

  // ---- the daemon is the truth: a browser with nothing stored adopts it ------------------------
  await daemon({ SetAppearance: { mode: "system", light: "gruvbox-light", dark: "gruvbox-dark" } });
  await page.evaluate(() => { localStorage.removeItem("neko-appearance"); localStorage.removeItem("neko-theme-boot"); });
  await page.reload({ waitUntil: "domcontentloaded" });
  await waitTheme("gruvbox-dark");
  check(true, "a browser with nothing stored shows the daemon's dark theme (Gruvbox Dark)");

  // ---- light and dark are chosen separately, and System follows the Mac ------------------------
  await open();
  check((await page.locator("[data-slot='theme-card'][data-theme-id]").count()) === 12 && (await page.locator("[role='radiogroup'][aria-label='Light theme'] [role='radio']").count()) === 4 && (await page.locator("[role='radiogroup'][aria-label='Dark theme'] [role='radio']").count()) === 8, "Appearance lists 4 light and 8 dark themes");
  await card("catppuccin-latte").click();
  await card("tokyo-night").click();
  await settle();
  check((await theme()) === "tokyo-night", "System on a dark Mac shows the dark pick (Tokyo Night) after picking both");
  check((await token("--background")) === "#24283b", `the page colour is Tokyo Night's (${await token("--background")})`);
  await page.emulateMedia({ colorScheme: "light" });
  await waitTheme("catppuccin-latte");
  check((await token("--background")) === "#eff1f5" && (await root("data-appearance")) === "light" && (await page.evaluate(() => document.documentElement.classList.contains("light"))), "the same setting on a light Mac shows the light pick (Catppuccin Latte)");
  await settle();
  await shot("appearance-light");
  await a11y("Appearance (light)");
  const stored = (await daemon("GetAppearance")).Appearance;
  check(stored.light === "catppuccin-latte" && stored.dark === "tokyo-night" && stored.mode === "system", "both picks are stored in the daemon");
  await page.emulateMedia({ colorScheme: "dark" });
  await waitTheme("tokyo-night");
  await settle();
  await shot("appearance-dark");
  await a11y("Appearance (dark)");

  // mode buttons override the Mac
  await page.locator("[data-mode='light']").click();
  await waitTheme("catppuccin-latte");
  check(true, "Mode: Light shows the light theme even on a dark Mac");
  await page.locator("[data-mode='system']").click();
  await waitTheme("tokyo-night");

  // keyboard: arrows move the selection inside a group
  await card("tokyo-night").focus();
  await page.keyboard.press("ArrowRight");
  await settle();
  check(await card("rose-pine").getAttribute("aria-checked") === "true" || (await page.locator("[role='radiogroup'][aria-label='Dark theme'] [aria-checked='true']").getAttribute("data-theme-id")) !== "tokyo-night", "arrow keys move the selection in a theme group");
  await card("tokyo-night").click();

  // ---- two themes in light and dark, screenshots of the app itself -----------------------------
  const looks = [["gruvbox-light", "gruvbox-dark"], ["solarized-light", "nord"], ["neutral-light", "glass"]];
  for (const [light, dark] of looks) {
    await card(light).click();
    await card(dark).click();
    await settle();
    for (const scheme of ["light", "dark"]) {
      await page.emulateMedia({ colorScheme: scheme });
      await waitTheme(scheme === "light" ? light : dark);
      await page.goto(`${base}/#/tasks`);
      await page.waitForSelector("[data-slot='task-row']", { timeout: 8000 }).catch(() => {});
      await sleep(500);
      await shot(`theme-${scheme === "light" ? light : dark}-tasks`);
      await a11y(`Tasks (${scheme === "light" ? light : dark})`);
      await open();
    }
  }
  await page.emulateMedia({ colorScheme: "dark" });
  await card("neutral-light").click();
  await card("neutral").click();
  await settle();

  // ---- a theme file appears, changes and goes, live --------------------------------------------
  await open();
  check((await card("ocean").count()) === 0, "no theme of mine yet");
  write(OCEAN);
  await card("ocean").waitFor({ timeout: 8000 });
  check((await page.locator("[data-slot='theme-grid'] [data-source='user'] [data-slot='theme-yours']").count()) === 1 && (await card("ocean").locator("xpath=ancestor::*[@role='radiogroup']").getAttribute("aria-label")) === "Dark theme", "a theme file appears in the Dark picker, marked Yours, without a reload");
  await card("ocean").click();
  await waitTheme("ocean");
  check((await token("--background")) === "#0b1f2a" && (await token("--primary")) === "#3ab0d0", "picking it applies its colours");
  await shot("appearance-ocean");
  write({ ...OCEAN, colors: { ...OCEAN.colors, background: "#201028" } });
  await page.waitForFunction(() => getComputedStyle(document.documentElement).getPropertyValue("--background").trim() === "#201028", null, { timeout: 8000 });
  check(true, "editing the file changes the page live");
  check(await page.evaluate(() => (JSON.parse(localStorage.getItem("neko-theme-boot")).dark.css || "").includes("#201028")), "the boot cache holds the theme's CSS, so a reload paints it first");

  // first paint: record what the very first frame looked like
  await page.addInitScript(() => {
    new PerformanceObserver(() => {
      if (window.__firstPaint) return;
      const root = document.documentElement;
      window.__firstPaint = { theme: root.getAttribute("data-neko-theme"), bg: getComputedStyle(root).getPropertyValue("--background").trim(), cls: root.className };
    }).observe({ type: "paint", buffered: true });
  });
  await page.reload({ waitUntil: "load" });
  await sleep(500);
  const first = await page.evaluate(() => window.__firstPaint);
  check(first?.theme === "ocean" && first.bg === "#201028" && /\bdark\b/.test(first.cls), `a reload paints the chosen theme first, with no flash (${JSON.stringify(first)})`);
  await page.emulateMedia({ colorScheme: "light" });
  await waitTheme("neutral-light");
  await page.reload({ waitUntil: "load" });
  await sleep(500);
  const firstLight = await page.evaluate(() => window.__firstPaint);
  check(firstLight?.theme === "neutral-light" && /\blight\b/.test(firstLight.cls) && firstLight.bg === "#fafafa", `...and a light Mac paints the light theme first (${JSON.stringify(firstLight)})`);
  await page.emulateMedia({ colorScheme: "dark" });
  await waitTheme("ocean");

  // ---- bad files are skipped, with a warning; low contrast loads with a chip -------------------
  await open();
  write("{ this is not json", "broken");
  write({ ...OCEAN, id: "mismatch" }, "wrong-name");
  write({ ...OCEAN, id: "css-sneak", colors: { ...OCEAN.colors, primary: "red; } body { display: none" } }, "css-sneak");
  write(LOW);
  await page.waitForSelector("[data-slot='theme-problems']", { timeout: 8000 });
  const problems = await page.locator("[data-slot='theme-problems']").innerText();
  check(/broken\.json[\s\S]*not valid JSON/.test(problems) && /wrong-name\.json[\s\S]*file is named wrong-name/.test(problems) && /css-sneak\.json[\s\S]*hex colour/.test(problems), `bad files are listed as skipped, each with its reason`);
  check((await card("broken").count()) === 0 && (await page.locator("body").evaluate((b) => getComputedStyle(b).display)) !== "none", "a bad file is not a theme and does not touch the page");
  await shot("appearance-problems");
  await card("washed-out").waitFor({ timeout: 8000 });
  check((await card("washed-out").locator("xpath=..").locator("[data-slot='theme-warning']").count()) === 1, "a low-contrast theme still loads, with a Low contrast chip");
  check((await card("ocean").locator("xpath=..").locator("[data-slot='theme-warning']").count()) === 0, "a theme that passes has no chip");
  await a11y("Appearance (problems)");
  const listed = (await daemon("ListThemes")).Themes;
  check(listed.themes.map((t) => t.id).sort().join() === "ocean,washed-out" && listed.problems.length === 3 && listed.dir === themesDir, "the daemon lists the good files and names the bad ones");
  rmSync(file("broken")); rmSync(file("wrong-name")); rmSync(file("css-sneak"));
  await page.waitForFunction(() => !document.querySelector("[data-slot='theme-problems']"), null, { timeout: 8000 });
  check(true, "fixing or deleting the bad files clears the warnings");

  // ---- a user theme with a built-in's id replaces it; removing the file restores it ------------
  write(NORD_MINE);
  await page.waitForFunction(() => document.querySelector("[data-slot='theme-card'][data-theme-id='nord']")?.textContent?.includes("Nord (mine)"), null, { timeout: 8000 });
  check((await card("nord").locator("xpath=..").innerText()).includes("replaces built-in"), "a file named for a built-in replaces it and says so");
  await card("nord").click();
  await waitTheme("nord");
  check((await token("--primary")) === "#ff8800", "the replacement's colours are what Nord shows now");

  // ---- Remove: a confirm, then the file is gone ------------------------------------------------
  await card("nord").locator("xpath=..").hover();
  await card("nord").locator("xpath=..").locator("[data-slot='theme-remove']").click();
  await page.getByRole("alertdialog").waitFor({ timeout: 3000 });
  check((await page.getByRole("alertdialog").innerText()).includes("built-in theme with this name comes back"), "Remove asks first, and says the built-in comes back");
  await page.getByRole("button", { name: "Cancel" }).click();
  check(existsSync(file("nord")), "Cancel leaves the file");
  await card("nord").locator("xpath=..").locator("[data-slot='theme-remove']").click();
  await page.getByRole("alertdialog").getByRole("button", { name: "Remove" }).click();
  await page.waitForFunction(() => document.querySelector("[data-slot='theme-card'][data-theme-id='nord']")?.textContent?.includes("Nord") && !document.querySelector("[data-slot='theme-card'][data-theme-id='nord']")?.textContent?.includes("(mine)"), null, { timeout: 8000 });
  check(!existsSync(file("nord")) && (await token("--primary")) === "#88c0d0", "Remove deletes the file and the built-in Nord is back");
  check((await card("nord").locator("xpath=..").locator("[data-slot='theme-remove']").count()) === 0, "a built-in has no Remove");
  const refused = await daemon({ RemoveTheme: { id: "nord" } });
  check(refused.Error?.message.includes("built-in themes cannot be removed") && (await daemon({ RemoveTheme: { id: "../secret" } })).Error, "the daemon refuses to remove a built-in, or a path");

  // removing the theme in use falls back to the default for dark, and removing it live updates other pages
  await card("ocean").click();
  await waitTheme("ocean");
  await card("ocean").locator("xpath=..").hover();
  await card("ocean").locator("xpath=..").locator("[data-slot='theme-remove']").click();
  await page.getByRole("alertdialog").getByRole("button", { name: "Remove" }).click();
  await waitTheme("neutral");
  check(!existsSync(file("ocean")), "removing the theme in use falls back to Neko Dark");
  rmSync(file("washed-out"), { force: true });

  // ---- Make me a theme -------------------------------------------------------------------------
  await page.locator("[data-slot='make-theme']").click();
  await page.waitForSelector("[data-slot='theme-dialog']");
  await page.locator("summary", { hasText: "What Neko will be told" }).click();
  const told = await page.locator("[data-slot='theme-prompt']").innerText();
  check(told.includes(themesDir) && told.includes('"mutedForeground"') && told.includes("4.5:1") && told.includes('"backdrop"'), "the dialog shows the prompt, with the themes folder and the schema");
  await page.locator("#theme-wish").fill("A dark theme in deep teal with a gold accent");
  check(await page.locator("[data-slot='theme-send']").isEnabled(), "Ask Neko is enabled once there is a request");
  await page.keyboard.press("Escape");

  // ---- the gear and ⌘K ---------------------------------------------------------------------------
  await page.goto(`${base}/#/`);
  await page.locator("[data-slot='theme-menu']").click();
  await page.locator("[data-slot='theme-menu-light']").hover();
  await page.getByRole("menuitemradio", { name: "Solarized Light" }).click();
  await page.locator("[data-slot='theme-menu-dark']").hover();
  await page.getByRole("menuitemradio", { name: "Rosé Pine" }).click();
  await waitTheme("rose-pine");
  check(true, "the gear picks the light and the dark theme separately");
  await page.keyboard.press("Escape");
  await page.keyboard.press("Escape");
  await palette("Theme: Nord");
  await waitTheme("nord");
  check(true, "⌘K “Theme: Nord” picks it");
  await palette("Mode: light");
  await waitTheme("solarized-light");
  check(true, "⌘K “Mode: always light” shows the light theme (Solarized Light)");
  await palette("Mode: follow");
  await waitTheme("nord");

  // leave the daemon as a new install would have it
  await daemon({ SetAppearance: { mode: "system", light: "neutral-light", dark: "neutral" } });
  await page.evaluate(() => localStorage.setItem("neko-appearance", JSON.stringify({ mode: "system", light: "neutral-light", dark: "neutral" })));
  await page.goto(`${base}/#/`);
  await waitTheme("neutral");
}
