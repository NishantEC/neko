// One suggestion: why Neko made it, exactly what would change, the evidence, what
// happened to it, and, once applied, whether the trouble went away.
import { useEffect, useRef } from "react";
import { Button } from "@/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "@/ui/dialog";
import { StatusGlyph } from "@/ui/status-glyph";
import { ArrowCounterClockwise } from "@/ui/icons";
import { ErrorNote } from "@/setup/kit";
import { relativeTime } from "@/setup/lib";
import { IMPROVE_KIND_LABELS, IMPROVE_STATE_WORDS, improveCounts, improveGlyph, type Improvement } from "@/setup/improve";
import { Heading, Mono } from "@/pages/setup/packs/parts";

/** Where the dialog scrolls to when it opens. */
export type DetailFocus = "evidence" | null;

/** A unified diff, one line each: additions mint, removals coral, the rest muted. */
function Diff({ text }: { text: string }) {
  return (
    <pre data-slot="improve-diff" className="max-h-72 select-text overflow-auto rounded-lg bg-foreground/[0.05] px-3 py-2 font-mono text-[12px] leading-relaxed">
      {text.split("\n").map((line, i) => (
        <div
          key={i}
          data-diff={line.startsWith("+") && !line.startsWith("+++") ? "add" : line.startsWith("-") && !line.startsWith("---") ? "del" : "ctx"}
          className={line.startsWith("+") && !line.startsWith("+++") ? "text-success" : line.startsWith("-") && !line.startsWith("---") ? "text-destructive" : "text-muted-foreground"}
        >{line || " "}</div>
      ))}
    </pre>
  );
}

export function ImproveDetail({ item, focus, busy, error, onClose, onAct, onDismissError }: {
  item: Improvement | null;
  focus: DetailFocus;
  busy: boolean;
  error: string | null;
  onClose: () => void;
  onAct: (act: "apply" | "reject" | "undo") => void;
  onDismissError: () => void;
}) {
  const evidence = useRef<HTMLDivElement>(null);
  const id = item?.id;
  useEffect(() => {
    if (!id || focus !== "evidence") return;
    const frame = requestAnimationFrame(() => evidence.current?.scrollIntoView({ block: "start" }));
    return () => cancelAnimationFrame(frame);
  }, [id, focus]);
  if (!item) return null;
  const counts = improveCounts(item);
  return (
    <Dialog open onOpenChange={(o) => { if (!o) onClose(); }}>
      <DialogContent className="max-h-[88vh] overflow-y-auto sm:max-w-2xl" data-slot="improve-detail" data-id={item.id}>
        <DialogHeader>
          <DialogTitle className="text-lg">{item.title}</DialogTitle>
          <DialogDescription>
            <span className="inline-flex items-center gap-1.5">
              <StatusGlyph status={improveGlyph(item.state)} label="" />
              {IMPROVE_STATE_WORDS[item.state]}
            </span>
            {` · ${IMPROVE_KIND_LABELS[item.kind]} · ${relativeTime(item.created_ms)}`}
          </DialogDescription>
        </DialogHeader>
        <div className="flex flex-col gap-5">
          <div className="flex flex-wrap items-center gap-2">
            <span className="flex-1" />
            {item.state === "open" ? (
              <>
                <Button size="sm" disabled={busy} onClick={() => onAct("apply")}>{item.default_label || "Apply"}</Button>
                <Button size="sm" variant="outline" disabled={busy} onClick={() => onAct("reject")}>Reject</Button>
              </>
            ) : null}
            {item.can_undo ? <Button size="sm" variant="outline" disabled={busy} onClick={() => onAct("undo")}><ArrowCounterClockwise /> Undo</Button> : null}
          </div>
          <ErrorNote error={error} title="That didn’t work" onDismiss={onDismissError} />
          {counts ? <p data-slot="improve-counts" className="text-[13px] text-foreground/85">{counts}</p> : null}
          {item.state === "sent" ? <p className="text-[13px] text-muted-foreground">The change waits for your approval in Skills.</p> : null}
          <div className="flex flex-col gap-2" data-slot="improve-why">
            <Heading>Why</Heading>
            <p className="whitespace-pre-wrap text-[13px] text-foreground/90">{item.why}</p>
          </div>
          <div className="flex flex-col gap-2" data-slot="improve-change">
            <Heading>The change</Heading>
            {item.kind === "install_pack" ? (
              <p className="text-[13px] text-foreground/90">
                Opens the preview of the {item.target ? <span className="font-mono text-[12.5px]">{item.target}</span> : null} pack. Nothing installs by itself; you read what it would do and install it there.
              </p>
            ) : item.kind === "skill_patch" && item.diff ? (
              <Diff text={item.diff} />
            ) : (
              <p className="whitespace-pre-wrap text-[13px] text-foreground/90">{item.change}</p>
            )}
          </div>
          <div ref={evidence} className="flex scroll-mt-4 flex-col gap-2" data-slot="improve-evidence">
            <Heading>Evidence</Heading>
            {item.evidence ? <Mono><span className="whitespace-pre-wrap">{item.evidence}</span></Mono> : <p className="text-[13px] text-muted-foreground">No evidence was kept.</p>}
          </div>
          {item.history.length > 0 ? (
            <div className="flex flex-col gap-2" data-slot="improve-history">
              <Heading>History</Heading>
              <ul className="flex flex-col gap-1 text-[13px]">
                {item.history.map((h, i) => (
                  <li key={`${h.at_ms}-${i}`} className="flex items-baseline gap-3">
                    <span className="w-24 shrink-0 text-[12.5px] tabular-nums text-muted-foreground/70">{relativeTime(h.at_ms)}</span>
                    <span className="text-foreground/90">{h.text}</span>
                  </li>
                ))}
              </ul>
            </div>
          ) : null}
        </div>
      </DialogContent>
    </Dialog>
  );
}
