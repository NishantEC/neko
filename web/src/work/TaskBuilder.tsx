// A task opened as a full page, in hushbacks' builder frame
// (apps/spa/src/pages/Builder/index.tsx, components/BuilderDesktopHeader.tsx and
// FormBuilderMode.tsx @da20dfaeb): a header row (breadcrumb, status, the task's
// actions) over the stage.
//
// Neko: the stage is a docking layout (dock/DockWorkspace, dockview) rather than
// hushbacks' fixed chat | views split. Every view is a tab: Chat (the task
// conversation + composer, owned by the chat work and mounted unchanged),
// Changes, Plan and Details. Tabs can be dragged to any pane, dropped on an edge
// to split, grouped, reordered, closed and reopened from the header's View menu.
// The default layout is the old one: Chat on the left (35%), the three views
// grouped on the right. One layout is kept for the person, not per task.
import { type ReactNode, type RefObject } from "react";
import { motion, useReducedMotion } from "motion/react";
import { GitDiff, ShieldCheck } from "@phosphor-icons/react";
import { ArrowLeft, ArrowsOutSimple, ArrowCounterClockwise, ChatCircle, Check, GitPullRequest, Info, Layout, Link as LinkIcon, ListChecks, SquareHalf } from "@/ui/icons";
import { Button } from "@/ui/button";
import { DropdownMenu, DropdownMenuCheckboxItem, DropdownMenuContent, DropdownMenuItem, DropdownMenuSeparator, DropdownMenuTrigger } from "@/ui/dropdown-menu";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/ui/tooltip";
import { hrefFor } from "@/router";
import { useRegisterCommands } from "@/shell/commands";
import { Changes } from "@/work/Changes";
import { MessageResponse } from "@/chat/message";
import { DeliveryCard, PlanBlock, ResultBlock } from "@/work/Thread";
import { STAGE_TONE, StatusLabel, Tag } from "@/work/tone";
import { conversationState, friendlyStatus, pullRequestNumber, repositoryOf, stageOf, taskTitle, type Task } from "@/work/model";
import type { TaskAction, Ui } from "@/work/taskActions";
import { Details, TaskActions } from "@/work/TaskDetail";
import { TaskComposer, TaskConversation } from "@/chat/TaskConversation";
import { DockWorkspace, useDockController, useOpenPanels, type DockGroupSpec, type DockPanelDef } from "@/work/dock/DockWorkspace";

const EASE_OUT: [number, number, number, number] = [0.16, 1, 0.3, 1];

/** One layout for the person, whichever task is open. */
export const TASK_DOCK_KEY = "neko:dock:task";

/** The panels a task page offers, in the order the View menu lists them. */
const PANELS = [
  { id: "chat", title: "Chat", icon: ChatCircle },
  { id: "result", title: "Result", icon: ShieldCheck },
  { id: "changes", title: "Changes", icon: GitDiff },
  { id: "plan", title: "Plan", icon: ListChecks },
  { id: "details", title: "Details", icon: Info },
] as const;

// Chat opens wider than hushbacks' form chat (35%, not 25%): a task conversation carries work logs and tables.
const DEFAULT_LAYOUT: DockGroupSpec[] = [
  { panels: ["chat"], size: 0.35, minWidth: 320 },
  { panels: ["result", "changes", "plan", "details"], active: "changes", of: 0, direction: "right" },
];

/**
 * The task's one result. While the task works again there is no final result: what was saved is
 * shown as the previous one, and says so. Whatever the agent writes there is Markdown with the
 * reply blocks (tables, charts, files, plans), so it can shape its own result.
 */
function ResultPane({ task, state }: { task: Task; state: ReturnType<typeof conversationState> }) {
  const has = String(task.result ?? "").trim() !== "";
  const working = ["Queued", "Planning", "Building"].includes(task.status);
  if (has) {
    return (
      <>
        {working ? <p data-slot="result-working" className="mb-2 text-[12.5px] text-muted-foreground">The agent is working again, so this is the previous result.</p> : null}
        <ResultBlock task={task} outcome={working ? "previous" : state.outcome} reviewIsCurrent={!working && state.reviewIsCurrent} />
        <DeliveryCard task={task} />
      </>
    );
  }
  // No saved result yet: never a blank pane. Show what the agents have said so far.
  const said = (task.events ?? [])
    .filter((e: any) => ["scout", "builder", "reviewer", "supervisor", "coordinator"].includes(e.role))
    .map((e: any) => String(e.message ?? "").trim())
    .filter((m: string) => m.length > 40 && !/^(VERIFICATION_COMMAND|USAGE|\{)/.test(m) && !/^Command (running|completed)/.test(m));
  const latest = said[said.length - 1];
  return (
    <section data-slot="result-progress">
      <p className="mb-2 text-[12.5px] text-muted-foreground">{working ? "No final result yet. This is what the agent has said so far." : "No result was saved. This is the agent's last message."}</p>
      {latest ? <div data-slot="prose" className="text-[14px] leading-[1.625]"><MessageResponse>{latest}</MessageResponse></div> : <p className="mt-10 text-center text-[13px] text-muted-foreground">Nothing to summarise yet.</p>}
    </section>
  );
}

export interface TaskBuilderProps {
  task: Task;
  snapshot: any;
  ui: Ui;
  actions: TaskAction[];
  state: ReturnType<typeof conversationState>;
  onClose: () => void;
  onToggleFull: () => void;
  onOpenTask: (id: string) => void;
  copied: boolean;
  onCopyLink: () => void;
  headingRef: RefObject<HTMLHeadingElement | null>;
}

/** One view inside a pane: scrolled, on the column the side panel uses. */
function StageView({ children, slot }: { children: ReactNode; slot?: string }) {
  return (
    <div data-slot={slot} className="h-full min-h-0 overflow-y-auto bg-card">
      <div className="mx-auto w-full min-w-0 max-w-[920px] px-6 pt-5 pb-10">{children}</div>
    </div>
  );
}

export function TaskBuilder({
  task, snapshot, ui, actions, state, onClose, onToggleFull, onOpenTask, copied, onCopyLink, headingRef,
}: TaskBuilderProps) {
  const reduceMotion = useReducedMotion();
  const dock = useDockController();
  const openPanels = useOpenPanels(dock);

  const stage = stageOf(task.status);
  const repo = repositoryOf(snapshot, task);
  const prNumber = pullRequestNumber(task);

  useRegisterCommands(
    [
      ...PANELS.map((p) => ({
        id: `task-view.${p.id}`,
        title: `View: ${p.title}`,
        group: "Task view",
        icon: p.icon,
        keywords: ["panel", "tab", "show", "open", "focus"],
        run: () => dock.open(p.id),
      })),
      { id: "task-view.reset", title: "Reset layout", group: "Task view", icon: ArrowCounterClockwise, keywords: ["panels", "dock", "default"], run: () => dock.reset() },
      { id: "task-view.split-right", title: "Split right", group: "Task view", icon: SquareHalf, keywords: ["panel", "pane", "tab", "side by side"], run: () => dock.splitRight() },
    ],
    [dock],
  );

  const panels: DockPanelDef[] = [
    {
      id: "chat",
      title: "Chat",
      icon: ChatCircle,
      render: () => (
        <div data-slot="dock-chat" className="flex h-full min-h-0 min-w-0 flex-1 flex-col bg-card">
          <TaskConversation task={task} showResult={false} />
          <div className="w-full shrink-0 px-3 pt-1 pb-3">
            <TaskComposer task={task} ui={ui} />
          </div>
        </div>
      ),
    },
    {
      id: "result",
      title: "Result",
      icon: ShieldCheck,
      render: () => (
        <StageView>
          <ResultPane task={task} state={state} />
        </StageView>
      ),
    },
    {
      id: "changes",
      title: "Changes",
      icon: GitDiff,
      render: () => (
        <StageView>
          {task.worktree || task.parked ? <Changes taskId={task.id} active /> : (
            <p data-slot="changes-empty" className="mt-10 text-center text-[13px] text-muted-foreground">This task has no working copy yet, so nothing has changed.</p>
          )}
        </StageView>
      ),
    },
    {
      id: "plan",
      title: "Plan",
      icon: ListChecks,
      render: () => (
        <StageView>
          {String(task.plan ?? "").trim() ? (
            <PlanBlock title={state.showsPlan ? state.planTitle : "Plan"} text={task.plan} />
          ) : (
            <p className="mt-10 text-center text-[13px] text-muted-foreground">The agent has not written a plan for this task.</p>
          )}
        </StageView>
      ),
    },
    {
      id: "details",
      title: "Details",
      icon: Info,
      render: () => (
        <StageView>
          <div className="mb-6 flex flex-wrap items-center gap-2 text-[12.5px] text-muted-foreground">
            {repo ? <Tag>{repo}</Tag> : null}
            {prNumber ? (
              <a href={task.pull_request.url} target="_blank" rel="noreferrer" className="inline-flex items-center gap-1 rounded-md hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring">
                <GitPullRequest size={13} aria-hidden /> {prNumber}
              </a>
            ) : null}
            <span className="font-mono text-[11px] text-muted-foreground/70">NEK-{String(task.id).slice(0, 4).toUpperCase()}</span>
          </div>
          <Details task={task} snapshot={snapshot} open={onOpenTask} />
        </StageView>
      ),
    },
  ];

  const headerEnd = (
    <div className="flex shrink-0 items-center gap-2">
      <TaskActions task={task} snapshot={snapshot} actions={actions} ui={ui} repo={repo} className="flex-nowrap" />
      <DropdownMenu>
        <DropdownMenuTrigger render={<Button variant="ghost" size="sm" aria-label="View: choose panels" data-slot="view-menu" />}>
          <Layout aria-hidden /> View
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end" className="min-w-48">
          {PANELS.map((p) => (
            <DropdownMenuCheckboxItem key={p.id} checked={openPanels.includes(p.id)} onCheckedChange={() => dock.toggle(p.id)}>
              {p.title}
            </DropdownMenuCheckboxItem>
          ))}
          <DropdownMenuSeparator />
          <DropdownMenuItem onClick={() => dock.reset()}><ArrowCounterClockwise aria-hidden /> Reset layout</DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
      <Tooltip>
        <TooltipTrigger asChild>
          <Button variant="ghost" size="icon-sm" aria-label="Copy a link to this task" onClick={onCopyLink}>{copied ? <Check /> : <LinkIcon />}</Button>
        </TooltipTrigger>
        <TooltipContent>{copied ? "Copied" : "Copy link"}</TooltipContent>
      </Tooltip>
      <Tooltip>
        <TooltipTrigger asChild>
          <Button variant="ghost" size="icon-sm" aria-label="Show beside the list" onClick={onToggleFull}><ArrowsOutSimple /></Button>
        </TooltipTrigger>
        <TooltipContent>Show beside the list</TooltipContent>
      </Tooltip>
    </div>
  );

  return (
    <article
      data-slot="task-detail"
      data-task-id={task.id}
      data-frame="builder"
      aria-label={taskTitle(task)}
      className="builder-shell flex h-full min-h-0 flex-1 flex-col bg-[var(--builder-chrome)]"
    >
      <motion.header
        initial={reduceMotion ? false : { opacity: 0, y: -6 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: reduceMotion ? 0 : 0.35, ease: EASE_OUT }}
        className="flex min-h-[52px] shrink-0 items-center gap-3 px-3"
      >
        <nav aria-label="Breadcrumb" className="flex min-w-0 flex-1 items-center gap-1.5 text-sm">
          <a
            href={hrefFor("tasks")}
            onClick={(e) => { e.preventDefault(); onClose(); }}
            aria-label="Back to Tasks"
            title="Back to Tasks (Esc)"
            className="flex shrink-0 items-center gap-1.5 rounded-md px-1 text-muted-foreground transition-colors hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
          >
            <ArrowLeft size={14} aria-hidden /> Tasks
          </a>
          <span aria-hidden className="shrink-0 text-foreground/30">/</span>
          <h2 className="min-w-0 truncate font-medium text-foreground" title={taskTitle(task)}>
            <span ref={headingRef} tabIndex={-1} className="outline-none">{taskTitle(task)}</span>
          </h2>
          <div className="ml-2 hidden shrink-0 items-center gap-2 text-[12.5px] text-muted-foreground min-[640px]:flex">
            <StatusLabel tone={STAGE_TONE[stage]} live={["Planning", "Building", "Reviewing"].includes(task.status)}>{friendlyStatus(task.status)}</StatusLabel>
          </div>
        </nav>
        {headerEnd}
      </motion.header>

      <div className="min-h-0 flex-1">
        <DockWorkspace panels={panels} defaultLayout={DEFAULT_LAYOUT} storageKey={TASK_DOCK_KEY} controller={dock} />
      </div>
    </article>
  );
}
