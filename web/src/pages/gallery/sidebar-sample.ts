// Static sample data for the gallery's sidebar options (/#/ui, "Sidebar").
// Shaped like what the owner's real sidebar shows on a normal day; never live.
import { Brain, Gauge, Heartbeat } from "@phosphor-icons/react";
import { Clock, Eye, Gear, GitPullRequest, House, ListChecks, Stack, TreeStructure, Users, Wrench, type Icon } from "@/ui/icons";

export type SamplePage = { id: string; title: string; icon: Icon; plugin?: boolean };

/** Tasks waiting on you: the one number the sidebar keeps. */
export const SAMPLE_NEEDS_YOU = 2;

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
