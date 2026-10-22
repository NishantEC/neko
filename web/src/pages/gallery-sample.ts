// Realistic Neko data for the gallery: shaped like the daemon's snapshot, so
// what the gallery shows is what Home will show.
import type { NekoMessage } from "@/chat/types";

const now = Date.now();

export const userMessage: NekoMessage = {
  id: "u1",
  at_ms: now - 150_000,
  role: "user",
  text: "Which of my pull requests are ready to merge? Check conflicts too.",
};

export const finishedReply: NekoMessage = {
  id: "n1",
  at_ms: now - 70_000,
  started_ms: now - 142_000,
  role: "assistant",
  activity: [
    "Ran `gh pr list --state open --json number,title,statusCheckRollup`",
    "Ran `gh pr view 4641 --json reviews,mergeStateStatus`",
    "Read neko_chat.rs",
    "Searched for `mergeStateStatus`",
    "Ran `git log --oneline -5` · failed",
  ],
  text: [
    "These statuses come from Neko's record; I did not query GitHub live. All 5 pass checks, but none has an approving review yet.",
    "",
    "| PR | Task | Checks | Conflicts | Verdict |",
    "| --- | --- | --- | --- | --- |",
    "| athena #4641 | Add coach tools for report review calls | passing | none | Blocked: the branch is behind master. It needs updating, then a review. |",
    "| athena #4628 | Show friendly messages instead of raw server error text | passing | none | Ready once someone approves it |",
    "| riavox #410 | Let Ria agents use tools from remote MCP servers | passing | none | Ready once someone approves it |",
    "",
    "Nothing here needs you to act today. Want me to update `athena #4641` from master?",
  ].join("\n"),
  tool_calls: [
    {
      id: "c1",
      connection_id: "linear",
      tool_name: "list_issues",
      arguments_json: JSON.stringify({ assignee: "me", state: "In Review", limit: 10 }),
      status: "succeeded",
    },
  ],
  remembered: ["You review athena pull requests before riavox ones"],
};

export const runningReply: NekoMessage = {
  id: "n2",
  at_ms: now,
  started_ms: now - 23_000,
  role: "assistant",
  pending: true,
  activity: ["Read neko_chat.rs", "Ran `cargo test -p neko-core chat -- --nocapture`"],
  text: "Checking the chat tests first.\n\nThe suite is still running",
  tool_calls: [
    {
      id: "c2",
      connection_id: "linear",
      tool_name: "save_issue",
      arguments_json: JSON.stringify({ title: "Update athena #4641 branch", team: "HEA", description: "Rebase onto master and rerun checks" }),
      status: "awaiting_approval",
    },
    {
      id: "c3",
      connection_id: "github",
      tool_name: "get_pull_request",
      arguments_json: JSON.stringify({ owner: "healthifyme", repo: "athena", number: 4641 }),
      status: "failed",
    },
  ],
};

export const queued: NekoMessage[] = [
  { id: "q1", at_ms: now, role: "user", queued: true, text: "After that, summarise what changed in athena this week" },
  { id: "q2", at_ms: now, role: "user", queued: true, text: "And do not start anything on riavox until I say so" },
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
