// The canvas: one artifact, live, with its versions, Download, Open in browser
// and Comment. Drawn in the inspector beside Home's conversation, and in the
// artifact's own tab.
//
// **Viewers.** What draws a version is the viewer registered for its kind
// (canvas/viewers.ts: the built-ins in builtin-viewers.tsx, plus any a plugin
// registers), else a Download. A viewer that edits (the sketch) gets
// `onEdit`, which saves the next version and moves the panel to it.
//
// **Isolation.** html and svg are shown in `<iframe sandbox="allow-scripts">`
// (never `allow-same-origin`) and served under their own CSP with `sandbox`
// (web.rs `ARTIFACT_CSP`): their scripts run in an opaque origin, so they can
// read neither Neko's cookie nor its storage, and reach no API. Every other
// kind is a file the viewer reads; nothing in it runs.
//
// **Comment mode.** The daemon appends a small bridge to html artifacts. The
// canvas posts `{type:"neko-comment-mode", on}`; the bridge outlines what the
// pointer is over and, on click, posts `{type:"neko-pick", selector, text,
// rect}` back. Only messages whose `event.source` is this very iframe are read.
import { Component, Suspense, useEffect, useMemo, useState, type ReactNode } from "react";
import { Button } from "@/ui/button";
import { ArrowSquareOut, CursorClick, DownloadSimple, File as FileIcon } from "@/ui/icons";
import { cn } from "@/ui/lib/utils";
import { Pick } from "@/setup/kit";
import { relativeTime, useNow } from "@/setup/lib";
import {
  artifactUrl, canComment, downloadUrl, editArtifact, latestVersion, useArtifact, versionKind,
  type Artifact, type CanvasTarget,
} from "@/canvas/artifacts";
import { useViewerRegistry, viewerFor, type ElementPick, type Viewer, type ViewerProps } from "@/canvas/viewers";
import { artifactMenu } from "@/canvas/ArtifactCard";
import { useContextMenu } from "@/shell/context-menu";
import { PluginBoundary } from "@/plugins";
import { getThemeState, useThemes } from "@/theme";

export type { ElementPick };

/** The theme on screen, for viewers that draw their own colours (a canvas, WebGL). */
export function useViewerTheme(): ViewerProps["theme"] {
  const themes = useThemes();
  return useMemo(() => getThemeState(), [themes]); // eslint-disable-line react-hooks/exhaustive-deps
}

/** A built-in viewer that throws says so in place; the rest of the panel carries on. */
class ViewerBoundary extends Component<{ children: ReactNode }, { error: string | null }> {
  state = { error: null as string | null };
  static getDerivedStateFromError(error: unknown) {
    return { error: error instanceof Error ? error.message : String(error) };
  }
  render() {
    return this.state.error ? <p role="alert" className="p-4 text-sm text-destructive">This viewer stopped: {this.state.error}</p> : this.props.children;
  }
}

/** A viewer in its error boundary (a plugin's crash is reported to `PluginStatus`), loaded lazily. */
export function ViewerSlot({ viewer, props, resetKey }: { viewer: Viewer; props: ViewerProps; resetKey: string }) {
  const View = viewer.component;
  const body = (
    <Suspense fallback={<p className="p-4 text-sm text-muted-foreground" aria-busy="true">Opening the {viewer.title.toLowerCase()}…</p>}>
      <View {...props} />
    </Suspense>
  );
  return viewer.plugin
    ? <PluginBoundary key={resetKey} plugin={viewer.plugin}>{body}</PluginBoundary>
    : <ViewerBoundary key={resetKey}>{body}</ViewerBoundary>;
}

/**
 * One version of an artifact, drawn by the viewer for its kind, or offered
 * as a download when none is installed. `commenting` turns the html bridge
 * on; `onEdited` hears of a version a viewer saved.
 */
export function ArtifactFrame({
  artifact, n, commenting = false, onPick, onCancel, onEdited, className,
}: {
  artifact: Artifact;
  n: number;
  commenting?: boolean;
  onPick?: (pick: ElementPick) => void;
  onCancel?: () => void;
  onEdited?: (n: number) => void;
  className?: string;
}) {
  useViewerRegistry();
  const theme = useViewerTheme();
  const kind = versionKind(artifact, n);
  const viewer = viewerFor(kind);
  if (!viewer) {
    return (
      <div data-slot="artifact-download" className={cn("flex h-full w-full flex-col items-center justify-center gap-3 p-6 text-center text-sm text-muted-foreground", className)}>
        <FileIcon size={28} aria-hidden />
        <p>A .{kind} file. No viewer for it is installed; download it to open it.</p>
        <Button variant="outline" size="sm" render={<a href={downloadUrl(artifact.id, n)} download />}>
          <DownloadSimple /> Download
        </Button>
      </div>
    );
  }
  const onEdit = viewer.edits
    ? async (content: string, note?: string) => {
        const saved = await editArtifact(artifact.id, n, content, note ?? "");
        onEdited?.(saved);
        return saved;
      }
    : undefined;
  return (
    <div data-slot="artifact-view" data-viewer={viewer.id} data-kind={kind} className={cn("h-full w-full", className)}>
      <ViewerSlot
        viewer={viewer}
        resetKey={`${artifact.id}:${n}:${viewer.id}`}
        props={{ artifact, version: n, kind, url: artifactUrl(artifact.id, n), theme, onEdit, commenting, onPick, onCancel }}
      />
    </div>
  );
}

/**
 * The panel beside the chat. `onComment` receives the element picked in
 * comment mode; without it there is no Comment button.
 */
export function ArtifactPanel({
  target, onVersion, onComment,
}: {
  target: CanvasTarget;
  onVersion?: (n: number) => void;
  onComment?: (artifact: Artifact, n: number, pick: ElementPick) => void;
}) {
  const { artifact, error } = useArtifact(target.id);
  const [commenting, setCommenting] = useState(false);
  const now = useNow();
  // Escape inside the frame ends comment mode; so does a new artifact.
  useEffect(() => setCommenting(false), [target.id]);
  // The right-click menu: the toolbar's actions plus its own tab (the frame itself is another document).
  const shownVersion = artifact && target.version && artifact.versions.some((v) => v.n === target.version) ? target.version : artifact ? latestVersion(artifact) : null;
  const menu = useContextMenu(() => artifactMenu(target.id, shownVersion, { beside: false }));

  if (error) return <p role="alert" className="p-4 text-sm text-destructive">{error}</p>;
  if (!artifact) return <p className="p-4 text-sm text-muted-foreground" aria-busy="true">Opening…</p>;
  const latest = latestVersion(artifact);
  const n = target.version && artifact.versions.some((v) => v.n === target.version) ? target.version : latest;
  const version = artifact.versions.find((v) => v.n === n);
  const kind = versionKind(artifact, n);
  const commentable = !!onComment && canComment(kind);
  const options = [...artifact.versions].sort((a, b) => b.n - a.n).map((v) => ({
    value: String(v.n),
    label: `v${v.n}${v.n === latest ? " · latest" : ""}`,
    hint: v.note ? v.note.slice(0, 40) : undefined,
  }));

  return (
    <div {...menu} data-slot="artifact-panel" data-artifact={artifact.id} data-version={n} data-kind={kind} className="flex h-full min-h-0 flex-col">
      <div role="toolbar" aria-label="Canvas" className="flex flex-wrap items-center gap-1.5 border-b border-border/70 px-3 py-2">
        {artifact.versions.length > 1 ? (
          <Pick label="Version" value={String(n)} onChange={(value) => onVersion?.(Number(value))} options={options} className="min-w-28" />
        ) : (
          <span className="px-1 text-xs text-muted-foreground tabular-nums">v{n}</span>
        )}
        {version ? <span className="truncate text-xs text-muted-foreground">{relativeTime(version.at_ms, now)}</span> : null}
        <span className="flex-1" />
        {commentable ? (
          <Button
            variant={commenting ? "default" : "outline"}
            size="sm"
            aria-pressed={commenting}
            title="Point at an element to comment on it"
            onClick={() => setCommenting((on) => !on)}
          >
            <CursorClick /> {commenting ? "Pick an element" : "Comment"}
          </Button>
        ) : null}
        <Button variant="ghost" size="icon-sm" aria-label="Download" title="Download" render={<a href={downloadUrl(artifact.id, n)} download />}>
          <DownloadSimple />
        </Button>
        <Button variant="ghost" size="icon-sm" aria-label="Open in browser" title="Open in browser" render={<a href={artifactUrl(artifact.id, n)} target="_blank" rel="noreferrer" />}>
          <ArrowSquareOut />
        </Button>
      </div>
      {commenting ? <p role="status" className="border-b border-border/70 bg-primary/8 px-3 py-1.5 text-xs text-foreground">Click an element in the design to comment on it. Esc stops.</p> : null}
      {version?.note ? <p className="border-b border-border/70 px-3 py-1.5 text-xs text-muted-foreground">{version.note}</p> : null}
      <div className="min-h-0 flex-1">
        <ArtifactFrame
          artifact={artifact}
          n={n}
          commenting={commenting}
          onPick={(pick) => {
            setCommenting(false);
            onComment?.(artifact, n, pick);
          }}
          onCancel={() => setCommenting(false)}
          onEdited={(saved) => onVersion?.(saved)}
        />
      </div>
    </div>
  );
}
