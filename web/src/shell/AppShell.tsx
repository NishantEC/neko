// The app shell: sidebar (hushbacks' sidebar.tsx), the open page, ⌘K and toasts.
import { Suspense, useSyncExternalStore } from "react";
import {
  Sidebar, SidebarContent, SidebarFooter, SidebarGroup, SidebarGroupContent, SidebarGroupLabel, SidebarHeader,
  SidebarInset, SidebarMenu, SidebarMenuButton, SidebarMenuItem, SidebarProvider, SidebarTrigger,
} from "@/ui/sidebar";
import { Skeleton } from "@/ui/skeleton";
import { Toaster } from "@/ui/sonner";
import { TooltipProvider } from "@/ui/tooltip";
import { Kbd } from "@/ui/kbd";
import { Button } from "@/ui/button";
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger } from "@/ui/dropdown-menu";
import { CircleHalf, Command as CommandIcon, MagnifyingGlass, Moon, PuzzlePiece, Sun, Warning } from "@/ui/icons";
import { EditorialHeading } from "@/editorial";
import { pages, pageById, type PageDef } from "@/pages";
import { hrefFor, HOME, useRoute } from "@/router";
import { PluginBoundary, usePlugins } from "@/plugins";
import { getResolvedTheme, getTheme, setTheme, subscribeTheme } from "@/theme";
import { useWorkbench } from "@/useStore";
import neko from "@/assets/neko-mark.svg";
import { CommandPalette, openPalette } from "@/shell/CommandPalette";

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

function AppSidebar({ route }: { route: string }) {
  const { connected } = useWorkbench();
  const { pages: pluginPages, problems } = usePlugins();
  return (
    <Sidebar>
      <SidebarHeader className="flex-row items-center gap-2 px-4 pt-4 pb-3">
        <Mark className="size-5 text-strong" />
        <EditorialHeading as="span" className="text-[19px] leading-none">Neko</EditorialHeading>
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
                        <span>{p.title}</span>
                      </SidebarMenuButton>
                    </SidebarMenuItem>
                  ))}
                </SidebarMenu>
              </SidebarGroupContent>
            </SidebarGroup>
          );
        })}
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
    <div className="mx-auto w-full max-w-3xl space-y-4 px-6 py-10" aria-busy="true">
      <Skeleton className="h-4 w-24" />
      <Skeleton className="h-10 w-72" />
      <Skeleton className="h-48 w-full" />
    </div>
  );
}

function Missing({ children }: { children: React.ReactNode }) {
  return <p className="px-6 py-12 text-center text-muted-foreground">{children}</p>;
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
      </div>
    );
  }
  return <Missing>This page is not available (yet).</Missing>;
}

export function AppShell() {
  const route = useRoute();
  return (
    <TooltipProvider>
      <SidebarProvider className="h-svh min-h-0">
        <AppSidebar route={route} />
        <SidebarInset className="min-h-0 min-w-0 overflow-hidden">
          <header className="flex h-10 shrink-0 items-center px-2 md:hidden">
            <SidebarTrigger />
          </header>
          <div data-slot="page" data-route={route} className="flex min-h-0 flex-1 flex-col">
            <Outlet route={route} />
          </div>
        </SidebarInset>
        <CommandPalette />
        <Toaster />
      </SidebarProvider>
    </TooltipProvider>
  );
}

export { HOME };
