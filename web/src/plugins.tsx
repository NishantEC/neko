// The plugin seam. A plugin is a directory in `<data dir>/plugins/<name>/` with
// a `manifest.json` and an ES module; the daemon lists and serves them from
// disk on every request, so adding or editing one needs no rebuild of this app.
//
//   manifest.json   { "title": "PR board", "entry": "index.js", "description": "…" }
//   index.js        export default function register(host) {
//                     host.registerPage({ id: "board", title: "PR board", component: Board });
//                   }
//
// The module gets a `host` and nothing else: React (so it needs no bundler and
// shares one copy), `request` (the very function the app itself uses against
// POST /api/request) and `registerPage`. A page is an ordinary React component.

import * as React from "react";
import { Component, useEffect, useSyncExternalStore, type ComponentType, type ReactNode } from "react";
import { request, onEvent, type Json } from "./api";

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
}

interface Listing {
  id: string;
  title: string;
  entry: string;
  version: number;
  description?: string;
  error?: string;
}

export interface RegisteredPage extends PluginPage {
  key: string;
  plugin: string;
}

export interface PluginState {
  pages: RegisteredPage[];
  problems: { plugin: string; message: string }[];
}

let state: PluginState = { pages: [], problems: [] };
const listeners = new Set<() => void>();
const loaded = new Map<string, number>();
const pagesByPlugin = new Map<string, RegisteredPage[]>();
const problemsByPlugin = new Map<string, string>();

function publish() {
  state = {
    pages: [...pagesByPlugin.values()].flat().sort((a, b) => a.title.localeCompare(b.title)),
    problems: [...problemsByPlugin].map(([plugin, message]) => ({ plugin, message })),
  };
  listeners.forEach((l) => l());
}

async function load(listing: Listing) {
  const registered: RegisteredPage[] = [];
  const host: PluginHost = {
    React,
    request,
    onEvent,
    registerPage: (page) => registered.push({ ...page, key: `${listing.id}:${page.id}`, plugin: listing.id }),
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
  let listings: Listing[];
  try {
    const response = await fetch("/api/plugins");
    if (!response.ok) return;
    listings = await response.json();
  } catch {
    return;
  }
  let changed = false;
  const present = new Set(listings.map((l) => l.id));
  for (const id of [...loaded.keys()]) {
    if (!present.has(id)) {
      loaded.delete(id);
      pagesByPlugin.delete(id);
      problemsByPlugin.delete(id);
      changed = true;
    }
  }
  for (const listing of listings) {
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
