// AI & agents: the default runtime and model for new work, the Home chat's
// model pins (provider+model, effort, speed — each independently Auto), the
// budget and concurrency limits, and how much a ticket does before asking.
import { useEffect, useMemo, useState } from "react";
import { command, request, type Json } from "@/api";
import { Button } from "@/ui/button";
import { Input } from "@/ui/input";
import { Skeleton } from "@/ui/skeleton";
import { CheckCircle, ArrowsClockwise, WarningCircle, XCircle } from "@/ui/icons";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/ui/collapsible";
import { Panel } from "@/setup/kit";
import { useWorkbench } from "@/useStore";
import { ConfirmDialog, ErrorNote, PanelFooter, Pick, Section, SettingRow, Status, Toggle, type Option } from "@/setup/kit";
import { arr, num, profileFor, relativeTime, str, useLoad, useRun } from "@/setup/lib";

const SEP = "\u001f";

interface Catalog { sources: Json[]; read_at_ms: number }

function useCatalog() {
  const loaded = useLoad<Catalog>(async (signal) => {
    const reply = await request({ AgentModels: { refresh: false } }, signal);
    return reply?.AgentModels ?? { sources: [], read_at_ms: 0 };
  });
  const [refreshing, setRefreshing] = useState(false);
  const [refreshError, setRefreshError] = useState<string | null>(null);
  const refresh = async () => {
    setRefreshing(true);
    setRefreshError(null);
    try {
      const reply = await request({ AgentModels: { refresh: true } });
      loaded.setData(reply?.AgentModels ?? { sources: [], read_at_ms: 0 });
    } catch (e) {
      setRefreshError(e instanceof Error ? e.message : String(e));
    } finally {
      setRefreshing(false);
    }
  };
  return { ...loaded, refresh, refreshing, refreshError };
}

const usable = (model: Json) => model.access !== "unavailable";
const modelTitle = (m: Json) => `${str(m.label) || str(m.id)}${m.recommended ? " · Recommended" : ""}${m.access === "checked" ? " · Checked" : ""}${usable(m) ? "" : " · Unavailable"}`;
const effortOptions = (model: Json | undefined): Json[] =>
  model ? (Array.isArray(model.effort_options) ? model.effort_options : arr(model.reasoning_efforts).map((id) => ({ id, label: str(id) }))) : [];
const speedOptions = (model: Json | undefined): Json[] => (model && Array.isArray(model.speed_options) ? model.speed_options : []);
const cap = (s: string) => s.charAt(0).toUpperCase() + s.slice(1);

function RuntimePanel({ snapshot, catalog }: { snapshot: Json; catalog: ReturnType<typeof useCatalog> }) {
  const saved = snapshot.agent_runtime ?? {};
  const sources = arr(catalog.data?.sources);
  const [provider, setProvider] = useState(str(saved.provider) || "codex");
  const [model, setModel] = useState(str(saved.model));
  const [effort, setEffort] = useState(str(saved.reasoning_effort));
  const [tier, setTier] = useState(str(saved.service_tier));
  const [custom, setCustom] = useState("");
  const [check, setCheck] = useState<{ ok: boolean; unavailable: boolean; message: string } | null>(null);
  const { run, busy, error, clear } = useRun();
  const [checking, setChecking] = useState(false);

  // Follow what the daemon saved, unless this form has been edited.
  const savedKey = `${str(saved.provider)}|${str(saved.model)}|${str(saved.reasoning_effort)}|${str(saved.service_tier)}`;
  useEffect(() => {
    setProvider(str(saved.provider) || "codex");
    setModel(str(saved.model));
    setEffort(str(saved.reasoning_effort));
    setTier(str(saved.service_tier));
  }, [savedKey]); // eslint-disable-line react-hooks/exhaustive-deps

  const source = sources.find((s) => s.provider === provider);
  const selected = arr(source?.models).find((m) => m.id === model);
  const ready = source ? source.status === "ready" : true;
  const efforts = effortOptions(selected);
  const speeds = speedOptions(selected);
  const dirty = provider !== (str(saved.provider) || "codex") || model !== str(saved.model) || effort !== str(saved.reasoning_effort) || tier !== str(saved.service_tier);
  const blocked = !ready || (!!selected && !usable(selected));

  const runtime = { provider, model: model.trim(), reasoning_effort: effort || null, service_tier: tier || null };
  const modelOptions: Option[] = [
    { value: "", label: source?.default_model ? `Default · ${source.default_model}` : "Default model" },
    ...arr(source?.models).map((m) => ({ value: str(m.id), label: modelTitle(m) })),
    ...(model && !arr(source?.models).some((m) => m.id === model) ? [{ value: model, label: model }] : []),
  ];
  const validId = (id: string) => id.length > 0 && id.length <= 120 && !id.startsWith("-") && /^[A-Za-z0-9.\-_:/@[\]]+$/.test(id);

  const checkAndUse = async () => {
    setChecking(true);
    setCheck(null);
    try {
      const reply = await request({ CheckAgentModel: { runtime, save: true } });
      setCheck(reply?.AgentModelCheck ?? null);
      catalog.reload();
    } catch (e) {
      setCheck({ ok: false, unavailable: false, message: e instanceof Error ? e.message : String(e) });
    } finally {
      setChecking(false);
    }
  };

  return (
    <Panel data-slot="runtime">
      <div className="divide-y divide-border/70">
        <SettingRow id="runtime" title="Runs with" description="The runtime that does the work for new conversations, tasks and background checks. Work already running isn’t interrupted.">
          {catalog.loading && !catalog.data ? <Skeleton className="h-8 w-44" /> : (
            <Pick
              label="Agent runtime"
              value={provider}
              onChange={(v) => { setProvider(v); setModel(""); setEffort(""); setTier(""); setCheck(null); }}
              options={sources.length ? sources.map((s) => ({ value: str(s.provider), label: str(s.label) })) : [{ value: "codex", label: "Codex" }]}
            />
          )}
        </SettingRow>
        {source ? (
          <div className="flex flex-wrap items-center justify-between gap-3 px-6 py-3.5 text-[13px]">
            <span className="text-muted-foreground">Connection</span>
            <span className="flex items-center gap-2">
              <Status tone={ready ? "success" : "warning"}>{str(source.connection) || (ready ? "Ready" : "Not ready")}</Status>
              {str(source.note) ? <span className="max-w-[42ch] text-muted-foreground">{str(source.note)}</span> : null}
            </span>
          </div>
        ) : null}
        <SettingRow id="model" title="Model" description={selected ? str(selected.reason) || str(selected.description) || undefined : "Leave on Default to let the runtime choose."}>
          <Pick label="Model" value={model} onChange={(v) => { setModel(v); setCheck(null); }} options={modelOptions} className="min-w-56" />
        </SettingRow>
        <SettingRow id="effort" title="Reasoning effort" description={efforts.length ? "Only the levels this model advertises." : "This model doesn’t advertise effort levels."}>
          <Pick
            label="Reasoning effort"
            value={effort || "__auto"}
            onChange={(v) => setEffort(v === "__auto" ? "" : v)}
            options={[{ value: "__auto", label: "Auto" }, ...efforts.map((o) => ({ value: str(o.id), label: str(o.label) || cap(str(o.id)) })), ...(effort && !efforts.some((o) => o.id === effort) ? [{ value: effort, label: `${effort} (unavailable)` }] : [])]}
            disabled={efforts.length === 0 && !effort}
          />
        </SettingRow>
        <SettingRow id="speed" title="Speed" description={speeds.length ? "Extra speed may use more allowance." : "This model doesn’t advertise a faster tier."}>
          <Pick
            label="Speed"
            value={tier || "__auto"}
            onChange={(v) => setTier(v === "__auto" ? "" : v)}
            options={[{ value: "__auto", label: "Auto" }, { value: "default", label: "Normal" }, ...speeds.map((o) => ({ value: str(o.id), label: str(o.label) || cap(str(o.id)) }))]}
            disabled={speeds.length === 0 && !tier}
          />
        </SettingRow>
        <Collapsible>
          <SettingRow id="custom-model" title="Exact model ID" description="For a model that isn’t in the list." stacked>
            <CollapsibleTrigger render={<Button variant="outline" size="sm" className="w-fit" />}>Use an exact model ID</CollapsibleTrigger>
          </SettingRow>
          <CollapsibleContent>
            <div className="flex items-center gap-2 px-6 pb-4">
              <Input aria-label="Exact model ID" value={custom} onChange={(e) => setCustom(e.target.value)} placeholder="Or enter an exact model ID" className="font-mono" />
              <Button variant="outline" className="h-10" disabled={!validId(custom.trim())} onClick={() => { setModel(custom.trim()); setCheck(null); }}>Use ID</Button>
            </div>
          </CollapsibleContent>
        </Collapsible>
      </div>
      {check ? (
        <div className="flex items-start gap-2 border-t border-border/70 px-6 py-3.5 text-[13px]" role="status">
          {check.ok ? <CheckCircle className="mt-0.5 size-4 text-success" /> : check.unavailable ? <WarningCircle className="mt-0.5 size-4 text-warning-foreground" /> : <XCircle className="mt-0.5 size-4 text-destructive" />}
          <span className="select-text">{check.message}</span>
        </div>
      ) : null}
      {error || catalog.error || catalog.refreshError ? <div className="border-t border-border/70 p-4"><ErrorNote error={error ?? catalog.error ?? catalog.refreshError} title="Models" onDismiss={clear} /></div> : null}
      <PanelFooter className="justify-between">
        <span className="text-xs text-muted-foreground">
          {catalog.data?.read_at_ms ? `Catalog read ${relativeTime(catalog.data.read_at_ms)}.` : "Reading models from your runtimes…"} Checking sends one short reply, so it may use quota; a failed check keeps your current model.
        </span>
        <div className="flex flex-wrap gap-2">
          <Button variant="ghost" size="sm" onClick={() => void catalog.refresh()} loading={catalog.refreshing}><ArrowsClockwise /> Refresh models</Button>
          <Button variant="outline" size="sm" disabled={busy || checking || blocked || !dirty} onClick={() => void run(() => command("SetAgentRuntime", { runtime }))}>Use without checking</Button>
          <Button size="sm" disabled={busy || checking || blocked} loading={checking} onClick={() => void checkAndUse()}>Check &amp; use model</Button>
        </div>
      </PanelFooter>
    </Panel>
  );
}

// ---------------------------------------------------------------------------
// Home chat pins
// ---------------------------------------------------------------------------

function PinsPanel({ snapshot, catalog }: { snapshot: Json; catalog: ReturnType<typeof useCatalog> }) {
  const profile = profileFor(snapshot, null);
  const conversationId = `home:${profile}:*`;
  const saved = snapshot.conversation_runtime?.[conversationId] ?? {};
  const savedKey = JSON.stringify(saved);
  const [pin, setPin] = useState("");
  const [effort, setEffort] = useState("");
  const [tier, setTier] = useState("");
  const [paid, setPaid] = useState(false);
  const { run, busy, error, clear } = useRun();
  useEffect(() => {
    setPin(saved.provider && saved.model ? `${saved.provider}${SEP}${saved.model}` : "");
    setEffort(str(saved.reasoning_effort));
    setTier(str(saved.service_tier));
    setPaid(!!saved.allow_paid_speed);
  }, [savedKey]); // eslint-disable-line react-hooks/exhaustive-deps

  const sources = arr(catalog.data?.sources);
  const all = useMemo(() => sources.filter((s) => s.status === "ready").flatMap((s) => arr(s.models).filter(usable).map((m) => ({ source: s, model: m }))), [sources]);
  const [pinProvider, pinModel] = pin ? pin.split(SEP) : [null, null];
  const pinned = all.find((x) => x.source.provider === pinProvider && x.model.id === pinModel)?.model;
  const efforts = pinned ? effortOptions(pinned) : Array.from(new Map(all.flatMap((x) => effortOptions(x.model)).map((o) => [o.id, o])).values());
  const speeds = pinned ? speedOptions(pinned) : Array.from(new Map(all.flatMap((x) => speedOptions(x.model)).map((o) => [o.id, o])).values());
  const dirty = pin !== (saved.provider && saved.model ? `${saved.provider}${SEP}${saved.model}` : "") || effort !== str(saved.reasoning_effort) || tier !== str(saved.service_tier) || paid !== !!saved.allow_paid_speed;
  const automatic = !pin && !effort && !tier;
  const last = [...arr(snapshot.runtime_selections)].reverse().find((r) => r.conversation_id === conversationId);

  const apply = (next: { provider: string | null; model: string | null; reasoning_effort: string | null; service_tier: string | null; allow_paid_speed: boolean }) =>
    run(() => command("SetConversationRuntime", { conversation_id: conversationId, preferences: next }));

  const selectPin = (value: string) => {
    setPin(value);
    // A model change drops only the pins it can’t honour.
    if (value) {
      const [p, m] = value.split(SEP);
      const model = all.find((x) => x.source.provider === p && x.model.id === m)?.model;
      if (effort && !effortOptions(model).some((o) => o.id === effort)) setEffort("");
      if (tier && tier !== "default" && !speedOptions(model).some((o) => o.id === tier)) setTier("");
    }
  };

  return (
    <Panel data-slot="pins">
      <div className="divide-y divide-border/70">
        <SettingRow id="pins" title="Model" description="Neko decides picks a compatible model for each message, keeping any pins you set. Pinning a model pins its provider too.">
          <Pick
            label="Home chat model"
            value={pin}
            onChange={selectPin}
            options={[{ value: "", label: "Neko decides" }, ...all.map((x) => ({ value: `${x.source.provider}${SEP}${x.model.id}`, label: `${str(x.source.label)} · ${str(x.model.label) || str(x.model.id)}` }))]}
            className="min-w-60"
          />
        </SettingRow>
        <SettingRow title="Effort" description={efforts.length ? undefined : "No connected model advertises effort levels."}>
          <Pick
            label="Home chat reasoning effort"
            value={effort || "__auto"}
            onChange={(v) => setEffort(v === "__auto" ? "" : v)}
            options={[{ value: "__auto", label: "Auto" }, ...efforts.map((o) => ({ value: str(o.id), label: str(o.label) || cap(str(o.id)) }))]}
          />
        </SettingRow>
        <SettingRow title="Speed" description="Extra speed may use more allowance.">
          <Pick
            label="Home chat speed"
            value={tier || "__auto"}
            onChange={(v) => setTier(v === "__auto" ? "" : v)}
            options={[{ value: "__auto", label: "Auto" }, { value: "default", label: "Normal" }, ...speeds.map((o) => ({ value: str(o.id), label: str(o.label) || cap(str(o.id)) }))]}
          />
        </SettingRow>
        <SettingRow title="Allow automatic paid speed" description="Off by default. When speed is Auto, lets Neko choose a tier that may use more allowance.">
          <Toggle checked={paid} onChange={setPaid} label="Allow automatic paid speed" />
        </SettingRow>
        {last ? (
          <div className="px-6 py-3.5 text-[13px] text-muted-foreground">
            <span className="text-[12.5px] font-medium">Last run</span>{" "}
            {str(last.runtime?.model) || "default model"}{last.runtime?.reasoning_effort ? ` · ${last.runtime.reasoning_effort} effort` : ""}
            {last.automatic ? " · chosen by Neko" : " · pinned"} — {str(last.reason)}
          </div>
        ) : null}
      </div>
      {error ? <div className="border-t border-border/70 p-4"><ErrorNote error={error} title="Couldn’t apply the pins" onDismiss={clear} /></div> : null}
      <PanelFooter className="justify-between">
        <span className="text-xs text-muted-foreground">{automatic && !paid ? "Neko decides everything." : "Some choices are pinned."} Applies to your next message.</span>
        <div className="flex gap-2">
          <Button variant="ghost" size="sm" disabled={busy || (automatic && !paid && !saved.model)} onClick={() => void apply({ provider: null, model: null, reasoning_effort: null, service_tier: null, allow_paid_speed: false })}>Reset to Neko decides</Button>
          <Button size="sm" disabled={busy || !dirty} loading={busy} onClick={() => void apply({ provider: pinProvider, model: pinModel, reasoning_effort: effort || null, service_tier: tier || null, allow_paid_speed: paid })}>Apply</Button>
        </div>
      </PanelFooter>
    </Panel>
  );
}

// ---------------------------------------------------------------------------
// Budget and limits
// ---------------------------------------------------------------------------

function BudgetPanel({ snapshot }: { snapshot: Json }) {
  const cents = snapshot.task_budget_cents;
  const savedText = cents == null ? "" : (num(cents) / 100).toFixed(2);
  const [text, setText] = useState(savedText);
  useEffect(() => setText(savedText), [savedText]);
  const { run, busy, error, clear } = useRun();
  const clean = text.trim().replace(/^\$/, "");
  const dollars = clean === "" ? null : Number(clean);
  const invalid = dollars !== null && !(Number.isFinite(dollars) && dollars >= 0.01 && dollars <= 1000);
  const limit = (name: string, value: string) => run(() => command(name, { limit: value === "0" ? null : Number(value) }));
  const options = (values: number[], current: number | null | undefined, zeroLabel: string): Option[] => [
    { value: "0", label: zeroLabel },
    ...Array.from(new Set([...values, ...(current ? [current] : [])])).sort((a, b) => a - b).map((n) => ({ value: String(n), label: String(n) })),
  ];
  return (
    <Panel data-slot="budget">
      <div className="divide-y divide-border/70">
        <SettingRow id="budget" title="Stop a task after" description="Counts the cost Claude Code and OpenCode report for each task and stops it when it passes this amount. Codex subscriptions don’t report a price, so they aren’t limited here. Leave empty for no limit.">
          <form className="flex items-center gap-2" onSubmit={(e) => { e.preventDefault(); if (!invalid) void run(() => command("SetTaskBudget", { cents: dollars === null ? null : Math.round(dollars * 100) })); }}>
            <span className="font-mono text-[13px] text-muted-foreground">US$</span>
            <Input aria-label="Task budget in US dollars" inputMode="decimal" value={text} onChange={(e) => setText(e.target.value)} placeholder="No limit" aria-invalid={invalid} className="h-8 w-28 font-mono" />
            <Button type="submit" size="sm" variant="outline" disabled={busy || invalid || clean === (savedText === "" ? "" : savedText)}>Save budget</Button>
          </form>
        </SettingRow>
        {invalid ? <p className="px-6 py-3 text-[13px] text-warning-foreground">Enter an amount between 0.01 and 1000, or leave it empty.</p> : null}
        <SettingRow id="tickets-at-once" title="Tasks at once" description="Other tasks wait their turn and start as running ones finish. Work already running is not stopped.">
          <Pick label="Tasks at once" value={String(snapshot.max_running_tickets ?? 0)} onChange={(v) => void limit("SetMaxRunningTickets", v)} options={options([1, 2, 3, 5], snapshot.max_running_tickets, "No limit")} disabled={busy} />
        </SettingRow>
        <SettingRow id="writable-per-repo" title="Writing per repository" description="Each writing task has its own copy of the repository with its own installs and builds. Extra tasks in the same repository wait; planning and questions are not limited.">
          <Pick label="Writing per repository" value={String(snapshot.max_writable_per_repository ?? 0)} onChange={(v) => void limit("SetMaxWritablePerRepository", v)} options={options([1, 2, 3, 6, 8], snapshot.max_writable_per_repository, "4 (default)")} disabled={busy} />
        </SettingRow>
      </div>
      {error ? <div className="border-t border-border/70 p-4"><ErrorNote error={error} title="Couldn’t save" onDismiss={clear} /></div> : null}
    </Panel>
  );
}

function WorkStylePanel({ snapshot }: { snapshot: Json }) {
  const { run, busy, error, clear } = useRun();
  const [confirm, setConfirm] = useState(false);
  return (
    <Panel data-slot="work-style">
      <div className="divide-y divide-border/70">
        <SettingRow id="plan-first" title="Plan before working" description="Off: a ticket is a normal session that writes code right away, and the agent stops for approval only on critical work. On: every ticket first gets a read-only plan to approve.">
          <Toggle checked={!!snapshot.plan_first} label="Plan before working" disabled={busy} onChange={(enabled) => void run(() => command("SetPlanFirst", { enabled }))} />
        </SettingRow>
        <SettingRow title="Start without approval" description="When plans are required, a planned chat or manual ticket starts building without waiting. Nothing is ever published.">
          <Toggle checked={!!snapshot.start_without_approval} label="Start without approval" disabled={busy} onChange={(enabled) => void run(() => command("SetStartWithoutApproval", { enabled }))} />
        </SettingRow>
        <SettingRow id="start-over" title="Start over" description="Removes every task except those with a pull request, and lets your watches file their issues again from scratch. Memory, settings, workspaces, tools and watches stay.">
          <Button variant="destructive-outline" size="sm" onClick={() => setConfirm(true)}>Start over…</Button>
        </SettingRow>
      </div>
      {error ? <div className="border-t border-border/70 p-4"><ErrorNote error={error} title="Couldn’t update" onDismiss={clear} /></div> : null}
      <ConfirmDialog
        open={confirm}
        onOpenChange={setConfirm}
        title="Start over?"
        description="Tasks without a pull request are removed, along with their worktrees and unpublished changes. Tasks with a pull request stay. This can’t be undone."
        confirmLabel="Remove tasks and start over"
        onConfirm={() => void run(() => command("StartOver", { keep_published: true }))}
      />
    </Panel>
  );
}

export function AiSection() {
  const { snapshot } = useWorkbench();
  const catalog = useCatalog();
  if (!snapshot) return null;
  return (
    <div className="flex flex-col gap-8">
      <Section eyebrow="Runtime" title={<>Agent runtime.</>} intro="Choose the runtime and model for new work.">
        <RuntimePanel snapshot={snapshot} catalog={catalog} />
      </Section>
      <Section eyebrow="Home chat" title="Model pins" intro="Each pin is independent; anything left on Auto is chosen by Neko at the next run.">
        <PinsPanel snapshot={snapshot} catalog={catalog} />
      </Section>
      <Section eyebrow="Budget" title="Budget & limits">
        <BudgetPanel snapshot={snapshot} />
      </Section>
      <Section eyebrow="Work style" title="How work starts">
        <WorkStylePanel snapshot={snapshot} />
      </Section>
    </div>
  );
}
