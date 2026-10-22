// from t3code apps/web/src/components/chat/ComposerPrimaryActions.tsx @3b95e7831 (MIT)
// (adapted: the three states Neko has — send, queue while a reply runs, stop
// when nothing is typed — with T3's buttons, glyphs and tooltips. Dropped:
// pending-question navigation, plan Refine / Implement, resume, compaction.
// `data-slot="send"` / `data-mode` are Neko's, for its checks.)
import { memo } from "react";
import { ListPlus } from "lucide-react";
import { cn } from "@/ui/lib/utils";
import { Spinner } from "@/ui/spinner";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/ui/tooltip";

export const ComposerPrimaryActions = memo(function ComposerPrimaryActions({
  isRunning, canInterrupt, hasSendableContent, isSendBusy, disabled, onInterrupt, sendTooltip,
}: {
  isRunning: boolean;
  canInterrupt: boolean;
  hasSendableContent: boolean;
  isSendBusy: boolean;
  disabled: boolean;
  onInterrupt?: () => void;
  sendTooltip?: string;
}) {
  if (canInterrupt && !hasSendableContent) {
    return (
      <Tooltip key="interrupt">
        <TooltipTrigger
          render={
            <button
              type="button"
              data-slot="send"
              data-mode="stop"
              className="flex size-8 shrink-0 cursor-pointer items-center justify-center rounded-full bg-destructive/90 text-white shadow-xs shadow-destructive/24 inset-shadow-control-highlight transition-colors duration-150 hover:bg-destructive active:inset-shadow-control-pressed active:shadow-none sm:h-8 sm:w-8 [&_svg]:pointer-events-none"
              onClick={() => onInterrupt?.()}
              aria-label="Stop"
            />
          }
        >
          <svg width="12" height="12" viewBox="0 0 12 12" fill="currentColor" aria-hidden="true">
            <rect x="2" y="2" width="8" height="8" rx="1.5" />
          </svg>
        </TooltipTrigger>
        <TooltipContent>Stop this reply</TooltipContent>
      </Tooltip>
    );
  }

  const isQueuing = isRunning;
  const submitLabel = isSendBusy ? "Sending" : isQueuing ? "Queue message" : "Send";
  const submitTooltip = sendTooltip ?? (isQueuing ? "Queue message (⌘↵ interrupts and sends)" : "Send (↵)");

  return (
    <Tooltip key="submit">
      <TooltipTrigger render={<span className="inline-flex" />}>
        <button
          type="submit"
          data-slot="send"
          data-mode={isQueuing ? "queue" : "send"}
          className={cn(
            "relative isolate flex h-9 w-9 items-center justify-center overflow-hidden rounded-full shadow-xs transition-all duration-150 enabled:cursor-pointer enabled:inset-shadow-control-highlight hover:scale-105 active:inset-shadow-control-pressed active:shadow-none disabled:pointer-events-none disabled:opacity-64 disabled:shadow-none disabled:hover:scale-100 sm:h-8 sm:w-8 [&_svg]:pointer-events-none",
            "bg-message-action text-message-action-foreground enabled:shadow-message-action/24 hover:bg-message-action-hover",
          )}
          disabled={disabled || isSendBusy || !hasSendableContent}
          aria-label={submitLabel}
        >
          {isSendBusy ? (
            <Spinner className="size-4" aria-hidden="true" />
          ) : isQueuing ? (
            <ListPlus className="size-4" aria-hidden="true" />
          ) : (
            <svg width="14" height="14" viewBox="0 0 14 14" fill="none" aria-hidden="true">
              <path d="M7 11.5V2.5M7 2.5L3 6.5M7 2.5L11 6.5" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" />
            </svg>
          )}
        </button>
      </TooltipTrigger>
      <TooltipContent>{submitTooltip}</TooltipContent>
    </Tooltip>
  );
});
