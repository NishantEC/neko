// Which tasks a page shows, from the snapshot and the filters (TicketsView.tasks(in:)).
import { COLUMNS, stageOf, taskTitle, type Snapshot, type Stage, type Task } from "@/work/model";

export interface Filters {
  /** A workspace id, or "" for all. */
  workspace: string;
  stage: Stage | "all";
  search: string;
  includeStopped: boolean;
}

export const emptyByStage = (): Record<Stage, Task[]> => ({ needsYou: [], review: [], running: [], upNext: [], done: [] });

export function matches(task: Task, filters: Filters): boolean {
  if (filters.workspace && task.workspace_id !== filters.workspace) return false;
  if (filters.stage !== "all" && stageOf(task.status) !== filters.stage) return false;
  if (!filters.includeStopped && ["Failed", "Cancelled"].includes(task.status)) return false;
  const q = filters.search.trim().toLowerCase();
  if (q && !taskTitle(task).toLowerCase().includes(q) && !String(task.title ?? "").toLowerCase().includes(q)) return false;
  return true;
}

/** Tasks per board column, filtered, newest first. */
export function byStage(snapshot: Snapshot | null, filters: Filters): Record<Stage, Task[]> {
  const out = emptyByStage();
  for (const task of snapshot?.tasks ?? []) {
    if (!matches(task, filters)) continue;
    const column = COLUMNS.find((c) => c.statuses.includes(task.status));
    if (column) out[column.id].push(task);
  }
  for (const list of Object.values(out)) list.sort((a, b) => (b.updated_at_ms ?? 0) - (a.updated_at_ms ?? 0));
  return out;
}

export const flatten = (grouped: Record<Stage, Task[]>): Task[] => COLUMNS.flatMap((c) => grouped[c.id]);

/** Counts per stage ignoring the stage filter itself, so the chips stay stable while you pick one. */
export function stageCounts(snapshot: Snapshot | null, filters: Filters): Record<Stage, number> {
  const grouped = byStage(snapshot, { ...filters, stage: "all" });
  return Object.fromEntries(COLUMNS.map((c) => [c.id, grouped[c.id].length])) as Record<Stage, number>;
}
