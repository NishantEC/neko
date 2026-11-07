// A reply's fenced block whose language has a fence registered
// (`host.registerFence`, canvas/viewers.ts) is drawn by it: through a viewer
// (the block's text, or the artifact it names) or a component of its own.
// Anything else, and every block while the reply is still streaming, stays
// the code block it is. Built-in fences: ```csv, ```tsv, ```3d.
import { Component, Suspense, type ReactNode } from "react";
import { artifactUrl, latestVersion, useArtifact } from "@/canvas/artifacts";
import { artifactInFence, fenceFor, fenceViewer, useViewerRegistry, type Fence, type Viewer } from "@/canvas/viewers";
import { useViewerTheme, ViewerSlot } from "@/canvas/ArtifactPanel";
import { PluginBoundary } from "@/plugins";

/** A fence component that throws falls back to the code it was given. */
class FenceBoundary extends Component<{ fallback: ReactNode; children: ReactNode }, { failed: boolean }> {
  state = { failed: false };
  static getDerivedStateFromError() {
    return { failed: true };
  }
  render() {
    return this.state.failed ? this.props.fallback : this.props.children;
  }
}

function FenceArtifact({ viewer, id, version, fallback }: { viewer: Viewer; id: string; version: number | null; fallback: ReactNode }) {
  const { artifact, error } = useArtifact(id);
  const theme = useViewerTheme();
  if (error) return <>{fallback}</>;
  if (!artifact) return <p className="my-2 text-sm text-muted-foreground" aria-busy="true">Opening artifact {id}…</p>;
  const n = version && artifact.versions.some((v) => v.n === version) ? version : latestVersion(artifact);
  const kind = artifact.versions.find((v) => v.n === n)?.file.split(".").pop()?.toLowerCase() ?? artifact.kind;
  return (
    <div data-slot="reply-fence" data-viewer={viewer.id} data-artifact={id} className="not-prose my-2">
      <ViewerSlot viewer={viewer} resetKey={`${id}:${n}`} props={{ artifact, version: n, kind, url: artifactUrl(id, n), theme, inline: true }} />
    </div>
  );
}

function FenceText({ viewer, code, language }: { viewer: Viewer; code: string; language: string }) {
  const theme = useViewerTheme();
  return (
    <div data-slot="reply-fence" data-viewer={viewer.id} data-language={language} className="not-prose my-2">
      <ViewerSlot viewer={viewer} resetKey={`${language}:${code.length}`} props={{ artifact: null, version: 0, kind: language, url: null, text: code, theme, inline: true }} />
    </div>
  );
}

function FenceComponent({ fence, code, language }: { fence: Fence; code: string; language: string }) {
  const View = fence.component!;
  const body = <View code={code} language={language} isStreaming={false} />;
  return (
    <div data-slot="reply-fence" data-language={language} className="not-prose my-2">
      <Suspense fallback={null}>{fence.plugin ? <PluginBoundary plugin={fence.plugin}>{body}</PluginBoundary> : body}</Suspense>
    </div>
  );
}

/** The block as its fence draws it, or `fallback` (the code block). */
export function FencedBlock({ code, language, isStreaming, fallback }: { code: string; language: string; isStreaming: boolean; fallback: ReactNode }) {
  useViewerRegistry();
  const fence = isStreaming ? null : fenceFor(language);
  if (!fence) return <>{fallback}</>;
  const text = code.replace(/\n$/, "");
  let drawn: ReactNode = null;
  if (fence.component) drawn = <FenceComponent fence={fence} code={text} language={language} />;
  else {
    const viewer = fenceViewer(fence);
    if (!viewer) return <>{fallback}</>;
    const named = artifactInFence(text);
    drawn = named ? <FenceArtifact viewer={viewer} id={named.id} version={named.version} fallback={fallback} /> : <FenceText viewer={viewer} code={text} language={language} />;
  }
  return <FenceBoundary fallback={fallback}>{drawn}</FenceBoundary>;
}
