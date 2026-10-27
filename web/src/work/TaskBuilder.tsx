// A task opened as a full page, in hushbacks' builder frame
// (apps/spa/src/pages/Builder/index.tsx, components/BuilderDesktopHeader.tsx and
// FormBuilderMode.tsx @da20dfaeb): a header row whose left column carries the
// breadcrumb, the status and the fused panel tab, and whose right column is
// where the view tabs and the actions mount; under it a resizable split, the
// chat on the left in its rounded panel surface and the stage on the right
// with the task's views (Changes, Plan, Details).
//
// Neko: the form's ChatProvider, presence, sync and publish controls are gone;
// the chat side mounts the task conversation (TaskThread + Composer, owned by the
// chat work) unchanged; the header end is the task's own actions. The chat panel
// opens wider than hushbacks' (35% default, 45% max, against 25% / 30%) because
// a task conversation carries work logs and tables a form chat does not.
import { useCallback, useEffect, useState, type ReactNode, type RefObject } from "react";
import { motion, useReducedMotion } from "motion/react";
import { GitDiff, ShieldCheck } from "@phosphor-icons/react";
import { ArrowLeft, ArrowsOutSimple, ChatCircle, Check, GitPullRequest, Info, Link as LinkIcon, ListChecks } from "@/ui/icons";
import { Button } from "@/ui/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/ui/tooltip";
import { cn } from "@/ui/lib/utils";
import { hrefFor } from "@/router";
import { Changes } from "@/work/Changes";
import { MessageResponse } from "@/chat/message";
import { DeliveryCard, PlanBlock, ResultBlock } from "@/work/Thread";
import { STAGE_TONE, StatusLabel, Tag } from "@/work/tone";
import { conversationState, friendlyStatus, pullRequestNumber, repositoryOf, stageOf, taskTitle, type Task } from "@/work/model";
import type { TaskAction, Ui } from "@/work/taskActions";
import { Details, TaskActions } from "@/work/TaskDetail";
import { TaskComposer, TaskConversation } from "@/chat/TaskConversation";
import { BuilderViewTabs, type BuilderViewTabItem } from "@/work/builder/BuilderViewTabs";
import { BUILDER_HEADER_SLOT } from "@/work/builder/builderHeaderLayout";

const EASE_OUT: [number, number, number, number] = [0.16, 1, 0.3, 1];


// One row of tabs, switchable, none closable (owner, 2026-10-27): the conversation is a tab
// like the rest, not a side panel.
type View = "chat" | "result" | "changes" | "plan" | "details";
const VIEW_TABS: BuilderViewTabItem<View>[] = [
  { title: "Chat", icon: ChatCircle, value: "chat" },
  { title: "Result", icon: ShieldCheck, value: "result" },
  { title: "Changes", icon: GitDiff, value: "changes" },
  { type: "separator" },
  { title: "Plan", icon: ListChecks, value: "plan" },
  { title: "Details", icon: Info, value: "details" },
];

function useTaskViewMode(taskId: string): [View, (next: View) => void] {
  const key = `neko:task-builder:view:${taskId}`;
  const read = (): View => {
    try {
      const stored = localStorage.getItem(key);
      if (stored === "chat" || stored === "result" || stored === "changes" || stored === "plan" || stored === "details") return stored;
    } catch { /* storage can be blocked */ }
    return "chat";
  };
  const [view, setViewState] = useState<View>(read);
  useEffect(() => setViewState(read()), [key]); // eslint-disable-line react-hooks/exhaustive-deps
  const setView = useCallback((next: View) => {
    setViewState(next);
    try { localStorage.setItem(key, next); } catch { /* ignore */ }
  }, [key]);
  return [view, setView];
}

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

/** One view inside the stage: the rounded card, scrolled, on the column the side panel uses. */
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
  const [view, setView] = useTaskViewMode(task.id);
  const [viewHeaderTarget, setViewHeaderTarget] = useState<HTMLDivElement | null>(null);

  const stage = stageOf(task.status);
  const repo = repositoryOf(snapshot, task);
  const prNumber = pullRequestNumber(task);

  const headerEnd = (
    <div className="flex items-center gap-2">
      <TaskActions task={task} snapshot={snapshot} actions={actions} ui={ui} repo={repo} className="flex-nowrap" />
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
        className="shrink-0"
      >
        <div className="flex min-h-[52px] min-w-0 items-stretch gap-3 pl-3">
          <nav aria-label="Breadcrumb" className={cn("flex min-w-0 max-w-80 shrink items-center gap-1.5 text-sm", BUILDER_HEADER_SLOT)}>
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
          </nav>
          <div className={cn("flex shrink-0 items-center gap-2 text-[12.5px] text-muted-foreground", BUILDER_HEADER_SLOT)}>
            <StatusLabel tone={STAGE_TONE[stage]} live={["Planning", "Building", "Reviewing"].includes(task.status)}>{friendlyStatus(task.status)}</StatusLabel>
          </div>
          <div ref={setViewHeaderTarget} className="min-w-0 flex-1 overflow-visible pr-3" />
        </div>
      </motion.header>

      <div className="flex min-h-0 flex-1 flex-col overflow-hidden px-3 pb-3">
        <BuilderViewTabs
          tabs={VIEW_TABS}
          value={view}
          onChange={setView}
          headerEnd={headerEnd}
          headerPortalTarget={viewHeaderTarget}
          renderContent={(value) =>
            value === "chat" ? (
              <div data-slot="task-chat" className="flex h-full min-h-0 min-w-0 flex-1 flex-col bg-card">
                <TaskConversation task={task} showResult={false} />
                <div className="mx-auto w-full max-w-(--chat-content-max-width) shrink-0 px-3 pt-1 pb-3">
                  <TaskComposer task={task} ui={ui} />
                </div>
              </div>
            ) : value === "result" ? (
                  <StageView>
                    <ResultPane task={task} state={state} />
                  </StageView>
                ) : value === "changes" ? (
                  <StageView>
                    {task.worktree || task.parked ? <Changes taskId={task.id} active /> : (
                      <p data-slot="changes-empty" className="mt-10 text-center text-[13px] text-muted-foreground">This task has no working copy yet, so nothing has changed.</p>
                    )}
                  </StageView>
                ) : value === "plan" ? (
                  <StageView>
                    {String(task.plan ?? "").trim() ? (
                      <PlanBlock title={state.showsPlan ? state.planTitle : "Plan"} text={task.plan} />
                    ) : (
                      <p className="mt-10 text-center text-[13px] text-muted-foreground">The agent has not written a plan for this task.</p>
                    )}
                  </StageView>
                ) : (
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
                )
              }
            />
          </div>
    </article>
  );
}
