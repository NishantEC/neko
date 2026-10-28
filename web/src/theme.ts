// The web UI's appearance: a mode (System, Light or Dark) and a theme for light
// and a theme for dark. The theme in use is the mode's pick, or, in System, the
// one for whichever the Mac is showing, so a person who likes Gruvbox Dark keeps
// it when the system flips.
//
// A theme is data (src/themes/derive.js turns its colours into the app's CSS
// variables). The built-ins are the JSON files in src/themes/builtin/, compiled
// into builtin.generated.css; the person's own are `<data dir>/themes/<id>.json`,
// which the daemon lists, validates and serves (`ListThemes`). This page polls
// that list, so a theme file added, edited or deleted shows up without a reload.
//
// What is where:
//   daemon settings   mode, light, dark: the source of truth (GetAppearance,
//                     SetAppearance, AppearanceChanged), shared by every browser
//   localStorage      `neko-appearance` (the same three) and `neko-theme-boot`
//                     (what public/theme-boot.js needs before first paint)
//   <html>            data-neko-theme=<id>, data-appearance, class dark|light,
//                     data-backdrop when the theme has one; never data-theme-id
//                     (the chat composer's rules key on that name)
//
// `?theme=light|dark|glass|<theme id>` on the first load forces one without
// saving it; scripts/web-verify.mjs mostly emulates the system preference
// instead, which is what the app follows.
import { useSyncExternalStore } from "react";
import { onEvent, request } from "./api";
import { contrastFailures, resolveColors, themesCss, validateTheme, type Appearance, type ThemeData } from "./themes/derive";

export type Mode = "system" | "light" | "dark";
export interface Choice { mode: Mode; light: string; dark: string }

export const DEFAULT_LIGHT = "neutral-light";
export const DEFAULT_DARK = "neutral";
const DEFAULT_CHOICE: Choice = { mode: "system", light: DEFAULT_LIGHT, dark: DEFAULT_DARK };

/** Where a theme comes from: shipped with the app, or a file of the person's. */
export type ThemeSource = "builtin" | "user";

export interface ThemeInfo {
  id: string;
  name: string;
  appearance: Appearance;
  family: string;
  source: ThemeSource;
  /** A file of the person's with a built-in's id: deleting the file restores the built-in. */
  overrides: boolean;
  /** The file in the themes folder, for a user theme. */
  file?: string;
  /** Text pairs under 4.5:1, for the warning chip. */
  lowContrast: string[];
  data: ThemeData;
}

export interface ThemeProblem { file: string; message: string }

// ---- the built-ins ---------------------------------------------------------

const ORDER = [
  "neutral-light", "catppuccin-latte", "gruvbox-light", "solarized-light",
  "neutral", "glass", "catppuccin-mocha", "gruvbox-dark", "solarized-dark", "nord", "tokyo-night", "rose-pine",
];
const modules = import.meta.glob<ThemeData>("./themes/builtin/*.json", { eager: true, import: "default" });
const BUILTIN_DATA: ThemeData[] = Object.values(modules).sort((a, b) => {
  const rank = (t: ThemeData) => (ORDER.includes(t.id) ? ORDER.indexOf(t.id) : ORDER.length);
  return rank(a) - rank(b) || a.name.localeCompare(b.name);
});

function info(data: ThemeData, source: ThemeSource, extra: Partial<ThemeInfo> = {}): ThemeInfo {
  return {
    id: data.id,
    name: data.name,
    appearance: data.appearance,
    family: data.family ?? (source === "builtin" ? data.name : ""),
    source,
    overrides: false,
    lowContrast: source === "builtin" ? [] : contrastFailures(data).map((f) => `${f.label} ${f.ratio.toFixed(1)}:1`),
    data,
    ...extra,
  };
}

const BUILTIN: ThemeInfo[] = BUILTIN_DATA.map((t) => info(t, "builtin"));

// ---- state -----------------------------------------------------------------

const KEY = "neko-appearance";
const SEED = "neko-appearance-seed";
const BOOT = "neko-theme-boot";
const STYLE_ID = "neko-user-themes";
const query = matchMedia("(prefers-color-scheme: dark)");

const isMode = (v: unknown): v is Mode => v === "system" || v === "light" || v === "dark";
const isId = (v: unknown): v is string => typeof v === "string" && /^[a-z0-9][a-z0-9-]{0,47}$/.test(v);

function readChoice(): Choice {
  try {
    const parsed = JSON.parse(localStorage.getItem(KEY) ?? "null");
    if (parsed && isMode(parsed.mode)) {
      return { mode: parsed.mode, light: isId(parsed.light) ? parsed.light : DEFAULT_LIGHT, dark: isId(parsed.dark) ? parsed.dark : DEFAULT_DARK };
    }
  } catch {
    /* storage can be blocked in an embedded view, or hold junk */
  }
  return { ...DEFAULT_CHOICE };
}

/** `?theme=` on the load: light or dark forces the mode, glass (the old name) forces that dark theme, any other id forces that theme. */
function forcedFromQuery(): string | null {
  const value = new URLSearchParams(location.search).get("theme");
  return value && isId(value) ? value : null;
}
const forced = forcedFromQuery();

let choice: Choice = readChoice();
let userThemes: ThemeInfo[] = [];
let problems: ThemeProblem[] = [];
let themesDir: string | null = null;
let listed = false;
let listingKey = "";

interface Snapshot {
  choice: Choice;
  themes: ThemeInfo[];
  problems: ThemeProblem[];
  themesDir: string | null;
  /** The theme on screen now, and which appearance it is. */
  active: ThemeInfo;
  appearance: Appearance;
}
let snapshot: Snapshot;
const listeners = new Set<() => void>();

function registry(): Map<string, ThemeInfo> {
  const map = new Map<string, ThemeInfo>();
  for (const t of BUILTIN) map.set(t.id, t);
  for (const t of userThemes) map.set(t.id, { ...t, overrides: map.has(t.id) });
  return map;
}

/** The themes a picker offers: the built-ins in order (replaced in place when overridden), then the person's own. */
function ordered(map: Map<string, ThemeInfo>): ThemeInfo[] {
  const builtinIds = new Set(BUILTIN.map((t) => t.id));
  const rest = [...map.values()].filter((t) => !builtinIds.has(t.id));
  return [...BUILTIN.map((t) => map.get(t.id)!), ...rest];
}

function bootCache(): Record<string, { id: string }> {
  try {
    return JSON.parse(localStorage.getItem(BOOT) ?? "{}") ?? {};
  } catch {
    return {};
  }
}

function compute(): Snapshot {
  const map = registry();
  let mode = choice.mode;
  let light = choice.light;
  let dark = choice.dark;
  let pick: string | null = null;
  if (forced === "light" || forced === "dark") mode = forced;
  else if (forced === "glass") { mode = "dark"; dark = "glass"; }
  else if (forced) pick = forced;
  const fromPick = pick ? map.get(pick) : undefined;
  const appearance: Appearance = fromPick ? fromPick.appearance : mode === "system" ? (query.matches ? "dark" : "light") : mode;
  const wanted = appearance === "dark" ? dark : light;
  const fallback = map.get(appearance === "dark" ? DEFAULT_DARK : DEFAULT_LIGHT)!;
  let active = fromPick ?? map.get(wanted);
  if (!active || active.appearance !== appearance) {
    // The user themes have not been listed yet and the boot cache says this id was on screen: keep it (its CSS came from the cache).
    const cached = bootCache()[appearance];
    active = !listed && !map.has(wanted) && cached?.id === wanted ? { ...fallback, id: wanted } : fallback;
  }
  return { choice: { mode, light, dark }, themes: ordered(map), problems, themesDir, active, appearance };
}

// ---- applying --------------------------------------------------------------

function userCss(): string {
  const data = userThemes.map((t) => t.data);
  return data.length ? themesCss(data, { user: true }) : "";
}

function writeStyle() {
  let style = document.getElementById(STYLE_ID) as HTMLStyleElement | null;
  const css = userCss();
  if (!css) {
    // Only drop the boot script's copy once the daemon has said there are none.
    if (style && listed) style.remove();
    return;
  }
  if (!style) {
    style = document.createElement("style");
    style.id = STYLE_ID;
    document.head.appendChild(style);
  }
  if (style.textContent !== css) style.textContent = css;
}

function writeBootCache() {
  // What public/theme-boot.js needs for either appearance: the id, whether it
  // has a backdrop, its page colour, and the CSS when it is not a built-in.
  const map = registry();
  const entry = (appearance: Appearance, id: string) => {
    const theme = map.get(id);
    const usable = theme && theme.appearance === appearance ? theme : map.get(appearance === "dark" ? DEFAULT_DARK : DEFAULT_LIGHT)!;
    const css = usable.source === "builtin" ? "" : themesCss([usable.data], { user: true });
    return { id: usable.id, backdrop: Boolean(usable.data.backdrop), bg: resolveColors(usable.data).background, css };
  };
  try {
    localStorage.setItem(BOOT, JSON.stringify({ light: entry("light", choice.light), dark: entry("dark", choice.dark) }));
  } catch {
    /* ignore */
  }
}

function apply() {
  snapshot = compute();
  const { active, appearance } = snapshot;
  const root = document.documentElement;
  writeStyle();
  root.setAttribute("data-neko-theme", active.id);
  root.setAttribute("data-appearance", appearance);
  root.classList.toggle("dark", appearance === "dark");
  root.classList.toggle("light", appearance === "light");
  root.toggleAttribute("data-backdrop", Boolean(active.data.backdrop));
  root.style.colorScheme = appearance;
  document.querySelector('meta[name="theme-color"]')?.setAttribute("content", resolveColors(active.data).background);
  if (forced === null) writeBootCache();
  listeners.forEach((l) => l());
}

function persist() {
  try {
    localStorage.setItem(KEY, JSON.stringify(choice));
  } catch {
    /* ignore */
  }
}

// ---- the daemon ------------------------------------------------------------

const asChoice = (a: any): Choice | null => (a && isMode(a.mode) && isId(a.light) && isId(a.dark) ? { mode: a.mode, light: a.light, dark: a.dark } : null);

function adopt(next: Choice | null) {
  if (!next || (next.mode === choice.mode && next.light === choice.light && next.dark === choice.dark)) return;
  choice = next;
  persist();
  apply();
}

async function loadAppearance() {
  try {
    const reply = (await request("GetAppearance"))?.Appearance;
    if (!reply) return;
    if (reply.stored) {
      adopt(asChoice(reply));
      return;
    }
    // Nothing saved on the daemon yet: the first browser to arrive hands over its own old setting.
    if (localStorage.getItem(SEED) === "1") await save(choice);
  } catch {
    /* the daemon is unreachable: the local choice stands */
  } finally {
    try {
      localStorage.removeItem(SEED);
    } catch {
      /* ignore */
    }
  }
}

async function save(next: Partial<Choice>) {
  try {
    const body = { mode: next.mode ?? null, light: next.light ?? null, dark: next.dark ?? null };
    await request({ SetAppearance: body });
  } catch {
    /* kept locally; the next change tries again */
  }
}

function takeListing(reply: any) {
  const body = reply?.Themes;
  if (!body) return;
  const key = JSON.stringify(body);
  if (listed && key === listingKey) return;
  listingKey = key;
  const accepted: ThemeInfo[] = [];
  const bad: ThemeProblem[] = (body.problems ?? []).map((p: ThemeProblem) => ({ file: p.file, message: p.message }));
  for (const entry of body.themes ?? []) {
    // The daemon has checked it; the page checks again so it never builds CSS from something it does not understand.
    const message = validateTheme(entry.theme, entry.id);
    if (message) bad.push({ file: entry.file, message });
    else accepted.push(info(entry.theme as ThemeData, "user", { file: entry.file }));
  }
  userThemes = accepted;
  problems = bad;
  themesDir = typeof body.dir === "string" ? body.dir : themesDir;
  listed = true;
  apply();
}

async function refreshThemes() {
  try {
    takeListing(await request("ListThemes"));
  } catch {
    /* unreachable: keep what we have */
  }
}

let polling: ReturnType<typeof setInterval> | undefined;

function startPolling() {
  if (polling) return;
  // The same shape as the plugins loader: a short poll while the page is visible.
  polling = setInterval(() => {
    if (!document.hidden) void refreshThemes();
  }, 3000);
  document.addEventListener("visibilitychange", () => {
    if (!document.hidden) void refreshThemes();
  });
}

// ---- the public surface ----------------------------------------------------

export function initTheme() {
  snapshot = compute();
  apply();
  query.addEventListener("change", () => {
    apply();
  });
  window.addEventListener("storage", (event) => {
    if (event.key === KEY) adopt(readChoice());
  });
  onEvent((event) => {
    const changed = (event as any)?.AppearanceChanged;
    if (changed) adopt(asChoice(changed));
  });
  void loadAppearance();
  void refreshThemes();
  startPolling();
}

export function setMode(mode: Mode) {
  choice = { ...choice, mode };
  persist();
  apply();
  void save({ mode });
}

/** Picks the theme for light or for dark. */
export function setSlot(slot: Appearance, id: string) {
  choice = { ...choice, [slot]: id };
  persist();
  apply();
  void save({ [slot]: id });
}

/** Deletes a theme file of the person's. A built-in that it overrode comes back. */
export async function removeUserTheme(id: string) {
  const reply = await request({ RemoveTheme: { id } });
  takeListing(reply);
}

export const getSnapshot = () => snapshot;
export const getChoice = () => snapshot.choice;
/** The appearance on screen: "light" or "dark". */
export const getResolvedTheme = (): Appearance => snapshot.appearance;
/** What `host.theme.get()` returns. */
export const getThemeState = () => ({ mode: snapshot.choice.mode, appearance: snapshot.appearance, id: snapshot.active.id, name: snapshot.active.name });

export function subscribeTheme(listener: () => void) {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

/** Everything the pickers need, current: the choice, every theme, the files that were skipped, the one on screen. */
export function useThemes(): Snapshot {
  return useSyncExternalStore(subscribeTheme, getSnapshot);
}
