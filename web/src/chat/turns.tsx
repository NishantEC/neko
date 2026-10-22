// One turn of Neko's conversation, drawn with T3 Code's timeline rows
// (chat/t3/TimelineUserMessageRow.tsx and TimelineAssistantRows.tsx, from
// t3code @3b95e7831, MIT): your message as a bubble on the right with its time
// and Copy on hover; Neko's reply as prose with its work log above it and its
// meta row (Copy, time) below.
import { cn } from "@/ui/lib/utils";
import { ToolCallLine, WorkLog } from "@/chat/work-log";
import { UserTimelineRow } from "@/chat/t3/TimelineUserMessageRow";
import { AssistantTimelineRow } from "@/chat/t3/TimelineAssistantRows";
import type { ChatAction, NekoMessage } from "@/chat/types";

/**
 * While a reply streams, only finished blocks are drawn: text up to the last
 * blank line, with an open code fence held back. A half-typed table or list
 * never reflows; the next paragraph arrives whole.
 */
export function finishedBlocks(text: string): string {
  const cut = text.lastIndexOf("\n\n");
  if (cut < 0) return "";
  let head = text.slice(0, cut);
  // A code fence still open inside the finished part is held back whole.
  const fence = /^ {0,3}(```|~~~)/gm;
  let open = -1;
  for (let m = fence.exec(head); m; m = fence.exec(head)) open = open < 0 ? m.index : -1;
  if (open >= 0) head = head.slice(0, open).trimEnd();
  return head;
}

const ACTION_VERB: Record<string, string> = { message: "Tell its agent" };

/** Neko's proposals that were answered (the pending ones dock on the composer). */
export function SettledActions({ actions }: { actions: ChatAction[] }) {
  const settled = actions.filter((a) => a.status !== "Proposed");
  return (
    <>
      {settled.map((action) => (
        <div key={action.id} className="my-[0.65rem] rounded-xl border border-border/70 bg-card/60 px-3 py-2.5 shadow-xs">
          <div className="flex items-baseline justify-between gap-3">
            <span className="text-sm font-medium text-foreground">{ACTION_VERB[action.kind] ?? action.kind}</span>
            <span
              data-status={action.status}
              className={cn(
                "text-xs",
                action.status === "Declined" ? "text-muted-foreground" : action.status === "Failed" ? "text-destructive" : "text-success-foreground",
              )}
            >
              {action.status}
            </span>
          </div>
          <p className="mt-1 text-xs text-muted-foreground">{action.text}</p>
        </div>
      ))}
    </>
  );
}

export function UserTurn({ message }: { message: NekoMessage }) {
  return <UserTimelineRow text={message.text} createdAt={message.at_ms} failed={message.failed} className="pt-4 first:pt-6" />;
}

export function NekoTurn({ message }: { message: NekoMessage }) {
  const calls = message.tool_calls ?? [];
  const hasLog = !!message.pending || (message.started_ms ?? 0) > 0 || (message.activity?.length ?? 0) > 0;
  const text = message.pending ? finishedBlocks(message.text) : message.text;
  return (
    <div data-slot="neko-turn" data-pending={!!message.pending} className="pt-4">
      {hasLog ? <WorkLog message={message} /> : null}
      {calls.length ? (
        <div className="pb-2">
          {calls.map((call) => (
            <ToolCallLine key={call.id} call={call} />
          ))}
        </div>
      ) : null}
      <AssistantTimelineRow text={text} streaming={!!message.pending} createdAt={message.at_ms} showMeta={!!message.text}>
        {message.failed ? <p className="text-xs text-warning-foreground">This reply did not finish. Try sending it again.</p> : null}
        {message.remembered?.length ? <p className="mt-2 text-xs text-muted-foreground/70">Remembered: {message.remembered.join(" · ")}</p> : null}
        {message.actions?.length ? <SettledActions actions={message.actions} /> : null}
      </AssistantTimelineRow>
    </div>
  );
}
