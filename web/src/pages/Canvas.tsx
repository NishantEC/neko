// Canvas: every artifact Neko made, as frames on a board (a grid, like the
// frames of a Figma page), newest first. Filtered by the workspace chosen in
// the sidebar. Opening one returns to the conversation with it on the canvas
// beside the chat (Home).
import { useEffect, useRef, useState } from "react";
import { Button } from "@/ui/button";
import { File as FileIcon, FrameCorners } from "@/ui/icons";
import { EmptyBlock, ErrorNote, LoadingRows, Pick, SetupPage } from "@/setup/kit";
import { relativeTime, useNow } from "@/setup/lib";
import { useSelectedWorkspace } from "@/shell/workspace";
import { HOME, navigate } from "@/router";
import {
  artifactUrl, latestVersion, openCanvas, updatedMs, useArtifacts, versionKind,
  type Artifact,
} from "@/canvas/artifacts";

/** The width a frame is laid out at before it is scaled into its tile. */
const FRAME_WIDTH = 1280;
const FRAME_HEIGHT = 800;

/** A live, scaled-down look at the latest version; a placeholder for kinds the canvas does not draw. */
function Thumbnail({ artifact }: { artifact: Artifact }) {
  const box = useRef<HTMLDivElement>(null);
  const [scale, setScale] = useState(0.25);
  useEffect(() => {
    const el = box.current;
    if (!el) return;
    const measure = () => setScale(el.clientWidth / FRAME_WIDTH || 0.25);
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(el);
    return () => observer.disconnect();
  }, []);
  const n = latestVersion(artifact);
  const kind = versionKind(artifact, n);
  const src = artifactUrl(artifact.id, n);
  return (
    <div ref={box} aria-hidden className="relative aspect-[16/10] w-full overflow-hidden rounded-t-[inherit] bg-muted/50">
      {kind === "html" || kind === "svg" ? (
        <iframe
          src={src}
          title=""
          tabIndex={-1}
          sandbox="allow-scripts"
          referrerPolicy="no-referrer"
          loading="lazy"
          className="pointer-events-none absolute top-0 left-0 origin-top-left border-0 bg-white"
          style={{ width: FRAME_WIDTH, height: FRAME_HEIGHT, transform: `scale(${scale})` }}
        />
      ) : kind === "png" ? (
        <img src={src} alt="" loading="lazy" className="h-full w-full object-contain" />
      ) : (
        <div className="flex h-full w-full flex-col items-center justify-center gap-2 text-muted-foreground">
          <FileIcon size={28} />
          <span className="text-xs font-medium">{kind.toUpperCase()}</span>
        </div>
      )}
    </div>
  );
}

function Frame({ artifact, workspace, now }: { artifact: Artifact; workspace?: string; now: number }) {
  const n = latestVersion(artifact);
  const open = () => {
    openCanvas(artifact.id, null);
    navigate(HOME);
  };
  return (
    <li data-slot="artifact-frame-tile" data-artifact={artifact.id} className="group/frame">
      <button
        type="button"
        onClick={open}
        aria-label={`Open ${artifact.title}`}
        className="flex w-full flex-col overflow-hidden rounded-[14px] border border-border/70 bg-card text-left shadow-xs outline-none transition-shadow hover:shadow-md focus-visible:ring-2 focus-visible:ring-ring"
      >
        <Thumbnail artifact={artifact} />
        <span className="flex flex-col gap-0.5 border-t border-border/70 px-3 py-2.5">
          <span className="truncate text-sm font-medium text-foreground">{artifact.title}</span>
          <span className="truncate text-xs text-muted-foreground tabular-nums">
            v{n} · {versionKind(artifact, n).toUpperCase()} · {relativeTime(updatedMs(artifact), now)}
            {workspace ? ` · ${workspace}` : ""}
          </span>
        </span>
      </button>
    </li>
  );
}

export default function Canvas() {
  const { id: workspaceId, workspaces, select } = useSelectedWorkspace();
  const { artifacts, error, reload } = useArtifacts(workspaceId || null);
  const now = useNow();
  const names = new Map(workspaces.map((w) => [w.id, w.name]));
  const options = [{ value: "*", label: "All workspaces" }, ...workspaces.map((w) => ({ value: w.id, label: w.name }))];
  return (
    <SetupPage
      eyebrow="Canvas"
      title="Canvas"
      subtitle="Designs, documents and dashboards Neko made for you, newest first. Open one to see it beside the chat."
      width="max-w-[1200px]"
      actions={
        workspaces.length ? (
          <Pick label="Workspace" value={workspaceId || "*"} onChange={(value) => select(value === "*" ? "" : value)} options={options} />
        ) : null
      }
    >
      <ErrorNote error={error} title="Couldn’t read the artifacts" onDismiss={() => void reload()} />
      {artifacts === null && !error ? <LoadingRows rows={2} /> : null}
      {artifacts && artifacts.length === 0 ? (
        <EmptyBlock
          icon={FrameCorners}
          title={workspaceId ? "Nothing made in this workspace yet" : "Nothing on the canvas yet"}
          description="Ask Neko in Home chat to design a screen, draft a document or build a dashboard. What it makes appears here and beside the conversation."
        >
          <Button variant="outline" size="sm" onClick={() => navigate(HOME)}>Go to Home</Button>
        </EmptyBlock>
      ) : null}
      {artifacts && artifacts.length > 0 ? (
        <ul aria-label="Artifacts" className="grid grid-cols-[repeat(auto-fill,minmax(260px,1fr))] gap-5">
          {artifacts.map((artifact) => (
            <Frame key={artifact.id} artifact={artifact} workspace={artifact.workspace_id ? names.get(artifact.workspace_id) : undefined} now={now} />
          ))}
        </ul>
      ) : null}
    </SetupPage>
  );
}
