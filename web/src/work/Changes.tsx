// What a ticket changed in its own worktree: the `TaskChanges` request, shown
// as a file list with +/- totals and a unified diff per file (TicketFilesChanged
// and NativeDiffBlock in the SwiftUI app). Loads after the page, at a fixed
// height, so the conversation never jumps under the reader.
import { memo, useCallback, useEffect, useMemo, useState } from "react";
import { request } from "@/api";
import { ArrowClockwise, CaretRight, File, WarningCircle } from "@/ui/icons";
import { Button } from "@/ui/button";
import { Skeleton } from "@/ui/skeleton";
import { cn } from "@/ui/lib/utils";
import { MonoLabel } from "@/editorial";
import { Block } from "@/work/Thread";
import { parseDiff, type DiffFile } from "@/work/model";

interface Changes { files: string[]; patch: string; truncated: boolean }

const LINE_CAP = 400;

const LINE_STYLE = {
  add: "bg-success/10",
  del: "bg-destructive/10",
  hunk: "bg-info/10 text-info-foreground",
  ctx: "",
} as const;

const DiffBody = memo(function DiffBody({ file }: { file: DiffFile }) {
  const [all, setAll] = useState(false);
  const lines = all ? file.lines : file.lines.slice(0, LINE_CAP);
  return (
    <div data-slot="diff" className="overflow-x-auto border-t border-border/70 bg-background font-mono text-[11.5px] leading-[1.55]">
      <table className="w-full border-collapse">
        <tbody>
          {lines.map((line, i) => (
            <tr key={i} data-kind={line.kind} className={LINE_STYLE[line.kind]}>
              <td aria-hidden className="w-10 select-none px-2 text-end tabular-nums text-muted-foreground/50">{line.oldNo ?? ""}</td>
              <td aria-hidden className="w-10 select-none px-2 text-end tabular-nums text-muted-foreground/50">{line.newNo ?? ""}</td>
              <td className={cn("w-4 select-none text-center", line.kind === "add" && "text-success-foreground", line.kind === "del" && "text-destructive-foreground")}>
                {line.kind === "add" ? "+" : line.kind === "del" ? "−" : ""}
              </td>
              <td className="whitespace-pre pe-4">{line.text || " "}</td>
            </tr>
          ))}
        </tbody>
      </table>
      {!all && file.lines.length > LINE_CAP ? (
        <div className="border-t border-border/70 p-2">
          <Button variant="ghost" size="xs" onClick={() => setAll(true)}>Show the other {file.lines.length - LINE_CAP} lines</Button>
        </div>
      ) : null}
    </div>
  );
});

function FileRow({ file, defaultOpen }: { file: DiffFile; defaultOpen: boolean }) {
  const [open, setOpen] = useState(defaultOpen);
  return (
    <div data-slot="changed-file" className="border-b border-border/70 last:border-b-0">
      <button
        type="button"
        onClick={() => setOpen(!open)}
        aria-expanded={open}
        className="flex min-h-9 w-full items-center gap-2 px-3 text-start text-[12.5px] outline-none hover:bg-foreground/[0.03] focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring"
      >
        <CaretRight size={11} weight="bold" className={cn("shrink-0 text-muted-foreground transition-transform", open && "rotate-90")} aria-hidden />
        <File size={13} className="shrink-0 text-muted-foreground" aria-hidden />
        <span className="min-w-0 flex-1 truncate font-mono text-[12px]" title={file.path}>{file.path}</span>
        <span className="shrink-0 font-mono text-[11.5px] tabular-nums text-success-foreground">+{file.added}</span>
        <span className="shrink-0 font-mono text-[11.5px] tabular-nums text-destructive-foreground">−{file.removed}</span>
      </button>
      {open ? <DiffBody file={file} /> : null}
    </div>
  );
}

export function Changes({ taskId, active }: { taskId: string; active: boolean }) {
  const [changes, setChanges] = useState<Changes | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const reply = (await request({ TaskChanges: { task_id: taskId } })).TaskChanges;
      setChanges({ files: reply?.files ?? [], patch: reply?.patch ?? "", truncated: !!reply?.truncated });
      setError(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
    setLoading(false);
  }, [taskId]);

  useEffect(() => {
    setChanges(null);
    if (active) void load();
  }, [taskId, active, load]);

  const files = useMemo(() => (changes ? parseDiff(changes.patch) : []), [changes]);
  const added = files.reduce((n, f) => n + f.added, 0);
  const removed = files.reduce((n, f) => n + f.removed, 0);
  // Files the patch does not describe (binary, renamed) still count as changed.
  const total = Math.max(files.length, changes?.files.length ?? 0);

  return (
    <section data-slot="changes" aria-label="Files changed" className="mt-6">
      <div className="mb-2 flex items-center gap-2">
        <MonoLabel className="text-[10.5px]">Files changed</MonoLabel>
        {changes && total > 0 ? (
          <span className="font-mono text-[11.5px] tabular-nums">
            <span className="text-muted-foreground">{total} {total === 1 ? "file" : "files"} · </span>
            <span className="text-success-foreground">+{added}</span> <span className="text-destructive-foreground">−{removed}</span>
          </span>
        ) : null}
        <span className="flex-1" />
        <Button variant="ghost" size="icon-xs" aria-label="Reload changes" title="Reload changes" disabled={loading} onClick={() => void load()}>
          <ArrowClockwise className={loading ? "animate-spin motion-reduce:animate-none" : undefined} />
        </Button>
      </div>
      {error ? (
        <p role="alert" className="flex items-center gap-2 text-[12.5px] text-warning-foreground"><WarningCircle aria-hidden /> {error}</p>
      ) : !changes ? (
        <Block className="py-0"><Skeleton className="my-3 h-5 w-48" /></Block>
      ) : total === 0 ? (
        <Block className="py-3 text-[13px] text-muted-foreground">No changes in the worktree yet.</Block>
      ) : (
        <>
          {changes.truncated ? (
            <p className="mb-2 flex items-center gap-2 text-[12.5px] text-warning-foreground"><WarningCircle aria-hidden /> The diff is too long to show in full here.</p>
          ) : null}
          <div className="overflow-hidden rounded-2xl border border-border bg-card">
            {files.map((file, i) => (
              <FileRow key={file.path + i} file={file} defaultOpen={files.length <= 3 ? true : i === 0} />
            ))}
            {files.length === 0
              ? changes.files.map((path) => (
                  <div key={path} className="flex min-h-9 items-center gap-2 px-3 font-mono text-[12px]"><File size={13} aria-hidden /> {path}</div>
                ))
              : null}
          </div>
        </>
      )}
    </section>
  );
}
