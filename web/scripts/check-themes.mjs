#!/usr/bin/env node
// The theme gate: `pnpm themes:check` (also run by scripts/check.sh). Reads the
// built-in theme data with the same derive.js the page uses and fails on:
//   - a theme that is not valid against the schema, or whose file name is not its id
//   - any text pair under 4.5:1 (derive.js contrastPairs), or a flat theme (structureFailures)
//   - a stale src/themes/builtin.generated.css
//   - the web list drifting from the native one (crates/neko-protocol BUILTIN_THEMES):
//     a web built-in must exist natively, and share its page colour
//   - the colour keys differing between derive.js, the daemon (appearance.rs) and the
//     "Make me a theme" prompt
//   - public/theme-boot.js and src/theme.ts naming different storage keys
// It also proves the checker bites: a made-up low-contrast theme must fail.
import { readFileSync, readdirSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { COLOR_KEYS, contrast, contrastFailures, structureFailures, validateTheme } from "../src/themes/derive.js";
import { builtinCss, builtinThemes } from "./themes.mjs";

const web = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const repo = path.resolve(web, "..");
const failures = [];
const check = (ok, label) => {
  console.log(`${ok ? "ok  " : "FAIL"} ${label}`);
  if (!ok) failures.push(label);
};
const read = (file) => readFileSync(path.join(repo, file), "utf8");

const themes = builtinThemes();
const files = readdirSync(path.join(web, "src/themes/builtin")).filter((f) => f.endsWith(".json")).map((f) => f.replace(/\.json$/, "")).sort();
check(themes.length === 12, `12 built-in themes (found ${themes.length})`);
check(themes.filter((t) => t.appearance === "light").length === 4 && themes.filter((t) => t.appearance === "dark").length === 8, "4 light and 8 dark");
check(JSON.stringify(themes.map((t) => t.id).sort()) === JSON.stringify(files), "every file is named for the id inside it");
for (const theme of themes) {
  const invalid = validateTheme(theme, theme.id);
  check(invalid === null, `${theme.id}: valid against the schema${invalid ? ` (${invalid})` : ""}`);
  const low = contrastFailures(theme);
  check(low.length === 0, `${theme.id}: every text pair is 4.5:1 or better${low.length ? ` (${low.map((f) => `${f.label} ${f.ratio.toFixed(2)}`).join("; ")})` : ""}`);
  const flat = structureFailures(theme);
  check(flat.length === 0, `${theme.id}: surfaces are visibly distinct${flat.length ? ` (${flat.map((f) => `${f.label} ${f.ratio.toFixed(2)}`).join("; ")})` : ""}`);
}

// The checker has to be able to fail.
const bad = { id: "bad", name: "Bad", appearance: "dark", colors: { background: "#888888", foreground: "#777777", primary: "#999999" } };
check(contrastFailures(bad).length > 0 && contrast("#777777", "#888888") < 4.5, "self-test: a grey-on-grey theme is reported");
check(validateTheme({ ...bad, colors: { ...bad.colors, accent: "#fff" } }) !== null && validateTheme({ ...bad, extra: 1 }) !== null && validateTheme({ ...bad, colors: { ...bad.colors, primary: "teal" } }) !== null, "self-test: unknown keys and non-hex colours are refused");

// The generated stylesheet is the data, not a copy that can go stale.
check(readFileSync(path.join(web, "src/themes/builtin.generated.css"), "utf8") === builtinCss(), "builtin.generated.css matches the theme data (run `node scripts/themes.mjs`)");

// Web and native lists.
const protocol = read("crates/neko-protocol/src/lib.rs");
const nativeIds = [...protocol.slice(protocol.indexOf("BUILTIN_THEMES")).matchAll(/id: "([a-z0-9-]+)"/g)].map((m) => m[1]);
const native = new Map();
for (const m of read("crates/neko/src/theme.rs").matchAll(/id: "([a-z0-9-]+)",[\s\S]*?surface_panel: 0x([0-9a-f]{6})/g)) native.set(m[1], `#${m[2]}`);
for (const theme of themes) {
  check(nativeIds.includes(theme.id), `${theme.id}: also a native theme (same id)`);
}
// Neko's own three are tuned for the web (a lighter page); the vendored palettes share the native page colour.
for (const theme of themes.filter((t) => !["neutral", "neutral-light", "glass"].includes(t.id))) {
  check(native.get(theme.id)?.toLowerCase() === theme.colors.background.toLowerCase(), `${theme.id}: page colour equals the native panel colour`);
}

// One list of colour keys, three places.
const rust = read("crates/neko-core/src/appearance.rs");
const rustKeys = [...rust.slice(rust.indexOf("pub const COLOR_KEYS"), rust.indexOf("];", rust.indexOf("pub const COLOR_KEYS"))).matchAll(/"([A-Za-z]+)"/g)].map((m) => m[1]);
check(JSON.stringify(rustKeys) === JSON.stringify(COLOR_KEYS), "the daemon and derive.js allow the same colour keys");
const prompt = readFileSync(path.join(web, "src/themes/prompt.ts"), "utf8");
check(COLOR_KEYS.every((k) => prompt.includes(`"${k}"`)), "the Make me a theme prompt names every colour key");
const doc = read("docs/design/themes.md");
check(COLOR_KEYS.every((k) => doc.includes(`"${k}"`)), "docs/design/themes.md names every colour key");

// The boot script and the page agree on what is stored.
const boot = readFileSync(path.join(web, "public/theme-boot.js"), "utf8");
const page = readFileSync(path.join(web, "src/theme.ts"), "utf8");
for (const key of ["neko-appearance", "neko-appearance-seed", "neko-theme-boot", "neko-user-themes", "data-neko-theme", "data-backdrop"]) {
  check(boot.includes(key) && page.includes(key), `theme-boot.js and theme.ts both use “${key}”`);
}
check(!/setAttribute\(\s*["']data-theme-id|dataset\.themeId/.test(boot + page), "nothing sets data-theme-id (the chat composer keys on it)");
check(!/\bglass\b/i.test(page.replace(/`?\?theme=[^\n]*|forced === "glass"|"glass"/g, "")), "theme.ts has no Glass-specific handling beyond the legacy ?theme=glass alias");

console.log(failures.length ? `\n${failures.length} failed` : "\nall theme checks passed");
process.exit(failures.length ? 1 : 0);
