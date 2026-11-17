// The web page's text nodes with their rects and type, for lining Rune's elements up against them (docs/knowledge/rune-identity.md).
//
//   node identity-texts.mjs <route> <width> <height> <out.json> [theme]
//
// Every visible text node of the embedded page: its words, its rect in points (relative to the page), font size, weight, colour.
// `identity-texts.py <out.json> <rune.tree.json> <rune-rect x,y>` then prints, per matching words, how far Rune's text is off.
import { createRequire } from "node:module";
import fs from "node:fs";

const OUT = process.env.OUT ?? "/tmp/neko-rune-identity";
const require = createRequire(`${process.env.TOOLS ?? `${OUT}/tools`}/package.json`);
const { chromium } = require("playwright-core");
const [route, w, h, out, theme = "dark"] = process.argv.slice(2);
const token = fs.readFileSync(`${process.env.DATA ?? `${OUT}/data`}/web-token`, "utf8").trim();
const browser = await chromium.connectOverCDP("http://127.0.0.1:9333");
const context = await browser.newContext({ viewport: { width: +w, height: +h }, deviceScaleFactor: 2, colorScheme: theme });
try {
  const page = await context.newPage();
  const port = process.env.PORT ?? "4771";
  await page.goto(`http://127.0.0.1:${port}/?token=${token}#/${route}${route.includes("?") ? "&" : "?"}embed=1&theme=${theme}`, { waitUntil: "load" });
  await page.reload({ waitUntil: "load" });
  await page.waitForTimeout(3000);
  const texts = await page.evaluate(() => {
    const found = [];
    const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
    for (let n = walker.nextNode(); n; n = walker.nextNode()) {
      const words = n.textContent.replace(/\s+/g, " ").trim();
      if (!words) continue;
      const el = n.parentElement;
      const style = getComputedStyle(el);
      if (style.visibility === "hidden" || style.display === "none") continue;
      const range = document.createRange();
      range.selectNodeContents(n);
      const r = range.getBoundingClientRect();
      if (r.width < 1 || r.height < 1) continue;
      found.push({ text: words, x: +r.x.toFixed(2), y: +r.y.toFixed(2), w: +r.width.toFixed(2), h: +r.height.toFixed(2), fs: style.fontSize, fw: style.fontWeight, color: style.color, family: style.fontFamily.split(",")[0], ls: style.letterSpacing, lh: style.lineHeight });
    }
    return found;
  });
  fs.writeFileSync(out, JSON.stringify(texts, null, 1));
  console.log(`${texts.length} text nodes`);
} finally {
  await context.close();
  await browser.close();
}
