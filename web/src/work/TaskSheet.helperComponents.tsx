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
import { Details } from "@/work/TaskDetail";
import type { Ui } from "@/work/taskActions";
import { friendlyStatus, pullRequestNumber, relative, repositoryOf, stageOf, type Task } from "@/work/model";

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
  tab: string;
  onTab: (tab: string) => void;
  onOpenTask: (id: string) => void;
  swapFade: string | undefined;
};

export function TaskSheetBody({ task, snapshot, ui, tab, onTab, onOpenTask, swapFade }: TaskSheetBodyProps) {
  const extraTabs = useTaskTabs(); // tabs a page added with host.addTab
  const repo = repositoryOf(snapshot, task);
  const prNumber = pullRequestNumber(task);
  return (
    <>
      <Tabs value={tab} onValueChange={(v: string) => onTab(v)} className="sky-tabs min-h-0 flex-1 gap-0">
        {/* Sections on the left; where the task stands on the right. */}
        <div className="flex shrink-0 items-center gap-3 px-6 pb-3">
          <TabsList aria-label="Task sections" className="shrink-0">
            <TabsTab value="conversation">Conversation</TabsTab>
            <TabsTab value="changes">Changes</TabsTab>
            <TabsTab value="details">Details</TabsTab>
            {extraTabs.map((t) => <TabsTab key={t.id} value={t.id}>{t.title}</TabsTab>)}
          </TabsList>
          <div className="ms-auto flex min-w-0 shrink items-center justify-end gap-x-3 overflow-hidden text-xs text-muted-foreground">
            <span className="shrink-0"><TaskStatusPill status={task.status} /></span>
            {repo ? <span className="hidden truncate @xl/sheet:inline">{repo}</span> : null}
            {prNumber ? (
              <a href={task.pull_request.url} target="_blank" rel="noreferrer" className="hidden items-center gap-1 rounded-sm tabular-nums outline-none hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring @2xl/sheet:inline-flex">
                <GitPullRequest size={13} aria-hidden /> {prNumber}
              </a>
            ) : null}
            <span className="hidden whitespace-nowrap tabular-nums @3xl/sheet:inline" title={`NEK-${String(task.id).slice(0, 4).toUpperCase()}`}>Updated {relative(task.updated_at_ms) || "—"}</span>
          </div>
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
      <div className="shrink-0 px-4 pt-1 pb-4">
        <TaskComposer task={task} ui={ui} />
      </div>
    </>
  );
}
