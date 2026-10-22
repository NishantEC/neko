// from t3code apps/web/src/components/chat/TimelineRowParts.tsx @3b95e7831 (MIT)
// (adapted: timestamps are epoch milliseconds, formatted with Neko's 24-hour
// clock, and the tooltip carries the full date)
import { type ReactNode } from "react";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/ui/tooltip";
import { cn } from "@/ui/lib/utils";

export function formatDayAwareTimestamp(ms: number): string {
  const date = new Date(ms);
  const time = date.toLocaleTimeString([], { hour: "numeric", minute: "2-digit", hour12: false });
  const today = new Date();
  if (date.toDateString() === today.toDateString()) return time;
  const yesterday = new Date(today.getFullYear(), today.getMonth(), today.getDate() - 1);
  if (date.toDateString() === yesterday.toDateString()) return `Yesterday ${time}`;
  return `${date.toLocaleDateString([], { month: "short", day: "numeric" })} ${time}`;
}

export function formatChatTimestampTooltip(ms: number): string {
  return new Date(ms).toLocaleString([], { dateStyle: "full", timeStyle: "short" });
}

export function MessageAuthorHeading({ children }: { children: string }) {
  return <h3 className="sr-only select-none">{children}</h3>;
}

export function TimelineRowTimestamp({
  createdAt,
  className,
  alwaysVisible = false,
}: {
  createdAt: number;
  className?: string;
  alwaysVisible?: boolean;
}) {
  return (
    <Tooltip>
      <TooltipTrigger
        render={
          <span
            className={cn(
              "pointer-events-none absolute me-1 shrink-0 whitespace-nowrap rounded-md text-muted-foreground text-xs tabular-nums opacity-0 group-hover/timeline-row:pointer-events-auto group-hover/timeline-row:static group-hover/timeline-row:opacity-100 group-focus-within/timeline-row:pointer-events-auto group-focus-within/timeline-row:static group-focus-within/timeline-row:opacity-100",
              alwaysVisible && "pointer-events-auto static opacity-100",
              className,
            )}
          />
        }
      >
        {formatDayAwareTimestamp(createdAt)}
      </TooltipTrigger>
      <TooltipContent>{formatChatTimestampTooltip(createdAt)}</TooltipContent>
    </Tooltip>
  );
}

export function ActivityShimmerOverlay({ children }: { children: ReactNode }) {
  return (
    <span aria-hidden className="live-activity-focus pointer-events-none absolute inset-y-0 select-none">
      <span className="live-activity-focus-counter block">
        <span className="live-activity-focus-aligned block text-foreground">{children}</span>
      </span>
    </span>
  );
}
