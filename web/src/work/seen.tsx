// "Done, not yet seen" (herdr's unseen done; docs/research/herdr.md): a task
// that reached Ready to review, Completed or Failed since the person last
// opened it carries a small Iris dot (DESIGN.md §2: Iris is "now", what
// changed since you looked) in the task list, on its board card, on its tab
// and in Home's crew line. Opening the task (its sheet or its tab) marks it
// seen. The daemon keeps the record (`UnseenTasks` / `MarkTaskSeen`, a quiet
// settings row, neko_core::task_seen), so every window agrees; a mark in one
// window reaches the others as `UnseenTasksChanged`.
import { useEffect, useSyncExternalStore } from "react";
import { getStore, onEvent, request, subscribe as subscribeWorkbench } from "@/api";
import { cn } from "@/ui/lib/utils";

let unseen: ReadonlySet<string> = new Set();
const listeners = new Set<() => void>();
let started = false;
let lastSnapshot: unknown = null;
let timer: ReturnType<typeof setTimeout> | null = null;

function publish(ids: string[]) {
  const next = new Set(ids);
  if (next.size === unseen.size && [...next].every((id) => unseen.has(id))) return;
  unseen = next;
  listeners.forEach((l) => l());
}

async function refresh() {
  try {
    const reply = await request("UnseenTasks");
    if (reply?.UnseenTasks) publish(reply.UnseenTasks.ids ?? []);
  } catch {
    /* an older daemon, or one restarting: no dots until it answers */
  }
}

/** A task's status changes with the snapshot, so ask again shortly after each new one. */
function soon() {
  if (timer) clearTimeout(timer);
  timer = setTimeout(() => { timer = null; void refresh(); }, 250);
}

function start() {
  if (started) return;
  started = true;
  onEvent((event) => { if (event?.UnseenTasksChanged) publish(event.UnseenTasksChanged.ids ?? []); });
  subscribeWorkbench(() => {
    const snapshot = getStore().snapshot;
    if (snapshot && snapshot !== lastSnapshot) { lastSnapshot = snapshot; soon(); }
  });
  addEventListener("focus", soon);
  void refresh();
}

function subscribe(listener: () => void) {
  listeners.add(listener);
  start();
  return () => { listeners.delete(listener); };
}

/** Ids of the finished tasks the person has not opened since they finished. */
export function useUnseen(): ReadonlySet<string> {
  return useSyncExternalStore(subscribe, () => unseen);
}

/** Marks a task seen (`seen`) or unread. The dot changes at once; the daemon's answer settles it. */
export async function markTaskSeen(taskId: string, seen = true) {
  const next = new Set(unseen);
  if (seen) next.delete(taskId);
  else next.add(taskId);
  publish([...next]);
  try {
    const reply = await request({ MarkTaskSeen: { task_id: taskId, seen } });
    if (reply?.UnseenTasks) publish(reply.UnseenTasks.ids ?? []);
  } catch {
    /* a task the daemon does not have (a fixture), or an older daemon: keep the local answer */
  }
}

/** Opening a task marks it seen, and keeps it seen while it stays open (it may finish on screen). */
export function useMarkSeen(taskId: string | null | undefined) {
  const set = useUnseen();
  const flagged = !!taskId && set.has(taskId);
  useEffect(() => {
    if (taskId && flagged) void markTaskSeen(taskId);
  }, [taskId, flagged]);
}

/** The dot itself: Iris, 6px, named for screen readers. */
export function UnseenDot({ className }: { className?: string }) {
  return (
    <span
      data-slot="unseen-dot"
      role="img"
      aria-label="Not yet seen"
      title="Finished since you last opened it"
      className={cn("inline-block size-1.5 shrink-0 rounded-full bg-primary", className)}
    />
  );
}
