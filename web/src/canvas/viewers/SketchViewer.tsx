// A sketch: .excalidraw artifacts, drawn and edited with Excalidraw 0.18.1
// (MIT, web/package.json; licence in node_modules/@excalidraw/excalidraw and
// web/src/ui/NOTICE.md), in a chunk of its own loaded the first time a sketch
// opens. "Save as v<n+1>" writes the board as the artifact's next version
// (`EditArtifact`); nothing is saved until then.
//
// Excalidraw's fonts are served by Neko itself from `/excalidraw/fonts/`
// (copied from the package at build time by vite.config.ts, without the
// 12 MB Xiaolai CJK font): `EXCALIDRAW_ASSET_PATH` points there, since the
// page policy refuses fonts from other sites. Modelled on the owner's Canvas
// plugin (`<data dir>/plugins/canvas`, written by Neko, 2026-11).
import "./excalidraw-assets";
import { useEffect, useMemo, useRef, useState } from "react";
import { Excalidraw, MainMenu, serializeAsJSON, restore, getSceneVersion } from "@excalidraw/excalidraw";
import "@excalidraw/excalidraw/index.css";
import { Button } from "@/ui/button";
import { toast } from "@/ui/sonner";
import { FloppyDisk } from "@/ui/icons";
import { useViewerText, ViewerProblem } from "@/canvas/builtin-viewers";
import type { ViewerProps } from "@/canvas/viewers";

/* eslint-disable @typescript-eslint/no-explicit-any */
type Scene = { elements: readonly any[]; appState: any; files: any };

const UI_OPTIONS = { canvasActions: { loadScene: false, saveToActiveFile: false, toggleTheme: null, export: false as const } };

export default function SketchViewer(props: ViewerProps) {
  const { text, error } = useViewerText(props);
  const [busy, setBusy] = useState(false);
  const [dirty, setDirty] = useState(false);
  const latest = useRef<Scene | null>(null);
  const baseline = useRef<number | null>(null);

  const initial = useMemo(() => {
    if (text === null) return null;
    try {
      const data = JSON.parse(text);
      if (!data || typeof data !== "object" || !Array.isArray(data.elements)) throw new Error("no elements");
      const restored = restore({ elements: data.elements, appState: data.appState ?? {}, files: data.files ?? {} }, null, null);
      return { elements: restored.elements, appState: { ...restored.appState, collaborators: new Map() }, files: restored.files, scrollToContent: true };
    } catch {
      return "invalid" as const;
    }
  }, [text]);

  // A new version on screen is the new baseline: nothing to save yet.
  useEffect(() => { baseline.current = null; setDirty(false); }, [text]);

  if (error) return <ViewerProblem message={error} />;
  if (text === null) return <p className="p-4 text-sm text-muted-foreground" aria-busy="true">Opening the sketch…</p>;
  if (initial === "invalid" || initial === null) return <ViewerProblem message="This file is not an Excalidraw scene (it needs an “elements” list). Download it to look at it as text." />;

  const save = async () => {
    if (!props.onEdit || !latest.current) return;
    setBusy(true);
    try {
      const { elements, appState, files } = latest.current;
      const json = serializeAsJSON(elements, appState, files, "local");
      const n = await props.onEdit(json, "Edited in Neko");
      toast.success(`Saved as v${n}`);
      setDirty(false);
    } catch (e) {
      toast.error("The sketch was not saved", { description: e instanceof Error ? e.message : String(e) });
    } finally {
      setBusy(false);
    }
  };

  return (
    <div data-slot="sketch-viewer" className={props.inline ? "flex h-96 flex-col overflow-hidden rounded-lg border border-border/70" : "flex h-full min-h-0 flex-col"}>
      {props.onEdit ? (
        <div role="toolbar" aria-label="Sketch" className="flex items-center gap-2 border-b border-border/70 px-3 py-1.5 text-xs text-muted-foreground">
          <span className="flex-1">{dirty ? "Unsaved changes" : "Draw on the board; saving keeps it as the next version."}</span>
          <Button size="xs" onClick={() => void save()} disabled={!dirty || busy}>
            <FloppyDisk /> {busy ? "Saving…" : `Save as v${(props.artifact?.versions.reduce((m, v) => Math.max(m, v.n), 0) ?? props.version) + 1}`}
          </Button>
        </div>
      ) : null}
      <div className="relative min-h-0 flex-1" data-slot="sketch-board">
        <Excalidraw
          key={`${props.url ?? ""}:${props.version}`}
          initialData={initial}
          theme={props.theme.appearance === "dark" ? "dark" : "light"}
          viewModeEnabled={!props.onEdit}
          UIOptions={UI_OPTIONS}
          langCode="en"
          onChange={(elements: readonly any[], appState: any, files: any) => {
            latest.current = { elements, appState, files };
            const version = getSceneVersion(elements as any);
            if (baseline.current === null) baseline.current = version;
            else if (version !== baseline.current) setDirty(true);
          }}
        >
          <MainMenu>
            <MainMenu.DefaultItems.SaveAsImage />
            <MainMenu.DefaultItems.ClearCanvas />
            <MainMenu.Separator />
            <MainMenu.DefaultItems.ChangeCanvasBackground />
            <MainMenu.DefaultItems.Help />
          </MainMenu>
        </Excalidraw>
      </div>
    </div>
  );
}
/* eslint-enable @typescript-eslint/no-explicit-any */
