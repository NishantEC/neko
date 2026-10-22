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
    "Ran `gh pr view 101 --json reviews,mergeStateStatus`",
    "Read neko_chat.rs",
    "Searched for `mergeStateStatus`",
    "Ran `git log --oneline -5` · failed",
  ],
  text: [
    "These statuses come from Neko's record; I did not query GitHub live. All 5 pass checks, but none has an approving review yet.",
    "",
    "| PR | Task | Checks | Conflicts | Verdict |",
    "| --- | --- | --- | --- | --- |",
    "| acme-api #101 | Add review tools for report review calls | passing | none | Blocked: the branch is behind master. It needs updating, then a review. |",
    "| acme-api #102 | Show friendly messages instead of raw server error text | passing | none | Ready once someone approves it |",
    "| acme-web #110 | Let Ria agents use tools from remote MCP servers | passing | none | Ready once someone approves it |",
    "",
    "Nothing here needs you to act today. Want me to update `acme-api #101` from master?",
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
  remembered: ["You review acme-api pull requests before acme-web ones"],
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
      arguments_json: JSON.stringify({ title: "Update acme-api #101 branch", team: "HEA", description: "Rebase onto master and rerun checks" }),
      status: "awaiting_approval",
    },
    {
      id: "c3",
      connection_id: "github",
      tool_name: "get_pull_request",
      arguments_json: JSON.stringify({ owner: "acme", repo: "acme-api", number: 101 }),
      status: "failed",
    },
  ],
};

export const queued: NekoMessage[] = [
  { id: "q1", at_ms: now, role: "user", queued: true, text: "After that, summarise what changed in acme-api this week" },
  { id: "q2", at_ms: now, role: "user", queued: true, text: "And do not start anything on acme-web until I say so" },
];

export const sampleArgs = {
  title: "Update acme-api #101 branch",
  team: "HEA",
  labels: ["merge", "acme-api"],
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
  { ref: "acme-api #101", task: "Add review tools for report review calls", status: "Blocked", checks: "passing" },
  { ref: "acme-api #102", task: "Show friendly messages instead of raw server error text", status: "Ready", checks: "passing" },
  { ref: "acme-web #110", task: "Let Ria agents use tools from remote MCP servers", status: "Ready", checks: "failing" },
];
