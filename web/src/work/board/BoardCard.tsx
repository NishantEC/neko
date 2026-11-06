// A task on the board: the notched card (work/board/NotchCard.tsx) at compact density,
// from real task data. The notch says where the task stands (Stopped, Asks you, Needs
// approval, Ready to review, Up next; running, the live step and its clock). A task that
// waits on you carries its one action on the card (Start again, Answer, Approve), the same
// actions the task page runs; Done cards have no notch. The focusable element is a
// full-size button under the content: it carries the drag listeners, the context menu
// and the hooks the checks use (data-slot="task-row", data-card, data-task-id).
import { cloneElement, useEffect, useState, type ComponentProps, type ReactNode } from "react";
import { useDraggable } from "@dnd-kit/core";
import { ArrowBendUpLeft, ArrowCounterClockwise, ArrowSquareOut, Check, Copy, Link as LinkIcon, SidebarSimple } from "@/ui/icons";
import { cn } from "@/ui/lib/utils";
import { copyText, useContextMenu, type MenuEntry } from "@/shell/context-menu";
import { taskHref } from "@/shell/status";
import {
  COLUMNS, friendlyStatus, moveCommand, pullRequestNumber, repositoryOf, sourceOf, stageOf, STAGE_TITLE, stopReason, taskTitle, waitingReason,
  type Snapshot, type Task,
} from "@/work/model";
import { moveTask, taskActions, type TaskAction, type Ui } from "@/work/taskActions";
import { runtimeOf, shortId } from "@/work/board/moves";
import { clock, NotchCard, shortAge, type NotchAction, type NotchCardProps } from "@/work/board/NotchCard";
import type { Tone } from "@/work/tone";
import { IdLabel, StatusGlyph as SharedGlyph, statusOf } from "@/ui/status-glyph";

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
    // Done cards end on their meta line: the status glyph and the id lead it.
    meta: c.done ? (
      <>
        <SharedGlyph status={statusOf(task.status)} label="" size={12} className="me-1 inline align-[-2px]" />
        <IdLabel className="text-[12px]">{shortId(task)}</IdLabel>{c.meta ? ` · ${c.meta}` : null}
      </>
    ) : c.meta,
    notchLead: <SharedGlyph status={statusOf(task.status)} label="" size={12} />,
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

/**
 * The context menu every task shares (shell/context-menu.tsx): open beside the
 * list or in a tab, the actions taskActions() allows (the same list the task page
 * and ⌘K use), the board moves the daemon allows, then rename, copy and delete.
 */
export function taskMenuItems(task: Task, snapshot: Snapshot, onOpen: ((id: string) => void) | null, ui: Ui): MenuEntry[] {
  const moves = COLUMNS.filter((c) => !c.statuses.includes(task.status) && moveCommand(task.status, c.id).command);
  const actions = taskActions(task, snapshot);
  const act = (a: TaskAction): MenuEntry => ({
    label: a.label,
    destructive: a.kind === "destructive",
    run: () => (a.href ? void window.open(a.href, "_blank", "noreferrer") : a.run?.(ui)),
  });
  const named = (id: string) => actions.find((a) => a.id === id);
  const rename = named("rename");
  const remove = named("delete");
  return [
    onOpen ? { label: "Open", icon: SidebarSimple, run: () => onOpen(task.id) } : null,
    { label: "Open in a tab", icon: ArrowSquareOut, run: () => { location.hash = taskHref(task.id).slice(1); } },
    "separator",
    ...actions.filter((a) => a.id !== "rename" && a.id !== "delete").map(act),
    "separator",
    ...moves.map((c): MenuEntry => ({
      label: c.id === "done" && moveCommand(task.status, c.id).command === "CancelTask" ? "Cancel task" : `Move to ${STAGE_TITLE[c.id]}`,
      run: () => moveTask(task, c.id, ui),
    })),
    "separator",
    rename ? act(rename) : null,
    { label: `Copy id (${shortId(task)})`, icon: Copy, run: () => void copyText(shortId(task), `Copied ${shortId(task)}`) },
    { label: "Copy link", icon: LinkIcon, run: () => void copyText(`${location.origin}${location.pathname}${taskHref(task.id)}`, "Copied the link") },
    remove ? "separator" : null,
    remove ? act(remove) : null,
  ];
}

/** Gives `children` (a row or a card's button) the task's context menu. */
export function CardMenu({ task, snapshot, onOpen, ui, children }: { task: Task; snapshot: Snapshot; onOpen: (id: string) => void; ui: Ui; children: React.ReactElement }) {
  const menu = useContextMenu(() => taskMenuItems(task, snapshot, onOpen, ui));
  return cloneElement(children, menu as Record<string, unknown>);
}

/** A card you can pick up with the pointer, or with Space and the arrow keys. Click and Enter still open the task. */
export function DraggableCard({ task, snapshot, selected, onOpen, ui, index }: CardViewProps & { onOpen: (id: string) => void; ui: Ui }) {
  const { attributes, listeners, setNodeRef, isDragging } = useDraggable({ id: task.id, data: { task } });
  // The element is already a button; dnd-kit's role/aria-pressed would only restate (or contradict) that.
  const { role: _role, "aria-pressed": _pressed, ...aria } = attributes as Record<string, any>;
  const handle = (
    <CardMenu task={task} snapshot={snapshot} onOpen={onOpen} ui={ui}>
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
