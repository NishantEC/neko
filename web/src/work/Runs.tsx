// A ticket's runs, one line each: status glyph, "Run 3", files changed and
// +/-, from Neko's per-run checkpoints (hidden git refs; see
// docs/knowledge/checkpoints-and-undo.md). Expanding a line shows that run's
// diff only; "Whole task" shows the first start to the last end. "Undo this
// run" restores the folder to how it was before the run, asks first, and when
// the daemon refuses says why and offers the option that would work.
import { useCallback, useEffect, useMemo, useState } from "react";
import { command, request } from "@/api";
import { ArrowCounterClockwise, CaretRight, WarningCircle } from "@/ui/icons";
import { Button } from "@/ui/button";
import { Skeleton } from "@/ui/skeleton";
import { StatusGlyph, type StatusKind } from "@/ui/status-glyph";
import { toast } from "@/ui/sonner";
import { cn } from "@/ui/lib/utils";
import { MonoLabel } from "@/editorial";
import { ConfirmDialog, type ConfirmSpec } from "@/work/actions";
import { FileRow } from "@/work/Changes";
import { parseDiff } from "@/work/model";

interface RunLine {
  n: number;
  outcome: string;
  files: number;
  added: number;
  removed: number;
  undone: boolean;
  skipped: string[];
  skipped_count: number;
}
interface RunList { runs: RunLine[]; redo: string | null; unsaved: boolean; busy: boolean; restorable: boolean }
interface UndoOptions { run: number; through_later?: boolean; snapshot_unsaved?: boolean; revert?: boolean }
interface Refusal { code: string; message: string; options: UndoOptions }

const GLYPH: Record<string, StatusKind> = { running: "inProgress", done: "done", failed: "failed", cancelled: "cancelled" };
const OUTCOME_WORD: Record<string, string> = { running: "Running", done: "Done", failed: "Failed", cancelled: "Stopped" };

/** `checkpoint:<code>: <message>` is how the daemon says why it refused. */
function parseRefusal(error: unknown, options: UndoOptions): Refusal | null {
  const text = error instanceof Error ? error.message : String(error);
  const m = text.match(/^checkpoint:([a-z_]+): ([\s\S]*)$/);
  return m ? { code: m[1], message: m[2], options } : null;
}

function Diff({ taskId, run }: { taskId: string; run: number | null }) {
  const [patch, setPatch] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    let live = true;
    setPatch(null);
    setError(null);
    request({ CheckpointDiff: { task_id: taskId, run } })
      .then((reply) => { if (live) setPatch(reply.TaskChanges?.patch ?? ""); })
      .catch((e) => { if (live) setError(e instanceof Error ? e.message : String(e)); });
    return () => { live = false; };
  }, [taskId, run]);
  const files = useMemo(() => (patch ? parseDiff(patch) : []), [patch]);
  if (error) return <p role="alert" className="flex items-center gap-2 px-3 py-2 text-[12.5px] text-warning-foreground"><WarningCircle aria-hidden /> {error}</p>;
  if (patch === null) return <Skeleton className="m-3 h-5 w-48" />;
  if (files.length === 0) return <p className="px-3 py-2 text-[12.5px] text-muted-foreground">This run changed no files.</p>;
  return <div data-slot="run-diff">{files.map((f, i) => <FileRow key={f.path + i} file={f} defaultOpen={files.length <= 2} />)}</div>;
}

function Counts({ files, added, removed }: { files: number; added: number; removed: number }) {
  return (
    <span className="shrink-0 font-mono text-[11.5px] tabular-nums">
      <span className="text-muted-foreground">{files} {files === 1 ? "file" : "files"} · </span>
      <span className="text-success-foreground">+{added}</span> <span className="text-destructive-foreground">−{removed}</span>
    </span>
  );
}

export function Runs({ taskId, active, revision }: { taskId: string; active: boolean; revision: string }) {
  const [list, setList] = useState<RunList | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [open, setOpen] = useState<number | "whole" | null>(null);
  const [confirm, setConfirm] = useState<ConfirmSpec | null>(null);
  const [refusal, setRefusal] = useState<Refusal | null>(null);
  const [working, setWorking] = useState(false);

  const load = useCallback(async () => {
    try {
      const reply = (await request({ TaskCheckpoints: { task_id: taskId } })).TaskCheckpoints;
      setList({ runs: reply?.runs ?? [], redo: reply?.redo ?? null, unsaved: !!reply?.unsaved, busy: !!reply?.busy, restorable: !!reply?.restorable });
      setError(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }, [taskId]);

  useEffect(() => { setList(null); setOpen(null); setRefusal(null); }, [taskId]);
  useEffect(() => { if (active) void load(); }, [active, load, revision]);

  const act = useCallback(async (body: Record<string, unknown>, done: string) => {
    setWorking(true);
    setRefusal(null);
    try {
      await command("Checkpoints", body);
      toast.success(done);
      setOpen(null);
    } catch (e) {
      const options = (body.Undo ?? {}) as UndoOptions & { task_id?: string };
      const parsed = "Undo" in body ? parseRefusal(e, { run: options.run, through_later: options.through_later, snapshot_unsaved: options.snapshot_unsaved, revert: options.revert }) : null;
      if (parsed) setRefusal(parsed);
      else toast.error(e instanceof Error ? e.message : String(e));
    }
    setWorking(false);
    void load();
  }, [load]);

  const undo = useCallback((options: UndoOptions) => {
    const through = options.through_later;
    setConfirm({
      title: through ? `Undo back to run ${options.run}?` : `Undo run ${options.run}?`,
      description: options.revert
        ? `Adds a commit that reverts run ${options.run}${through ? " and the runs after it" : ""}. History is kept. A safety checkpoint is taken first, so you can redo it.`
        : `The folder goes back to how it was before run ${options.run} started${through ? ", which also undoes the runs after it" : ""}. A safety checkpoint is taken first, so you can redo it.`,
      confirm: through ? "Undo back to here" : "Undo this run",
      cancel: "Keep it",
      destructive: true,
      onConfirm: () => act({ Undo: { task_id: taskId, ...options } }, `Undid run ${options.run}.`),
    });
  }, [act, taskId]);

  if (error) return null;
  const runs = list?.runs ?? [];
  if (list && runs.length === 0) return null;
  const whole = runs.length > 1;
  const undoOff = !list?.restorable ? "This task's folder is parked or missing, so there is nothing to restore." : list?.busy ? "A run is working in this folder." : null;

  return (
    <section data-slot="runs" aria-label="Runs" className="mt-6">
      <div className="mb-2 flex items-center gap-2">
        <MonoLabel className="text-[10.5px]">Runs</MonoLabel>
        <span className="flex-1" />
        {list?.redo ? (
          <Button variant="outline" size="xs" disabled={working || !!undoOff} title={undoOff ?? undefined} data-slot="redo" onClick={() => void act({ Redo: { task_id: taskId } }, "Redone.")}>
            <ArrowCounterClockwise aria-hidden className="-scale-x-100" /> Redo {list.redo.replace(/^undo of /, "")}
          </Button>
        ) : null}
      </div>
      {!list ? (
        <Skeleton className="h-9 w-full rounded-2xl" />
      ) : (
        <div className="overflow-hidden rounded-2xl border border-border bg-card">
          {runs.map((r) => {
            const isOpen = open === r.n;
            const skipped = r.skipped_count > 0;
            return (
              <div key={r.n} data-slot="run" data-run={r.n} data-undone={r.undone || undefined} className="border-b border-border/70 last:border-b-0">
                <div className="flex min-h-10 items-center gap-2 px-3">
                  <button
                    type="button"
                    aria-expanded={isOpen}
                    onClick={() => setOpen(isOpen ? null : r.n)}
                    className="flex min-w-0 flex-1 items-center gap-2 py-2 text-start text-[12.5px] outline-none hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring"
                  >
                    <CaretRight size={11} weight="bold" className={cn("shrink-0 text-muted-foreground transition-transform", isOpen && "rotate-90")} aria-hidden />
                    <StatusGlyph status={GLYPH[r.outcome] ?? "backlog"} label={OUTCOME_WORD[r.outcome] ?? r.outcome} />
                    <span className={cn("font-medium", r.undone && "text-muted-foreground line-through")}>Run {r.n}</span>
                    {r.undone ? <span className="text-[11.5px] text-muted-foreground">undone</span> : null}
                    {r.outcome === "running" ? null : <Counts files={r.files} added={r.added} removed={r.removed} />}
                  </button>
                  {skipped ? <span className="shrink-0 text-[11.5px] text-warning-foreground" title={r.skipped.join("\n")}>{r.skipped_count} large {r.skipped_count === 1 ? "file" : "files"} not saved</span> : null}
                  {!r.undone && r.outcome !== "running" ? (
                    <Button
                      variant="ghost"
                      size="xs"
                      disabled={working || !!undoOff}
                      title={undoOff ?? undefined}
                      data-slot="undo-run"
                      onClick={() => undo({ run: r.n })}
                    >
                      <ArrowCounterClockwise aria-hidden /> Undo this run
                    </Button>
                  ) : null}
                </div>
                {isOpen ? <Diff taskId={taskId} run={r.n} /> : null}
              </div>
            );
          })}
          {whole ? (
            <div data-slot="run-whole" className="border-t border-border/70">
              <button
                type="button"
                aria-expanded={open === "whole"}
                onClick={() => setOpen(open === "whole" ? null : "whole")}
                className="flex min-h-10 w-full items-center gap-2 px-3 text-start text-[12.5px] outline-none hover:text-foreground focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring"
              >
                <CaretRight size={11} weight="bold" className={cn("shrink-0 text-muted-foreground transition-transform", open === "whole" && "rotate-90")} aria-hidden />
                <span className="font-medium">Whole task</span>
                <span className="text-muted-foreground">first start to last end</span>
              </button>
              {open === "whole" ? <Diff taskId={taskId} run={null} /> : null}
            </div>
          ) : null}
        </div>
      )}
      {refusal ? (
        <div role="alert" data-slot="undo-refused" data-code={refusal.code} className="mt-2 rounded-2xl border border-warning/40 bg-warning/10 px-4 py-3 text-[12.5px]">
          <p className="flex items-start gap-2"><WarningCircle className="mt-0.5 shrink-0" aria-hidden /> <span>{refusal.message}</span></p>
          <div className="mt-2 flex flex-wrap gap-2">
            {refusal.code === "later_runs" ? <Button size="xs" variant="outline" onClick={() => undo({ ...refusal.options, through_later: true })}>Undo back to here</Button> : null}
            {refusal.code === "unsaved_changes" ? <Button size="xs" variant="outline" onClick={() => undo({ ...refusal.options, snapshot_unsaved: true })}>Snapshot them first, then undo</Button> : null}
            {refusal.code === "pushed" || refusal.code === "foreign_commits" ? <Button size="xs" variant="outline" onClick={() => undo({ ...refusal.options, revert: true })}>Add a revert commit instead</Button> : null}
            <Button size="xs" variant="ghost" onClick={() => setRefusal(null)}>Dismiss</Button>
          </div>
        </div>
      ) : null}
      <ConfirmDialog spec={confirm} onClose={() => setConfirm(null)} />
    </section>
  );
}
