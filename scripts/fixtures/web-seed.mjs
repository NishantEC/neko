// A made-up workbench snapshot for scripts/web-verify.mjs, so the checks never
// depend on the owner's live data (the old seed was a copy of the live
// snapshot in /tmp, lost on every reboot, and this repository is public, so
// real task text must never be committed). The web harness copies the
// workspaces, the pull-request tasks, the 14 newest other tasks and the last 8
// chat messages out of it (crates/neko-daemon/src/bin/web_harness.rs,
// `seed_from`); scripts/verify/work.mjs builds its richer pass from it.
//
// What the checks need, and so what is here:
//   - Home: finished Neko replies, one of them with a GFM table;
//   - Tasks: every stage, 8 waiting for approval (work.mjs turns 7 of them into
//     running, planning, queued and split tasks), completed, failed, cancelled,
//     and a ready one whose result carries an independent review;
//   - Pull requests: open ones in every state, merged and closed ones;
//   - the agent tree: two watches that filed some of the tasks.

const MINUTE = 60_000;
const HOUR = 60 * MINUTE;

const WORKSPACES = [
  { id: "ws-api", name: "acme-api", repository: "/tmp/neko-fixture/acme-api", instructions: "", away_enabled: false },
  { id: "ws-web", name: "acme-web", repository: "/tmp/neko-fixture/acme-web", instructions: "", away_enabled: false },
];

const TABLE_REPLY = `Here is where the open pull requests stand:

| PR | Checks | Blocker |
|---|---|---|
| acme-api #101 | passing | needs one approving review |
| acme-api #102 | failing | \`Lint\` fails on an unused import |
| acme-web #57 | passing | branch is behind \`main\` |

Two need nothing but a review. I have not changed anything.`;

const FINAL_REPLY = `The retry is in place: a request that times out is tried twice more with a short back-off, and only then reported.

| Attempt | Wait |
|---|---|
| 1 | none |
| 2 | 200 ms |
| 3 | 800 ms |

The tests for the client pass.`;

let counter = 0;
const id = (prefix) => `${prefix}-${String(++counter).padStart(4, "0")}`;

function task(now, { title, status, workspace = "ws-api", ago = 2 * HOUR, result = "", plan = "", pr = null, events = null }) {
  const created = now - ago;
  return {
    id: id("task"),
    workspace_id: workspace,
    issue_id: null,
    title,
    goal: `${title}.`,
    status,
    plan: plan || `1. Find where it happens.\n2. ${title}.\n3. Add a test that fails without the change.`,
    result,
    worktree: null,
    events: events ?? [
      { at_ms: created, role: "system", message: "Queued for planning." },
      { at_ms: created + 5 * MINUTE, role: "scout", message: "Plan ready." },
    ],
    created_at_ms: created,
    updated_at_ms: created + 10 * MINUTE,
    source_revision: null,
    supervision: null,
    ...(pr ? { pull_request: pr } : {}),
  };
}

function pullRequest(repo, number, status) {
  return {
    url: `https://github.com/acme/${repo}/pull/${number}`,
    branch: `codex/neko-fixture-${number}`,
    status: { checks: "passing", mergeable: "MERGEABLE", review: "REVIEW_REQUIRED", head: "a1b2c3d", base: "main", merge_state: "BLOCKED", base_head: "e4f5a6b", ...status },
  };
}

// The result's review block, as the reviewer writes it (web/src/work/model.ts reads it).
const REVIEWED = "Added the retry and its test.\n\nIndependent review:\n" + JSON.stringify({
  passed: true, findings: [], files: ["src/http/client.ts", "src/http/client.spec.ts"], tests: ["pnpm test src/http"],
  summary: "The back-off is bounded and the test covers the third attempt.",
});

export function webSeed(now = Date.now()) {
  counter = 0;
  const tasks = [
    // Pull requests (all kept by the harness).
    task(now, { title: "Retry timed-out requests in the API client", status: "Completed", ago: 30 * HOUR, result: REVIEWED, pr: pullRequest("acme-api", 101, { state: "OPEN", review: "REVIEW_REQUIRED" }) }),
    task(now, { title: "Drop the unused logger import", status: "Completed", ago: 28 * HOUR, result: REVIEWED, pr: pullRequest("acme-api", 102, { state: "OPEN", checks: "failing", failing: ["Lint"], merge_state: "UNSTABLE" }) }),
    task(now, { title: "Cache the settings lookup", status: "Completed", ago: 26 * HOUR, result: REVIEWED, pr: pullRequest("acme-api", 103, { state: "OPEN", review: "APPROVED", merge_state: "CLEAN" }) }),
    task(now, { title: "Show the empty state on the orders page", status: "Completed", workspace: "ws-web", ago: 24 * HOUR, result: REVIEWED, pr: pullRequest("acme-web", 57, { state: "OPEN", merge_state: "BEHIND" }) }),
    task(now, { title: "Fix the date picker in Safari", status: "Completed", workspace: "ws-web", ago: 22 * HOUR, result: REVIEWED, pr: pullRequest("acme-web", 58, { state: "OPEN", checks: "pending", merge_state: "UNKNOWN" }) }),
    task(now, { title: "Paginate the audit log endpoint", status: "Completed", ago: 20 * HOUR, result: REVIEWED, pr: pullRequest("acme-api", 104, { state: "OPEN", review: "APPROVED", merge_state: "CLEAN" }) }),
    task(now, { title: "Rename the billing feature flag", status: "Completed", ago: 50 * HOUR, result: REVIEWED, pr: pullRequest("acme-api", 95, { state: "MERGED", review: "APPROVED", merge_state: "CLEAN" }) }),
    task(now, { title: "Trim whitespace in signup emails", status: "Completed", workspace: "ws-web", ago: 52 * HOUR, result: REVIEWED, pr: pullRequest("acme-web", 51, { state: "MERGED", review: "APPROVED", merge_state: "CLEAN" }) }),
    task(now, { title: "Upgrade the HTTP client", status: "Completed", ago: 54 * HOUR, result: REVIEWED, pr: pullRequest("acme-api", 93, { state: "MERGED", review: "APPROVED", merge_state: "CLEAN" }) }),
    task(now, { title: "Log slow database queries", status: "Completed", ago: 56 * HOUR, result: REVIEWED, pr: pullRequest("acme-api", 92, { state: "CLOSED", merge_state: "CLEAN" }) }),
    // Everything else; the harness keeps the 14 newest, so these are the newest.
    task(now, { title: "Handle a missing customer in the invoice job", status: "Completed", ago: 9 * HOUR, result: "Guarded the lookup and added a test." }),
    task(now, { title: "Explain the 502s on the health check", status: "Completed", ago: 8 * HOUR, result: "The load balancer timed out at 5 s while the check took 6 s; raised the check's limit." }),
    task(now, { title: "Stop the nightly export running twice", status: "Failed", ago: 7 * HOUR, result: "The run stopped: the export script needs credentials this machine does not have." }),
    task(now, { title: "Move the uploads bucket to the new region", status: "Cancelled", ago: 7 * HOUR }),
    task(now, { title: "Add a retry to the webhook sender", status: "ReadyForReview", ago: 6 * HOUR, result: REVIEWED, pr: pullRequest("acme-api", 105, { state: "OPEN" }) }),
    task(now, { title: "Speed up the search index rebuild", status: "Failed", workspace: "ws-web", ago: 6 * HOUR, result: "Its checks failed twice: the benchmark is flaky on this machine." }),
    task(now, { title: "Make the order list sortable by date", status: "AwaitingApproval", workspace: "ws-web", ago: 5 * HOUR }),
    task(now, { title: "Validate phone numbers on the profile form", status: "AwaitingApproval", workspace: "ws-web", ago: 4.5 * HOUR }),
    task(now, { title: "Fix the crash when a breadcrumb is circular", status: "AwaitingApproval", ago: 4 * HOUR }),
    task(now, { title: "Split the reports module", status: "AwaitingApproval", ago: 3.5 * HOUR }),
    task(now, { title: "Guard the missing client in the reports job", status: "AwaitingApproval", ago: 3 * HOUR }),
    task(now, { title: "Cover the reports job with a test", status: "AwaitingApproval", ago: 2.5 * HOUR }),
    task(now, { title: "Document the reports job's retries", status: "AwaitingApproval", ago: 2 * HOUR }),
    task(now, { title: "Add dark mode to the email preview", status: "AwaitingApproval", workspace: "ws-web", ago: 1.5 * HOUR }),
  ];

  const message = (role, text, ago) => ({
    agent_profile_revision: 0, agent_profile_id: "default", workspace_id: null, tool_calls: [], id: id("msg"),
    at_ms: now - ago, role, parts: [{ type: "text", text, state: "done" }], ticket_ids: [], pending: false, queued: false, failed: false, responsibility_ids: [],
  });
  const conversation = [
    message("user", "Which pull requests are waiting on something other than a review?", 40 * MINUTE),
    message("neko", TABLE_REPLY, 39 * MINUTE),
    message("user", "Add a retry to the API client's timeouts.", 20 * MINUTE),
    message("neko", FINAL_REPLY, 15 * MINUTE),
  ];

  // Two watches that filed some of the tasks (the agent tree groups them).
  // Kept apart (`fixture_mcp`: the harness ignores it, a full `mcp` would need
  // every field); work.mjs's richer pass serves them to the page.
  const watch = (name, instruction, enabled) => ({
    id: id("watch"), workspace_id: "ws-api", instruction, connection_ids: [], enabled, prepare_low_risk: false,
    next_due_ms: now + HOUR, last_attempt_ms: now - HOUR, last_result: "", failures: 0, name,
  });
  const watches = [watch("Production errors", "File a task for every new unresolved production error.", true), watch("Flaky tests", "File a task when a test fails twice on main.", true)];
  const filed = tasks.filter((t) => !t.pull_request).slice(0, 8);
  const sources = filed.map((t, i) => ({
    task_id: t.id, id: id("source"), responsibility_id: watches[i % 2].id, connection_id: "", external_id: `ACME-${200 + i}`,
    revision: "r1", title: t.title, description: "", retrieved_ms: t.created_at_ms, receipt_ids: [], eligible: true,
  }));

  // Each task's repository (the pull-request page and the land-on-main switch read these).
  const task_roots = Object.fromEntries(tasks.map((t) => [t.id, WORKSPACES.find((w) => w.id === t.workspace_id).repository]));

  return { task_roots, workspaces: WORKSPACES, connections: [], issues: [], tasks, heartbeat_ms: 0, conversation, fixture_mcp: { responsibilities: watches, sources } };
}
