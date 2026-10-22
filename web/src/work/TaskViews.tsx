// The task list and the board (TicketsView.swift, TicketCard.swift). Both are
// built from the same `TaskButton` (a real button: Tab reaches it, Enter opens
// it, and j/k on the page move focus between them). Groups follow the stage
// mapping the whole app uses, so a count here is the count everywhere.
import { useState, type DragEvent, type ReactNode } from "react";
import { CaretLeft, CaretRight, GitPullRequest } from "@/ui/icons";
import { Button } from "@/ui/button";
import { cn } from "@/ui/lib/utils";
import {
  ContextMenu, ContextMenuContent, ContextMenuItem, ContextMenuSeparator, ContextMenuTrigger,
} from "@/ui/context-menu";
import {
  blockedNote, COLUMNS, detailLine, friendlyStatus, moveCommand, pullRequestNumber, relative, repositoryOf, sourceOf, STAGE_EMPTY,
  STAGE_TITLE, taskTitle, workspaceName, type Snapshot, type Stage, type Task,
} from "@/work/model";
import { moveTask, type Ui } from "@/work/taskActions";
import { run } from "@/work/actions";
import { StatusGlyph, STAGE_TONE, TONE_DOT, TONE_TEXT, StatusLabel, Tag } from "@/work/tone";
import { HandPalm } from "@/work/icons";

export interface ViewProps {
  snapshot: Snapshot;
  /** Tasks per column id, already filtered and sorted. */
  byStage: Record<Stage, Task[]>;
  selectedId: string | null;
  onOpen: (id: string) => void;
  ui: Ui;
  showWorkspace: boolean;
}

/** The context menu every task shares: open, run next, and the moves the daemon allows. */
function TaskMenu({ task, onOpen, ui, children }: { task: Task; onOpen: (id: string) => void; ui: Ui; children: ReactNode }) {
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

/** A task as one dense line: status, title, why it waits, repository, pull request, age. */
function ListRow({ task, snapshot, selected, onOpen, ui, showWorkspace }: { task: Task; snapshot: Snapshot; selected: boolean; onOpen: (id: string) => void; ui: Ui; showWorkspace: boolean }) {
  const note = blockedNote(task);
  const repo = repositoryOf(snapshot, task);
  const pr = pullRequestNumber(task);
  const source = sourceOf(snapshot, task);
  const title = taskTitle(task);
  return (
    <TaskMenu task={task} onOpen={onOpen} ui={ui}>
      <button
        type="button"
        data-slot="task-row"
        data-task-id={task.id}
        data-status={task.status}
        aria-current={selected ? "true" : undefined}
        aria-label={`${title}, ${friendlyStatus(task.status)}${repo ? `, ${repo}` : ""}`}
        onClick={() => onOpen(task.id)}
        className={cn(
          "group flex h-9 w-full items-center gap-2.5 rounded-lg px-2.5 text-start text-[13.5px] outline-none transition-colors",
          "hover:bg-foreground/[0.05] focus-visible:bg-foreground/[0.05] focus-visible:ring-2 focus-visible:ring-ring",
          selected && "bg-foreground/[0.07]",
        )}
      >
        <StatusGlyph status={task.status} size={14} />
        <span className="max-w-[60%] shrink-0 truncate font-medium text-strong" title={title}>{title}</span>
        <span className="min-w-0 flex-1 truncate text-[12.5px] text-muted-foreground" title={note ?? undefined}>
          <span className="hidden @2xl:inline">{note}</span>
        </span>
        {source ? <span className="hidden shrink-0 text-[12px] text-muted-foreground @4xl:inline">{source}</span> : null}
        {pr ? (
          <span className="inline-flex shrink-0 items-center gap-1 rounded-md bg-foreground/[0.06] px-1.5 py-0.5 font-mono text-[11.5px] text-foreground/75" title="Pull request">
            <GitPullRequest size={12} aria-hidden /> {pr}
          </span>
        ) : null}
        {repo ? <span className="shrink-0 text-[12.5px] text-muted-foreground">{repo}</span> : null}
        {showWorkspace ? <Tag className="hidden @4xl:inline-flex">{workspaceName(snapshot, task)}</Tag> : null}
        <time className="w-[4.75rem] shrink-0 text-end text-[12px] tabular-nums text-muted-foreground">{relative(task.updated_at_ms)}</time>
      </button>
    </TaskMenu>
  );
}

function GroupHeader({ stage, count, spaced, children }: { stage: Stage; count: number; spaced?: boolean; children?: ReactNode }) {
  return (
    <div data-slot="group-header" data-stage={stage} className={cn("flex h-8 items-center gap-2 px-2.5", spaced && "mt-4")}>
      <span aria-hidden className={cn("size-2 rounded-full", TONE_DOT[STAGE_TONE[stage]])} />
      <h3 className="text-[13px] font-semibold text-strong">{STAGE_TITLE[stage]}</h3>
      <span className="text-[12.5px] tabular-nums text-muted-foreground">{count}</span>
      <span className="flex-1" />
      {children}
    </div>
  );
}

export function TaskList(props: ViewProps) {
  const shown = COLUMNS.filter((c) => props.byStage[c.id].length > 0);
  return (
    <div data-slot="task-list" role="list" aria-label="Tasks" className="@container w-full px-3.5 pt-2 pb-10">
      {shown.map((column, i) => (
        <section key={column.id} role="listitem" aria-label={STAGE_TITLE[column.id]}>
          <GroupHeader stage={column.id} count={props.byStage[column.id].length} spaced={i > 0} />
          {props.byStage[column.id].map((task) => (
            <ListRow key={task.id} task={task} snapshot={props.snapshot} selected={props.selectedId === task.id} onOpen={props.onOpen} ui={props.ui} showWorkspace={props.showWorkspace} />
          ))}
        </section>
      ))}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Board
// ---------------------------------------------------------------------------

function Card({ task, snapshot, selected, onOpen, ui }: { task: Task; snapshot: Snapshot; selected: boolean; onOpen: (id: string) => void; ui: Ui }) {
  const note = blockedNote(task);
  const detail = detailLine(snapshot, task);
  const title = taskTitle(task);
  return (
    <TaskMenu task={task} onOpen={onOpen} ui={ui}>
      <button
        type="button"
        draggable
        data-slot="task-row"
        data-task-id={task.id}
        data-status={task.status}
        aria-current={selected ? "true" : undefined}
        aria-label={`${title}, ${friendlyStatus(task.status)}`}
        onDragStart={(e: DragEvent) => { e.dataTransfer.setData("text/plain", task.id); e.dataTransfer.effectAllowed = "move"; }}
        onClick={() => onOpen(task.id)}
        className={cn(
          "flex w-full flex-col gap-1.5 rounded-xl border bg-card px-3 py-2.5 text-start outline-none transition-colors",
          "hover:bg-foreground/[0.04] focus-visible:ring-2 focus-visible:ring-ring",
          selected ? "border-primary" : "border-border",
        )}
      >
        <span className="flex items-start gap-2">
          <StatusGlyph status={task.status} size={14} className="mt-[3px]" />
          <span className="line-clamp-2 min-w-0 text-[13.5px] font-medium leading-[1.35] text-strong" title={title}>{title}</span>
        </span>
        {stageIsNeedsYou(task) ? (
          <StatusLabel tone="attention" icon={<HandPalm weight="bold" />} className="ms-[22px] w-fit">{friendlyStatus(task.status)}</StatusLabel>
        ) : null}
        {note ? <span className="ms-[22px] line-clamp-2 text-[12px] leading-[1.4] text-muted-foreground" title={note}>{note}</span> : null}
        <span className="ms-[22px] flex items-center gap-2 text-[11.5px] text-muted-foreground">
          <span className="min-w-0 flex-1 truncate">{detail}</span>
          <time className="shrink-0 tabular-nums">{relative(task.updated_at_ms)}</time>
        </span>
      </button>
    </TaskMenu>
  );
}

const stageIsNeedsYou = (task: Task) => ["AwaitingApproval", "Failed"].includes(task.status);

function Column({ stage, tasks, snapshot, selectedId, onOpen, ui, fold, over, setOver, onDropTask }: {
  stage: Stage; tasks: Task[]; snapshot: Snapshot; selectedId: string | null; onOpen: (id: string) => void; ui: Ui;
  fold?: () => void; over: boolean; setOver: (over: boolean) => void; onDropTask: (id: string) => void;
}) {
  return (
    <section
      data-slot="board-column"
      data-stage={stage}
      aria-label={`${STAGE_TITLE[stage]}, ${tasks.length}`}
      onDragOver={(e) => { e.preventDefault(); setOver(true); }}
      onDragLeave={() => setOver(false)}
      onDrop={(e) => { e.preventDefault(); setOver(false); const id = e.dataTransfer.getData("text/plain"); if (id) onDropTask(id); }}
      className={cn("flex min-h-0 min-w-0 flex-col rounded-2xl border border-transparent", over && "border-primary/50 bg-primary/[0.06]")}
    >
      <GroupHeader stage={stage} count={tasks.length}>
        {fold ? (
          <Button variant="ghost" size="icon-xs" aria-label="Hide the Done column" title="Fold Done into a rail" onClick={fold}><CaretRight /></Button>
        ) : null}
      </GroupHeader>
      <div className="flex min-h-0 flex-1 flex-col gap-2 overflow-y-auto px-1 pt-1 pb-6">
        {tasks.map((task) => (
          <Card key={task.id} task={task} snapshot={snapshot} selected={selectedId === task.id} onOpen={onOpen} ui={ui} />
        ))}
        {tasks.length === 0 ? (
          <p className="px-3 py-10 text-center text-[12.5px] text-muted-foreground">{over ? "Drop to move here" : STAGE_EMPTY[stage]}</p>
        ) : null}
      </div>
    </section>
  );
}

export function TaskBoard({ doneOpen, setDoneOpen, ...props }: ViewProps & { doneOpen: boolean; setDoneOpen: (open: boolean) => void }) {
  const [over, setOver] = useState<Stage | null>(null);
  const columns = COLUMNS.filter((c) => c.id !== "done" || doneOpen);
  const taskById = (id: string) => Object.values(props.byStage).flat().find((t) => t.id === id);
  const drop = (stage: Stage) => (id: string) => {
    const task = taskById(id);
    if (!task) return;
    const column = COLUMNS.find((c) => c.id === stage)!;
    if (column.statuses.includes(task.status)) return;
    moveTask(task, stage, props.ui);
  };
  const doneCount = props.byStage.done.length;
  return (
    <div
      data-slot="task-board"
      className="grid h-full min-h-0 gap-4 px-4 pt-2"
      style={{ gridTemplateColumns: `${columns.map(() => "minmax(210px,1fr)").join(" ")}${doneOpen ? "" : " 44px"}` }}
    >
      {columns.map((c) => (
        <Column
          key={c.id} stage={c.id} tasks={props.byStage[c.id]} snapshot={props.snapshot} selectedId={props.selectedId} onOpen={props.onOpen} ui={props.ui}
          fold={c.id === "done" ? () => setDoneOpen(false) : undefined}
          over={over === c.id} setOver={(on) => setOver(on ? c.id : over === c.id ? null : over)} onDropTask={drop(c.id)}
        />
      ))}
      {!doneOpen ? (
        <button
          type="button"
          data-slot="done-rail"
          aria-label={`Done, ${doneCount} ${doneCount === 1 ? "task" : "tasks"}. Show column`}
          title="Show Done"
          onClick={() => setDoneOpen(true)}
          onDragOver={(e) => { e.preventDefault(); setOver("done"); }}
          onDragLeave={() => setOver(null)}
          onDrop={(e) => { e.preventDefault(); setOver(null); const id = e.dataTransfer.getData("text/plain"); if (id) drop("done")(id); }}
          className={cn(
            "flex h-fit flex-col items-center gap-2 rounded-xl border border-transparent py-2.5 text-muted-foreground outline-none hover:bg-foreground/[0.05] focus-visible:ring-2 focus-visible:ring-ring",
            over === "done" && "border-primary/50 bg-primary/[0.06]",
          )}
        >
          <CaretLeft size={13} weight="bold" aria-hidden />
          <StatusGlyph status="Completed" size={14} />
          <span className={cn("text-[12px] tabular-nums", TONE_TEXT.neutral)}>{doneCount}</span>
        </button>
      ) : null}
    </div>
  );
}
