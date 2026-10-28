// Inbox: what Neko's routines did for you and what only you can do, per scope.
// A routine is a recurring Claude Code run in a scope's folder (messages, the
// morning brief, the PR sweep) that acts under the scope's standing orders and
// then reports. The daemon owns them (crates/neko-daemon/src/routines.rs); this
// page polls `Routines` and sends the edits.
import { useCallback, useEffect, useMemo, useState } from "react";
import { request, type Json } from "@/api";
import { Button } from "@/ui/button";
import { Input } from "@/ui/input";
import { Textarea } from "@/ui/textarea";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/ui/dialog";
import { toast } from "@/ui/sonner";
import { ArrowSquareOut, Check, PencilSimple, Play, Tray } from "@/ui/icons";
import { EmptyBlock, ErrorNote, FormField, Panel, Section, SetupPage, Status, Toggle } from "@/setup/kit";
import { relativeTime, useNow } from "@/setup/lib";

const POLL_MS = 10_000;

type Data = { routines: Json[]; standing_orders: Record<string, string>; scopes: Json[] };

function cadenceText(cadence: Json): string {
  if (cadence?.Every) {
    const { minutes, from_hour, to_hour } = cadence.Every;
    return `Every ${minutes} min, ${String(from_hour).padStart(2, "0")}:00–${String(to_hour).padStart(2, "0")}:00`;
  }
  if (cadence?.Daily) {
    const { hour, minute } = cadence.Daily;
    return `Daily at ${String(hour).padStart(2, "0")}:${String(minute).padStart(2, "0")}`;
  }
  return "";
}

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
    const id = setInterval(() => void load(), POLL_MS);
    return () => clearInterval(id);
  }, [load]);
  const send = useCallback(async (body: Json, done?: string) => {
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

function RoutineEditor({ routine, onClose, send }: { routine: Json | null; onClose: () => void; send: (body: Json, done?: string) => Promise<boolean> }) {
  const [instruction, setInstruction] = useState("");
  const [model, setModel] = useState("");
  const [every, setEvery] = useState(true);
  const [minutes, setMinutes] = useState(20);
  const [fromHour, setFromHour] = useState(8);
  const [toHour, setToHour] = useState(23);
  const [time, setTime] = useState("10:30");
  useEffect(() => {
    if (!routine) return;
    setInstruction(routine.instruction ?? "");
    setModel(routine.model ?? "");
    const c = routine.cadence ?? {};
    setEvery(!!c.Every);
    if (c.Every) { setMinutes(c.Every.minutes); setFromHour(c.Every.from_hour); setToHour(c.Every.to_hour); }
    if (c.Daily) setTime(`${String(c.Daily.hour).padStart(2, "0")}:${String(c.Daily.minute).padStart(2, "0")}`);
  }, [routine]);
  const [hour, minute] = time.split(":").map((n) => Number(n) || 0);
  const save = async () => {
    const cadence = every ? { Every: { minutes, from_hour: fromHour, to_hour: toHour } } : { Daily: { hour, minute } };
    if (await send({ SaveRoutine: { routine: { ...routine, instruction: instruction.trim(), model: model.trim(), cadence } } }, "Routine saved")) onClose();
  };
  return (
    <Dialog open={!!routine} onOpenChange={(o) => { if (!o) onClose(); }}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>Edit {routine?.name}</DialogTitle>
          <DialogDescription>What this routine does each run and how often. It runs Claude Code in the scope’s folder with your own setup.</DialogDescription>
        </DialogHeader>
        <form className="flex max-h-[70vh] flex-col gap-5 overflow-y-auto px-0.5" onSubmit={(e) => { e.preventDefault(); if (instruction.trim()) void save(); }}>
          <FormField label="What to do" htmlFor="rt-instruction">
            <Textarea id="rt-instruction" rows={8} value={instruction} onChange={(e) => setInstruction(e.target.value)} />
          </FormField>
          <div className="flex gap-2">
            <Button type="button" size="sm" variant={every ? "default" : "outline"} onClick={() => setEvery(true)}>Repeat during the day</Button>
            <Button type="button" size="sm" variant={every ? "outline" : "default"} onClick={() => setEvery(false)}>Once a day</Button>
          </div>
          {every ? (
            <div className="grid gap-4 sm:grid-cols-3">
              <FormField label="Every (minutes)" htmlFor="rt-min"><Input id="rt-min" type="number" min={5} max={720} value={minutes} onChange={(e) => setMinutes(Math.max(5, Number(e.target.value) || 20))} /></FormField>
              <FormField label="From hour" htmlFor="rt-from"><Input id="rt-from" type="number" min={0} max={23} value={fromHour} onChange={(e) => setFromHour(Math.min(23, Math.max(0, Number(e.target.value) || 0)))} /></FormField>
              <FormField label="Until hour" htmlFor="rt-to"><Input id="rt-to" type="number" min={1} max={24} value={toHour} onChange={(e) => setToHour(Math.min(24, Math.max(1, Number(e.target.value) || 24)))} /></FormField>
            </div>
          ) : (
            <FormField label="At" htmlFor="rt-time"><Input id="rt-time" type="time" value={time} onChange={(e) => setTime(e.target.value || "10:30")} /></FormField>
          )}
          <FormField label="Model" htmlFor="rt-model" hint="A Claude Code model alias such as sonnet or opus. Empty means sonnet.">
            <Input id="rt-model" value={model} onChange={(e) => setModel(e.target.value)} placeholder="sonnet" />
          </FormField>
        </form>
        <DialogFooter>
          <Button variant="outline" onClick={onClose}>Cancel</Button>
          <Button disabled={!instruction.trim()} onClick={() => void save()}>Save</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function StandingOrders({ workspaceId, text, send }: { workspaceId: string; text: string; send: (body: Json, done?: string) => Promise<boolean> }) {
  const [draft, setDraft] = useState(text);
  useEffect(() => setDraft(text), [text]);
  return (
    <div className="flex flex-col gap-2 px-6 py-4">
      <FormField label="Standing orders" htmlFor={`orders-${workspaceId}`} hint="Every routine in this scope follows these. Say what Neko may do on its own and what it should leave for you.">
        <Textarea id={`orders-${workspaceId}`} rows={3} value={draft} onChange={(e) => setDraft(e.target.value)} />
      </FormField>
      {draft !== text ? (
        <div><Button size="sm" onClick={() => void send({ SetStandingOrders: { workspace_id: workspaceId, text: draft } }, "Standing orders saved")}>Save standing orders</Button></div>
      ) : null}
    </div>
  );
}

function RoutineRow({ view, now, send, onEdit }: { view: Json; now: number; send: (body: Json, done?: string) => Promise<boolean>; onEdit: () => void }) {
  const { routine, state, runs } = view;
  const last: Json | undefined = runs?.[0];
  const outcome = state.running ? <Status tone="warning">Running</Status>
    : !last ? <Status>Not run yet</Status>
    : !last.ok ? <Status tone="error">Failed</Status>
    : last.quiet ? <Status>Quiet</Status>
    : <Status tone="success">Acted</Status>;
  return (
    <div className="flex flex-col gap-2 px-6 py-4">
      <div className="flex flex-wrap items-center gap-3">
        <span className="font-medium text-foreground">{routine.name}</span>
        {outcome}
        <span className="text-[13px] text-muted-foreground">
          {cadenceText(routine.cadence)} · next {state.running ? "after this run" : relativeTime(state.next_due_ms || now, now)}
          {last ? ` · last ${relativeTime(last.finished_ms ?? last.started_ms, now)}` : ""}
        </span>
        <span className="ms-auto flex items-center gap-2">
          <Button variant="outline" size="sm" disabled={state.running} onClick={() => void send({ RunRoutineNow: { id: routine.id } }, "Starts within 30 seconds")}><Play /> Run now</Button>
          <Button variant="ghost" size="icon-sm" aria-label={`Edit ${routine.name}`} onClick={onEdit}><PencilSimple /></Button>
          <Toggle checked={routine.enabled} label={`${routine.enabled ? "Pause" : "Resume"} ${routine.name}`} onChange={(on) => void send({ SetRoutineEnabled: { id: routine.id, enabled: on } })} />
        </span>
      </div>
      {last && !last.ok && last.error ? <p className="text-[13px] text-destructive-foreground">{last.error}</p> : null}
      {state.note ? <p className="text-[13px] text-muted-foreground">{state.note}</p> : null}
    </div>
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

  if (!data) return error ? <ErrorNote error={error} onDismiss={clear} /> : <p className="text-sm text-muted-foreground">Loading routines…</p>;

  const scopes = data.scopes.filter((s) => s.workspace_id && data.routines.some((v) => v.routine.workspace_id === s.workspace_id));
  return (
    <>
      <ErrorNote error={error} onDismiss={clear} />
      <Section title="Needs you" intro={open.length ? `${open.length} thing${open.length === 1 ? "" : "s"} only you can do.` : undefined}>
        {open.length === 0 ? (
          <Panel><p className="px-6 py-4 text-[13px] text-muted-foreground">Nothing needs you. Neko handles the rest and lists it under each scope below.</p></Panel>
        ) : (
          <Panel><div className="divide-y divide-border/70">
            {open.map(({ item, run, routine }) => (
              <div key={item.seq} className="flex items-start gap-3 px-6 py-4">
                <div className="min-w-0 flex-1">
                  <p className="text-[12.5px] text-muted-foreground">{scopeName[routine.workspace_id] ?? "Scope"} · {routine.name} · {relativeTime(run.finished_ms ?? run.started_ms, now)}</p>
                  <p className="font-medium text-foreground">{item.title}</p>
                  {item.detail ? <p className="mt-0.5 max-w-[70ch] whitespace-pre-wrap text-[13px] text-muted-foreground">{item.detail}</p> : null}
                </div>
                {item.link ? <Button variant="outline" size="sm" render={<a href={item.link} target="_blank" rel="noreferrer" />}><ArrowSquareOut /> Open</Button> : null}
                <Button variant="outline" size="sm" onClick={() => void send({ ClearNeedsYou: { routine_id: routine.id, run_id: run.id, seq: item.seq } })}><Check /> Done</Button>
              </div>
            ))}
          </div></Panel>
        )}
      </Section>
      {scopes.map((scope) => {
        const views = data.routines.filter((v) => v.routine.workspace_id === scope.workspace_id);
        const did = views
          .flatMap((v) => (v.runs ?? []).filter((r: Json) => r.ok).flatMap((r: Json) => (r.did ?? []).map((line: string, i: number) => ({ line, at: r.finished_ms ?? r.started_ms, key: `${r.id}-${i}`, name: v.routine.name }))))
          .sort((a, b) => b.at - a.at)
          .slice(0, 12);
        return (
          <Section key={scope.workspace_id} title={scope.name} intro={`${(scope.folders ?? []).join(", ")}${scope.servers?.length ? ` · ${new Set(scope.servers.filter((s: Json) => s.level === "scope").map((s: Json) => s.name)).size} scope connections` : ""}`}>
            <Panel><div className="divide-y divide-border/70">
              <div className="px-6 py-4">
                <p className="mb-2 text-[12.5px] font-medium text-muted-foreground">What Neko did</p>
                {did.length === 0 ? <p className="text-[13px] text-muted-foreground">Nothing yet.</p> : (
                  <ul className="flex flex-col gap-1.5">
                    {did.map((d) => <li key={d.key} className="text-[13px] text-foreground/90"><span className="me-2 text-muted-foreground">{relativeTime(d.at, now)}</span>{d.line}</li>)}
                  </ul>
                )}
              </div>
              {views.map((v) => <RoutineRow key={v.routine.id} view={v} now={now} send={send} onEdit={() => setEditing(v.routine)} />)}
              {scope.problems?.length ? (
                <div className="px-6 py-4">
                  <p className="mb-2 text-[12.5px] font-medium text-muted-foreground">Setup notes</p>
                  <ul className="flex list-disc flex-col gap-1 ps-5 text-[13px] text-muted-foreground">{scope.problems.map((p: string) => <li key={p}>{p}</li>)}</ul>
                </div>
              ) : null}
              <StandingOrders workspaceId={scope.workspace_id} text={data.standing_orders[scope.workspace_id] ?? ""} send={send} />
            </div></Panel>
          </Section>
        );
      })}
      {scopes.length === 0 ? <EmptyBlock icon={Tray} title="No routines yet" description="Neko creates them for each scope it finds under ~/Documents." /> : null}
      <RoutineEditor routine={editing} onClose={() => setEditing(null)} send={send} />
    </>
  );
}

export default function Inbox() {
  return (
    <SetupPage eyebrow="Today" title={<>Inbox</>} subtitle="What Neko handled for you, and what only you can do.">
      <InboxBody />
    </SetupPage>
  );
}
