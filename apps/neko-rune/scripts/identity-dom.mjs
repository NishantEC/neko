// The web page's element boxes, for porting a layout by number (docs/knowledge/rune-identity.md).
//
//   node identity-dom.mjs <route> <width> <height> [maxDepth=12] [selector]
//
// One line per element that draws something (a fill, a border, text of its own, an image) or lays children out: depth, tag,
// data-slot or role, rect in points (x y w h), then the styles that matter (font, colour, background, radius, padding, gap).
import { createRequire } from "node:module";
import fs from "node:fs";

const OUT = process.env.OUT ?? "/tmp/neko-rune-identity";
const require = createRequire(`${process.env.TOOLS ?? `${OUT}/tools`}/package.json`);
const { chromium } = require("playwright-core");
const [route, w, h, maxDepth = "12", selector = "body"] = process.argv.slice(2);
const token = fs.readFileSync(`${process.env.DATA ?? `${OUT}/data`}/web-token`, "utf8").trim();
const browser = await chromium.connectOverCDP("http://127.0.0.1:9333");
const context = await browser.newContext({ viewport: { width: +w, height: +h }, deviceScaleFactor: 2, colorScheme: "dark" });
try {
  const page = await context.newPage();
  const port = process.env.PORT ?? "4771";
  for (const [k, v] of Object.entries(JSON.parse(process.env.STORAGE ?? "{}"))) {
    await page.goto(`http://127.0.0.1:${port}/?token=${token}`, { waitUntil: "load" });
    await page.evaluate(([a, b]) => localStorage.setItem(a, b), [k, v]);
  }
  await page.goto(`http://127.0.0.1:${port}/?token=${token}#/${route}${route.includes("?") ? "&" : "?"}embed=1&theme=dark`, { waitUntil: "load" });
  await page.reload({ waitUntil: "load" });
  await page.waitForTimeout(3000);
  // CLICK="tab:Skills" or CLICK="Some text" first (comma separated): states that need a click to show.
  for (const step of (process.env.CLICK ?? "").split(",").filter(Boolean)) {
    if (step.startsWith("tab:")) await page.getByRole("tab", { name: step.slice(4) }).first().click();
    else await page.getByText(step, { exact: true }).first().click();
    await page.waitForTimeout(800);
  }
  const lines = await page.evaluate(([max, sel]) => {
    const out = [];
    const r = (v) => Math.round(v * 10) / 10;
    const walk = (el, depth) => {
      if (depth > max) return;
      const s = getComputedStyle(el);
      if (s.display === "none") return;
      const b = el.getBoundingClientRect();
      const own = [...el.childNodes].filter((n) => n.nodeType === 3 && n.textContent.trim()).map((n) => n.textContent.replace(/\s+/g, " ").trim()).join(" ");
      const bg = s.backgroundColor !== "rgba(0, 0, 0, 0)" ? s.backgroundColor : "";
      const border = parseFloat(s.borderTopWidth) || parseFloat(s.borderBottomWidth) || parseFloat(s.borderLeftWidth) ? `${s.borderTopWidth} ${s.borderTopColor}${s.borderBottomWidth !== s.borderTopWidth ? ` / b ${s.borderBottomWidth}` : ""}` : "";
      const interesting = own || bg || border || el.tagName === "svg" || el.tagName === "IMG" || s.display.includes("flex") || s.display.includes("grid") || el.dataset.slot || el.getAttribute("role");
      if (interesting && b.width > 0 && b.height > 0) {
        const parts = [`${"  ".repeat(depth)}${el.tagName.toLowerCase()}${el.dataset.slot ? `[${el.dataset.slot}]` : ""}${el.getAttribute("role") ? `(${el.getAttribute("role")})` : ""} ${r(b.x)} ${r(b.y)} ${r(b.width)}x${r(b.height)}`];
        if (own) parts.push(`"${own.slice(0, 40)}" ${s.fontSize}/${s.fontWeight} ${s.color}${s.lineHeight !== "normal" ? ` lh ${s.lineHeight}` : ""}`);
        if (bg) parts.push(`bg ${bg}`);
        if (border) parts.push(`border ${border}`);
        if (parseFloat(s.borderTopLeftRadius)) parts.push(`r ${s.borderTopLeftRadius}`);
        if (s.padding !== "0px") parts.push(`pad ${s.padding}`);
        if (s.gap !== "normal") parts.push(`gap ${s.gap}`);
        if (s.display.includes("flex") || s.display.includes("grid")) parts.push(`${s.display}${s.flexDirection === "column" ? " col" : ""}`);
        out.push(parts.join("  "));
      }
      for (const c of el.children) walk(c, depth + 1);
    };
    walk(document.querySelector(sel), 0);
    return out;
  }, [+maxDepth, selector]);
  console.log(lines.join("\n"));
} finally {
  await context.close();
  await browser.close();
}
