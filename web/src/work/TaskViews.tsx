// The task list (TicketsView.swift). Each row is a real button: Tab reaches it, Enter opens
// it, and j/k on the page move focus between them. Groups follow the stage mapping the whole
// app uses, so a count here is the count everywhere. The board lives in work/board.
import { GitPullRequest } from "@/ui/icons";
import { Chip } from "@/ui/chip";
import { cn } from "@/ui/lib/utils";
import {
  blockedNote, COLUMNS, friendlyStatus, pullRequestNumber, relative, repositoryOf, sourceOf, STAGE_TITLE, taskTitle, workspaceName,
  type Snapshot, type Stage, type Task,
} from "@/work/model";
import type { Ui } from "@/work/taskActions";
import { StatusGlyph, STAGE_TONE, TONE_DOT } from "@/work/tone";
import { CardMenu, RuntimeAvatar } from "@/work/board/BoardCard";
import { shortId } from "@/work/board/moves";

export interface ViewProps {
  snapshot: Snapshot;
  /** Tasks per column id, already filtered and sorted. */
  byStage: Record<Stage, Task[]>;
  selectedId: string | null;
  onOpen: (id: string) => void;
  ui: Ui;
  showWorkspace: boolean;
}

/** A task as one calm line: status, title, why it waits, repository, pull request, agent, age. */
function ListRow({ task, snapshot, selected, onOpen, ui, showWorkspace }: { task: Task; snapshot: Snapshot; selected: boolean; onOpen: (id: string) => void; ui: Ui; showWorkspace: boolean }) {
  const note = blockedNote(task);
  const repo = repositoryOf(snapshot, task);
  const pr = pullRequestNumber(task);
  const source = sourceOf(snapshot, task);
  const title = taskTitle(task);
  return (
    <CardMenu task={task} onOpen={onOpen} ui={ui}>
      <button
        type="button"
        data-slot="task-row"
        data-task-id={task.id}
        data-status={task.status}
        aria-current={selected ? "true" : undefined}
        aria-label={`${title}, ${friendlyStatus(task.status)}${repo ? `, ${repo}` : ""}`}
        onClick={() => onOpen(task.id)}
        className={cn(
          "group flex h-11 w-full items-center gap-3 rounded-xl px-3 text-start text-[14px] outline-none transition-colors duration-150",
          "hover:bg-foreground/[0.04] active:bg-foreground/[0.07] focus-visible:bg-foreground/[0.04] focus-visible:ring-2 focus-visible:ring-ring",
          selected && "bg-foreground/[0.06]",
        )}
      >
        <StatusGlyph status={task.status} size={14} />
        <span className="hidden w-[4.25rem] shrink-0 text-[12px] tabular-nums text-muted-foreground @3xl:inline">{shortId(task)}</span>
        <span className="max-w-[60%] shrink-0 truncate font-medium text-strong" title={title}>{title}</span>
        <span className="min-w-0 flex-1 truncate text-[13px] text-muted-foreground" title={note ?? undefined}>
          <span className="hidden @2xl:inline">{note}</span>
        </span>
        {source ? <span className="hidden shrink-0 text-[12.5px] text-muted-foreground @4xl:inline">{source}</span> : null}
        {pr ? (
          <span className="inline-flex shrink-0 items-center gap-1 text-[12.5px] tabular-nums text-muted-foreground" title="Pull request">
            <GitPullRequest size={12} aria-hidden /> {pr}
          </span>
        ) : null}
        {repo ? <Chip className="hidden max-w-[10rem] truncate @xl:inline-flex">{repo}</Chip> : null}
        {showWorkspace ? <Chip className="hidden max-w-[10rem] truncate @4xl:inline-flex">{workspaceName(snapshot, task)}</Chip> : null}
        <RuntimeAvatar snapshot={snapshot} task={task} className="hidden @lg:inline-flex" />
        <time className="w-[4.75rem] shrink-0 text-end text-[12.5px] tabular-nums text-muted-foreground">{relative(task.updated_at_ms)}</time>
      </button>
    </CardMenu>
  );
}

function GroupHeader({ stage, count, spaced }: { stage: Stage; count: number; spaced?: boolean }) {
  return (
    <div data-slot="group-header" data-stage={stage} className={cn("flex h-9 items-center gap-2 px-3", spaced && "mt-6")}>
      <span aria-hidden className={cn("size-2 rounded-full", TONE_DOT[STAGE_TONE[stage]])} />
      <h3 className="text-[13.5px] font-semibold text-strong">{STAGE_TITLE[stage]}</h3>
      <span className="inline-flex h-[18px] min-w-[18px] items-center justify-center rounded-md bg-foreground/[0.07] px-1 text-[12px] font-medium tabular-nums text-muted-foreground">{count}</span>
    </div>
  );
}

export function TaskList(props: ViewProps) {
  const shown = COLUMNS.filter((c) => props.byStage[c.id].length > 0);
  return (
    <div data-slot="task-list" role="list" aria-label="Tasks" className="@container w-full px-6 pt-1 pb-12 md:px-8 xl:px-10">
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

export { TaskBoard } from "@/work/board/TaskBoard";
