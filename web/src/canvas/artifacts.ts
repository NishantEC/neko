// Artifacts: files an agent made (a design, a document, a dashboard), kept by
// the daemon (`neko_core::artifacts`) and drawn on the canvas beside the chat.
// See docs/knowledge/the-canvas-beside-the-chat.md.
//
//   request({ Artifact: { id } })              one record
//   /artifact/<id>/<n>                         version n, under its own CSP
//   ArtifactsChanged { id, version, message_id }   pushed on every save
//   ShowArtifact { id, version } / OpenTab { route }   an agent asks every page to show (obeyAgentRequests)
//   EditArtifact { artifact_id, content, note }   a viewer saves the next version (editArtifact)
//
// Which artifact the canvas shows is kept here, outside any page, so a card,
// a menu or an artifact's own tab can open one and Home shows it beside the
// conversation. There is no gallery page (removed 2026-11-06, owner's call):
// an artifact lives beside the reply that made it, or in a tab of its own.
import { useEffect, useState, useSyncExternalStore } from "react";
import { onEvent, request, type Json } from "@/api";
import { HOME } from "@/router";

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
  /** The newest version's file extension: `html`, `md`, `csv`, `glb`, `excalidraw`, `png`… (any). */
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

/** Only html carries the comment bridge. */
export const canComment = (kind: string) => kind === "html";

/** An artifact's own tab in the app (shell/tabs.ts keys it `artifact:<id>`); `artifactUrl` is the file itself. */
export const artifactHref = (id: string, n?: number | null) => `#/artifact?id=${encodeURIComponent(id)}${n ? `&v=${n}` : ""}`;
export const artifactUrl = (id: string, n: number) => `/artifact/${encodeURIComponent(id)}/${n}`;
export const downloadUrl = (id: string, n: number) => `${artifactUrl(id, n)}?download=1`;

export const latestVersion = (artifact: Artifact) => artifact.versions.reduce((max, v) => Math.max(max, v.n), 0);
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

/** The route on screen (`home`, `tasks`, `artifact`…), read from the address. */
const currentRoute = () => location.hash.replace(/^#\/?/, "").split("?")[0] || HOME;

/**
 * Shows an artifact as the agent asked (`ShowArtifact`, `SaveArtifact` with
 * `show`): beside the chat when Home is on screen, otherwise in the
 * artifact's own tab. The canvas target is set either way, so going back to
 * Home finds it there.
 */
export function showArtifact(id: string, version: number | null = null) {
  openCanvas(id, version);
  if (currentRoute() !== HOME) location.hash = artifactHref(id, version).slice(1);
}

/** Saves `content` as the next version of a text artifact (a viewer that edits); resolves to the new version. */
export async function editArtifact(id: string, content: string, note = ""): Promise<number> {
  const reply = await request({ EditArtifact: { artifact_id: id, content, note } });
  const version = reply?.ArtifactSaved?.version;
  if (typeof version !== "number") throw new Error(reply?.Error?.message ?? "The new version was not saved.");
  return version;
}

const isShow = (event: Json): event is { ShowArtifact: { id: string; version: number | null } } =>
  !!event && typeof event === "object" && "ShowArtifact" in event;
const isOpenTab = (event: Json): event is { OpenTab: { route: string } } =>
  !!event && typeof event === "object" && "OpenTab" in event;

/** A Neko route as the daemon checked it (`neko_core::viewers::tab_route`), checked again: never another site. */
export function tabHash(route: string): string | null {
  const clean = route.replace(/^#?\/?/, "");
  if (clean.length > 300 || !/^[A-Za-z0-9\-_:/?=&.%]*$/.test(clean) || clean.includes("//") || /^[a-z]+:\/?\//i.test(clean)) return null;
  return `#/${clean}`;
}

/**
 * What an agent asks every open page to do (`ShowArtifact`, `OpenTab`).
 * Mounted once by the shell; returns the unsubscribe.
 */
export function obeyAgentRequests(): () => void {
  return onEvent((event) => {
    if (isShow(event)) {
      const { id, version } = event.ShowArtifact;
      if (typeof id === "string" && /^[A-Za-z0-9_-]{1,64}$/.test(id)) showArtifact(id, typeof version === "number" ? version : null);
    } else if (isOpenTab(event)) {
      const hash = tabHash(String(event.OpenTab.route ?? ""));
      if (hash) location.hash = hash.slice(1);
    }
  });
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
