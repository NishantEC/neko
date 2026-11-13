// Messages waiting for a running agent (docs/knowledge/the-chat-directs-the-crew.md).
// A message typed into a running task's composer does not interrupt it by
// default: it queues, one line each with Cancel, and reaches the agent as its
// mode says. The queue is the daemon's (`snapshot.queued_messages`, its own
// rows), so it survives a restart and a retried send is one message.
//
//   after the turn         (default) waits for the running turn to end
//   steer now   (⌘↩)       into the live session if the runtime can take it, else at the turn boundary
//   interrupt and restart  stops the turn and restarts it with the message
import { useRef } from "react";
import { command, type Json } from "@/api";
import { Button } from "@/ui/button";
import { toast } from "@/ui/sonner";
import { cn } from "@/ui/lib/utils";

export type DeliveryMode = "after_turn" | "steer_now" | "interrupt_restart";

export const MODES: { id: DeliveryMode; label: string; hint: string }[] = [
  { id: "after_turn", label: "After this turn", hint: "Waits for the running turn to end" },
  { id: "steer_now", label: "Steer now", hint: "⌘↩ · Into the live session where the agent can take it, else when the turn ends" },
  { id: "interrupt_restart", label: "Interrupt and restart", hint: "Stops the turn and restarts it with your message" },
];

const WORD: Record<string, string> = { after_turn: "after the turn", steer_now: "steer now", interrupt_restart: "interrupt" };

/** The messages still waiting for this task's agent, oldest first. */
export function queuedFor(snapshot: Json | null | undefined, taskId: string): Json[] {
  const all: Json[] = Array.isArray(snapshot?.queued_messages) ? snapshot.queued_messages : [];
  return all.filter((m) => m.task_id === taskId && m.state === "queued");
}

/** Whether a task is in a turn right now, so a message needs a delivery mode. */
export const isTurnRunning = (status: string) => ["Planning", "Building", "Reviewing"].includes(status);

const newCommandId = () => (globalThis.crypto?.randomUUID?.() ?? `c-${Date.now()}-${Math.random().toString(36).slice(2)}`);

/**
 * Queues a message for a running task. A retry of the same text and mode keeps
 * its command id, so the daemon never queues it twice.
 */
export function useQueueSend(taskId: string) {
  const attempt = useRef<{ key: string; id: string } | null>(null);
  return async (text: string, mode: DeliveryMode): Promise<{ ok: true } | { ok: false; error: string }> => {
    const key = `${taskId}|${mode}|${text}`;
    if (attempt.current?.key !== key) attempt.current = { key, id: newCommandId() };
    try {
      await command("Crew", { Enqueue: { task_id: taskId, command_id: attempt.current.id, text, mode, kind: mode === "interrupt_restart" ? "interrupt" : mode === "steer_now" ? "steer" : "send" } });
      attempt.current = null;
      return { ok: true };
    } catch (error) {
      return { ok: false, error: error instanceof Error ? error.message : String(error) };
    }
  };
}

/** The three delivery modes, as one radio group above the composer while a turn runs. */
export function ModePicker({ mode, onMode }: { mode: DeliveryMode; onMode: (mode: DeliveryMode) => void }) {
  return (
    <div role="radiogroup" aria-label="When the agent should get your message" data-slot="queue-modes" className="flex w-full flex-wrap items-center gap-1 text-[12.5px] text-muted-foreground">
      <span className="me-1">Send</span>
      {MODES.map((m) => (
        <button
          key={m.id}
          type="button"
          role="radio"
          aria-checked={mode === m.id}
          title={m.hint}
          data-mode={m.id}
          onClick={() => onMode(m.id)}
          className={cn("rounded-md px-2 py-0.5 outline-none transition-colors hover:bg-foreground/[0.05] hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring/60", mode === m.id && "bg-foreground/[0.07] font-medium text-foreground")}
        >
          {m.label}
        </button>
      ))}
    </div>
  );
}

/** One line per waiting message, each with Cancel. Nothing renders when nothing waits. */
export function QueuedMessages({ taskId, queued }: { taskId: string; queued: Json[] }) {
  if (!queued.length) return null;
  const cancel = async (id: string) => {
    try {
      await command("Crew", { CancelQueued: { task_id: taskId, id } });
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error));
    }
  };
  return (
    <ul data-slot="queued-messages" aria-label="Messages waiting for the agent" className="flex w-full flex-col gap-0.5 pb-1">
      {queued.map((m) => (
        <li key={m.id} data-slot="queued-message" data-mode={m.mode} className="group/row flex min-w-0 items-center gap-2 text-[12.5px] text-muted-foreground">
          <span className="shrink-0 rounded-sm bg-foreground/[0.06] px-1.5 py-px text-[11.5px]">Queued · {WORD[m.mode] ?? m.mode}</span>
          <span className="min-w-0 flex-1 truncate text-foreground/85" title={m.detail ? `${m.text}\n\n${m.detail}` : m.text}>{String(m.text).split("\n")[0]}</span>
          {m.source && m.source !== "you" ? <span className="shrink-0 text-muted-foreground/70">from {m.source}</span> : null}
          <Button variant="ghost" size="sm" className="h-6 shrink-0 px-2 text-[12px]" onClick={() => void cancel(String(m.id))} aria-label={`Cancel queued message: ${String(m.text).slice(0, 40)}`}>
            Cancel
          </Button>
        </li>
      ))}
    </ul>
  );
}
