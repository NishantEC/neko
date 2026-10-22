import { useEffect, useState } from "react";
import { accept, command, type Json } from "@/api";
import { Shield } from "@/ui/icons";
import { Button } from "@/ui/button";
import { Kbd } from "@/ui/kbd";
import { MonoLabel } from "@/editorial";

export interface Decision {
  key: string;
  /** Small amber label: what kind of approval this is. */
  label: string;
  /** The exact thing being approved, in mono. */
  subject: string;
  /** What it will do, in full (plain words). */
  detail: string;
  /** Structured arguments, when the thing is a tool call. */
  args?: unknown;
  /** Which task it concerns, when there is one. */
  context?: string;
  decide(approve: boolean): Promise<Json>;
}

const ACTION_LABELS: Record<string, string> = {
  message: "Tell its agent",
  start: "Start this task",
  stop: "Stop this task",
  close: "Close this task",
  merge: "Merge as a duplicate",
  run_next: "Run next",
  approve: "Approve its plan",
  accept: "Accept its result",
  retry: "Try again",
  rename: "Rename",
  merge_pr: "Merge the pull request",
  close_pr: "Close the pull request",
};

function parse(json: string): unknown {
  try {
    return JSON.parse(json);
  } catch {
    return json;
  }
}

/**
 * Everything waiting for a yes or a no: tool calls a chat reply needs approved
 * and actions Neko proposed on tasks. Newest reply first (older proposals can
 * linger unanswered for days and must not sit in front of the one being waited
 * on), and within a reply the tool call that blocks it before its proposals.
 */
export function pendingDecisions(messages: Json[], tasks: Json[]): Decision[] {
  const out: Decision[] = [];
  for (const message of [...messages].reverse()) {
    for (const call of message.tool_calls ?? []) {
      if (call.status !== "awaiting_approval") continue;
      const args = parse(call.arguments_json);
      out.push({
        key: `${message.id}:${call.id}`,
        label: `Tool approval · ${call.connection_id}`,
        subject: call.tool_name,
        detail: typeof args === "string" ? args : JSON.stringify(args, null, 2),
        args: typeof args === "string" ? undefined : args,
        decide: (approve) => command("DecideChatTool", { turn_id: message.id, call_id: call.id, approve }),
      });
    }
    for (const action of message.actions ?? []) {
      if (action.status !== "Proposed") continue;
      const task = tasks.find((t) => t.id === action.task_id);
      out.push({
        key: `${message.id}:${action.id}`,
        label: ACTION_LABELS[action.kind] ?? action.kind,
        subject: task ? task.display_title || task.title : action.task_id,
        detail: action.text,
        context: action.kind,
        decide: (approve) => command("DecideChatAction", { message_id: message.id, action_id: action.id, accept: approve }),
      });
    }
  }
  return out;
}

export const isMac = /Mac|iPhone|iPad/.test(navigator.platform);

/** The card the user answers: what is being approved, the exact thing, two buttons. */
export function DecisionCard({
  decision, count = 1, busy, error, onDecide,
}: {
  decision: Decision;
  count?: number;
  busy?: boolean;
  error?: string | null;
  onDecide: (approve: boolean) => void;
}) {
  return (
    <section
      data-slot="decision"
      role="alertdialog"
      aria-label={decision.label}
      className="mb-2 rounded-2xl border border-warning/40 bg-warning/[0.07] px-4 py-3 shadow-xs"
    >
      <div className="flex items-center gap-2 text-warning-foreground">
        <Shield size={15} weight="fill" aria-hidden />
        <MonoLabel className="text-warning-foreground">{decision.label}</MonoLabel>
        {count > 1 ? <span className="ms-auto text-xs text-muted-foreground">1 of {count}</span> : null}
      </div>
      <div className="mt-1.5 font-medium text-strong">{decision.subject}</div>
      <pre className="mt-1.5 max-h-28 overflow-auto whitespace-pre-wrap break-words rounded-lg bg-foreground/[0.04] px-2.5 py-1.5 font-mono text-xs leading-[1.55] text-muted-foreground">
        {decision.detail}
      </pre>
      {error ? <p className="mt-2 text-[13px] text-destructive">{error}</p> : null}
      <div className="mt-3 flex justify-end gap-2">
        <Button variant="outline" size="sm" disabled={busy} onClick={() => onDecide(false)}>
          Decline <Kbd>Esc</Kbd>
        </Button>
        <Button size="sm" disabled={busy} onClick={() => onDecide(true)}>
          Approve <Kbd className="bg-white/15 text-primary-foreground">{isMac ? "⌘↵" : "Ctrl↵"}</Kbd>
        </Button>
      </div>
    </section>
  );
}

/**
 * The dock above the composer. It does not take the composer's place: you can
 * still type and queue the next message while this waits for an answer.
 * ⌘↵ approves, Esc declines (Esc only when it cannot be mistaken for
 * abandoning a draft).
 */
export function Decisions({ items }: { items: Decision[] }) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const first = items[0];

  async function decide(approve: boolean) {
    if (!first || busy) return;
    setBusy(true);
    setError(null);
    try {
      accept(await first.decide(approve));
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  useEffect(() => {
    if (!first) return;
    const onKey = (e: KeyboardEvent) => {
      const typing = e.target instanceof HTMLTextAreaElement && e.target.value.trim() !== "";
      if (e.key === "Enter" && (isMac ? e.metaKey : e.ctrlKey) && !e.shiftKey) {
        e.preventDefault();
        void decide(true);
      } else if (e.key === "Escape" && !typing) {
        e.preventDefault();
        void decide(false);
      }
    };
    addEventListener("keydown", onKey);
    return () => removeEventListener("keydown", onKey);
  });

  if (!first) return null;
  return <DecisionCard decision={first} count={items.length} busy={busy} error={error} onDecide={decide} />;
}
