// A theme is data: a handful of colours. This file turns one into the CSS
// custom properties the whole app reads, and checks it against the contrast
// rules. It is plain JavaScript on purpose: the build step (vite.config.ts), the
// contrast script (scripts/verify/themes.mjs) and the page (src/themes/theme.ts)
// all import this one file, so they cannot disagree about what a theme means.
//
// A theme supplies values; it never redefines what a token means. Everything a
// theme does not set is derived here, in one place, the way the native app's
// `theme::build()` does. The schema (and docs/design/themes.md) allows exactly
// the keys in COLOR_KEYS; the daemon (crates/neko-core/src/appearance.rs) and
// src/themes/schema.js check the same list.

export const COLOR_KEYS = [
  "background", "card", "popover", "sidebar", "foreground", "strong", "mutedForeground", "border", "primary", "primaryForeground",
  "destructive", "success", "warning", "info", "bubble", "bubbleForeground",
];
export const REQUIRED_COLORS = ["background", "foreground", "primary"];
export const TOP_KEYS = ["id", "name", "appearance", "family", "colors", "backdrop"];

// ---------------------------------------------------------------- colour maths

/** "#abc", "#aabbcc" or "#aabbccdd" -> [r, g, b, a] (0-255, a 0-1), or null. */
export function parseHex(text) {
  if (typeof text !== "string") return null;
  const m = /^#([0-9a-fA-F]{3}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})$/.exec(text);
  if (!m) return null;
  let h = m[1];
  if (h.length === 3) h = [...h].map((c) => c + c).join("");
  const n = (i) => parseInt(h.slice(i, i + 2), 16);
  return [n(0), n(2), n(4), h.length === 8 ? n(6) / 255 : 1];
}

const clamp = (x, lo = 0, hi = 255) => Math.min(hi, Math.max(lo, x));
const byte = (x) => clamp(Math.round(x)).toString(16).padStart(2, "0");

/** [r, g, b] -> "#rrggbb" (alpha is dropped: use rgba() for that). */
export const toHex = ([r, g, b]) => `#${byte(r)}${byte(g)}${byte(b)}`;

/** A colour at some opacity, as CSS. */
export function rgba(color, alpha) {
  const [r, g, b] = typeof color === "string" ? parseHex(color) : color;
  const a = Math.round(alpha * 1000) / 1000;
  return a >= 1 ? toHex([r, g, b]) : `rgba(${Math.round(r)}, ${Math.round(g)}, ${Math.round(b)}, ${a})`;
}

/** `from` moved `t` (0-1) of the way to `to`; both opaque hex strings. */
export function mix(from, to, t) {
  const a = parseHex(from);
  const b = parseHex(to);
  return toHex([0, 1, 2].map((i) => a[i] + (b[i] - a[i]) * t));
}

/** WCAG relative luminance of an opaque colour. */
export function luminance(color) {
  const [r, g, b] = parseHex(color).map((v) => v / 255).slice(0, 3).map((v) => (v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4));
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

/** A colour with an alpha, composited over an opaque base. */
export function over(color, base) {
  const [r, g, b, a] = parseHex(color);
  const [br, bg, bb] = parseHex(base);
  return toHex([r * a + br * (1 - a), g * a + bg * (1 - a), b * a + bb * (1 - a)]);
}

/** WCAG contrast ratio of two opaque colours. */
export function contrast(a, b) {
  const [x, y] = [luminance(a), luminance(b)].sort((p, q) => q - p);
  return (x + 0.05) / (y + 0.05);
}

/**
 * Moves `color` towards white or black (whichever the surfaces are not) until it
 * clears `min` on every surface. The policy the native app set: move the text,
 * never the surface.
 */
export function ensure(color, surfaces, min = 4.5) {
  const light = surfaces.every((s) => luminance(s) > 0.4);
  const target = light ? "#000000" : "#ffffff";
  for (let t = 0; t <= 1.0001; t += 0.02) {
    const c = mix(color, target, t);
    if (surfaces.every((s) => contrast(c, s) >= min)) return c;
  }
  return target;
}

/** The first candidate that reads comfortably (7:1) on `surface`, else whichever reads best. */
const bestOn = (surface, candidates) => candidates.find((c) => contrast(c, surface) >= 7) ?? candidates.reduce((best, c) => (contrast(c, surface) > contrast(best, surface) ? c : best));

// ------------------------------------------------------------------ derivation

/** How see-through the surfaces are when a theme has a backdrop (Glass). */
export const BACKDROP_ALPHA = { background: 0.42, card: 0.48, popover: 0.78, sidebar: 0.46, code: 0.62 };

const STATUS_DEFAULTS = {
  dark: { destructive: "#e5484d", success: "#4cc38a", warning: "#e0a042", info: "#5b9cf5" },
  light: { destructive: "#e6293f", success: "#00af6f", warning: "#e19b1b", info: "#0083e0" },
};

/**
 * What a surface actually looks like: itself, or, with a backdrop, its translucent
 * fill composited over the brightest part of the wash (the worst case for text).
 */
export function effectiveSurfaces(theme, solid) {
  if (!theme.backdrop) return solid;
  const base = theme.backdrop.base[0].slice(0, 7);
  const wash = theme.backdrop.base.map((b) => b.slice(0, 7)).concat((theme.backdrop.glows ?? []).map((g) => over(rgbaHex(g.color, g.alpha), base)));
  const brightest = wash.reduce((a, b) => (luminance(b) > luminance(a) ? b : a));
  return Object.fromEntries(Object.entries(solid).map(([k, v]) => [k, BACKDROP_ALPHA[k] != null ? over(rgbaHex(v, BACKDROP_ALPHA[k]), brightest) : v]));
}

/**
 * Every colour the app reads, resolved: the theme's own where it set one, a
 * derived one where it did not. All opaque hex, so contrast can be measured.
 */
export function resolveColors(theme) {
  const dark = theme.appearance === "dark";
  const set = theme.colors;
  const bg = set.background;
  const fg = set.foreground;
  const white = "#ffffff";
  const black = "#000000";
  const card = set.card ?? (dark ? mix(bg, white, 0.04) : mix(bg, white, 0.6));
  const popover = set.popover ?? (dark ? mix(card, white, 0.03) : card);
  const sidebar = set.sidebar ?? (dark ? mix(bg, black, 0.2) : mix(bg, black, 0.025));
  const strong = set.strong ?? (dark ? mix(fg, white, 0.5) : mix(fg, black, 0.55));
  const status = STATUS_DEFAULTS[theme.appearance];
  const destructive = set.destructive ?? status.destructive;
  const success = set.success ?? status.success;
  const warning = set.warning ?? status.warning;
  const info = set.info ?? status.info;
  const primary = set.primary;
  const eff = effectiveSurfaces(theme, { background: bg, card, popover, sidebar });
  const surfaces = [eff.background, eff.card, eff.popover, eff.sidebar];
  // An authored colour is used as written (a low-contrast one is reported, never silently changed); a derived one is nudged until it passes.
  const muted = set.mutedForeground ?? ensure(mix(fg, bg, 0.38), [eff.background, eff.card, eff.popover]);
  const primaryForeground = set.primaryForeground ?? bestOn(primary, [bg, white, black]);
  const bubble = set.bubble ?? (dark ? mix(bg, primary, 0.35) : mix(white, primary, 0.14));
  const bubbleForeground = set.bubbleForeground ?? bestOn(bubble, [strong, white, black]);
  return {
    background: bg, card, popover, sidebar, foreground: fg, strong, primary, primaryForeground, bubble, bubbleForeground,
    mutedForeground: muted,
    sidebarForeground: ensure(mix(fg, sidebar, 0.25), [eff.sidebar]),
    destructive, success, warning, info,
    destructiveForeground: ensure(destructive, surfaces),
    successForeground: ensure(success, surfaces),
    warningForeground: ensure(warning, surfaces),
    infoForeground: ensure(info, surfaces),
    border: set.border ?? fg,
    code: mix(bg, fg, 0.04),
    codeForeground: fg,
    dark,
  };
}

/**
 * The CSS custom properties for a theme, as an ordered list of [name, value].
 * Alpha steps (borders, hover washes) are fixed fractions of the text colour,
 * which is how today's tokens were written.
 */
export function deriveVars(theme) {
  const c = resolveColors(theme);
  const dark = c.dark;
  const wash = (a) => rgba(c.foreground, a);
  const backdrop = theme.backdrop;
  const hair = backdrop ? 0.14 : dark ? 0.07 : 0.08;
  const alpha = (name, value) => (backdrop && BACKDROP_ALPHA[name] != null ? rgba(value, BACKDROP_ALPHA[name]) : value);
  const edge = c.border;
  const scrollbar = dark ? "#ffffff" : "#000000";
  const vars = [
    ["--background", alpha("background", c.background)],
    ["--foreground", c.foreground],
    ["--strong", c.strong],
    ["--card", alpha("card", c.card)],
    ["--card-foreground", c.foreground],
    ["--popover", alpha("popover", c.popover)],
    ["--popover-foreground", c.foreground],
    ["--primary", c.primary],
    ["--primary-foreground", c.primaryForeground],
    ["--secondary", wash(dark ? 0.05 : 0.04)],
    ["--secondary-foreground", c.foreground],
    ["--muted", wash(dark ? 0.04 : 0.04)],
    ["--muted-foreground", c.mutedForeground],
    ["--accent", wash(dark ? 0.06 : 0.04)],
    ["--accent-foreground", c.strong],
    ["--destructive", c.destructive],
    ["--destructive-foreground", c.destructiveForeground],
    ["--border", rgba(edge, hair)],
    ["--input", rgba(edge, hair + 0.02)],
    ["--ring", rgba(c.primary, dark ? 0.65 : 0.55)],
    ["--info", c.info],
    ["--info-foreground", c.infoForeground],
    ["--success", c.success],
    ["--success-foreground", c.successForeground],
    ["--warning", c.warning],
    ["--warning-foreground", c.warningForeground],
    ["--chart-1", c.primary],
    ["--chart-2", c.success],
    ["--chart-3", c.warning],
    ["--chart-4", c.info],
    ["--chart-5", c.destructive],
    ["--sidebar", alpha("sidebar", c.sidebar)],
    ["--sidebar-foreground", c.sidebarForeground],
    ["--sidebar-primary", c.strong],
    ["--sidebar-primary-foreground", c.background],
    ["--sidebar-accent", wash(dark ? 0.07 : 0.06)],
    ["--sidebar-accent-foreground", c.strong],
    ["--sidebar-border", rgba(edge, backdrop ? hair : dark ? 0.06 : 0.07)],
    ["--sidebar-ring", rgba(c.primary, dark ? 0.65 : 0.55)],
    ["--code", alpha("code", c.code)],
    ["--code-foreground", c.codeForeground],
    ["--bubble", c.bubble],
    ["--bubble-foreground", c.bubbleForeground],
    ["--panel", rgba(backdrop ? c.card : dark ? mix(c.card, c.background, 0.25) : "#ffffff", backdrop ? 0.42 : dark ? 0.55 : 0.7)],
    ["--panel-border", rgba(dark ? "#ffffff" : "#000000", backdrop ? 0.16 : dark ? 0.1 : 0.06)],
    ["--panel-highlight", rgba("#ffffff", backdrop ? 0.1 : dark ? 0.06 : 0.6)],
    ["--glow-violet", rgba(c.primary, dark ? 0.2 : 0.1)],
    ["--glow-sky", rgba("#3b9ff0", dark ? 0.2 : 0.1)],
    ["--glow-amber", rgba("#f0b429", dark ? 0.18 : 0.1)],
    ["--glow-emerald", rgba("#2fc98a", dark ? 0.18 : 0.1)],
    ["--glow-rose", rgba("#e255c4", dark ? 0.2 : 0.1)],
    ["--scrollbar-thumb", rgba(scrollbar, dark ? 0.16 : 0.18)],
    ["--scrollbar-thumb-hover", rgba(scrollbar, dark ? 0.3 : 0.32)],
    ["--smooth-ring-color", backdrop ? "rgba(255, 255, 255, 0.16)" : dark ? "rgba(255, 255, 255, 0.12)" : "rgba(0, 0, 0, 0.06)"],
    ["color-scheme", dark ? "dark" : "light"],
  ];
  if (backdrop) {
    const [first, ...rest] = backdrop.base;
    const glows = (backdrop.glows ?? []).map((g) => `radial-gradient(ellipse ${g.width}% ${g.height}% at ${g.x}% ${g.y}%, ${rgba(g.color, g.alpha)}, transparent 70%)`);
    const base = rest.length ? `linear-gradient(160deg, ${[first, ...rest].join(", ")})` : first;
    vars.push(["--backdrop-wash", [...glows, base].join(", ")]);
    vars.push(["--backdrop-frost", `blur(${backdrop.blur}px) saturate(160%)`]);
    vars.push(["--backdrop-solid", c.background]);
  }
  return vars;
}

const RULE_OPEN = (id) => `html[data-neko-theme="${id}"]`;

/** One theme's CSS rule (built-ins), or the same with doubled specificity (user themes, which must beat a built-in of the same id). */
export function themeRule(theme, { user = false } = {}) {
  const selector = user ? `${RULE_OPEN(theme.id)}[data-neko-theme]` : RULE_OPEN(theme.id);
  const body = deriveVars(theme).map(([name, value]) => `  ${name}: ${value};`).join("\n");
  const solid = theme.backdrop
    ? `\n@supports not ((backdrop-filter: blur(1px)) or (-webkit-backdrop-filter: blur(1px))) {\n  ${selector} {\n${solidFallback(theme).map(([n, v]) => `    ${n}: ${v};`).join("\n")}\n  }\n}`
    : "";
  return `${selector} {\n${body}\n}${solid}`;
}

/** Without backdrop-filter a translucent surface would sit on an unblurred wash: use the solid colours. */
function solidFallback(theme) {
  const c = resolveColors(theme);
  return [["--background", c.background], ["--card", c.card], ["--popover", c.popover], ["--sidebar", c.sidebar], ["--code", c.code]];
}

/** The stylesheet text for a list of themes, inside the `neko-theme` layer. */
export function themesCss(themes, options) {
  return `@layer neko-theme {\n${themes.map((t) => themeRule(t, options)).join("\n")}\n}\n`;
}

// -------------------------------------------------------------------- contrast

/**
 * The text-on-surface pairs a theme must clear (4.5:1), as [label, text, surface].
 * With a backdrop, each surface is also measured as it composites over the
 * brightest part of the wash (the rule Glass was verified under).
 */
export function contrastPairs(theme) {
  const c = resolveColors(theme);
  const pairs = [];
  const add = (label, text, surface) => pairs.push([label, text, surface]);
  const surfaces = effectiveSurfaces(theme, { background: c.background, card: c.card, popover: c.popover, sidebar: c.sidebar });
  for (const [name, surface] of Object.entries(surfaces)) {
    add(`foreground on ${name}`, c.foreground, surface);
    add(`strong on ${name}`, c.strong, surface);
  }
  for (const name of ["background", "card", "popover"]) add(`mutedForeground on ${name}`, c.mutedForeground, surfaces[name]);
  add("sidebarForeground on sidebar", c.sidebarForeground, surfaces.sidebar);
  add("primaryForeground on primary", c.primaryForeground, c.primary);
  add("bubbleForeground on bubble", c.bubbleForeground, c.bubble);
  add("codeForeground on code", c.codeForeground, theme.backdrop ? over(rgbaHex(c.code, BACKDROP_ALPHA.code), surfaces.background) : c.code);
  for (const status of ["destructive", "success", "warning", "info"]) {
    add(`${status}-foreground on background`, c[`${status}Foreground`], surfaces.background);
    add(`${status}-foreground on card`, c[`${status}Foreground`], surfaces.card);
  }
  return pairs;
}

function rgbaHex(color, alpha) {
  const [r, g, b] = parseHex(color);
  const [, , , a0] = parseHex(color);
  return `#${byte(r)}${byte(g)}${byte(b)}${byte(255 * alpha * a0)}`;
}

/** Every pair under `min`, with its ratio. Empty means the theme passes. */
export function contrastFailures(theme, min = 4.5) {
  return contrastPairs(theme)
    .map(([label, text, surface]) => ({ label, ratio: contrast(text, surface) }))
    .filter((p) => p.ratio < min);
}

/**
 * The visible-step rules: a card or sidebar that is not distinguishable from the
 * page is a flat theme, and tuning text to pass must never get there by
 * flattening a surface. Soft thresholds, calibrated on Neko Dark and Light.
 */
export function structureFailures(theme) {
  const c = resolveColors(theme);
  const failures = [];
  const step = (label, a, b, min) => {
    const r = contrast(a, b);
    if (r < min) failures.push({ label, ratio: r });
  };
  if (!theme.backdrop) {
    step("card against background", c.card, c.background, 1.03);
    step("sidebar against background", c.sidebar, c.background, 1.02);
  }
  step("border against card", over(rgbaHex(c.border, theme.backdrop ? 0.14 : c.dark ? 0.07 : 0.08), c.card), c.card, 1.05);
  return failures;
}

// ---------------------------------------------------------------------- schema

/**
 * Checks one theme document, the same rules as the daemon's
 * `appearance::validate_theme`. Returns null when it is fine, else the first
 * problem in plain words.
 */
export function validateTheme(value, expectedId) {
  if (!value || typeof value !== "object" || Array.isArray(value)) return "the file must hold one JSON object";
  const unknown = Object.keys(value).find((k) => !TOP_KEYS.includes(k));
  if (unknown) return `unknown key “${unknown}” (allowed: ${TOP_KEYS.join(", ")})`;
  if (typeof value.id !== "string") return "“id” is missing";
  if (expectedId != null && value.id !== expectedId) return `“id” is “${value.id}” but the file is named ${expectedId}.json`;
  if (!/^[a-z0-9][a-z0-9-]{0,47}$/.test(value.id)) return "“id” must be lowercase letters, digits and hyphens";
  if (typeof value.name !== "string" || !value.name.trim() || [...value.name.trim()].length > 60) return "“name” must be 1 to 60 characters";
  if (value.appearance !== "light" && value.appearance !== "dark") return "“appearance” must be \"light\" or \"dark\"";
  if (value.family != null && (typeof value.family !== "string" || !value.family || [...value.family].length > 40)) return "“family” must be 1 to 40 characters";
  const colors = value.colors;
  if (!colors || typeof colors !== "object" || Array.isArray(colors)) return "“colors” must be an object";
  for (const key of REQUIRED_COLORS) if (!(key in colors)) return `colors.${key} is required`;
  for (const [key, hex] of Object.entries(colors)) {
    if (!COLOR_KEYS.includes(key)) return `unknown colour “${key}” (allowed: ${COLOR_KEYS.join(", ")})`;
    if (!parseHex(hex)) return `colors.${key} must be a hex colour like #1a2b3c (3, 6 or 8 digits)`;
  }
  if (value.backdrop != null) return validateBackdrop(value.backdrop);
  return null;
}

const inRange = (n, lo, hi) => typeof n === "number" && n >= lo && n <= hi;

function validateBackdrop(b) {
  if (!b || typeof b !== "object" || Array.isArray(b)) return "“backdrop” must be an object";
  const unknown = Object.keys(b).find((k) => !["blur", "base", "glows"].includes(k));
  if (unknown) return `unknown backdrop key “${unknown}” (allowed: blur, base, glows)`;
  if (!inRange(b.blur, 0, 60)) return "backdrop.blur must be a number from 0 to 60";
  if (!Array.isArray(b.base) || b.base.length < 1 || b.base.length > 4 || !b.base.every((x) => parseHex(x))) return "backdrop.base must be a list of 1 to 4 hex colours";
  if (b.glows != null) {
    if (!Array.isArray(b.glows) || b.glows.length > 4) return "backdrop.glows must be a list of at most 4 glows";
    for (const g of b.glows) {
      if (!g || typeof g !== "object") return "each glow must be an object";
      const bad = Object.keys(g).find((k) => !["color", "alpha", "x", "y", "width", "height"].includes(k));
      if (bad) return `unknown glow key “${bad}”`;
      if (!parseHex(g.color)) return "a glow needs a hex \"color\"";
      if (!inRange(g.alpha, 0, 1) || !inRange(g.x, -50, 150) || !inRange(g.y, -50, 150) || !inRange(g.width, 1, 200) || !inRange(g.height, 1, 200)) {
        return "a glow needs alpha 0 to 1, x and y in percent, width and height 1 to 200";
      }
    }
  }
  return null;
}
