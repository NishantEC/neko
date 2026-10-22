#!/usr/bin/env node
// Copies a file from hushbacks' packages/ui into web/src with the import
// rewrites and the provenance header the plan requires (docs/design/web-ui-from-hushbacks.md).
//
//   node web/scripts/vendor.mjs components/button.tsx            -> web/src/ui/button.tsx
//   node web/scripts/vendor.mjs components/editorial/Panel.tsx editorial
//   node web/scripts/vendor.mjs components/ai-elements/message.tsx chat
//
// Read-only on hushbacks. Review the result: anything needing the Vercel `ai`
// package, GCS upload or other app-specific code must be adapted by hand.
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const SRC = process.env.HUSHBACKS_UI ?? "/Users/nish/Documents/hushbacks/packages/ui/src";
const web = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const [rel, area = "ui"] = process.argv.slice(2);
if (!rel) throw new Error("usage: vendor.mjs <path under packages/ui/src> [ui|editorial|chat]");

const out = path.join(web, "src", area, path.basename(rel));
let code = readFileSync(path.join(SRC, rel), "utf8");
code = code
  .replace(/^"use client";\n+/m, "")
  .replace(/@workspace\/ui\/lib\/utils/g, "@/ui/lib/utils")
  .replace(/@workspace\/ui\/components\/ai-elements\//g, "@/chat/")
  .replace(/@workspace\/ui\/components\/editorial/g, "@/editorial")
  .replace(/@workspace\/ui\/components\//g, "@/ui/")
  .replace(/@workspace\/ui\/icons\/phosphor/g, "@/ui/icons")
  .replace(/@workspace\/ui\/hooks\//g, "@/ui/hooks/")
  .replace(/"\.\.\/\.\.\/lib\/utils"/g, '"@/ui/lib/utils"')
  .replace(/"\.\.\/button"/g, '"@/ui/button"');
mkdirSync(path.dirname(out), { recursive: true });
writeFileSync(out, `// from hushbacks packages/ui/src/${rel}\n${code}`);
console.log(`${rel} -> ${path.relative(web, out)}`);
