// The one page registry. Neko's own pages (pages/index.ts) and the pages a
// person or an agent writes into the plugins folder register through the same
// `host.registerPage`, and the sidebar, ⌘K and the router read this one list.
//
//   - A built-in is registered once, at startup, with the built-in host.
//   - A user page with a NEW id is a page of its own, at `#/<plugin>:<id>`, in
//     the sidebar group its manifest (or `group:`) names.
//   - A user page whose id is a built-in's id REPLACES that built-in (`component`)
//     or WRAPS it (`wrap(Original, host)` returns the component to show). The
//     replaced page is marked "Customised"; "Reset to original" switches the
//     override off (kept in the app's storage, so it survives a reload) and
//     "Use my version" switches it back on. Nothing of the original is touched.
import { useSyncExternalStore, type ComponentType, type LazyExoticComponent } from "react";
import { PuzzlePiece, type Icon } from "@/ui/icons";
import type { Host } from "@/host";
import { appStorage } from "@/storage";

export type PageGroup = "main" | "work" | "system";
export type PageComponent = ComponentType<any> | LazyExoticComponent<ComponentType<any>>;

export interface PageDef {
  /** The route: `tasks`, or `<plugin>:<id>` for a user page. */
  id: string;
  title: string;
  icon: Icon;
  keywords: string[];
  group: PageGroup;
  component: PageComponent;
  /** The host the page was registered with; it is handed to the page as `host`. */
  host: Host;
  source: "builtin" | "user";
  /** The plugin folder a user page (or the override of a built-in) came from. */
  plugin?: string;
  /** A user page is replacing or wrapping this built-in right now. */
  customised?: boolean;
  /** A user override exists for this built-in but was reset: the original is showing. */
  overrideOff?: boolean;
}

export interface UserRegistration {
  /** The id the plugin gave the page. */
  pageId: string;
  title?: string;
  icon?: Icon;
  keywords?: string[];
  group?: PageGroup;
  /** The manifest's group: for pages of its own only. A replaced built-in keeps its own group unless `group` says otherwise. */
  defaultGroup?: PageGroup;
  component?: PageComponent;
  wrap?: (Original: PageComponent, host: Host) => ComponentType<any>;
  host: Host;
}

/** What the Customisations page lists: every user page, and every override. */
export interface UserEntry {
  plugin: string;
  pageId: string;
  title: string;
  /** The route that opens it. */
  route: string;
  /** The built-in it replaces or wraps, when it does. */
  overrides: string | null;
  wraps: boolean;
  /** An override that was reset to the original. */
  off: boolean;
}

const builtins = new Map<string, PageDef>();
const userPages = new Map<string, UserRegistration[]>();
const suppressed = new Set<string>();
const listeners = new Set<() => void>();
let snapshot: PageDef[] = [];
let entries: UserEntry[] = [];

const wrapped = new WeakMap<UserRegistration, { original: PageComponent; component: PageComponent }>();

function wrappedFor(reg: UserRegistration, original: PageComponent): PageComponent {
  const hit = wrapped.get(reg);
  if (hit && hit.original === original) return hit.component;
  let component: PageComponent = original;
  try {
    component = reg.wrap!(original, reg.host);
  } catch (error) {
    console.error(`neko: ${reg.host.id} could not wrap “${reg.pageId}”; showing the original.`, error);
  }
  wrapped.set(reg, { original, component });
  return component;
}

function recompute() {
  const regs = [...userPages.entries()].sort(([a], [b]) => a.localeCompare(b)).flatMap(([plugin, list]) => list.map((reg) => ({ plugin, reg })));
  const overrides = new Map<string, { plugin: string; reg: UserRegistration }>();
  for (const item of regs) if (builtins.has(item.reg.pageId)) overrides.set(item.reg.pageId, item);

  const pages: PageDef[] = [];
  for (const base of builtins.values()) {
    const over = overrides.get(base.id);
    if (!over) pages.push(base);
    else if (suppressed.has(base.id)) pages.push({ ...base, overrideOff: true, plugin: over.plugin });
    else {
      const { reg } = over;
      pages.push({
        ...base,
        title: reg.title ?? base.title,
        icon: reg.icon ?? base.icon,
        keywords: reg.keywords ?? base.keywords,
        group: reg.group ?? base.group,
        component: reg.component ?? wrappedFor(reg, base.component),
        host: reg.host,
        plugin: over.plugin,
        customised: true,
      });
    }
  }
  const own = regs.filter(({ reg }) => !builtins.has(reg.pageId) && reg.component).map(({ plugin, reg }): PageDef => ({
    id: `${plugin}:${reg.pageId}`,
    title: reg.title ?? reg.pageId,
    icon: reg.icon ?? PuzzlePiece,
    keywords: reg.keywords ?? [],
    group: reg.group ?? reg.defaultGroup ?? "main",
    component: reg.component!,
    host: reg.host,
    source: "user",
    plugin,
  }));
  own.sort((a, b) => a.title.localeCompare(b.title));
  snapshot = [...pages, ...own];
  entries = regs.map(({ plugin, reg }) => {
    const base = builtins.get(reg.pageId);
    return {
      plugin,
      pageId: reg.pageId,
      title: reg.title ?? base?.title ?? reg.pageId,
      route: base ? base.id : `${plugin}:${reg.pageId}`,
      overrides: base ? base.id : null,
      wraps: !!reg.wrap,
      off: !!base && suppressed.has(base.id),
    };
  });
}

function publish() {
  recompute();
  listeners.forEach((l) => l());
}

/** A built-in page, registered once at startup through the built-in host. */
export function registerBuiltin(def: PageDef) {
  builtins.set(def.id, def);
  publish();
}

/** Replaces everything one plugin registered (called after its module ran). */
export function setUserPages(plugin: string, list: UserRegistration[]) {
  userPages.set(plugin, list);
  publish();
}

export function clearUserPages(plugin: string) {
  if (userPages.delete(plugin)) publish();
}

export const getPages = () => snapshot;
export const getUserEntries = () => entries;
export const pageById = (id: string) => snapshot.find((p) => p.id === id);

const subscribe = (listener: () => void) => {
  listeners.add(listener);
  return () => listeners.delete(listener);
};

/** Every page, built-in and yours, in sidebar order. */
export function usePages(): PageDef[] {
  return useSyncExternalStore(subscribe, getPages);
}

export function useUserEntries(): UserEntry[] {
  return useSyncExternalStore(subscribe, getUserEntries);
}

// ---------------------------------------------------------------------------
// Resetting an override (kept in the app's own storage namespace)
// ---------------------------------------------------------------------------

const OFF_KEY = "overrides-off";

/** Reads which overrides were reset. Done before the first plugin loads, so a reset page never flashes its override. */
export async function loadResetOverrides(): Promise<void> {
  try {
    const list = await appStorage.get<string[]>(OFF_KEY, []);
    suppressed.clear();
    for (const id of Array.isArray(list) ? list : []) if (typeof id === "string") suppressed.add(id);
  } catch { /* the daemon did not answer: every override stays on */ }
  publish();
}

/** Shows the original page again (`off`), or the person's version again. */
export async function setOverrideOff(id: string, off: boolean): Promise<void> {
  if (off) suppressed.add(id);
  else suppressed.delete(id);
  publish();
  try {
    await appStorage.set(OFF_KEY, [...suppressed].sort());
  } catch (error) {
    if (off) suppressed.delete(id);
    else suppressed.add(id);
    publish();
    throw error;
  }
}
