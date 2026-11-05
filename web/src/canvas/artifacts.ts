// Artifacts: files an agent made (a design, a document, a dashboard), kept by
// the daemon (`neko_core::artifacts`) and drawn on the canvas beside the chat.
// See docs/knowledge/the-canvas-beside-the-chat.md.
//
//   request({ Artifacts: { workspace_id } })   the list, newest first
//   request({ Artifact: { id } })              one record
//   /artifact/<id>/<n>                         version n, under its own CSP
//   ArtifactsChanged { id, version, message_id }   pushed on every save
//
// Which artifact the canvas shows is kept here, outside any page, so the Canvas
// page can open one and Home shows it beside the conversation.
import { useCallback, useEffect, useState, useSyncExternalStore } from "react";
import { onEvent, request, type Json } from "@/api";

export interface ArtifactVersion {
  n: number;
  file: string;
  at_ms: number;
  note?: string;
  message_id?: string | null;
}

export interface Artifact {
  id: string;
  title: string;
  /** `html`, `svg`, `png`, `pdf`, `json` or `md`. */
  kind: string;
  workspace_id?: string | null;
  /** The Home reply that made it. */
  message_id?: string | null;
  versions: ArtifactVersion[];
}

/** What a `data-neko-artifact` part on a reply carries. */
export interface ArtifactCardData {
  id?: string;
  title?: string;
  kind?: string;
  version?: number;
}

/** Kinds the canvas draws live; the others are shown as a download. */
export const LIVE_KINDS = ["html", "svg"];
export const isLive = (kind: string) => LIVE_KINDS.includes(kind);
/** Only html carries the comment bridge. */
export const canComment = (kind: string) => kind === "html";

export const artifactUrl = (id: string, n: number) => `/artifact/${encodeURIComponent(id)}/${n}`;
export const downloadUrl = (id: string, n: number) => `${artifactUrl(id, n)}?download=1`;

export const latestVersion = (artifact: Artifact) => artifact.versions.reduce((max, v) => Math.max(max, v.n), 0);
export const updatedMs = (artifact: Artifact) => artifact.versions.reduce((max, v) => Math.max(max, v.at_ms), 0);
/** The kind of one version, from its file name (versions may differ). */
export const versionKind = (artifact: Artifact, n: number) => artifact.versions.find((v) => v.n === n)?.file.split(".").pop()?.toLowerCase() ?? artifact.kind;

/** One element of one version, written into the message so the agent knows exactly what to change. */
export function referenceText(id: string, n: number, selector: string, text: string): string {
  const quoted = text.replace(/\s+/g, " ").trim().slice(0, 120).replace(/"/g, "'");
  return `[artifact ${id} v${n}, element ${selector}: "${quoted}"]`;
}

const isChange = (event: Json): event is { ArtifactsChanged: { id: string; version: number; message_id: string | null } } =>
  !!event && typeof event === "object" && "ArtifactsChanged" in event;

/** Runs `fn` for every artifact saved from now on. */
export function onArtifactSaved(fn: (change: { id: string; version: number; message_id: string | null }) => void): () => void {
  const stop = onEvent((event) => {
    if (isChange(event)) fn(event.ArtifactsChanged);
  });
  return () => { stop(); };
}

/** Every artifact (one workspace's with `workspaceId`), kept current by the daemon's events. */
export function useArtifacts(workspaceId: string | null) {
  const [artifacts, setArtifacts] = useState<Artifact[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const load = useCallback(async () => {
    try {
      const reply = await request({ Artifacts: { workspace_id: workspaceId } });
      setArtifacts((reply?.Artifacts?.artifacts ?? []) as Artifact[]);
      setError(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }, [workspaceId]);
  useEffect(() => {
    void load();
    return onArtifactSaved(() => void load());
  }, [load]);
  return { artifacts, error, reload: load };
}

/** One artifact's record, read again whenever it gets a new version. */
export function useArtifact(id: string | null) {
  const [artifact, setArtifact] = useState<Artifact | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    setArtifact(null);
    setError(null);
    if (!id) return;
    let live = true;
    const load = async () => {
      try {
        const reply = await request({ Artifact: { id } });
        if (live) setArtifact(reply?.Artifact?.artifact ?? null);
      } catch (e) {
        if (live) setError(e instanceof Error ? e.message : String(e));
      }
    };
    void load();
    const stop = onArtifactSaved((change) => { if (change.id === id) void load(); });
    return () => { live = false; stop(); };
  }, [id]);
  return { artifact, error };
}

// ---------------------------------------------------------------------------
// What the canvas shows
// ---------------------------------------------------------------------------

/** `version` null follows the newest. */
export interface CanvasTarget { id: string; version: number | null }

let target: CanvasTarget | null = null;
const listeners = new Set<() => void>();
const emit = () => listeners.forEach((l) => l());

export function openCanvas(id: string, version: number | null = null) {
  if (target?.id === id && target.version === version) return;
  target = { id, version };
  emit();
}

export function closeCanvas() {
  if (!target) return;
  target = null;
  emit();
}

export function useCanvas(): CanvasTarget | null {
  return useSyncExternalStore(
    (listener) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    () => target,
  );
}
