// User pages: the loader. A user page is a directory in `<data dir>/plugins/<name>/`
// with a `manifest.json` and an ES module; the daemon lists and serves them from
// disk on every request, so adding or editing one needs no rebuild of this app.
//
//   manifest.json   { "title": "PR board", "entry": "index.js", "description": "…", "version": "0.1.0", "group": "work" }
//   index.js        export default function register(host) {
//                     host.registerPage({ id: "board", title: "PR board", component: Board });
//                   }
//
// There is no separate plugin world: the module gets the same `host` Neko's own
// pages are registered with (host.ts), and what it registers goes into the same
// registry (registry.ts), so its pages sit in the normal sidebar groups and in
// ⌘K. Use a built-in's id to replace or wrap that page; see host.ts and
// docs/design/one-registry.md. This file only fetches the list, imports the
// modules that changed, and manages them (enable, disable, remove, reload).
import { Component, useEffect, useSyncExternalStore, type ReactNode } from "react";
import type { Json } from "./api";
import { createUserHost, type UserHost } from "./host";
import { clearUserPages, loadResetOverrides, setUserPages, type PageGroup } from "./registry";

export interface PluginListing {
  id: string;
  title: string;
  entry: string;
  /** The entry file's mtime: changes when the module does, so the page reloads it. */
  version: number;
  /** The manifest's own "version" (for people). */
  manifest_version?: string;
  /** A `.disabled` marker file is in the plugin's folder: listed, never loaded. */
  disabled?: boolean;
  /** Absolute path of the plugin's folder. */
  path?: string;
  description?: string;
  /** The sidebar group the manifest asks for. */
  group?: PageGroup;
  /** Why the daemon could not read the manifest or entry. */
  error?: string;
}
type Listing = PluginListing;

/** Where a person's customisations live, as the daemon reports it. */
export interface CustomisationInfo {
  plugins_dir: string;
  css_path: string;
  css_exists: boolean;
  css_version: number;
}

export interface PluginState {
  /** Plugins that did not load: the daemon's manifest error, or what the module threw. */
  problems: { plugin: string; message: string }[];
  /** Everything installed, enabled or not, as the daemon last listed it. */
  listings: PluginListing[];
  /** True once the daemon has answered at least once. */
  listed: boolean;
  /** Set when the last listing request failed. */
  failed: boolean;
  /** The plugins folder and `custom.css`, once the daemon has said. */
  custom: CustomisationInfo | null;
}

let state: PluginState = { problems: [], listings: [], listed: false, failed: false, custom: null };
let listings: PluginListing[] = [];
let listed = false;
let failed = false;
let custom: CustomisationInfo | null = null;
const listeners = new Set<() => void>();
const loaded = new Map<string, number>();
const hosts = new Map<string, UserHost>();
const problemsByPlugin = new Map<string, string>();

function publish() {
  state = {
    problems: [...problemsByPlugin].map(([plugin, message]) => ({ plugin, message })),
    listings,
    listed,
    failed,
    custom,
  };
  listeners.forEach((l) => l());
}

function drop(id: string) {
  hosts.get(id)?.dispose();
  hosts.delete(id);
  clearUserPages(id);
}

async function load(listing: Listing) {
  drop(listing.id);
  const user = createUserHost(listing.id, listing.group);
  hosts.set(listing.id, user);
  try {
    // `?v=` is the module file's mtime: an edit is a new URL, so the browser
    // imports the new code instead of the module it already holds.
    const module = await import(/* @vite-ignore */ `${listing.entry}?v=${listing.version}`);
    if (typeof module.default !== "function") throw new Error("The module must export a default function register(host).");
    await module.default(user.host);
    setUserPages(listing.id, [...user.pages]);
    problemsByPlugin.delete(listing.id);
  } catch (e) {
    drop(listing.id);
    problemsByPlugin.set(listing.id, e instanceof Error ? e.message : String(e));
  }
  loaded.set(listing.id, listing.version);
}

// ---------------------------------------------------------------------------
// The person's own stylesheet (`<data dir>/custom.css`), applied live
// ---------------------------------------------------------------------------

const CSS_LINK = "neko-custom-css";

/** Keeps one <link> after the app's own styles, re-pointed whenever the file's mtime changes. */
function applyCustomCss(info: CustomisationInfo) {
  let link = document.getElementById(CSS_LINK) as HTMLLinkElement | null;
  if (!link) {
    link = document.createElement("link");
    link.id = CSS_LINK;
    link.rel = "stylesheet";
    document.head.appendChild(link);
  }
  const href = `/custom.css?v=${info.css_version}`;
  if (link.getAttribute("href") !== href) link.setAttribute("href", href);
  keepLast(link);
}

/** Page chunks add their own stylesheets as you navigate; yours has to stay after all of them to win. */
let observing = false;
function keepLast(link: HTMLLinkElement) {
  if (document.head.lastElementChild !== link) document.head.appendChild(link);
  if (observing) return;
  observing = true;
  new MutationObserver(() => {
    if (document.head.lastElementChild !== link) document.head.appendChild(link);
  }).observe(document.head, { childList: true });
}

async function readCustomisation(): Promise<CustomisationInfo | null> {
  try {
    const response = await fetch("/api/customisation");
    return response.ok ? ((await response.json()) as CustomisationInfo) : null;
  } catch {
    return null;
  }
}

let started = false;

/** Reads the plugin list from the daemon and (re)loads whatever changed. */
export async function refreshPlugins() {
  let latest: Listing[];
  try {
    const response = await fetch("/api/plugins");
    if (!response.ok) throw new Error(String(response.status));
    latest = await response.json();
  } catch {
    if (!failed) {
      failed = true;
      publish();
    }
    return;
  }
  let changed = failed || !listed;
  if (!started) {
    started = true;
    // Which overrides were reset is read before the first module loads, so a reset page never flashes its override.
    await loadResetOverrides();
  }
  const info = await readCustomisation();
  if (info) {
    applyCustomCss(info);
    if (JSON.stringify(info) !== JSON.stringify(custom)) changed = true;
    custom = info;
  }
  if (JSON.stringify(latest) !== JSON.stringify(listings)) changed = true;
  failed = false;
  listed = true;
  listings = latest;
  const present = new Set(latest.filter((l) => !l.disabled).map((l) => l.id));
  for (const id of [...loaded.keys()]) {
    if (!present.has(id)) {
      drop(id);
      loaded.delete(id);
      problemsByPlugin.delete(id);
      changed = true;
    }
  }
  for (const listing of latest) {
    if (listing.disabled) continue;
    if (listing.error) {
      if (problemsByPlugin.get(listing.id) !== listing.error) changed = true;
      drop(listing.id);
      problemsByPlugin.set(listing.id, listing.error);
      loaded.delete(listing.id);
    } else if (loaded.get(listing.id) !== listing.version) {
      await load(listing);
      changed = true;
    }
  }
  if (changed) publish();
}

const subscribePlugins = (listener: () => void) => {
  listeners.add(listener);
  return () => listeners.delete(listener);
};

let watchers = 0;
let timer: ReturnType<typeof setInterval> | undefined;

/** Plugin listings and load problems, kept current by one shared 3 s poll. */
export function usePlugins(): PluginState {
  useEffect(() => {
    if (watchers++ === 0) {
      void refreshPlugins();
      timer = setInterval(refreshPlugins, 3000);
    }
    return () => {
      if (--watchers === 0) clearInterval(timer);
    };
  }, []);
  return useSyncExternalStore(subscribePlugins, () => state);
}

/** A page that throws shows its error rather than taking the app down. */
export class PluginBoundary extends Component<{ plugin: string; children: ReactNode }, { error: Json | null }> {
  state = { error: null };
  static getDerivedStateFromError(error: unknown) {
    return { error: error instanceof Error ? error.message : String(error) };
  }
  render() {
    if (this.state.error) {
      return (
        <div role="alert" className="mx-auto w-full max-w-[728px] px-6 py-10">
          <h2 className="text-lg font-semibold text-strong">Your “{this.props.plugin}” page crashed</h2>
          <pre className="mt-3 whitespace-pre-wrap rounded-xl border border-border bg-foreground/[0.03] p-3 text-destructive">
            {String(this.state.error)}
          </pre>
        </div>
      );
    }
    return this.props.children;
  }
}

// ---------------------------------------------------------------------------
// Managing plugins (the Customisations page): enable, disable, remove, reload
// ---------------------------------------------------------------------------

async function manage(method: "POST" | "DELETE", path: string): Promise<void> {
  const response = await fetch(path, { method });
  if (!response.ok) {
    throw new Error(
      response.status === 404 ? "That plugin is no longer installed."
      : response.status === 401 || response.status === 403 ? "This page is not signed in to the Neko daemon."
      : `The daemon answered ${response.status}.`,
    );
  }
  await refreshPlugins();
}

/** Switches a plugin on or off (a `.disabled` marker in its folder). Its pages go at once. */
export const setPluginEnabled = (id: string, enabled: boolean) => manage("POST", `/api/plugins/${encodeURIComponent(id)}/${enabled ? "enable" : "disable"}`);

/** Deletes the plugin's folder. Ask first: this cannot be undone. */
export const removePlugin = (id: string) => manage("DELETE", `/api/plugins/${encodeURIComponent(id)}`);

/** Imports one plugin's module again, even if its file did not change. */
export async function reloadPlugin(id?: string) {
  if (id) loaded.delete(id);
  else loaded.clear();
  await refreshPlugins();
}
