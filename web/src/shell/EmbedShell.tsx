// Embed mode's whole shell (`?embed=1`, shell/embed.ts): the routed content and nothing around it. No
// tab strip, rail, sidebar, status bar, ⌘K or onboarding: the Rune client that holds this page in a
// web view has its own. What stays is what the content itself needs: tooltips, the right-click
// menu, confirmations and toasts.
//
// It also leaves out what only a person's own window should do: it does not follow an agent's
// ShowArtifact / OpenTab (the Rune client obeys those itself) and it does not report itself to
// InspectApp as "the window".
import { useEffect, useState } from "react";
import { Skeleton } from "@/ui/skeleton";
import { Toaster } from "@/ui/sonner";
import { TooltipProvider } from "@/ui/tooltip";
import { usePages } from "@/registry";
import { useRoute } from "@/router";
import { usePlugins } from "@/plugins";
import { ContextMenuHost } from "@/shell/context-menu";
import { ConfirmHost } from "@/shell/confirm";
import { Outlet } from "@/shell/AppShell";
import { ArtifactEmbed } from "@/canvas/ArtifactEmbed";
import { useReportPluginPages } from "@/shell/pluginPages";

/** `artifact/<id>/<n>` (the address the Rune client builds) or `artifact?id=<id>&v=<n>` (the tab's own). */
export function artifactFromRoute(route: string): { id: string; version: number | null } | null {
  const m = /^artifact\/([A-Za-z0-9_-]{1,64})(?:\/(\d+|latest))?$/.exec(route);
  if (!m) return null;
  const n = Number(m[2]);
  return { id: m[1], version: Number.isFinite(n) && n > 0 ? n : null };
}

/** A plugin's page is registered when its module has loaded: a moment after the page does. */
function PluginWait({ route }: { route: string }) {
  const { listed } = usePlugins();
  const registered = usePages().some((p) => p.id === route);
  const [waited, setWaited] = useState(false);
  useEffect(() => {
    const timer = setTimeout(() => setWaited(true), 5000);
    return () => clearTimeout(timer);
  }, [route]);
  if (!registered && !waited && (!listed || route.includes(":"))) {
    return (
      <div aria-busy="true" aria-label="Loading the page" role="status" className="space-y-3 p-4">
        <Skeleton className="h-8 w-56" />
        <Skeleton className="h-40 w-full" />
      </div>
    );
  }
  return <Outlet route={route} />;
}

export function EmbedShell() {
  const route = useRoute();
  useReportPluginPages();
  const artifact = artifactFromRoute(route);
  return (
    <TooltipProvider>
      <ContextMenuHost>
        <div data-slot="embed" data-route={route} className="relative isolate flex h-svh min-h-0 flex-col overflow-hidden">
          {artifact ? <ArtifactEmbed id={artifact.id} version={artifact.version} /> : route === "artifact" ? <ArtifactEmbed id={null} version={null} /> : <PluginWait route={route} />}
        </div>
        <ConfirmHost />
        <Toaster />
      </ContextMenuHost>
    </TooltipProvider>
  );
}
