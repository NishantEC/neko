// Realistic Neko data for the gallery: shaped like the daemon's snapshot, so
// what the gallery shows is what Home will show.
import type { NekoPart, NekoUIMessage } from "@/chat/sdk/types";

const now = Date.now();

const text = (value: string): NekoPart => ({ type: "text", text: value, state: "done" });
const bash = (id: string, command: string, output: string, exit = 0): NekoPart => ({
  type: "dynamic-tool", toolName: "Bash", toolCallId: id, state: "output-available", input: { command }, output: { exit_code: exit, output },
});

export const userMessage: NekoUIMessage = {
  id: "u1",
  role: "user",
  metadata: { atMs: now - 150_000, state: "done" },
  parts: [text("Which of my pull requests are ready to merge? Check conflicts too.")],
};

export const finishedReply: NekoUIMessage = {
  id: "n1",
  role: "assistant",
  metadata: { atMs: now - 70_000, startedMs: now - 142_000, state: "done" },
  parts: [
    { type: "step-start" },
    { type: "reasoning", text: "List the open pull requests, then check each one's merge state.", state: "done" },
    bash("g1", "gh pr list --state open --json number,title,statusCheckRollup", "4641 Update the branch\n4628 Fix the sync"),
    bash("g2", "gh pr view 4641 --json reviews,mergeStateStatus", "{\"mergeStateStatus\":\"BEHIND\"}"),
    { type: "dynamic-tool", toolName: "Read", toolCallId: "g3", state: "output-available", input: { file_path: "/Users/you/neko/crates/neko-core/src/neko_chat.rs" }, output: { output: "…" } },
    bash("g4", "git log --oneline -5", "fatal: not a git repository", 128),
    {
      type: "dynamic-tool", toolName: "list_issues", toolCallId: "c1", state: "output-available",
      input: { assignee: "me", state: "In Review", limit: 10 }, output: { status: "succeeded" }, toolMetadata: { connectionId: "linear" },
    },
    text([
      "These statuses come from Neko's record; I did not query GitHub live. All 5 pass checks, but none has an approving review yet.",
      "",
      "| PR | Task | Checks | Conflicts | Verdict |",
      "| --- | --- | --- | --- | --- |",
      "| athena #4641 | Add coach tools for report review calls | passing | none | Blocked: the branch is behind master. It needs updating, then a review. |",
      "| athena #4628 | Show friendly messages instead of raw server error text | passing | none | Ready once someone approves it |",
      "| riavox #410 | Let Ria agents use tools from remote MCP servers | passing | none | Ready once someone approves it |",
      "",
      "Nothing here needs you to act today. Want me to update `athena #4641` from master?",
    ].join("\n")),
    { type: "data-neko-memory", data: { items: ["You review athena pull requests before riavox ones"] } },
  ],
};

export const runningReply: NekoUIMessage = {
  id: "n2",
  role: "assistant",
  metadata: { atMs: now, startedMs: now - 23_000, state: "pending" },
  parts: [
    { type: "step-start" },
    { type: "dynamic-tool", toolName: "Read", toolCallId: "r1", state: "output-available", input: { file_path: "/Users/you/neko/crates/neko-core/src/neko_chat.rs" }, output: { output: "…" } },
    bash("r2", "cargo test -p neko-core chat -- --nocapture", "running 12 tests"),
    {
      type: "dynamic-tool", toolName: "save_issue", toolCallId: "c2", state: "approval-requested",
      input: { title: "Update athena #4641 branch", team: "HEA", description: "Rebase onto master and rerun checks" },
      approval: { id: "c2" }, toolMetadata: { connectionId: "linear" },
    },
    {
      type: "dynamic-tool", toolName: "get_pull_request", toolCallId: "c3", state: "output-error",
      input: { owner: "healthifyme", repo: "athena", number: 4641 }, errorText: "The call failed or was stopped.", toolMetadata: { connectionId: "github" },
    },
    { type: "text", text: "Checking the chat tests first.\n\nThe suite is still running", state: "streaming" },
  ],
};

export const queued: NekoUIMessage[] = [
  { id: "q1", role: "user", metadata: { atMs: now, state: "queued" }, parts: [text("After that, summarise what changed in athena this week")] },
  { id: "q2", role: "user", metadata: { atMs: now, state: "queued" }, parts: [text("And do not start anything on riavox until I say so")] },
];

export const sampleArgs = {
  title: "Update athena #4641 branch",
  team: "HEA",
  labels: ["merge", "athena"],
  description: "Rebase onto master and rerun checks",
  notify: { slack: true, email: false },
};

export const usage = [
  { day: "Mon", tokens: 182 },
  { day: "Tue", tokens: 240 },
  { day: "Wed", tokens: 96 },
  { day: "Thu", tokens: 310 },
  { day: "Fri", tokens: 275 },
  { day: "Sat", tokens: 60 },
  { day: "Sun", tokens: 124 },
];

export const prRows = [
  { ref: "athena #4641", task: "Add coach tools for report review calls", status: "Blocked", checks: "passing" },
  { ref: "athena #4628", task: "Show friendly messages instead of raw server error text", status: "Ready", checks: "passing" },
  { ref: "riavox #410", task: "Let Ria agents use tools from remote MCP servers", status: "Ready", checks: "failing" },
];

/** A reply that asks a question and shows a plan, a chart and a form, as typed parts. */
export const structuredReply: NekoUIMessage = {
  id: "n-structured",
  role: "assistant",
  metadata: { atMs: now - 30_000, state: "done" },
  parts: [
    text("I need one answer before I propose the ticket."),
    { type: "data-neko-choices", data: { question: "What should the rewrite change?", options: [{ label: "New design", detail: "Same data, a cleaner page." }, { label: "Data that isn't stuck in one browser", detail: "Save entries through the daemon." }] } },
    { type: "data-neko-plan", data: { title: "Move the log to the daemon", estimate: "about 20 min", steps: ["Add host.storage calls", "Import the old localStorage log once", "Check a reload keeps it"] } },
    { type: "data-neko-chart", data: { title: "Entries this week", unit: "", bars: [{ label: "Mon", value: 3 }, { label: "Tue", value: 5, highlight: true }, { label: "Wed", value: 2 }] } },
    { type: "data-neko-form", data: { title: "Details", submit: "Send", fields: [{ label: "Track", kind: "choice", options: ["Weight", "Sleep"] }, { label: "Remind me", kind: "toggle", value: true }, { label: "Note", kind: "text", placeholder: "Optional" }] } },
  ],
};
