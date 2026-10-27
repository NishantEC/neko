// Static sample data for the gallery's sidebar options (/#/ui, "Sidebar").
// Shaped like what the owner's real sidebar shows on a normal day; never live.
import { Brain, Gauge, Heartbeat } from "@phosphor-icons/react";
import { Clock, Eye, Gear, GitPullRequest, House, ListChecks, Stack, TreeStructure, Users, Wrench, type Icon } from "@/ui/icons";
import type { Stage } from "@/shell/status";

export type SampleTask = { id: string; title: string; stage: Stage; meta?: string };
export type SamplePage = { id: string; title: string; icon: Icon; plugin?: boolean };

export const SAMPLE_TASKS: SampleTask[] = [
  { id: "t1", title: "Approve the workbench_tasks migration", stage: "needsYou" },
  { id: "t2", title: "HEA-3006: Sentry spike in checkout", stage: "needsYou" },
  { id: "t3", title: "Recreate missing worktrees on resume", stage: "running", meta: "52s" },
  { id: "t4", title: "Index Linear comments for search", stage: "running", meta: "4m" },
  { id: "t5", title: "Cap Vitest workers at two", stage: "review" },
  { id: "t6", title: "Collapse the sidebar on narrow windows", stage: "review" },
  { id: "t7", title: "Prune node_modules at review", stage: "upNext" },
];
/** Everything open, including what the sidebar does not list. */
export const SAMPLE_TASK_TOTAL = 14;
export const SAMPLE_UP_NEXT = 8;
export const SAMPLE_NEEDS_YOU = SAMPLE_TASKS.filter((t) => t.stage === "needsYou").length;

const P = (id: string, title: string, icon: Icon, plugin?: boolean): SamplePage => ({ id, title, icon, plugin });

export const PAGE = {
  home: P("home", "Home", House),
  tasks: P("tasks", "Tasks", ListChecks),
  prs: P("prs", "Pull requests", GitPullRequest),
  tree: P("tree", "Agent tree", TreeStructure),
  workspaces: P("workspaces", "Workspaces", Stack),
  watching: P("watching", "Watching", Eye),
  schedules: P("schedules", "Schedules", Clock),
  memory: P("memory", "Memory", Brain),
  profiles: P("profiles", "Profiles", Users),
  tools: P("tools", "Tools & skills", Wrench),
  health: P("health", "Health", Heartbeat, true),
  settings: P("settings", "Settings", Gear),
  usage: P("usage", "Usage & machine", Gauge),
} as const;

export const SAMPLE_MACHINE = { level: "ok" as const, title: "Mac OK", detail: "load 4" };
