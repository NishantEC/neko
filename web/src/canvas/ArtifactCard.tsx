// An artifact's card on the reply that made it (`data-neko-artifact` parts,
// added by the daemon when the agent saves one). Open shows it on the canvas
// beside the conversation; Open in a tab gives it a tab of its own. Its
// right-click menu (artifactMenu) is shared with the canvas panel.
import { Button } from "@/ui/button";
import { ArrowSquareOut, DownloadSimple, FrameCorners, Link, SidebarSimple } from "@/ui/icons";
import { cn } from "@/ui/lib/utils";
import { artifactHref, artifactUrl, downloadUrl, openCanvas, useCanvas, type ArtifactCardData } from "@/canvas/artifacts";
import { copyText, useContextMenu, type MenuEntry } from "@/shell/context-menu";
import type { NekoPart } from "@/chat/sdk/types";

/** The artifact parts of a message, one per artifact (the daemon keys them by id). */
export function artifactCards(parts: NekoPart[]): ArtifactCardData[] {
  const byId = new Map<string, ArtifactCardData>();
  for (const part of parts) {
    if (part.type !== "data-neko-artifact") continue;
    const data: ArtifactCardData = part.data;
    if (data && typeof data.id === "string" && data.id) byId.set(data.id, data);
  }
  return [...byId.values()];
}

/** Opens the artifact's own tab (shell/tabs.ts). */
export const openArtifactTab = (id: string, n?: number | null) => {
  location.hash = artifactHref(id, n).slice(1);
};

/** The same actions as the card's and the panel's buttons, for the right-click menu. `version` null is the newest. */
export function artifactMenu(id: string, version: number | null, { beside = true }: { beside?: boolean } = {}): MenuEntry[] {
  const n = version ?? ("latest" as unknown as number);
  return [
    { label: "Open in a tab", icon: ArrowSquareOut, run: () => openArtifactTab(id, version) },
    beside ? { label: "Open beside the chat", icon: SidebarSimple, run: () => openCanvas(id, version) } : null,
    "separator",
    {
      label: "Download",
      icon: DownloadSimple,
      run: () => {
        const a = document.createElement("a");
        a.href = downloadUrl(id, n);
        a.download = "";
        a.click();
      },
    },
    { label: "Open in browser", icon: ArrowSquareOut, run: () => void window.open(artifactUrl(id, n), "_blank", "noreferrer") },
    { label: "Copy link", icon: Link, run: () => void copyText(`${location.origin}${location.pathname}${artifactHref(id, version)}`, "Link copied") },
  ];
}

export function ArtifactCard({ card }: { card: ArtifactCardData }) {
  const canvas = useCanvas();
  const id = card.id ?? "";
  const open = canvas?.id === id;
  const title = card.title || "Artifact";
  const version = card.version ?? null;
  const menu = useContextMenu(() => artifactMenu(id, version));
  return (
    <div
      {...menu}
      data-slot="artifact-card"
      data-artifact={id}
      data-open={open || undefined}
      className={cn(
        "not-prose my-2 flex items-center gap-3 rounded-xl border bg-card/60 px-3 py-2.5 shadow-xs",
        open ? "border-primary/50" : "border-border/70",
      )}
    >
      <span aria-hidden className="flex size-8 shrink-0 items-center justify-center rounded-lg bg-foreground/[0.06] text-muted-foreground">
        <FrameCorners size={18} />
      </span>
      <div className="min-w-0 flex-1">
        <p className="truncate text-sm font-medium text-foreground">{title}</p>
        <p className="text-xs text-muted-foreground tabular-nums">
          v{card.version ?? 1} · {(card.kind ?? "file").toUpperCase()}
        </p>
      </div>
      <Button variant="ghost" size="icon-sm" aria-label={`Open ${title} in a tab`} title="Open in a tab" onClick={() => openArtifactTab(id, version)}>
        <ArrowSquareOut />
      </Button>
      <Button variant="outline" size="sm" aria-label={`Open ${title} on the canvas`} onClick={() => openCanvas(id, version)}>
        {open ? "Showing" : "Open"}
      </Button>
    </div>
  );
}
