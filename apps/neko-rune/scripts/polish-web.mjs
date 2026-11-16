// The web half of `polish-pairs.sh`: screenshots of the web UI over the shared headless Chrome (never launches its own).
// Env: PORT, TOKEN, OUT, THEME (dark|light), SHOTS (JSON [[name, route], ...]), PLAYWRIGHT_CORE (a folder with node_modules/playwright-core).
import { createRequire } from "node:module";
import path from "node:path";

const require = createRequire(path.join(process.env.PLAYWRIGHT_CORE, "package.json"));
const { chromium } = require("playwright-core");
const base = `http://127.0.0.1:${process.env.PORT}`;
const dark = process.env.THEME !== "light";
const browser = await chromium.connectOverCDP("http://127.0.0.1:9333");
const context = await browser.newContext({ viewport: { width: 1440, height: 900 }, deviceScaleFactor: 2, colorScheme: dark ? "dark" : "light" });
try {
  const page = await context.newPage();
  await page.goto(`${base}/?token=${process.env.TOKEN}`, { waitUntil: "load" });
  await page.waitForTimeout(3000);
  for (const [name, hash] of JSON.parse(process.env.SHOTS)) {
    await page.evaluate((h) => { location.hash = h; }, hash);
    await page.waitForTimeout(2500);
    await page.screenshot({ path: `${process.env.OUT}/${name}-web.png` });
    console.log("web", name, await page.evaluate(() => location.hash));
  }
} finally {
  await context.close();
  await browser.close();
}
