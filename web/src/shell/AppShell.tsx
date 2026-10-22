// The app shell: sidebar with live state, the open page beside the inspector,
// the status bar, ⌘K, the shortcut overlay, toasts, confirmations and the
// first-run tour. Pages only ever render their own content (pages/index.ts).
import { Suspense, useEffect, useSyncExternalStore } from "react";
import {
  Sidebar, SidebarContent, SidebarFooter, SidebarGroup, SidebarGroupContent, SidebarGroupLabel, SidebarHeader,
  SidebarInset, SidebarMenu, SidebarMenuButton, SidebarMenuItem, SidebarProvider, SidebarTrigger,
} from "@/ui/sidebar";
import { Skeleton } from "@/ui/skeleton";
import { Toaster } from "@/ui/sonner";
import { TooltipProvider } from "@/ui/tooltip";
import { Kbd } from "@/ui/kbd";
import { Button } from "@/ui/button";
import { Empty, EmptyDescription, EmptyHeader, EmptyMedia, EmptyTitle } from "@/ui/empty";
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger } from "@/ui/dropdown-menu";
import { Compass } from "@phosphor-icons/react";
import { CircleHalf, Command as CommandIcon, MagnifyingGlass, Moon, PuzzlePiece, Sun, Warning, Plus } from "@/ui/icons";
import { EditorialHeading } from "@/editorial";
import { pages, pageById, type PageDef } from "@/pages";
import { hrefFor, HOME, navigate, useRoute } from "@/router";
import { PluginBoundary, usePlugins } from "@/plugins";
import { getResolvedTheme, getTheme, setTheme, subscribeTheme } from "@/theme";
import { useWorkbench } from "@/useStore";
import { cn } from "@/ui/lib/utils";
import neko from "@/assets/neko-mark.svg";
import { CommandPalette, openPalette } from "@/shell/CommandPalette";
import { StatusBar } from "@/shell/StatusBar";
import { InspectorHost } from "@/shell/inspector";
import { ShortcutsOverlay } from "@/shell/shortcuts";
import { ConfirmHost } from "@/shell/confirm";
import { newTask, stopAllWork } from "@/shell/actions";
import { useAttention } from "@/shell/attention";
import { elapsed, STAGE_TITLES, taskHref, taskTitle, useNow, useShellStatus, type Stage } from "@/shell/status";
import { OnboardingHost } from "@/onboarding/Onboarding";

const GROUPS: { id: PageDef["group"]; label: string | null }[] = [
  { id: "main", label: null },
  { id: "work", label: "Work" },
  { id: "system", label: "System" },
];

function Mark({ className }: { className?: string }) {
  return (
    <span
      aria-hidden
      className={className}
      style={{ background: "currentColor", maskImage: `url(${neko})`, WebkitMaskImage: `url(${neko})`, maskSize: "contain", WebkitMaskSize: "contain", maskRepeat: "no-repeat", WebkitMaskRepeat: "no-repeat" }}
    />
  );
}

function ThemeMenu() {
  const choice = useSyncExternalStore(subscribeTheme, getTheme);
  const resolved = useSyncExternalStore(subscribeTheme, getResolvedTheme);
  const Icon = choice === "system" ? CircleHalf : resolved === "dark" ? Moon : Sun;
  return (
    <DropdownMenu>
      <DropdownMenuTrigger render={<Button variant="ghost" size="icon-sm" aria-label="Theme" className="text-sidebar-foreground" />}>
        <Icon />
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" side="top">
        <DropdownMenuItem onClick={() => setTheme("system")}><CircleHalf /> System</DropdownMenuItem>
        <DropdownMenuItem onClick={() => setTheme("light")}><Sun /> Light</DropdownMenuItem>
        <DropdownMenuItem onClick={() => setTheme("dark")}><Moon /> Dark</DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

// ---------------------------------------------------------------------------
// Live state in the row's right-hand slot
// ---------------------------------------------------------------------------

type Tone = "amber" | "sky" | "primary" | "muted";
const TONES: Record<Tone, string> = {
  amber: "bg-warning/15 text-warning-foreground",
  sky: "bg-info/15 text-info-foreground",
  primary: "bg-primary/15 text-primary",
  muted: "bg-foreground/[0.06] text-muted-foreground",
};

function Chip({ tone, children, label }: { tone: Tone; children: React.ReactNode; label?: string }) {
  return (
    <span data-slot="state-chip" data-tone={tone} aria-label={label} className={cn("ms-auto shrink-0 rounded-full px-1.5 py-px text-[11px] font-medium leading-4 tabular-nums", TONES[tone])}>
      {children}
    </span>
  );
}

function WorkingChip({ since }: { since: number }) {
  const now = useNow(1000);
  return <Chip tone="primary" label={`Working ${elapsed(now - since)}`}>Working {elapsed(now - since)}</Chip>;
}

const STAGE_CHIP: Record<Stage, { tone: Tone; text: string } | null> = {
  needsYou: { tone: "amber", text: "Needs you" },
  review: { tone: "sky", text: "Review" },
  running: { tone: "primary", text: "Working" },
  upNext: { tone: "muted", text: "Next" },
  done: null,
};
const STAGE_DOT: Record<Stage, string> = {
  needsYou: "bg-warning",
  review: "bg-info",
  running: "bg-primary animate-pulse motion-reduce:animate-none",
  upNext: "bg-muted-foreground/50",
  done: "bg-success",
};

/** The live chip for a page row, from the snapshot (null when there is nothing to say). */
function PageChip({ id }: { id: string }) {
  const status = useShellStatus();
  if (id === HOME) {
    if (status.working) return <WorkingChip since={status.working.startedMs} />;
    if (status.approvals > 0) return <Chip tone="amber" label={`${status.approvals} waiting for your approval`}>Needs you{status.approvals > 1 ? ` · ${status.approvals}` : ""}</Chip>;
    return null;
  }
  if (id === "tasks" && status.waitingOnYou > 0) {
    return <Chip tone="amber" label={`${status.waitingOnYou} tasks waiting on you`}>{status.waitingOnYou}</Chip>;
  }
  return null;
}

/** How many tasks of each stage the sidebar lists; the rest are one click away. */
const SHOWN_PER_STAGE: Record<Stage, number> = { needsYou: 3, review: 2, running: 3, upNext: 1, done: 0 };

function TaskRows() {
  const status = useShellStatus();
  const { snapshot } = useWorkbench();
  const order: Stage[] = ["needsYou", "review", "running", "upNext"];
  const shown = order.flatMap((stage) => status.tasks[stage].slice(0, SHOWN_PER_STAGE[stage]).map((task) => ({ task, stage })));
  const total = order.reduce((n, s) => n + status.tasks[s].length, 0);
  if (!snapshot || shown.length === 0) return null;
  return (
    <SidebarGroup data-slot="sidebar-tasks">
      <SidebarGroupLabel>Tasks</SidebarGroupLabel>
      <SidebarGroupContent>
        <SidebarMenu>
          {shown.map(({ task, stage }) => {
            const chip = STAGE_CHIP[stage];
            return (
              <SidebarMenuItem key={task.id}>
                <SidebarMenuButton
                  data-slot="task-row"
                  data-stage={stage}
                  title={`${taskTitle(task)} · ${STAGE_TITLES[stage]}`}
                  render={<a href={taskHref(task.id)} />}
                >
                  <span aria-hidden className={cn("size-1.5 shrink-0 rounded-full", STAGE_DOT[stage])} />
                  <span className="min-w-0 flex-1 truncate">{taskTitle(task)}</span>
                  {chip ? <Chip tone={chip.tone} label={STAGE_TITLES[stage]}>{chip.text}</Chip> : null}
                </SidebarMenuButton>
              </SidebarMenuItem>
            );
          })}
          {total > shown.length ? (
            <SidebarMenuItem>
              <SidebarMenuButton className="text-muted-foreground" render={<a href={hrefFor("tasks")} />}>
                <span>Show all {total}</span>
              </SidebarMenuButton>
            </SidebarMenuItem>
          ) : null}
        </SidebarMenu>
      </SidebarGroupContent>
    </SidebarGroup>
  );
}

function AppSidebar({ route }: { route: string }) {
  const { connected } = useWorkbench();
  const { pages: pluginPages, problems } = usePlugins();
  return (
    <Sidebar>
      <SidebarHeader className="flex-row items-center gap-2 px-4 pt-4 pb-3">
        <Mark className="size-5 text-strong" />
        <EditorialHeading as="span" className="flex-1 text-[19px] leading-none">Neko</EditorialHeading>
        <Button type="button" variant="ghost" size="icon-sm" aria-label="New task" title="New task (⌘N)" className="text-sidebar-foreground" onClick={newTask}>
          <Plus />
        </Button>
      </SidebarHeader>
      <SidebarContent>
        <SidebarGroup className="pb-0">
          <SidebarGroupContent>
            <button
              type="button"
              onClick={openPalette}
              className="flex h-8 w-full items-center gap-2 rounded-lg border border-sidebar-border bg-foreground/[0.03] px-2.5 text-[13px] text-muted-foreground outline-none transition-colors hover:bg-foreground/[0.06] focus-visible:ring-2 focus-visible:ring-sidebar-ring"
              aria-label="Open the command palette"
            >
              <MagnifyingGlass size={14} />
              <span className="flex-1 text-start">Search or jump to…</span>
              <Kbd className="bg-transparent"><CommandIcon size={11} />K</Kbd>
            </button>
          </SidebarGroupContent>
        </SidebarGroup>
        {GROUPS.map(({ id, label }) => {
          const list = pages.filter((p) => p.group === id);
          if (!list.length) return null;
          return (
            <SidebarGroup key={id}>
              {label ? <SidebarGroupLabel>{label}</SidebarGroupLabel> : null}
              <SidebarGroupContent>
                <SidebarMenu>
                  {list.map((p) => (
                    <SidebarMenuItem key={p.id}>
                      <SidebarMenuButton
                        data-slot="nav-item"
                        isActive={route === p.id}
                        render={<a href={hrefFor(p.id)} aria-current={route === p.id ? "page" : undefined} />}
                      >
                        <p.icon />
                        <span className="min-w-0 flex-1 truncate">{p.title}</span>
                        <PageChip id={p.id} />
                      </SidebarMenuButton>
                    </SidebarMenuItem>
                  ))}
                </SidebarMenu>
              </SidebarGroupContent>
            </SidebarGroup>
          );
        })}
        <TaskRows />
        {(pluginPages.length > 0 || problems.length > 0) && (
          <SidebarGroup>
            <SidebarGroupLabel>Plugins</SidebarGroupLabel>
            <SidebarGroupContent>
              <SidebarMenu>
                {pluginPages.map((p) => (
                  <SidebarMenuItem key={p.key}>
                    <SidebarMenuButton
                      data-slot="nav-item"
                      isActive={route === p.key}
                      render={<a href={hrefFor(p.key)} aria-current={route === p.key ? "page" : undefined} />}
                    >
                      <PuzzlePiece />
                      <span>{p.title}</span>
                    </SidebarMenuButton>
                  </SidebarMenuItem>
                ))}
                {problems.map((p) => (
                  <SidebarMenuItem key={p.plugin}>
                    <SidebarMenuButton
                      data-slot="nav-item"
                      title={p.message}
                      className="text-warning-foreground"
                      isActive={route === `problem:${p.plugin}`}
                      render={<a href={hrefFor(`problem:${p.plugin}`)} />}
                    >
                      <Warning />
                      <span>{p.plugin}</span>
                    </SidebarMenuButton>
                  </SidebarMenuItem>
                ))}
              </SidebarMenu>
            </SidebarGroupContent>
          </SidebarGroup>
        )}
      </SidebarContent>
      <SidebarFooter className="flex-row items-center gap-2 px-4 py-3">
        <span data-slot="connection" data-connected={connected} className="flex flex-1 items-center gap-2 text-xs text-muted-foreground">
          <span aria-hidden className={`size-1.5 rounded-full ${connected ? "bg-success" : "bg-warning"}`} />
          {connected ? "Connected" : "Reconnecting…"}
        </span>
        <ThemeMenu />
      </SidebarFooter>
    </Sidebar>
  );
}

function PageFallback() {
  return (
    <div className="mx-auto w-full max-w-3xl space-y-4 px-6 py-10" aria-busy="true" aria-label="Loading the page" role="status">
      <Skeleton className="h-4 w-24" />
      <Skeleton className="h-10 w-72" />
      <Skeleton className="h-48 w-full" />
    </div>
  );
}

/** A route nothing serves: say so, and offer the way back. */
function Missing({ route }: { route: string }) {
  return (
    <Empty data-slot="missing" className="m-auto max-w-lg border-0">
      <EmptyHeader>
        <EmptyMedia variant="icon"><Compass /></EmptyMedia>
        <EmptyTitle>That page is not in the web app yet</EmptyTitle>
        <EmptyDescription>
          Nothing is registered at “{route}”. It may live in the Neko app, or it may be a plugin that was removed or switched off.
        </EmptyDescription>
      </EmptyHeader>
      <div className="flex gap-2">
        <Button type="button" onClick={() => navigate(HOME)}>Back to Home</Button>
        <Button type="button" variant="outline" onClick={openPalette}>Search with ⌘K</Button>
      </div>
    </Empty>
  );
}

function Outlet({ route }: { route: string }) {
  const { pages: pluginPages, problems } = usePlugins();
  const page = pageById(route);
  if (page) {
    return (
      <Suspense fallback={<PageFallback />}>
        <page.component />
      </Suspense>
    );
  }
  const plugin = pluginPages.find((p) => p.key === route);
  if (plugin) {
    return (
      <PluginBoundary plugin={plugin.plugin} key={plugin.key}>
        <plugin.component />
      </PluginBoundary>
    );
  }
  if (route.startsWith("problem:")) {
    const name = route.slice(8);
    return (
      <div className="mx-auto w-full max-w-[728px] px-6 py-10">
        <h2 className="text-lg font-semibold text-strong">The “{name}” plugin did not load</h2>
        <pre className="mt-3 whitespace-pre-wrap rounded-xl border border-border bg-foreground/[0.03] p-3 text-destructive">
          {problems.find((p) => p.plugin === name)?.message ?? "It loads now."}
        </pre>
        <Button type="button" variant="outline" className="mt-4" onClick={() => navigate("plugins")}>Open the Plugins page</Button>
      </div>
    );
  }
  return <Missing route={route} />;
}

/** ⌘N / ⌘⇧O new task and ⌘⇧⎋ stop all work (the native app's own keys). */
function useGlobalShortcuts() {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const mod = e.metaKey || e.ctrlKey;
      if (mod && !e.altKey && ((e.key.toLowerCase() === "n" && !e.shiftKey) || (e.key.toLowerCase() === "o" && e.shiftKey))) {
        e.preventDefault();
        newTask();
      } else if (mod && e.shiftKey && e.key === "Escape") {
        e.preventDefault();
        void stopAllWork();
      }
    };
    addEventListener("keydown", onKey);
    return () => removeEventListener("keydown", onKey);
  }, []);
}

export function AppShell() {
  const route = useRoute();
  useAttention();
  useGlobalShortcuts();
  return (
    <TooltipProvider>
      <SidebarProvider className="h-svh min-h-0">
        <AppSidebar route={route} />
        <SidebarInset className="min-h-0 min-w-0 overflow-hidden">
          <header className="flex h-10 shrink-0 items-center px-2 md:hidden">
            <SidebarTrigger />
          </header>
          <InspectorHost>
            <div data-slot="page" data-route={route} className="flex min-h-0 flex-1 flex-col">
              <Outlet route={route} />
            </div>
          </InspectorHost>
          <StatusBar />
        </SidebarInset>
        <CommandPalette />
        <ShortcutsOverlay />
        <ConfirmHost />
        <OnboardingHost />
        <Toaster />
      </SidebarProvider>
    </TooltipProvider>
  );
}

export { HOME };
