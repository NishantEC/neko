// from t3code apps/web/src/components/chat/TimelineSystemDivider.tsx @3b95e7831 (MIT)
// (adapted: Neko's tooltip; a role=separator with the label, for screen readers)
import type { ReactNode } from "react";
import type { LucideIcon } from "lucide-react";

import { cn } from "@/ui/lib/utils";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/ui/tooltip";

export function TimelineSystemDivider(props: {
  readonly label: string;
  readonly detail?: ReactNode | null;
  readonly tone?: "neutral" | "danger";
  readonly icon?: LucideIcon;
  readonly showDetailSeparator?: boolean;
  readonly actionLabel?: string;
  readonly onAction?: () => void;
  readonly className?: string;
}) {
  const Icon = props.icon;
  const content = (
    <>
      {Icon ? <Icon className="size-3 shrink-0" /> : null}
      <span className="max-w-[36rem] truncate font-medium" title={props.label}>{props.label}</span>
      {props.detail ? (
        <span className={cn("inline-flex min-w-0 items-center opacity-70", typeof props.detail === "string" && "max-w-80 truncate")}>
          {props.showDetailSeparator === false ? null : "· "}
          {props.detail}
        </span>
      ) : null}
    </>
  );

  return (
    <div
      role="separator"
      aria-label={props.label}
      className={cn("flex min-w-0 items-center gap-2 py-2 text-2xs text-muted-foreground", props.tone === "danger" && "text-destructive", props.className)}
    >
      <span aria-hidden="true" className="h-px flex-1 bg-border/70" />
      {props.onAction ? (
        <Tooltip>
          <TooltipTrigger
            render={
              <button
                type="button"
                aria-label={props.actionLabel}
                onClick={props.onAction}
                className="flex min-w-0 cursor-pointer items-center gap-1.5 rounded-full border border-border/70 bg-background px-2.5 py-1 transition-colors hover:bg-accent hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring/70"
              />
            }
          >
            {content}
          </TooltipTrigger>
          <TooltipContent side="top">{props.actionLabel}</TooltipContent>
        </Tooltip>
      ) : (
        <span className="flex min-w-0 flex-wrap items-center justify-center gap-1.5 rounded-full px-2 py-1">{content}</span>
      )}
      <span aria-hidden="true" className="h-px flex-1 bg-border/70" />
    </div>
  );
}
