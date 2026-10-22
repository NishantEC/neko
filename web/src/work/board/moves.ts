// What dropping a card on a column means. The daemon's own rules live in `moveCommand`
// (the Swift board's `moveCommand`); this adds the words: what the drop will do, or why
// it will not. A refusal is always explained, never silent, and never sent to the daemon.
import { COLUMNS, friendlyStatus, moveCommand, type Snapshot, type Stage, type Task } from "@/work/model";

export type Decision =
  | { kind: "same" }
  | { kind: "move"; command: string; verb: string }
  | { kind: "refuse"; reason: string };

const VERB: Record<string, string> = {
  StartTask: "Start work",
  CompleteTask: "Mark done",
  CancelTask: "Cancel the task",
};

function fallbackReason(task: Task, to: Stage): string {
  if (task.status === "Completed") return "Finished work stays finished. Describe a follow-up in Home instead.";
  if (task.status === "Queued" && to === "running") return "A queued task starts by itself when a slot is free. Use Run next to put it first.";
  if (task.status === "Failed" && to === "done") return "A stopped task needs a decision: start it again, or open it and delete it.";
  if (to === "upNext") return "Work that has started can’t go back to the queue. Stop it first.";
  if (to === "done") return "Only a task that is ready for review can be marked done.";
  return `A task that is “${friendlyStatus(task.status).toLowerCase()}” can’t move there.`;
}

export function decide(task: Task, to: Stage): Decision {
  const own = COLUMNS.find((c) => c.id === to);
  if (!own || own.statuses.includes(task.status)) return { kind: "same" };
  const plan = moveCommand(task.status, to);
  if (plan.command) return { kind: "move", command: plan.command, verb: VERB[plan.command] ?? "Move here" };
  return { kind: "refuse", reason: plan.reason ?? fallbackReason(task, to) };
}

/** "NEK-1BB3": a short, stable handle for a task (the first four characters of its id). */
export const shortId = (task: Task) => `NEK-${String(task.id ?? "").slice(0, 4).toUpperCase()}`;

export interface Runtime { provider: string; label: string; initials: string; model: string }

/** The agent runtime that last ran (or will run) a task: the newest runtime choice made for it, else the default. */
export function runtimeOf(snapshot: Snapshot, task: Task): Runtime {
  const own = ((snapshot?.runtime_selections ?? []) as any[]).filter((s) => s.conversation_id === `task:${task.id}`);
  const latest = own.reduce<any>((a, b) => (!a || (b.selected_at_ms ?? 0) >= (a.selected_at_ms ?? 0) ? b : a), null);
  const raw = String(latest?.runtime?.provider || snapshot?.agent_runtime?.provider || "codex");
  const provider = raw === "claude" || raw === "opencode" || raw === "codex" ? raw : raw || "codex";
  const label = provider === "claude" ? "Claude" : provider === "opencode" ? "OpenCode" : provider === "codex" ? "Codex" : provider[0].toUpperCase() + provider.slice(1);
  const initials = provider === "claude" ? "Cl" : provider === "opencode" ? "Oc" : provider === "codex" ? "Cx" : label.slice(0, 2);
  const model = String(latest?.runtime?.model ?? "");
  return { provider, label, initials, model };
}

// While a drag is in flight (and for one tick after it ends) Escape belongs to the drag, not to
// whatever else listens for it on the page, such as closing the open task.
let dragging = false;
let releaseTimer: ReturnType<typeof setTimeout> | undefined;
export const isBoardDragging = () => dragging;
export function setBoardDragging(on: boolean) {
  clearTimeout(releaseTimer);
  if (on) dragging = true;
  else releaseTimer = setTimeout(() => { dragging = false; }, 0);
}
