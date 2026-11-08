// The task list (TicketsView.swift). Each row is a real button: Tab reaches it, Enter opens
// it, and j/k on the page move focus between them. Groups follow the stage mapping the whole
// app uses, so a count here is the count everywhere. The board lives in work/board.
import { ArrowCounterClockwise, ArrowSquareOut, CaretRight, GitPullRequest } from "@/ui/icons";
import { GlyphButton, IdLabel, statusOf } from "@/ui/status-glyph";
import { RowAction, RowActions } from "@/setup/kit";
import { useLocal } from "@/work/useLocal";
import { cn } from "@/ui/lib/utils";
import { taskHref } from "@/shell/status";
import {
  blockedNote, COLUMNS, friendlyStatus, pullRequestNumber, relative, repositoryOf, sourceOf, STAGE_TITLE, taskTitle, workspaceName,
  type Snapshot, type Stage, type Task,
} from "@/work/model";
import { taskActions, type Ui } from "@/work/taskActions";
import { StatusGlyph } from "@/work/tone";
import { CardMenu, RuntimeAvatar } from "@/work/board/BoardCard";
import { runtimeOf, shortId } from "@/work/board/moves";

export interface ViewProps {
  snapshot: Snapshot;
  /** Tasks per column id, already filtered and sorted. */
  byStage: Record<Stage, Task[]>;
  selectedId: string | null;
  onOpen: (id: string) => void;
  ui: Ui;
  showWorkspace: boolean;
}

/** The one action a row completes with a click on its glyph (DESIGN.md §8): approve a plan, mark a review done. */
const GLYPH_ACTIONS = ["approve", "done"];

/**
 * A task as one calm line (DESIGN.md §8): the status glyph (a button when the
 * task has a completing action), the faint id, the title, then quiet meta on
 * the right (where it came from, the pull request, the repository, the age).
 * Why it waits shows only while it waits on you, one muted truncated line.
 * The runtime shows only when it is not the default one. Open in a tab and
 * Start again appear on hover; every action is in the right-click menu.
 */
function ListRow({ task, snapshot, selected, onOpen, ui, showWorkspace }: { task: Task; snapshot: Snapshot; selected: boolean; onOpen: (id: string) => void; ui: Ui; showWorkspace: boolean }) {
  const note = blockedNote(task);
  const repo = repositoryOf(snapshot, task);
  const pr = pullRequestNumber(task);
  const source = sourceOf(snapshot, task);
  const title = taskTitle(task);
  const actions = taskActions(task, snapshot);
  const glyphAction = actions.find((a) => GLYPH_ACTIONS.includes(a.id) && a.run);
  const restart = actions.find((a) => a.id === "start" && a.run);
  const runtime = runtimeOf(snapshot, task);
  const defaultProvider = String(snapshot?.agent_runtime?.provider || "codex");
  const where = repo ?? (showWorkspace ? workspaceName(snapshot, task) : null);
  const tip = [title, note, showWorkspace ? `Workspace: ${workspaceName(snapshot, task)}` : null, `Agent: ${runtime.model ? `${runtime.label} ${runtime.model}` : runtime.label}`].filter(Boolean).join("\n");
  return (
    <CardMenu task={task} snapshot={snapshot} onOpen={onOpen} ui={ui}>
      <div
        data-slot="task-line"
        data-selected={selected || undefined}
        className={cn(
          "group/row flex h-10 w-full items-center gap-3 rounded-[10px] px-3 transition-colors duration-150",
          "hover:bg-foreground/[0.03] has-[[data-slot=task-row]:focus-visible]:bg-foreground/[0.04]",
          selected && "bg-foreground/[0.06] hover:bg-foreground/[0.06]",
        )}
      >
        {glyphAction ? (
          <GlyphButton status={statusOf(task.status)} label={`${glyphAction.label}: ${title}`} onClick={() => glyphAction.run?.(ui)} />
        ) : (
          <StatusGlyph status={task.status} size={14} />
        )}
        <button
          type="button"
          data-slot="task-row"
          data-task-id={task.id}
          data-status={task.status}
          aria-current={selected ? "true" : undefined}
          aria-label={`${title}, ${friendlyStatus(task.status)}${repo ? `, ${repo}` : ""}`}
          title={tip}
          onClick={() => onOpen(task.id)}
          className="flex h-full min-w-0 flex-1 items-center gap-3 rounded-md text-start text-[13.5px] outline-none focus-visible:ring-2 focus-visible:ring-ring"
        >
          <IdLabel className="hidden w-[4.25rem] @3xl:inline">{shortId(task)}</IdLabel>
          <span className="max-w-[60%] shrink-0 truncate text-foreground">{title}</span>
          <span className="min-w-0 flex-1 truncate text-[12.5px] text-muted-foreground">
            <span className="hidden @2xl:inline">{note}</span>
          </span>
          {source ? <span className="hidden max-w-[11rem] shrink-0 truncate text-[12.5px] text-muted-foreground/80 @4xl:inline">{source}</span> : null}
          {pr ? (
            <span className="inline-flex shrink-0 items-center gap-1 text-[12.5px] tabular-nums text-muted-foreground/80">
              <GitPullRequest size={12} aria-hidden /> {pr}
            </span>
          ) : null}
          {where ? <span data-slot="task-where" className="hidden max-w-[9rem] shrink-0 truncate text-[12.5px] text-muted-foreground/80 @xl:inline">{where}</span> : null}
          {runtime.provider !== defaultProvider ? <RuntimeAvatar snapshot={snapshot} task={task} className="hidden size-5 text-[9px] @lg:inline-flex" /> : null}
        </button>
        <span className="relative flex w-[4.75rem] shrink-0 items-center justify-end">
          <time className="text-[12.5px] tabular-nums text-muted-foreground/70 group-hover/row:invisible group-focus-within/row:invisible">{relative(task.updated_at_ms)}</time>
          <RowActions className="absolute inset-y-0 end-0">
            {restart ? <RowAction label={`Start “${title}” again`} icon={ArrowCounterClockwise} onClick={() => restart.run?.(ui)} /> : null}
            <RowAction label={`Open “${title}” in a tab`} icon={ArrowSquareOut} onClick={() => { location.hash = taskHref(task.id).slice(1); }} />
          </RowActions>
        </span>
      </div>
    </CardMenu>
  );
}

/** A stage's header: small and quiet, its name and a faint count; a button that folds the group (DESIGN.md §3, §8). */
function GroupHeader({ stage, count, spaced, open, onToggle, controls }: { stage: Stage; count: number; spaced?: boolean; open: boolean; onToggle: () => void; controls: string }) {
  return (
    <h3 data-slot="group-header" data-stage={stage} className={cn("flex h-8 items-center px-1", spaced && "mt-5")}>
      <button
        type="button"
        aria-expanded={open}
        aria-controls={controls}
        onClick={onToggle}
        className="flex h-7 items-center gap-1.5 rounded-md px-2 text-start text-[12.5px] font-medium text-muted-foreground outline-none transition-colors hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring"
      >
        <CaretRight aria-hidden size={10} weight="bold" className={cn("transition-transform duration-150 motion-reduce:transition-none", open && "rotate-90")} />
        <span>{STAGE_TITLE[stage]}</span>
        <span data-slot="group-count" className="tabular-nums text-muted-foreground/70">{count}</span>
      </button>
    </h3>
  );
}

export function TaskList(props: ViewProps & { showEmpty?: boolean }) {
  const shown = COLUMNS.filter((c) => props.showEmpty || props.byStage[c.id].length > 0);
  const [foldedRaw, setFolded] = useLocal("neko.work.folded", "");
  const folded = new Set(foldedRaw.split(",").filter(Boolean));
  const toggle = (stage: Stage) => {
    const next = new Set(folded);
    if (next.has(stage)) next.delete(stage);
    else next.add(stage);
    setFolded([...next].join(","));
  };
  return (
    <div data-slot="task-list" role="list" aria-label="Tasks" className="@container w-full px-4 pt-1 pb-12">
      {shown.map((column, i) => {
        const tasks = props.byStage[column.id];
        const open = !folded.has(column.id);
        const id = `task-group-${column.id}`;
        return (
          <section key={column.id} role="listitem" aria-label={`${STAGE_TITLE[column.id]}, ${tasks.length}`}>
            <GroupHeader stage={column.id} count={tasks.length} spaced={i > 0} open={open} onToggle={() => toggle(column.id)} controls={id} />
            <div id={id} hidden={!open}>
              {tasks.length === 0 ? (
                <p data-slot="group-empty" className="px-3 py-1.5 text-[13px] text-muted-foreground">No tasks</p>
              ) : tasks.map((task) => (
                <ListRow key={task.id} task={task} snapshot={props.snapshot} selected={props.selectedId === task.id} onOpen={props.onOpen} ui={props.ui} showWorkspace={props.showWorkspace} />
              ))}
            </div>
          </section>
        );
      })}
    </div>
  );
}

export { TaskBoard } from "@/work/board/TaskBoard";
