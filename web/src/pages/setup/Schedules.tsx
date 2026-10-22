// Schedules: Neko's scheduled planning. A schedule runs a read-only plan on a
// recurrence (a morning brief, a weekly review); each run appears as a task.
// Saving always pauses a schedule, so turning it on is a separate, deliberate step.
import { useEffect, useMemo, useState } from "react";
import type { Json } from "@/api";
import { Button } from "@/ui/button";
import { Input } from "@/ui/input";
import { Textarea } from "@/ui/textarea";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/ui/dialog";
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuSeparator, DropdownMenuTrigger } from "@/ui/dropdown-menu";
import { toast } from "@/ui/sonner";
import { Clock, DotsThree, PencilSimple, Play, Plus, Trash } from "@/ui/icons";
import { Panel } from "@/editorial";
import { navigate } from "@/router";
import {
  ConfirmDialog, EmptyBlock, ErrorNote, FormField, Gate, Pick, Section, SetupPage, Status, Toggle, registerCommands,
} from "@/setup/kit";
import { ScopePicker, useScope } from "@/setup/scope";
import { useSetupCommands } from "@/setup/commands";
import {
  RECURRENCES, WEEKDAYS, arr, parseRule, relativeTime, ruleFor, ruleSummary, schedulesCmd, str, useNow, useRun, workspaceName,
  workspacesOf, type Recurrence,
} from "@/setup/lib";

type Draft = { value: Json; workspace: string } | null;

function ScheduleEditor({ draft, onClose }: { draft: Draft; onClose: () => void }) {
  const { run, busy, error, clear } = useRun();
  const original = draft?.value ?? {};
  const isNew = !str(original.id);
  const [name, setName] = useState("");
  const [prompt, setPrompt] = useState("");
  const [kind, setKind] = useState<Recurrence>("daily");
  const [time, setTime] = useState("09:00");
  const [weekday, setWeekday] = useState("MO");
  const [interval, setInterval_] = useState(4);
  const [custom, setCustom] = useState("FREQ=DAILY;BYHOUR=9;BYMINUTE=0");
  const [timezone, setTimezone] = useState("");

  useEffect(() => {
    if (!draft) return;
    clear();
    setName(str(original.name));
    setPrompt(str(original.prompt));
    setTimezone(str(original.timezone) || Intl.DateTimeFormat().resolvedOptions().timeZone);
    const rule = str(original.rule);
    if (rule) {
      const p = parseRule(rule);
      setKind(p.kind);
      setTime(`${String(p.hour).padStart(2, "0")}:${String(p.minute).padStart(2, "0")}`);
      setWeekday(p.weekday);
      setInterval_(p.interval);
      setCustom(rule);
    } else {
      setKind("daily"); setTime("09:00"); setWeekday("MO"); setInterval_(4); setCustom("FREQ=DAILY;BYHOUR=9;BYMINUTE=0");
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [draft]);

  const [hour, minute] = time.split(":").map((n) => Number(n) || 0);
  const rule = ruleFor(kind, { hour, minute, weekday, interval, custom });
  const canSave = name.trim() && prompt.trim() && rule.trim() && timezone.trim() && !busy;
  const save = async () => {
    const saved = await run(() =>
      schedulesCmd("Save", {
        schedule: {
          ...original,
          id: str(original.id),
          name: name.trim(),
          prompt: prompt.trim(),
          workspace_id: isNew ? draft?.workspace || null : original.workspace_id ?? null,
          rule,
          timezone: timezone.trim(),
          anchor_ms: isNew ? Date.now() : original.anchor_ms,
          enabled: false,
        },
      }),
    );
    if (saved) onClose();
  };

  return (
    <Dialog open={!!draft} onOpenChange={(o) => { if (!o) onClose(); }}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>{isNew ? "New schedule" : "Edit schedule"}</DialogTitle>
          <DialogDescription>Choose what to prepare and when it should run. Saving pauses this schedule; enable it after reviewing the saved settings.</DialogDescription>
        </DialogHeader>
        <form className="flex max-h-[70vh] flex-col gap-5 overflow-y-auto px-0.5" onSubmit={(e) => { e.preventDefault(); if (canSave) void save(); }}>
          <FormField label="Name" htmlFor="sch-name">
            <Input id="sch-name" value={name} onChange={(e) => setName(e.target.value)} placeholder="Morning brief" autoComplete="off" />
          </FormField>
          <FormField label="What to prepare" htmlFor="sch-prompt">
            <Textarea id="sch-prompt" rows={4} value={prompt} onChange={(e) => setPrompt(e.target.value)} placeholder="Summarise what changed overnight and what needs me today." />
          </FormField>
          <div className="grid gap-4 sm:grid-cols-2">
            <FormField label="Repeat">
              <Pick label="Repeat" value={kind} onChange={(v) => setKind(v as Recurrence)} options={RECURRENCES.map(([value, label]) => ({ value, label }))} className="w-full" />
            </FormField>
            {kind === "hourly" ? (
              <FormField label="Every N hours" htmlFor="sch-interval">
                <Input id="sch-interval" type="number" min={1} max={168} value={interval} onChange={(e) => setInterval_(Math.max(1, Math.min(168, Number(e.target.value) || 1)))} />
              </FormField>
            ) : kind !== "custom" ? (
              <FormField label="At" htmlFor="sch-time">
                <Input id="sch-time" type="time" value={time} onChange={(e) => setTime(e.target.value || "09:00")} />
              </FormField>
            ) : null}
            {kind === "weekly" ? (
              <FormField label="Day">
                <Pick label="Day of the week" value={weekday} onChange={setWeekday} options={WEEKDAYS.map(([value, label]) => ({ value, label }))} className="w-full" />
              </FormField>
            ) : null}
          </div>
          {kind === "custom" ? (
            <FormField label="Custom recurrence rule" htmlFor="sch-custom" hint="An iCalendar-style rule, e.g. FREQ=WEEKLY;BYDAY=MO;BYHOUR=9;BYMINUTE=0. The daemon validates it when you save.">
              <Input id="sch-custom" value={custom} onChange={(e) => setCustom(e.target.value)} className="font-mono" />
            </FormField>
          ) : null}
          <FormField label="Time zone" htmlFor="sch-tz" hint={`Runs ${ruleSummary(rule).toLowerCase()} (${timezone || "no time zone"}).`}>
            <Input id="sch-tz" value={timezone} onChange={(e) => setTimezone(e.target.value)} className="font-mono" />
          </FormField>
          <ErrorNote error={error} title="Couldn’t save the schedule" />
          <DialogFooter>
            <Button type="button" variant="outline" onClick={onClose}>Cancel</Button>
            <Button type="submit" disabled={!canSave} loading={busy}>Save</Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}

function ScheduleRow({ item, onEdit, now }: { item: Json; onEdit: () => void; now: number }) {
  const { run, busy, error, clear } = useRun();
  const [confirmDelete, setConfirmDelete] = useState(false);
  const enabled = !!item.enabled;
  return (
    <div data-slot="schedule" data-enabled={enabled ? "true" : "false"} className="flex flex-col gap-3 px-5 py-4">
      <div className="flex items-start gap-3">
        <div className="flex min-w-0 flex-1 flex-col gap-1">
          <div className="flex flex-wrap items-center gap-2">
            <h3 className="text-[15px] font-medium leading-snug">{str(item.name)}</h3>
            <Status tone={enabled ? "success" : "neutral"}>{enabled ? "Enabled" : "Paused"}</Status>
          </div>
          <p className="font-mono text-[12px] text-muted-foreground">
            {ruleSummary(str(item.rule))} · {str(item.timezone)}
            {item.next_due_ms != null && enabled ? ` · next ${relativeTime(item.next_due_ms, now)}` : ""}
          </p>
        </div>
        <Toggle
          checked={enabled}
          label={`${enabled ? "Pause" : "Enable"} ${str(item.name)}`}
          disabled={busy}
          onChange={(on) => void run(() => schedulesCmd("SetEnabled", { id: item.id, enabled: on }))}
        />
        <DropdownMenu>
          <DropdownMenuTrigger render={<Button variant="ghost" size="icon-sm" aria-label={`Actions for ${str(item.name)}`} />}><DotsThree weight="bold" /></DropdownMenuTrigger>
          <DropdownMenuContent align="end">
            <DropdownMenuItem onClick={() => void run(() => schedulesCmd("RunNow", { id: item.id })).then((ok) => ok && toast.success("Queued a read-only plan. It appears as a task."))}><Play /> Run now</DropdownMenuItem>
            <DropdownMenuItem onClick={onEdit}><PencilSimple /> Edit schedule</DropdownMenuItem>
            <DropdownMenuSeparator />
            <DropdownMenuItem onClick={() => setConfirmDelete(true)} className="text-destructive-foreground"><Trash /> Delete schedule…</DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </div>
      <p className="line-clamp-3 max-w-[70ch] whitespace-pre-wrap text-[13px] leading-relaxed text-muted-foreground">{str(item.prompt)}</p>
      {str(item.last_result) ? (
        <p className="max-w-[70ch] rounded-lg border border-border bg-foreground/[0.03] px-3 py-2 text-[13px] leading-relaxed text-foreground/80">
          <span className="me-1.5 font-mono text-[11px] uppercase tracking-[0.14em] text-muted-foreground">Last run</span>
          {str(item.last_result)}
        </p>
      ) : null}
      <div className="flex gap-2">
        <Button variant="outline" size="sm" disabled={busy} onClick={() => void run(() => schedulesCmd("RunNow", { id: item.id })).then((ok) => ok && toast.success("Queued a read-only plan. It appears as a task."))}><Play /> Run now</Button>
        <Button variant="outline" size="sm" onClick={onEdit}>Edit schedule</Button>
        {str(item.last_task_id) ? <Button variant="ghost" size="sm" onClick={() => navigate("home")}>Last task</Button> : null}
      </div>
      <ErrorNote error={error} onDismiss={clear} />
      <ConfirmDialog
        open={confirmDelete}
        onOpenChange={setConfirmDelete}
        title={`Delete “${str(item.name)}”?`}
        description="Future runs stop. Tasks it already created stay."
        confirmLabel="Delete schedule"
        onConfirm={() => void run(() => schedulesCmd("Remove", { id: item.id }))}
      />
    </div>
  );
}

function SchedulesBody({ snapshot }: { snapshot: Json }) {
  const scope = useScope(snapshot);
  const now = useNow();
  const [draft, setDraft] = useState<Draft>(null);
  const workspaces = workspacesOf(snapshot);
  const items: Json[] = arr(snapshot.schedules);
  const shown = workspaces.filter((w) => !scope || w.id === scope);
  const orphans = useMemo(() => items.filter((s) => !s.workspace_id || !workspaces.some((w) => w.id === s.workspace_id)), [items, workspaces]);

  useEffect(
    () => registerCommands([{ id: "schedules.new", title: "New schedule", keywords: ["recurring", "brief", "cron"], icon: Plus, group: "Schedules", run: () => setDraft({ value: {}, workspace: scope || workspaces[0]?.id || "" }) }]),
    [scope, workspaces],
  );

  if (workspaces.length === 0) {
    return <EmptyBlock icon={Clock} title="Add a workspace first" description="Schedules belong to a workspace. Add one, then create its first schedule here.">
      <Button variant="outline" onClick={() => navigate("workspaces")}>Open Workspaces</Button>
    </EmptyBlock>;
  }
  return (
    <>
      <div className="flex items-center gap-2"><ScopePicker snapshot={snapshot} label="Show schedules for" /></div>
      {shown.map((w) => {
        const list = items.filter((s) => s.workspace_id === w.id);
        return (
          <Section
            key={w.id}
            eyebrow={list.length ? `${list.length} scheduled` : "Nothing scheduled"}
            title={w.name}
            actions={<Button variant="outline" size="sm" onClick={() => setDraft({ value: {}, workspace: w.id })}><Plus /> New schedule</Button>}
          >
            {list.length === 0 ? (
              <EmptyBlock icon={Clock} title="No schedules yet" description="Create a schedule for a morning brief, a weekly review, or another recurring task." />
            ) : (
              <Panel><div className="divide-y divide-border/70">
                {list.map((s) => <ScheduleRow key={s.id} item={s} now={now} onEdit={() => setDraft({ value: s, workspace: w.id })} />)}
              </div></Panel>
            )}
          </Section>
        );
      })}
      {!scope && orphans.length > 0 ? (
        <Section eyebrow={`${orphans.length}`} title="Without a workspace">
          <Panel><div className="divide-y divide-border/70">
            {orphans.map((s) => <ScheduleRow key={s.id} item={s} now={now} onEdit={() => setDraft({ value: s, workspace: "" })} />)}
          </div></Panel>
        </Section>
      ) : null}
      <p className="text-xs text-muted-foreground">Workspace: {workspaceName(snapshot, scope)} · times are shown in your browser’s clock.</p>
      <ScheduleEditor draft={draft} onClose={() => setDraft(null)} />
    </>
  );
}

export default function Schedules() {
  useSetupCommands();
  return (
    <SetupPage
      eyebrow="Work"
      title={<>Recurring, <em>on time.</em></>}
      subtitle="Recurring briefs and reviews. Each run appears in All tasks."
    >
      <Gate>{(snapshot) => <SchedulesBody snapshot={snapshot} />}</Gate>
    </SetupPage>
  );
}
