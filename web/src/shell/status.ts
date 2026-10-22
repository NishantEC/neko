// What the shell says about the workbench: stages, counts, who is waiting on
// you, what the Mac can take. One mapping from the snapshot, so the sidebar,
// the title badge, the toasts, the palette and the status bar always agree
// (the native app does the same with TaskStage.swift and StatusSummary.swift).
import { useEffect, useMemo, useState } from "react";
import type { Json } from "@/api";
import { useWorkbench } from "@/useStore";
import { pendingDecisions } from "@/chat/decisions";

export type Stage = "needsYou" | "review" | "running" | "upNext" | "done";

/** TaskStage.init(status:). */
export function stageOf(status: string): Stage {
  switch (status) {
    case "AwaitingApproval":
    case "Failed": return "needsYou";
    case "ReadyForReview": return "review";
    case "Planning":
    case "Building":
    case "Reviewing": return "running";
    case "Queued": return "upNext";
    default: return "done";
  }
}

export const STAGE_TITLES: Record<Stage, string> = {
  needsYou: "Needs you",
  review: "Ready to review",
  running: "Running",
  upNext: "Up next",
  done: "Done",
};

/** The short human title when Neko wrote one, else the source title. */
export function taskTitle(task: Json): string {
  const display = String(task.display_title ?? "").trim();
  return display || String(task.title ?? "Untitled task");
}

export interface MachineView {
  known: boolean;
  level: "ok" | "limited" | "holding";
  title: string;
  facts: [string, string][];
}

/** MachineStatus (MachineStrip.swift): what the Mac can take right now, in words. */
export function machineView(health: Json | null | undefined): MachineView {
  if (!health) return { known: false, level: "ok", title: "Reading the Mac…", facts: [] };
  const level = health.holding ? "holding" : health.cap != null ? "limited" : "ok";
  const title =
    level === "holding" ? "New work is on hold"
    : level === "limited" ? (health.night ? "Night: one task at a time" : "Memory tight: one at a time")
    : `Mac OK · load ${Math.round(health.load ?? 0)}`;
  const facts: [string, string][] = [];
  if (health.holding) facts.push(["On hold", String(health.holding)]);
  if (health.cap != null) facts.push(["Agents at once", `At most ${health.cap} right now`]);
  facts.push(
    ["Load", `${Number(health.load ?? 0).toFixed(1)} on ${health.cores ?? "?"} cores`],
    ["Memory", `${String(health.pressure ?? "normal").replace(/^./, (c: string) => c.toUpperCase())} · ${Number(health.compressed_gb ?? 0).toFixed(1)} GB compressed · swap ${Number(health.swap_used_gb ?? 0).toFixed(1)} of ${Number(health.swap_total_gb ?? 0).toFixed(1)} GB`],
    ["Disk", `${Math.round(health.free_disk_gb ?? 0)} GB free`],
    ["Cooling", (health.kernel_task_cpu ?? 0) >= 50 ? `macOS is throttling (kernel_task ${Math.round(health.kernel_task_cpu)}%)` : "Normal"],
  );
  return { known: true, level, title, facts };
}

export interface ShellStatus {
  counts: Record<Stage, number>;
  /** Things only you can move forward: decisions plus results to review. */
  waitingOnYou: number;
  /** Tool calls and proposed actions in the conversation that wait for a yes or no. */
  approvals: number;
  /** Everything that should pull you back here. */
  needsUser: number;
  tasks: Record<Stage, Json[]>;
  /** The reply Neko is working on, with when it started. */
  working: { id: string; startedMs: number } | null;
  machine: MachineView;
}

export function deriveStatus(snapshot: Json | null): ShellStatus {
  const counts: Record<Stage, number> = { needsYou: 0, review: 0, running: 0, upNext: 0, done: 0 };
  const tasks: Record<Stage, Json[]> = { needsYou: [], review: [], running: [], upNext: [], done: [] };
  const all: Json[] = snapshot?.tasks ?? [];
  const order: string[] = snapshot?.run_next ?? [];
  for (const task of all) {
    const stage = stageOf(task.status);
    counts[stage] += 1;
    tasks[stage].push(task);
  }
  for (const stage of ["needsYou", "review", "running"] as const) tasks[stage].sort((a, b) => b.updated_at_ms - a.updated_at_ms);
  // The queue: tasks you asked to run next first, then oldest first.
  tasks.upNext.sort((a, b) => {
    const [ia, ib] = [order.indexOf(a.id), order.indexOf(b.id)].map((i) => (i < 0 ? Number.MAX_SAFE_INTEGER : i));
    return ia - ib || a.created_at_ms - b.created_at_ms;
  });
  const conversation: Json[] = snapshot?.conversation ?? [];
  const approvals = snapshot ? pendingDecisions(conversation, all).length : 0;
  const pending = [...conversation].reverse().find((m) => m.pending && m.role !== "user");
  const waitingOnYou = counts.needsYou + counts.review;
  return {
    counts,
    waitingOnYou,
    approvals,
    needsUser: waitingOnYou + approvals,
    tasks,
    working: pending ? { id: pending.id, startedMs: pending.started_ms || pending.at_ms } : null,
    machine: machineView(snapshot?.machine),
  };
}

export function useShellStatus(): ShellStatus {
  const { snapshot } = useWorkbench();
  return useMemo(() => deriveStatus(snapshot), [snapshot]);
}

/** Re-renders every `ms`; for the "Working 12s" clock. */
export function useNow(ms = 1000, active = true): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (!active) return;
    setNow(Date.now());
    const timer = setInterval(() => setNow(Date.now()), ms);
    return () => clearInterval(timer);
  }, [ms, active]);
  return now;
}

/** "12s", "3m 04s", "1h 02m". */
export function elapsed(ms: number): string {
  const s = Math.max(0, Math.floor(ms / 1000));
  if (s < 60) return `${s}s`;
  if (s < 3600) return `${Math.floor(s / 60)}m ${String(s % 60).padStart(2, "0")}s`;
  return `${Math.floor(s / 3600)}h ${String(Math.floor((s % 3600) / 60)).padStart(2, "0")}m`;
}

/** Where a task opens. The Tasks page reads `?task=<id>` from the hash. */
export const taskHref = (id: string) => `#/tasks?task=${encodeURIComponent(id)}`;
