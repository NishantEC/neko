// The plugin seam. A plugin is a directory in `<data dir>/plugins/<name>/` with
// a `manifest.json` and an ES module; the daemon lists and serves them from
// disk on every request, so adding or editing one needs no rebuild of this app.
//
//   manifest.json   { "title": "PR board", "entry": "index.js", "description": "…", "version": "0.1.0" }
//   index.js        export default function register(host) {
//                     host.registerPage({ id: "board", title: "PR board", component: Board });
//                   }
//
// The module gets a `host` and nothing else: React (so it needs no bundler and
// shares one copy), `request` (the very function the app itself uses against
// POST /api/request), `onEvent`, `registerPage`, and two ways into the shell:
// `registerCommands([{ id, title, run }])` adds entries to ⌘K while the plugin
// is loaded, and `inspector.open({ title, children })` / `inspector.close()`
// uses the resizable panel on the right. A page is an ordinary React component.

import * as React from "react";
import { Component, useEffect, useSyncExternalStore, type ComponentType, type ReactNode } from "react";
import { request, onEvent, type Json } from "./api";
import { registerCommands, type PaletteCommand } from "./shell/commands";
import { inspector } from "./shell/inspector";

export interface PluginPage {
  id: string;
  title: string;
  component: ComponentType;
}

export interface PluginHost {
  React: typeof React;
  request: typeof request;
  onEvent: typeof onEvent;
  registerPage(page: PluginPage): void;
  /** Adds entries to the ⌘K palette until the plugin is switched off, reloaded or removed. */
  registerCommands(list: PaletteCommand[]): void;
  inspector: typeof inspector;
}

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
  /** Why the daemon could not read the manifest or entry. */
  error?: string;
}
type Listing = PluginListing;

export interface RegisteredPage extends PluginPage {
  key: string;
  plugin: string;
}

export interface PluginState {
  pages: RegisteredPage[];
  /** Plugins that did not load: the daemon's manifest error, or what the module threw. */
  problems: { plugin: string; message: string }[];
  /** Everything installed, enabled or not, as the daemon last listed it. */
  listings: PluginListing[];
  /** True once the daemon has answered at least once. */
  listed: boolean;
  /** Set when the last listing request failed. */
  failed: boolean;
}

let state: PluginState = { pages: [], problems: [], listings: [], listed: false, failed: false };
let listings: PluginListing[] = [];
let listed = false;
let failed = false;
const listeners = new Set<() => void>();
const loaded = new Map<string, number>();
const pagesByPlugin = new Map<string, RegisteredPage[]>();
const problemsByPlugin = new Map<string, string>();

function publish() {
  state = {
    pages: [...pagesByPlugin.values()].flat().sort((a, b) => a.title.localeCompare(b.title)),
    problems: [...problemsByPlugin].map(([plugin, message]) => ({ plugin, message })),
    listings,
    listed,
    failed,
  };
  listeners.forEach((l) => l());
}

const removers = new Map<string, (() => void)[]>();
function dropCommands(id: string) {
  removers.get(id)?.forEach((remove) => remove());
  removers.delete(id);
}

async function load(listing: Listing) {
  dropCommands(listing.id);
  const registered: RegisteredPage[] = [];
  const host: PluginHost = {
    React,
    request,
    onEvent,
    registerPage: (page) => registered.push({ ...page, key: `${listing.id}:${page.id}`, plugin: listing.id }),
    registerCommands: (list) => {
      // Namespaced by plugin, so two plugins cannot replace each other's commands.
      const own = list.map((c) => ({ ...c, id: `plugin:${listing.id}:${c.id}`, group: c.group ?? "Plugins" }));
      removers.set(listing.id, [...(removers.get(listing.id) ?? []), registerCommands(own)]);
    },
    inspector,
  };
  try {
    // `?v=` is the module file's mtime: an edit is a new URL, so the browser
    // imports the new code instead of the module it already holds.
    const module = await import(/* @vite-ignore */ `${listing.entry}?v=${listing.version}`);
    if (typeof module.default !== "function") throw new Error("The module must export a default function register(host).");
    await module.default(host);
    pagesByPlugin.set(listing.id, registered);
    problemsByPlugin.delete(listing.id);
  } catch (e) {
    pagesByPlugin.delete(listing.id);
    problemsByPlugin.set(listing.id, e instanceof Error ? e.message : String(e));
  }
  loaded.set(listing.id, listing.version);
}

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
  let changed = failed || !listed || JSON.stringify(latest) !== JSON.stringify(listings);
  failed = false;
  listed = true;
  listings = latest;
  const present = new Set(latest.filter((l) => !l.disabled).map((l) => l.id));
  for (const id of [...loaded.keys()]) {
    if (!present.has(id)) {
      dropCommands(id);
      loaded.delete(id);
      pagesByPlugin.delete(id);
      problemsByPlugin.delete(id);
      changed = true;
    }
  }
  for (const listing of latest) {
    if (listing.disabled) continue;
    if (listing.error) {
      if (problemsByPlugin.get(listing.id) !== listing.error) changed = true;
      pagesByPlugin.delete(listing.id);
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

/** Registered pages and load problems, kept current by a 3 s poll. */
export function usePlugins(): PluginState {
  useEffect(() => {
    void refreshPlugins();
    const timer = setInterval(refreshPlugins, 3000);
    return () => clearInterval(timer);
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
          <h2 className="text-lg font-semibold text-strong">The “{this.props.plugin}” plugin crashed</h2>
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
// Managing plugins (the Plugins page): enable, disable, remove, reload
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

/** The folder plugins live in, as the daemon reports it. */
export async function pluginFolder(): Promise<string | null> {
  try {
    const response = await fetch("/api/plugin-folder");
    return response.ok ? (await response.json()).path : null;
  } catch {
    return null;
  }
}
