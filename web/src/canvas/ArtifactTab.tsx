// An artifact in a tab of its own: `#/artifact?id=<id>&v=<n>` (shell/tabs.ts
// keys it `artifact:<id>`). The same panel as beside the chat; commenting on
// an element needs the composer, so "Comment beside the chat" takes it there.
import { Button } from "@/ui/button";
import { ChatCircle } from "@/ui/icons";
import { HOME, navigate } from "@/router";
import { setHashParams, useHashParams } from "@/work/location";
import { ArtifactPanel } from "@/canvas/ArtifactPanel";
import { openCanvas, useArtifact } from "@/canvas/artifacts";

export function ArtifactTab() {
  const params = useHashParams();
  const id = params.get("id");
  const v = Number(params.get("v"));
  const version = Number.isFinite(v) && v > 0 ? v : null;
  const { artifact } = useArtifact(id);
  if (!id) return <p className="p-6 text-sm text-muted-foreground">No artifact named in this address.</p>;
  return (
    <div data-slot="artifact-tab" className="flex min-h-0 flex-1 flex-col">
      <header className="flex shrink-0 items-center gap-3 px-6 pt-4 pb-3">
        <h1 className="min-w-0 flex-1 truncate text-[17px] font-semibold text-strong">{artifact?.title ?? "Artifact"}</h1>
        <Button type="button" variant="outline" size="sm" onClick={() => { openCanvas(id, version); navigate(HOME); }}>
          <ChatCircle /> Comment beside the chat
        </Button>
      </header>
      <div className="min-h-0 flex-1 border-t border-border/60">
        <ArtifactPanel target={{ id, version }} onVersion={(n) => setHashParams("artifact", { id, v: String(n) })} />
      </div>
    </div>
  );
}
