// The Tasks board, in BoardUI's kanban language: calm trays with a name, a count and quiet
// actions; cards you carry between them. Dragging is @dnd-kit (pointer and keyboard), and a
// drop is only ever one of three things: nothing (same column), the daemon's own command
// (the same one the context menu and the sheet send), or an inline explanation of why not.
//
// Keyboard: Tab to a card, Space picks it up, ← → choose a column, Space or Enter drops it,
// Esc puts it back. Enter on a card still opens it. Every step is announced.
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  DndContext, DragOverlay, KeyboardSensor, PointerSensor, pointerWithin, rectIntersection, useDroppable, useSensor, useSensors,
  type Announcements, type CollisionDetection, type DragEndEvent, type DragOverEvent, type DragStartEvent, type KeyboardCoordinateGetter,
} from "@dnd-kit/core";
import { CaretLeft, DotsThree, Plus, X } from "@/ui/icons";
import { Button } from "@/ui/button";
import { cn } from "@/ui/lib/utils";
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger } from "@/ui/dropdown-menu";
import { COLUMNS, STAGE_EMPTY, STAGE_TITLE, taskTitle, type Snapshot, type Stage, type Task } from "@/work/model";
import { moveTask, type Ui } from "@/work/taskActions";
import { StatusGlyph, STAGE_TONE, TONE_DOT, TONE_TEXT } from "@/work/tone";
import { CardFace, DraggableCard } from "@/work/board/BoardCard";
import { decide, setBoardDragging, shortId } from "@/work/board/moves";

export interface BoardProps {
  snapshot: Snapshot;
  /** Tasks per column id, already filtered and sorted. */
  byStage: Record<Stage, Task[]>;
  selectedId: string | null;
  onOpen: (id: string) => void;
  ui: Ui;
  doneOpen: boolean;
  setDoneOpen: (open: boolean) => void;
  /** Narrow the page to one stage. */
  onShowOnly: (stage: Stage) => void;
  onNewTask: () => void;
}

// --- collisions and keyboard ---------------------------------------------------------------

/** The column under the pointer; with the keyboard (no pointer), the column the card overlaps most. */
const collide: CollisionDetection = (args) => {
  const within = pointerWithin(args);
  return within.length ? within : rectIntersection(args);
};

/** ← → carry the card to the neighbouring column. (Up and down are left to the page.) */
const columnCoordinates: KeyboardCoordinateGetter = (event, { context: { droppableRects, droppableContainers, over, active }, currentCoordinates }) => {
  const step = event.code === "ArrowRight" ? 1 : event.code === "ArrowLeft" ? -1 : 0;
  if (!step) return undefined;
  const columns = droppableContainers.getEnabled()
    .map((c) => ({ id: String(c.id), rect: droppableRects.get(c.id) }))
    .filter((c): c is { id: string; rect: NonNullable<typeof c.rect> } => !!c.rect)
    .sort((a, b) => a.rect.left - b.rect.left);
  if (!columns.length) return undefined;
  const origin = columns.findIndex((c) => c.id === String(over?.id ?? ""));
  const home = columns.findIndex((c) => c.id === (active?.data.current?.task && COLUMNS.find((s) => s.statuses.includes(active.data.current!.task.status))?.id));
  const at = origin >= 0 ? origin : Math.max(0, home);
  const next = columns[Math.min(columns.length - 1, Math.max(0, at + step))];
  event.preventDefault();
  return next ? { x: next.rect.left + 16, y: next.rect.top + 64 } : currentCoordinates;
};

const reducedMotion = () => typeof matchMedia === "function" && matchMedia("(prefers-reduced-motion: reduce)").matches;

// --- parts ----------------------------------------------------------------------------------

function ColumnHeader({ stage, count, children }: { stage: Stage; count: number; children?: React.ReactNode }) {
  return (
    <div data-slot="group-header" data-stage={stage} className="flex h-9 shrink-0 items-center gap-2 px-2">
      <span aria-hidden className={cn("size-2 rounded-full", TONE_DOT[STAGE_TONE[stage]])} />
      <h3 className="text-[13.5px] font-semibold text-strong">{STAGE_TITLE[stage]}</h3>
      <span data-slot="column-count" className="inline-flex h-[18px] min-w-[18px] items-center justify-center rounded-md bg-foreground/[0.07] px-1 text-[12px] font-medium tabular-nums text-muted-foreground">{count}</span>
      <span className="flex-1" />
      {children}
    </div>
  );
}

function DropSlot({ stage, task, hovered }: { stage: Stage; task: Task; hovered: boolean }) {
  const d = decide(task, stage);
  if (d.kind === "same") return null;
  const ok = d.kind === "move";
  return (
    <div
      data-slot="drop-slot"
      data-drop={ok ? "ok" : "refuse"}
      className={cn(
        "flex min-h-[64px] items-center justify-center rounded-xl border border-dashed px-3 py-3 text-center text-[12.5px] leading-[1.4]",
        ok ? "border-primary/50 bg-primary/[0.06] text-primary" : "border-foreground/15 text-muted-foreground",
        !hovered && "opacity-0",
        "transition-opacity duration-150",
      )}
    >
      {ok ? d.verb : d.reason}
    </div>
  );
}

function Column({ stage, tasks, active, props, onMenu }: { stage: Stage; tasks: Task[]; active: Task | null; props: BoardProps; onMenu: React.ReactNode }) {
  const { setNodeRef, isOver } = useDroppable({ id: stage });
  const verdict = active ? decide(active, stage) : null;
  const target = verdict && verdict.kind !== "same";
  return (
    <section
      ref={setNodeRef}
      data-slot="board-column"
      data-stage={stage}
      data-drop={target ? (verdict.kind === "move" ? "ok" : "refuse") : undefined}
      data-over={isOver ? "" : undefined}
      aria-label={`${STAGE_TITLE[stage]}, ${tasks.length}`}
      className={cn(
        "flex min-h-0 min-w-[248px] flex-1 flex-col rounded-2xl border border-transparent bg-foreground/[0.03] p-1.5 transition-[background-color,border-color,opacity] duration-150",
        target && verdict.kind === "move" && (isOver ? "border-primary/50 bg-primary/[0.07]" : "border-primary/20"),
        target && verdict.kind === "refuse" && (isOver ? "border-foreground/20" : "opacity-55"),
      )}
    >
      <ColumnHeader stage={stage} count={tasks.length}>{onMenu}</ColumnHeader>
      <div className="flex min-h-0 flex-1 flex-col gap-2 overflow-y-auto px-0.5 pt-1 pb-3">
        {tasks.map((task, i) => (
          <DraggableCard key={task.id} task={task} snapshot={props.snapshot} selected={props.selectedId === task.id} onOpen={props.onOpen} ui={props.ui} index={i} />
        ))}
        {active && target ? <DropSlot stage={stage} task={active} hovered={isOver} /> : null}
        {tasks.length === 0 && !target ? (
          <p className="px-3 py-10 text-center text-[12.5px] leading-[1.5] text-muted-foreground">{STAGE_EMPTY[stage]}</p>
        ) : null}
      </div>
    </section>
  );
}

function DoneRail({ count, active, onOpen }: { count: number; active: Task | null; onOpen: () => void }) {
  const { setNodeRef, isOver } = useDroppable({ id: "done" });
  const verdict = active ? decide(active, "done") : null;
  return (
    <button
      ref={setNodeRef as any}
      type="button"
      data-slot="done-rail"
      data-drop={verdict && verdict.kind !== "same" ? (verdict.kind === "move" ? "ok" : "refuse") : undefined}
      aria-label={`Done, ${count} ${count === 1 ? "task" : "tasks"}. Show column`}
      title="Show Done"
      onClick={onOpen}
      className={cn(
        "flex h-fit w-11 shrink-0 flex-col items-center gap-2 rounded-2xl border border-transparent bg-foreground/[0.03] py-3 text-muted-foreground outline-none transition-colors duration-150",
        "hover:bg-foreground/[0.06] active:bg-foreground/[0.09] focus-visible:ring-2 focus-visible:ring-ring",
        verdict?.kind === "move" && (isOver ? "border-primary/50 bg-primary/[0.07]" : "border-primary/20"),
        verdict?.kind === "refuse" && (isOver ? "border-foreground/20" : "opacity-55"),
      )}
    >
      <CaretLeft size={13} weight="bold" aria-hidden />
      <StatusGlyph status="Completed" size={14} />
      <span className={cn("text-[12px] tabular-nums", TONE_TEXT.neutral)}>{count}</span>
    </button>
  );
}

// --- the board ------------------------------------------------------------------------------

export function TaskBoard(props: BoardProps) {
  const { byStage, doneOpen, setDoneOpen, snapshot, ui } = props;
  const [active, setActive] = useState<Task | null>(null);
  const [notice, setNotice] = useState<{ id: number; text: string } | null>(null);
  const noticeTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const columns = COLUMNS.filter((c) => c.id !== "done" || doneOpen);

  const tasks = useMemo(() => new Map(Object.values(byStage).flat().map((t) => [t.id as string, t])), [byStage]);
  const stageOf = useCallback((task: Task) => COLUMNS.find((c) => c.statuses.includes(task.status))!.id, []);

  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 6 } }),
    useSensor(KeyboardSensor, { coordinateGetter: columnCoordinates, keyboardCodes: { start: ["Space"], cancel: ["Escape"], end: ["Space", "Enter"] } }),
  );

  useEffect(() => () => clearTimeout(noticeTimer.current), []);
  const say = (text: string) => {
    clearTimeout(noticeTimer.current);
    setNotice({ id: Date.now(), text });
    noticeTimer.current = setTimeout(() => setNotice(null), 7000);
  };

  const describeOver = (task: Task, over: DragOverEvent["over"]) => {
    const to = over ? (String(over.id) as Stage) : null;
    if (!to || !COLUMNS.some((c) => c.id === to)) return `${taskTitle(task)} is not over a column.`;
    const d = decide(task, to);
    if (d.kind === "same") return `${taskTitle(task)} is over ${STAGE_TITLE[to]}, where it already is.`;
    return d.kind === "move" ? `${taskTitle(task)} is over ${STAGE_TITLE[to]}. Dropping here will: ${d.verb}.` : `${taskTitle(task)} is over ${STAGE_TITLE[to]}. It can’t go here. ${d.reason}`;
  };

  const announcements: Announcements = {
    onDragStart: ({ active: a }) => { const t = tasks.get(String(a.id)); return t ? `Picked up ${shortId(t)}, ${taskTitle(t)}, from ${STAGE_TITLE[stageOf(t)]}.` : undefined; },
    onDragOver: ({ active: a, over }) => { const t = tasks.get(String(a.id)); return t ? describeOver(t, over) : undefined; },
    onDragEnd: ({ active: a, over }) => {
      const t = tasks.get(String(a.id));
      if (!t || !over) return t ? `${taskTitle(t)} was dropped outside a column and stays in ${STAGE_TITLE[stageOf(t)]}.` : undefined;
      const to = String(over.id) as Stage;
      const d = decide(t, to);
      if (d.kind === "same") return `${taskTitle(t)} stays in ${STAGE_TITLE[to]}.`;
      return d.kind === "move" ? `${taskTitle(t)} moved to ${STAGE_TITLE[to]}: ${d.verb}.` : `${taskTitle(t)} was not moved. ${d.reason}`;
    },
    onDragCancel: ({ active: a }) => { const t = tasks.get(String(a.id)); return t ? `Move cancelled. ${taskTitle(t)} stays in ${STAGE_TITLE[stageOf(t)]}.` : undefined; },
  };

  const onDragStart = (e: DragStartEvent) => { setBoardDragging(true); setNotice(null); setActive(tasks.get(String(e.active.id)) ?? null); };
  const finish = () => { setActive(null); setBoardDragging(false); };
  const onDragEnd = (e: DragEndEvent) => {
    const task = tasks.get(String(e.active.id));
    finish();
    if (!task || !e.over) return;
    const to = String(e.over.id) as Stage;
    const d = decide(task, to);
    if (d.kind === "move") moveTask(task, to, ui);
    else if (d.kind === "refuse") say(d.reason);
  };

  return (
    <DndContext
      sensors={sensors}
      collisionDetection={collide}
      accessibility={{
        announcements,
        screenReaderInstructions: {
          draggable: "To move this task, press Space. Then use the left and right arrow keys to choose a column, Space or Enter to drop it, and Escape to cancel. Enter on its own opens the task.",
        },
      }}
      onDragStart={onDragStart}
      onDragEnd={onDragEnd}
      onDragCancel={finish}
    >
      <div data-slot="task-board" data-dragging={active ? "" : undefined} className="relative flex h-full min-h-0 gap-3 px-6 pt-1 pb-4 xl:px-10">
        {columns.map((c) => (
          <Column
            key={c.id} stage={c.id} tasks={byStage[c.id]} active={active} props={props}
            onMenu={
              <>
                {c.id === "upNext" ? (
                  <Button variant="ghost" size="icon-xs" aria-label="New task" title="New task (opens Home)" onClick={props.onNewTask}><Plus /></Button>
                ) : null}
                <DropdownMenu>
                  <DropdownMenuTrigger render={<Button variant="ghost" size="icon-xs" aria-label={`${STAGE_TITLE[c.id]} options`} />}><DotsThree weight="bold" /></DropdownMenuTrigger>
                  <DropdownMenuContent align="end">
                    <DropdownMenuItem onClick={() => props.onShowOnly(c.id)}>Show only {STAGE_TITLE[c.id]}</DropdownMenuItem>
                    {c.id === "done" ? <DropdownMenuItem onClick={() => setDoneOpen(false)}>Fold into a rail</DropdownMenuItem> : null}
                  </DropdownMenuContent>
                </DropdownMenu>
              </>
            }
          />
        ))}
        {!doneOpen ? <DoneRail count={byStage.done.length} active={active} onOpen={() => setDoneOpen(true)} /> : null}

        {notice ? (
          <div
            key={notice.id}
            role="status"
            data-slot="board-notice"
            className="board-notice-in pointer-events-auto absolute inset-x-0 bottom-5 z-20 mx-auto flex w-fit max-w-[min(34rem,calc(100%-3rem))] items-start gap-2 rounded-xl border border-border bg-popover px-3.5 py-2.5 text-[13px] leading-[1.45] text-foreground shadow-lg"
          >
            <span className="min-w-0 flex-1">{notice.text}</span>
            <Button variant="ghost" size="icon-xs" aria-label="Dismiss" className="-me-1 -mt-0.5" onClick={() => setNotice(null)}><X /></Button>
          </div>
        ) : null}
      </div>
      <DragOverlay dropAnimation={reducedMotion() ? null : { duration: 200, easing: "cubic-bezier(0.22, 1, 0.36, 1)" }}>
        {active ? <CardFace task={active} snapshot={snapshot} overlay /> : null}
      </DragOverlay>
    </DndContext>
  );
}
