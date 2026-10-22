// What you can do to a task, decided once. The detail page, the board's context
// menu and ⌘K all read this list, so they can never disagree about which button
// a task in a given state gets. Mirrors TicketDetail.swift (`agentToolbar`) and
// TicketsView.swift (`moveMenu`).
import { toast } from "@/ui/sonner";
import { run, type ConfirmSpec, type PromptSpec } from "@/work/actions";
import {
  canProposeSplit, isFinished, isRunning, moveCommand, openPullRequest, STAGE_TITLE, taskTitle, waitingReason,
  type Snapshot, type Stage, type Task,
} from "@/work/model";

export interface Ui {
  confirm(spec: ConfirmSpec): void;
  prompt(spec: PromptSpec): void;
}

export type ActionKind = "primary" | "secondary" | "destructive";

export interface TaskAction {
  id: string;
  label: string;
  /** Short sentence for ⌘K: "Approve selected task". */
  palette?: string;
  kind: ActionKind;
  /** A pull request link: opens instead of running a command. */
  href?: string;
  /** Shown in the ⋯ menu rather than as a button. */
  menu?: boolean;
  run?: (ui: Ui) => void;
}

const send = (name: string, task: Task, done?: string, extra: Record<string, unknown> = {}) =>
  run(name, { task_id: task.id, ...extra }, done);

export function landsOnMain(snapshot: Snapshot, task: Task): boolean {
  const repository: string = snapshot?.task_roots?.[task.id] ?? "";
  if (!repository) return false;
  return (snapshot?.land_on_main ?? []).some((p: string) => p === repository);
}

export function repositoryPath(snapshot: Snapshot, task: Task): string {
  return snapshot?.task_roots?.[task.id] ?? "";
}

export function taskActions(task: Task, snapshot: Snapshot): TaskAction[] {
  const title = taskTitle(task);
  const out: TaskAction[] = [];
  const status: string = task.status;
  const question = waitingReason(task);

  if (status === "AwaitingApproval") {
    if (question === null) {
      out.push({ id: "approve", label: "Approve plan", palette: "Approve selected task", kind: "primary", run: () => void send("ApproveTask", task, `Approved “${title}”.`) });
    }
    if (canProposeSplit(task, snapshot?.splits ?? [])) {
      out.push({ id: "split", label: "Propose parallel work", kind: "secondary", menu: question === null, run: () => void send("ProposeSplit", task) });
    }
  }
  if (status === "ReadyForReview") {
    const pr = openPullRequest(task);
    if (pr) out.push({ id: "pr", label: "Open pull request", palette: "Open the selected task’s pull request", kind: "primary", href: pr });
    out.push({
      id: "done", label: "Mark done", palette: "Mark selected task done", kind: pr ? "secondary" : "primary", menu: !!pr,
      run: () => void send("CompleteTask", task, `Marked “${title}” done.`),
    });
    if (!pr) out.push({ id: "publish", label: "Publish pull request", kind: "secondary", menu: true, run: () => void send("PublishTask", task, "Opening a pull request.") });
  }
  if (status === "Failed" || status === "Cancelled") {
    out.push({ id: "start", label: "Start again", palette: "Start selected task again", kind: "secondary", run: () => void send("StartTask", task, `Started “${title}”.`) });
    out.push({ id: "retry", label: "Retry from the beginning", kind: "secondary", menu: true, run: () => void send("RetryTask", task, `Retrying “${title}”.`) });
  }
  if (status === "Queued") {
    out.push({ id: "next", label: "Run next", palette: "Run selected task next", kind: "secondary", menu: true, run: () => void send("PrioritizeTask", task, "Moved to the front of the queue.") });
  }
  if (isRunning(task)) {
    out.push({
      id: "stop", label: "Stop", palette: "Stop selected task", kind: "secondary",
      run: (ui) => ui.confirm({
        title: `Stop “${title}”?`,
        description: "The agent stops where it is. Its files and worktree stay on disk, and you can start it again.",
        confirm: "Stop it", cancel: "Keep working", destructive: true,
        onConfirm: () => send("CancelTask", task, `Stopped “${title}”.`),
      }),
    });
  } else if (status === "AwaitingApproval" || status === "ReadyForReview") {
    out.push({
      id: "cancel", label: "Cancel task", palette: "Cancel selected task", kind: "destructive", menu: true,
      run: (ui) => ui.confirm({
        title: `Cancel “${title}”?`,
        description: "It hasn’t been reviewed yet, so this closes it. Its files and worktree stay on disk.",
        confirm: "Cancel task", cancel: "Keep it", destructive: true,
        onConfirm: () => send("CancelTask", task, `Cancelled “${title}”.`),
      }),
    });
  }
  out.push({
    id: "rename", label: "Rename…", palette: "Rename selected task", kind: "secondary", menu: true,
    run: (ui) => ui.prompt({
      title: "Rename task", label: "Name", initial: String(task.display_title ?? "") || title,
      description: "An empty name lets Neko name it again.", confirm: "Rename", allowEmpty: true,
      onSubmit: (text) => send("RenameTask", task, text ? "Renamed." : "Neko will name it again.", { title: text }),
    }),
  });
  if (isFinished(task)) {
    out.push({
      id: "delete", label: "Delete…", palette: "Delete selected task", kind: "destructive", menu: true,
      run: (ui) => ui.confirm({
        title: "Delete this task?",
        description: "Removes the task, its subtasks and their history from Neko. Your files and the task’s worktree on disk stay as they are.",
        confirm: "Delete task", destructive: true,
        onConfirm: () => send("DeleteTask", task, `Deleted “${title}”.`),
      }),
    });
  }
  return out;
}

/** Moves a task to a board column the way a drop or a menu choice asks: the daemon's own command, or a reason why not. */
export function moveTask(task: Task, column: Stage, ui: Ui) {
  const plan = moveCommand(task.status, column);
  if (plan.reason) {
    toast.info(plan.reason);
    return;
  }
  if (!plan.command) return;
  const title = taskTitle(task);
  if (plan.command === "CancelTask") {
    ui.confirm({
      title: `Cancel “${title}”?`,
      description: "It hasn’t been reviewed yet, so moving it to Done stops the work. Its files and worktree stay on disk.",
      confirm: "Cancel task", cancel: "Keep working", destructive: true,
      onConfirm: () => send("CancelTask", task, `Cancelled “${title}”. Its files and worktree stay on disk.`),
    });
    return;
  }
  void send(plan.command, task, plan.command === "StartTask" ? `Started “${title}”. Neko finds its repository, plans, then builds.` : `Moved “${title}” to ${STAGE_TITLE[column]}.`);
}

/** Toggles delivery for the task's repository: land on local main, or open a pull request. */
export function setLandOnMain(snapshot: Snapshot, task: Task, enabled: boolean) {
  const repository = repositoryPath(snapshot, task);
  if (!repository) {
    toast.error("This task has no repository yet.");
    return Promise.resolve(false);
  }
  return run("SetLandOnMain", { repository, enabled }, enabled ? "Reviewed tasks here now land on local main." : "Reviewed tasks here now open a pull request.");
}
