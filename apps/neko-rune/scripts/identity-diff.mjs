// Compares one Rune region with the web's page of the same size (docs/knowledge/rune-identity.md).
//
//   node identity-diff.mjs <rune.png> <x,y,w,h> <web.png> <out-prefix> [mask=x,y,w,h ...]
//
// `<x,y,w,h>` is the region's rect in points, as `--tree` reports it; both pictures are at 2x. The Rune picture is cropped to the
// rect, the web picture (rendered at the rect's size) is compared as it is. Each `mask` rect (points, relative to the region) is
// painted the same flat colour in both pictures first: dynamic text (times, counts) that is allowed to differ.
// Writes <out-prefix>-pair.png (Rune | web | difference) and prints `{"pct": 1.23, "width": ..., "height": ...}`.
// Needs `pngjs` and `pixelmatch` (ISC, MIT): install them in a scratch folder and point IDENTITY_TOOLS at it.
import { createRequire } from "node:module";
import fs from "node:fs";

const require = createRequire(`${process.env.IDENTITY_TOOLS ?? "/tmp/neko-rune-identity/tools"}/package.json`);
const { PNG } = require("pngjs");
const pixelmatch = require("pixelmatch").default ?? require("pixelmatch");

const [runePath, rectArg, webPath, prefix, ...rest] = process.argv.slice(2);
const [rx, ry, rw, rh] = rectArg.split(",").map(Number);
const masks = rest.filter((a) => a.startsWith("mask=")).map((a) => a.slice(5).split(",").map(Number));
const S = 2;

const rune = PNG.sync.read(fs.readFileSync(runePath));
const web = PNG.sync.read(fs.readFileSync(webPath));
const W = Math.round(rw * S), H = Math.round(rh * S);

function crop(img, x, y) {
  const out = new PNG({ width: W, height: H });
  PNG.bitblt(img, out, Math.round(x * S), Math.round(y * S), Math.min(W, img.width - Math.round(x * S)), Math.min(H, img.height - Math.round(y * S)), 0, 0);
  return out;
}
const a = crop(rune, rx, ry);
const b = crop(web, 0, 0);
for (const [mx, my, mw, mh] of masks) {
  for (const img of [a, b]) {
    for (let y = Math.round(my * S); y < Math.min(H, Math.round((my + mh) * S)); y++) {
      for (let x = Math.round(mx * S); x < Math.min(W, Math.round((mx + mw) * S)); x++) {
        const i = (y * W + x) * 4;
        img.data[i] = 255; img.data[i + 1] = 0; img.data[i + 2] = 255; img.data[i + 3] = 255;
      }
    }
  }
}
const diff = new PNG({ width: W, height: H });
const n = pixelmatch(a.data, b.data, diff.data, W, H, { threshold: 0.1, includeAA: false, diffColor: [255, 60, 60], alpha: 0.3 });
const pct = (100 * n) / (W * H);
const sheet = new PNG({ width: W * 3 + 16, height: H });
for (let i = 0; i < sheet.data.length; i += 4) { sheet.data[i] = 60; sheet.data[i + 1] = 60; sheet.data[i + 2] = 60; sheet.data[i + 3] = 255; }
PNG.bitblt(a, sheet, 0, 0, W, H, 0, 0);
PNG.bitblt(b, sheet, 0, 0, W, H, W + 8, 0);
PNG.bitblt(diff, sheet, 0, 0, W, H, 2 * W + 16, 0);
fs.writeFileSync(`${prefix}-pair.png`, PNG.sync.write(sheet));
console.log(JSON.stringify({ pct: Math.round(pct * 100) / 100, width: W, height: H, webWidth: web.width, webHeight: web.height }));
