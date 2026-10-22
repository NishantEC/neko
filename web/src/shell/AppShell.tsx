// The app shell: hushbacks' sidebar (shell/AppSidebar.tsx) beside the open
// page and the inspector, the status bar, ⌘K, the shortcut overlay, toasts,
// confirmations and the first-run tour. Pages only ever render their own
// content (pages/index.ts), each inside hushbacks' PageContainer column.
import { Suspense, useEffect } from "react";
import { SidebarInset, SidebarProvider } from "@/ui/sidebar";
import { Skeleton } from "@/ui/skeleton";
import { Toaster } from "@/ui/sonner";
import { TooltipProvider } from "@/ui/tooltip";
import { Button } from "@/ui/button";
import { Empty, EmptyDescription, EmptyHeader, EmptyMedia, EmptyTitle } from "@/ui/empty";
import { Compass } from "@phosphor-icons/react";
import { pageById } from "@/pages";
import { HOME, navigate, useRoute } from "@/router";
import { PluginBoundary, usePlugins } from "@/plugins";
import { CommandPalette, openPalette } from "@/shell/CommandPalette";
import { InspectorHost } from "@/shell/inspector";
import { ShortcutsOverlay } from "@/shell/shortcuts";
import { ConfirmHost } from "@/shell/confirm";
import { newTask, stopAllWork } from "@/shell/actions";
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
      <PageContainer className="max-w-[52rem]">
        <h2 className="text-lg font-semibold text-strong">The “{name}” plugin did not load</h2>
        <pre className="mt-3 whitespace-pre-wrap rounded-xl border border-border bg-foreground/[0.03] p-3 text-destructive">
          {problems.find((p) => p.plugin === name)?.message ?? "It loads now."}
        </pre>
        <Button type="button" variant="outline" className="mt-4" onClick={() => navigate("plugins")}>Open the Plugins page</Button>
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

export function AppShell() {
  const route = useRoute();
  useAttention();
  useGlobalShortcuts();
  return (
    <TooltipProvider>
      <SidebarProvider defaultOpen className="sky relative isolate h-svh min-h-0 has-data-[variant=inset]:bg-background">
        <AppSidebar route={route} />
        <SidebarInset className="relative flex h-svh min-h-0 min-w-0 flex-col overflow-hidden md:peer-data-[variant=inset]:m-0 md:peer-data-[variant=inset]:rounded-none md:peer-data-[variant=inset]:shadow-none md:peer-data-[variant=inset]:peer-data-[state=collapsed]:ml-0">
          <MobileTopBar />
          <InspectorHost>
            <div data-slot="page" data-route={route} className="relative flex min-h-0 flex-1 flex-col overflow-hidden">
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
    </TooltipProvider>
  );
}

export { HOME };
