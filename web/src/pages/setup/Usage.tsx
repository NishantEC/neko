// Usage & machine: what the Mac can take right now (load, memory pressure, free
// disk, and whether Neko is holding or limiting new work), and what Neko's
// agents have cost. Machine health is polled; the last good reading stays on
// screen across a slow or failed poll, and the page only re-renders when a
// reading actually changed.
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { onEvent, request, type Json } from "@/api";
import { Button } from "@/ui/button";
import { Skeleton } from "@/ui/skeleton";
import { ArrowsClockwise } from "@/ui/icons";
import { Panel } from "@/setup/kit";
import { Bar, BarChart as RechartsBarChart, CartesianGrid, XAxis, YAxis } from "recharts";
import { ChartContainer, ChartTooltip, ChartTooltipContent } from "@/ui/chart";
import { BarList, EmptyBlock, ErrorNote, Gate, Row, Section, SetupPage, Status, Well } from "@/setup/kit";
import { useSetupCommands } from "@/setup/commands";
import { arr, num, rec, relativeTime, sizeLabel, str, useNow, type Tone } from "@/setup/lib";
import { Gauge } from "@phosphor-icons/react";

const POLL_MS = 5000;

// ---------------------------------------------------------------------------
// Machine health: polled, held across failures.
// ---------------------------------------------------------------------------

function useMachine() {
  const [health, setHealth] = useState<Json | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [fetchedAt, setFetchedAt] = useState(0);
  const lastAt = useRef(-1);
  const inflight = useRef<AbortController | null>(null);

  const poll = useCallback(async () => {
    inflight.current?.abort();
    const controller = new AbortController();
    inflight.current = controller;
    try {
      const reply = await request("MachineHealth", controller.signal);
      const next = reply?.MachineHealth ?? null;
      setFetchedAt(Date.now());
      setError(null);
      // Held: an unchanged reading doesn't replace the one on screen.
      if (next && next.at_ms !== lastAt.current) {
        lastAt.current = next.at_ms;
        setHealth(next);
      }
    } catch (e) {
      if (!controller.signal.aborted) setError(e instanceof Error ? e.message : String(e));
    } finally {
      if (!controller.signal.aborted) setLoading(false);
    }
  }, []);

  useEffect(() => {
    void poll();
    let timer: ReturnType<typeof setInterval> | null = setInterval(() => { if (!document.hidden) void poll(); }, POLL_MS);
    const onVisible = () => { if (!document.hidden) void poll(); };
    document.addEventListener("visibilitychange", onVisible);
    return () => {
      if (timer) clearInterval(timer);
      timer = null;
      document.removeEventListener("visibilitychange", onVisible);
      inflight.current?.abort();
    };
  }, [poll]);

  return { health, error, loading, fetchedAt, refresh: poll };
}

function machineState(h: Json): { tone: Tone; title: string; detail?: string } {
  if (str(h.holding)) return { tone: "error", title: "New work is on hold", detail: str(h.holding) };
  if (h.cap != null) {
    return {
      tone: "warning",
      title: h.night ? "Night: one task at a time" : "Memory tight: one at a time",
      detail: h.night ? "01:00–06:00: Neko runs one task at a time, and new work waits while the load is above 12." : "Memory pressure is high, so agents start one at a time until it eases.",
    };
  }
  return { tone: "success", title: `Mac OK · load ${Math.round(num(h.load))}` };
}

function MachineWells({ health }: { health: Json }) {
  const load = num(health.load);
  const cores = Math.max(1, num(health.cores));
  const loadPerCore = load / cores;
  const pressure = str(health.pressure) || "normal";
  const swapTotal = num(health.swap_total_gb);
  const disk = num(health.free_disk_gb);
  const throttle = num(health.kernel_task_cpu);
  const tone = (t: Tone) => t;
  return (
    <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4" data-slot="machine-wells">
      <Well
        label="Load"
        value={load.toFixed(1)}
        unit={`/ ${cores} cores`}
        fraction={loadPerCore}
        tone={tone(load >= 20 ? "error" : loadPerCore >= 0.85 ? "warning" : "success")}
        detail={load >= 20 ? "No new agents start above a load of 20." : `${loadPerCore.toFixed(2)} per core`}
      />
      <Well
        label="Memory pressure"
        value={pressure.charAt(0).toUpperCase() + pressure.slice(1)}
        fraction={pressure === "critical" ? 1 : pressure === "warning" ? 0.6 : 0.2}
        tone={pressure === "critical" ? "error" : pressure === "warning" ? "warning" : "success"}
        detail={`${num(health.compressed_gb).toFixed(1)} GB compressed · swap ${num(health.swap_used_gb).toFixed(1)} of ${swapTotal.toFixed(1)} GB`}
      />
      <Well
        label="Free disk"
        value={disk.toFixed(0)}
        unit="GB"
        fraction={Math.min(1, disk / 100)}
        tone={disk < 15 ? "error" : disk < 30 ? "warning" : "success"}
        detail={disk < 15 ? "Under 15 GB: nothing new starts." : "Neko holds new work under 15 GB."}
      />
      <Well
        label="Cooling"
        value={throttle >= 50 ? "Throttling" : "Normal"}
        fraction={Math.min(1, throttle / 100)}
        tone={throttle >= 50 ? "warning" : "success"}
        detail={throttle >= 50 ? `macOS is throttling (kernel_task ${throttle.toFixed(0)}%)` : `kernel_task ${throttle.toFixed(0)}%`}
      />
    </div>
  );
}

function AgentMemory({ health }: { health: Json }) {
  const agents = arr(health.agents).slice(0, 6);
  if (agents.length === 0) return <p className="text-[13px] text-muted-foreground">No agent processes are running right now.</p>;
  return (
    <Panel data-slot="agent-memory">
      <BarList
        label="Memory held by each kind of agent"
        items={agents.map((a) => ({ name: str(a.name), value: num(a.mb), text: sizeLabel(num(a.mb)), hint: `${num(a.processes)} ${num(a.processes) === 1 ? "process" : "processes"}` }))}
      />
    </Panel>
  );
}

// ---------------------------------------------------------------------------
// Spend, derived from what agents reported
// ---------------------------------------------------------------------------

interface Spend {
  total: number; week: number; runs: number; tokensIn: number; tokensOut: number;
  days: { day: string; cost: number }[];
  tasks: { id: string; title: string; cost: number; runs: number }[];
  roles: { role: string; cost: number }[];
}

function spendOf(tasks: Json[], now: number): Spend {
  const dayKey = (ms: number) => new Date(ms).toLocaleDateString("en-CA");
  const days = new Map<string, number>();
  for (let i = 13; i >= 0; i -= 1) days.set(dayKey(now - i * 86_400_000), 0);
  const result: Spend = { total: 0, week: 0, runs: 0, tokensIn: 0, tokensOut: 0, days: [], tasks: [], roles: [] };
  const roles = new Map<string, number>();
  for (const task of tasks) {
    let cost = 0, runs = 0;
    for (const event of arr(task.events)) {
      const message = str(event.message);
      if (!message.startsWith("USAGE ")) continue;
      let usage: Json;
      try { usage = JSON.parse(message.slice(6)); } catch { continue; }
      const c = num(usage.cost_usd);
      cost += c; runs += 1;
      result.tokensIn += num(usage.input_tokens);
      result.tokensOut += num(usage.output_tokens);
      roles.set(str(event.role) || "agent", (roles.get(str(event.role) || "agent") ?? 0) + c);
      const key = dayKey(num(event.at_ms));
      if (days.has(key)) days.set(key, (days.get(key) ?? 0) + c);
      if (now - num(event.at_ms) <= 7 * 86_400_000) result.week += c;
    }
    if (runs) result.tasks.push({ id: str(task.id), title: str(task.display_title) || str(task.title), cost, runs });
    result.total += cost;
    result.runs += runs;
  }
  result.days = [...days].map(([day, cost]) => ({ day: new Date(day + "T12:00:00").toLocaleDateString("en", { month: "short", day: "numeric" }), cost: Number(cost.toFixed(2)) }));
  result.tasks.sort((a, b) => b.cost - a.cost);
  result.roles = [...roles].map(([role, cost]) => ({ role, cost })).sort((a, b) => b.cost - a.cost);
  return result;
}

const usd = (n: number) => `$${n >= 100 ? n.toFixed(0) : n.toFixed(2)}`;
const compact = (n: number) => (n >= 1e6 ? `${(n / 1e6).toFixed(1)}M` : n >= 1e3 ? `${(n / 1e3).toFixed(1)}k` : String(n));

function SpendSection({ snapshot, now }: { snapshot: Json; now: number }) {
  const spend = useMemo(() => spendOf(arr(snapshot.tasks), now), [snapshot.tasks, now]);
  const budgetCents = snapshot.task_budget_cents as number | null | undefined;
  const budget = budgetCents == null ? null : budgetCents / 100;
  const top = spend.tasks[0];
  const hasData = spend.runs > 0;
  return (
    <Section eyebrow="Spend" title={<>What agents cost.</>} intro="Read from the cost each Claude Code and OpenCode run reports. Codex subscriptions don’t report a price, so their runs aren’t counted here.">
      <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4" data-slot="spend-wells">
        <Well label="Recorded spend" value={usd(spend.total)} detail={`${spend.runs} priced ${spend.runs === 1 ? "run" : "runs"} · ${compact(spend.tokensIn)} in · ${compact(spend.tokensOut)} out`} />
        <Well label="Last 7 days" value={usd(spend.week)} tone={spend.week > 0 ? "neutral" : "success"} detail={spend.week > 0 ? "Across every task" : "Nothing priced this week"} />
        <Well
          label="Per-task budget"
          value={budget == null ? "None" : usd(budget)}
          fraction={budget && top ? top.cost / budget : undefined}
          tone={budget && top && top.cost / budget >= 1 ? "error" : budget && top && top.cost / budget >= 0.8 ? "warning" : "neutral"}
          detail={budget == null ? "No limit set. Change it in Settings → AI & agents." : top ? `Costliest task so far: ${usd(top.cost)}` : "No priced task yet"}
        />
        <Well label="Tasks with a price" value={spend.tasks.length} detail={`${arr(snapshot.tasks).length} tasks in history`} />
      </div>
      {hasData ? (
        <Panel>
          <div className="px-6 pt-4">
            <p className="font-mono text-[11px] uppercase tracking-[0.14em] text-muted-foreground">Spend per day · last 14 days (USD)</p>
            <ChartContainer config={{ cost: { label: "Spend (USD)", color: "var(--chart-1)" } }} className="mt-3 aspect-auto h-44 w-full" data-slot="spend-chart">
              <RechartsBarChart accessibilityLayer data={spend.days} margin={{ top: 8, right: 4, left: 8, bottom: 0 }}>
                <CartesianGrid vertical={false} />
                <XAxis dataKey="day" tickLine={false} axisLine={false} tickMargin={8} interval={1} />
                <YAxis tickLine={false} axisLine={false} width={44} tickFormatter={(v) => `$${v}`} />
                <ChartTooltip cursor={false} content={<ChartTooltipContent formatter={(v) => `$${Number(v).toFixed(2)}`} />} />
                <Bar dataKey="cost" fill="var(--color-cost)" radius={[4, 4, 0, 0]} isAnimationActive={false} />
              </RechartsBarChart>
            </ChartContainer>
          </div>
          <div className="divide-y divide-border/70 border-t border-border/70">
            {spend.tasks.slice(0, 5).map((t) => (
              <Row key={t.id} title={<span className="line-clamp-1">{t.title}</span>} description={`${t.runs} ${t.runs === 1 ? "run" : "runs"}`} trailing={<span className="font-mono text-[13px] tabular-nums">{usd(t.cost)}</span>} />
            ))}
          </div>
        </Panel>
      ) : (
        <EmptyBlock icon={Gauge} title="No priced runs yet" description="When an agent that reports its cost finishes a run, its spend shows up here." />
      )}
    </Section>
  );
}

function ModelSection({ snapshot }: { snapshot: Json }) {
  const selections = arr(snapshot.runtime_selections);
  const counts = new Map<string, number>();
  for (const s of selections) {
    const runtime = rec(s.runtime);
    const key = `${str(runtime.model) || "default model"}${runtime.reasoning_effort ? ` · ${runtime.reasoning_effort}` : ""}`;
    counts.set(key, (counts.get(key) ?? 0) + 1);
  }
  const data = [...counts].map(([name, runs]) => ({ name, runs })).sort((a, b) => b.runs - a.runs).slice(0, 6);
  if (data.length === 0) return null;
  return (
    <Section eyebrow="Models" title="What ran" intro={`The last ${selections.length} dispatches, by model and effort.`}>
      <Panel data-slot="model-use"><BarList label="Runs by model" items={data.map((d) => ({ name: d.name, value: d.runs, text: `${d.runs} ${d.runs === 1 ? "run" : "runs"}` }))} /></Panel>
    </Section>
  );
}

// ---------------------------------------------------------------------------
// Vendor quota (when the daemon exposes it)
// ---------------------------------------------------------------------------

function QuotaSection() {
  const [rows, setRows] = useState<Json[] | null>(null);
  const [unavailable, setUnavailable] = useState<string | null>(null);
  const [quotas, setQuotas] = useState<Json[]>([]);
  const [loading, setLoading] = useState(true);

  const load = useCallback(async (signal?: AbortSignal) => {
    try {
      const reply = await request({ Search: { query: "", limit: 50, provider: "usage" } }, signal);
      setRows(arr(reply?.SearchResults?.items));
      setUnavailable(null);
    } catch (e) {
      if (!signal?.aborted) setUnavailable(e instanceof Error ? e.message : String(e));
    } finally {
      if (!signal?.aborted) setLoading(false);
    }
  }, []);
  useEffect(() => {
    const controller = new AbortController();
    void load(controller.signal);
    const off = onEvent((event) => { if (event?.QuotaChanged) setQuotas(arr(event.QuotaChanged.quotas)); });
    return () => { controller.abort(); off(); };
  }, [load]);

  const meters = arr(rows).filter((r) => r.meter);
  return (
    <Section eyebrow="Vendors" title="Quota windows" intro="What each signed-in vendor has left, read from its own API.">
      {loading ? <Skeleton className="h-28 w-full" /> : meters.length > 0 || quotas.length > 0 ? (
        <div className="grid gap-4 sm:grid-cols-2">
          {quotas.map((q) => (
            <Well key={str(q.vendor)} label={str(q.label)} value={q.percent_left == null ? "Unknown" : `${q.percent_left}%`} unit={q.percent_left == null ? undefined : "left"} fraction={q.percent_left == null ? undefined : 1 - q.percent_left / 100} tone={q.percent_left == null ? "neutral" : q.percent_left < 10 ? "error" : q.percent_left < 30 ? "warning" : "success"} />
          ))}
          {meters.map((r) => {
            const stats = arr(r.meter.stats);
            return <Well key={str(r.id)} label={`${str(r.group_label) ? `${str(r.group_label)} · ` : ""}${str(r.title)}`} value={str(stats[0]?.value) || `${Math.round(num(r.meter.fraction) * 100)}%`} unit={str(stats[0]?.label)} fraction={num(r.meter.fraction)} tone={num(r.meter.fraction) >= 0.9 ? "error" : num(r.meter.fraction) >= 0.7 ? "warning" : "success"} detail={[str(r.subtitle), ...stats.slice(1).map((s) => `${s.value} ${s.label}`)].filter(Boolean).join(" · ")} />;
          })}
        </div>
      ) : (
        <EmptyBlock icon={Gauge} title="Quota isn’t reported by this daemon" description={unavailable ? "Vendor quota windows come from the legacy palette providers (NEKO_LEGACY_AGENTS=1); the standalone workspace doesn’t read them, so there is nothing to show here yet." : "No vendor is signed in, or none has a metered window."}>
          <Button variant="outline" size="sm" onClick={() => { setLoading(true); void load(); }}><ArrowsClockwise /> Check again</Button>
        </EmptyBlock>
      )}
    </Section>
  );
}

// ---------------------------------------------------------------------------

function UsageBody({ snapshot }: { snapshot: Json }) {
  const machine = useMachine();
  const now = useNow(1000);
  const state = machine.health ? machineState(machine.health) : null;
  const health = machine.health;
  return (
    <>
      <Section
        eyebrow="This Mac"
        title="What the Mac can take"
        intro="Neko reads the machine every 15 seconds and decides whether new agent work may start. Running work is never touched."
        actions={
          <>
            <span className="font-mono text-[11px] text-muted-foreground" data-slot="updated">
              {machine.fetchedAt ? `Updated ${Math.max(0, Math.round((now - machine.fetchedAt) / 1000))}s ago` : "Reading…"}
            </span>
            <Button variant="outline" size="sm" onClick={() => void machine.refresh()}><ArrowsClockwise /> Refresh</Button>
          </>
        }
      >
        {machine.error ? <ErrorNote title={health ? "Showing the last reading" : "Couldn’t read the machine"} error={machine.error} /> : null}
        {machine.loading && !health ? (
          <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">{[0, 1, 2, 3].map((i) => <Skeleton key={i} className="h-32" />)}</div>
        ) : !health ? (
          <EmptyBlock icon={Gauge} title="No reading yet" description="The daemon hasn’t sampled the machine. It does so every 15 seconds while it runs." />
        ) : (
          <>
            <div className="flex flex-wrap items-center gap-3 rounded-xl border border-border bg-card px-4 py-3" data-slot="machine-state" data-tone={state?.tone}>
              <Status tone={state!.tone}>{state!.title}</Status>
              <span className="text-[13px] text-muted-foreground">
                {state!.detail ?? (health.night ? "Night hours: one heavy job at a time." : "New agent work starts normally.")}
                {num(health.slowed) > 0 ? ` Slowed ${num(health.slowed)} build and test ${num(health.slowed) === 1 ? "process" : "processes"}; nothing was stopped.` : ""}
              </span>
              <span className="ms-auto font-mono text-[11px] text-muted-foreground">read {relativeTime(num(health.at_ms), now)}</span>
            </div>
            <MachineWells health={health} />
            <div className="flex flex-col gap-3">
              <p className="font-mono text-[11px] uppercase tracking-[0.14em] text-muted-foreground">Memory held by agents</p>
              <AgentMemory health={health} />
            </div>
          </>
        )}
      </Section>
      <SpendSection snapshot={snapshot} now={now} />
      <ModelSection snapshot={snapshot} />
      <QuotaSection />
    </>
  );
}

export default function Usage() {
  useSetupCommands();
  return (
    <SetupPage
      eyebrow="System"
      title={<>Usage &amp; machine</>}
      subtitle="What the Mac can take right now, and what Neko’s agents have cost."
      width="max-w-[980px]"
    >
      <Gate>{(snapshot) => <UsageBody snapshot={snapshot} />}</Gate>
    </SetupPage>
  );
}
