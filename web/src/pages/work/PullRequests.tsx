// #/prs: every pull request Neko opened, as one list of decisions: what is ready
// to merge, what its agent says to close, what needs your call and what is still
// being worked on (PullRequestsView.swift). Neko follows each one on GitHub
// (`neko_core::pull_requests`); merging and closing wait for your click.
import { useEffect, useMemo, useState } from "react";
import { useWorkbench } from "@/useStore";
import { registerCommands } from "@/shell/commands";
import { ArrowSquareOut, ArrowsClockwise, CheckCircle, Clock, DotsThree, GitPullRequest, Hammer, WarningCircle, XCircle, type Icon } from "@/ui/icons";
import { GitMerge, HandPalm } from "@/work/icons";
import { toast } from "@/ui/sonner";
import { Button } from "@/ui/button";
import { Panel } from "@/editorial";
import { Empty, EmptyDescription, EmptyHeader, EmptyMedia, EmptyTitle } from "@/ui/empty";
import {
  DropdownMenu, DropdownMenuCheckboxItem, DropdownMenuContent, DropdownMenuGroup, DropdownMenuItem, DropdownMenuLabel, DropdownMenuSeparator, DropdownMenuTrigger,
} from "@/ui/dropdown-menu";
import { cn } from "@/ui/lib/utils";
import {
  AlertDialog, AlertDialogAction, AlertDialogCancel, AlertDialogContent, AlertDialogDescription, AlertDialogFooter, AlertDialogHeader, AlertDialogTitle,
} from "@/ui/alert-dialog";

import { run } from "@/work/actions";
import { useDialogs } from "@/work/dialogs";
import { WorkHeader, WorkSkeleton } from "@/work/PageFrame";
import { StatusLabel, TONE_DOT, TONE_TEXT, type Tone } from "@/work/tone";
import {
  basename, pullRequestRows, pullRequestSubtitle, relative, VERDICTS, VERDICT_TITLE, type PullRequestRow, type Verdict,
} from "@/work/model";
import type { Ui } from "@/work/taskActions";

const VERDICT_TONE: Record<Verdict, Tone> = {
  ready: "done", merged: "done", close: "failed", needsYou: "attention", merging: "running", working: "running", closed: "neutral", unchecked: "neutral",
};
const VERDICT_ICON: Record<Verdict, Icon> = {
  ready: CheckCircle, close: XCircle, closed: XCircle, needsYou: HandPalm,
  merging: GitMerge, merged: GitMerge, working: Hammer, unchecked: Clock,
};
const PROBLEM_ICON = { checks: XCircle, conflicts: WarningCircle, behind: WarningCircle } as const;
const MAX_FOLLOW_UPS = 4;

const send = (name: string, row: PullRequestRow, done: string, extra: Record<string, unknown> = {}) =>
  run(name, { task_id: row.id, ...extra }, done);

function askToClose(row: PullRequestRow, ui: Ui) {
  ui.prompt({
    title: `Close ${row.number || "the pull request"}?`,
    description: "The comment goes on the pull request, so the next person knows why. Its task is cancelled once GitHub confirms.",
    label: "Why (posted as a comment)", initial: row.reason ?? "", multiline: true, confirm: "Close pull request", destructive: true,
    onSubmit: (comment) => send("ClosePullRequest", row, `Closing ${row.number}. Its task is cancelled once GitHub confirms.`, { comment }),
  });
}

function Row({ row, ui }: { row: PullRequestRow; ui: Ui }) {
  const Icon = VERDICT_ICON[row.verdict];
  const tone = VERDICT_TONE[row.verdict];
  const merge = () => void send("SetMergeWhenReady", row, `Merging ${row.number} once its checks pass.`, { enabled: true });
  const closeButton = (variant: "ghost" | "outline") => (
    <Button size="sm" variant={variant} aria-label={`Close ${row.number}…`} onClick={() => askToClose(row, ui)}>Close…</Button>
  );
  const mergeButton = (variant: "ghost" | "outline") => (
    <Button size="sm" variant={variant} aria-label={`Merge ${row.number}`} onClick={merge}>Merge</Button>
  );
  return (
    <li data-slot="pr-row" data-verdict={row.verdict} data-task-id={row.id} className="flex items-start gap-3 px-4 py-3">
      <Icon size={17} weight="bold" aria-label={VERDICT_TITLE[row.verdict]} role="img" className={cn("mt-0.5 shrink-0", TONE_TEXT[tone])} />
      <div className="min-w-0 flex-1">
        {/* The title opens the task; the actions stay sibling buttons, never nested inside it. */}
        <a
          href={`#/tasks?task=${row.id}`}
          title={`Open the task for ${row.number || "this pull request"}`}
          className="line-clamp-2 rounded-sm font-medium leading-[1.35] text-strong outline-none hover:underline focus-visible:ring-2 focus-visible:ring-ring"
        >
          {row.title}
        </a>
        {row.meta ? <p className="mt-0.5 truncate text-[12.5px] text-muted-foreground">{row.meta}</p> : null}
        {row.problems.length || row.followUps > 0 ? (
          <div className="mt-1.5 flex flex-wrap items-center gap-1.5">
            {row.problems.map((p) => {
              const P = PROBLEM_ICON[p.kind];
              return <StatusLabel key={p.text} tone="attention" icon={<P weight="bold" aria-hidden />}>{p.text}</StatusLabel>;
            })}
            {row.followUps > 0 ? (
              <span
                data-slot="follow-ups"
                title="How many times Neko has handed a failure, conflict or comment back to the task’s agent. After four it waits for you."
                className="inline-flex h-5 items-center gap-1 rounded-md bg-foreground/[0.06] px-1.5 text-[11.5px] text-muted-foreground"
              >
                <ArrowsClockwise size={11} aria-hidden /> Handed back {Math.min(row.followUps, MAX_FOLLOW_UPS)} of {MAX_FOLLOW_UPS}
              </span>
            ) : null}
          </div>
        ) : null}
        {row.reason ? <p className="mt-1.5 line-clamp-3 text-[12.5px] leading-[1.5] text-foreground/80" title={row.reason}>{row.reason}</p> : null}
      </div>
      <time className="hidden w-20 shrink-0 pt-0.5 text-end text-[12px] tabular-nums text-muted-foreground sm:block">{relative(row.updated)}</time>
      <div className="flex shrink-0 items-center gap-1.5">
        {row.verdict === "ready" ? <>{closeButton("ghost")}{mergeButton("outline")}</> : null}
        {row.verdict === "close" ? <>{mergeButton("ghost")}{closeButton("outline")}</> : null}
        {row.verdict === "merging" ? (
          <Button size="sm" variant="outline" aria-label={`Cancel merging ${row.number}`} onClick={() => void send("SetMergeWhenReady", row, `Won't merge ${row.number}.`, { enabled: false })}>Cancel merge</Button>
        ) : null}
        {["needsYou", "working", "unchecked", "merged", "closed"].includes(row.verdict) ? (
          <Button size="sm" variant="outline" aria-label={`Open task for ${row.number}`} asChild><a href={`#/tasks?task=${row.id}`}>Open task</a></Button>
        ) : null}
        <Button size="icon-sm" variant="ghost" aria-label={`Open ${row.number} on GitHub`} title={`Open ${row.number} on GitHub`} asChild>
          <a href={row.url} target="_blank" rel="noreferrer"><ArrowSquareOut /></a>
        </Button>
      </div>
    </li>
  );
}

export default function PullRequests() {
  const { snapshot, connected, error } = useWorkbench();
  const { ui, dialogs } = useDialogs();
  const [showFinished, setShowFinished] = useState(false);
  const [confirm, setConfirm] = useState<"recheck" | "merge" | null>(null);

  const tasks = snapshot?.tasks;
  const rows = useMemo(() => (snapshot ? pullRequestRows(snapshot, tasks ?? []) : []), [snapshot, tasks]);
  const ready = rows.filter((r) => r.verdict === "ready");
  const reviewable = rows.filter((r) => r.verdict === "ready" || r.verdict === "close").length;
  const repositories = useMemo(() => {
    const allowed = new Set<string>(snapshot?.merge_without_review ?? []);
    const paths = [...new Set(rows.map((r) => r.repositoryPath).filter(Boolean))].sort();
    return paths.map((path) => ({ path, name: basename(path), allowed: allowed.has(path) }));
  }, [rows, snapshot?.merge_without_review]);

  const grouped = useMemo(() => {
    const out = new Map<Verdict, PullRequestRow[]>();
    for (const row of rows) out.set(row.verdict, [...(out.get(row.verdict) ?? []), row]);
    return out;
  }, [rows]);
  const shown = VERDICTS.filter((v) => (grouped.get(v)?.length ?? 0) > 0 && (showFinished || (v !== "merged" && v !== "closed")));
  const hiddenFinished = (grouped.get("merged")?.length ?? 0) + (grouped.get("closed")?.length ?? 0);

  const recheck = () =>
    void run("ReviewPullRequests", { workspace_id: null }, "Each agent is re-checking its pull request. They run a few at a time, so this takes a while.");
  const mergeAll = async () => {
    for (const row of ready) await run("SetMergeWhenReady", { task_id: row.id, enabled: true });
    toast.success("Neko merges each one as soon as its checks pass.");
  };

  useEffect(
    () =>
      registerCommands([
        { id: "prs.check", title: "Check GitHub for pull request changes now", keywords: ["refresh", "pull requests"], icon: ArrowsClockwise, group: "Pull requests", run: () => void run("CheckPullRequests", undefined, "Checking GitHub.") },
        ...(ready.length ? [{ id: "prs.merge", title: `Merge ${ready.length} ready pull requests`, keywords: ["merge", "ready"], icon: GitPullRequest, group: "Pull requests", run: () => setConfirm("merge") }] : []),
        ...(reviewable ? [{ id: "prs.recheck", title: "Re-check all open pull requests", keywords: ["review", "agents"], icon: ArrowsClockwise, group: "Pull requests", run: () => setConfirm("recheck") }] : []),
      ]),
    [ready.length, reviewable],
  );

  return (
    <div data-slot="prs-page" className="flex min-h-0 flex-1 flex-col overflow-y-auto">
      <WorkHeader
        className="w-full max-w-[1088px]"
        eyebrow="Work"
        title={<>Pull <em>requests.</em></>}
        subtitle={snapshot ? pullRequestSubtitle(rows) : "Loading…"}
        actions={
          <>
            <Button variant="outline" size="sm" disabled={reviewable === 0} onClick={() => setConfirm("recheck")} title="Have each agent re-check its open pull request and give a verdict">
              Re-check all
            </Button>
            {ready.length ? (
              <Button size="sm" onClick={() => setConfirm("merge")} title="Merge every pull request in Ready to merge once its checks pass">
                Merge {ready.length} ready
              </Button>
            ) : null}
            <DropdownMenu>
              <DropdownMenuTrigger render={<Button variant="ghost" size="icon-sm" aria-label="Pull request options" />}><DotsThree weight="bold" /></DropdownMenuTrigger>
              <DropdownMenuContent align="end" className="min-w-64">
                <DropdownMenuItem onClick={() => void run("CheckPullRequests", undefined, "Checking GitHub.")}>Check GitHub now</DropdownMenuItem>
                <DropdownMenuCheckboxItem checked={showFinished} onCheckedChange={setShowFinished}>Show merged and closed</DropdownMenuCheckboxItem>
                {repositories.length ? (
                  <>
                    <DropdownMenuSeparator />
                    <DropdownMenuGroup>
                      <DropdownMenuLabel>Merge without an approving review</DropdownMenuLabel>
                      {repositories.map((repo) => (
                        <DropdownMenuCheckboxItem
                          key={repo.path}
                          checked={repo.allowed}
                          onCheckedChange={(enabled) => void run("SetMergeWithoutReview", { repository: repo.path, enabled })}
                        >
                          {repo.name}
                        </DropdownMenuCheckboxItem>
                      ))}
                    </DropdownMenuGroup>
                  </>
                ) : null}
              </DropdownMenuContent>
            </DropdownMenu>
          </>
        }
      >
        <p className="mt-3 max-w-[72ch] text-[12.5px] leading-[1.55] text-muted-foreground">
          Neko checks each pull request on GitHub every 3 minutes. Failing checks, conflicts and review comments go back to the task’s agent; merging and closing wait for you.
        </p>
      </WorkHeader>

      {!snapshot ? (
        error ? (
          <Empty className="border-0"><EmptyHeader><EmptyTitle>Can’t reach the Neko daemon</EmptyTitle><EmptyDescription>{error}</EmptyDescription></EmptyHeader></Empty>
        ) : <WorkSkeleton rows={6} />
      ) : rows.length === 0 ? (
        <Empty className="border-0">
          <EmptyHeader>
            <EmptyMedia variant="icon"><GitPullRequest /></EmptyMedia>
            <EmptyTitle>No pull requests yet</EmptyTitle>
            <EmptyDescription>When a task’s work passes its independent review, Neko opens a pull request and it appears here.</EmptyDescription>
          </EmptyHeader>
        </Empty>
      ) : (
        <div className="w-full max-w-[1040px] px-6 pb-12">
          {!connected ? <p role="status" className="mb-3 text-[13px] text-destructive">Can’t reach the Neko daemon. Showing the last known state.</p> : null}
          {shown.length === 0 ? (
            <p className="py-10 text-center text-[13px] text-muted-foreground">Nothing is open. {hiddenFinished} merged or closed.</p>
          ) : null}
          {shown.map((verdict, i) => {
            const items = grouped.get(verdict) ?? [];
            const tone = VERDICT_TONE[verdict];
            return (
              <section key={verdict} data-slot="pr-group" data-verdict={verdict} aria-label={VERDICT_TITLE[verdict]} className={cn(i > 0 && "mt-7")}>
                <div className="mb-2 flex items-center gap-2 px-1">
                  <span aria-hidden className={cn("size-2 rounded-full", TONE_DOT[tone])} />
                  <h2 className="text-[13px] font-semibold text-strong">{VERDICT_TITLE[verdict]}</h2>
                  <span className="text-[12.5px] tabular-nums text-muted-foreground">{items.length}</span>
                </div>
                <Panel>
                  <ul className="divide-y divide-border/70">
                    {items.map((row) => <Row key={row.id} row={row} ui={ui} />)}
                  </ul>
                </Panel>
              </section>
            );
          })}
          {!showFinished && hiddenFinished > 0 ? (
            <div className="mt-6 text-center">
              <Button variant="ghost" size="sm" className="text-muted-foreground" onClick={() => setShowFinished(true)}>Show {hiddenFinished} merged and closed</Button>
            </div>
          ) : null}
        </div>
      )}

      {dialogs}
      <ConfirmRecheck open={confirm === "recheck"} count={reviewable} onClose={() => setConfirm(null)} onConfirm={recheck} />
      <ConfirmMerge open={confirm === "merge"} count={ready.length} onClose={() => setConfirm(null)} onConfirm={() => void mergeAll()} />
    </div>
  );
}

function ConfirmRecheck({ open, count, onClose, onConfirm }: { open: boolean; count: number; onClose: () => void; onConfirm: () => void }) {
  return (
    <AlertDialog open={open} onOpenChange={(o) => { if (!o) onClose(); }}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>Re-check {count} open pull {count === 1 ? "request" : "requests"}?</AlertDialogTitle>
          <AlertDialogDescription>
            Each task’s agent updates its pull request with main, re-checks the cause against the evidence, cuts it to the smallest fix, and says whether it is ready, should be closed, or needs your decision. Agents run a few at a time per repository.
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel>Cancel</AlertDialogCancel>
          <AlertDialogAction onClick={onConfirm}>Re-check them</AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}

function ConfirmMerge({ open, count, onClose, onConfirm }: { open: boolean; count: number; onClose: () => void; onConfirm: () => void }) {
  return (
    <AlertDialog open={open} onOpenChange={(o) => { if (!o) onClose(); }}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>Merge {count} pull {count === 1 ? "request" : "requests"}?</AlertDialogTitle>
          <AlertDialogDescription>
            Each is squashed with an empty body once its checks pass and nothing conflicts. A failure or a conflict stops that one and tells you.
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel>Cancel</AlertDialogCancel>
          <AlertDialogAction onClick={onConfirm}>Merge {count}</AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
