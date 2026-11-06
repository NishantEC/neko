// A routine (the daemon's `Routines` view: { routine, state, runs }) in the one
// status language (ui/status-glyph.tsx, DESIGN.md §3). The Inbox and the
// Workspaces page both read this, so a routine reads the same in each.
//
//   running → in progress · asked to run → to do · paused or never run → backlog
//   the last run failed → failed · it ran (acted, or found nothing) → done
import type { Json } from "@/api";
import type { StatusKind } from "@/ui/status-glyph";

export function routineStatus(view: Json): [StatusKind, string] {
  const { routine, state, runs } = view ?? {};
  const last: Json | undefined = runs?.[0];
  if (state?.running) return ["inProgress", "Running"];
  if (state?.run_requested) return ["todo", "Queued"];
  if (!routine?.enabled) return ["backlog", "Paused"];
  if (!last) return ["backlog", "Not run yet"];
  if (!last.ok) return ["failed", "Failed"];
  return ["done", last.quiet ? "Quiet" : "Acted"];
}

/** The items a routine's latest run left for the person: how many, and how many are done. */
export function itemProgress(view: Json): { done: number; total: number } {
  const items: Json[] = view?.runs?.[0]?.needs_you ?? [];
  return { done: items.filter((i) => i?.done).length, total: items.length };
}
