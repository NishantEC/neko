// Shared helpers for the setup pages (Workspaces, Watching, Schedules, Tools,
// Memory, Profiles, Settings, Usage). Data comes from the workbench snapshot;
// every write goes through `useRun`, which shows the daemon's own error.
import { useCallback, useEffect, useRef, useState } from "react";
import { accept, command, request, type Json } from "@/api";

export const arr = (v: Json): Json[] => (Array.isArray(v) ? v : []);
export const str = (v: Json): string => (typeof v === "string" ? v : "");
export const num = (v: Json): number => (typeof v === "number" && Number.isFinite(v) ? v : 0);
export const rec = (v: Json): Record<string, Json> => (v && typeof v === "object" && !Array.isArray(v) ? v : {});

/** "3 minutes ago", "in 2 hours", "Never". */
export function relativeTime(ms: number | null | undefined, now = Date.now()): string {
  if (!ms || ms <= 0) return "Never";
  const diff = ms - now;
  const abs = Math.abs(diff);
  if (abs < 45_000) return diff < 0 ? "just now" : "in a few seconds";
  const formatter = new Intl.RelativeTimeFormat("en", { numeric: "auto" });
  const steps: [number, Intl.RelativeTimeFormatUnit][] = [
    [365 * 86_400_000, "year"], [30 * 86_400_000, "month"], [86_400_000, "day"], [3_600_000, "hour"], [60_000, "minute"],
  ];
  const [size, unit] = steps.find(([s]) => abs >= s) ?? steps[steps.length - 1];
  return formatter.format(Math.round(diff / size), unit);
}

export function useNow(intervalMs = 30_000) {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const id = setInterval(() => setNow(Date.now()), intervalMs);
    return () => clearInterval(id);
  }, [intervalMs]);
  return now;
}

/**
 * Runs writes one at a time. A Workbench command's new snapshot is handed to
 * the store at once; a failure keeps the daemon's message, verbatim.
 */
export function useRun() {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const alive = useRef(true);
  /** The newest failure, readable straight after `await run(...)`. */
  const lastError = useRef<string | null>(null);
  useEffect(() => {
    alive.current = true;
    return () => { alive.current = false; };
  }, []);
  const run = useCallback(async (step: () => Promise<Json>): Promise<boolean> => {
    setBusy(true);
    setError(null);
    lastError.current = null;
    try {
      accept(await step());
      return true;
    } catch (e) {
      lastError.current = e instanceof Error ? e.message : String(e);
      if (alive.current) setError(lastError.current);
      return false;
    } finally {
      if (alive.current) setBusy(false);
    }
  }, []);
  return { run, busy, error, lastError, setError, clear: () => setError(null) };
}

/** `mcp("Wake", { responsibility_id })` — the same command the native app sends. */
export const mcp = (name: string, fields: Record<string, unknown>) => command("Mcp", { [name]: fields });
export const schedulesCmd = (name: string, fields: Record<string, unknown>) => command("Schedules", { [name]: fields });
export const profilesCmd = (name: string, fields: Record<string, unknown>) => command("AgentProfiles", { [name]: fields });
export const skillsCmd = (name: string, fields?: Record<string, unknown>) => command("Skills", fields ? { [name]: fields } : undefined);
export const decisionCmd = (name: string, fields: Record<string, unknown>) => command("DecisionContext", { [name]: fields });

/** A request that is not a workbench command, loaded once and on demand. */
export function useLoad<T>(load: (signal: AbortSignal) => Promise<T>, deps: unknown[] = []) {
  const [data, setData] = useState<T | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [tick, setTick] = useState(0);
  useEffect(() => {
    const controller = new AbortController();
    setLoading(true);
    load(controller.signal)
      .then((value) => {
        if (controller.signal.aborted) return;
        setData(value);
        setError(null);
      })
      .catch((e) => { if (!controller.signal.aborted) setError(e instanceof Error ? e.message : String(e)); })
      .finally(() => { if (!controller.signal.aborted) setLoading(false); });
    return () => controller.abort();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [...deps, tick]);
  return { data, error, loading, reload: () => setTick((t) => t + 1), setData };
}

/** Provider rows the native Settings pages read (`Search` scoped to one provider). */
export async function providerRows(provider: string, signal?: AbortSignal): Promise<Json[]> {
  const reply = await request({ Search: { query: "", limit: 200, provider } }, signal);
  return arr(reply?.SearchResults?.items);
}

export const activate = (kind: string, id: string, action: string | null = null) =>
  request({ Activate: { kind, id, action, query: "" } });

// ---------------------------------------------------------------------------
// Snapshot views
// ---------------------------------------------------------------------------

export interface Workspace { id: string; name: string; repository: string; instructions: string; away_enabled: boolean }

export const workspacesOf = (snapshot: Json): Workspace[] => arr(snapshot?.workspaces);
export const foldersOf = (snapshot: Json, workspace: Workspace): string[] => {
  const folders = arr(snapshot?.workspace_folders?.[workspace.id]).map(str).filter(Boolean);
  return folders.length ? folders : workspace.repository ? [workspace.repository] : [];
};
export const workspaceName = (snapshot: Json, id: string | null | undefined, fallback = "All workspaces") =>
  id ? workspacesOf(snapshot).find((w) => w.id === id)?.name ?? "Unavailable workspace" : fallback;

export function profileFor(snapshot: Json, workspaceId?: string | null): string {
  const profiles = snapshot?.agent_profiles;
  if (workspaceId) {
    return arr(profiles?.assignments).find((a) => a.workspace_id === workspaceId)?.profile_id ?? "default";
  }
  return str(profiles?.active_profile_id) || "default";
}

/** A watch's short name, or its instruction until Neko has named it. */
export const watchTitle = (item: Json): string => str(item?.name).trim() || str(item?.instruction);

// ---------------------------------------------------------------------------
// Schedules: the same recurrence rules the native editor writes.
// ---------------------------------------------------------------------------

export const WEEKDAYS: [string, string][] = [
  ["MO", "Monday"], ["TU", "Tuesday"], ["WE", "Wednesday"], ["TH", "Thursday"], ["FR", "Friday"], ["SA", "Saturday"], ["SU", "Sunday"],
];
export type Recurrence = "daily" | "weekdays" | "weekly" | "hourly" | "custom";
export const RECURRENCES: [Recurrence, string][] = [
  ["daily", "Daily"], ["weekdays", "Weekdays"], ["weekly", "Weekly"], ["hourly", "Every N hours"], ["custom", "Custom"],
];

export function ruleFor(kind: Recurrence, o: { hour: number; minute: number; weekday: string; interval: number; custom: string }): string {
  switch (kind) {
    case "daily": return `FREQ=DAILY;BYHOUR=${o.hour};BYMINUTE=${o.minute}`;
    case "weekdays": return `FREQ=WEEKLY;BYDAY=MO,TU,WE,TH,FR;BYHOUR=${o.hour};BYMINUTE=${o.minute}`;
    case "weekly": return `FREQ=WEEKLY;BYDAY=${o.weekday};BYHOUR=${o.hour};BYMINUTE=${o.minute}`;
    case "hourly": return `FREQ=HOURLY;INTERVAL=${o.interval}`;
    default: return o.custom;
  }
}

/** Parses a rule this UI wrote back into its pieces; anything else is "custom". */
export function parseRule(rule: string): { kind: Recurrence; hour: number; minute: number; weekday: string; interval: number } {
  const base = { kind: "custom" as Recurrence, hour: 9, minute: 0, weekday: "MO", interval: 4 };
  const pairs = rule.split(";").map((p) => p.split("="));
  if (!pairs.every((p) => p.length === 2) || new Set(pairs.map((p) => p[0])).size !== pairs.length) return base;
  const f = Object.fromEntries(pairs as [string, string][]);
  const n = Object.keys(f).length;
  if (f.FREQ === "HOURLY" && n === 2 && /^\d+$/.test(f.INTERVAL ?? "")) return { ...base, kind: "hourly", interval: Number(f.INTERVAL) };
  const hour = Number(f.BYHOUR), minute = Number(f.BYMINUTE);
  if (!(Number.isInteger(hour) && hour >= 0 && hour <= 23 && Number.isInteger(minute) && minute >= 0 && minute <= 59)) return base;
  if (f.FREQ === "DAILY" && n === 3) return { ...base, kind: "daily", hour, minute };
  if (f.FREQ === "WEEKLY" && n === 4) {
    if (f.BYDAY === "MO,TU,WE,TH,FR") return { ...base, kind: "weekdays", hour, minute };
    if (WEEKDAYS.some(([code]) => code === f.BYDAY)) return { ...base, kind: "weekly", hour, minute, weekday: f.BYDAY };
  }
  return base;
}

export function ruleSummary(rule: string): string {
  const p = parseRule(rule);
  const time = `${String(p.hour).padStart(2, "0")}:${String(p.minute).padStart(2, "0")}`;
  switch (p.kind) {
    case "hourly": return `Every ${p.interval} hours`;
    case "daily": return `Daily at ${time}`;
    case "weekdays": return `Weekdays at ${time}`;
    case "weekly": return `Every ${WEEKDAYS.find(([c]) => c === p.weekday)?.[1]} at ${time}`;
    default: return "Custom schedule";
  }
}

// ---------------------------------------------------------------------------
// Connections
// ---------------------------------------------------------------------------

export type Tone = "neutral" | "success" | "warning" | "error";

export function connectionStatus(c: Json): string {
  if (!c.enabled) return "Paused";
  const error = str(c.error).toLowerCase();
  if (/auth|oauth|sign-in|401/.test(error)) return "Sign in needed";
  if (error) return "Needs attention";
  if (arr(c.tools).length === 0) return c.discovered_ms == null ? "Discovering" : "No tools";
  return "Available";
}

export function connectionTone(c: Json): Tone {
  switch (connectionStatus(c)) {
    case "Sign in needed": case "Needs attention": return "warning";
    case "Available": return "success";
    default: return "neutral";
  }
}

export function connectionDetail(c: Json): string {
  const error = str(c.error);
  const lower = error.toLowerCase();
  if (lower.includes("definition changed")) return "Changed since you approved it. Open to review.";
  if (/auth|401|sign-in/.test(lower)) return "Signed out. Open to sign in again.";
  if (error) return error.split(/[.\n]/)[0] || error;
  const config = rec(c.config);
  if (config.transport === "http") {
    try {
      const host = new URL(str(config.url)).hostname;
      return host === "127.0.0.1" || host === "localhost" ? "A server on this Mac" : host;
    } catch { return "Remote server"; }
  }
  const args = arr(config.args);
  const program = str(args[args.length - 1]) || str(config.command);
  return `Runs on this Mac · ${program.split("/").pop()}`;
}

/** Problems first, then work in progress, then what works, then the rest. */
export function sortConnections(list: Json[]): Json[] {
  const rank = (c: Json) => {
    switch (connectionStatus(c)) {
      case "Sign in needed": case "Needs attention": return 0;
      case "Discovering": return 1;
      case "Available": return 2;
      default: return 3;
    }
  };
  return [...list].sort((a, b) => rank(a) - rank(b) || str(a.label).localeCompare(str(b.label), undefined, { numeric: true }));
}

export function toolSummary(c: Json): string {
  const count = arr(c.tools).length;
  if (count > 0) return `${count} ${count === 1 ? "tool" : "tools"}`;
  if (!c.enabled || str(c.error)) return "No tools";
  return c.discovered_ms == null ? "Discovering" : "No tools";
}

/** Names of a watch's connections that cannot be used yet. */
export function ungranted(snapshot: Json, item: Json): string[] {
  const workspace = str(item.workspace_id);
  const connections = arr(snapshot?.mcp?.connections);
  const available = new Set(
    connections
      .filter((c) => c.enabled && c.trusted && !str(c.error) && (!str(c.workspace_id) || c.workspace_id === workspace) && arr(c.tools).length > 0)
      .map((c) => c.id),
  );
  return arr(item.connection_ids)
    .filter((id) => !available.has(id))
    .map((id) => connections.find((c) => c.id === id)?.label ?? "Removed tool");
}

export const connectionLabel = (snapshot: Json, id: string) =>
  arr(snapshot?.mcp?.connections).find((c) => c.id === id)?.label ?? "Removed tool";

/** What a watch's last check amounts to, in words. */
export function watchStatus(item: Json): { text: string; tone: Tone } {
  if (!item.enabled) return { text: num(item.failures) > 0 ? "Paused · last check failed" : "Paused", tone: "neutral" };
  if (num(item.failures) > 0) return { text: "Needs attention", tone: "warning" };
  if (item.last_attempt_ms == null) return { text: "First check queued", tone: "neutral" };
  return { text: `Checked ${relativeTime(item.last_attempt_ms)}`, tone: "success" };
}

/** Responsibilities Neko proposed in chat that nobody has acted on yet. */
export function suggestedWatchIds(snapshot: Json): Set<string> {
  const offered = new Set(arr(snapshot?.conversation).flatMap((m) => arr(m.responsibility_ids)));
  return new Set(
    arr(snapshot?.mcp?.responsibilities).filter((r) => offered.has(r.id) && !r.enabled && r.last_attempt_ms == null).map((r) => r.id),
  );
}

export function watchIdea(label: string): string {
  const l = label.toLowerCase();
  if (l.includes("linear")) return "New Linear issues assigned to me, and anything of mine that becomes blocked";
  if (l.includes("sentry")) return "New or spiking Sentry errors in my projects";
  if (l.includes("github")) return "Pull requests waiting on my review, and failing checks on my own PRs";
  if (l.includes("slack")) return "Slack messages that mention me or are waiting on my reply";
  if (l.includes("gmail") || l.includes("mail")) return "Emails that need a reply from me today";
  if (l.includes("calendar")) return "Meetings today that need preparation";
  if (l.includes("notion")) return "Notion pages assigned to me or recently changed in my projects";
  return `Anything new in ${label} that needs my attention`;
}

export const WATCH_SUGGEST_PROMPT =
  "Look at the tools connected to this workspace and suggest up to three things you should keep an eye on for me. Only suggest what those tools can actually check.";

export function sizeLabel(mb: number): string {
  return mb >= 1024 ? `${(mb / 1024).toFixed(1)} GB` : `${Math.round(mb)} MB`;
}
