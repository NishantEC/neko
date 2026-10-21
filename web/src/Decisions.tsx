import { useEffect, useState } from "react";
import { accept, command, type Json } from "./api";
import { Shield } from "./icons";

export interface Decision {
  key: string;
  /** Small amber label: what kind of approval this is. */
  label: string;
  /** The exact thing being approved, in mono. */
  subject: string;
  /** What it will do, in full. */
  detail: string;
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

function pretty(json: string): string {
  try {
    return JSON.stringify(JSON.parse(json), null, 2);
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
      out.push({
        key: `${message.id}:${call.id}`,
        label: `Tool approval · ${call.connection_id}`,
        subject: call.tool_name,
        detail: pretty(call.arguments_json),
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

const isMac = /Mac|iPhone|iPad/.test(navigator.platform);

/**
 * The card above the composer. It does not take the composer's place: you can
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
  return (
    <section className="decision" role="alertdialog" aria-label={first.label}>
      <div className="decision-top">
        <Shield />
        <span className="decision-label">{first.label}</span>
        {items.length > 1 && <span className="decision-count">1 of {items.length}</span>}
      </div>
      <div className="decision-subject">{first.subject}</div>
      <pre className="decision-detail">{first.detail}</pre>
      {error && <p className="composer-error">{error}</p>}
      <div className="decision-actions">
        <button className="btn" disabled={busy} onClick={() => decide(false)}>
          Decline <kbd>Esc</kbd>
        </button>
        <button className="btn primary" disabled={busy} onClick={() => decide(true)}>
          Approve <kbd>{isMac ? "⌘↵" : "Ctrl↵"}</kbd>
        </button>
      </div>
    </section>
  );
}
