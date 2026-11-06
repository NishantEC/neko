// How status reads: a glyph per task status (ui/status-glyph.tsx, shared by every surface), a tone
// per stage, and the small labelled pill. Colour is never the only cue: every
// tone comes with a symbol or a word.
import type { ReactNode } from "react";
import { cn } from "@/ui/lib/utils";
import { friendlyStatus, type Stage } from "@/work/model";
import { StatusGlyph as SharedGlyph, statusOf, type StatusKind } from "@/ui/status-glyph";

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

/**
 * A task's status as the one status language (ui/status-glyph.tsx, DESIGN.md §3):
 * the shared glyph, named with the task page's own words ("Making a plan"),
 * pulsing gently while an agent works on it (still under Reduce Motion).
 */
export function StatusGlyph({ status, size = 14, className }: { status: string; size?: number; className?: string }) {
  const live = ["Planning", "Building", "Reviewing"].includes(status);
  return (
    <SharedGlyph
      status={statusOf(status)}
      label={friendlyStatus(status)}
      size={size}
      data-task-status={status}
      className={cn(live && "animate-pulse motion-reduce:animate-none", className)}
    />
  );
}

/** A board or list stage as a glyph: what the tasks in it have in common. */
export const STAGE_GLYPH: Record<Stage, StatusKind> = {
  needsYou: "inProgress",
  review: "review",
  running: "inProgress",
  upNext: "todo",
  done: "done",
};

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
