// A task on the board: the notched card (work/board/NotchCard.tsx) at compact density,
// from real task data. The notch says where the task stands (Stopped, Asks you, Needs
// approval, Ready to review, Up next; running, the live step and its clock). A task that
// waits on you carries its one action on the card (Start again, Answer, Approve), the same
// actions the task page runs; Done cards have no notch. The focusable element is a
// full-size button under the content: it carries the drag listeners, the context menu
// and the hooks the checks use (data-slot="task-row", data-card, data-task-id).
import { useEffect, useState, type ComponentProps, type ReactNode } from "react";
import { useDraggable } from "@dnd-kit/core";
import { ArrowBendUpLeft, ArrowCounterClockwise, Check } from "@/ui/icons";
import { cn } from "@/ui/lib/utils";
import {
  ContextMenu, ContextMenuContent, ContextMenuItem, ContextMenuSeparator, ContextMenuTrigger,
} from "@/ui/context-menu";
import {
  COLUMNS, friendlyStatus, moveCommand, pullRequestNumber, repositoryOf, sourceOf, stageOf, STAGE_TITLE, stopReason, taskTitle, waitingReason,
  type Snapshot, type Task,
} from "@/work/model";
import { run } from "@/work/actions";
import { moveTask, taskActions, type Ui } from "@/work/taskActions";
import { runtimeOf, shortId } from "@/work/board/moves";
import { clock, NotchCard, shortAge, type NotchAction, type NotchCardProps } from "@/work/board/NotchCard";
import type { Tone } from "@/work/tone";

/** The agent runtime that works on the task, as a small round avatar (the task list uses it). */
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

const LIVE = ["Planning", "Building", "Reviewing"];

/** Seconds since `ms`, ticking once a second while `on`. */
function useSecondsSince(ms: number, on: boolean): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (!on) return;
    const id = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(id);
  }, [on]);
  return ms > 0 ? Math.max(0, (now - ms) / 1000) : 0;
}

/** What the card shows for a task: its state, reason and action, from the task's own status. */
export function cardFor(task: Task, snapshot: Snapshot, ui: Ui | null, onOpen: ((id: string) => void) | null) {
  const status: string = task.status;
  const repo = repositoryOf(snapshot, task);
  const source = sourceOf(snapshot, task);
  const pr = pullRequestNumber(task);
  const age = shortAge(task.updated_at_ms);
  const done = stageOf(status) === "done";
  const actions = ui ? taskActions(task, snapshot) : [];
  const runAction = (id: string) => () => { const a = actions.find((x) => x.id === id); if (a?.run && ui) a.run(ui); };
  let tone: Tone = "neutral";
  let label = friendlyStatus(status);
  let reason: string | null = null;
  let action: NotchAction | null = null;
  if (status === "Failed") {
    tone = "failed"; label = "Stopped"; reason = stopReason(task);
    if (actions.some((a) => a.id === "start")) action = { label: "Start again", icon: ArrowCounterClockwise, onClick: runAction("start") };
  } else if (status === "AwaitingApproval") {
    tone = "attention";
    const question = waitingReason(task);
    if (question) {
      label = "Asks you";
      reason = question.replace(/^Needs your input: /, "");
      if (onOpen) action = { label: "Answer", icon: ArrowBendUpLeft, onClick: () => onOpen(task.id) };
    } else {
      label = "Needs approval";
      if (actions.some((a) => a.id === "approve")) action = { label: "Approve", icon: Check, onClick: runAction("approve") };
    }
  } else if (status === "ReadyForReview") {
    tone = "review"; label = "Ready to review";
  } else if (status === "Queued") {
    label = "Up next";
  } else if (LIVE.includes(status)) {
    tone = "running";
  } else if (done) {
    tone = status === "Completed" ? "done" : "neutral";
  }
  const meta = [status === "Cancelled" ? "Cancelled" : null, repo, source, age].filter(Boolean).join(" · ");
  return { tone, label, reason, action, repo, source, pr, age, done, live: LIVE.includes(status), meta };
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
  ui?: Ui | null;
  onOpen?: ((id: string) => void) | null;
  /** The focusable element (the board's draggable button). */
  handle?: ReactNode;
}

/** The card itself; the drag overlay wears the same face, without a handle. */
export function CardFace({ task, snapshot, selected, ghost, overlay, index = 0, ui = null, onOpen = null, handle }: CardViewProps) {
  const c = cardFor(task, snapshot, ui, onOpen);
  const seconds = useSecondsSince(task.updated_at_ms, c.live);
  const props: NotchCardProps = {
    tone: c.tone,
    title: taskTitle(task),
    density: "compact",
    // the repository rides in the notch only when no action or PR needs the room
    state: c.done ? null : { label: c.label, age: c.age, detail: c.action || c.pr ? undefined : c.repo ?? undefined },
    live: c.live ? { step: friendlyStatus(task.status), clock: clock(seconds) } : null,
    reason: c.reason,
    meta: c.meta,
    pr: c.pr,
    action: overlay ? null : c.action,
  };
  return (
    <NotchCard
      {...props}
      data-slot={overlay ? "board-card-overlay" : "board-card"}
      data-status={task.status}
      selected={selected}
      ghost={ghost}
      lifted={overlay}
      handle={handle}
      className={cn(!overlay && "board-card-in", overlay && "cursor-grabbing")}
      style={{ "--i": Math.min(index, 8) } as React.CSSProperties}
    />
  );
}

/** The accessible name of a card: title first, then where it stands, then the rest. */
export function cardLabel(task: Task, snapshot: Snapshot): string {
  const c = cardFor(task, snapshot, null, null);
  const state = c.done ? friendlyStatus(task.status) : c.live ? friendlyStatus(task.status) : c.label;
  return [taskTitle(task), state, c.reason, c.repo, c.source, c.pr ? `pull request ${c.pr}` : null, shortId(task)].filter(Boolean).join(", ");
}

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
  const handle = (
    <CardMenu task={task} onOpen={onOpen} ui={ui}>
      <HandleButton
        ref={setNodeRef}
        {...aria}
        {...listeners}
        data-card="board-card"
        data-task-id={task.id}
        data-status={task.status}
        aria-roledescription="draggable task"
        aria-current={selected ? "true" : undefined}
        aria-label={cardLabel(task, snapshot)}
        onClick={() => onOpen(task.id)}
      />
    </CardMenu>
  );
  return <CardFace task={task} snapshot={snapshot} selected={selected} ghost={isDragging} index={index} ui={ui} onOpen={onOpen} handle={handle} />;
}

/** The card's hit area: full size, under the content; focus outlines the whole notched shape. */
const HandleButton = ({ ref, className, ...props }: ComponentProps<"button">) => (
  <button
    ref={ref}
    type="button"
    {...props}
    // after the spread: a context-menu trigger passes its own data-slot down
    data-slot="task-row"
    className={cn("absolute inset-0 z-0 cursor-default rounded-[16px] outline-none", className)}
  />
);
