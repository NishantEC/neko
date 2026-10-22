// A task on the board, drawn the way BoardUI draws a kanban ticket: a short id and the
// owner's avatar on top, the title (two lines), the chips (state, repository), and a quiet
// footer with the date. Real Neko data only: the state chip is the task's status, the
// avatar is the agent runtime that works on it, and there is no invented priority.
import { forwardRef, type ComponentProps } from "react";
import { useDraggable } from "@dnd-kit/core";
import { GitPullRequest } from "@/ui/icons";
import { Chip, type ChipTone } from "@/ui/chip";
import { cn } from "@/ui/lib/utils";
import {
  ContextMenu, ContextMenuContent, ContextMenuItem, ContextMenuSeparator, ContextMenuTrigger,
} from "@/ui/context-menu";
import {
  blockedNote, COLUMNS, friendlyStatus, moveCommand, pullRequestNumber, relative, repositoryOf, sourceOf, STAGE_TITLE, taskTitle,
  type Snapshot, type Task,
} from "@/work/model";
import { run } from "@/work/actions";
import { moveTask, type Ui } from "@/work/taskActions";
import { runtimeOf, shortId } from "@/work/board/moves";

const STATUS_TONE: Record<string, ChipTone> = {
  AwaitingApproval: "attention", Failed: "failed", ReadyForReview: "review",
  Planning: "running", Building: "running", Reviewing: "running",
  Queued: "neutral", Completed: "done", Cancelled: "neutral",
};

/** The agent runtime that works on the task, as a small round avatar (BoardUI's Avatar, sm). */
export function RuntimeAvatar({ snapshot, task, className }: { snapshot: Snapshot; task: Task; className?: string }) {
  const runtime = runtimeOf(snapshot, task);
  const label = runtime.model ? `${runtime.label} ${runtime.model}` : runtime.label;
  return (
    <span
      data-slot="runtime-avatar"
      title={label}
      role="img"
      aria-label={`Agent: ${label}`}
      className={cn("inline-flex size-6 shrink-0 select-none items-center justify-center rounded-full bg-foreground/[0.08] text-[10px] font-semibold text-foreground/70", className)}
    >
      {runtime.initials}
    </span>
  );
}

export interface CardViewProps {
  task: Task;
  snapshot: Snapshot;
  selected?: boolean;
  /** The placeholder left behind while the card is carried. */
  ghost?: boolean;
  /** The copy that follows the pointer. */
  overlay?: boolean;
  index?: number;
}

/** The card itself: no behaviour, so the drag overlay can wear the same face. */
export const CardFace = forwardRef<HTMLButtonElement, CardViewProps & Omit<ComponentProps<"button">, "children">>(function CardFace(
  { task, snapshot, selected, ghost, overlay, index = 0, className, style, ...props }, ref,
) {
  const title = taskTitle(task);
  const note = blockedNote(task);
  const repo = repositoryOf(snapshot, task);
  const source = sourceOf(snapshot, task);
  const pr = pullRequestNumber(task);
  const tone = STATUS_TONE[task.status] ?? "neutral";
  const when = relative(task.updated_at_ms);
  return (
    <button
      ref={ref}
      type="button"
      data-card="board-card"
      data-task-id={task.id}
      data-status={task.status}
      data-ghost={ghost ? "" : undefined}
      aria-current={selected ? "true" : undefined}
      aria-label={`${shortId(task)}, ${title}, ${friendlyStatus(task.status)}${repo ? `, ${repo}` : ""}`}
      style={{ ...style, "--i": Math.min(index, 8) } as React.CSSProperties}
      className={cn(
        "group/card flex w-full flex-col gap-2.5 rounded-xl border bg-card p-3 text-start outline-none",
        "shadow-xs transition-[background-color,border-color,box-shadow] duration-150 ease-out",
        "hover:border-foreground/15 active:bg-foreground/[0.04] focus-visible:ring-2 focus-visible:ring-ring",
        selected ? "border-primary/70 ring-1 ring-primary/40" : "border-border",
        !overlay && "board-card-in",
        ghost && "border-dashed bg-transparent opacity-45 shadow-none",
        overlay && "cursor-grabbing border-foreground/15 shadow-lg",
        className,
      )}
      {...props}
      // after the spread: a context-menu trigger wrapping this card passes its own data-slot down
      data-slot={overlay ? "board-card-overlay" : "task-row"}
    >
      <span className="flex items-center gap-2">
        <span className="text-[12px] tabular-nums text-muted-foreground">{shortId(task)}</span>
        <span className="flex-1" />
        <RuntimeAvatar snapshot={snapshot} task={task} />
      </span>
      <span className="line-clamp-2 text-[14px] font-medium leading-[1.4] text-strong" title={title}>{title}</span>
      {note ? <span className="line-clamp-2 text-[12.5px] leading-[1.45] text-muted-foreground" title={note}>{note}</span> : null}
      <span className="flex flex-wrap items-center gap-1.5">
        <Chip tone={tone}>
          <span aria-hidden className="size-1.5 rounded-full bg-current opacity-80" />
          {friendlyStatus(task.status)}
        </Chip>
        {repo ? <Chip className="max-w-[10rem] truncate">{repo}</Chip> : null}
        {source ? <Chip className="max-w-[10rem] truncate">{source}</Chip> : null}
      </span>
      {when || pr ? (
        <span className="flex items-center gap-2 border-t border-border/70 pt-2 text-[12px] text-muted-foreground">
          <time className="min-w-0 flex-1 truncate tabular-nums">{when}</time>
          {pr ? (
            <span className="inline-flex shrink-0 items-center gap-1 tabular-nums" title="Pull request">
              <GitPullRequest size={12} aria-hidden /> {pr}
            </span>
          ) : null}
        </span>
      ) : null}
    </button>
  );
});

/** The context menu every task shares: open, run next, and the moves the daemon allows. */
export function CardMenu({ task, onOpen, ui, children }: { task: Task; onOpen: (id: string) => void; ui: Ui; children: React.ReactElement }) {
  const moves = COLUMNS.filter((c) => !c.statuses.includes(task.status) && moveCommand(task.status, c.id).command);
  return (
    <ContextMenu>
      <ContextMenuTrigger asChild>{children}</ContextMenuTrigger>
      <ContextMenuContent>
        <ContextMenuItem onClick={() => onOpen(task.id)}>Open</ContextMenuItem>
        {task.status === "Queued" ? (
          <ContextMenuItem onClick={() => void run("PrioritizeTask", { task_id: task.id }, "Moved to the front of the queue.")}>Run next</ContextMenuItem>
        ) : null}
        {moves.length ? <ContextMenuSeparator /> : null}
        {moves.map((c) => (
          <ContextMenuItem key={c.id} onClick={() => moveTask(task, c.id, ui)}>
            {c.id === "done" && moveCommand(task.status, c.id).command === "CancelTask" ? "Cancel task" : `Move to ${STAGE_TITLE[c.id]}`}
          </ContextMenuItem>
        ))}
      </ContextMenuContent>
    </ContextMenu>
  );
}

/** A card you can pick up with the pointer, or with Space and the arrow keys. Click and Enter still open the task. */
export function DraggableCard({ task, snapshot, selected, onOpen, ui, index }: CardViewProps & { onOpen: (id: string) => void; ui: Ui }) {
  const { attributes, listeners, setNodeRef, isDragging } = useDraggable({ id: task.id, data: { task } });
  // The element is already a button; dnd-kit's role/aria-pressed would only restate (or contradict) that.
  const { role: _role, "aria-pressed": _pressed, ...aria } = attributes as Record<string, any>;
  return (
    <CardMenu task={task} onOpen={onOpen} ui={ui}>
      <CardFace
        ref={setNodeRef} task={task} snapshot={snapshot} selected={selected} ghost={isDragging} index={index}
        {...aria} {...listeners}
        aria-roledescription="draggable task"
        onClick={() => onOpen(task.id)}
      />
    </CardMenu>
  );
}
