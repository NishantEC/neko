// The pure half of the work pages: what a status means, how a task reads
// anywhere it appears, how a ticket's events become a conversation, how pull
// requests are judged and how the agent tree is derived. Every rule here is a
// port of the Swift one (native/NekoKit/Sources/NekoNative): TaskStage.swift,
// TicketPresentation.swift, TicketThread.swift, TicketCommandEvidence.swift,
// AgentTree.swift and PullRequestsView.swift. Nothing in this file touches the
// DOM or the daemon, so it can be read (and checked) on its own.
import type { Json } from "@/api";

export type Task = Json;
export type Snapshot = Json;

const arr = (value: unknown): Json[] => (Array.isArray(value) ? value : []);
const str = (value: unknown): string => (typeof value === "string" ? value : "");
const num = (value: unknown): number => (typeof value === "number" && Number.isFinite(value) ? value : 0);

// ---------------------------------------------------------------------------
// Stages (TaskStage.swift): the one mapping every count, column and badge uses
// ---------------------------------------------------------------------------

export type Stage = "needsYou" | "review" | "running" | "upNext" | "done";
export const STAGES: Stage[] = ["needsYou", "review", "running", "upNext", "done"];

export function stageOf(status: string): Stage {
  switch (status) {
    case "AwaitingApproval":
    case "Failed":
      return "needsYou";
    case "ReadyForReview":
      return "review";
    case "Planning":
    case "Building":
    case "Reviewing":
      return "running";
    case "Queued":
      return "upNext";
    default:
      return "done";
  }
}

export const STAGE_TITLE: Record<Stage, string> = {
  needsYou: "Needs you",
  review: "Ready to review",
  running: "Running",
  upNext: "Up next",
  done: "Done",
};

export const STAGE_EMPTY: Record<Stage, string> = {
  needsYou: "Nothing is waiting on you.",
  review: "Nothing to review yet.",
  running: "No agent is working right now.",
  upNext: "Nothing is queued.",
  done: "Nothing finished yet.",
};

/** The statuses each board column holds (TicketsView.columns). */
export const COLUMNS: { id: Stage; statuses: string[] }[] = [
  { id: "needsYou", statuses: ["AwaitingApproval", "Failed"] },
  { id: "review", statuses: ["ReadyForReview"] },
  { id: "running", statuses: ["Planning", "Building", "Reviewing"] },
  { id: "upNext", statuses: ["Queued"] },
  { id: "done", statuses: ["Completed", "Cancelled"] },
];

export function friendlyStatus(status: string): string {
  switch (status) {
    case "AwaitingApproval": return "Needs approval";
    case "ReadyForReview": return "Ready to review";
    case "Queued": return "Queued";
    case "Planning": return "Making a plan";
    case "Building": return "Preparing a fix";
    case "Reviewing": return "Checking the result";
    case "Completed": return "Completed";
    case "Cancelled": return "Cancelled";
    case "Failed": return "Stopped";
    default: return status;
  }
}

export const isRunning = (task: Task) => ["Queued", "Planning", "Building", "Reviewing"].includes(task.status);
export const isFinished = (task: Task) => ["Completed", "Cancelled", "Failed"].includes(task.status);

// ---------------------------------------------------------------------------
// How a task reads (TaskText)
// ---------------------------------------------------------------------------

export function taskTitle(task: Task): string {
  const display = str(task.display_title).trim();
  return display || str(task.title);
}

export const basename = (path: string) => path.replace(/\/+$/, "").split("/").pop() ?? path;

export function repositoryOf(snapshot: Snapshot, task: Task): string | null {
  const root = str(snapshot?.task_roots?.[task.id]);
  return root ? basename(root) : null;
}

export function sourceName(label: string): string {
  const lower = label.toLowerCase();
  for (const known of ["sentry", "linear", "github", "jira", "slack", "notion", "figma", "paper"]) {
    if (lower.includes(known)) return known[0].toUpperCase() + known.slice(1);
  }
  return label;
}

/** "Sentry · CONNECT-PROD-P5E" or "Linear · HEA-1427" when a watch filed it. */
export function sourceOf(snapshot: Snapshot, task: Task): string | null {
  const observed = arr(snapshot?.mcp?.sources).find((s) => s.task_id === task.id);
  if (!observed) return null;
  const label = str(arr(snapshot?.mcp?.connections).find((c) => c.id === observed.connection_id)?.label);
  const name = sourceName(label);
  const id = str(observed.external_id);
  const useful = id.length > 0 && id.length <= 12 && !taskTitle(task).includes(id);
  return [name, useful ? id : null].filter(Boolean).join(" · ") || null;
}

export function pullRequestNumber(task: Task): string | null {
  const url = str(task.pull_request?.url);
  if (!url) return null;
  const last = url.split("/").filter(Boolean).pop();
  return last ? `#${last}` : null;
}

export const detailLine = (snapshot: Snapshot, task: Task) =>
  [repositoryOf(snapshot, task), pullRequestNumber(task), sourceOf(snapshot, task)].filter(Boolean).join(" · ");

export function workspaceName(snapshot: Snapshot, task: Task): string {
  return str(arr(snapshot?.workspaces).find((w) => w.id === task.workspace_id)?.name) || "Workspace";
}

/** "now", "5 min ago", "1 hr ago", "yesterday", "5 days ago". */
export function relative(ms: number, now = Date.now()): string {
  if (!ms || ms <= 0) return "";
  const s = Math.max(0, Math.round((now - ms) / 1000));
  if (s < 60) return "now";
  const m = Math.floor(s / 60);
  if (m < 60) return `${m} min ago`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h} hr ago`;
  const d = Math.floor(h / 24);
  if (d === 1) return "yesterday";
  if (d < 30) return `${d} days ago`;
  const mo = Math.floor(d / 30);
  return mo < 12 ? `${mo} mo ago` : `${Math.floor(mo / 12)} yr ago`;
}

export function taskCount(n: number): string {
  return `${n} ${n === 1 ? "task" : "tasks"}`;
}

// ---------------------------------------------------------------------------
// Presentation (TicketPresentation.swift)
// ---------------------------------------------------------------------------

/** The task's open pull request URL, when its work went out as one. */
export function openPullRequest(task: Task): string | null {
  const url = str(task.pull_request?.url);
  if (!url || ["MERGED", "CLOSED"].includes(str(task.pull_request?.status?.state))) return null;
  return url;
}

/** The planner's question for a ticket waiting on you, if it asked one. */
export function waitingReason(task: Task): string | null {
  if (task.status !== "AwaitingApproval") return null;
  const decision = task.supervision;
  if (decision?.action === "AskUser") return `Needs your input: ${str(decision.reason)}`;
  if (decision?.action === "Skip") return `Nothing to build: ${str(decision.reason)}`;
  const event = [...arr(task.events)].reverse().find((e) => ["supervisor", "coordinator"].includes(e.role));
  if (event) {
    const message = str(event.message);
    for (const prefix of ["Needs your input before building: ", "Nothing to build: "]) {
      if (message.startsWith(prefix)) return `${prefix.startsWith("Needs") ? "Needs your input: " : "Nothing to build: "}${message.slice(prefix.length)}`;
    }
  }
  return null;
}

/** The newest daemon message on a stopped ticket, in one readable line. */
export function stopReason(task: Task): string {
  const event = [...arr(task.events)].reverse().find((e) => !["user", "note", "host", "neko"].includes(e.role));
  const line = str(event?.message).trim();
  if (line.includes("not a git repository")) return "This task isn’t linked to a Git repository yet, so Neko couldn’t make its working copy.";
  if (line.includes("daemon restart")) return "Stopped when Neko restarted. Start it again to continue.";
  if (line.startsWith("Run tool scope is paused, unavailable, or changed")) return "A tool connection it relied on was paused or changed while it worked. Start it again to retry.";
  if (line.startsWith("Independent verification failed:")) return `Review found problems: ${line.slice("Independent verification failed:".length).trim().slice(0, 380)}`;
  if (line.startsWith("Verifier returned a malformed verdict")) return "The reviewer’s answer couldn’t be read. Start it again to re-review.";
  return line ? line.slice(0, 400) : "Neko stopped without a reason. Start it again to retry.";
}

/** The line under a task that is blocked on you. */
export function blockedNote(task: Task): string | null {
  if (stageOf(task.status) !== "needsYou") return null;
  return task.status === "Failed" ? stopReason(task) : waitingReason(task);
}

export interface Review {
  passed: boolean;
  findings: string[];
  files: string[];
  tests: string[];
  summary: string;
}

function findingText(value: Json): string {
  if (typeof value === "string") return value;
  const severity = str(value?.severity);
  const path = str(value?.path);
  const body = str(value?.finding) || str(value?.message);
  const head = [severity ? severity[0].toUpperCase() + severity.slice(1) : null, path ? basename(path) : null].filter(Boolean).join(" · ");
  return head ? `${head}: ${body}` : body;
}

/** Splits a saved result into its text and the independent reviewer's verdict. */
export function parseResult(result: string): { body: string; verdict: Review | null } {
  const marker = "\n\nIndependent review:\n";
  const at = result.lastIndexOf(marker);
  if (at < 0) return { body: result, verdict: null };
  try {
    const v = JSON.parse(result.slice(at + marker.length));
    // Every field is required: a malformed reply must never read as a pass.
    if (typeof v.passed !== "boolean" || !Array.isArray(v.findings) || !Array.isArray(v.files) || !Array.isArray(v.tests) || typeof v.summary !== "string") {
      return { body: result, verdict: null };
    }
    return {
      body: result.slice(0, at),
      verdict: { passed: v.passed, findings: v.findings.map(findingText), files: v.files.map(String), tests: v.tests.map(String), summary: v.summary },
    };
  } catch {
    return { body: result, verdict: null };
  }
}

export type Outcome = "ready" | "completed" | "reviewing" | "stopped" | "previous";

export const OUTCOME_TITLE: Record<Outcome, string> = {
  ready: "Result",
  completed: "Result",
  reviewing: "Result under review",
  stopped: "Last recorded result",
  previous: "Previous result",
};
export const OUTCOME_NOTE: Record<Outcome, string> = {
  ready: "Ready for your review",
  completed: "Task completed",
  reviewing: "Independent review is in progress. This output has not cleared the current review.",
  stopped: "Work stopped. This saved output may be from an earlier attempt.",
  previous: "Saved from earlier work; it does not describe the current attempt.",
};

export function conversationState(task: Task) {
  const status = task.status;
  const outcome: Outcome =
    status === "ReadyForReview" ? "ready"
    : status === "Completed" ? "completed"
    : status === "Reviewing" ? "reviewing"
    : status === "Failed" || status === "Cancelled" ? "stopped"
    : "previous";
  return {
    outcome,
    reviewIsCurrent: outcome === "ready" || outcome === "completed",
    showsPlan: ["Planning", "AwaitingApproval"].includes(status) && str(task.plan).trim() !== "",
    planTitle: status === "Planning" ? "Saved plan · being revised" : "Plan",
  };
}

export function canProposeSplit(task: Task, splits: Json[]): boolean {
  return (
    task.status === "AwaitingApproval" &&
    !splits.some((s) => s.parent_id === task.id || arr(s.subtasks).some((sub) => sub.task_id === task.id))
  );
}

// ---------------------------------------------------------------------------
// Command receipts (TicketCommandEvidence.swift)
// ---------------------------------------------------------------------------

export interface Receipt {
  command: string | null;
  output: string | null;
  exitCode: number | null;
  commandTruncated: boolean;
  outputTruncated: boolean;
  incomplete: boolean;
  failed: boolean;
  title: string;
  status: string;
  notices: string[];
}

const RECEIPT_PREFIX = "VERIFICATION_COMMAND ";
const RECEIPT_ROLES = ["scout", "supervisor", "builder", "reviewer", "coordinator"];

/** "/bin/zsh -lc 'rg -n foo'" reads as "rg -n foo". */
export function shortCommand(command: string): string {
  let text = command;
  for (const shell of ["/bin/zsh -lc ", "/bin/bash -lc ", "bash -lc ", "zsh -lc "]) {
    if (text.startsWith(shell)) {
      text = text.slice(shell.length);
      const first = text[0];
      if ((first === "'" || first === '"') && text.endsWith(first)) text = text.slice(1, -1);
    }
  }
  text = text.replaceAll("/Library/Developer/CommandLineTools/usr/bin/", "");
  for (;;) {
    const cut = text.indexOf(" && ");
    if (!text.startsWith("cd ") || cut < 0) break;
    const target = text.slice(3, cut).replace(/^["' ]+|["' ]+$/g, "");
    if (!(target.includes("/neko/task-worktrees/") || target === ".")) break;
    text = text.slice(cut + 4);
  }
  return text.split("\n")[0].slice(0, 140);
}

export function receiptOf(event: Json): Receipt | null {
  // A human can paste anything into a reply, including this internal prefix.
  // Its content remains a message, never execution evidence.
  if (!RECEIPT_ROLES.includes(event?.role)) return null;
  const message = str(event.message);
  if (!message.startsWith(RECEIPT_PREFIX)) return null;
  let value: Json = null;
  try {
    value = JSON.parse(message.slice(RECEIPT_PREFIX.length));
  } catch {
    value = null;
  }
  value = value && typeof value === "object" ? value : {};
  const command = typeof value.command === "string" && value.command.trim() ? value.command : null;
  const output = typeof value.output === "string" ? value.output : null;
  const exitCode = Number.isInteger(value.exit_code) ? (value.exit_code as number) : null;
  const commandTruncated = value.command_truncated === true;
  const outputTruncated = value.output_truncated === true;
  const flags = ["command_truncated", "output_truncated", "receipt_incomplete", "receipt_malformed", "raw_truncated"];
  const incomplete =
    value.receipt_incomplete === true || value.receipt_malformed === true || value.raw != null ||
    flags.some((k) => k in value && typeof value[k] !== "boolean") ||
    command === null || output === null || exitCode === null || commandTruncated;
  const failed = exitCode !== null && exitCode !== 0;
  const status = incomplete
    ? failed ? `Failed · exit ${exitCode} · incomplete record` : "Incomplete record"
    : failed ? `Failed · exit ${exitCode}` : "Completed · exit 0";
  const notices: string[] = [];
  if (incomplete) notices.push("This saved record is incomplete. It does not provide full command evidence.");
  if (commandTruncated) notices.push("The command was shortened before it was saved.");
  if (outputTruncated) notices.push("Output was shortened before it was saved. Only the retained output is available here.");
  return {
    command, output, exitCode, commandTruncated, outputTruncated, incomplete, failed,
    title: command ? shortCommand(command) : "Command details unavailable",
    status, notices,
  };
}

/** The latest saved run of a check the reviewer reported. */
export function receiptFor(test: string, receipts: Receipt[]): Receipt | null {
  const wanted = test.trim();
  return [...receipts].reverse().find((r) => {
    const c = r.command?.trim();
    return !!c && (c === wanted || c.endsWith(`&& ${wanted}`));
  }) ?? null;
}

// ---------------------------------------------------------------------------
// The thread (TicketThread.swift)
// ---------------------------------------------------------------------------

export type ThreadItem =
  | { kind: "brief"; text: string }
  | { kind: "you"; text: string }
  | { kind: "agent"; role: string; text: string }
  | { kind: "steps"; role: string; commands: Receipt[] }
  | { kind: "status"; text: string };

export interface ThreadRow {
  id: string;
  at: number;
  item: ThreadItem;
  /** The speaker line shows only when the speaker changes. */
  showsSpeaker: boolean;
}

export function agentName(role: string): string {
  switch (role) {
    case "scout": case "supervisor": case "splitter": return "Planner";
    case "builder": return "Builder";
    case "reviewer": return "Reviewer";
    case "learning": return "Memory";
    default: return "Neko";
  }
}

function isPlumbing(message: string): boolean {
  return (
    message.startsWith("Command ") || message.startsWith("Advisory:") || message.startsWith("USAGE ") ||
    message === "Updating workspace files" || message.startsWith("{") || message.startsWith("Decision: ") ||
    message.startsWith("Needs your input before building:") || message.startsWith("Nothing to build:") ||
    message === "Agent started in an isolated task worktree"
  );
}

export function threadRows(task: Task): ThreadRow[] {
  const entries: { start: number; at: number; item: ThreadItem }[] = [];
  const goal = str(task.goal).trim();
  if (goal) entries.push({ start: -1, at: num(task.created_at_ms), item: { kind: "brief", text: goal } });
  const echoes = [str(task.plan), str(task.result)].map((s) => s.trim().slice(0, 160)).filter(Boolean);
  arr(task.events).forEach((event, index) => {
    const role = str(event.role);
    const message = str(event.message).trim();
    const at = num(event.at_ms);
    if (!message) return;
    const receipt = receiptOf(event);
    if (receipt) {
      const last = entries[entries.length - 1];
      if (last && last.item.kind === "steps" && last.item.role === role) last.item.commands.push(receipt);
      else entries.push({ start: index, at, item: { kind: "steps", role, commands: [receipt] } });
      return;
    }
    if (isPlumbing(message)) return;
    if (role === "note") entries.push({ start: index, at, item: { kind: "you", text: message } });
    else if (role === "user" || role === "system") entries.push({ start: index, at, item: { kind: "status", text: message } });
    else {
      if (echoes.some((e) => message.startsWith(e) || e.startsWith(message.slice(0, 160)))) return;
      if (message.startsWith("Working in ") || message === "Plan ready." || message.startsWith("Investigation complete.")) {
        entries.push({ start: index, at, item: { kind: "status", text: message } });
      } else entries.push({ start: index, at, item: { kind: "agent", role, text: message } });
    }
  });
  let previous: string | null = null;
  return entries.map((entry) => {
    const speaker = speakerOf(entry.item);
    const showsSpeaker = speaker !== null && speaker !== previous;
    if (speaker) previous = speaker;
    return { id: `${entry.item.kind}#${entry.start}`, at: entry.at, item: entry.item, showsSpeaker };
  });
}

function speakerOf(item: ThreadItem): string | null {
  switch (item.kind) {
    case "brief": case "you": return "you";
    case "agent": case "steps": return agentName(item.role);
    default: return null;
  }
}

/** The live line under a running ticket. */
export function workingLine(status: string): string | null {
  switch (status) {
    case "Queued": return "Getting this task ready…";
    case "Planning": return "Planner is reading the code…";
    case "Building": return "Builder is making the change in its own copy…";
    case "Reviewing": return "Reviewer is checking the change…";
    default: return null;
  }
}

/** Decisions: one row per episode, newest version (DecisionPresentation). */
export function latestDecisions(records: Json[]): Json[] {
  const episodes = new Map<string, Json>();
  for (const record of records) {
    const key = str(record.episode_id);
    const seen = episodes.get(key);
    if (!seen || num(record.version) > num(seen.version)) episodes.set(key, record);
  }
  return [...episodes.values()].sort((a, b) => num(b.created_at_ms) - num(a.created_at_ms));
}

export function decisionTitle(action: string): string {
  switch (action) {
    case "prepare_fix": return "Prepare a local fix";
    case "ask_user": return "Ask for your decision";
    case "skip": return "Defer this work";
    case "approve_local_build": return "You approved a local build";
    case "accept_local_result": return "You accepted the local result";
    case "cancel_task": return "You stopped this task";
    case "report_failure": return "Work stopped";
    case "review_local_result": return "Local work is ready to review";
    case "add_note": return "Your direction was recorded";
    default: return "Prepare a plan";
  }
}

// ---------------------------------------------------------------------------
// Moving tasks between board columns (TicketsView.moveCommand)
// ---------------------------------------------------------------------------

export function moveCommand(status: string, column: Stage): { command: string | null; reason: string | null } {
  const to = column;
  if ((to === "running" || to === "upNext") && ["AwaitingApproval", "Failed", "Cancelled"].includes(status)) return { command: "StartTask", reason: null };
  if (to === "done" && status === "ReadyForReview") return { command: "CompleteTask", reason: null };
  if (to === "done" && ["AwaitingApproval", "Queued", "Planning", "Building", "Reviewing"].includes(status)) return { command: "CancelTask", reason: null };
  if (to === "needsYou" && ["Queued", "Planning", "Building", "Reviewing"].includes(status)) {
    return { command: null, reason: "Work that has started can’t go back to waiting. Cancel it, or let it finish." };
  }
  if (to === "review") return { command: null, reason: "Neko moves a task to review once its work and checks are done." };
  if (to === "running" && status === "ReadyForReview") return { command: null, reason: "Add a note on the task to send it back with changes." };
  return { command: null, reason: null };
}

// ---------------------------------------------------------------------------
// Diff (TaskChanges)
// ---------------------------------------------------------------------------

export interface DiffLine { kind: "add" | "del" | "hunk" | "ctx"; text: string; oldNo?: number; newNo?: number }
export interface DiffFile { path: string; added: number; removed: number; lines: DiffLine[] }

/** Splits a unified diff into files with line numbers. */
export function parseDiff(patch: string): DiffFile[] {
  const files: DiffFile[] = [];
  let current: DiffFile | null = null;
  let oldNo = 0;
  let newNo = 0;
  for (const raw of patch.split("\n")) {
    if (raw.startsWith("diff --git ")) {
      const m = raw.match(/ b\/(.+)$/);
      current = { path: m ? m[1] : raw.slice(11), added: 0, removed: 0, lines: [] };
      files.push(current);
      continue;
    }
    if (!current) continue;
    if (raw.startsWith("@@")) {
      const m = raw.match(/@@ -(\d+)(?:,\d+)? \+(\d+)/);
      oldNo = m ? Number(m[1]) : 0;
      newNo = m ? Number(m[2]) : 0;
      current.lines.push({ kind: "hunk", text: raw });
    } else if (raw.startsWith("+++") || raw.startsWith("---") || raw.startsWith("index ") || raw.startsWith("new file") || raw.startsWith("deleted file") || raw.startsWith("similarity") || raw.startsWith("rename ") || raw.startsWith("old mode") || raw.startsWith("new mode")) {
      if (raw.startsWith("+++ b/")) current.path = raw.slice(6);
      continue;
    } else if (raw.startsWith("+")) {
      current.added += 1;
      current.lines.push({ kind: "add", text: raw.slice(1), newNo: newNo++ });
    } else if (raw.startsWith("-")) {
      current.removed += 1;
      current.lines.push({ kind: "del", text: raw.slice(1), oldNo: oldNo++ });
    } else if (raw.startsWith("\\")) {
      current.lines.push({ kind: "ctx", text: raw });
    } else if (current.lines.length) {
      current.lines.push({ kind: "ctx", text: raw.slice(1), oldNo: oldNo++, newNo: newNo++ });
    }
  }
  return files;
}

// ---------------------------------------------------------------------------
// Pull requests (PullRequestRow)
// ---------------------------------------------------------------------------

export type Verdict = "ready" | "close" | "needsYou" | "merging" | "working" | "unchecked" | "merged" | "closed";
export const VERDICTS: Verdict[] = ["ready", "close", "needsYou", "merging", "working", "unchecked", "merged", "closed"];
export const VERDICT_TITLE: Record<Verdict, string> = {
  ready: "Ready to merge",
  close: "Suggested to close",
  needsYou: "Needs your call",
  merging: "Merging when checks pass",
  working: "Being updated",
  unchecked: "Not checked on GitHub yet",
  merged: "Merged",
  closed: "Closed",
};

export interface PullRequestRow {
  id: string;
  task: Task;
  title: string;
  url: string;
  number: string;
  repository: string | null;
  repositoryPath: string;
  verdict: Verdict;
  reason: string | null;
  checks: string | null;
  review: string | null;
  conflicts: boolean;
  behind: boolean;
  draft: boolean;
  updated: number;
  base: string;
  checksFailing: boolean;
  followUps: number;
  meta: string;
  problems: { kind: "checks" | "conflicts" | "behind"; text: string }[];
}

export function pullRequestRow(task: Task, snapshot: Snapshot): PullRequestRow {
  const pr = task.pull_request ?? {};
  const status = pr.status ?? {};
  const base = str(status.base) || "main";
  const checksFailing = status.checks === "failing";
  const conflicts = status.merge_state === "DIRTY" || status.mergeable === "CONFLICTING";
  const behind = status.merge_state === "BEHIND";
  const draft = status.draft === true;
  const alsoOnBase = status.base_failing ? ` (also failing on ${base})` : "";
  const checks =
    status.checks === "passing" ? "Checks passing"
    : status.checks === "failing" ? `Checks failing${arr(status.failing).length ? `: ${arr(status.failing).join(", ")}` : ""}${alsoOnBase}`
    : status.checks === "pending" ? "Checks running"
    : status.checks === "none" ? "No checks"
    : null;
  const review =
    status.review === "APPROVED" ? "Approved"
    : status.review === "CHANGES_REQUESTED" ? "Changes requested"
    : status.review === "REVIEW_REQUIRED" ? "Review required"
    : null;
  const state = str(status.state);
  const taskStatus = task.status;
  const suggestion = str(pr.close_suggestion);
  let verdict: Verdict;
  let reason: string | null = null;
  if (state === "MERGED") verdict = "merged";
  else if (state === "CLOSED") verdict = "closed";
  else if (["Queued", "Planning", "Building", "Reviewing"].includes(taskStatus)) verdict = "working";
  // Until Neko has read it, it may already be merged or closed: no verdict.
  else if (!state) { verdict = "unchecked"; reason = "Neko reads it on GitHub within a few minutes."; }
  else if (pr.merge_when_ready === true) {
    verdict = "merging";
    const waiting = str(pr.follow_up?.merge_note);
    reason = waiting ? `Waiting: ${waiting}` : null;
  } else if (suggestion) { verdict = "close"; reason = suggestion; }
  else if (taskStatus === "Cancelled") { verdict = "close"; reason = "Its task was cancelled here, but the pull request is still open."; }
  else if (["AwaitingApproval", "Failed"].includes(taskStatus)) {
    verdict = "needsYou";
    reason = taskStatus === "Failed" ? stopReason(task) : waitingReason(task);
  } else verdict = "ready";
  const number = pullRequestNumber(task) ?? "";
  const repository = repositoryOf(snapshot, task);
  const problems: PullRequestRow["problems"] = [];
  if (checksFailing && checks) problems.push({ kind: "checks", text: checks });
  if (conflicts) problems.push({ kind: "conflicts", text: `Conflicts with ${base}` });
  if (behind) problems.push({ kind: "behind", text: `Behind ${base}` });
  const f = pr.follow_up ?? {};
  const followUps = num(f.count);
  return {
    id: task.id, task, title: taskTitle(task), url: str(pr.url), number, repository,
    repositoryPath: str(snapshot?.task_roots?.[task.id]),
    verdict, reason, checks, review, conflicts, behind, draft, updated: num(task.updated_at_ms), base, checksFailing, followUps,
    meta: [number || null, repository, review, draft ? "Draft" : null, checksFailing ? null : checks].filter(Boolean).join(" · "),
    problems,
  };
}

/** Every task with a pull request, decisions first. */
export function pullRequestRows(snapshot: Snapshot, tasks: Task[]): PullRequestRow[] {
  return tasks
    .filter((t) => str(t.pull_request?.url))
    .map((t) => pullRequestRow(t, snapshot))
    .sort((a, b) => VERDICTS.indexOf(a.verdict) - VERDICTS.indexOf(b.verdict) || b.updated - a.updated);
}

/** "2 ready · 1 to close", only the parts that are not zero. */
export function pullRequestSubtitle(rows: PullRequestRow[]): string {
  const parts = ([["ready", "ready"], ["close", "to close"], ["needsYou", "need you"]] as [Verdict, string][])
    .map(([v, label]) => [rows.filter((r) => r.verdict === v).length, label] as const)
    .filter(([n]) => n > 0)
    .map(([n, label]) => `${n} ${label}`);
  return parts.length ? parts.join(" · ") : "Nothing waiting on you";
}

// ---------------------------------------------------------------------------
// The agent tree (AgentTree.swift)
// ---------------------------------------------------------------------------

export type NodeKind = "neko" | "watch" | "task" | "run";
export type NodeState =
  | "running" | "upNext" | "needsYou" | "review" | "done" | "passed" | "changesAsked"
  | "stopped" | "failed" | "watching" | "paused" | "failing";

export const NODE_STATE_TITLE: Record<NodeState, string> = {
  running: "Running", upNext: "Up next", needsYou: "Needs you", review: "Ready to review", done: "Done",
  passed: "Passed", changesAsked: "Changes asked", stopped: "Stopped", failed: "Failed",
  watching: "Watching", paused: "Paused", failing: "Failing",
};

export interface AgentNode {
  id: string;
  kind: NodeKind;
  name: string;
  subtitle: string;
  work: string;
  origin?: string;
  state: NodeState;
  costUSD: number | null;
  durationMs: number | null;
  tools: string[];
  taskId?: string;
  children: AgentNode[];
}

export interface AgentTree {
  root: AgentNode;
  tasks: number;
  running: number;
  costUSD: number;
}

export const UNRECORDED_MODEL = "Model not recorded";

const RUN_ROLES: Record<string, { name: string; work: string }> = {
  scout: { name: "Scout", work: "Read the code and planned the work" },
  builder: { name: "Builder", work: "Made the change and checked it" },
  reviewer: { name: "Reviewer", work: "Checked the change independently" },
  coordinator: { name: "Recovery", work: "Worked out why it stopped and what to try next" },
};

export function descendants(node: AgentNode): AgentNode[] {
  return node.children.flatMap((c) => [c, ...descendants(c)]);
}

export const isLive = (node: AgentNode) => node.state === "running";

function taskNodeState(status: string): NodeState {
  switch (status) {
    case "Planning": case "Building": case "Reviewing": return "running";
    case "Queued": return "upNext";
    case "AwaitingApproval": return "needsYou";
    case "ReadyForReview": return "review";
    case "Completed": return "done";
    case "Failed": return "failed";
    default: return "stopped";
  }
}

/** "Claude Opus", "Codex gpt-5.5", or the provider alone. */
export function modelLabel(runtime: Json): string {
  const p = str(runtime?.provider);
  const provider = p === "" || p === "codex" ? "Codex" : p === "claude" ? "Claude" : p === "opencode" ? "OpenCode" : p[0].toUpperCase() + p.slice(1);
  const model = str(runtime?.model);
  if (!model) return provider;
  const shown = /^[a-z]+$/.test(model) ? model[0].toUpperCase() + model.slice(1) : model;
  return `${provider} ${shown}`;
}

function modelFor(taskId: string, at: number, selections: Json[]): string | null {
  const own = selections.filter((s) => s.conversation_id === `task:${taskId}`);
  const chosen = [...own].reverse().find((s) => num(s.selected_at_ms) <= at) ?? own[0];
  return chosen ? modelLabel(chosen.runtime) : null;
}

/** The agent runs in a task's event log, oldest first. */
export function runsOf(task: Task, selections: Json[], now: number): AgentNode[] {
  interface Run { role: string; start: number; end: number; cost: number | null; closed: boolean; verdict: boolean | null; stoppedBy: string | null }
  const runs: Run[] = [];
  for (const event of arr(task.events)) {
    const role = str(event.role);
    const message = str(event.message);
    const at = num(event.at_ms);
    if (!RUN_ROLES[role]) {
      // Anything else (Neko, the host, you) ends the run in progress.
      const last = runs[runs.length - 1];
      if (last && !last.closed) {
        last.closed = true;
        if (role === "supervisor") last.stoppedBy = message;
      }
      continue;
    }
    const restart = message.startsWith("Agent started");
    let last = runs[runs.length - 1];
    if (last && last.role === role && !last.closed && !restart) last.end = at;
    else {
      last = { role, start: at, end: at, cost: null, closed: false, verdict: null, stoppedBy: null };
      runs.push(last);
    }
    if (message.startsWith("USAGE ")) {
      try {
        const usage = JSON.parse(message.slice(6));
        if (typeof usage.cost_usd === "number") {
          last.cost = (last.cost ?? 0) + usage.cost_usd;
          last.closed = true;
        }
      } catch { /* not a usage line */ }
    } else if (role === "reviewer" && message.startsWith("{")) {
      try {
        const verdict = JSON.parse(message);
        if (typeof verdict.passed === "boolean") last.verdict = verdict.passed;
      } catch { /* not a verdict */ }
    }
  }
  const status = task.status;
  const live = stageOf(status) === "running";
  return runs.map((run, index) => {
    const isLast = index === runs.length - 1;
    let state: NodeState;
    if (isLast && live && !run.closed) state = "running";
    else if (run.verdict !== null) state = run.verdict ? "passed" : "changesAsked";
    else if (isLast && ["Failed", "Cancelled"].includes(status) && run.cost === null) state = "stopped";
    else if (run.stoppedBy !== null && !run.stoppedBy.startsWith("Result ready") && !run.stoppedBy.startsWith("Opened") && !run.stoppedBy.startsWith("Plan ready") && run.cost === null) state = "stopped";
    else state = "done";
    const end = state === "running" ? now : run.end;
    const label = RUN_ROLES[run.role];
    return {
      id: `${task.id}#${index}`, kind: "run" as const, name: label.name,
      subtitle: modelFor(task.id, run.start, selections) ?? UNRECORDED_MODEL,
      work: label.work, state, costUSD: run.cost,
      durationMs: end >= run.start ? end - run.start : null,
      tools: [], taskId: task.id, children: [],
    };
  });
}

function taskOrder(a: Task, b: Task): number {
  const rank = (t: Task) => ({ running: 0, needsYou: 1, review: 2, upNext: 3, done: 4 }[stageOf(t.status)]);
  return rank(a) - rank(b) || num(b.updated_at_ms) - num(a.updated_at_ms);
}

const unique = (names: string[]) => [...new Set(names.filter(Boolean))];
const sum = (values: number[]) => values.reduce((a, b) => a + b, 0);

/** Builds the tree for `tasks` (already filtered to what the page shows). */
export function buildTree(snapshot: Snapshot, tasks: Task[], opts: { workspace?: string | null; showIdleWatches?: boolean; now: number }): AgentTree {
  const { workspace = null, showIdleWatches = true, now } = opts;
  const selections = arr(snapshot?.runtime_selections);
  const visible = new Set(tasks.map((t) => t.id));
  const byId = new Map(tasks.map((t) => [t.id as string, t]));

  // Who started whom.
  const parentOf = new Map<string, string>();
  for (const split of arr(snapshot?.splits)) {
    for (const sub of arr(split.subtasks)) if (str(sub.task_id)) parentOf.set(sub.task_id, split.parent_id);
  }
  // A fix an agent proposed in another repository sits under the task that found it.
  const followUpOf = new Map<string, string>();
  for (const [child, parent] of Object.entries<Json>(snapshot?.follow_up_of ?? {})) {
    followUpOf.set(child, String(parent));
    parentOf.set(child, String(parent));
  }
  const watchOf = new Map<string, string>();
  for (const source of arr(snapshot?.mcp?.sources)) if (str(source.task_id)) watchOf.set(source.task_id, source.responsibility_id);
  const fromChat = new Set(arr(snapshot?.conversation).flatMap((m) => arr(m.ticket_ids).map(String)));
  const connectionNames = new Map(arr(snapshot?.mcp?.connections).map((c) => [str(c.id), sourceName(str(c.label))]));

  const taskNode = (task: Task): AgentNode => {
    const id = task.id as string;
    const runs = runsOf(task, selections, now);
    const splitChildren = tasks.filter((t) => parentOf.get(t.id) === id).sort(taskOrder).map(taskNode);
    const own = runs.map((r) => r.costUSD).filter((c): c is number => c !== null);
    const below = splitChildren.map((c) => c.costUSD).filter((c): c is number => c !== null);
    const cost = own.length || below.length ? sum([...own, ...below]) : null;
    const created = num(task.created_at_ms);
    const ended = stageOf(task.status) === "running" ? now : num(task.updated_at_ms);
    let origin: string;
    const parent = parentOf.get(id);
    if (parent && byId.get(parent)) {
      const parentTitle = taskTitle(byId.get(parent)!);
      origin = followUpOf.has(id) ? `Proposed by “${parentTitle}”` : `Split from “${parentTitle}”`;
    } else if (watchOf.has(id)) {
      const source = sourceOf(snapshot, task);
      origin = source ? `Filed from ${source}` : "Filed by a watch";
    } else if (fromChat.has(id)) origin = "From your chat with Neko";
    else origin = "You asked";
    const source = arr(snapshot?.mcp?.sources).find((s) => s.task_id === id);
    const tool = watchOf.has(id) && source ? connectionNames.get(str(source.connection_id)) : undefined;
    return {
      id, kind: "task", name: taskTitle(task),
      subtitle: [...runs].reverse().find((r) => r.subtitle !== UNRECORDED_MODEL)?.subtitle ?? "",
      work: [repositoryOf(snapshot, task), pullRequestNumber(task)].filter(Boolean).join(" · "),
      origin, state: taskNodeState(task.status), costUSD: cost,
      durationMs: created > 0 && ended >= created ? ended - created : null,
      tools: tool ? [tool] : [], taskId: id, children: [...runs, ...splitChildren],
    };
  };

  // Tasks a split started sit under their parent, not at the top.
  const topLevel = tasks.filter((t) => {
    const parent = parentOf.get(t.id);
    return parent ? !visible.has(parent) : true;
  });

  const watchNodes: AgentNode[] = [];
  for (const watch of arr(snapshot?.mcp?.responsibilities)) {
    if (workspace && watch.workspace_id !== workspace) continue;
    const filed = topLevel.filter((t) => watchOf.get(t.id) === watch.id).sort(taskOrder).map(taskNode);
    const enabled = watch.enabled === true;
    if (!filed.length && !(showIdleWatches && enabled)) continue;
    const name = str(watch.name) || str(watch.instruction).slice(0, 60);
    const costs = filed.map((c) => c.costUSD).filter((c): c is number => c !== null);
    watchNodes.push({
      id: `watch:${watch.id}`, kind: "watch", name, subtitle: "Watch", work: str(watch.instruction).trim(),
      state: !enabled ? "paused" : num(watch.failures) > 0 ? "failing" : "watching",
      costUSD: costs.length ? sum(costs) : null, durationMs: null,
      tools: unique(arr(watch.connection_ids).map((c) => connectionNames.get(String(c)) ?? "")),
      children: filed,
    });
  }
  // Watches with live work first, then the busiest.
  const liveBelow = (n: AgentNode) => (descendants(n).some(isLive) ? 0 : 1);
  watchNodes.sort((a, b) => liveBelow(a) - liveBelow(b) || b.children.length - a.children.length || a.name.localeCompare(b.name));

  const direct = topLevel
    .filter((t) => {
      const watch = watchOf.get(t.id);
      return !watch || !watchNodes.some((n) => n.id === `watch:${watch}`);
    })
    .sort(taskOrder)
    .map(taskNode);

  const children = [...watchNodes, ...direct];
  const everything = children.flatMap((c) => [c, ...descendants(c)]);
  const runNodes = everything.filter((n) => n.kind === "run");
  const running = runNodes.filter(isLive).length;
  const cost = sum(runNodes.map((n) => n.costUSD ?? 0));
  const taskNodes = everything.filter((n) => n.kind === "task");
  const waiting = taskNodes.filter((n) => n.state === "needsYou" || n.state === "review").length;
  const work =
    taskNodes.length === 0 ? "Nothing to look after yet"
    : waiting > 0 ? `Looking after ${taskCount(taskNodes.length)}, ${waiting} waiting on you`
    : `Looking after ${taskCount(taskNodes.length)}`;
  const root: AgentNode = {
    id: "neko", kind: "neko", name: "Neko", subtitle: "Chief of staff", work,
    state: running > 0 ? "running" : waiting > 0 ? "needsYou" : "done",
    costUSD: runNodes.some((n) => n.costUSD !== null) ? cost : null,
    durationMs: null, tools: [], children,
  };
  return { root, tasks: taskNodes.length, running, costUSD: cost };
}

/** Neko is always open; a watch or task opens only while an agent below it works. */
export function expandsByDefault(node: AgentNode): boolean {
  if (node.kind === "neko") return true;
  if (node.kind === "run") return false;
  return descendants(node).some((n) => n.kind === "run" && isLive(n));
}

export function visibleOrder(node: AgentNode, isExpanded: (n: AgentNode) => boolean): string[] {
  return [node.id, ...(isExpanded(node) ? node.children.flatMap((c) => visibleOrder(c, isExpanded)) : [])];
}

/** "$0.27", "$12", or "<$0.01". */
export function formatCost(usd: number): string {
  if (usd > 0 && usd < 0.01) return "<$0.01";
  return usd >= 100 ? `$${usd.toFixed(0)}` : `$${usd.toFixed(2)}`;
}

/** "18s", "1m 30s", "2h 05m", "3d". */
export function formatDuration(ms: number): string {
  const s = Math.max(0, Math.floor(ms / 1000));
  if (s < 60) return `${s}s`;
  if (s < 3600) return `${Math.floor(s / 60)}m ${s % 60}s`;
  if (s < 86400) return `${Math.floor(s / 3600)}h ${String(Math.floor((s % 3600) / 60)).padStart(2, "0")}m`;
  return `${Math.floor(s / 86400)}d`;
}

export function treeSummary(tree: AgentTree): string {
  return `${taskCount(tree.tasks)} · ${tree.running} running · ${formatCost(tree.costUSD)} reported`;
}
