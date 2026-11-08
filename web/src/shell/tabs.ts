// One window, opened as tabs (DESIGN.md §1). The address is still the hash
// (router.ts), so every link, `navigate(route)`, `host.navigation.navigate`
// and the browser's own back and forward keep working; this store watches the
// hash and keeps a strip of tabs in step with it:
//
//   - Every location has a key. A page is its route (`tasks`, `pr-board:board`);
//     a detail has its own: `#/tasks?task=<id>&view=full` is `task:<id>`,
//     `#/workspaces?workspace=<id>` is `workspace:<id>`, `#/artifact?id=<id>`
//     is `artifact:<id>`. Anything else in the query (`#/tasks?task=<id>`, the
//     sheet beside the list) belongs to the page's tab.
//   - When the hash changes, the tab with that key is focused and remembers the
//     new address; if none has it, a tab opens to the right of the current one.
//     Leaving the New tab launcher replaces it in place.
//   - Clicking a tab sets the hash to the address it remembers. Back and forward
//     are the browser's history (history.back/forward), so they walk the places
//     you visited across tabs; a closed tab's place reopens it.
//   - Home is pinned first and cannot be closed. The set persists in
//     localStorage (`neko.tabs.v1`): it is UI state, not the daemon's.
import { useSyncExternalStore } from "react";
import { HOME } from "@/router";

export interface Tab {
  /** Unique per tab (a duplicated tab has the same key, a different id). */
  id: string;
  key: string;
  /** The hash it shows, with its leading `#`. */
  href: string;
  pinned?: boolean;
}

interface State {
  tabs: Tab[];
  active: string;
  /** Position in this window's history, to know whether back / forward can go anywhere. */
  index: number;
  max: number;
}

const STORE = "neko.tabs.v1";
const SESSION = "neko.tabs.history";
export const NEW_TAB = "new";

let seq = 0;
const uid = () => `t${Date.now().toString(36)}${(seq++).toString(36)}`;

/** The route and query of a hash, read the way router.ts reads it. */
function parse(hash: string): { route: string; params: URLSearchParams } {
  const body = hash.replace(/^#\/?/, "");
  const [raw, query = ""] = body.split("?");
  let route = HOME;
  try {
    route = raw === "" ? HOME : decodeURIComponent(raw);
  } catch {
    route = raw;
  }
  return { route: route === "plugins" ? "customisations" : route, params: new URLSearchParams(query) };
}

/** Which tab a location belongs to. */
export function keyFor(hash: string): string {
  const { route, params } = parse(hash);
  if (route === "tasks" && params.get("task") && params.get("view") === "full") return `task:${params.get("task")}`;
  if (route === "workspaces" && params.get("workspace")) return `workspace:${params.get("workspace")}`;
  if (route === "artifact" && params.get("id")) return `artifact:${params.get("id")}`;
  return route;
}

const normal = (hash: string) => (hash === "" || hash === "#" ? "#/" : hash.startsWith("#") ? hash : `#${hash}`);
const homeTab = (): Tab => ({ id: "home", key: HOME, href: "#/", pinned: true });

function load(): Tab[] {
  try {
    const raw = JSON.parse(localStorage.getItem(STORE) ?? "null");
    const list: Tab[] = Array.isArray(raw?.tabs) ? raw.tabs : [];
    const tabs = list
      .filter((t) => t && typeof t.href === "string" && typeof t.id === "string")
      .map((t) => ({ id: t.id, href: normal(t.href), key: keyFor(t.href), pinned: !!t.pinned }))
      .filter((t) => t.key !== HOME && t.key !== NEW_TAB);
    return [homeTab(), ...tabs.slice(0, 40)];
  } catch {
    return [homeTab()];
  }
}

function readHistory(): { index: number; max: number } {
  try {
    const raw = JSON.parse(sessionStorage.getItem(SESSION) ?? "null");
    return { index: 0, max: typeof raw?.max === "number" ? raw.max : 0 };
  } catch {
    return { index: 0, max: 0 };
  }
}

let state: State = { tabs: [homeTab()], active: "home", ...readHistory() };
const listeners = new Set<() => void>();

function persist() {
  try {
    localStorage.setItem(STORE, JSON.stringify({ tabs: state.tabs.filter((t) => t.key !== NEW_TAB).map(({ id, href, pinned }) => ({ id, href, pinned })) }));
    sessionStorage.setItem(SESSION, JSON.stringify({ max: state.max }));
  } catch { /* private mode: tabs last for this page only */ }
}

function set(next: Partial<State>) {
  state = { ...state, ...next };
  persist();
  listeners.forEach((l) => l());
}

/** Pinned tabs first, Home first of all, the rest in their own order. */
function ordered(tabs: Tab[]): Tab[] {
  const home = tabs.filter((t) => t.key === HOME && t.pinned);
  return [...home, ...tabs.filter((t) => t.pinned && !home.includes(t)), ...tabs.filter((t) => !t.pinned)];
}

/** The hash changed (a link, navigate(), back/forward, a tab click): focus or open its tab. */
function follow() {
  const href = normal(location.hash);
  const key = keyFor(href);
  stampHistory();
  const tabs = [...state.tabs];
  const current = tabs.find((t) => t.id === state.active);
  // The active tab already shows this place (a query change, or its duplicate): it keeps it.
  let tab = current && current.key === key ? current : tabs.find((t) => t.key === key);
  if (tab) {
    const i = tabs.indexOf(tab);
    tab = { ...tab, href };
    tabs[i] = tab;
    // Leaving the launcher for a tab that was already open closes the launcher.
    const next = current && current.key === NEW_TAB && current.id !== tab.id ? tabs.filter((t) => t.id !== current.id) : tabs;
    set({ tabs: ordered(next), active: tab.id });
    return;
  }
  tab = { id: uid(), key, href };
  const at = current ? tabs.indexOf(current) : tabs.length - 1;
  if (current && current.key === NEW_TAB) tabs.splice(at, 1, tab);
  else tabs.splice(at + 1, 0, tab);
  set({ tabs: ordered(tabs), active: tab.id });
}

/** Marks each history entry with its position, so back / forward know where they can go. */
function stampHistory() {
  const stamp = (history.state as { nekoIndex?: number } | null)?.nekoIndex;
  if (typeof stamp === "number") {
    state = { ...state, index: stamp };
    return;
  }
  const index = state.index + 1;
  state = { ...state, index, max: index };
  try {
    history.replaceState({ ...(history.state ?? {}), nekoIndex: index }, "");
  } catch { /* a sandboxed frame */ }
}

let started = false;
/** Called once by the shell: restores the strip and starts following the hash. */
export function startTabs() {
  if (started) return;
  started = true;
  const tabs = load();
  const stamp = (history.state as { nekoIndex?: number } | null)?.nekoIndex;
  if (typeof stamp === "number") state = { ...state, index: stamp, max: Math.max(state.max, stamp) };
  else {
    state = { ...state, index: 0, max: 0 };
    try { history.replaceState({ ...(history.state ?? {}), nekoIndex: 0 }, ""); } catch { /* ignore */ }
  }
  state = { ...state, tabs, active: tabs[0].id };
  // Whatever the address says wins: it may be a link the app was opened at.
  const href = normal(location.hash);
  const key = keyFor(href);
  const existing = tabs.find((t) => t.key === key);
  if (existing) {
    existing.href = href;
    state = { ...state, active: existing.id };
  } else {
    const tab = { id: uid(), key, href };
    state = { ...state, tabs: ordered([...tabs, tab]), active: tab.id };
  }
  persist();
  addEventListener("hashchange", follow);
}

const go = (href: string) => {
  if (normal(location.hash) === href) follow();
  else location.hash = href.slice(1);
};

/** Shows a tab: its remembered address becomes the hash. */
export function activateTab(id: string) {
  const tab = state.tabs.find((t) => t.id === id);
  if (!tab) return;
  if (tab.id !== state.active) set({ active: tab.id });
  go(tab.href);
}

/** Opens `href` (a hash) in a tab of its own, even when one already shows it. */
export function openInNewTab(href: string) {
  const target = normal(href);
  const tab: Tab = { id: uid(), key: keyFor(target), href: target };
  const tabs = [...state.tabs];
  const at = tabs.findIndex((t) => t.id === state.active);
  tabs.splice(at + 1, 0, tab);
  set({ tabs: ordered(tabs), active: tab.id });
  go(target);
}

/** ⌘T: the launcher, or the one already open. */
export function newTab() {
  const open = state.tabs.find((t) => t.key === NEW_TAB);
  if (open) activateTab(open.id);
  else openInNewTab(`#/${NEW_TAB}`);
}

export function duplicateTab(id: string) {
  const tab = state.tabs.find((t) => t.id === id);
  if (tab) openInNewTab(tab.href);
}

export function closeTab(id: string) {
  const tabs = state.tabs;
  const i = tabs.findIndex((t) => t.id === id);
  const tab = tabs[i];
  if (!tab || (tab.pinned && tab.key === HOME)) return;
  const rest = tabs.filter((t) => t.id !== id);
  if (tab.id !== state.active) {
    set({ tabs: rest });
    return;
  }
  // The tab to its right takes its place, else the one to its left.
  const next = rest[Math.min(i, rest.length - 1)] ?? homeTab();
  set({ tabs: rest.length ? rest : [homeTab()], active: next.id });
  go(next.href);
}

/** Closes every tab but this one (and the pinned ones). */
export function closeOtherTabs(id: string) {
  const keep = state.tabs.filter((t) => t.id === id || t.pinned);
  const active = keep.some((t) => t.id === state.active) ? state.active : id;
  set({ tabs: keep, active });
  const tab = keep.find((t) => t.id === active);
  if (tab) go(tab.href);
}

export function setPinned(id: string, pinned: boolean) {
  const tab = state.tabs.find((t) => t.id === id);
  if (!tab || tab.key === HOME) return;
  set({ tabs: ordered(state.tabs.map((t) => (t.id === id ? { ...t, pinned } : t))) });
}

/** Moves a tab to `index` (drag, or the keyboard). Pinned and unpinned tabs stay in their own runs. */
export function moveTab(id: string, index: number) {
  const tabs = [...state.tabs];
  const from = tabs.findIndex((t) => t.id === id);
  if (from < 0 || tabs[from].key === HOME) return;
  const [tab] = tabs.splice(from, 1);
  tabs.splice(Math.max(1, Math.min(index, tabs.length)), 0, tab);
  set({ tabs: ordered(tabs) });
}

/** ⌘1…⌘8 the nth tab, ⌘9 the last, as in a browser. */
export function activateNth(n: number) {
  const tab = n >= 9 ? state.tabs[state.tabs.length - 1] : state.tabs[n - 1];
  if (tab) activateTab(tab.id);
}

/** The tab beside the active one (⌘⇧[ / ⌘⇧]). */
export function activateBeside(step: -1 | 1) {
  const i = state.tabs.findIndex((t) => t.id === state.active);
  const tab = state.tabs[(i + step + state.tabs.length) % state.tabs.length];
  if (tab) activateTab(tab.id);
}

/** Closes the tab the person is on (⌘W), or the given one. */
export function closeActiveTab() {
  closeTab(state.active);
}

export const canGoBack = () => state.index > 0;
export const canGoForward = () => state.index < state.max;
export const goBack = () => { if (canGoBack()) history.back(); };
export const goForward = () => { if (canGoForward()) history.forward(); };

export const getTabs = () => state;
export const subscribeTabs = (listener: () => void): (() => void) => subscribe(listener);
const subscribe = (listener: () => void) => {
  listeners.add(listener);
  return () => listeners.delete(listener);
};

export function useTabs(): State {
  return useSyncExternalStore(subscribe, getTabs);
}

/** The link to copy for a tab: the app's address with the tab's hash. */
export const tabLink = (tab: Tab) => `${location.origin}${location.pathname}${tab.href}`;
