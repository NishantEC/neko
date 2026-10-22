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
import { GitDiff } from "@phosphor-icons/react";
import { ArrowLeft, ArrowsOutSimple, ChatCircle, Check, GitPullRequest, Info, Link as LinkIcon, ListChecks } from "@/ui/icons";
import { Button } from "@/ui/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/ui/tooltip";
import { ResizableHandle, ResizablePanel, ResizablePanelGroup, type ImperativePanelHandle } from "@/ui/resizable";
import { cn } from "@/ui/lib/utils";
import { hrefFor } from "@/router";
import { Changes } from "@/work/Changes";
import { PlanBlock } from "@/work/Thread";
import { STAGE_TONE, StatusLabel, Tag } from "@/work/tone";
import { conversationState, friendlyStatus, pullRequestNumber, repositoryOf, stageOf, taskTitle, type Task } from "@/work/model";
import type { TaskAction, Ui } from "@/work/taskActions";
import { Details, TaskActions } from "@/work/TaskDetail";
import { TaskComposer, TaskConversation } from "@/chat/TaskConversation";
import { BuilderPanelTabs, type BuilderPanelTab } from "@/work/builder/BuilderPanelTabs";
import { BuilderViewTabs, type BuilderViewTabItem } from "@/work/builder/BuilderViewTabs";
import { BUILDER_HEADER_SLOT } from "@/work/builder/builderHeaderLayout";

const EASE_OUT: [number, number, number, number] = [0.16, 1, 0.3, 1];

type PanelTab = "chat";
const PANEL_TABS: BuilderPanelTab<PanelTab>[] = [{ title: "Chat", icon: ChatCircle, value: "chat" }];

type View = "changes" | "plan" | "details";
const VIEW_TABS: BuilderViewTabItem<View>[] = [
  { title: "Changes", icon: GitDiff, value: "changes" },
  { type: "separator" },
  { title: "Plan", icon: ListChecks, value: "plan" },
  { title: "Details", icon: Info, value: "details" },
];

// hushbacks' hooks/useChatPanelSize + hooks/useCollapsedChat, on Neko's keys.
const SIZE_KEY = "neko:task-builder:chat-size";
const COLLAPSED_KEY = "neko:task-builder:chat-collapsed";
const DEFAULT_CHAT_PANEL_SIZE = 35;
const NARROW_QUERY = "(max-width: 899px)";

function readSize(): number {
  try {
    const stored = Number.parseFloat(localStorage.getItem(SIZE_KEY) ?? "");
    return Number.isFinite(stored) && stored > 0 ? stored : DEFAULT_CHAT_PANEL_SIZE;
  } catch {
    return DEFAULT_CHAT_PANEL_SIZE;
  }
}

function readCollapsed(): boolean {
  if (typeof matchMedia === "function" && matchMedia(NARROW_QUERY).matches) return true;
  try {
    return localStorage.getItem(COLLAPSED_KEY) === "1";
  } catch {
    return false;
  }
}

// hushbacks' useBuilderViewMode: the view is remembered per task.
function useTaskViewMode(taskId: string): [View, (next: View) => void] {
  const key = `neko:task-builder:view:${taskId}`;
  const read = (): View => {
    try {
      const stored = localStorage.getItem(key);
      if (stored === "changes" || stored === "plan" || stored === "details") return stored;
    } catch { /* storage can be blocked */ }
    return "changes";
  };
  const [view, setViewState] = useState<View>(read);
  useEffect(() => setViewState(read()), [key]); // eslint-disable-line react-hooks/exhaustive-deps
  const setView = useCallback((next: View) => {
    setViewState(next);
    try { localStorage.setItem(key, next); } catch { /* ignore */ }
  }, [key]);
  return [view, setView];
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
  const [chatCollapsed, setChatCollapsedState] = useState(readCollapsed);
  const [chatPanelSize, setChatPanelSizeState] = useState(readSize);
  const [panelTab, setPanelTab] = useState<PanelTab>("chat");
  const [viewHeaderTarget, setViewHeaderTarget] = useState<HTMLDivElement | null>(null);
  const [chatPanel, setChatPanel] = useState<ImperativePanelHandle | null>(null);
  const [view, setView] = useTaskViewMode(task.id);

  const setChatCollapsed = (next: boolean) => {
    setChatCollapsedState(next);
    try { localStorage.setItem(COLLAPSED_KEY, next ? "1" : "0"); } catch { /* ignore */ }
  };

  const handlePanelTabChange = (next: PanelTab) => {
    setPanelTab(next);
    if (chatPanel?.isCollapsed()) {
      chatPanel.expand();
      chatPanel.resize(chatPanelSize);
    }
  };

  const handleChatPanelResize = (size: number) => {
    const collapsed = size === 0;
    if (collapsed !== chatCollapsed) setChatCollapsed(collapsed);
    if (size > 0) {
      setChatPanelSizeState(size);
      try { localStorage.setItem(SIZE_KEY, String(size)); } catch { /* ignore */ }
    }
  };

  const stage = stageOf(task.status);
  const repo = repositoryOf(snapshot, task);
  const prNumber = pullRequestNumber(task);
  const gridTemplateColumns = chatCollapsed ? "auto minmax(0, 1fr)" : `minmax(22.5rem, ${chatPanelSize}%) minmax(0, 1fr)`;

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
        <div className="grid min-h-[52px]" style={{ gridTemplateColumns }}>
          <div className={cn("flex min-w-0 items-stretch gap-3 pl-3", !chatCollapsed && "@container/chathead")}>
            <nav
              aria-label="Breadcrumb"
              className={cn("flex min-w-0 flex-1 items-center gap-1.5 text-sm", chatCollapsed ? "max-w-48 min-[900px]:max-w-80" : "max-w-none", BUILDER_HEADER_SLOT)}
            >
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

            <div className={cn("flex shrink-0 items-center gap-2 text-[12.5px] text-muted-foreground @max-[30rem]/chathead:hidden", BUILDER_HEADER_SLOT)}>
              <StatusLabel tone={STAGE_TONE[stage]} live={["Planning", "Building", "Reviewing"].includes(task.status)}>{friendlyStatus(task.status)}</StatusLabel>
            </div>

            <BuilderPanelTabs
              tabs={PANEL_TABS}
              value={panelTab}
              collapsed={chatCollapsed}
              onChange={handlePanelTabChange}
              onClose={() => chatPanel?.collapse()}
              className="ml-auto self-end pr-7"
            />
          </div>
          <div ref={setViewHeaderTarget} className="min-w-0 overflow-visible pl-1.5 pr-3" />
        </div>
      </motion.header>

      <ResizablePanelGroup direction="horizontal" className={cn("min-h-0 flex-1", !chatCollapsed && "[&>[data-panel-id=builder-chat-panel]]:min-w-90!")}>
        <ResizablePanel
          id="builder-chat-panel"
          order={1}
          ref={setChatPanel}
          defaultSize={chatCollapsed ? 0 : chatPanelSize}
          minSize={25}
          maxSize={45}
          collapsible
          collapsedSize={0}
          onResize={handleChatPanelResize}
        >
          {chatCollapsed ? null : (
            <div className="flex h-full min-h-0 flex-col pb-3 pl-3 pr-0.5">
              <div
                id="builder-panel-surface"
                role="tabpanel"
                aria-labelledby={`builder-panel-tab-${panelTab}`}
                className="flex min-h-0 flex-1 flex-col overflow-hidden rounded-t-xl rounded-b-4xl bg-[var(--builder-panel)] p-1"
              >
                <div className="flex h-full min-h-0 min-w-0 flex-1 flex-col overflow-hidden rounded-t-lg rounded-b-[28px] bg-card">
                  <TaskConversation task={task} />
                  <div className="w-full shrink-0 px-3 pt-1 pb-3">
                    <TaskComposer task={task} ui={ui} />
                  </div>
                </div>
              </div>
            </div>
          )}
        </ResizablePanel>

        <ResizableHandle className="opacity-0" aria-label="Resize the chat" />

        <ResizablePanel id="builder-stage-panel" order={2} defaultSize={chatCollapsed ? 100 : 100 - chatPanelSize} minSize={55} className={cn("builder-stage-panel", chatCollapsed && "pl-3")}>
          <div className="flex h-full flex-1 flex-col overflow-hidden pb-3 pl-[3px] pr-3">
            <BuilderViewTabs
              tabs={VIEW_TABS}
              value={view}
              onChange={setView}
              headerEnd={headerEnd}
              headerPortalTarget={viewHeaderTarget}
              renderContent={(value) =>
                value === "changes" ? (
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
        </ResizablePanel>
      </ResizablePanelGroup>
    </article>
  );
}
