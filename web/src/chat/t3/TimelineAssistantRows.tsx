// from t3code apps/web/src/components/chat/TimelineAssistantRows.tsx @3b95e7831 (MIT)
// (adapted: AssistantTimelineRow and AssistantMessageMeta, fed from Neko's
// message — text, streaming, time — instead of T3's row context. Dropped:
// citations, fork, changed-files card, plan / HTML / MCP-app rows.)
import { type ReactNode } from "react";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/ui/tooltip";
import { cn } from "@/ui/lib/utils";
import ChatMarkdown from "./ChatMarkdown";
import { MessageCopyButton } from "./MessageCopyButton";
import { MessageAuthorHeading, formatChatTimestampTooltip, formatDayAwareTimestamp } from "./TimelineRowParts";

export function AssistantTimelineRow({
  text, streaming = false, createdAt, author = "Neko", showMeta = true, blocks, children, className,
}: {
  /** The reply as plain text: what Copy copies, and the prose when there are no `blocks`. */
  text: string;
  /** The reply as it is drawn: prose and typed views in the order they were said. */
  blocks?: ReactNode;
  streaming?: boolean;
  createdAt?: number;
  author?: string;
  showMeta?: boolean;
  /** Below the prose: what else the turn carries (settled proposals, notes). */
  children?: ReactNode;
  className?: string;
}) {
  return (
    <div className={cn("group/assistant relative min-w-0 px-1 py-0.5", className)}>
      <MessageAuthorHeading>{author}</MessageAuthorHeading>
      {blocks ?? (text ? (
        <div data-slot="prose">
          <ChatMarkdown text={text} isStreaming={streaming} />
        </div>
      ) : null)}
      {children}
      {showMeta && !streaming && createdAt ? (
        <AssistantMessageMeta className="mt-1.5" text={text} createdAt={createdAt} />
      ) : null}
    </div>
  );
}

export function AssistantMessageMeta({
  className, text, createdAt, alwaysVisible = false,
}: {
  className?: string;
  text: string;
  createdAt: number;
  alwaysVisible?: boolean;
}) {
  return (
    <div
      data-slot="message-meta"
      className={cn(
        "flex items-center gap-2 text-xs tabular-nums transition-opacity duration-200",
        alwaysVisible ? "opacity-100" : "opacity-0 pointer-coarse:opacity-100 focus-within:opacity-100 group-hover/assistant:opacity-100",
        className,
      )}
    >
      {text ? <MessageCopyButton text={text} label="Copy reply" variant="ghost" /> : null}
      <Tooltip>
        <TooltipTrigger render={<p className="text-muted-foreground text-xs tabular-nums" />}>{formatDayAwareTimestamp(createdAt)}</TooltipTrigger>
        <TooltipContent>{formatChatTimestampTooltip(createdAt)}</TooltipContent>
      </Tooltip>
    </div>
  );
}
