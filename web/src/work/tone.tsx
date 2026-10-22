// How status reads: a glyph per task status (the SwiftUI StatusGlyph), a tone
// per stage, and the small labelled pill. Colour is never the only cue: every
// tone comes with a symbol or a word.
import type { ReactNode } from "react";
import { cn } from "@/ui/lib/utils";
import { friendlyStatus, type Stage } from "@/work/model";

export type Tone = "attention" | "review" | "running" | "done" | "failed" | "neutral";

export const TONE_TEXT: Record<Tone, string> = {
  attention: "text-warning-foreground",
  review: "text-info-foreground",
  running: "text-primary",
  done: "text-success-foreground",
  failed: "text-destructive-foreground",
  neutral: "text-muted-foreground",
};

export const TONE_BG: Record<Tone, string> = {
  attention: "bg-warning/12",
  review: "bg-info/12",
  running: "bg-primary/12",
  done: "bg-success/12",
  failed: "bg-destructive/12",
  neutral: "bg-foreground/[0.06]",
};

export const TONE_DOT: Record<Tone, string> = {
  attention: "bg-warning",
  review: "bg-info",
  running: "bg-primary",
  done: "bg-success",
  failed: "bg-destructive",
  neutral: "bg-muted-foreground/60",
};

export const STAGE_TONE: Record<Stage, Tone> = {
  needsYou: "attention",
  review: "review",
  running: "running",
  upNext: "neutral",
  done: "done",
};

const GLYPH_COLOR: Record<string, string> = {
  AwaitingApproval: "text-warning",
  ReadyForReview: "text-info",
  Completed: "text-success",
  Failed: "text-destructive",
  Cancelled: "text-muted-foreground",
  Queued: "text-muted-foreground",
  Planning: "text-primary",
  Building: "text-primary",
  Reviewing: "text-primary",
};

const PROGRESS: Record<string, number> = { Queued: 0, Planning: 0.25, Building: 0.5, Reviewing: 0.75, AwaitingApproval: 0.5, ReadyForReview: 0.5 };

/** A task's status as a small ring: filled for done, "!" for stopped, a pie for work in flight. */
export function StatusGlyph({ status, size = 14, className }: { status: string; size?: number; className?: string }) {
  const color = GLYPH_COLOR[status] ?? "text-muted-foreground";
  const live = ["Planning", "Building", "Reviewing"].includes(status);
  const r = 6;
  const progress = PROGRESS[status] ?? 1;
  const angle = progress * 2 * Math.PI;
  // A pie wedge from 12 o'clock clockwise.
  const x = 8 + 3.4 * Math.sin(angle);
  const y = 8 - 3.4 * Math.cos(angle);
  const wedge = progress >= 1 ? null : progress <= 0 ? null : `M8 8 L8 4.6 A3.4 3.4 0 ${progress > 0.5 ? 1 : 0} 1 ${x.toFixed(2)} ${y.toFixed(2)} Z`;
  return (
    <svg
      role="img"
      aria-label={friendlyStatus(status)}
      viewBox="0 0 16 16"
      width={size}
      height={size}
      data-status={status}
      className={cn("shrink-0", color, live && "animate-pulse motion-reduce:animate-none", className)}
    >
      {status === "Completed" ? (
        <>
          <circle cx="8" cy="8" r="7" fill="currentColor" />
          <path d="M4.9 8.2l2.1 2.1 4.1-4.3" fill="none" stroke="var(--background)" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" />
        </>
      ) : status === "Cancelled" ? (
        <>
          <circle cx="8" cy="8" r={r + 0.25} fill="none" stroke="currentColor" strokeWidth="1.5" />
          <path d="M5.8 5.8l4.4 4.4M10.2 5.8l-4.4 4.4" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
        </>
      ) : status === "Failed" ? (
        <>
          <circle cx="8" cy="8" r="7" fill="currentColor" />
          <path d="M8 4.6v4" stroke="var(--background)" strokeWidth="1.8" strokeLinecap="round" />
          <circle cx="8" cy="11.1" r="1" fill="var(--background)" />
        </>
      ) : (
        <>
          <circle cx="8" cy="8" r={r + 0.25} fill="none" stroke="currentColor" strokeWidth="1.5" strokeDasharray={status === "Queued" ? "2 2" : undefined} />
          {wedge ? <path d={wedge} fill="currentColor" /> : null}
        </>
      )}
    </svg>
  );
}

/** A tinted pill: a symbol (or a spinner for live work) and a word. */
export function StatusLabel({ tone, icon, children, live, className }: { tone: Tone; icon?: ReactNode; children: ReactNode; live?: boolean; className?: string }) {
  return (
    <span
      data-slot="status-label"
      data-tone={tone}
      className={cn(
        "inline-flex h-[20px] shrink-0 items-center gap-1 whitespace-nowrap rounded-md px-1.5 text-[11.5px] font-medium leading-none [&_svg]:size-3",
        TONE_TEXT[tone], TONE_BG[tone], className,
      )}
    >
      {live ? (
        <span aria-hidden className="size-2.5 animate-spin rounded-full border-[1.5px] border-current border-t-transparent motion-reduce:animate-none" />
      ) : icon}
      {children}
    </span>
  );
}

/** A neutral tag: a workspace, a tool, a mode name. */
export function Tag({ children, className }: { children: ReactNode; className?: string }) {
  return (
    <span data-slot="tag" className={cn("inline-flex h-[20px] max-w-40 shrink-0 items-center truncate rounded-md bg-foreground/[0.06] px-1.5 text-[11.5px] font-medium text-foreground/70", className)}>
      {children}
    </span>
  );
}
