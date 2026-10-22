// from t3code apps/web/src/components/chat/TimelineWorkGroupRows.tsx,
// TimelineWorkEntryRow.tsx, TimelineToolActivityIcon.tsx and the turn fold in
// MessagesTimeline.tsx @3b95e7831 (MIT)
//
// Adapted: the rows take plain props (label, icon, failed, detail, time) instead
// of T3's row context and projected items, and there is no virtualised list
// (a Neko turn has tens of steps, not thousands). Kept as T3 draws them:
// "Working for 12s" over the live entry with its shine; a finished turn folds to
// "Worked for 1m 12s ›" with its entries (icon, label, chevron, detail on demand).
// Dropped: subagent / thread links, question answers, images, V2 inspectors.
import { useEffect, useRef, useState, type KeyboardEvent, type ReactNode } from "react";
import {
  BotIcon, BrainIcon, CheckIcon, ChevronDown, ChevronRight, CircleAlertIcon, EyeIcon, GlobeIcon, HammerIcon,
  MessageCircleIcon, SearchIcon, SquarePenIcon, TerminalIcon, WrenchIcon, XIcon, ZapIcon,
} from "lucide-react";
import { cn } from "@/ui/lib/utils";
import { WorkLogDetails, WorkLogList, WorkLogRow } from "./WorkLog";
import { ActivityShimmerOverlay, TimelineRowTimestamp } from "./TimelineRowParts";

// ---- TimelineToolActivityIcon ---------------------------------------------------

export const failedToolIconClassName = "text-tool-error-icon/40";

export type WorkEntryIconName =
  | "bot" | "brain" | "check" | "circle-alert" | "eye" | "globe" | "hammer" | "message-circle" | "search"
  | "square-pen" | "terminal" | "wrench" | "x" | "zap";

const ICONS = {
  bot: BotIcon,
  brain: BrainIcon,
  check: CheckIcon,
  "circle-alert": CircleAlertIcon,
  eye: EyeIcon,
  globe: GlobeIcon,
  hammer: HammerIcon,
  "message-circle": MessageCircleIcon,
  search: SearchIcon,
  "square-pen": SquarePenIcon,
  terminal: TerminalIcon,
  wrench: WrenchIcon,
  x: XIcon,
  zap: ZapIcon,
} as const;

export function ToolActivityIconView(props: { name: WorkEntryIconName; className: string; muted: boolean }) {
  const Icon = ICONS[props.name];
  return <Icon aria-hidden className={cn(props.className, props.muted && "opacity-70")} />;
}

// ---- the live turn ----------------------------------------------------------------

/** "12s", "1m 12s", "1h 5m" (Neko's own wording, which the rest of the app uses). */
export function formatDuration(seconds: number): string {
  const s = Math.max(0, Math.round(seconds));
  if (s < 60) return `${s}s`;
  const m = Math.floor(s / 60);
  if (m < 60) return `${m}m ${s % 60}s`;
  return `${Math.floor(m / 60)}h ${m % 60}m`;
}

function WorkingTimer({ startedAt }: { startedAt: number }) {
  const textRef = useRef<HTMLSpanElement>(null);
  const initialText = formatDuration((Date.now() - startedAt) / 1000);

  useEffect(() => {
    const updateText = () => {
      if (textRef.current) textRef.current.textContent = formatDuration((Date.now() - startedAt) / 1000);
    };
    updateText();
    const id = setInterval(updateText, 1000);
    return () => clearInterval(id);
  }, [startedAt]);

  return (
    <span ref={textRef} className="tabular-nums">
      {initialText}
    </span>
  );
}

export function WorkingTimelineRow({ startedAt, label }: { startedAt?: number | null; label?: ReactNode }) {
  const text = label ?? (startedAt ? <>Working for <WorkingTimer startedAt={startedAt} /></> : "Working...");
  return (
    <div className="border-b border-border/60 pb-2 pt-1">
      <div data-slot="work-head" className="flex h-6 min-w-0 items-baseline gap-2 px-1 text-sm leading-relaxed text-muted-foreground tabular-nums">
        <span className="relative shrink-0 overflow-hidden whitespace-nowrap">{text}</span>
      </div>
    </div>
  );
}

export function LiveActivityRow({
  label, iconName, failed = false, active = false, shimmer = false,
}: {
  label: ReactNode;
  iconName?: WorkEntryIconName;
  failed?: boolean;
  active?: boolean;
  shimmer?: boolean;
}) {
  const animated = active && !failed;
  const showShimmer = animated && shimmer;
  return (
    <div className="relative min-h-6 w-fit max-w-full min-w-0 overflow-hidden rounded-md text-sm leading-relaxed">
      <LiveActivityContent label={label} iconName={iconName} failed={failed} announceFailure={failed} active={animated && !shimmer} />
      {showShimmer ? (
        <ActivityShimmerOverlay>
          <LiveActivityContent label={label} iconName={iconName} highlighted />
        </ActivityShimmerOverlay>
      ) : null}
    </div>
  );
}

function LiveActivityContent({
  label, iconName, failed = false, announceFailure = false, active = false, highlighted = false,
}: {
  label: ReactNode;
  iconName: WorkEntryIconName | undefined;
  failed?: boolean;
  announceFailure?: boolean;
  active?: boolean;
  highlighted?: boolean;
}) {
  return (
    <WorkLogRow
      icon={
        iconName ? (
          <span
            className={cn("flex size-4 items-center justify-center", failed ? failedToolIconClassName : highlighted ? "text-foreground" : "text-icon-muted")}
            role={announceFailure ? "img" : undefined}
            aria-label={announceFailure ? "Tool call failed" : undefined}
          >
            <ToolActivityIconView name={iconName} className="block size-4 shrink-0 stroke-2" muted={!highlighted} />
          </span>
        ) : null
      }
      label={<span className={cn("block truncate", highlighted && "text-foreground", active && "live-tool-shine")}>{label}</span>}
    />
  );
}

// ---- the finished turn ------------------------------------------------------------

/** "Worked for 1m 12s ›", folding what the turn did. */
export function TurnFoldTimelineRow({
  label, expanded, onToggle, createdAt, disabled,
}: {
  label: string;
  expanded: boolean;
  onToggle: () => void;
  createdAt: number;
  disabled?: boolean;
}) {
  return (
    <div className="group/timeline-row relative flex items-center gap-1 border-b border-border/60 pb-2 pe-0.5 pt-1">
      <button
        type="button"
        data-slot="work-head"
        aria-expanded={expanded}
        disabled={disabled}
        onClick={onToggle}
        className="flex cursor-pointer select-none items-center gap-1 rounded-md px-1 text-sm leading-relaxed text-muted-foreground tabular-nums transition-colors enabled:hover:text-foreground disabled:cursor-default focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring/70"
      >
        <span>{label}</span>
        {disabled ? null : expanded ? <ChevronDown className="size-3.5" /> : <ChevronRight className="size-3.5" />}
      </button>
      <TimelineRowTimestamp createdAt={createdAt} className="ms-auto" />
    </div>
  );
}

const toolCallExpandedBodyClassName =
  "max-h-64 cursor-text overflow-auto whitespace-pre-wrap break-words font-mono text-secondary-label text-2xs leading-relaxed select-text";

const stopRowToggle = (e: { stopPropagation: () => void }) => e.stopPropagation();

/** One thing the agent did: an icon, a line, and its detail on demand. */
export function SimpleWorkEntryRow({
  label, aside, iconName, failed = false, warning = false, createdAt, detail, tone = "tool", slot = "work-step", trailing,
}: {
  label: ReactNode;
  /** Muted text after the label. */
  aside?: ReactNode;
  iconName: WorkEntryIconName;
  failed?: boolean;
  /** Waiting on you (amber), rather than failed. */
  warning?: boolean;
  createdAt?: number;
  /** Shown under the row when it is opened. */
  detail?: string | null;
  tone?: "tool" | "info";
  slot?: string;
  trailing?: ReactNode;
}) {
  const [expanded, setExpanded] = useState(false);
  const canExpand = Boolean(detail?.trim());
  const accessible = typeof label === "string" ? (failed ? `${label}, tool call failed` : label) : undefined;
  const rowToggleProps = canExpand
    ? {
        role: "button" as const,
        tabIndex: 0 as const,
        "aria-label": accessible,
        "aria-expanded": expanded,
        onClick: () => setExpanded((v) => !v),
        onKeyDown: (e: KeyboardEvent<HTMLDivElement>) => {
          if (e.target === e.currentTarget && (e.key === "Enter" || e.key === " ")) {
            e.preventDefault();
            setExpanded((v) => !v);
          }
        },
      }
    : {};
  // As T3 draws a failed tool call: the icon tinted, an X at the end, the label left as it was.
  const iconWrapperClass = cn(
    "flex size-4 items-center justify-center",
    warning ? "text-warning" : failed ? failedToolIconClassName : tone === "tool" ? "text-icon-muted" : "text-muted-foreground",
  );
  const headingClass = warning ? "font-medium text-warning-foreground" : "text-secondary-label";

  return (
    <WorkLogRow
      data-slot={slot}
      data-failed={failed || undefined}
      {...rowToggleProps}
      icon={
        <span className={iconWrapperClass} role={failed ? "img" : undefined} aria-label={failed ? "Tool call failed" : undefined}>
          <ToolActivityIconView name={iconName} className="block size-4 shrink-0 stroke-2" muted={!failed} />
        </span>
      }
      label={
        <div className="min-w-0 flex-1 overflow-hidden">
          <p className="flex min-w-0 w-full items-baseline gap-1.5 text-sm leading-relaxed">
            <span className={cn(aside ? "min-w-0" : "min-w-0 flex-1", "truncate", headingClass)}>{label}</span>
            {aside ? <span className="min-w-0 truncate text-muted-foreground/70">{aside}</span> : null}
          </p>
        </div>
      }
      trailing={
        <>
          {trailing ? <span onClick={stopRowToggle} onKeyDown={stopRowToggle}>{trailing}</span> : null}
          {failed ? <XIcon aria-hidden className={cn("size-3 shrink-0", failedToolIconClassName)} /> : null}
          {createdAt ? <TimelineRowTimestamp createdAt={createdAt} /> : null}
          <span className={cn("flex size-4 shrink-0 items-center justify-center", !canExpand && "invisible")} aria-hidden>
            <ChevronRight className={cn("size-3 shrink-0 text-icon-muted opacity-70 transition-transform duration-200", expanded && "rotate-90")} />
          </span>
        </>
      }
    >
      {expanded && canExpand ? (
        <WorkLogDetails kind="panel">
          <pre className={cn(toolCallExpandedBodyClassName, "ms-7")}>{detail}</pre>
        </WorkLogDetails>
      ) : null}
    </WorkLogRow>
  );
}

export { WorkLogList };
