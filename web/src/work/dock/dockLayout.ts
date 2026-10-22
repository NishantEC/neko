// What the dock saves and how a saved layout is checked before it is trusted.
// Pure (no React, no dockview classes) so it can be reasoned about on its own.
//
// A layout is dockview's own `toJSON()` plus the ids the person closed, so a
// closed panel stays closed after a reload while a panel that is simply missing
// (a new one, or a hand-edited file) is put back.

export const LAYOUT_VERSION = 1;

export interface SavedDock {
  v: number;
  layout: any;
  /** Panel ids the person closed; they are not re-added on load. */
  closed: string[];
}

/** Where a group sits in the default layout. The first group needs no position. */
export interface DockGroupSpec {
  panels: string[];
  /** The tab shown first. Defaults to the first panel. */
  active?: string;
  /** Which earlier group (by index) this one is placed beside. Defaults to 0. */
  of?: number;
  /** Which side of that group. Defaults to "right". */
  direction?: "left" | "right" | "above" | "below";
  /** Share of the dock's width (left/right) or height (above/below), 0..1, for the FIRST group of a split. */
  size?: number;
  /** Smallest width in px the group may be dragged to. */
  minWidth?: number;
}

export function readSaved(key: string): unknown {
  try {
    const raw = localStorage.getItem(key);
    return raw ? JSON.parse(raw) : null;
  } catch {
    return null;
  }
}

export function writeSaved(key: string, value: SavedDock | null) {
  try {
    if (value) localStorage.setItem(key, JSON.stringify(value));
    else localStorage.removeItem(key);
  } catch {
    /* storage can be blocked or full; the layout just does not persist */
  }
}

/**
 * The saved layout if it has the shape dockview's `fromJSON` expects and only
 * names the dock's own component; otherwise null (and the default is used).
 * Unknown panel ids are not rejected here: they are removed after loading,
 * which keeps the grid valid.
 */
export function validateSaved(raw: unknown, componentName: string): SavedDock | null {
  if (!raw || typeof raw !== "object") return null;
  const saved = raw as Partial<SavedDock>;
  if (saved.v !== LAYOUT_VERSION) return null;
  const layout = saved.layout as any;
  if (!layout || typeof layout !== "object") return null;
  const grid = layout.grid;
  if (!grid || typeof grid !== "object" || !grid.root || typeof grid.root !== "object") return null;
  if (typeof grid.width !== "number" || typeof grid.height !== "number") return null;
  if (!layout.panels || typeof layout.panels !== "object" || Array.isArray(layout.panels)) return null;
  const ids = Object.keys(layout.panels);
  if (ids.length === 0) return null;
  for (const id of ids) {
    const p = layout.panels[id];
    if (!p || typeof p !== "object" || p.contentComponent !== componentName) return null;
  }
  const closed = Array.isArray(saved.closed) ? saved.closed.filter((c): c is string => typeof c === "string") : [];
  // Pop-out windows do not come back after a reload.
  const { popoutGroups: _popouts, ...rest } = layout;
  return { v: LAYOUT_VERSION, layout: rest, closed };
}
