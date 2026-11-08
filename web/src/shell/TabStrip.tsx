// The tab strip at the top of the window (DESIGN.md §1): back and forward, one
// tab per open place, and `+`. The tabs are a WAI-ARIA tablist: ← → Home End
// move between tabs, Enter or Space opens one, Delete closes it; ⌘T, ⌘W, ⌘1–9,
// ⌘[ ⌘] and ⌘⇧[ ⌘⇧] work anywhere. Tabs reorder by drag. The store is
// shell/tabs.ts.
import { useEffect, useRef, useState, type DragEvent, type KeyboardEvent as ReactKeyboardEvent } from "react";
import { ArrowLeft, ArrowRight, FrameCorners, ListChecks, Plus, Stack, Warning, X, type Icon } from "@/ui/icons";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/ui/tooltip";
import { cn } from "@/ui/lib/utils";
import { usePages } from "@/registry";
import { HOME } from "@/router";
import { useWorkbench } from "@/useStore";
import { useArtifact } from "@/canvas/artifacts";
import { StatusGlyph, statusOf } from "@/ui/status-glyph";
import { taskTitle } from "@/shell/status";
import { shortId } from "@/work/board/moves";
import { useContextMenu, copyText } from "@/shell/context-menu";
import {
  NEW_TAB, activateBeside, activateNth, activateTab, canGoBack, canGoForward, closeActiveTab, closeOtherTabs, closeTab,
  duplicateTab, goBack, goForward, moveTab, newTab, setPinned, tabLink, useTabs, type Tab,
} from "@/shell/tabs";

export const TAB_PANEL_ID = "neko-tab-panel";
export const tabElementId = (id: string) => `neko-tab-${id}`;

interface Label {
  title: string;
  /** Longer words for the tooltip and screen readers. */
  detail?: string;
  icon?: Icon;
  glyph?: string;
}

/** What a tab is called: the page's title, or the task, workspace or artifact it shows. */
export function useTabLabel(tab: Tab): Label {
  const pages = usePages();
  const { snapshot } = useWorkbench();
  const artifactId = tab.key.startsWith("artifact:") ? tab.key.slice(9) : null;
  const { artifact } = useArtifact(artifactId);
  if (tab.key === NEW_TAB) return { title: "New tab", icon: Plus };
  if (tab.key.startsWith("task:")) {
    const id = tab.key.slice(5);
    const task = (snapshot?.tasks ?? []).find((t: { id: string }) => t.id === id);
    if (!task) return { title: "Task", detail: id, icon: ListChecks };
    return { title: taskTitle(task), detail: `${shortId(task)} · ${taskTitle(task)}`, glyph: task.status };
  }
  if (tab.key.startsWith("workspace:")) {
    const id = tab.key.slice(10);
    const workspace = (snapshot?.workspaces ?? []).find((w: { id: string }) => w.id === id);
    return { title: workspace?.name ?? "Workspace", detail: workspace ? `Workspace · ${workspace.name}` : undefined, icon: Stack };
  }
  if (artifactId) return { title: artifact?.title ?? "Artifact", detail: artifact ? `Artifact · ${artifact.title}` : undefined, icon: FrameCorners };
  if (tab.key.startsWith("problem:")) return { title: tab.key.slice(8), detail: `${tab.key.slice(8)} did not load`, icon: Warning };
  const page = pages.find((p) => p.id === tab.key);
  return page ? { title: page.title, icon: page.icon } : { title: tab.key, detail: `${tab.key} (not found)` };
}

function TabItem({
  tab, index, count, active, focused, onFocusIndex, onDragTab, dragging,
}: {
  tab: Tab;
  index: number;
  count: number;
  active: boolean;
  focused: boolean;
  onFocusIndex: (i: number) => void;
  onDragTab: (id: string | null) => void;
  dragging: string | null;
}) {
  const label = useTabLabel(tab);
  const ref = useRef<HTMLButtonElement>(null);
  const home = tab.key === HOME;
  const closable = !home;
  const [over, setOver] = useState(false);
  const menu = useContextMenu(() => [
    { label: "Close tab", icon: X, shortcut: "⌘W", disabled: !closable, run: () => closeTab(tab.id) },
    { label: "Close other tabs", disabled: count <= 1, run: () => closeOtherTabs(tab.id) },
    "separator",
    { label: "Duplicate tab", run: () => duplicateTab(tab.id) },
    { label: tab.pinned ? "Unpin tab" : "Pin tab", disabled: home, run: () => setPinned(tab.id, !tab.pinned) },
    { label: "Move left", disabled: home || index <= 1, run: () => moveTab(tab.id, index - 1) },
    { label: "Move right", disabled: home || index >= count - 1, run: () => moveTab(tab.id, index + 1) },
    "separator",
    { label: "Copy link", run: () => void copyText(tabLink(tab), "Link copied") },
  ]);

  useEffect(() => {
    if (focused && document.activeElement?.closest("[role=tablist]")) ref.current?.focus();
  }, [focused]);

  const onDrop = (e: DragEvent) => {
    e.preventDefault();
    setOver(false);
    const id = e.dataTransfer.getData("application/x-neko-tab");
    if (id && id !== tab.id) moveTab(id, index);
    onDragTab(null);
  };

  return (
    <div
      {...menu}
      role="presentation"
      data-slot="tab"
      data-active={active || undefined}
      data-pinned={tab.pinned || undefined}
      draggable={!home}
      onDragStart={(e) => { e.dataTransfer.setData("application/x-neko-tab", tab.id); e.dataTransfer.effectAllowed = "move"; onDragTab(tab.id); }}
      onDragEnd={() => onDragTab(null)}
      onDragOver={(e) => { if (dragging && !home) { e.preventDefault(); setOver(true); } }}
      onDragLeave={() => setOver(false)}
      onDrop={onDrop}
      className={cn(
        // Content-fitting (DESIGN.md §8): as wide as its title, never wider than 200px, shrinking to 72px before the strip scrolls.
        "group/tab relative flex h-8 min-w-[72px] max-w-[200px] items-center rounded-[10px] text-[13px] transition-colors duration-150",
        tab.pinned || home ? "shrink-0" : "shrink",
        active ? "bg-foreground/[0.07] text-foreground" : "text-muted-foreground hover:bg-foreground/[0.04] hover:text-foreground",
        over && "ring-1 ring-primary/60",
        dragging === tab.id && "opacity-50",
      )}
    >
      <button
        ref={ref}
        type="button"
        role="tab"
        id={tabElementId(tab.id)}
        aria-selected={active}
        aria-controls={active ? TAB_PANEL_ID : undefined}
        aria-label={label.detail ?? label.title}
        aria-keyshortcuts={closable ? "Delete" : undefined}
        tabIndex={focused ? 0 : -1}
        title={label.detail ?? label.title}
        onClick={() => activateTab(tab.id)}
        onFocus={() => onFocusIndex(index)}
        className={cn(
          "flex h-full min-w-0 flex-1 items-center gap-2 rounded-[10px] ps-2.5 text-start outline-none focus-visible:ring-2 focus-visible:ring-ring/70",
          // Only the active tab keeps room for its close button; others are symmetric and show it over the title's end on hover.
          closable && active ? "pe-7" : "pe-3",
        )}
      >
        {label.glyph ? <StatusGlyph status={statusOf(label.glyph)} label="" size={13} /> : label.icon ? <label.icon aria-hidden size={14} className="shrink-0" /> : null}
        <span className={cn("min-w-0 flex-1 truncate", active && "font-medium")}>{label.title}</span>
      </button>
      {closable ? (
        <button
          type="button"
          tabIndex={-1}
          aria-label={`Close ${label.title}`}
          title="Close tab (⌘W)"
          onClick={(e) => { e.stopPropagation(); closeTab(tab.id); }}
          className={cn(
            "absolute end-1 flex size-5 items-center justify-center rounded-md text-muted-foreground outline-none hover:bg-foreground/10 hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring",
            active ? "opacity-100" : "bg-[color-mix(in_oklab,var(--background),var(--foreground)_4%)] opacity-0 shadow-[-6px_0_6px_color-mix(in_oklab,var(--background),var(--foreground)_4%)] group-hover/tab:opacity-100 focus-visible:opacity-100",
          )}
        >
          <X size={12} weight="bold" />
        </button>
      ) : null}
    </div>
  );
}

function StripButton({ label, shortcut, onClick, disabled, children }: { label: string; shortcut: string; onClick: () => void; disabled?: boolean; children: React.ReactNode }) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <button
          type="button"
          aria-label={label}
          aria-keyshortcuts={shortcut.replace("⌘", "Meta+")}
          disabled={disabled}
          onClick={onClick}
          className="flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground outline-none transition-colors hover:bg-foreground/[0.06] hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring/70 disabled:pointer-events-none disabled:opacity-35"
        >
          {children}
        </button>
      </TooltipTrigger>
      <TooltipContent>{label} ({shortcut})</TooltipContent>
    </Tooltip>
  );
}

/** ⌘T, ⌘W, ⌘1–9, ⌘[ ⌘] and ⌘⇧[ ⌘⇧], from anywhere in the window. */
function useTabShortcuts() {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const mod = e.metaKey || e.ctrlKey;
      if (!mod || e.altKey) return;
      const key = e.key.toLowerCase();
      if (key === "t" && !e.shiftKey) { e.preventDefault(); newTab(); return; }
      if (key === "w" && !e.shiftKey) { e.preventDefault(); closeActiveTab(); return; }
      if (!e.shiftKey && /^[1-9]$/.test(e.key)) { e.preventDefault(); activateNth(Number(e.key)); return; }
      if (e.code === "BracketLeft") { e.preventDefault(); if (e.shiftKey) activateBeside(-1); else goBack(); return; }
      if (e.code === "BracketRight") { e.preventDefault(); if (e.shiftKey) activateBeside(1); else goForward(); }
    };
    addEventListener("keydown", onKey);
    return () => removeEventListener("keydown", onKey);
  }, []);
}

export function TabStrip() {
  const { tabs, active } = useTabs();
  useTabShortcuts();
  const activeIndex = Math.max(0, tabs.findIndex((t) => t.id === active));
  const [focus, setFocus] = useState(activeIndex);
  const [dragging, setDragging] = useState<string | null>(null);
  const focusIndex = Math.min(focus, tabs.length - 1);
  useEffect(() => setFocus(activeIndex), [activeIndex]);

  const onKeyDown = (e: ReactKeyboardEvent<HTMLDivElement>) => {
    const last = tabs.length - 1;
    // Handled keys stop here, so a page's own shortcuts (Delete, Home…) never see them.
    const move = (i: number) => { e.preventDefault(); e.stopPropagation(); setFocus(i); };
    if (e.key === "ArrowRight") move(focusIndex >= last ? 0 : focusIndex + 1);
    else if (e.key === "ArrowLeft") move(focusIndex <= 0 ? last : focusIndex - 1);
    else if (e.key === "Home") move(0);
    else if (e.key === "End") move(last);
    else if (e.key === "Delete" || e.key === "Backspace") {
      const tab = tabs[focusIndex];
      if (tab && tab.key !== HOME) { e.preventDefault(); e.stopPropagation(); closeTab(tab.id); }
    }
  };

  return (
    <div data-slot="tab-strip" className="flex h-11 shrink-0 items-center gap-1 px-2">
      <StripButton label="Back" shortcut="⌘[" onClick={goBack} disabled={!canGoBack()}><ArrowLeft size={15} /></StripButton>
      <StripButton label="Forward" shortcut="⌘]" onClick={goForward} disabled={!canGoForward()}><ArrowRight size={15} /></StripButton>
      <div
        role="tablist"
        aria-label="Open tabs"
        aria-orientation="horizontal"
        onKeyDown={onKeyDown}
        className="ms-1 flex min-w-0 flex-1 items-center gap-0.5 overflow-x-auto [scrollbar-width:none]"
      >
        {tabs.map((tab, index) => (
          <TabItem
            key={tab.id}
            tab={tab}
            index={index}
            count={tabs.length}
            active={tab.id === active}
            focused={index === focusIndex}
            onFocusIndex={setFocus}
            onDragTab={setDragging}
            dragging={dragging}
          />
        ))}
        <StripButton label="New tab" shortcut="⌘T" onClick={newTab}><Plus size={15} /></StripButton>
      </div>
    </div>
  );
}
