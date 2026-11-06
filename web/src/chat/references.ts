// References in reply text (DESIGN.md §5): a task id or an artifact named in
// prose is an Iris link that opens its tab. The one recogniser; the markdown
// pipeline (t3/ChatMarkdownRenderer.tsx) runs `remarkReferences` over text
// nodes only, so code spans, code blocks and existing links stay as written.
//
//   NEK-1BB3                     a task's short id (shortId in work/board/moves.ts), when a task has that prefix
//   <a task's full id>           when it is a task in the snapshot
//   artifact 0a1b2c3d4e5f v2     an artifact id after the word "artifact" (with an optional version),
//                                which covers `[artifact <id> v<n>, element …]` references
import { useMemo, useSyncExternalStore } from "react";
import { getStore, subscribe } from "@/api";
import { taskHref } from "@/shell/status";
import { artifactHref } from "@/canvas/artifacts";

export interface TaskIndex {
  /** Full ids. */
  ids: Set<string>;
  /** Short id ("1BB3", upper case) → full id. Ambiguous prefixes are left out. */
  short: Map<string, string>;
}

export function indexTasks(ids: string[]): TaskIndex {
  const short = new Map<string, string>();
  const clash = new Set<string>();
  for (const id of ids) {
    const key = id.slice(0, 4).toUpperCase();
    if (short.has(key)) clash.add(key);
    else short.set(key, id);
  }
  clash.forEach((key) => short.delete(key));
  return { ids: new Set(ids), short };
}

const taskIdsKey = () => ((getStore().snapshot?.tasks ?? []) as { id?: unknown }[]).map((t) => String(t.id ?? "")).filter(Boolean).join(",");

/** The snapshot's tasks as an index; re-renders only when the set of ids changes, not on every snapshot. */
export function useTaskIndex(): TaskIndex {
  const key = useSyncExternalStore(subscribe, taskIdsKey);
  return useMemo(() => indexTasks(key ? key.split(",") : []), [key]);
}

export type Reference =
  | { kind: "task"; id: string; start: number; end: number; text: string }
  | { kind: "artifact"; id: string; version: number | null; start: number; end: number; text: string };

const PATTERN = /\bNEK-([0-9A-Za-z]{4})\b|\bartifact\s+([0-9a-f]{12})(?:\s+v(\d+))?\b|\b([0-9A-Za-z][0-9A-Za-z_-]{7,63})\b/g;

/** Every reference in `text`, in order. */
export function findReferences(text: string, tasks: TaskIndex): Reference[] {
  const out: Reference[] = [];
  PATTERN.lastIndex = 0;
  for (let m = PATTERN.exec(text); m; m = PATTERN.exec(text)) {
    const [whole, shortId, artifact, version, token] = m;
    const start = m.index;
    const end = start + whole.length;
    if (shortId) {
      const id = tasks.short.get(shortId.toUpperCase());
      if (id) out.push({ kind: "task", id, start, end, text: whole });
    } else if (artifact) {
      out.push({ kind: "artifact", id: artifact, version: version ? Number(version) : null, start, end, text: whole });
    } else if (token && tasks.ids.has(token)) {
      out.push({ kind: "task", id: token, start, end, text: whole });
    }
  }
  return out;
}

export function referenceHref(ref: Reference): string {
  return ref.kind === "task" ? taskHref(ref.id) : artifactHref(ref.id, ref.version);
}

/** The tasks a text names, full ids, first mention first. */
export function tasksIn(text: string, tasks: TaskIndex): string[] {
  return [...new Set(findReferences(text, tasks).filter((r) => r.kind === "task").map((r) => r.id))];
}

// ---------------------------------------------------------------------------
// The remark plugin: text nodes become text + link nodes.
// ---------------------------------------------------------------------------

interface MdNode { type: string; value?: string; url?: string; children?: MdNode[]; data?: Record<string, unknown> }

const SKIP = new Set(["link", "linkReference", "code", "inlineCode", "html", "definition"]);

function split(node: MdNode, tasks: TaskIndex): MdNode[] | null {
  const text = node.value ?? "";
  const refs = findReferences(text, tasks);
  if (!refs.length) return null;
  const out: MdNode[] = [];
  let at = 0;
  for (const ref of refs) {
    if (ref.start > at) out.push({ type: "text", value: text.slice(at, ref.start) });
    out.push({
      type: "link",
      url: referenceHref(ref),
      children: [{ type: "text", value: ref.text }],
      data: { hProperties: { "data-reference": ref.kind, "data-reference-id": ref.id } },
    });
    at = ref.end;
  }
  if (at < text.length) out.push({ type: "text", value: text.slice(at) });
  return out;
}

function walk(node: MdNode, tasks: TaskIndex) {
  if (!node.children || SKIP.has(node.type)) return;
  const next: MdNode[] = [];
  for (const child of node.children) {
    if (child.type === "text") {
      const parts = split(child, tasks);
      if (parts) next.push(...parts);
      else next.push(child);
    } else {
      walk(child, tasks);
      next.push(child);
    }
  }
  node.children = next;
}

/** `remarkPlugins={[remarkGfm, remarkReferences(tasks)]}`. */
export function remarkReferences(tasks: TaskIndex) {
  return () => (tree: MdNode) => walk(tree, tasks);
}
