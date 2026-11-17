// Improvements: what Neko suggests after it notices the same trouble again. One row per
// proposal, newest first. Apply, Reject and Undo are the daemon's `Improve` command
// (neko-protocol improve.rs); every answer is the whole listing, which replaces what
// is shown. Nothing installs by itself: a pack suggestion opens the pack's preview.
import { useCallback, useEffect, useRef, useState } from "react";
import { Button } from "@/ui/button";
import { StatusGlyph } from "@/ui/status-glyph";
import { ArrowCounterClockwise, Eye, Lightbulb } from "@/ui/icons";
import { EmptyBlock, ErrorNote, LoadingRows, Panel, RowAction, RowActions, Section, SetupPage } from "@/setup/kit";
import { useContextMenu } from "@/shell/context-menu";
import { confirm } from "@/shell/confirm";
import { navigate } from "@/router";
import { relativeTime } from "@/setup/lib";
import { useSetupCommands } from "@/setup/commands";
import { openPackPreviewNext } from "@/setup/packs";
import {
  IMPROVE_KIND_LABELS, IMPROVE_STATE_WORDS, applyImprovement, improveGlyph, improveStateWord, listImprovements, rejectImprovement, setImproveEnabled,
  undoImprovement, undoWords, type ImproveReply, type Improvement,
} from "@/setup/improve";
import { ImproveDetail, type DetailFocus } from "@/pages/setup/improve/ImproveDetail";

/** The listing is also reloaded on this slow interval, in case something changed outside this page. */
const REFRESH_MS = 30_000;

type Act = "apply" | "reject" | "undo";

function ImproveRow({ item, busy, error, onOpen, onAct, onDismissError }: {
  item: Improvement;
  busy: boolean;
  error: string | null;
  onOpen: (focus: DetailFocus) => void;
  onAct: (act: Act) => void;
  onDismissError: () => void;
}) {
  const open = item.state === "open";
  const word = improveStateWord(item.state);
  const menu = useContextMenu(() => [
    open ? { label: "Apply", disabled: busy, run: () => onAct("apply") } : null,
    open ? { label: "Reject", disabled: busy, run: () => onAct("reject") } : null,
    item.can_undo ? { label: "Undo", icon: ArrowCounterClockwise, disabled: busy, run: () => onAct("undo") } : null,
    { label: "Open evidence", icon: Eye, run: () => onOpen("evidence") },
  ]);
  return (
    <div
      data-slot="improvement"
      data-id={item.id}
      data-state={item.state}
      data-kind={item.kind}
      {...menu}
      className="group/row flex flex-col gap-1 rounded-[10px] px-3 py-1.5 transition-colors hover:bg-foreground/[0.03]"
    >
      <div className="flex min-h-8 items-center gap-3">
        <StatusGlyph status={improveGlyph(item.state)} label={IMPROVE_STATE_WORDS[item.state]} />
        <button
          type="button"
          onClick={() => onOpen(null)}
          aria-label={`${item.title}, ${IMPROVE_KIND_LABELS[item.kind]}, ${IMPROVE_STATE_WORDS[item.state]}. Open details`}
          className="flex min-w-0 flex-1 items-baseline gap-3 rounded-md text-start outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
        >
          <span className={`min-w-0 truncate text-[13.5px] ${item.state === "rejected" ? "text-muted-foreground" : "text-foreground"}`}>{item.title}</span>
          <span data-slot="improve-kind" className="shrink-0 text-[12.5px] text-muted-foreground">{IMPROVE_KIND_LABELS[item.kind]}</span>
          {word ? <span data-slot="improve-word" className="shrink-0 text-[12.5px] text-muted-foreground">{word}</span> : null}
          <span className="ms-auto shrink-0 text-[12.5px] tabular-nums text-muted-foreground/60">{relativeTime(item.created_ms)}</span>
        </button>
        {open ? (
          <>
            <Button size="xs" variant="outline" disabled={busy} aria-label={`Apply ${item.title}`} onClick={() => onAct("apply")}>Apply</Button>
            <Button size="xs" variant="ghost" disabled={busy} aria-label={`Reject ${item.title}`} onClick={() => onAct("reject")}>Reject</Button>
          </>
        ) : null}
        {item.can_undo ? (
          <Button size="xs" variant="outline" disabled={busy} aria-label={`Undo ${item.title}`} onClick={() => onAct("undo")}>Undo</Button>
        ) : null}
        <RowActions>
          <RowAction label={`Open evidence for ${item.title}`} icon={Eye} onClick={() => onOpen("evidence")} />
        </RowActions>
      </div>
      <ErrorNote error={error} onDismiss={onDismissError} />
    </div>
  );
}

function ImproveBody() {
  const [reply, setReply] = useState<ImproveReply | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [note, setNote] = useState<string | null>(null);
  const [openId, setOpenId] = useState<string | null>(null);
  const [focus, setFocus] = useState<DetailFocus>(null);
  const [busyId, setBusyId] = useState<string | null>(null);
  const [errors, setErrors] = useState<Record<string, string>>({});
  const busyRef = useRef(false);

  const reload = useCallback(async (signal?: AbortSignal) => {
    try {
      const next = await listImprovements(signal);
      if (signal?.aborted || busyRef.current) return;
      setReply(next);
      setLoadError(null);
    } catch (e) {
      if (!signal?.aborted) setLoadError(e instanceof Error ? e.message : String(e));
    }
  }, []);

  // On open, and slowly after that.
  useEffect(() => {
    const controller = new AbortController();
    void reload(controller.signal);
    const id = setInterval(() => void reload(controller.signal), REFRESH_MS);
    return () => { controller.abort(); clearInterval(id); };
  }, [reload]);

  const take = (next: ImproveReply) => {
    setReply(next);
    setLoadError(null);
    setNote(next.note);
  };
  const dismiss = (id: string) => setErrors((e) => { const { [id]: _gone, ...rest } = e; return rest; });

  const act = async (item: Improvement, what: Act) => {
    if (busyRef.current) return;
    // Reject never asks. Undo asks only when the daemon says it is hard to take back.
    if (what === "undo" && item.undo_confirms) {
      const ok = await confirm({
        title: `Undo “${item.title}”?`,
        description: undoWords(item.kind),
        action: "Undo",
        destructive: true,
      });
      if (!ok) return;
    }
    busyRef.current = true;
    setBusyId(item.id);
    dismiss(item.id);
    try {
      const next = await (what === "apply" ? applyImprovement(item.id) : what === "reject" ? rejectImprovement(item.id) : undoImprovement(item.id));
      take(next);
      if (what === "apply" && next.open_pack) {
        // The pack's preview opens on the Packs page; the person installs it there.
        setOpenId(null);
        openPackPreviewNext(next.open_pack);
        navigate("packs");
      }
    } catch (e) {
      setErrors((current) => ({ ...current, [item.id]: e instanceof Error ? e.message : String(e) }));
    } finally {
      busyRef.current = false;
      setBusyId(null);
    }
  };

  const turnOn = async () => {
    if (busyRef.current) return;
    busyRef.current = true;
    setBusyId("enable");
    try {
      take(await setImproveEnabled(true));
    } catch (e) {
      setLoadError(e instanceof Error ? e.message : String(e));
    } finally {
      busyRef.current = false;
      setBusyId(null);
    }
  };

  const items = reply?.proposals ?? [];
  const opened = openId ? items.find((p) => p.id === openId) ?? null : null;

  return (
    <SetupPage title="Improvements" count={items.length || undefined}>
      {loadError && !reply ? <ErrorNote title="Couldn’t load improvements" error={loadError} /> : null}
      {!reply && !loadError ? <LoadingRows rows={3} /> : null}
      {reply && !reply.enabled ? (
        <div data-slot="improve-off" className="flex flex-wrap items-center gap-3 text-[13px] text-muted-foreground">
          <span>Suggestions are off. Neko is not looking for patterns.</span>
          <Button size="xs" variant="outline" disabled={busyId === "enable"} onClick={() => void turnOn()}>Turn on suggestions</Button>
        </div>
      ) : null}
      {note ? <p data-slot="improve-note" role="status" className="text-[13px] text-muted-foreground">{note}</p> : null}
      {reply && items.length === 0 && reply.enabled ? (
        <EmptyBlock icon={Lightbulb} title="Neko hasn't suggested anything yet." />
      ) : null}
      {reply && items.length > 0 ? (
        <Section title="Suggestions" count={items.length}>
          <Panel>
            {items.map((p) => (
              <ImproveRow
                key={p.id}
                item={p}
                busy={busyId === p.id}
                error={errors[p.id] ?? null}
                onOpen={(f) => { setOpenId(p.id); setFocus(f); }}
                onAct={(what) => void act(p, what)}
                onDismissError={() => dismiss(p.id)}
              />
            ))}
          </Panel>
        </Section>
      ) : null}
      <ImproveDetail
        item={opened}
        focus={focus}
        busy={opened ? busyId === opened.id : false}
        error={opened ? errors[opened.id] ?? null : null}
        onClose={() => { setOpenId(null); setFocus(null); }}
        onAct={(what) => opened && void act(opened, what)}
        onDismissError={() => opened && dismiss(opened.id)}
      />
    </SetupPage>
  );
}

export default function Improvements() {
  useSetupCommands();
  return <ImproveBody />;
}
