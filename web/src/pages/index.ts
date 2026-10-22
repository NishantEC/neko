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
import { House, Palette, PuzzlePiece, type Icon } from "@/ui/icons";
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
  page({
    id: "ui",
    title: "UI gallery",
    icon: Palette,
    keywords: ["design", "components", "primitives", "storybook"],
    group: "system",
    load: () => import("@/pages/Gallery"),
  }),
  page({
    id: "plugins",
    title: "Plugins",
    icon: PuzzlePiece,
    keywords: ["extend", "extensions", "build", "agents", "self-evolving"],
    group: "system",
    load: () => import("@/pages/plugins/Plugins"),
  }),
];

export const pageById = (id: string) => pages.find((p) => p.id === id);
