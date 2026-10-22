// When something needs you and you are not looking: the tab/window title gets a
// count, and a toast says what it is. "Needs you" is the one definition in
// status.ts: a decision waiting for a yes or no, a task that failed or waits for
// approval, a task whose result is ready to review.
import { useEffect, useRef } from "react";
import { toast } from "@/ui/sonner";
import { navigate, HOME } from "@/router";
import { useWorkbench } from "@/useStore";
import { currentDecisions } from "@/shell/actions";
import { stageOf, taskHref, taskTitle, useShellStatus } from "@/shell/status";

export function useAttention() {
  const { snapshot } = useWorkbench();
  const status = useShellStatus();
  const seen = useRef<Set<string> | null>(null);

  // The title: "(2) Neko" when something waits, "Neko · working" while a reply runs.
  useEffect(() => {
    document.title = status.needsUser > 0 ? `(${status.needsUser}) Neko` : status.working ? "Neko · working" : "Neko";
  }, [status.needsUser, status.working]);

  // Toasts for what is NEW since the last snapshot (never for what was already waiting at load).
  useEffect(() => {
    if (!snapshot) return;
    const waiting = new Map<string, { title: string; detail: string; open(): void }>();
    for (const d of currentDecisions()) {
      waiting.set(`decision:${d.key}`, { title: "Neko needs your approval", detail: d.subject, open: () => navigate(HOME) });
    }
    for (const task of snapshot.tasks ?? []) {
      const stage = stageOf(task.status);
      if (stage !== "needsYou" && stage !== "review") continue;
      waiting.set(`task:${task.id}:${task.status}`, {
        title: stage === "review" ? "A task is ready to review" : task.status === "Failed" ? "A task failed" : "A task needs your approval",
        detail: taskTitle(task),
        open: () => { location.hash = taskHref(task.id).slice(1); },
      });
    }
    const previous = seen.current;
    seen.current = new Set(waiting.keys());
    if (!previous) return;
    const fresh = [...waiting].filter(([key]) => !previous.has(key));
    if (!fresh.length) return;
    const [, first] = fresh[0];
    toast.warning(fresh.length === 1 ? first.title : `${fresh.length} things need you`, {
      description: fresh.length === 1 ? first.detail : `${first.detail} and ${fresh.length - 1} more`,
      duration: 12000,
      action: { label: "Open", onClick: first.open },
    });
  }, [snapshot]);

}
