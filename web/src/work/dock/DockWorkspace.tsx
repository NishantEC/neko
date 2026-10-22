// A docking layout: every view is a tab, tabs can be dragged into any pane,
// dropped on an edge to split, dropped on another tab strip to share it,
// reordered, closed and reopened. Built on dockview (MIT, `dockview-react`);
// drag and drop, resizing, maximise and floating groups are dockview's own.
//
//   const dock = useDockController();
//   <DockWorkspace
//     storageKey="neko:dock:task"
//     controller={dock}
//     panels={[{ id: "chat", title: "Chat", icon: ChatCircle, render: () => <Chat/> }, …]}
//     defaultLayout={[{ panels: ["chat"], size: 0.35 }, { panels: ["changes", "plan"], of: 0, direction: "right" }]}
//   />
//   dock.open("plan"); dock.reset(); useOpenPanels(dock)  // for a "View" menu
//
// What this adds on top of dockview:
// - panels are described once ({ id, title, icon, render, closable }) and their
//   `render` is read fresh on every render, so a panel always shows current props;
// - panels keep their DOM when hidden behind another tab (renderer "always"), so
//   a conversation keeps its scroll and a composer its draft;
// - ONE saved layout per `storageKey` (not per record), checked before it is
//   trusted: unknown panels are dropped, panels that are missing and were not
//   closed are put back into the first group, anything unreadable falls back to
//   the default, so the dock can never come up empty;
// - below `narrowQuery` it is a single tabbed group (no splitting), and the
//   wide layout comes back untouched when the window grows again.
import { createPortal } from "react-dom";
import {
  createContext, useCallback, useLayoutEffect, useContext, useEffect, useReducer, useRef, useState, useSyncExternalStore,
  type ReactNode,
} from "react";
import {
  DockviewReact,
  type DockviewApi,
  type DockviewReadyEvent,
  type IDockviewHeaderActionsProps,
  type IDockviewPanelHeaderProps,
  type IDockviewPanelProps,
} from "dockview-react";
import "dockview-react/dist/styles/dockview.css";
import { ArrowsInSimple, ArrowsOutSimple } from "@phosphor-icons/react";
import { X, type Icon } from "@/ui/icons";
import { cn } from "@/ui/lib/utils";
import { LAYOUT_VERSION, readSaved, validateSaved, writeSaved, type DockGroupSpec } from "./dockLayout";
import "./dock.css";

export type { DockGroupSpec } from "./dockLayout";

export interface DockPanelDef {
  id: string;
  title: string;
  icon?: Icon;
  /** Called on every render of the panel; keep it cheap and read current props. */
  render: () => ReactNode;
  /** Shows a close button on the tab. Default true; a closed panel is reopened from `controller.open`. */
  closable?: boolean;
}

const COMPONENT = "panel";
const TAB = "tab";
const DEFAULT_NARROW_QUERY = "(max-width: 899px)";

interface DockImpl {
  open(id: string): void;
  close(id: string): void;
  reset(): void;
  splitRight(id?: string): void;
  isNarrow(): boolean;
}

/** What a page uses to drive its dock from outside (header menu, ⌘K). */
export class DockController {
  private impl: DockImpl | null = null;
  private ids: string[] = [];
  private listeners = new Set<() => void>();
  /** @internal */ bind(impl: DockImpl | null) { this.impl = impl; }
  /** @internal */ setOpen(next: string[]) {
    if (next.length === this.ids.length && next.every((id, i) => id === this.ids[i])) return;
    this.ids = next;
    this.listeners.forEach((l) => l());
  }
  subscribe = (listener: () => void) => {
    this.listeners.add(listener);
    return () => { this.listeners.delete(listener); };
  };
  getOpenIds = () => this.ids;
  isOpen(id: string) { return this.ids.includes(id); }
  /** Opens the panel (in the group it belongs to by default) or, if it is already open, brings it to the front. */
  open(id: string) { this.impl?.open(id); }
  close(id: string) { this.impl?.close(id); }
  toggle(id: string) { if (this.isOpen(id)) this.close(id); else this.open(id); }
  reset() { this.impl?.reset(); }
  /** Moves the active (or the given) panel into a new pane on the right. A no-op when it is alone in its pane or the dock is narrow. */
  splitRight(id?: string) { this.impl?.splitRight(id); }
  get narrow() { return this.impl?.isNarrow() ?? false; }
}

export function useDockController(): DockController {
  const ref = useRef<DockController | null>(null);
  if (!ref.current) ref.current = new DockController();
  return ref.current;
}

/** The ids of the panels that are open now, in the dock's own order. Re-renders when they change. */
export function useOpenPanels(controller: DockController): string[] {
  return useSyncExternalStore(controller.subscribe, controller.getOpenIds, controller.getOpenIds);
}

const PanelsContext = createContext<Map<string, DockPanelDef>>(new Map());

function PanelBody({ api }: IDockviewPanelProps) {
  const def = useContext(PanelsContext).get(api.id);
  return <div className="dock-panel-body">{def ? def.render() : null}</div>;
}

function DockTab({ api }: IDockviewPanelHeaderProps) {
  const [active, setActive] = useState(api.isActive);
  useEffect(() => {
    setActive(api.isActive);
    const sub = api.onDidActiveChange((e) => setActive(e.isActive));
    return () => sub.dispose();
  }, [api]);
  const def = useContext(PanelsContext).get(api.id);
  const [title, setTitle] = useState(api.title ?? api.id);
  useEffect(() => {
    setTitle(api.title ?? api.id);
    const sub = api.onDidTitleChange((e) => setTitle(e.title));
    return () => sub.dispose();
  }, [api]);
  const TabIcon = def?.icon;
  return (
    <div
      className="dock-tab"
      data-panel={api.id}
      data-iconless={TabIcon ? undefined : "true"}
      // An idle tab is its icon alone (hushbacks); the name is on hover and to assistive tech.
      title={title}
      onDoubleClick={() => (api.isMaximized() ? api.exitMaximized() : api.maximize())}
    >
      {TabIcon ? <TabIcon size={18} aria-hidden className="dock-tab-icon" weight={active ? "fill" : "regular"} /> : null}
      <span className="dock-tab-more">
        <span className="dock-tab-more-inner">
          <span className={cn("dock-tab-title", !TabIcon && "dock-tab-title-bare")}>{title}</span>
          {def?.closable === false ? null : (
            <button
              type="button"
              tabIndex={-1}
              className="dock-tab-close"
              aria-label={`Close ${title}`}
              title={`Close ${title}`}
              // The strip starts a drag on pointer-down; the close button must not.
              onPointerDown={(e) => e.stopPropagation()}
              onMouseDown={(e) => e.stopPropagation()}
              onDoubleClick={(e) => e.stopPropagation()}
              onClick={(e) => { e.stopPropagation(); api.close(); }}
            >
              <X size={13} weight="bold" aria-hidden />
            </button>
          )}
        </span>
      </span>
    </div>
  );
}

/**
 * The sliding fused indicator for ONE group (hushbacks useFusedTabIndicator, per group).
 * It is the card-coloured tab with its concave feet, drawn once behind the group's strip
 * and moved with a CSS transition; the active tab itself is transparent. It lives in a
 * portal inside the group's own header, so dockview re-rendering, moving or dragging tabs
 * never remounts it, and a group that appears starts with the indicator already in place.
 * Geometry is the SETTLED one (label open on the active tab, shut on the others) so the
 * slide goes straight to where the tab will end up while its label is still opening.
 */
function GroupIndicator({ group }: IDockviewHeaderActionsProps) {
  const [host, setHost] = useState<HTMLElement | null>(null);
  const ref = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    setHost((group.element as HTMLElement).querySelector<HTMLElement>(".dv-tabs-and-actions-container"));
  }, [group]);

  useLayoutEffect(() => {
    const indicator = ref.current;
    const root = group.element as HTMLElement;
    const strip = host?.querySelector<HTMLElement>(".dv-tabs-container");
    if (!host || !strip || !indicator) return;
    let last: { id: string; ids: string } | null = null;
    let raf = 0;
    const reduced = () => typeof matchMedia === "function" && matchMedia("(prefers-reduced-motion: reduce)").matches;

    const measure = () => {
      raf = 0;
      const tabs = [...strip.querySelectorAll<HTMLElement>(":scope > .dv-tab")];
      const active = tabs.find((t) => t.classList.contains("dv-active-tab"));
      if (!active || !active.offsetWidth) { indicator.style.opacity = "0"; return; }
      // settled geometry: each tab's collapsible part is at its final width
      let before = 0;
      let width = active.offsetWidth;
      for (const tab of tabs) {
        const more = tab.querySelector<HTMLElement>(".dock-tab-more");
        const inner = tab.querySelector<HTMLElement>(".dock-tab-more-inner");
        if (!more || !inner) continue;
        const open = tab.classList.contains("dv-active-tab") || tab.querySelector(".dock-tab")?.hasAttribute("data-iconless");
        const delta = more.getBoundingClientRect().width - (open ? inner.scrollWidth : 0);
        if (tab === active) width -= delta;
        else if (tabs.indexOf(tab) < tabs.indexOf(active)) before += delta;
      }
      const left = strip.offsetLeft + active.offsetLeft - before - strip.scrollLeft;
      const id = active.querySelector<HTMLElement>(".dock-tab")?.dataset.panel ?? "";
      const ids = tabs.map((t) => t.querySelector<HTMLElement>(".dock-tab")?.dataset.panel ?? "").join("|");
      // Slide only when the active tab changed inside the same set of tabs; anything else
      // (first paint, a tab moved or closed, a resize) just snaps.
      const slide = !!last && last.id !== id && last.ids === ids && !reduced();
      last = { id, ids };
      if (!slide) indicator.style.transition = "none";
      indicator.style.opacity = "1";
      indicator.style.transform = `translateX(${left}px)`;
      indicator.style.width = `${width}px`;
      if (!slide) { void indicator.offsetWidth; indicator.style.transition = ""; }
      root.toggleAttribute("data-dock-first-active", tabs[0] === active);
    };
    const schedule = () => { if (!raf) raf = requestAnimationFrame(measure); };

    measure();
    const ro = new ResizeObserver(schedule);
    ro.observe(host); ro.observe(strip);
    const mo = new MutationObserver(schedule);
    mo.observe(strip, { childList: true, subtree: true, attributes: true, attributeFilter: ["class"] });
    strip.addEventListener("scroll", schedule, { passive: true });
    // a tab's label opening or closing changes widths mid-transition: re-read each frame it moves
    const onTransition = (e: Event) => { if ((e.target as HTMLElement).classList?.contains("dock-tab-more")) schedule(); };
    strip.addEventListener("transitionrun", onTransition);
    strip.addEventListener("transitionend", onTransition);
    return () => {
      cancelAnimationFrame(raf);
      ro.disconnect(); mo.disconnect();
      strip.removeEventListener("scroll", schedule);
      strip.removeEventListener("transitionrun", onTransition);
      strip.removeEventListener("transitionend", onTransition);
    };
  }, [host, group]);

  if (!host) return null;
  return createPortal(<div ref={ref} className="dock-indicator" aria-hidden="true" style={{ opacity: 0 }} />, host);
}

function GroupActions({ api, containerApi, group }: IDockviewHeaderActionsProps) {
  const [, bump] = useReducer((n: number) => n + 1, 0);
  useEffect(() => {
    const sub = containerApi.onDidMaximizedGroupChange(bump);
    return () => sub.dispose();
  }, [containerApi]);
  if (group.api.location.type !== "grid") return null;
  // Maximising only makes sense when there is something to be maximised over.
  if (containerApi.groups.length < 2 && !api.isMaximized()) return null;
  const maximized = api.isMaximized();
  return (
    <div className="dock-group-actions">
      <button
        type="button"
        className="dock-group-action"
        aria-label={maximized ? "Restore pane size" : "Maximise pane"}
        title={maximized ? "Restore pane size" : "Maximise pane (double-click a tab)"}
        onClick={() => (maximized ? api.exitMaximized() : api.maximize())}
      >
        {maximized ? <ArrowsInSimple size={14} aria-hidden /> : <ArrowsOutSimple size={14} aria-hidden />}
      </button>
    </div>
  );
}

const DOCK_THEME = { name: "neko", className: "neko-dock" };
const COMPONENTS = { [COMPONENT]: PanelBody };
const TAB_COMPONENTS = { [TAB]: DockTab };

export interface DockWorkspaceProps {
  panels: DockPanelDef[];
  /** Groups to build when there is no (usable) saved layout, in order. */
  defaultLayout: DockGroupSpec[];
  /** localStorage key; one layout per key, shared by every record the page shows. */
  storageKey: string;
  controller?: DockController;
  /** Below this the dock is one tabbed group. Default `(max-width: 899px)`. */
  narrowQuery?: string;
  className?: string;
}

export function DockWorkspace({
  panels, defaultLayout, storageKey, controller, narrowQuery = DEFAULT_NARROW_QUERY, className,
}: DockWorkspaceProps) {
  const defs = useRef(new Map<string, DockPanelDef>());
  const nextDefs = new Map(panels.map((p) => [p.id, p]));
  defs.current = nextDefs;
  const specRef = useRef(defaultLayout);
  specRef.current = defaultLayout;
  const keyRef = useRef(storageKey);
  keyRef.current = storageKey;

  const apiRef = useRef<DockviewApi | null>(null);
  const loadingRef = useRef(false);
  const saveTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const [narrow, setNarrow] = useState(() => typeof matchMedia === "function" && matchMedia(narrowQuery).matches);
  const narrowRef = useRef(narrow);
  narrowRef.current = narrow;
  const [ready, setReady] = useState(false);

  useEffect(() => {
    if (typeof matchMedia !== "function") return;
    const query = matchMedia(narrowQuery);
    const update = () => setNarrow(query.matches);
    update();
    query.addEventListener("change", update);
    return () => query.removeEventListener("change", update);
  }, [narrowQuery]);

  const orderedIds = (api: DockviewApi) => {
    const open = new Set(api.panels.map((p) => p.id));
    return [...defs.current.keys()].filter((id) => open.has(id));
  };

  const publishOpen = useCallback((api: DockviewApi) => controller?.setOpen(orderedIds(api)), [controller]);

  const save = useCallback(() => {
    const api = apiRef.current;
    if (!api || loadingRef.current || narrowRef.current || api.hasMaximizedGroup()) return;
    try {
      const open = new Set(api.panels.map((p) => p.id));
      writeSaved(keyRef.current, {
        v: LAYOUT_VERSION,
        layout: api.toJSON(),
        closed: [...defs.current.keys()].filter((id) => !open.has(id)),
      });
    } catch { /* an unserialisable layout is simply not saved */ }
  }, []);

  const scheduleSave = useCallback(() => {
    clearTimeout(saveTimer.current);
    saveTimer.current = setTimeout(save, 120);
  }, [save]);

  const addPanelTo = (api: DockviewApi, id: string, position?: any, inactive?: boolean, minimumWidth?: number) => {
    const def = defs.current.get(id)!;
    return api.addPanel({
      id,
      component: COMPONENT,
      tabComponent: TAB,
      title: def.title,
      renderer: "always",
      ...(position ? { position } : {}),
      ...(inactive ? { inactive: true } : {}),
      ...(minimumWidth ? { minimumWidth } : {}),
    });
  };

  const buildDefault = (api: DockviewApi) => {
    if (api.hasMaximizedGroup()) api.exitMaximizedGroup();
    api.clear();
    const spec = specRef.current;
    const groupIds: string[] = [];
    const sizes: { groupId: string; spec: DockGroupSpec }[] = [];
    spec.forEach((g) => {
      const ids = g.panels.filter((id) => defs.current.has(id));
      if (!ids.length) { groupIds.push(""); return; }
      const active = g.active && ids.includes(g.active) ? g.active : ids[0];
      let groupId = "";
      ids.forEach((id, n) => {
        let position: any;
        if (n === 0 && api.groups.length > 0) {
          const ref = groupIds[g.of ?? 0] || groupIds.find(Boolean);
          position = ref ? { referenceGroup: ref, direction: g.direction ?? "right" } : undefined;
        } else if (n > 0) {
          position = { referenceGroup: groupId, direction: "within" };
        }
        const panel = addPanelTo(api, id, position, n > 0 && id !== active, n === 0 ? g.minWidth : undefined);
        if (n === 0) groupId = panel.group.id;
      });
      api.getPanel(active)?.api.setActive();
      groupIds.push(groupId);
      if (g.size) sizes.push({ groupId, spec: g });
    });
    requestAnimationFrame(() => {
      for (const { groupId, spec: g } of sizes) {
        const group = api.getGroup(groupId);
        if (!group) continue;
        const vertical = g.direction === "above" || g.direction === "below";
        if (vertical) group.api.setSize({ height: Math.round(api.height * g.size!) });
        else group.api.setSize({ width: Math.round(api.width * g.size!) });
      }
    });
  };

  const collapseToOneGroup = (api: DockviewApi) => {
    if (api.groups.length < 2) return;
    if (api.hasMaximizedGroup()) api.exitMaximizedGroup();
    const activeId = api.activePanel?.id;
    const order = orderedIds(api);
    const first = (order[0] ? api.getPanel(order[0])?.group : undefined) ?? api.groups[0];
    for (const id of order) {
      const panel = api.getPanel(id);
      if (panel && panel.group !== first) panel.api.moveTo({ group: first });
    }
    if (activeId) api.getPanel(activeId)?.api.setActive();
  };

  /** Applies the saved layout (checked) or the default. Never leaves the dock empty. */
  const loadLayout = (api: DockviewApi) => {
    loadingRef.current = true;
    try {
      const saved = validateSaved(readSaved(keyRef.current), COMPONENT);
      let loaded = false;
      if (saved) {
        try {
          api.fromJSON(saved.layout);
          for (const p of [...api.panels]) if (!defs.current.has(p.id)) api.removePanel(p);
          for (const g of [...api.groups]) if (g.panels.length === 0) api.removeGroup(g);
          if (api.panels.length > 0 && api.groups.length > 0) {
            for (const def of defs.current.values()) {
              if (api.getPanel(def.id) || saved.closed.includes(def.id)) continue;
              addPanelTo(api, def.id, { referenceGroup: api.groups[0], direction: "within" }, true);
            }
            for (const p of api.panels) {
              const def = defs.current.get(p.id)!;
              if (p.title !== def.title) p.api.setTitle(def.title);
              if (p.api.renderer !== "always") p.api.setRenderer("always");
            }
            loaded = true;
          }
        } catch (error) {
          console.warn("neko: saved dock layout could not be restored; using the default", error);
        }
      }
      if (!loaded) buildDefault(api);
      if (narrowRef.current) collapseToOneGroup(api);
    } finally {
      loadingRef.current = false;
    }
    publishOpen(api);
  };

  const openPanel = (id: string) => {
    const api = apiRef.current;
    const def = defs.current.get(id);
    if (!api || !def) return;
    const existing = api.getPanel(id);
    if (existing) {
      if (api.hasMaximizedGroup() && api.activeGroup && existing.group !== api.activeGroup) api.exitMaximizedGroup();
      existing.api.setActive();
      return;
    }
    const spec = specRef.current;
    const at = spec.findIndex((g) => g.panels.includes(id));
    let position: any;
    if (api.groups.length === 0) position = undefined;
    else if (narrowRef.current) position = { referenceGroup: api.activeGroup ?? api.groups[0], direction: "within" };
    else {
      const sibling = at >= 0 ? spec[at].panels.find((p) => p !== id && api.getPanel(p)) : undefined;
      if (sibling) position = { referencePanel: sibling, direction: "within" };
      else if (at === 0) position = { referenceGroup: api.groups[0], direction: "left" };
      else if (at > 0) {
        const parent = spec[spec[at].of ?? 0]?.panels.find((p) => api.getPanel(p));
        position = parent
          ? { referencePanel: parent, direction: spec[at].direction ?? "right" }
          : { referenceGroup: api.groups[0], direction: "within" };
      } else position = { referenceGroup: api.activeGroup ?? api.groups[0], direction: "within" };
    }
    addPanelTo(api, id, position).api.setActive();
  };

  const impl: DockImpl = {
    open: openPanel,
    close(id) { apiRef.current?.getPanel(id)?.api.close(); },
    reset() {
      const api = apiRef.current;
      if (!api) return;
      loadingRef.current = true;
      try {
        buildDefault(api);
        if (narrowRef.current) collapseToOneGroup(api);
      } finally { loadingRef.current = false; }
      publishOpen(api);
      if (!narrowRef.current) save();
    },
    splitRight(id) {
      const api = apiRef.current;
      if (!api || narrowRef.current) return;
      const panel = (id ? api.getPanel(id) : undefined) ?? api.activePanel;
      if (!panel || panel.group.panels.length < 2) return;
      panel.api.moveTo({ group: panel.group, position: "right" });
    },
    isNarrow: () => narrowRef.current,
  };
  const implRef = useRef(impl);
  implRef.current = impl;

  useEffect(() => {
    controller?.bind({
      open: (id) => implRef.current.open(id),
      close: (id) => implRef.current.close(id),
      reset: () => implRef.current.reset(),
      splitRight: (id) => implRef.current.splitRight(id),
      isNarrow: () => implRef.current.isNarrow(),
    });
    return () => controller?.bind(null);
  }, [controller]);

  // The window crossed the narrow threshold: fold into one group, or bring the wide layout back.
  const wasNarrow = useRef(narrow);
  useEffect(() => {
    const api = apiRef.current;
    if (!api || !ready || wasNarrow.current === narrow) return;
    wasNarrow.current = narrow;
    if (narrow) {
      loadingRef.current = true;
      try { collapseToOneGroup(api); } finally { loadingRef.current = false; }
      publishOpen(api);
    } else loadLayout(api);
  }); // eslint-disable-line react-hooks/exhaustive-deps

  useEffect(() => () => clearTimeout(saveTimer.current), []);

  const onReady = ({ api }: DockviewReadyEvent) => {
    apiRef.current = api;
    loadLayout(api);
    api.onDidLayoutChange(() => {
      if (loadingRef.current) return;
      publishOpen(api);
      scheduleSave();
    });
    // A test and debugging hook: the dock's api, by storage key.
    const w = window as any;
    (w.__nekoDock ??= {})[keyRef.current] = { api, controller };
    setReady(true);
  };

  return (
    <PanelsContext.Provider value={nextDefs}>
      <div className={cn("neko-dock-host", className)} data-slot="dock" data-narrow={narrow ? "true" : undefined}>
        <DockviewReact
          theme={DOCK_THEME}
          components={COMPONENTS}
          tabComponents={TAB_COMPONENTS}
          prefixHeaderActionsComponent={GroupIndicator}
          rightHeaderActionsComponent={GroupActions}
          defaultRenderer="always"
          disableFloatingGroups={narrow}
          dndEdges={narrow ? false : undefined}
          onWillDrop={(event) => {
            if (narrowRef.current && event.position !== "center") event.preventDefault();
          }}
          getTabContextMenuItems={({ panel }) => {
            const closable = defs.current.get(panel.id)?.closable !== false;
            return narrow
              ? closable ? ["close"] : []
              : [...(closable ? (["close", "closeOthers", "separator"] as const) : []), "maximize", "float"];
          }}
          onReady={onReady}
        />
      </div>
    </PanelsContext.Provider>
  );
}
