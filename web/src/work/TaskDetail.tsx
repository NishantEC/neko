// A task opened as a full page (TaskBuilder), and the pieces the full page and the
// side sheet (TaskSheet) share: the Details view and the task's actions.
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useWorkbench } from "@/useStore";
import { Check, CaretRight, DotsThree, GitPullRequest, X } from "@/ui/icons";
import { Button } from "@/ui/button";
import { Skeleton } from "@/ui/skeleton";
import {
  DropdownMenu, DropdownMenuCheckboxItem, DropdownMenuContent, DropdownMenuItem, DropdownMenuSeparator, DropdownMenuTrigger,
} from "@/ui/dropdown-menu";
import { Empty, EmptyDescription, EmptyHeader, EmptyTitle } from "@/ui/empty";
import { MonoLabel } from "@/editorial";
import { MessageResponse } from "@/chat/message";
import { cn } from "@/ui/lib/utils";
import { TaskBuilder } from "@/work/TaskBuilder";
import { Block, CommandRow } from "@/work/Thread";
import { setLandOnMain, landsOnMain, repositoryPath, taskActions, type TaskAction, type Ui } from "@/work/taskActions";
import {
  conversationState, decisionTitle, friendlyStatus, latestDecisions, receiptOf, type Receipt, type Task,
} from "@/work/model";

// The task's conversation and reply box are chat/TaskConversation.tsx.
function Fold({ label, children, defaultOpen = false }: { label: string; children: React.ReactNode; defaultOpen?: boolean }) {
  const [open, setOpen] = useState(defaultOpen);
  return (
    <div className="mt-2">
      <button type="button" onClick={() => setOpen(!open)} aria-expanded={open} className="flex items-center gap-1.5 rounded-sm py-1 text-[12.5px] text-muted-foreground outline-none hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring">
        <CaretRight size={11} weight="bold" className={cn("transition-transform", open && "rotate-90")} aria-hidden /> {label}
      </button>
      {open ? <div className="mt-1.5">{children}</div> : null}
    </div>
  );
}

const SUPPORT_KEYS = ["evidence", "files", "tests", "sensitive_areas", "uncertainties"] as const;
const EVENT_CAP = 150;

export function Details({ task, snapshot, open }: { task: Task; snapshot: any; open: (id: string) => void }) {
  const assessment = task.supervision;
  const splits = (snapshot?.splits ?? []).filter((s: any) => s.parent_id === task.id);
  const decisions = latestDecisions((snapshot?.decision_records ?? []).filter((r: any) => r.task_id === task.id));
  const events: any[] = task.events ?? [];
  const [all, setAll] = useState(false);
  const shown = all ? events : events.slice(-EVENT_CAP);
  return (
    <div className="grid min-w-0 grid-cols-[minmax(0,1fr)] gap-8 pb-10">
      {assessment ? (
        <section data-slot="risk">
          <MonoLabel className="text-[10.5px]">Risk · {String(assessment.risk ?? "").replace(/^./, (c: string) => c.toUpperCase())}</MonoLabel>
          <div className="mt-2 text-[13.5px] leading-[1.6]"><MessageResponse>{String(assessment.reason ?? "")}</MessageResponse></div>
          {SUPPORT_KEYS.filter((k) => (assessment[k] ?? []).length).map((k) => (
            <div key={k} className="mt-3">
              <MonoLabel className="text-[10.5px]">{k.replace("_", " ")}</MonoLabel>
              <ul className="mt-1 list-disc ps-5 text-[13px] leading-[1.6] text-muted-foreground">
                {(assessment[k] as string[]).map((item, i) => <li key={i}>{item}</li>)}
              </ul>
            </div>
          ))}
        </section>
      ) : null}
      {splits.map((split: any, si: number) => (
        <section key={si} data-slot="split">
          <MonoLabel className="text-[10.5px]">{split.approved ? "Parallel work" : "Parallel proposal · approval required"}</MonoLabel>
          {split.approved ? <p className="mt-1 text-[12.5px] text-muted-foreground">{split.integrated ? "Child changes integrated into this task's worktree." : "Child changes are not yet integrated."}</p> : null}
          <div className="mt-2 grid grid-cols-[minmax(0,1fr)] gap-2">
            {(split.subtasks ?? []).map((child: any, i: number) => {
              const sub = (snapshot?.tasks ?? []).find((t: any) => t.id === child.task_id);
              return (
                <Block key={i}>
                  <div className="font-medium">{i + 1}. {child.title}</div>
                  <p className="mt-1 text-[13px] text-muted-foreground">{child.goal}</p>
                  {(child.files ?? []).length ? <pre className="mt-2 whitespace-pre-wrap font-mono text-[11.5px] text-muted-foreground">{child.files.join("\n")}</pre> : null}
                  {(child.tests ?? []).length ? <p className="mt-1 text-[12px] text-muted-foreground">Checks: {child.tests.join(", ")}</p> : null}
                  {(child.depends_on ?? []).length ? <p className="mt-1 text-[12px] text-muted-foreground">After subtasks: {child.depends_on.map((n: number) => n + 1).join(", ")}</p> : null}
                  <div className="mt-2 flex items-center gap-2 text-[12.5px] text-muted-foreground">
                    {sub ? (
                      <>
                        {friendlyStatus(sub.status)}
                        <Button variant="outline" size="xs" onClick={() => open(sub.id)}>Open subtask</Button>
                      </>
                    ) : split.approved ? "Subtask not available in this snapshot" : "Awaiting approval"}
                  </div>
                </Block>
              );
            })}
          </div>
        </section>
      ))}
      {decisions.length ? (
        <section data-slot="decisions">
          <MonoLabel className="text-[10.5px]">Decisions</MonoLabel>
          <div className="mt-2 grid gap-2">
            {decisions.map((record: any) => (
              <Block key={record.id ?? record.episode_id}>
                <h4 className="font-medium text-strong">{decisionTitle(record.action)}</h4>
                {record.observed_outcome ? (
                  <p className="mt-1.5 text-[13px] leading-[1.55] text-muted-foreground">{record.observed_outcome}</p>
                ) : <p className="mt-1.5 text-[12.5px] text-muted-foreground">Outcome not yet observed</p>}
                {record.delivery_stage === "local_accepted" ? <p className="mt-1.5 flex items-center gap-1.5 text-[12.5px] text-success-foreground"><Check aria-hidden /> Accepted locally</p> : null}
                {record.correction?.text ? <p className="mt-2 text-[13px]"><span className="text-muted-foreground">Your correction: </span>{record.correction.text}</p> : null}
                {record.rationale ? <Fold label="Reasoning"><p className="whitespace-pre-wrap text-[13px] leading-[1.6] text-muted-foreground">{record.rationale}</p></Fold> : null}
              </Block>
            ))}
          </div>
        </section>
      ) : null}
      <section data-slot="where">
        <MonoLabel className="text-[10.5px]">Working copy</MonoLabel>
        <p className="mt-1.5 text-[13px] text-muted-foreground select-text">
          {task.parked
            ? `Saved on branch ${task.parked.branch}. Its folder comes back when the task runs again.`
            : task.worktree ? task.worktree : "No working copy yet."}
        </p>
      </section>
      <section data-slot="log">
        <MonoLabel className="text-[10.5px]">Activity log · {events.length}</MonoLabel>
        {events.length === 0 ? <p className="mt-1.5 text-[13px] text-muted-foreground">Nothing recorded yet.</p> : (
          <div className="mt-2 grid gap-2.5">
            {events.length > shown.length ? (
              <Button variant="ghost" size="xs" className="w-fit text-muted-foreground" onClick={() => setAll(true)}>Show {events.length - shown.length} earlier</Button>
            ) : null}
            {shown.map((event, i) => {
              const receipt: Receipt | null = receiptOf(event);
              return receipt ? <CommandRow key={i} receipt={receipt} /> : (
                <div key={i}>
                  <div className="text-[11.5px] capitalize text-muted-foreground">{event.role}</div>
                  <pre className="line-clamp-[12] whitespace-pre-wrap break-words font-mono text-[11.5px] leading-[1.55] text-foreground/80 select-text">{event.message}</pre>
                </div>
              );
            })}
          </div>
        )}
      </section>
    </div>
  );
}

export function ActionButton({ action, ui }: { action: TaskAction; ui: Ui }) {
  const variant = action.kind === "primary" ? "default" : "outline";
  if (action.href) {
    return (
      <Button size="sm" variant={variant} asChild>
        <a href={action.href} target="_blank" rel="noreferrer"><GitPullRequest /> {action.label}</a>
      </Button>
    );
  }
  return <Button size="sm" variant={variant} onClick={() => action.run?.(ui)}>{action.label}</Button>;
}

/** The task's buttons and its ⋯ menu: the header of the side panel and the builder frame share it. */
export function TaskActions({ task, snapshot, actions, ui, repo, className }: {
  task: Task;
  snapshot: any;
  actions: TaskAction[];
  ui: Ui;
  repo: string | null | undefined;
  className?: string;
}) {
  const buttons = actions.filter((a) => !a.menu);
  const menu = actions.filter((a) => a.menu);
  const repository = repositoryPath(snapshot, task);
  return (
    <div className={cn("flex flex-wrap items-center gap-2", className)} role="toolbar" aria-label="Task actions">
      {buttons.map((a) => <ActionButton key={a.id} action={a} ui={ui} />)}
      <DropdownMenu>
        <DropdownMenuTrigger render={<Button size="icon-sm" variant="ghost" aria-label="More task actions" />}><DotsThree weight="bold" /></DropdownMenuTrigger>
        <DropdownMenuContent align="start" className="min-w-56">
          {menu.filter((a) => a.kind !== "destructive").map((a) => (
            <DropdownMenuItem key={a.id} onClick={() => a.run?.(ui)}>{a.label}</DropdownMenuItem>
          ))}
          {repository ? (
            <DropdownMenuCheckboxItem checked={landsOnMain(snapshot, task)} onCheckedChange={(on) => void setLandOnMain(snapshot, task, on)}>
              Land on main in {repo}
            </DropdownMenuCheckboxItem>
          ) : null}
          {menu.some((a) => a.kind === "destructive") ? <DropdownMenuSeparator /> : null}
          {menu.filter((a) => a.kind === "destructive").map((a) => (
            <DropdownMenuItem key={a.id} variant="destructive" onClick={() => a.run?.(ui)}>{a.label}</DropdownMenuItem>
          ))}
        </DropdownMenuContent>
      </DropdownMenu>
    </div>
  );
}

export interface TaskDetailProps {
  id: string;
  ui: Ui;
  onClose: () => void;
  onToggleFull: () => void;
  onOpenTask: (id: string) => void;
}

/** A task opened as a full page: hushbacks' builder frame (TaskBuilder). Beside the list it is TaskSheet. */
export function TaskDetail({ id, ui, onClose, onToggleFull, onOpenTask }: TaskDetailProps) {
  const { snapshot } = useWorkbench();
  const task: Task | undefined = (snapshot?.tasks ?? []).find((t: Task) => t.id === id);
  const [copied, setCopied] = useState(false);

  const actions = useMemo(() => (task ? taskActions(task, snapshot) : []), [task, snapshot]);
  const copyLink = useCallback(() => {
    navigator.clipboard?.writeText(`${location.origin}${location.pathname}#/tasks?task=${id}`).catch(() => {});
    setCopied(true);
    setTimeout(() => setCopied(false), 1200);
  }, [id]);

  const heading = useRef<HTMLHeadingElement>(null);
  useEffect(() => heading.current?.focus({ preventScroll: true }), [id]);

  if (!snapshot) return <div className="p-6"><Skeleton className="h-7 w-2/3" /><Skeleton className="mt-4 h-40 w-full" /></div>;
  if (!task) {
    return (
      <div className="flex h-full flex-col">
        <div className="flex h-11 items-center px-3"><Button variant="ghost" size="icon-sm" aria-label="Close task" onClick={onClose}><X /></Button></div>
        <Empty className="border-0">
          <EmptyHeader>
            <EmptyTitle>This task is gone</EmptyTitle>
            <EmptyDescription>It was deleted, or the link is from another Neko.</EmptyDescription>
          </EmptyHeader>
        </Empty>
      </div>
    );
  }

  return (
    <TaskBuilder
      task={task}
      snapshot={snapshot}
      ui={ui}
      actions={actions}
      state={conversationState(task)}
      onClose={onClose}
      onToggleFull={onToggleFull}
      onOpenTask={onOpenTask}
      copied={copied}
      onCopyLink={copyLink}
      headingRef={heading}
    />
  );
}
