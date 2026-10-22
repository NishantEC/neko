// from t3code apps/web/src/components/chat/TimelineUserMessageRow.tsx @3b95e7831 (MIT)
// (adapted: UserTimelineRow, CollapsibleUserMessageBody and UserMessageBody,
// fed from Neko's message — text, time, a "not sent" flag — instead of T3's
// row context. Attachments are the daemon's own references in the text,
// `![Image](file://…)` / `[name](file://…)`, shown as T3 shows file rows. Dropped:
// context chips, videos, revert, prompt links, automation folding.)
import { memo, useState, type ReactNode } from "react";
import { File as FileIcon, Image as ImageIcon } from "lucide-react";
import { Button } from "@/ui/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/ui/tooltip";
import { cn } from "@/ui/lib/utils";
import ChatMarkdown from "./ChatMarkdown";
import { MessageCopyButton } from "./MessageCopyButton";
import { MessageAuthorHeading, formatChatTimestampTooltip, formatDayAwareTimestamp } from "./TimelineRowParts";

const MAX_COLLAPSED_USER_MESSAGE_LINES = 8;
const MAX_COLLAPSED_USER_MESSAGE_LENGTH = 600;

export function shouldCollapseUserMessage(text: string): boolean {
  if (text.trim().length === 0) return false;
  return text.length > MAX_COLLAPSED_USER_MESSAGE_LENGTH || text.split("\n").length > MAX_COLLAPSED_USER_MESSAGE_LINES;
}

export interface MessageFile {
  name: string;
  path: string;
  image: boolean;
}

const REFERENCE = /(!?)\[([^\]]*)\]\((file:\/\/[^)\s]+)\)/g;

/** The daemon's attachment references taken out of the text, so they never load as images. */
export function splitAttachments(text: string): { text: string; files: MessageFile[] } {
  const files: MessageFile[] = [];
  const rest = text.replace(REFERENCE, (_all, bang: string, label: string, url: string) => {
    const path = decodeURIComponent(url.replace(/^file:\/\//, ""));
    files.push({ name: label && label !== "Image" ? label : path.split("/").pop() || label, path, image: bang === "!" });
    return "";
  });
  return { text: rest.replace(/\n{3,}/g, "\n\n").trim(), files };
}

const COLLAPSED_USER_MESSAGE_FADE_HEIGHT_REM = 1.75;
const COLLAPSED_USER_MESSAGE_FADE_MASK = `linear-gradient(to bottom, black calc(100% - ${COLLAPSED_USER_MESSAGE_FADE_HEIGHT_REM}rem), transparent)`;

export const CollapsibleUserMessageBody = memo(function CollapsibleUserMessageBody(props: { text: string; footer?: ReactNode }) {
  const [expanded, setExpanded] = useState(false);
  const hasVisibleBody = props.text.trim().length > 0;
  const canCollapse = hasVisibleBody && shouldCollapseUserMessage(props.text);
  const isCollapsed = canCollapse && !expanded;

  return (
    <div>
      {hasVisibleBody ? (
        <div
          className={cn("relative", isCollapsed && "max-h-44 overflow-hidden")}
          data-user-message-body="true"
          data-user-message-collapsed={isCollapsed ? "true" : "false"}
          style={isCollapsed ? { WebkitMaskImage: COLLAPSED_USER_MESSAGE_FADE_MASK, maskImage: COLLAPSED_USER_MESSAGE_FADE_MASK } : undefined}
        >
          <UserMessageBody text={props.text} />
        </div>
      ) : null}
      {canCollapse || props.footer ? (
        <div className={cn("mt-1.5 flex items-center gap-2", canCollapse && props.footer ? "justify-between" : "justify-end")} data-user-message-footer="true">
          {canCollapse ? (
            <Button
              type="button"
              size="xs"
              variant="ghost"
              aria-expanded={expanded}
              onClick={() => setExpanded((value) => !value)}
              className="-ml-1 h-6 px-1.5 text-muted-foreground hover:text-foreground"
            >
              {expanded ? "Show less" : "Show full message"}
            </Button>
          ) : null}
          {props.footer ? <div className="ml-auto flex items-center gap-2">{props.footer}</div> : null}
        </div>
      ) : null}
    </div>
  );
});

const UserMessageBody = memo(function UserMessageBody(props: { text: string }) {
  if (props.text.length === 0) return null;
  return <ChatMarkdown text={props.text} className="text-message-foreground" lineBreaks />;
});

export function UserTimelineRow({
  text, createdAt, failed, label, className,
}: {
  text: string;
  createdAt: number;
  failed?: boolean;
  /** Shown above the bubble: who sent it, when it was not you. */
  label?: string | null;
  className?: string;
}) {
  const { text: body, files } = splitAttachments(text);
  return (
    <div data-slot="user-turn" className={cn("group flex flex-col items-end gap-1", className)}>
      {label ? <p className="me-1 text-2xs text-muted-foreground/70">{label}</p> : null}
      <div className="relative max-w-[80%] rounded-2xl bg-message p-3 text-message-foreground">
        <MessageAuthorHeading>You</MessageAuthorHeading>
        {files.length > 0 ? (
          <div className={cn("flex flex-col gap-1", body && "mb-2")} data-slot="message-files">
            {files.map((file) => (
              <div key={file.path} className="flex min-w-0 items-center gap-2 py-1 text-sm" title={file.path}>
                {file.image ? <ImageIcon className="size-4 shrink-0 opacity-70" /> : <FileIcon className="size-4 shrink-0 opacity-70" />}
                <span className="min-w-0 flex-1 truncate">{file.name}</span>
              </div>
            ))}
          </div>
        ) : null}
        <CollapsibleUserMessageBody text={body} />
      </div>
      {failed ? (
        <div className="me-1 flex items-center gap-1.5">
          <span className="rounded-full border border-destructive/25 bg-destructive/8 px-1.5 py-0.5 text-3xs font-medium text-destructive">Not sent</span>
        </div>
      ) : null}
      <div className="flex w-full max-w-[80%] items-center justify-end pe-1 text-xs tabular-nums opacity-0 transition-opacity duration-200 pointer-coarse:opacity-100 focus-within:opacity-100 group-hover:opacity-100">
        <div className="flex shrink-0 items-center gap-2">
          <Tooltip>
            <TooltipTrigger render={<p className="text-muted-foreground text-xs tabular-nums" />}>
              {formatDayAwareTimestamp(createdAt)}
            </TooltipTrigger>
            <TooltipContent>{formatChatTimestampTooltip(createdAt)}</TooltipContent>
          </Tooltip>
          <div className="flex items-center gap-0.5">{body ? <MessageCopyButton text={body} variant="ghost" /> : null}</div>
        </div>
      </div>
    </div>
  );
}
