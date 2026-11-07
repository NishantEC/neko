// The app shell: one window of tabs (DESIGN.md §1). The tab strip
// (shell/TabStrip.tsx, store in shell/tabs.ts) sits on top of the open page
// and the inspector; ui-kit' sidebar (shell/AppSidebar.tsx) is a launcher
// beside them, collapsed to its icon rail by default. Around it: Neko's own
// right-click menu (shell/context-menu.tsx), ⌘K, the shortcut overlay, toasts,
// confirmations and the first-run tour. Pages only ever render their own
// content (pages/index.ts), each inside ui-kit' PageContainer column.
import { Suspense, useEffect, useState } from "react";
import { SidebarInset, SidebarProvider } from "@/ui/sidebar";
import { Skeleton } from "@/ui/skeleton";
import { Toaster } from "@/ui/sonner";
import { TooltipProvider } from "@/ui/tooltip";
import { Button } from "@/ui/button";
import { Empty, EmptyDescription, EmptyHeader, EmptyMedia, EmptyTitle } from "@/ui/empty";
import { ArrowCounterClockwise, Compass } from "@phosphor-icons/react";
import { toast } from "@/ui/sonner";
import "@/pages";
import { setOverrideOff, usePages } from "@/registry";
import { HostProvider } from "@/host";
import { HOME, navigate, useRoute } from "@/router";
import { PluginBoundary, usePlugins } from "@/plugins";
import { CommandPalette, openPalette } from "@/shell/CommandPalette";
import { InspectorHost } from "@/shell/inspector";
import { ShortcutsOverlay } from "@/shell/shortcuts";
import { ConfirmHost } from "@/shell/confirm";
import { askNeko, newTask, stopAllWork } from "@/shell/actions";
import { ContextMenuHost, setShellMenuActions } from "@/shell/context-menu";
import { canGoBack, canGoForward, goBack, goForward, newTab, startTabs, useTabs } from "@/shell/tabs";
import { TAB_PANEL_ID, TabStrip, tabElementId } from "@/shell/TabStrip";
import { Launcher } from "@/shell/Launcher";
import { ArtifactTab } from "@/canvas/ArtifactTab";
import { obeyAgentRequests } from "@/canvas/artifacts";
import { searchPalette } from "@/shell/CommandPalette";
import { useAttention } from "@/shell/attention";
import { AppSidebar, MobileTopBar } from "@/shell/AppSidebar";
import { PageContainer } from "@/shell/PageContainer";
import { OnboardingHost } from "@/onboarding/Onboarding";

function PageFallback() {
  return (
    <PageContainer aria-busy="true" aria-label="Loading the page" role="status" className="space-y-4">
      <Skeleton className="h-10 w-72" />
      <Skeleton className="h-48 w-full" />
    </PageContainer>
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
          Nothing is registered at “{route}”. It may live in the Neko app, or it may be one of your own pages that was removed or switched off.
        </EmptyDescription>
      </EmptyHeader>
      <div className="flex gap-2">
        <Button type="button" onClick={() => navigate(HOME)}>Back to Home</Button>
        <Button type="button" variant="outline" onClick={openPalette}>Search with ⌘K</Button>
      </div>
    </Empty>
  );
}

/** Above an overridden built-in: it says so, and offers the way back. */
function CustomisedStrip({ id, title, plugin }: { id: string; title: string; plugin?: string }) {
  return (
    <div data-slot="customised-strip" className="flex shrink-0 items-center gap-3 border-b border-border/60 bg-primary/[0.05] px-6 py-1.5 text-[12.5px] text-muted-foreground">
      <span className="rounded-full bg-primary/15 px-2 py-px text-[11px] font-medium text-primary">Customised</span>
      <span className="min-w-0 flex-1 truncate">You are using your own version of {title}{plugin ? <> from “{plugin}”</> : null}.</span>
      <Button
        type="button"
        variant="ghost"
        size="xs"
        onClick={() => void setOverrideOff(id, true).then(() => toast(`${title} is back to the original.`), (e) => toast.error("Could not reset", { description: e instanceof Error ? e.message : String(e) }))}
      >
        <ArrowCounterClockwise /> Reset to original
      </Button>
    </div>
  );
}

function Outlet({ route }: { route: string }) {
  const { problems } = usePlugins();
  const page = usePages().find((p) => p.id === route);
  // The shell's own places: not pages, so they are not in the sidebar or ⌘K's "Go to".
  if (route === "new") return <Launcher />;
  if (route === "artifact") return <ArtifactTab />;
  if (page) {
    const body = (
      <HostProvider value={page.host}>
        <Suspense fallback={<PageFallback />}>
          <page.component host={page.host} />
        </Suspense>
      </HostProvider>
    );
    // A user's page can throw; the app carries on and says which one.
    const guarded = page.plugin && !page.overrideOff ? <PluginBoundary plugin={page.plugin} key={`${page.id}:${page.customised}`}>{body}</PluginBoundary> : body;
    return page.customised ? (
      <>
        <CustomisedStrip id={page.id} title={page.title} plugin={page.plugin} />
        <div className="flex min-h-0 flex-1 flex-col overflow-hidden">{guarded}</div>
      </>
    ) : guarded;
  }
  if (route.startsWith("problem:")) {
    const name = route.slice(8);
    return (
      <PageContainer className="max-w-[52rem]">
        <h2 className="text-lg font-semibold text-strong">The “{name}” page did not load</h2>
        <pre className="mt-3 whitespace-pre-wrap rounded-xl border border-border bg-foreground/[0.03] p-3 text-destructive">
          {problems.find((p) => p.plugin === name)?.message ?? "It loads now."}
        </pre>
        <Button type="button" variant="outline" className="mt-4" onClick={() => navigate("customisations")}>Open Customisations</Button>
      </PageContainer>
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

setShellMenuActions({
  back: goBack,
  forward: goForward,
  canGoBack,
  canGoForward,
  newTab,
  openPalette,
  askNeko,
  search: searchPalette,
});

const SIDEBAR_KEY = "neko.sidebar.open";

/** The sidebar is a launcher now that tabs hold the places: collapsed to its rail unless you open it (remembered). */
function useSidebarOpen(): [boolean, (open: boolean) => void] {
  const [open, setOpen] = useState(() => {
    try { return localStorage.getItem(SIDEBAR_KEY) === "true"; } catch { return false; }
  });
  return [open, (next: boolean) => {
    setOpen(next);
    try { localStorage.setItem(SIDEBAR_KEY, String(next)); } catch { /* private mode */ }
  }];
}

export function AppShell() {
  useState(startTabs);
  const route = useRoute();
  const { active } = useTabs();
  const [sidebarOpen, setSidebarOpen] = useSidebarOpen();
  useAttention();
  useGlobalShortcuts();
  // An agent's ShowArtifact / OpenTab: every open page opens the canvas or the tab.
  useEffect(obeyAgentRequests, []);
  return (
    <TooltipProvider>
      <ContextMenuHost>
      <SidebarProvider open={sidebarOpen} onOpenChange={setSidebarOpen} className="sky relative isolate h-svh min-h-0 has-data-[variant=inset]:bg-background">
        <AppSidebar route={route} />
        <SidebarInset className="relative flex h-svh min-h-0 min-w-0 flex-col overflow-hidden md:peer-data-[variant=inset]:m-0 md:peer-data-[variant=inset]:rounded-none md:peer-data-[variant=inset]:shadow-none md:peer-data-[variant=inset]:peer-data-[state=collapsed]:ml-0">
          <MobileTopBar />
          <TabStrip />
          <InspectorHost>
            <div
              data-slot="page"
              data-route={route}
              role="tabpanel"
              id={TAB_PANEL_ID}
              aria-labelledby={tabElementId(active)}
              className="relative flex min-h-0 flex-1 flex-col overflow-hidden"
            >
              <Outlet route={route} />
            </div>
          </InspectorHost>
        </SidebarInset>
        <CommandPalette />
        <ShortcutsOverlay />
        <ConfirmHost />
        <OnboardingHost />
        <Toaster />
      </SidebarProvider>
      </ContextMenuHost>
    </TooltipProvider>
  );
}

export { HOME };
