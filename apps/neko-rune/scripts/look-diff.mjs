// Compares a Rune snapshot with the matching prototype screenshot, both at 1x (docs/knowledge/rune-look-2026-11.md).
//
//   node look-diff.mjs <rune.png> <prototype.png> <out-prefix> [x,y,w,h]
//
// `x,y,w,h` (points) limits the comparison to a region (the sidebar is 0,0,248,900). Prints `{"pct": 1.23, "w":..,"h":..}` and writes
// <out-prefix>-pair.png (Rune | prototype | difference). pixelmatch threshold 0.1, anti-aliasing ignored.
// Needs `pngjs` and `pixelmatch`: install them in a scratch folder and point LOOK_TOOLS at it (default /tmp/neko-look/tools).
import { createRequire } from "node:module";
import fs from "node:fs";

const require = createRequire(`${process.env.LOOK_TOOLS ?? "/tmp/neko-look/tools"}/package.json`);
const { PNG } = require("pngjs");
const pixelmatch = require("pixelmatch").default ?? require("pixelmatch");

const [runePath, protoPath, prefix, rectArg] = process.argv.slice(2);
const rune = PNG.sync.read(fs.readFileSync(runePath));
const proto = PNG.sync.read(fs.readFileSync(protoPath));
const [rx, ry, rw, rh] = rectArg ? rectArg.split(",").map(Number) : [0, 0, Math.min(rune.width, proto.width), Math.min(rune.height, proto.height)];

function crop(img) {
  const out = new PNG({ width: rw, height: rh });
  PNG.bitblt(img, out, rx, ry, Math.min(rw, img.width - rx), Math.min(rh, img.height - ry), 0, 0);
  return out;
}
const a = crop(rune), b = crop(proto);
const diff = new PNG({ width: rw, height: rh });
const n = pixelmatch(a.data, b.data, diff.data, rw, rh, { threshold: 0.1, includeAA: false, diffColor: [255, 60, 60], alpha: 0.3 });
const sheet = new PNG({ width: rw * 3 + 16, height: rh });
for (let i = 0; i < sheet.data.length; i += 4) { sheet.data[i] = 60; sheet.data[i + 1] = 60; sheet.data[i + 2] = 60; sheet.data[i + 3] = 255; }
PNG.bitblt(a, sheet, 0, 0, rw, rh, 0, 0);
PNG.bitblt(b, sheet, 0, 0, rw, rh, rw + 8, 0);
PNG.bitblt(diff, sheet, 0, 0, rw, rh, 2 * rw + 16, 0);
fs.writeFileSync(`${prefix}-pair.png`, PNG.sync.write(sheet));
console.log(JSON.stringify({ pct: Math.round((10000 * n) / (rw * rh)) / 100, w: rw, h: rh, rune: [rune.width, rune.height], proto: [proto.width, proto.height] }));
