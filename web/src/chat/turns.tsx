// One turn of Neko's conversation, drawn with T3 Code's timeline rows
// (chat/t3/TimelineUserMessageRow.tsx and TimelineAssistantRows.tsx, from
// t3code @3b95e7831, MIT): your message as a bubble on the right with its time
// and Copy on hover; Neko's reply as prose with its work log above it and its
// meta row (Copy, time) below.
//
// The turn is the SDK's `UIMessage`: its work (reasoning, tools) folds into the
// work log, and what follows is the reply, prose and typed `data-neko-*` views
// in the order they were said.
import { Fragment } from "react";
import { cn } from "@/ui/lib/utils";
import { WorkLog } from "@/chat/work-log";
import { UserTimelineRow } from "@/chat/t3/TimelineUserMessageRow";
import { AssistantTimelineRow } from "@/chat/t3/TimelineAssistantRows";
import ChatMarkdown from "@/chat/t3/ChatMarkdown";
import { ReplyAnswered, ReplyData } from "@/chat/reply-views";
import { ACTION_TOOL, isToolPart, type NekoPart, type NekoUIMessage, type ToolPart } from "@/chat/sdk/types";
import { messageText, replyText, splitTurn } from "@/chat/sdk/parts";

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
export function SettledActions({ parts }: { parts: ToolPart[] }) {
  const settled = parts.filter((p) => p.toolName === ACTION_TOOL && p.state !== "approval-requested" && p.state !== "input-available" && p.state !== "input-streaming");
  return (
    <>
      {settled.map((part) => {
        const input = (part.input ?? {}) as { kind?: string; text?: string };
        const status = part.state === "output-denied" || (part.state === "approval-responded" && part.approval?.approved === false) ? "Declined" : part.state === "output-error" ? "Failed" : "Done";
        return (
          <div key={part.toolCallId} data-slot="settled-action" className="my-[0.65rem] rounded-xl border border-border/70 bg-card/60 px-3 py-2.5 shadow-xs">
            <div className="flex items-baseline justify-between gap-3">
              <span className="text-sm font-medium text-foreground">{ACTION_VERB[input.kind ?? ""] ?? input.kind}</span>
              <span
                data-status={status}
                className={cn("text-xs", status === "Declined" ? "text-muted-foreground" : status === "Failed" ? "text-destructive" : "text-success-foreground")}
              >
                {status}
              </span>
            </div>
            <p className="mt-1 text-xs text-muted-foreground">{part.state === "output-error" && part.errorText ? part.errorText : input.text}</p>
          </div>
        );
      })}
    </>
  );
}

export function UserTurn({ message }: { message: NekoUIMessage }) {
  return <UserTimelineRow text={messageText(message)} createdAt={message.metadata?.atMs ?? 0} failed={message.metadata?.state === "failed"} className="pt-4 first:pt-6" />;
}

function Block({ part, last, streaming }: { part: NekoPart; last: boolean; streaming: boolean }) {
  if (part.type === "text") {
    const text = streaming && last ? finishedBlocks(part.text) : part.text;
    return text ? <div data-slot="prose"><ChatMarkdown text={text} isStreaming={streaming && last} /></div> : null;
  }
  if (part.type === "data-neko-memory") {
    const items = part.data.items ?? [];
    return items.length ? <p className="mt-2 text-xs text-muted-foreground/70">Remembered: {items.join(" · ")}</p> : null;
  }
  if (part.type.startsWith("data-neko-")) return <ReplyData kind={part.type.slice("data-".length)} data={(part as { data: unknown }).data} />;
  return null;
}

/** `answered`: a later message exists, so the questions in this one are closed. */
export function NekoTurn({ message, answered = false }: { message: NekoUIMessage; answered?: boolean }) {
  const meta = message.metadata ?? {};
  const pending = meta.state === "pending";
  const { work, reply } = splitTurn(message.parts);
  const text = replyText(reply);
  const actions = message.parts.filter((p): p is ToolPart => isToolPart(p) && p.toolName === ACTION_TOOL);
  const lastIndex = reply.length - 1;
  return (
    <div data-slot="neko-turn" data-pending={pending} className="pt-4">
      {work.length || pending ? <WorkLog message={message} work={work} /> : null}
      <AssistantTimelineRow
        text={text}
        streaming={pending}
        createdAt={meta.atMs}
        showMeta={!!text}
        blocks={
          <ReplyAnswered value={answered}>
            {reply.map((part, index) => <Fragment key={index}><Block part={part} last={index === lastIndex} streaming={pending} /></Fragment>)}
          </ReplyAnswered>
        }
      >
        {meta.state === "failed" ? <p className="text-xs text-warning-foreground">This reply did not finish. Try sending it again.</p> : null}
        {actions.length ? <SettledActions parts={actions} /> : null}
      </AssistantTimelineRow>
    </div>
  );
}
