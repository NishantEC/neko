// The host: the one object every page is registered with, Neko's own and yours.
//
//   export default function register(host) {            // a user page (index.js)
//     host.registerPage({ id: "board", title: "PR board", component: Board });
//   }
//   host.registerPage({ id: "tasks", component: MyTasks });   // same id as a built-in: replaces it
//   host.registerPage({ id: "tasks", wrap: (Original) => (props) => … });  // or wraps it
//
// Neko's own pages (pages/index.ts) call exactly this function on the built-in
// host, and read the same `ui`, `icons`, `storage`, `navigation` and `request`
// a user page does. Anything a built-in needs from the shell is added here, so
// a page you write is never a weaker kind of page.
//
//   React, request, onEvent          the app's own copies (no bundler needed)
//   registerPage(spec)               component | load | wrap, see PageSpec
//   registerCommands([…])            ⌘K entries, while the page is loaded
//   inspector                        the resizable panel on the right
//   ui, icons                        Neko's components and Phosphor icons
//   storage                          get/set/delete/list JSON, kept by the daemon
//   theme                            get() the theme on screen, subscribe(fn) to changes
//   navigation                       navigate(route), hrefFor(route), useRoute()
//   addSection / addTab              add to Settings / to the task sheet
import * as React from "react";
import { createContext, lazy, useContext, type ComponentType } from "react";
import { request, onEvent } from "@/api";
import { registerCommands, type PaletteCommand } from "@/shell/commands";
import { inspector } from "@/shell/inspector";
import { pluginIcons, pluginUi } from "@/pluginKit";
import { addSettingsSection, addTaskTab, type SettingsSectionExt, type TaskTabExt } from "@/extensions";
import { HOME, hrefFor, navigate, useRoute } from "@/router";
import { storageFor, type PageStorage } from "@/storage";
import { getThemeState, subscribeTheme } from "@/theme";
import { PuzzlePiece, type Icon } from "@/ui/icons";
import { registerBuiltin, type PageComponent, type PageGroup, type UserRegistration } from "@/registry";

export interface PageSpec {
  /** A new id is a page of its own; a built-in's id (`tasks`, `settings`, …) replaces or wraps that page. */
  id: string;
  title?: string;
  /** A Phosphor icon, by name (`"Heart"`) or the component. */
  icon?: string | Icon;
  keywords?: string[];
  /** Sidebar group: `main`, `work` or `system`. Defaults to the manifest's `group`, then `main`. */
  group?: PageGroup;
  /** Exactly one of these. */
  component?: ComponentType<any>;
  /** Lazy: the module's default export is the page. */
  load?: () => Promise<{ default: ComponentType<any> }>;
  /** Only for a built-in's id: receives the original component, returns the one to show. */
  wrap?: (Original: PageComponent, host: Host) => ComponentType<any>;
}

export interface Host {
  /** `neko` for the app's own pages, otherwise the plugin's folder name. */
  id: string;
  source: "builtin" | "user";
  React: typeof React;
  request: typeof request;
  onEvent: typeof onEvent;
  registerPage(spec: PageSpec): void;
  /** Adds entries to ⌘K until the plugin is switched off, reloaded or removed. */
  registerCommands(list: PaletteCommand[]): void;
  inspector: typeof inspector;
  /** Neko's own components (Button, Input, Panel, Badge, Tabs, Table, Dialog, …). */
  ui: Record<string, any>;
  /** Phosphor icons by name (`icons.Heart`). */
  icons: Record<string, any>;
  /** JSON values kept by the daemon, one namespace per plugin. */
  storage: PageStorage;
  /** The theme on screen: `get()` is `{ mode, appearance, id, name }`; `subscribe(fn)` runs on any change and returns an unsubscribe. Read colours with `var(--background)` etc.; this is for code that cannot (a canvas, a chart). */
  theme: { get: typeof getThemeState; subscribe: typeof subscribeTheme };
  navigation: { navigate: typeof navigate; hrefFor: typeof hrefFor; useRoute: typeof useRoute; HOME: string };
  /** Adds a section to Settings. */
  addSection(section: SettingsSectionExt): void;
  /** Adds a tab to the task sheet. */
  addTab(tab: TaskTabExt): void;
}

const resolveIcon = (icon: PageSpec["icon"]): Icon => (typeof icon === "string" ? (pluginIcons[icon] as Icon | undefined) : icon) ?? PuzzlePiece;

function shared(id: string, source: Host["source"], storage: PageStorage) {
  return {
    id,
    source,
    React,
    request,
    onEvent,
    inspector,
    ui: pluginUi,
    icons: pluginIcons,
    storage,
    theme: { get: getThemeState, subscribe: subscribeTheme },
    navigation: { navigate, hrefFor, useRoute, HOME },
  };
}

/** The host Neko's own pages register with (pages/index.ts). */
export const builtinHost: Host = (() => {
  const host: Host = {
    ...shared("neko", "builtin", storageFor("_neko")),
    registerPage(spec) {
      if (!spec.title) throw new Error(`A built-in page needs a title (“${spec.id}”).`);
      const component: PageComponent = spec.load ? lazy(spec.load) : spec.component!;
      registerBuiltin({
        id: spec.id,
        title: spec.title,
        icon: resolveIcon(spec.icon),
        keywords: spec.keywords ?? [],
        group: spec.group ?? "main",
        component,
        host,
        source: "builtin",
      });
    },
    registerCommands,
    addSection: (section) => void addSettingsSection(section),
    addTab: (tab) => void addTaskTab(tab),
  };
  return host;
})();

export interface UserHost {
  host: Host;
  /** What the plugin registered; handed to the registry once its module has run. */
  pages: UserRegistration[];
  /** Takes back the commands, sections and tabs it added. */
  dispose(): void;
}

/** A plugin's host: the same shape as the built-in one, namespaced to its folder. */
export function createUserHost(plugin: string, defaultGroup?: PageGroup): UserHost {
  const pages: UserRegistration[] = [];
  const removers: (() => void)[] = [];
  const host: Host = {
    ...shared(plugin, "user", storageFor(plugin)),
    registerPage(spec) {
      if (!spec || typeof spec.id !== "string" || !/^[A-Za-z0-9_-]+$/.test(spec.id)) {
        throw new Error("registerPage needs an id of letters, digits, - or _ (a built-in's id replaces that page).");
      }
      if (!spec.component && !spec.load && !spec.wrap) throw new Error(`Page “${spec.id}” needs a component, load or wrap.`);
      if (pages.some((p) => p.pageId === spec.id)) throw new Error(`Page “${spec.id}” is registered twice.`);
      pages.push({
        pageId: spec.id,
        title: spec.title,
        icon: spec.icon === undefined ? undefined : resolveIcon(spec.icon),
        keywords: spec.keywords,
        group: spec.group,
        defaultGroup,
        component: spec.load ? lazy(spec.load) : spec.component,
        wrap: spec.wrap,
        host,
      });
    },
    registerCommands(list) {
      // Namespaced by plugin, so two plugins cannot replace each other's commands.
      removers.push(registerCommands(list.map((c) => ({ ...c, id: `plugin:${plugin}:${c.id}`, group: c.group ?? "Yours" }))));
    },
    addSection: (section) => void removers.push(addSettingsSection(section)),
    addTab: (tab) => void removers.push(addTaskTab(tab)),
  };
  return {
    host,
    pages,
    dispose() {
      removers.splice(0).forEach((remove) => remove());
      pages.length = 0;
    },
  };
}

const HostContext = createContext<Host>(builtinHost);
export const HostProvider = HostContext.Provider;
/** The host the current page was registered with (also passed to it as the `host` prop). */
export const useHost = () => useContext(HostContext);
