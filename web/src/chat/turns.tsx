// One turn of Neko's conversation, built from the vendored ai-elements
// Message / MessageContent / MessageActions / MessageResponse.
import { useState } from "react";
import { Check, Copy } from "@/ui/icons";
import { cn } from "@/ui/lib/utils";
import { Message, MessageAction, MessageActions, MessageContent, MessageResponse } from "@/chat/message";
import { ToolCallLine, WorkLog } from "@/chat/work-log";
import { clock } from "@/chat/work";
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

/** Neko's proposals that were answered (the pending ones dock above the composer). */
export function SettledActions({ actions }: { actions: ChatAction[] }) {
  const settled = actions.filter((a) => a.status !== "Proposed");
  return (
    <>
      {settled.map((action) => (
        <div key={action.id} className="mt-3.5 rounded-xl border border-border bg-foreground/[0.03] px-3.5 py-3">
          <div className="flex items-baseline justify-between gap-3">
            <span className="font-medium text-strong">{ACTION_VERB[action.kind] ?? action.kind}</span>
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
          <p className="mt-1.5 text-[13px] text-muted-foreground">{action.text}</p>
        </div>
      ))}
    </>
  );
}

export function UserTurn({ message }: { message: NekoMessage }) {
  return (
    <Message from="user" data-slot="user-turn" className="mt-10 max-w-full first:mt-7">
      <MessageContent className="max-w-[80%] gap-0 whitespace-pre-wrap break-words text-[14px] leading-[1.625] group-[.is-user]:rounded-2xl group-[.is-user]:bg-bubble group-[.is-user]:px-4 group-[.is-user]:py-3 group-[.is-user]:text-bubble-foreground">
        {message.text}
      </MessageContent>
      {message.failed ? <span className="ml-auto mt-1 text-xs text-muted-foreground/70">Not sent</span> : null}
    </Message>
  );
}

export function NekoTurn({ message }: { message: NekoMessage }) {
  const [copied, setCopied] = useState(false);
  const calls = message.tool_calls ?? [];
  const hasLog = !!message.pending || (message.started_ms ?? 0) > 0 || (message.activity?.length ?? 0) > 0;
  const text = message.pending ? finishedBlocks(message.text) : message.text;
  return (
    <Message from="assistant" data-slot="neko-turn" data-pending={!!message.pending} className="mt-7 max-w-full">
      <MessageContent className="w-full gap-0 overflow-visible text-[14px] leading-[1.625]">
        {hasLog ? <WorkLog message={message} /> : null}
        {calls.map((call) => (
          <ToolCallLine key={call.id} call={call} />
        ))}
        {text ? (
          <div data-slot="prose" className="text-foreground">
            <MessageResponse>{text}</MessageResponse>
          </div>
        ) : null}
        {message.failed ? <p className="text-[13px] text-warning-foreground">This reply did not finish. Try sending it again.</p> : null}
        {message.remembered?.length ? (
          <p className="mt-2 text-xs text-muted-foreground/70">Remembered: {message.remembered.join(" · ")}</p>
        ) : null}
        {message.actions?.length ? <SettledActions actions={message.actions} /> : null}
      </MessageContent>
      {!message.pending && message.text ? (
        <MessageActions className="gap-2 text-xs text-muted-foreground/70">
          <MessageAction
            tooltip="Copy"
            label="Copy reply"
            size="icon-xs"
            onClick={() => {
              navigator.clipboard?.writeText(message.text).catch(() => {});
              setCopied(true);
              setTimeout(() => setCopied(false), 1200);
            }}
          >
            {copied ? <Check size={14} /> : <Copy size={14} />}
          </MessageAction>
          <time className="tabular-nums">{copied ? "Copied" : clock(message.at_ms)}</time>
        </MessageActions>
      ) : null}
    </Message>
  );
}
