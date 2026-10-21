// PR board: every task that has a pull request, grouped by task status.
//
// Install by copying this folder to `<data dir>/plugins/pr-board/`. It uses only
// what the host hands every plugin: React and `request`, the same function the
// app itself talks to the daemon with. No build step: edit this file and the
// page picks it up within a few seconds.

const GROUPS = [
  ["ReadyForReview", "Ready to review"],
  ["AwaitingApproval", "Waiting for you"],
  ["Building", "Building"],
  ["Reviewing", "Being reviewed"],
  ["Failed", "Failed"],
  ["Completed", "Completed"],
  ["Cancelled", "Cancelled"],
];

export default function register(host) {
  const { React, request } = host;
  const h = React.createElement;

  // One held request: the daemon answers when the workbench changes, so the
  // board is live without polling.
  function useTasks() {
    const [tasks, setTasks] = React.useState(null);
    const [error, setError] = React.useState(null);
    React.useEffect(() => {
      let stop = false;
      (async () => {
        let digest = "";
        while (!stop) {
          try {
            const answer = await request(
              digest ? { WorkbenchIfChanged: { digest, wait_ms: 20000 } } : { Workbench: "Snapshot" },
            );
            if (answer.Workbench) {
              digest = answer.Workbench.digest;
              setTasks(answer.Workbench.tasks.filter((t) => t.pull_request));
              setError(null);
            }
          } catch (e) {
            setError(e.message);
            await new Promise((r) => setTimeout(r, 2000));
          }
        }
      })();
      return () => { stop = true; };
    }, []);
    return { tasks, error };
  }

  const pill = (text, tone) => h("span", { className: "pill", "data-tone": tone }, text);

  function Card({ task }) {
    const pr = task.pull_request;
    const status = pr.status || {};
    const number = pr.url.split("/").pop();
    const repo = pr.url.split("/").slice(-3, -2)[0];
    return h("a", { className: "pr-card", href: pr.url, target: "_blank", rel: "noreferrer noopener" },
      h("div", { className: "pr-title" }, task.display_title || task.title),
      h("div", { className: "pr-meta" },
        h("span", { className: "pr-ref" }, `${repo} #${number}`),
        status.state && pill(status.state.toLowerCase(), status.state === "MERGED" ? "done" : status.state === "CLOSED" ? "bad" : "open"),
        status.checks && pill(`checks ${status.checks}`, status.checks === "passing" ? "ok" : status.checks === "failing" ? "bad" : "pending"),
        status.review && pill(status.review.toLowerCase().replace(/_/g, " "), status.review === "APPROVED" ? "ok" : "pending"),
      ),
    );
  }

  function Board() {
    const { tasks, error } = useTasks();
    if (!tasks) return h("div", { className: "pr-board" }, h("p", { className: "muted" }, error || "Loading…"));
    const known = new Set(GROUPS.map(([s]) => s));
    const groups = [...GROUPS, ["", "Other"]]
      .map(([status, label]) => [label, tasks.filter((t) => (status ? t.status === status : !known.has(t.status)))])
      .filter(([, list]) => list.length);
    return h("div", { className: "pr-board" },
      h("style", null, CSS),
      h("header", null, h("h1", null, "PR board"), h("span", { className: "muted" }, `${tasks.length} pull requests`)),
      groups.length === 0 && h("p", { className: "muted" }, "No task has a pull request yet."),
      groups.map(([label, list]) =>
        h("section", { key: label },
          h("h2", null, label, h("span", { className: "count" }, list.length)),
          h("div", { className: "pr-list" }, list.map((t) => h(Card, { key: t.id, task: t }))),
        )),
    );
  }

  host.registerPage({ id: "board", title: "PR board", component: Board });
}

const CSS = `
.pr-board { flex: 1; overflow-y: auto; width: 100%; max-width: 728px; margin: 0 auto; padding: 28px 24px 48px; }
.pr-board header { display: flex; align-items: baseline; gap: 12px; margin-bottom: 8px; }
.pr-board h1 { font-size: 20px; font-weight: 600; margin: 0; color: rgb(255 255 255 / .97); }
.pr-board h2 { font-size: 12px; font-weight: 600; letter-spacing: .04em; text-transform: uppercase; color: #8f8f8f; margin: 26px 0 8px; display: flex; gap: 8px; align-items: center; }
.pr-board .count { font-weight: 400; color: #6a6a6a; }
.pr-list { border-top: 1px solid rgb(255 255 255 / .06); }
.pr-card { display: block; padding: 10px 4px; border-bottom: 1px solid rgb(255 255 255 / .06); color: inherit; text-decoration: none; }
.pr-card:hover { background: rgb(255 255 255 / .03); }
.pr-title { color: rgb(255 255 255 / .92); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.pr-meta { display: flex; gap: 8px; align-items: center; margin-top: 2px; font-size: 12px; color: #8f8f8f; }
.pr-ref { font-family: ui-monospace, Menlo, monospace; }
.pill { padding: 0 8px; line-height: 18px; border-radius: 999px; background: rgb(255 255 255 / .06); }
.pill[data-tone="ok"], .pill[data-tone="done"] { color: #4cc38a; }
.pill[data-tone="bad"] { color: #e5484d; }
.pill[data-tone="pending"], .pill[data-tone="open"] { color: #e0a042; }
`;
