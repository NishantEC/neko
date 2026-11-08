// Inbox: what Neko's routines did for you and what only you can do, per scope.
// A routine is a recurring Claude Code run in a scope's folder (messages, the
// morning brief, the PR sweep) that acts under the scope's standing orders and
// then reports. The daemon owns them (crates/neko-daemon/src/routines.rs); this
// page polls `Routines` while it is visible and sends the edits.
import { useCallback, useEffect, useMemo, useState, type ReactNode } from "react";
import { request, type Json } from "@/api";
import { Button } from "@/ui/button";
import { Input } from "@/ui/input";
import { Textarea } from "@/ui/textarea";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/ui/collapsible";
import { SegmentedControl, SegmentedControlItem } from "@/ui/segmented-control";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/ui/dialog";
import { toast } from "@/ui/sonner";
import { ArrowSquareOut, CaretDown, Check, Copy, Link as LinkIcon, Pause, PencilSimple, Play, Tray } from "@/ui/icons";
import { GlyphButton, ProgressRing, StatusGlyph } from "@/ui/status-glyph";
import { itemProgress, routineStatus } from "@/setup/routine-status";
import { copyText, useContextMenu } from "@/shell/context-menu";
import { EmptyBlock, ErrorNote, FormField, GROUP_HEADING, LoadingRows, Panel, RowAction, RowActions, Section, SetupPage, Toggle } from "@/setup/kit";
import { fillOwner, relativeTime, useNow } from "@/setup/lib";
import { cn } from "@/ui/lib/utils";

const POLL_MS = 10_000;
/** Lists show this many before "Show more", so a scope fits on a screen. */
const SHORT = 5;

type Data = { routines: Json[]; standing_orders: Record<string, string>; scopes: Json[]; owner_name?: string };
type Send = (body: Json, done?: string) => Promise<boolean>;

const pad = (n: number) => String(n).padStart(2, "0");

function cadenceText(cadence: Json): string {
  if (cadence?.Every) {
    const { minutes, from_hour, to_hour } = cadence.Every;
    return from_hour === to_hour ? `Every ${minutes} min` : `Every ${minutes} min, ${pad(from_hour)}:00–${pad(to_hour)}:00`;
  }
  if (cadence?.Daily) return `Daily at ${pad(cadence.Daily.hour)}:${pad(cadence.Daily.minute)}`;
  return "";
}

/** Only web links open from the page; an agent wrote this one. */
function safeLink(link: unknown): string | null {
  return typeof link === "string" && /^https?:\/\//i.test(link) ? link : null;
}

const folderName = (path: string) => path.split("/").filter(Boolean).pop() ?? path;

function useRoutines() {
  const [data, setData] = useState<Data | null>(null);
  const [error, setError] = useState<string | null>(null);
  const take = (reply: Json) => {
    if (reply?.Routines) setData(reply.Routines as Data);
  };
  const load = useCallback(async () => {
    try {
      take(await request("Routines"));
      setError(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }, []);
  useEffect(() => {
    void load();
    // A hidden page does not poll: the daemon's idle cost matters.
    const id = setInterval(() => { if (!document.hidden) void load(); }, POLL_MS);
    const onVisible = () => { if (!document.hidden) void load(); };
    document.addEventListener("visibilitychange", onVisible);
    return () => { clearInterval(id); document.removeEventListener("visibilitychange", onVisible); };
  }, [load]);
  const send = useCallback<Send>(async (body, done) => {
    try {
      take(await request(body));
      if (done) toast.success(done);
      return true;
    } catch (e) {
      toast.error(e instanceof Error ? e.message : String(e));
      return false;
    }
  }, []);
  return { data, error, send, clear: () => setError(null) };
}

/** The first `SHORT` items, and a toggle for the rest. */
function ShortList<T>({ items, render, noun, inList = false }: { items: T[]; render: (item: T) => ReactNode; noun: string; inList?: boolean }) {
  const More = inList ? "li" : "div";
  const [all, setAll] = useState(false);
  const shown = all ? items : items.slice(0, SHORT);
  return (
    <>
      {shown.map(render)}
      {items.length > SHORT ? (
        <More className="list-none px-3 py-1">
          <button type="button" aria-expanded={all} onClick={() => setAll(!all)} className="rounded-md text-[12.5px] text-muted-foreground outline-none transition-colors hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring/60">
            {all ? "Show fewer" : `Show ${items.length - SHORT} more ${noun}`}
          </button>
        </More>
      ) : null}
    </>
  );
}

function clampInt(text: string, min: number, max: number, fallback: number) {
  const n = Number.parseInt(text, 10);
  return Number.isFinite(n) ? Math.min(max, Math.max(min, n)) : fallback;
}

function RoutineEditor({ routine, onClose, send }: { routine: Json | null; onClose: () => void; send: Send }) {
  const [instruction, setInstruction] = useState("");
  const [model, setModel] = useState("");
  const [mode, setMode] = useState("every");
  // Kept as typed text and clamped on save, so "30" can be typed.
  const [minutes, setMinutes] = useState("20");
  const [fromHour, setFromHour] = useState("8");
  const [toHour, setToHour] = useState("23");
  const [time, setTime] = useState("10:30");
  const [browser, setBrowser] = useState(true);
  const [computer, setComputer] = useState(true);
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    if (!routine) return;
    setInstruction(routine.instruction ?? "");
    setModel(routine.model ?? "");
    setBrowser(routine.browser ?? true);
    setComputer(routine.computer ?? true);
    const c = routine.cadence ?? {};
    setMode(c.Every ? "every" : "daily");
    if (c.Every) { setMinutes(String(c.Every.minutes)); setFromHour(String(c.Every.from_hour)); setToHour(String(c.Every.to_hour)); }
    if (c.Daily) setTime(`${pad(c.Daily.hour)}:${pad(c.Daily.minute)}`);
  }, [routine]);
  const from = clampInt(fromHour, 0, 23, 8);
  const to = clampInt(toHour, 1, 24, 23);
  const save = async () => {
    if (!instruction.trim() || busy) return;
    const [hour, minute] = time.split(":").map((n) => Number(n) || 0);
    const cadence = mode === "every"
      ? { Every: { minutes: clampInt(minutes, 5, 720, 20), from_hour: from, to_hour: to } }
      : { Daily: { hour, minute } };
    setBusy(true);
    const ok = await send({ SaveRoutine: { routine: { ...routine, instruction: instruction.trim(), model: model.trim(), cadence, browser, computer } } }, "Routine saved");
    setBusy(false);
    if (ok) onClose();
  };
  return (
    <Dialog open={!!routine} onOpenChange={(o) => { if (!o) onClose(); }}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>Edit {routine?.name}</DialogTitle>
          <DialogDescription>What this routine does each run and how often. It runs Claude Code in the scope’s folder with your own setup.</DialogDescription>
        </DialogHeader>
        <form className="flex flex-col gap-5" onSubmit={(e) => { e.preventDefault(); void save(); }}>
          <div className="flex max-h-[60vh] flex-col gap-5 overflow-y-auto px-0.5">
            <FormField label="What to do" htmlFor="rt-instruction">
              <Textarea id="rt-instruction" rows={8} value={instruction} onChange={(e) => setInstruction(e.target.value)} />
            </FormField>
            <SegmentedControl value={mode} onValueChange={(v) => v && setMode(v)} aria-label="How often">
              <SegmentedControlItem value="every">Repeat during the day</SegmentedControlItem>
              <SegmentedControlItem value="daily">Once a day</SegmentedControlItem>
            </SegmentedControl>
            {mode === "every" ? (
              <>
                <div className="grid gap-4 sm:grid-cols-3">
                  <FormField label="Every (minutes)" htmlFor="rt-min"><Input id="rt-min" inputMode="numeric" value={minutes} onChange={(e) => setMinutes(e.target.value)} onBlur={() => setMinutes(String(clampInt(minutes, 5, 720, 20)))} /></FormField>
                  <FormField label="From hour" htmlFor="rt-from"><Input id="rt-from" inputMode="numeric" value={fromHour} onChange={(e) => setFromHour(e.target.value)} onBlur={() => setFromHour(String(from))} /></FormField>
                  <FormField label="Until hour" htmlFor="rt-to"><Input id="rt-to" inputMode="numeric" value={toHour} onChange={(e) => setToHour(e.target.value)} onBlur={() => setToHour(String(to))} /></FormField>
                </div>
                <p className="text-[13px] text-muted-foreground">{from === to ? "Same start and end hour: runs all day and night." : `Runs between ${pad(from)}:00 and ${pad(to)}:00, less often while nothing is happening.`}</p>
              </>
            ) : (
              <FormField label="At" htmlFor="rt-time"><Input id="rt-time" type="time" value={time} onChange={(e) => setTime(e.target.value || "10:30")} /></FormField>
            )}
            <div className="flex items-start justify-between gap-4">
              <div>
                <p className="text-sm font-medium text-foreground">Use my Chrome</p>
                <p className="text-[13px] text-muted-foreground">Lets this routine work in your own Chrome, where you are signed in: approving a sign-in it needs, or work only a website allows. Chrome has to be open.</p>
              </div>
              <Toggle checked={browser} label="Use my Chrome" onChange={setBrowser} />
            </div>
            <div className="flex items-start justify-between gap-4">
              <div>
                <p className="text-sm font-medium text-foreground">Use my Mac’s apps</p>
                <p className="text-[13px] text-muted-foreground">Lets this routine see and operate Mac apps through Peekaboo, in the background where it can, for work that only lives in an app.</p>
              </div>
              <Toggle checked={computer} label="Use my Mac’s apps" onChange={setComputer} />
            </div>
            <FormField label="Model" htmlFor="rt-model" hint="A Claude Code model alias such as sonnet or opus. Empty means sonnet.">
              <Input id="rt-model" value={model} onChange={(e) => setModel(e.target.value)} placeholder="sonnet" />
            </FormField>
          </div>
          <DialogFooter>
            <Button type="button" variant="outline" onClick={onClose}>Cancel</Button>
            <Button type="submit" loading={busy} disabled={!instruction.trim()}>Save</Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}

function StandingOrders({ workspaceId, text, owner, send }: { workspaceId: string; text: string; owner?: string; send: Send }) {
  const [draft, setDraft] = useState(text);
  const [busy, setBusy] = useState(false);
  useEffect(() => setDraft(text), [text]);
  const firstLine = fillOwner(text.split("\n")[0] ?? "", owner);
  return (
    <Collapsible className="group/row rounded-[10px] px-3 py-1.5 transition-colors hover:bg-foreground/[0.03]">
      <CollapsibleTrigger className="group/so flex w-full min-w-0 items-center gap-3 rounded-md py-0.5 text-start outline-none focus-visible:ring-2 focus-visible:ring-ring/60">
        <CaretDown aria-hidden size={12} className="shrink-0 text-muted-foreground transition-transform group-data-[panel-open]/so:rotate-0 -rotate-90" />
        <span className="shrink-0 text-[13.5px] text-foreground">Standing orders</span>
        <span className="min-w-0 flex-1 truncate text-[12.5px] text-muted-foreground">{firstLine}</span>
      </CollapsibleTrigger>
      <CollapsibleContent className="mt-2 flex flex-col gap-2 ps-6">
        <FormField label="Standing orders" htmlFor={`orders-${workspaceId}`} hint={`Every routine in this scope follows these. {owner} stands for ${owner?.trim() || "the owner"}.`}>
          <Textarea id={`orders-${workspaceId}`} rows={4} value={draft} onChange={(e) => setDraft(e.target.value)} />
        </FormField>
        {draft !== text ? (
          <div>
            <Button size="sm" loading={busy} onClick={async () => { setBusy(true); await send({ SetStandingOrders: { workspace_id: workspaceId, text: draft } }, "Standing orders saved"); setBusy(false); }}>Save standing orders</Button>
          </div>
        ) : null}
      </CollapsibleContent>
    </Collapsible>
  );
}

function RoutineRow({ view, now, send, onEdit }: { view: Json; now: number; send: Send; onEdit: () => void }) {
  const { routine, state, runs } = view;
  const last: Json | undefined = runs?.[0];
  const queued = !!state.run_requested && !state.running;
  const [kind, word] = routineStatus(view);
  const status = (
    <span data-slot="routine-status" data-status={kind} className={`inline-flex items-center gap-1.5 text-[13px] ${kind === "failed" ? "text-destructive-foreground" : "text-foreground/80"}`}>
      <StatusGlyph status={kind} label="" />
      {word}
    </span>
  );
  // The latest run's items for you, as parts: done of all.
  const parts = itemProgress(view);
  const runNow = () => void send({ RunRoutineNow: { id: routine.id } }, `${routine.name} will start within 30 seconds`);
  const setEnabled = (on: boolean) => void send({ SetRoutineEnabled: { id: routine.id, enabled: on } });
  const menu = useContextMenu(() => [
    { label: "Run now", icon: Play, disabled: !!state.running || queued, run: runNow },
    { label: routine.enabled ? "Pause" : "Resume", icon: routine.enabled ? Pause : Play, run: () => setEnabled(!routine.enabled) },
    { label: "Edit…", icon: PencilSimple, run: onEdit },
    "separator",
    { label: "Copy name", icon: Copy, run: () => void copyText(String(routine.name ?? ""), "Copied the name") },
  ]);
  const due = state.next_due_ms;
  const next = state.running || queued || !routine.enabled ? null
    : !due || due <= now ? "due now"
    : `next ${relativeTime(due, now)}`;
  const facts = [cadenceText(routine.cadence), next, last ? `last ${relativeTime(last.finished_ms ?? last.started_ms, now)}` : null].filter(Boolean).join(" · ");
  return (
    <div data-slot="routine-row" {...menu} className="group/row flex flex-col gap-1 rounded-[10px] px-3 py-1.5 transition-colors hover:bg-foreground/[0.03]">
      <div className="flex min-h-7 items-center gap-3">
        <span className="shrink-0 text-[13.5px] text-foreground">{routine.name}</span>
        {status}
        {parts.total ? <ProgressRing done={parts.done} total={parts.total} /> : null}
        <span className="min-w-0 flex-1 truncate text-end text-[12.5px] text-muted-foreground">{facts}</span>
        <RowActions>
          <RowAction label={`Run ${routine.name} now`} icon={Play} disabled={state.running || queued} onClick={runNow} />
          <RowAction label={`Edit ${routine.name}`} icon={PencilSimple} onClick={onEdit} />
        </RowActions>
        <Toggle checked={routine.enabled} label={`${routine.enabled ? "Pause" : "Resume"} ${routine.name}`} onChange={setEnabled} />
      </div>
      {last && !last.ok ? (
        <div className="flex flex-col gap-1 ps-0">
          <p className="text-[13px] text-destructive-foreground">{last.error}</p>
          {last.text ? (
            <Collapsible>
              <CollapsibleTrigger render={<Button variant="ghost" size="xs" className="-ms-2" />}>Show the agent’s reply <CaretDown /></CollapsibleTrigger>
              <CollapsibleContent>
                <pre className="mt-1.5 max-h-64 overflow-auto whitespace-pre-wrap rounded-xl border border-border bg-foreground/[0.03] p-3 font-sans text-[13px] leading-relaxed text-foreground/85">{last.text}</pre>
              </CollapsibleContent>
            </Collapsible>
          ) : null}
        </div>
      ) : null}
      {state.note ? <p className="truncate text-[12.5px] text-muted-foreground" title={state.note}>{state.note}</p> : null}
    </div>
  );
}

/**
 * One thing only you can do, on one line: the glyph (click to mark it done),
 * the title, where it came from and when. The detail is one truncated line
 * under it, in full when the row is opened. Open and Done also sit at the
 * right on hover and in the right-click menu.
 */
function NeedsYouItem({ item, where, when, onDone }: { item: Json; where: string; when: string; onDone: () => void }) {
  const [expanded, setExpanded] = useState(false);
  const link = safeLink(item.link);
  const text = [item.title, item.detail].filter(Boolean).join("\n\n");
  const menu = useContextMenu(() => [
    { label: "Mark done", icon: Check, run: onDone },
    link ? { label: "Open link", icon: ArrowSquareOut, run: () => void window.open(link, "_blank", "noreferrer") } : null,
    "separator",
    { label: "Copy", icon: Copy, run: () => void copyText(text) },
    link ? { label: "Copy link", icon: LinkIcon, run: () => void copyText(link, "Copied the link") } : null,
  ]);
  return (
    <div data-slot="needs-you-item" data-expanded={expanded || undefined} {...menu} className="group/row flex items-start gap-3 rounded-[10px] px-3 py-2 transition-colors hover:bg-foreground/[0.03]">
      <span className="flex h-5 items-center">
        <GlyphButton status="inProgress" label={`Mark “${item.title}” done`} onClick={onDone} />
      </span>
      <button
        type="button"
        aria-expanded={item.detail ? expanded : undefined}
        onClick={() => setExpanded(!expanded)}
        className="flex min-w-0 flex-1 flex-col gap-0.5 rounded-md text-start outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
      >
        <span className="flex min-w-0 items-baseline gap-3">
          <span className="min-w-0 truncate text-[13.5px] text-foreground">{item.title}</span>
          <span className="ms-auto shrink-0 truncate text-[12.5px] text-muted-foreground group-hover/row:hidden group-focus-within/row:hidden">{where}</span>
        </span>
        {item.detail ? (
          <span className={cn("max-w-[72ch] text-[12.5px] text-muted-foreground", expanded ? "whitespace-pre-wrap" : "truncate")}>{item.detail}</span>
        ) : null}
      </button>
      <span className="flex h-5 shrink-0 items-center gap-3">
        <span className="text-[12.5px] tabular-nums text-muted-foreground/70">{when}</span>
        <RowActions className="hidden group-hover/row:flex group-focus-within/row:flex [@media(hover:none)]:flex">
          {link ? <RowAction label={`Open ${item.title}`} icon={ArrowSquareOut} href={link} /> : null}
          <RowAction label={`Mark “${item.title}” done`} icon={Check} onClick={onDone} />
        </RowActions>
      </span>
    </div>
  );
}

function SetupNotes({ problems }: { problems: string[] }) {
  return (
    <ul aria-label="Setup notes" className="flex flex-col gap-1 px-3 py-1.5 text-[12.5px] text-warning-foreground">
      {problems.map((p) => <li key={p} className="flex gap-2"><span aria-hidden>!</span><span className="min-w-0">{p}</span></li>)}
    </ul>
  );
}

function InboxBody() {
  const { data, error, send, clear } = useRoutines();
  const now = useNow();
  const [editing, setEditing] = useState<Json | null>(null);

  const scopeName = useMemo(() => {
    const names: Record<string, string> = {};
    for (const s of data?.scopes ?? []) if (s.workspace_id) names[s.workspace_id] = s.name;
    return names;
  }, [data]);

  const open = useMemo(() => {
    const items: { item: Json; run: Json; routine: Json }[] = [];
    for (const view of data?.routines ?? []) for (const run of view.runs ?? []) for (const item of run.needs_you ?? []) if (!item.done) items.push({ item, run, routine: view.routine });
    return items.sort((a, b) => b.item.seq - a.item.seq);
  }, [data]);

  const errorNote = <ErrorNote error={error} title="Can’t reach the Neko daemon" onDismiss={clear} />;
  if (!data) return error ? errorNote : <LoadingRows rows={4} />;

  const done = async (routineId: string, runId: string, seq: number) => {
    const left = open.length - 1;
    await send({ ClearNeedsYou: { routine_id: routineId, run_id: runId, seq } }, left > 0 ? `Done. ${left} left.` : "All clear.");
  };

  const scopes = data.scopes.filter((s) => s.workspace_id && (data.routines.some((v) => v.routine.workspace_id === s.workspace_id) || s.problems?.length));
  return (
    <>
      {error ? <ErrorNote error={`${error} The list below may be out of date.`} title="Can’t reach the Neko daemon" onDismiss={clear} /> : null}
      <Section title="Needs you" count={open.length || undefined}>
        <span role="status" aria-live="polite" className="sr-only">{open.length ? `${open.length} thing${open.length === 1 ? "" : "s"} only you can do.` : "Nothing needs you."}</span>
        {open.length === 0 ? (
          <p className="text-[13.5px] text-muted-foreground">Nothing needs you right now.</p>
        ) : (
          <Panel>
            <ShortList
              items={open}
              noun="items"
              render={({ item, run, routine }) => (
                <NeedsYouItem
                  key={item.seq}
                  item={item}
                  where={`${scopeName[routine.workspace_id] ?? "Scope"} · ${routine.name}`}
                  when={relativeTime(run.finished_ms ?? run.started_ms, now)}
                  onDone={() => void done(routine.id, run.id, item.seq)}
                />
              )}
            />
          </Panel>
        )}
      </Section>
      {scopes.map((scope) => {
        const views = data.routines.filter((v) => v.routine.workspace_id === scope.workspace_id);
        const did = views
          .flatMap((v) => (v.runs ?? []).filter((r: Json) => r.ok).flatMap((r: Json) => (r.did ?? []).map((line: string, i: number) => ({ line, at: r.finished_ms ?? r.started_ms, key: `${r.id}-${i}`, name: v.routine.name }))))
          .sort((a, b) => b.at - a.at)
          .slice(0, 20);
        return (
          <Section key={scope.workspace_id} title={<span title={(scope.folders ?? []).map(folderName).join(", ")}>{scope.name}</span>} collapsible>
            <Panel>
              {views.map((v) => <RoutineRow key={v.routine.id} view={v} now={now} send={send} onEdit={() => setEditing(v.routine)} />)}
              {views.length ? <StandingOrders workspaceId={scope.workspace_id} text={data.standing_orders[scope.workspace_id] ?? ""} owner={data.owner_name} send={send} /> : null}
              {scope.problems?.length ? <SetupNotes problems={scope.problems} /> : null}
              {views.length ? (
                <div className="mt-3 flex flex-col">
                  <p className={cn(GROUP_HEADING, "px-3 pb-1")}>What Neko did</p>
                  {did.length === 0 ? <p className="px-3 py-1 text-[13px] text-muted-foreground">Nothing yet.</p> : (
                    <ul>
                      <ShortList
                        items={did}
                        noun="actions"
                        inList
                        render={(d) => (
                          <li key={d.key} className="flex min-w-0 items-baseline gap-3 rounded-[10px] px-3 py-1 text-[13px] transition-colors hover:bg-foreground/[0.03]">
                            <span className="min-w-0 flex-1 truncate text-foreground/90" title={d.line}>{d.line}</span>
                            <span className="shrink-0 text-[12.5px] text-muted-foreground/70">{d.name} · {relativeTime(d.at, now)}</span>
                          </li>
                        )}
                      />
                    </ul>
                  )}
                </div>
              ) : null}
            </Panel>
          </Section>
        );
      })}
      {scopes.length === 0 ? <EmptyBlock icon={Tray} title="No routines yet" description="Neko creates them for your home folder and for each project folder with its own MCP setup in Documents, code, Projects or a similar folder." /> : null}
      <RoutineEditor routine={editing} onClose={() => setEditing(null)} send={send} />
    </>
  );
}

export default function Inbox() {
  return (
    <SetupPage title="Inbox">
      <InboxBody />
    </SetupPage>
  );
}
