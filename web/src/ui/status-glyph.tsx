// One status language (DESIGN.md §3): every task, routine item and run uses
// these glyphs, everywhere, always with words beside them or as their label.
//
//   backlog     dotted circle, faint          not started, nothing scheduled
//   todo        empty circle, muted           queued, will start
//   inProgress  half-filled circle, amber     working, or waiting on you
//   review      circle, sky                   ready for you to review
//   done        check in a circle, mint       finished
//   failed      cross in a circle, coral      stopped with an error
//   cancelled   cross in a circle, faint      stopped on purpose (no colour: it means nothing needs anyone)
//
// Neko's task statuses (TaskStatus in crates/neko-protocol/src/workbench.rs):
//
//   Queued → todo · Planning, Building, Reviewing → inProgress
//   AwaitingApproval → inProgress (amber: it needs you; the label says so)
//   ReadyForReview → review · Completed → done · Failed → failed · Cancelled → cancelled
//
// Anything else (a routine that has not run, a suggestion) is backlog.
//
//   <StatusGlyph status={statusOf(task.status)} />          // labelled for screen readers
//   <IdLabel>NEK-1BB3</IdLabel>                              // the faint tabular id beside a title
//   <ProgressRing done={3} total={5} />                      // work with parts: ring + "3/5"
import type { ComponentProps } from "react";
import { cn } from "@/ui/lib/utils";

export type StatusKind = "backlog" | "todo" | "inProgress" | "review" | "done" | "failed" | "cancelled";

export const STATUS_LABEL: Record<StatusKind, string> = {
  backlog: "Backlog",
  todo: "To do",
  inProgress: "In progress",
  review: "In review",
  done: "Done",
  failed: "Failed",
  cancelled: "Cancelled",
};

/** A task's status (or any status word Neko uses) as one of the seven glyphs. */
export function statusOf(status: string | null | undefined): StatusKind {
  switch (status) {
    case "Queued": return "todo";
    case "Planning":
    case "Building":
    case "Reviewing":
    case "AwaitingApproval": return "inProgress";
    case "ReadyForReview": return "review";
    case "Completed": return "done";
    case "Failed": return "failed";
    case "Cancelled": return "cancelled";
    default: return "backlog";
  }
}

/** The words for a task status, more exact than the glyph's ("Needs you" for AwaitingApproval). */
export function statusLabel(status: string | null | undefined): string {
  if (status === "AwaitingApproval") return "Needs you";
  if (status === "Planning") return "Planning";
  return STATUS_LABEL[statusOf(status)];
}

const TONE: Record<StatusKind, string> = {
  backlog: "text-muted-foreground/60",
  todo: "text-muted-foreground",
  inProgress: "text-warning",
  review: "text-info",
  done: "text-success",
  failed: "text-destructive",
  cancelled: "text-muted-foreground/60",
};

/**
 * The glyph, 14px by default. `label` names it for assistive tech (default:
 * the status's words); pass `label=""` when the words are already beside it.
 */
export function StatusGlyph({ status, label, size = 14, className, ...props }: { status: StatusKind; label?: string; size?: number } & Omit<ComponentProps<"svg">, "children">) {
  const name = label ?? STATUS_LABEL[status];
  return (
    <svg
      data-slot="status-glyph"
      data-status={status}
      viewBox="0 0 16 16"
      width={size}
      height={size}
      role={name ? "img" : undefined}
      aria-label={name || undefined}
      aria-hidden={name ? undefined : true}
      className={cn("shrink-0", TONE[status], className)}
      fill="none"
      {...props}
    >
      {status === "backlog" ? <circle cx="8" cy="8" r="6" stroke="currentColor" strokeWidth="1.5" strokeDasharray="1.6 2.1" /> : null}
      {status === "todo" ? <circle cx="8" cy="8" r="6" stroke="currentColor" strokeWidth="1.5" /> : null}
      {status === "inProgress" ? (
        <>
          <circle cx="8" cy="8" r="6" stroke="currentColor" strokeWidth="1.5" />
          <path d="M8 4.5a3.5 3.5 0 0 1 0 7z" fill="currentColor" />
        </>
      ) : null}
      {status === "review" ? (
        <>
          <circle cx="8" cy="8" r="6" stroke="currentColor" strokeWidth="1.5" />
          <circle cx="8" cy="8" r="2.25" fill="currentColor" />
        </>
      ) : null}
      {status === "done" ? (
        <>
          <circle cx="8" cy="8" r="7" fill="currentColor" />
          <path d="M5.2 8.2l1.9 1.9 3.7-4" stroke="var(--background)" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" />
        </>
      ) : null}
      {status === "failed" || status === "cancelled" ? (
        <>
          <circle cx="8" cy="8" r="7" fill="currentColor" />
          <path d="M5.8 5.8l4.4 4.4M10.2 5.8l-4.4 4.4" stroke="var(--background)" strokeWidth="1.6" strokeLinecap="round" />
        </>
      ) : null}
    </svg>
  );
}

/**
 * The row's one permanent action, expressed as its status glyph (DESIGN.md §8):
 * a button that completes the thing (mark done…). On hover or focus it
 * previews the outcome as a done check; `label` names the action.
 */
export function GlyphButton({ status, label, onClick, disabled, size = 14, className }: { status: StatusKind; label: string; onClick: () => void; disabled?: boolean; size?: number; className?: string }) {
  return (
    <button
      type="button"
      data-slot="glyph-button"
      aria-label={label}
      title={label}
      disabled={disabled}
      onClick={(e) => { e.stopPropagation(); onClick(); }}
      className={cn("group/glyph -m-1 flex shrink-0 items-center justify-center rounded-full p-1 outline-none transition-colors hover:bg-foreground/[0.06] focus-visible:ring-2 focus-visible:ring-ring/70 disabled:pointer-events-none", className)}
    >
      <StatusGlyph status={status} label="" size={size} className="group-hover/glyph:hidden group-focus-visible/glyph:hidden" />
      <StatusGlyph status="done" label="" size={size} className="hidden group-hover/glyph:block group-focus-visible/glyph:block" />
    </button>
  );
}

/** A short id beside a title (`NEK-1BB3`), faint and tabular. */
export function IdLabel({ className, ...props }: ComponentProps<"span">) {
  return <span data-slot="id-label" className={cn("shrink-0 text-[12px] tabular-nums text-muted-foreground/85", className)} {...props} />;
}

/** Work with parts: a ring filled to done/total and the count (`3/5`). Mint once every part is done. */
export function ProgressRing({ done, total, size = 14, showCount = true, className }: { done: number; total: number; size?: number; showCount?: boolean; className?: string }) {
  const parts = Math.max(0, total);
  const finished = Math.min(Math.max(0, done), parts);
  const ratio = parts === 0 ? 0 : finished / parts;
  const r = 6;
  const c = 2 * Math.PI * r;
  const complete = parts > 0 && finished === parts;
  return (
    <span data-slot="progress-ring" role="img" aria-label={`${finished} of ${parts} done`} className={cn("inline-flex shrink-0 items-center gap-1 text-[12px] tabular-nums text-muted-foreground", className)}>
      <svg aria-hidden viewBox="0 0 16 16" width={size} height={size} fill="none" className={complete ? "text-success" : "text-warning"}>
        <circle cx="8" cy="8" r={r} stroke="currentColor" strokeOpacity="0.25" strokeWidth="2" />
        <circle cx="8" cy="8" r={r} stroke="currentColor" strokeWidth="2" strokeDasharray={`${c * ratio} ${c}`} strokeLinecap="round" transform="rotate(-90 8 8)" />
      </svg>
      {showCount ? <span aria-hidden>{finished}/{parts}</span> : null}
    </span>
  );
}
