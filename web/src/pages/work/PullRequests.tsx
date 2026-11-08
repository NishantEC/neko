// #/prs: every pull request Neko opened, as one list of decisions: what is ready
// to merge, what its agent says to close, what needs your call and what is still
// being worked on (PullRequestsView.swift). Neko follows each one on GitHub
// (`neko_core::pull_requests`); merging and closing wait for your click.
import { useEffect, useMemo, useState, type ReactNode } from "react";
import { useWorkbench } from "@/useStore";
import { useSelectedWorkspace } from "@/shell/workspace";
import { taskHref } from "@/shell/status";
import { registerCommands } from "@/shell/commands";
import { ArrowSquareOut, ArrowsClockwise, CaretDown, Copy, DotsThree, GitPullRequest, ListChecks, Prohibit, XCircle } from "@/ui/icons";
import { GitMerge } from "@/work/icons";
import { toast } from "@/ui/sonner";
import { Button } from "@/ui/button";
import { GROUP_HEADING, Panel, RowAction, RowActions } from "@/setup/kit";
import { GlyphButton, StatusGlyph, type StatusKind } from "@/ui/status-glyph";
import { copyText, useContextMenu } from "@/shell/context-menu";
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
import { PageContainer } from "@/shell/PageContainer";
import { WorkHeader, WorkSkeleton } from "@/work/PageFrame";
import {
  basename, pullRequestRows, pullRequestSubtitle, relative, VERDICTS, VERDICT_TITLE, type PullRequestRow, type Verdict,
} from "@/work/model";
import type { Ui } from "@/work/taskActions";

/** The one status language (DESIGN.md §3): sky = ready, amber = needs you or in progress, coral = close. */
const VERDICT_STATUS: Record<Verdict, StatusKind> = {
  ready: "review", merged: "done", close: "failed", needsYou: "inProgress", merging: "inProgress", working: "inProgress", closed: "cancelled", unchecked: "todo",
};
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

/**
 * One pull request on one line (DESIGN.md §8): the verdict glyph (on a ready
 * row, click it to merge), the title (opens the task), `#num · repo` and the
 * time. What needs a look (failing checks, conflicts, behind main, the
 * agent's reason, hand-backs) is one faint line under it. Merge, Close…,
 * Cancel merge, Open task and GitHub appear on hover and in the right-click menu.
 */
function Row({ row, ui }: { row: PullRequestRow; ui: Ui }) {
  const status = VERDICT_STATUS[row.verdict];
  const merge = () => void send("SetMergeWhenReady", row, `Merging ${row.number} once its checks pass.`, { enabled: true });
  const cancelMerge = () => void send("SetMergeWhenReady", row, `Won't merge ${row.number}.`, { enabled: false });
  const canMerge = row.verdict === "ready" || row.verdict === "close";
  const canClose = row.verdict === "ready" || row.verdict === "close";
  const menu = useContextMenu(() => [
    canMerge ? { label: "Merge", icon: GitMerge, run: merge } : null,
    row.verdict === "merging" ? { label: "Cancel merge", icon: Prohibit, run: cancelMerge } : null,
    { label: "Open task", icon: ListChecks, run: () => { location.hash = taskHref(row.id).replace(/^#/, ""); } },
    { label: "Open on GitHub", icon: ArrowSquareOut, run: () => void window.open(row.url, "_blank", "noreferrer") },
    "separator",
    { label: "Copy link", icon: Copy, run: () => void copyText(row.url, "Copied the link") },
    canClose ? "separator" : null,
    canClose ? { label: "Close…", icon: XCircle, destructive: true, run: () => askToClose(row, ui) } : null,
  ]);
  const where = [row.number || null, row.repository].filter(Boolean).join(" · ");
  const notes: ReactNode[] = [
    ...row.problems.map((p) => <span key={p.text} className={p.kind === "behind" ? undefined : "text-warning-foreground"}>{p.text}</span>),
    row.draft ? <span key="draft">Draft</span> : null,
    row.followUps > 0 ? (
      <span
        key="follow-ups"
        data-slot="follow-ups"
        title="How many times Neko has handed a failure, conflict or comment back to the task’s agent. After four it waits for you."
      >
        Handed back {Math.min(row.followUps, MAX_FOLLOW_UPS)} of {MAX_FOLLOW_UPS}
      </span>
    ) : null,
    row.reason ? <span key="reason" className="text-foreground/75">{row.reason}</span> : null,
  ].filter(Boolean);
  return (
    <li
      {...menu}
      data-slot="pr-row"
      data-verdict={row.verdict}
      data-task-id={row.id}
      className="group/row relative flex items-start gap-3 rounded-[10px] px-3 py-2 transition-colors hover:bg-foreground/[0.03]"
    >
      <span className="flex h-5 items-center">
        {row.verdict === "ready" ? (
          <GlyphButton status={status} label={`Merge ${row.number || "this pull request"} once its checks pass`} onClick={merge} />
        ) : (
          <StatusGlyph status={status} label={VERDICT_TITLE[row.verdict]} />
        )}
      </span>
      <div className="flex min-w-0 flex-1 flex-col gap-0.5">
        <div className="flex min-w-0 items-baseline gap-3">
          {/* The title opens the task; the actions stay sibling buttons, never nested inside it. */}
          <a
            href={taskHref(row.id)}
            title={row.meta || `Open the task for ${row.number || "this pull request"}`}
            className="min-w-0 truncate rounded-sm text-[13.5px] text-foreground outline-none hover:underline focus-visible:ring-2 focus-visible:ring-ring"
          >
            {row.title}
          </a>
          <span className="ms-auto shrink-0 text-[12.5px] tabular-nums text-muted-foreground">{where}</span>
        </div>
        {notes.length ? (
          <p className="truncate text-[12.5px] text-muted-foreground [&>span+span]:before:mx-1.5 [&>span+span]:before:text-muted-foreground/60 [&>span+span]:before:content-['·']" title={row.reason ?? undefined}>
            {notes}
          </p>
        ) : null}
      </div>
      <time className="flex h-5 w-[4.5rem] shrink-0 items-center justify-end text-[12.5px] tabular-nums text-muted-foreground/70">{relative(row.updated)}</time>
      {/* Over the meta and time while the row is hovered or focused, so nothing reserves room for them. */}
      <span className="absolute end-0 top-0 flex h-9 items-center rounded-e-[10px] bg-background opacity-0 transition-opacity duration-150 group-hover/row:opacity-100 group-focus-within/row:opacity-100 [@media(hover:none)]:static [@media(hover:none)]:opacity-100">
        <RowActions className="h-full rounded-e-[10px] bg-foreground/[0.03] ps-3 pe-2 opacity-100">
          {canMerge ? <RowAction label={`Merge ${row.number}`} icon={GitMerge} onClick={merge} /> : null}
          {row.verdict === "merging" ? <RowAction label={`Cancel merging ${row.number}`} icon={Prohibit} onClick={cancelMerge} /> : null}
          {canClose ? <RowAction label={`Close ${row.number}…`} icon={XCircle} onClick={() => askToClose(row, ui)} /> : null}
          {!canMerge ? <RowAction label={`Open task for ${row.number}`} icon={ListChecks} onClick={() => { location.hash = taskHref(row.id).replace(/^#/, ""); }} /> : null}
          <RowAction label={`Open ${row.number} on GitHub`} icon={ArrowSquareOut} href={row.url} />
        </RowActions>
      </span>
    </li>
  );
}

/** A verdict group: a quiet heading with its count that folds, then its rows. */
function Group({ verdict, count, children }: { verdict: Verdict; count: number; children: ReactNode }) {
  const [open, setOpen] = useState(true);
  return (
    <section data-slot="pr-group" data-verdict={verdict} aria-label={VERDICT_TITLE[verdict]} className="flex flex-col gap-1">
      <h2 className={GROUP_HEADING}>
        <button
          type="button"
          aria-expanded={open}
          onClick={() => setOpen(!open)}
          className="-ms-1 flex items-center gap-1.5 rounded-md px-1 py-0.5 outline-none transition-colors hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring/60"
        >
          <CaretDown aria-hidden size={11} weight="bold" className={cn("transition-transform duration-150", !open && "-rotate-90")} />
          {VERDICT_TITLE[verdict]}
          <span className="tabular-nums text-muted-foreground/70">{count}</span>
        </button>
      </h2>
      {open ? <Panel><ul className="flex flex-col">{children}</ul></Panel> : null}
    </section>
  );
}

export default function PullRequests() {
  const { snapshot, connected, error } = useWorkbench();
  const { ui, dialogs } = useDialogs();
  const [showFinished, setShowFinished] = useState(false);
  const [confirm, setConfirm] = useState<"recheck" | "merge" | null>(null);

  const { id: workspaceId } = useSelectedWorkspace();
  const tasks = snapshot?.tasks;
  // The sidebar's workspace switcher scopes the board; "" is every workspace.
  const rows = useMemo(
    () => (snapshot ? pullRequestRows(snapshot, (tasks ?? []).filter((t: any) => !workspaceId || t.workspace_id === workspaceId)) : []),
    [snapshot, tasks, workspaceId],
  );
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
    <PageContainer data-slot="prs-page" className="max-w-[880px]">
      <WorkHeader
        className="px-0 pt-0 md:px-0"
        eyebrow="Work"
        title={<>Pull requests</>}
        count={snapshot ? pullRequestSubtitle(rows) : undefined}
        actions={
          <>
            {ready.length ? (
              <Button size="sm" onClick={() => setConfirm("merge")} title="Merge every pull request in Ready to merge once its checks pass">
                Merge {ready.length} ready
              </Button>
            ) : null}
            <DropdownMenu>
              <DropdownMenuTrigger render={<Button variant="ghost" size="icon-sm" aria-label="Pull request options" />}><DotsThree weight="bold" /></DropdownMenuTrigger>
              <DropdownMenuContent align="end" className="min-w-64">
                <DropdownMenuItem disabled={reviewable === 0} onClick={() => setConfirm("recheck")}>
                  <ArrowsClockwise /> Re-check all
                </DropdownMenuItem>
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
                <DropdownMenuSeparator />
                <p className="max-w-64 px-2 py-1.5 text-[12px] leading-snug text-muted-foreground">
                  Neko checks GitHub every 3 minutes and hands failing checks, conflicts and review comments back to the task’s agent. Merging and closing wait for you.
                </p>
              </DropdownMenuContent>
            </DropdownMenu>
          </>
        }
      />

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
        <div className="w-full">
          {!connected ? <p role="status" className="mb-3 text-[13px] text-destructive">Can’t reach the Neko daemon. Showing the last known state.</p> : null}
          {shown.length === 0 ? (
            <p className="py-10 text-center text-[13px] text-muted-foreground">Nothing is open. {hiddenFinished} merged or closed.</p>
          ) : null}
          <div className="flex flex-col gap-7">
            {shown.map((verdict) => {
              const items = grouped.get(verdict) ?? [];
              return (
                <Group key={verdict} verdict={verdict} count={items.length}>
                  {items.map((row) => <Row key={row.id} row={row} ui={ui} />)}
                </Group>
              );
            })}
          </div>
          {!showFinished && hiddenFinished > 0 ? (
            <button type="button" className="mt-6 rounded-md text-[12.5px] text-muted-foreground outline-none transition-colors hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring/60" onClick={() => setShowFinished(true)}>
              Show {hiddenFinished} merged and closed
            </button>
          ) : null}
        </div>
      )}

      {dialogs}
      <ConfirmRecheck open={confirm === "recheck"} count={reviewable} onClose={() => setConfirm(null)} onConfirm={recheck} />
      <ConfirmMerge open={confirm === "merge"} count={ready.length} onClose={() => setConfirm(null)} onConfirm={() => void mergeAll()} />
    </PageContainer>
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
