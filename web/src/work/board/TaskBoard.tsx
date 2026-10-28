// The Tasks board: calm columns (the gallery's option A, chosen 2026-10-28). No panels, just
// 12px of space between columns, each under a header with its dot, name and count (⋯ on hover).
// An empty column shrinks to a slim note; Done shows its latest five and "Show all N". Cards
// you carry between them. Dragging is @dnd-kit (pointer and keyboard), and a
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
import { DotsThree, Plus, X } from "@/ui/icons";
import { Button } from "@/ui/button";
import { cn } from "@/ui/lib/utils";
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger } from "@/ui/dropdown-menu";
import { COLUMNS, STAGE_EMPTY, STAGE_TITLE, taskTitle, type Snapshot, type Stage, type Task } from "@/work/model";
import { moveTask, type Ui } from "@/work/taskActions";
import { STAGE_TONE, TONE_DOT } from "@/work/tone";
import { CardFace, DraggableCard } from "@/work/board/BoardCard";
import { decide, setBoardDragging, shortId } from "@/work/board/moves";

export interface BoardProps {
  snapshot: Snapshot;
  /** Tasks per column id, already filtered and sorted. */
  byStage: Record<Stage, Task[]>;
  selectedId: string | null;
  onOpen: (id: string) => void;
  ui: Ui;
  /** Done shows every card, not only its latest five. */
  doneAll: boolean;
  setDoneAll: (all: boolean) => void;
  /** The stage tab that is on: the board shows that column alone, its cards flowing across. */
  only?: Stage | null;
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
  return next ? { x: next.rect.left + 16, y: currentCoordinates.y } : currentCoordinates;
};

const reducedMotion = () => typeof matchMedia === "function" && matchMedia("(prefers-reduced-motion: reduce)").matches;

// --- parts ----------------------------------------------------------------------------------

export function ColumnHeader({ stage, count, children }: { stage: Stage; count: number; children?: React.ReactNode }) {
  return (
    <div data-slot="group-header" data-stage={stage} className="flex h-8 shrink-0 items-center gap-1.5 px-0.5">
      <span aria-hidden className={cn("size-2 shrink-0 rounded-full", TONE_DOT[STAGE_TONE[stage]])} />
      <h3 className="truncate text-[13.5px] font-semibold text-strong">{STAGE_TITLE[stage]}</h3>
      <span data-slot="column-count" className="text-[12.5px] tabular-nums text-muted-foreground">{count}</span>
      <span className="flex-1" />
      {children}
    </div>
  );
}

/** Revealed on the column's hover, on focus, and while its menu is open. */
const ON_HOVER = "opacity-0 transition-opacity duration-150 group-hover/col:opacity-100 focus-visible:opacity-100 data-[popup-open]:opacity-100 motion-reduce:transition-none";
const DONE_CAP = 5;

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

function Column({ stage, tasks, active, props, menu, solo }: { stage: Stage; tasks: Task[]; active: Task | null; props: BoardProps; menu: React.ReactNode; solo: boolean }) {
  const { setNodeRef, isOver } = useDroppable({ id: stage });
  const verdict = active ? decide(active, stage) : null;
  const target = verdict && verdict.kind !== "same";
  const empty = tasks.length === 0;
  const capped = stage === "done" && !props.doneAll && tasks.length > DONE_CAP;
  const shown = capped ? tasks.slice(0, DONE_CAP) : tasks;
  return (
    <section
      ref={setNodeRef}
      data-slot="board-column"
      data-stage={stage}
      data-empty={empty ? "" : undefined}
      data-drop={target ? (verdict.kind === "move" ? "ok" : "refuse") : undefined}
      data-over={isOver ? "" : undefined}
      aria-label={`${STAGE_TITLE[stage]}, ${tasks.length}`}
      className={cn(
        // no panel: the column is its header and its cards; the 1.5px inset keeps the hover and drop wash off the cards
        "group/col -mx-1.5 flex min-h-0 flex-col rounded-2xl px-1.5 transition-[background-color,box-shadow,opacity] duration-150",
        empty && !solo ? "w-[144px] shrink-0" : "min-w-[208px] flex-1",
        target && verdict.kind === "move" && (isOver ? "bg-primary/[0.06] ring-1 ring-primary/45" : "ring-1 ring-primary/15"),
        target && verdict.kind === "refuse" && (isOver ? "ring-1 ring-foreground/20" : "opacity-55"),
      )}
    >
      <ColumnHeader stage={stage} count={tasks.length}>{empty ? (stage === "upNext" ? menu : null) : menu}</ColumnHeader>
      <div
        className={cn(
          "-mx-1 min-h-0 flex-1 overflow-y-auto px-1 pt-1.5 pb-4",
          solo ? "grid auto-rows-min content-start gap-2 [grid-template-columns:repeat(auto-fill,minmax(260px,1fr))]" : "flex flex-col gap-2",
        )}
      >
        {shown.map((task, i) => (
          <DraggableCard key={task.id} task={task} snapshot={props.snapshot} selected={props.selectedId === task.id} onOpen={props.onOpen} ui={props.ui} index={i} />
        ))}
        {stage === "done" && tasks.length > DONE_CAP ? (
          <button
            type="button"
            data-slot="show-all"
            aria-expanded={props.doneAll}
            onClick={() => props.setDoneAll(!props.doneAll)}
            className="h-8 shrink-0 rounded-xl text-[12.5px] font-medium text-muted-foreground outline-none transition-colors duration-150 hover:bg-foreground/[0.04] hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring/60"
          >
            {props.doneAll ? "Show fewer" : `Show all ${tasks.length}`}
          </button>
        ) : null}
        {active && target ? <DropSlot stage={stage} task={active} hovered={isOver} /> : null}
        {empty && !target ? (
          <p data-slot="column-empty" className="px-0.5 py-1 text-[12.5px] leading-[1.5] text-muted-foreground">{STAGE_EMPTY[stage]}</p>
        ) : null}
      </div>
    </section>
  );
}

// --- the board ------------------------------------------------------------------------------

export function TaskBoard(props: BoardProps) {
  const { byStage, snapshot, ui } = props;
  const [active, setActive] = useState<Task | null>(null);
  const [notice, setNotice] = useState<{ id: number; text: string } | null>(null);
  const noticeTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const columns = COLUMNS.filter((c) => !props.only || c.id === props.only);

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
      <div data-slot="task-board" data-dragging={active ? "" : undefined} className="relative flex h-full min-h-0 gap-3 px-7 pt-1">
        {columns.map((c) => (
          <Column
            key={c.id} stage={c.id} tasks={byStage[c.id]} active={active} props={props} solo={columns.length === 1}
            menu={
              <>
                {c.id === "upNext" ? (
                  <Button variant="ghost" size="icon-xs" aria-label="New task" title="New task (opens Home)" onClick={props.onNewTask}><Plus /></Button>
                ) : null}
                {byStage[c.id].length > 0 && columns.length > 1 ? (
                  <DropdownMenu>
                    <DropdownMenuTrigger render={<Button variant="ghost" size="icon-xs" aria-label={`${STAGE_TITLE[c.id]} options`} className={ON_HOVER} />}><DotsThree weight="bold" /></DropdownMenuTrigger>
                    <DropdownMenuContent align="end">
                      <DropdownMenuItem onClick={() => props.onShowOnly(c.id)}>Show only {STAGE_TITLE[c.id]}</DropdownMenuItem>
                    </DropdownMenuContent>
                  </DropdownMenu>
                ) : null}
              </>
            }
          />
        ))}

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
