// from hushbacks apps/spa/src/pages/Builder/components/ResponseDetailPanel.helperComponents.tsx @5d9a64b11
// (ResponseDetailBody). The response's heading, meta row and Answers / AI summary
// tabs became the task's: its title, a status pill with where it lives, its
// actions, then Conversation / Changes / Details. Colours stay Neko's tokens.
import type { CSSProperties } from "react";
import { GitPullRequest } from "@/ui/icons";
import { Tabs, TabsList, TabsPanel, TabsTab } from "@/ui/tabs";
import { useTaskTabs } from "@/extensions";
import { cn } from "@/ui/lib/utils";
import { TaskComposer, TaskConversation } from "@/chat/TaskConversation";
import { Changes } from "@/work/Changes";
import { Details, TaskActions } from "@/work/TaskDetail";
import type { TaskAction, Ui } from "@/work/taskActions";
import { friendlyStatus, pullRequestNumber, relative, repositoryOf, stageOf, taskTitle, type Task } from "@/work/model";

// The sky status pill's states (`on`, `warn`, `error`, `off`, `pending`) plus a tint
// for the two stages hushbacks has no state for.
function statusPill(status: string): { state?: string; style?: CSSProperties } {
  if (status === "Failed") return { state: "error" };
  if (status === "Cancelled") return { state: "off" };
  switch (stageOf(status)) {
    case "needsYou": return { state: "warn" };
    case "review": return { style: { "--tint": "var(--info)" } as CSSProperties };
    case "running": return { style: { "--tint": "var(--primary)" } as CSSProperties };
    case "done": return { state: "on" };
    default: return { state: "pending" };
  }
}

export function TaskStatusPill({ status }: { status: string }) {
  const { state, style } = statusPill(status);
  return <span data-slot="task-status" className="sky-status" data-state={state} style={style}>{friendlyStatus(status)}</span>;
}

type TaskSheetBodyProps = {
  task: Task;
  snapshot: any;
  ui: Ui;
  actions: TaskAction[];
  tab: string;
  onTab: (tab: string) => void;
  onOpenTask: (id: string) => void;
  swapFade: string | undefined;
};

export function TaskSheetBody({ task, snapshot, ui, actions, tab, onTab, onOpenTask, swapFade }: TaskSheetBodyProps) {
  const extraTabs = useTaskTabs(); // tabs a page added with host.addTab
  const repo = repositoryOf(snapshot, task);
  const prNumber = pullRequestNumber(task);
  return (
    <>
      <div className="shrink-0 space-y-2 px-6 pt-2 pb-5">
        <h3 key={task.id} data-slot="task-title" className={cn("break-words text-lg font-semibold tracking-tight", swapFade)}>
          {taskTitle(task)}
        </h3>
        <div className="flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-muted-foreground">
          <TaskStatusPill status={task.status} />
          {repo ? <span>{repo}</span> : null}
          {prNumber ? (
            <a href={task.pull_request.url} target="_blank" rel="noreferrer" className="inline-flex items-center gap-1 rounded-sm tabular-nums outline-none hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring">
              <GitPullRequest size={13} aria-hidden /> {prNumber}
            </a>
          ) : null}
          <span className="tabular-nums">Updated {relative(task.updated_at_ms) || "—"}</span>
          <span className="tabular-nums">NEK-{String(task.id).slice(0, 4).toUpperCase()}</span>
        </div>
        <TaskActions task={task} snapshot={snapshot} actions={actions} ui={ui} repo={repo} className="pt-2" />
      </div>
      <Tabs value={tab} onValueChange={(v: string) => onTab(v)} className="sky-tabs min-h-0 flex-1 gap-0">
        <div className="shrink-0 px-6 pb-2">
          <TabsList aria-label="Task sections">
            <TabsTab value="conversation">Conversation</TabsTab>
            <TabsTab value="changes">Changes</TabsTab>
            <TabsTab value="details">Details</TabsTab>
            {extraTabs.map((t) => <TabsTab key={t.id} value={t.id}>{t.title}</TabsTab>)}
          </TabsList>
        </div>
        <TabsPanel value="conversation" className="flex min-h-0 flex-col">
          <div key={task.id} className={cn("flex min-h-0 flex-1 flex-col", swapFade)}>
            <TaskConversation task={task} />
          </div>
        </TabsPanel>
        <TabsPanel value="changes" className="min-h-0 overflow-y-auto">
          <div key={task.id} className={cn("px-6 pb-6", swapFade)}>
            {task.worktree || task.parked ? <Changes taskId={task.id} active={tab === "changes"} /> : (
              <p data-slot="changes-empty" className="py-6 text-sm text-muted-foreground">This task has no working copy yet, so nothing has changed.</p>
            )}
          </div>
        </TabsPanel>
        <TabsPanel value="details" className="min-h-0 overflow-y-auto">
          <div key={task.id} className={cn("min-w-0 px-6 pt-4", swapFade)}>
            <Details task={task} snapshot={snapshot} open={onOpenTask} />
          </div>
        </TabsPanel>
        {extraTabs.map((t) => (
          <TabsPanel key={t.id} value={t.id} className="min-h-0 overflow-y-auto">
            <div key={task.id} className={cn("min-w-0 px-6 pt-4", swapFade)}><t.component task={task} /></div>
          </TabsPanel>
        ))}
      </Tabs>
      <div className="shrink-0 px-4 pt-1">
        <TaskComposer task={task} ui={ui} />
      </div>
    </>
  );
}
