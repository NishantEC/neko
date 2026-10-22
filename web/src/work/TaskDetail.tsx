// A task opened: its conversation, the files it changed, the supporting detail,
// the actions that apply to its state and a reply box (TicketPeek, TicketDetail
// and TicketComposer in the SwiftUI app). It renders beside the list (a
// resizable panel) or as a full page; the page decides.
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useWorkbench } from "@/useStore";
import {
  ArrowLeft, ArrowsOutSimple, Check, CaretRight, DotsThree, GitPullRequest, Link as LinkIcon, X,
} from "@/ui/icons";
import { Button } from "@/ui/button";
import { Skeleton } from "@/ui/skeleton";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/ui/tooltip";
import {
  DropdownMenu, DropdownMenuCheckboxItem, DropdownMenuContent, DropdownMenuItem, DropdownMenuSeparator, DropdownMenuTrigger,
} from "@/ui/dropdown-menu";
import { Tabs, TabsList, TabsPanel, TabsTab } from "@/ui/tabs";
import { Empty, EmptyDescription, EmptyHeader, EmptyTitle } from "@/ui/empty";
import { Conversation, ConversationContent, ConversationScrollButton } from "@/chat/conversation";
import { ChatInput } from "@/chat/chat-input";
import { EditorialHeading, MonoLabel } from "@/editorial";
import { MessageResponse } from "@/chat/message";
import { CaretDown } from "@/ui/icons";
import { cn } from "@/ui/lib/utils";
import { Changes } from "@/work/Changes";
import { TaskBuilder } from "@/work/TaskBuilder";
import { Block, CommandRow, DeliveryCard, PlanBlock, ResultBlock, ThreadView } from "@/work/Thread";
import { run } from "@/work/actions";
import { setLandOnMain, landsOnMain, repositoryPath, taskActions, type TaskAction, type Ui } from "@/work/taskActions";
import { STAGE_TONE, StatusLabel, Tag } from "@/work/tone";
import {
  conversationState, decisionTitle, friendlyStatus, latestDecisions, pullRequestNumber, receiptOf, relative, repositoryOf,
  stageOf, stopReason, taskTitle, threadRows, waitingReason, type Receipt, type Task,
} from "@/work/model";

const PLACEHOLDER: Record<string, string> = {
  AwaitingApproval: "Answer the agent, or tell it what to change…",
  ReadyForReview: "Ask a question or request changes…",
  Failed: "Tell the agent how to try again…",
  Cancelled: "Tell the agent how to try again…",
  Completed: "Reopen with a follow-up…",
};

// Drafts survive switching tasks (not a reload), like the native app's.
const drafts = new Map<string, string>();

export function Composer({ task, ui }: { task: Task; ui: Ui }) {
  const id: string = task.id;
  const [text, setText] = useState(() => drafts.get(id) ?? "");
  const [sending, setSending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => { setText(drafts.get(id) ?? ""); setError(null); }, [id]);
  const change = (value: string) => { setText(value); drafts.set(id, value); };
  const running = ["Queued", "Planning", "Building", "Reviewing"].includes(task.status);
  const stop = taskActions(task, {}).find((a) => a.id === "stop");

  const send = async () => {
    const trimmed = text.trim();
    if (!trimmed || sending) return;
    setSending(true);
    setError(null);
    const ok = await run("ReplyToTask", { task_id: id, text: trimmed });
    if (ok) { change(""); } else setError("The reply was not sent. Your text is still here.");
    setSending(false);
  };

  const reason = waitingReason(task);
  const tray = reason ? (
    <TrayNote title="The planner has a question" body={reason} />
  ) : task.status === "Failed" ? (
    <TrayNote title="Why it stopped" body={stopReason(task)} />
  ) : null;

  return (
    <ChatInput
      text={text}
      onTextChange={change}
      onSendMessage={send}
      sending={sending}
      isGenerating={running}
      onStop={() => stop?.run?.(ui)}
      error={error}
      maxLength={2048}
      placeholder={PLACEHOLDER[task.status] ?? "Redirect the agent. It pauses and replans…"}
      tray={tray}
      onKeyDown={(e) => {
        if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) void send();
      }}
    />
  );
}

function TrayNote({ title, body }: { title: string; body: string }) {
  const [open, setOpen] = useState(false);
  const first = body.split("\n")[0];
  return (
    <div data-slot="task-tray" role="note" className="mb-2 rounded-2xl border border-warning/40 bg-warning/[0.07] px-3.5 py-2.5">
      <div className="flex items-center gap-2 text-warning-foreground">
        <MonoLabel className="text-warning-foreground">{title}</MonoLabel>
        <span className="flex-1" />
        {body.length > first.length || body.length > 140 ? (
          <button type="button" onClick={() => setOpen(!open)} aria-expanded={open} className="rounded-sm text-xs outline-none hover:underline focus-visible:ring-2 focus-visible:ring-ring">
            {open ? "Less" : "Full text"}
          </button>
        ) : null}
      </div>
      <p className={cn("mt-1 text-[13px] leading-[1.5] text-foreground", !open && "line-clamp-2")}>{open ? body : first}</p>
    </div>
  );
}

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

/** The conversation: the thread, the plan and result blocks, delivery. Mounted by the side panel and by the builder frame's chat side. */
export function TaskThread({ task, rows, state, hasPlanBlock }: {
  task: Task;
  rows: ReturnType<typeof threadRows>;
  state: ReturnType<typeof conversationState>;
  hasPlanBlock: boolean;
}) {
  return (
    <Conversation className="min-h-0 flex-1">
      <ConversationContent className="mx-auto w-full max-w-[728px] gap-0 px-5 pt-1 pb-6">
        <ThreadView rows={rows} status={task.status} />
        {hasPlanBlock ? <PlanBlock title={state.showsPlan ? state.planTitle : "Plan"} text={task.plan} /> : null}
        {String(task.result ?? "") !== "" ? <ResultBlock task={task} outcome={state.outcome} reviewIsCurrent={state.reviewIsCurrent} /> : null}
        <DeliveryCard task={task} />
        {rows.length === 0 && !hasPlanBlock && !task.result ? (
          <p className="mt-10 text-center text-[13px] text-muted-foreground">Nothing has happened on this task yet.</p>
        ) : null}
      </ConversationContent>
      <ConversationScrollButton size="sm" className="gap-1.5 rounded-full bg-popover pe-3 ps-2 shadow-md"><CaretDown /> Scroll to end</ConversationScrollButton>
    </Conversation>
  );
}

export interface TaskDetailProps {
  id: string;
  ui: Ui;
  full: boolean;
  onClose: () => void;
  onToggleFull: () => void;
  onOpenTask: (id: string) => void;
}

export function TaskDetail({ id, ui, full, onClose, onToggleFull, onOpenTask }: TaskDetailProps) {
  const { snapshot } = useWorkbench();
  const task: Task | undefined = (snapshot?.tasks ?? []).find((t: Task) => t.id === id);
  const [tab, setTab] = useState("conversation");
  const [copied, setCopied] = useState(false);
  useEffect(() => setTab("conversation"), [id]);

  const rows = useMemo(() => (task ? threadRows(task) : []), [task]);
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

  const stage = stageOf(task.status);
  const state = conversationState(task);
  const repo = repositoryOf(snapshot, task);
  const prNumber = pullRequestNumber(task);
  const hasPlanBlock = String(task.plan ?? "").trim() !== "" && (state.showsPlan || state.outcome === "previous");

  if (full) {
    return (
      <TaskBuilder
        task={task}
        snapshot={snapshot}
        ui={ui}
        actions={actions}
        rows={rows}
        state={state}
        hasPlanBlock={hasPlanBlock}
        onClose={onClose}
        onToggleFull={onToggleFull}
        onOpenTask={onOpenTask}
        copied={copied}
        onCopyLink={copyLink}
        headingRef={heading}
      />
    );
  }

  return (
    <article data-slot="task-detail" data-task-id={id} aria-label={taskTitle(task)} className="flex h-full min-h-0 flex-col bg-background">
      <header className="shrink-0 border-b border-border px-6 pt-4 pb-4">
        <div className="mx-auto w-full max-w-[728px]">
        <div className="flex items-center gap-1">
          {full ? (
            <Button variant="ghost" size="sm" onClick={onClose} className="-ms-2 text-muted-foreground"><ArrowLeft /> Tasks</Button>
          ) : (
            <Button variant="ghost" size="icon-sm" aria-label="Close task" title="Close (Esc)" onClick={onClose} className="-ms-2"><X /></Button>
          )}
          <span className="flex-1" />
          <Tooltip>
            <TooltipTrigger asChild>
              <Button variant="ghost" size="icon-sm" aria-label="Copy a link to this task" onClick={copyLink}>{copied ? <Check /> : <LinkIcon />}</Button>
            </TooltipTrigger>
            <TooltipContent>{copied ? "Copied" : "Copy link"}</TooltipContent>
          </Tooltip>
          <Tooltip>
            <TooltipTrigger asChild>
              <Button variant="ghost" size="icon-sm" aria-label={full ? "Show beside the list" : "Open as a full page"} onClick={onToggleFull}><ArrowsOutSimple /></Button>
            </TooltipTrigger>
            <TooltipContent>{full ? "Show beside the list" : "Full page"}</TooltipContent>
          </Tooltip>
        </div>
        <EditorialHeading as="h2" className="mt-1 text-[22px] leading-[1.2]">
          <span ref={heading} tabIndex={-1} className="outline-none">{taskTitle(task)}</span>
        </EditorialHeading>
        <div className="mt-2 flex flex-wrap items-center gap-2 text-[12.5px] text-muted-foreground">
          <StatusLabel tone={STAGE_TONE[stage]} live={["Planning", "Building", "Reviewing"].includes(task.status)}>{friendlyStatus(task.status)}</StatusLabel>
          {repo ? <Tag>{repo}</Tag> : null}
          {prNumber ? (
            <a href={task.pull_request.url} target="_blank" rel="noreferrer" className="inline-flex items-center gap-1 rounded-md hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring">
              <GitPullRequest size={13} aria-hidden /> {prNumber}
            </a>
          ) : null}
          <span>Updated {relative(task.updated_at_ms) || "—"}</span>
          <span className="font-mono text-[11px] text-muted-foreground/70">NEK-{String(id).slice(0, 4).toUpperCase()}</span>
        </div>
        <TaskActions task={task} snapshot={snapshot} actions={actions} ui={ui} repo={repo} className="mt-4" />
        </div>
      </header>

      <Tabs value={tab} onValueChange={(v: string) => setTab(v)} className="min-h-0 flex-1 gap-0">
        <div className="shrink-0 border-b border-border px-3">
          <div className="mx-auto w-full max-w-[728px]">
            <TabsList variant="underline" aria-label="Task sections">
              <TabsTab value="conversation">Conversation</TabsTab>
              <TabsTab value="changes">Changes</TabsTab>
              <TabsTab value="details">Details</TabsTab>
            </TabsList>
          </div>
        </div>
        <TabsPanel value="conversation" className="flex min-h-0 flex-col">
          <TaskThread task={task} rows={rows} state={state} hasPlanBlock={hasPlanBlock} />
        </TabsPanel>
        <TabsPanel value="changes" className="min-h-0 overflow-y-auto">
          <div className="mx-auto w-full max-w-[728px] px-5 pb-10">
            {task.worktree || task.parked ? <Changes taskId={id} active={tab === "changes"} /> : (
              <p data-slot="changes-empty" className="mt-10 text-center text-[13px] text-muted-foreground">This task has no working copy yet, so nothing has changed.</p>
            )}
          </div>
        </TabsPanel>
        <TabsPanel value="details" className="min-h-0 overflow-y-auto">
          <div className="mx-auto w-full min-w-0 max-w-[728px] px-5 pt-5">
            <Details task={task} snapshot={snapshot} open={onOpenTask} />
          </div>
        </TabsPanel>
      </Tabs>

      <div className="mx-auto w-full max-w-[728px] shrink-0 px-5 pt-1 pb-4">
        <Composer task={task} ui={ui} />
      </div>
    </article>
  );
}
