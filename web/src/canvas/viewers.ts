// Viewers and reply fences: one registry, Neko's own and yours.
// docs/superpowers/specs/2026-11-08-everything-renders-design.md (A2, A3).
//
//   host.registerViewer({ id: "stl", title: "STL viewer", kinds: ["stl"], component: Stl, priority?: 0 })
//   host.registerFence({ language: "csv", viewer: "csv" })      // a reply's ```csv block, drawn by a viewer
//   host.registerFence({ language: "chart", component: Chart }) // or by a component of its own
//
// An artifact's kind is its file extension (the daemon's `artifacts::kind_of`).
// The canvas panel and the artifact tab ask `viewerFor(kind)`: the highest
// priority viewer that claims the kind, the latest registered on a tie (so a
// plugin's viewer replaces a built-in's at the same priority), else Download.
// Built-ins (builtin-viewers.tsx) register exactly like a plugin does. A
// plugin also declares its viewers in manifest.json, so the model is told
// about them (crates/neko-core/src/viewers.rs); what it registers here is
// what draws them.
import { lazy, useSyncExternalStore, type ComponentType } from "react";
import type { Artifact } from "@/canvas/artifacts";
import type { getThemeState } from "@/theme";

/** What a click in comment mode pointed at (html only). */
export interface ElementPick {
  selector: string;
  text: string;
}

/** What every viewer is given. */
export interface ViewerProps {
  /** The artifact, or null when a reply's fence is drawn from its own text. */
  artifact: Artifact | null;
  /** The version shown (0 for a fence's inline text). */
  version: number;
  /** `glb`, `csv`, `md`… */
  kind: string;
  /** The file (`/artifact/<id>/<n>`, same origin), or null for inline text. */
  url: string | null;
  /** Inline content (a reply's fence), instead of `url`. */
  text?: string;
  theme: ReturnType<typeof getThemeState>;
  /** Saves `content` as the next version (text kinds); resolves to its number. Absent when the viewer may not edit. */
  onEdit?: (content: string, note?: string) => Promise<number>;
  /** Comment mode, for viewers that support picking an element. */
  commenting?: boolean;
  onPick?: (pick: ElementPick) => void;
  onCancel?: () => void;
  /** Drawn inside a reply rather than on the canvas: keep it compact. */
  inline?: boolean;
}

export interface ViewerSpec {
  id: string;
  title: string;
  /** Kinds it draws, as extensions without the dot. */
  kinds: string[];
  /** Exactly one of these; `load` keeps a heavy viewer out of the main bundle. */
  component?: ComponentType<ViewerProps>;
  load?: () => Promise<{ default: ComponentType<ViewerProps> }>;
  /** Higher wins for a kind; 0 by default. */
  priority?: number;
  /** Whether it can edit (then the panel gives it `onEdit`). */
  edits?: boolean;
}

export interface Viewer {
  id: string;
  title: string;
  kinds: string[];
  component: ComponentType<ViewerProps>;
  priority: number;
  edits: boolean;
  source: "builtin" | "user";
  plugin?: string;
  order: number;
}

/** What a fence component is given. */
export interface FenceProps {
  code: string;
  language: string;
  isStreaming: boolean;
}

export interface FenceSpec {
  /** The fence's info string (```csv), lowercase. */
  language: string;
  /** A viewer id: the block's text is handed to it (or, when it is an artifact id, that artifact). */
  viewer?: string;
  component?: ComponentType<FenceProps>;
}

export interface Fence {
  language: string;
  viewer?: string;
  component?: ComponentType<FenceProps>;
  source: "builtin" | "user";
  plugin?: string;
  order: number;
}

let viewers: Viewer[] = [];
let fences: Fence[] = [];
let counter = 0;
let revision = 0;
const listeners = new Set<() => void>();
const changed = () => {
  revision++;
  listeners.forEach((l) => l());
};

const KIND = /^[a-z0-9]{1,16}$/;
const LANG = /^[a-z0-9_-]{1,32}$/;

function check(cond: unknown, message: string): asserts cond {
  if (!cond) throw new Error(message);
}

/** Adds a viewer; returns what takes it back. */
export function addViewer(spec: ViewerSpec, source: "builtin" | "user", plugin?: string): () => void {
  check(spec && typeof spec.id === "string" && /^[A-Za-z0-9_-]{1,64}$/.test(spec.id), "registerViewer needs an id of letters, digits, - or _.");
  check(Array.isArray(spec.kinds) && spec.kinds.length > 0, `Viewer “${spec.id}” needs kinds, e.g. ["stl"].`);
  const kinds = spec.kinds.map((k) => String(k).replace(/^\./, "").toLowerCase());
  check(kinds.every((k) => KIND.test(k)), `Viewer “${spec.id}”: a kind is a file extension of 1–16 letters or digits.`);
  check(!!spec.component !== !!spec.load, `Viewer “${spec.id}” needs exactly one of component or load.`);
  const id = source === "user" && plugin ? `${plugin}:${spec.id}` : spec.id;
  const entry: Viewer = {
    id,
    title: spec.title || spec.id,
    kinds,
    component: spec.load ? lazy(spec.load) : spec.component!,
    priority: Number.isFinite(spec.priority) ? Number(spec.priority) : 0,
    edits: !!spec.edits,
    source,
    plugin,
    order: counter++,
  };
  viewers = [...viewers.filter((v) => v.id !== id), entry];
  changed();
  return () => {
    viewers = viewers.filter((v) => v !== entry);
    changed();
  };
}

/** Adds a fence renderer; returns what takes it back. */
export function addFence(spec: FenceSpec, source: "builtin" | "user", plugin?: string): () => void {
  const language = String(spec?.language ?? "").toLowerCase();
  check(LANG.test(language), "registerFence needs a language of 1–32 letters, digits, - or _.");
  check(!!spec.viewer !== !!spec.component, `Fence “${language}” needs exactly one of viewer or component.`);
  const entry: Fence = { language, viewer: spec.viewer, component: spec.component, source, plugin, order: counter++ };
  fences = [...fences, entry];
  changed();
  return () => {
    fences = fences.filter((f) => f !== entry);
    changed();
  };
}

/** The viewer for a kind: highest priority, then the latest registered. */
export function viewerFor(kind: string): Viewer | null {
  const k = kind.toLowerCase();
  let best: Viewer | null = null;
  for (const v of viewers) {
    if (!v.kinds.includes(k)) continue;
    if (!best || v.priority > best.priority || (v.priority === best.priority && v.order > best.order)) best = v;
  }
  return best;
}

export const viewerById = (id: string) => viewers.find((v) => v.id === id) ?? null;

/** The viewer a fence names: the plugin's own `<plugin>:<id>` first, then any viewer with that id. */
export const fenceViewer = (fence: Fence): Viewer | null =>
  !fence.viewer ? null : (fence.plugin ? viewerById(`${fence.plugin}:${fence.viewer}`) : null) ?? viewerById(fence.viewer);

/** The latest fence registered for a language. */
export function fenceFor(language: string): Fence | null {
  const l = language.toLowerCase();
  let best: Fence | null = null;
  for (const f of fences) if (f.language === l && (!best || f.order > best.order)) best = f;
  return best;
}

export const listViewers = () => viewers;
export const listFences = () => fences;

const subscribe = (l: () => void) => {
  listeners.add(l);
  return () => listeners.delete(l);
};

/** Re-renders when a viewer or fence is added or taken back (a plugin loading). */
export function useViewerRegistry(): number {
  return useSyncExternalStore(subscribe, () => revision);
}

// ---------------------------------------------------------------------------
// Reading what a viewer shows
// ---------------------------------------------------------------------------

/** The text a viewer shows: its inline text, or the file. */
export async function readText(props: Pick<ViewerProps, "url" | "text">): Promise<string> {
  if (typeof props.text === "string") return props.text;
  if (!props.url) return "";
  const response = await fetch(props.url, { cache: "no-store" });
  if (!response.ok) throw new Error(`The file did not load (${response.status}).`);
  return response.text();
}

/** The bytes a viewer shows. */
export async function readBytes(props: Pick<ViewerProps, "url" | "text">): Promise<ArrayBuffer> {
  if (typeof props.text === "string") return new TextEncoder().encode(props.text).buffer as ArrayBuffer;
  if (!props.url) return new ArrayBuffer(0);
  const response = await fetch(props.url, { cache: "no-store" });
  if (!response.ok) throw new Error(`The file did not load (${response.status}).`);
  return response.arrayBuffer();
}

/** `a1b2c3d4e5f6`, `artifact a1b2c3d4e5f6 v2`, or `#/artifact?id=…&v=…`: the artifact a fence names, if it names one. */
export function artifactInFence(code: string): { id: string; version: number | null } | null {
  const text = code.trim();
  // A bare word is an id only in Neko's own shape (12 hex), so a one-word csv stays text.
  const plain = /^artifact\s+([A-Za-z0-9_-]{1,64})(?:\s+v(\d+))?$/i.exec(text) ?? /^([0-9a-f]{12})(?:\s+v(\d+))?$/i.exec(text);
  if (plain) return { id: plain[1], version: plain[2] ? Number(plain[2]) : null };
  const link = /artifact\?id=([A-Za-z0-9_-]{1,64})(?:&v=(\d+))?/.exec(text);
  if (link && !text.includes("\n")) return { id: link[1], version: link[2] ? Number(link[2]) : null };
  return null;
}
