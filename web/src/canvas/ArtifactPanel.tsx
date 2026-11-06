// The canvas: one artifact, live, with its versions, Download, Open in browser
// and Comment. Drawn in the inspector beside Home's conversation.
//
// **Isolation.** An artifact is shown in `<iframe sandbox="allow-scripts">`
// (never `allow-same-origin`) and served under its own CSP with `sandbox`
// (web.rs `ARTIFACT_CSP`): its scripts run, but in an opaque origin, so it can
// read neither Neko's cookie nor its storage, and reach no API.
//
// **Comment mode.** The daemon appends a small bridge to html artifacts. The
// canvas posts `{type:"neko-comment-mode", on}`; the bridge outlines what the
// pointer is over and, on click, posts `{type:"neko-pick", selector, text,
// rect}` back. Only messages whose `event.source` is this very iframe are read.
import { useEffect, useRef, useState } from "react";
import { Button } from "@/ui/button";
import { ArrowSquareOut, CursorClick, DownloadSimple, File as FileIcon } from "@/ui/icons";
import { cn } from "@/ui/lib/utils";
import { Pick } from "@/setup/kit";
import { relativeTime, useNow } from "@/setup/lib";
import {
  artifactUrl, canComment, downloadUrl, latestVersion, useArtifact, versionKind,
  type Artifact, type CanvasTarget,
} from "@/canvas/artifacts";
import { artifactMenu } from "@/canvas/ArtifactCard";
import { useContextMenu } from "@/shell/context-menu";

/** What a click in comment mode pointed at. */
export interface ElementPick {
  selector: string;
  text: string;
}

/**
 * One version of an artifact: a live sandboxed frame for html and svg, the
 * image for png, a download for the rest. `commenting` turns the bridge on.
 */
export function ArtifactFrame({
  artifact, n, commenting = false, onPick, onCancel, className,
}: {
  artifact: Artifact;
  n: number;
  commenting?: boolean;
  onPick?: (pick: ElementPick) => void;
  onCancel?: () => void;
  className?: string;
}) {
  const frame = useRef<HTMLIFrameElement>(null);
  const kind = versionKind(artifact, n);
  const src = artifactUrl(artifact.id, n);
  const handlers = useRef({ onPick, onCancel });
  handlers.current = { onPick, onCancel };
  const mode = useRef(commenting);
  mode.current = commenting;

  // The frame has an opaque origin, so a message to it can only be addressed to "*".
  const post = () => frame.current?.contentWindow?.postMessage({ type: "neko-comment-mode", on: mode.current }, "*");
  useEffect(post, [commenting, src]);

  useEffect(() => {
    const onMessage = (event: MessageEvent) => {
      if (!frame.current || event.source !== frame.current.contentWindow) return;
      const data = event.data;
      if (data?.type === "neko-pick" && typeof data.selector === "string" && data.selector) {
        handlers.current.onPick?.({ selector: data.selector.slice(0, 300), text: typeof data.text === "string" ? data.text.slice(0, 120) : "" });
      } else if (data?.type === "neko-comment-cancel") {
        handlers.current.onCancel?.();
      }
    };
    window.addEventListener("message", onMessage);
    return () => window.removeEventListener("message", onMessage);
  }, []);

  if (kind === "html" || kind === "svg") {
    return (
      <iframe
        ref={frame}
        key={src}
        src={src}
        title={`${artifact.title}, version ${n}`}
        sandbox="allow-scripts"
        referrerPolicy="no-referrer"
        onLoad={post}
        data-slot="artifact-frame"
        data-commenting={commenting || undefined}
        className={cn("block h-full w-full border-0 bg-white", commenting && "ring-2 ring-inset ring-primary", className)}
      />
    );
  }
  if (kind === "png") {
    return (
      <div className={cn("flex h-full w-full items-center justify-center overflow-auto bg-muted/40 p-4", className)}>
        <img src={src} alt={`${artifact.title}, version ${n}`} className="max-h-full max-w-full rounded-md shadow-sm" />
      </div>
    );
  }
  return (
    <div className={cn("flex h-full w-full flex-col items-center justify-center gap-3 p-6 text-center text-sm text-muted-foreground", className)}>
      <FileIcon size={28} aria-hidden />
      <p>A {kind.toUpperCase()} file. The canvas draws html and svg; download this one to open it.</p>
      <Button variant="outline" size="sm" render={<a href={downloadUrl(artifact.id, n)} download />}>
        <DownloadSimple /> Download
      </Button>
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
    <div {...menu} data-slot="artifact-panel" data-artifact={artifact.id} data-version={n} className="flex h-full min-h-0 flex-col">
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
        />
      </div>
    </div>
  );
}
