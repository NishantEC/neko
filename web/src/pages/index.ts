// The page registry. One line per page: parallel work adds a page by adding a
// `page({...})` entry here and a file under src/pages/.
//
//   page({
//     id: "tasks",                     // the route: #/tasks
//     title: "Tasks",                  // sidebar + ⌘K "Go to Tasks"
//     icon: ListChecks,                // a phosphor icon from "@/ui/icons"
//     keywords: ["board", "agents"],   // extra words the palette matches
//     load: () => import("./Tasks"),   // lazy; the module's default export is the page
//     group: "work",                   // sidebar section (optional, default "main")
//   }),
//
// Home is eager (it is what opens). Everything else is lazy so a page you never
// open costs nothing. A page is an ordinary React component; compose it from
// "@/ui" (primitives), "@/editorial" (PageHeader, Panel, SectionHead) and
// "@/chat" (conversation pieces). See docs/design/web-ui-from-hushbacks.md.
import { lazy, type ComponentType, type LazyExoticComponent } from "react";
import { Brain, Gauge } from "@phosphor-icons/react";
import { Clock, Eye, Gear, GitPullRequest, House, ListChecks, Palette, Stack, TreeStructure, Users, Wrench, type Icon } from "@/ui/icons";
import { HOME } from "@/router";
import Home from "@/pages/Home";

export type PageGroup = "main" | "work" | "system";

export interface PageDef {
  id: string;
  title: string;
  icon: Icon;
  keywords: string[];
  group: PageGroup;
  component: ComponentType | LazyExoticComponent<ComponentType>;
}

interface PageSpec {
  id: string;
  title: string;
  icon: Icon;
  keywords?: string[];
  group?: PageGroup;
  load: () => Promise<{ default: ComponentType }>;
}

function page({ load, keywords = [], group = "main", ...rest }: PageSpec): PageDef {
  return { ...rest, keywords, group, component: lazy(load) };
}

export const pages: PageDef[] = [
  { id: HOME, title: "Home", icon: House, keywords: ["chat", "ask", "conversation"], group: "main", component: Home },
  page({ id: "tasks", title: "Tasks", icon: ListChecks, keywords: ["board", "list", "agents", "tickets", "work"], group: "work", load: () => import("@/pages/work/Tasks") }),
  page({ id: "prs", title: "Pull requests", icon: GitPullRequest, keywords: ["merge", "github", "review", "checks"], group: "work", load: () => import("@/pages/work/PullRequests") }),
  page({ id: "tree", title: "Agent tree", icon: TreeStructure, keywords: ["who started whom", "runs", "watches", "agents"], group: "work", load: () => import("@/pages/work/Tree") }),
  page({ id: "workspaces", title: "Workspaces", icon: Stack, keywords: ["folders", "repository", "projects", "git"], group: "main", load: () => import("@/pages/setup/Workspaces") }),
  page({ id: "watching", title: "Watching", icon: Eye, keywords: ["watches", "responsibilities", "sources", "sentry", "linear"], group: "main", load: () => import("@/pages/setup/Watching") }),
  page({ id: "schedules", title: "Schedules", icon: Clock, keywords: ["recurring", "brief", "cron", "routine"], group: "main", load: () => import("@/pages/setup/Schedules") }),
  page({ id: "memory", title: "Memory", icon: Brain, keywords: ["remember", "working style", "guidance", "context"], group: "main", load: () => import("@/pages/setup/Memory") }),
  page({ id: "profiles", title: "Profiles", icon: Users, keywords: ["agent profiles", "persona", "instructions"], group: "main", load: () => import("@/pages/setup/Profiles") }),
  page({ id: "tools", title: "Tools & skills", icon: Wrench, keywords: ["mcp", "connections", "servers", "skills", "integrations"], group: "main", load: () => import("@/pages/setup/Tools") }),
  page({ id: "settings", title: "Settings", icon: Gear, keywords: ["preferences", "rulebook", "runtime", "model", "budget", "permissions", "diagnostics", "about"], group: "system", load: () => import("@/pages/setup/Settings") }),
  page({ id: "usage", title: "Usage & machine", icon: Gauge, keywords: ["load", "memory", "disk", "quota", "spend", "cost", "health"], group: "system", load: () => import("@/pages/setup/Usage") }),
  page({
    id: "ui",
    title: "UI gallery",
    icon: Palette,
    keywords: ["design", "components", "primitives", "storybook"],
    group: "system",
    load: () => import("@/pages/Gallery"),
  }),
];

export const pageById = (id: string) => pages.find((p) => p.id === id);
