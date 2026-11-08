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
      // One quiet row (DESIGN.md §8): no box inside the reply. The whole row
      // opens the canvas; the tab button appears on hover and is in the menu.
      className={cn(
        "group/row not-prose my-2 -mx-2 flex min-h-10 items-center gap-2.5 rounded-[10px] px-2 transition-colors hover:bg-foreground/[0.04]",
        open && "bg-primary/[0.06]",
      )}
    >
      <button
        type="button"
        aria-label={`Open ${title} on the canvas`}
        onClick={() => openCanvas(id, version)}
        className="flex min-w-0 flex-1 items-center gap-2.5 self-stretch rounded-md text-start outline-none focus-visible:ring-2 focus-visible:ring-ring/70"
      >
        <FrameCorners aria-hidden size={16} className={cn("shrink-0", open ? "text-primary" : "text-muted-foreground")} />
        <span className="min-w-0 truncate text-[13.5px] text-foreground">{title}</span>
        <span className="shrink-0 text-[12.5px] tabular-nums text-muted-foreground/80">
          v{card.version ?? 1} · {(card.kind ?? "file").toUpperCase()}
        </span>
        <span className="ms-auto shrink-0 text-[12.5px] text-muted-foreground">{open ? "Showing" : "Open"}</span>
      </button>
      <Button variant="ghost" size="icon-sm" aria-label={`Open ${title} in a tab`} title="Open in a tab" onClick={() => openArtifactTab(id, version)} className="opacity-0 transition-opacity group-hover/row:opacity-100 group-focus-within/row:opacity-100 [@media(hover:none)]:opacity-100">
        <ArrowSquareOut />
      </Button>
    </div>
  );
}
