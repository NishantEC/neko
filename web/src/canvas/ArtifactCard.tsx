// An artifact's card on the reply that made it (`data-neko-artifact` parts,
// added by the daemon when the agent saves one). Open shows it on the canvas
// beside the conversation.
import { Button } from "@/ui/button";
import { FrameCorners } from "@/ui/icons";
import { cn } from "@/ui/lib/utils";
import { openCanvas, useCanvas, type ArtifactCardData } from "@/canvas/artifacts";
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

export function ArtifactCard({ card }: { card: ArtifactCardData }) {
  const canvas = useCanvas();
  const id = card.id ?? "";
  const open = canvas?.id === id;
  const title = card.title || "Artifact";
  return (
    <div
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
      <Button variant="outline" size="sm" aria-label={`Open ${title} on the canvas`} onClick={() => openCanvas(id, card.version ?? null)}>
        {open ? "Showing" : "Open"}
      </Button>
    </div>
  );
}
