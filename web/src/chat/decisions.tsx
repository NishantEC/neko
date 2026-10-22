import { useEffect, useState } from "react";
import { accept, command, type Json } from "@/api";
import { ACTION_TOOL, isToolPart, type NekoUIMessage } from "@/chat/sdk/types";
import { connectionOf } from "@/chat/sdk/parts";
import { ComposerApprovalDrawer, ComposerPendingApprovalActions, ComposerPendingApprovalPanel } from "@/chat/t3/ComposerPendingApproval";

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
 * What waits for a yes or a no in a conversation, read from the SDK's tool
 * approval state: every `dynamic-tool` part in `approval-requested`. A bridge
 * tool the reply needs approved answers `DecideChatTool`; an action Neko
 * proposed on a task (`neko_task_action`) answers `DecideChatAction`. The daemon
 * checks and runs both exactly as before and streams the approval's outcome.
 */
export function decisionsFromParts(messages: NekoUIMessage[], tasks: Json[]): Decision[] {
  const out: Decision[] = [];
  for (const message of [...messages].reverse()) {
    for (const part of message.parts) {
      if (!isToolPart(part) || part.state !== "approval-requested") continue;
      const args = part.input && typeof part.input === "object" ? (part.input as Json) : {};
      const approvalId = part.approval.id;
      if (part.toolName === ACTION_TOOL) {
        const task = tasks.find((t) => t.id === args.taskId);
        out.push({
          key: `${message.id}:${part.toolCallId}`,
          label: ACTION_LABELS[String(args.kind)] ?? String(args.kind ?? "Action"),
          subject: task ? task.display_title || task.title : String(args.taskId ?? ""),
          detail: String(args.text ?? ""),
          context: String(args.kind ?? ""),
          decide: (approve) => command("DecideChatAction", { message_id: message.id, action_id: approvalId, accept: approve }),
        });
      } else {
        out.push({
          key: `${message.id}:${part.toolCallId}`,
          label: `Tool approval · ${connectionOf(part) ?? "tool"}`,
          subject: part.toolName,
          detail: JSON.stringify(args, null, 2),
          args,
          decide: (approve) => command("DecideChatTool", { turn_id: message.id, call_id: part.toolCallId, approve }),
        });
      }
    }
  }
  return out;
}

/**
 * The same list from the daemon's ledger in the workbench snapshot, for the
 * parts of the app that count what waits without a conversation loaded (the
 * shell's badge, the palette). The ledger and the parts are written together.
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

/**
 * The card the user answers: T3 Code's approval drawer on the composer's top
 * edge (ComposerPendingApprovalPanel / Actions), holding what is being
 * approved, the exact thing, and two buttons.
 */
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
    <ComposerApprovalDrawer
      data-slot="decision"
      role="alertdialog"
      aria-label={decision.label}
      error={error}
      actions={<ComposerPendingApprovalActions isResponding={!!busy} onDecide={onDecide} approveKey={isMac ? "⌘↵" : "Ctrl↵"} />}
    >
      <ComposerPendingApprovalPanel label={decision.label} subject={decision.subject} detail={decision.detail} pendingCount={count} />
    </ComposerApprovalDrawer>
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
