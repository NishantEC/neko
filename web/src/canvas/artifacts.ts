// Artifacts: files an agent made (a design, a document, a dashboard), kept by
// the daemon (`neko_core::artifacts`) and drawn on the canvas beside the chat.
// See docs/knowledge/the-canvas-beside-the-chat.md.
//
//   request({ Artifact: { id } })              one record
//   /artifact/<id>/<n>                         version n, under its own CSP
//   ArtifactsChanged { id, version, message_id }   pushed on every save
//   ShowArtifact { id, version } / OpenTab { route }   an agent asks every page to show (obeyAgentRequests)
//   EditArtifact { artifact_id, version, content, note }   a viewer saves the next version (editArtifact)
//   ReportArtifactShown / ReportArtifactProblem   what drawing a version did, for the agent (reportShown, reportProblem)
//   a data-neko-send click in an html artifact    a draft in Home's composer, never a sent message (offerFromArtifact)
//
// Which artifact the canvas shows is kept here, outside any page, so a card,
// a menu or an artifact's own tab can open one and Home shows it beside the
// conversation. There is no gallery page (removed 2026-11-06, owner's call):
// an artifact lives beside the reply that made it, or in a tab of its own.
import { useEffect, useState, useSyncExternalStore } from "react";
import { onEvent, request, type Json } from "@/api";
import { HOME } from "@/router";
import { toast } from "@/ui/sonner";

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

/**
 * One element of one version, written into the message so the agent knows
 * exactly what to change: its selector, the `data-neko-id` the daemon stamped
 * (`neko_core::html_ids`), its opening tag to find it in the file, and its text.
 */
export function referenceText(id: string, n: number, pick: { selector: string; text: string; id?: string; hint?: string }): string {
  // What the page posted is the page's to choose: nothing in it may close the
  // bracket and write text of its own into the message.
  const flat = (s: string, max: number) => s.replace(/[[\]"]/g, "").replace(/\s+/g, " ").trim().slice(0, max);
  const element = pick.id && /^[\w-]{1,64}$/.test(pick.id) ? pick.id : "";
  const facts = [element ? `data-neko-id ${element}` : "", pick.hint ? flat(pick.hint.replace(/"/g, "'"), 200) : ""].filter(Boolean);
  return `[artifact ${id} v${n}, element ${flat(pick.selector, 300)}${facts.length ? ` (${facts.join(", ")})` : ""}: "${flat(pick.text.replace(/"/g, "'"), 120)}"]`;
}

/** Ends a message that names canvas elements (after Open Design's "Hard scope"), once however many it names. */
export const SCOPE_NOTE =
  "Hard scope: change only the element(s) named above, in the next version of that artifact. Leave other elements, the layout, global CSS and design tokens as they are unless this message asks otherwise; mention anything else you notice in your reply instead.";

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
  // The first render after `id` changes still holds the previous record (the
  // effect resets it after): never hand that out as this one, or the panel
  // briefly draws the old artifact at the new one's version.
  return { artifact: artifact && artifact.id === id ? artifact : null, error };
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

/** Calls `l` whenever the canvas target changes. */
export function subscribeCanvas(l: () => void): () => void {
  listeners.add(l);
  return () => { listeners.delete(l); };
}

export const getCanvas = () => target;

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

/** Saves `content` as the next version of a text artifact, in the kind of `edited` (the version the viewer showed); resolves to the new version. */
export async function editArtifact(id: string, edited: number, content: string, note = ""): Promise<number> {
  const reply = await request({ EditArtifact: { artifact_id: id, version: edited, content, note } });
  const saved = reply?.ArtifactSaved?.version;
  if (typeof saved !== "number") throw new Error(reply?.Error?.message ?? "The new version was not saved.");
  return saved;
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
  const stopFocus = trackFocus();
  const stop = onEvent((event) => {
    if (isShow(event)) {
      const { id, version } = event.ShowArtifact;
      if (typeof id !== "string" || !/^[A-Za-z0-9_-]{1,64}$/.test(id)) return;
      const n = typeof version === "number" ? version : null;
      if (isActivePage()) showArtifact(id, n);
      else toast("Neko showed an artifact in another window", { action: { label: "Show here", onClick: () => showArtifact(id, n) } });
    } else if (isOpenTab(event)) {
      const hash = tabHash(String(event.OpenTab.route ?? ""));
      if (!hash) return;
      if (isActivePage()) location.hash = hash.slice(1);
      else toast("Neko opened a page in another window", { action: { label: "Open here", onClick: () => { location.hash = hash.slice(1); } } });
    }
  });
  return () => { stop(); stopFocus(); };
}

// ---------------------------------------------------------------------------
// Which open page an agent's request moves
// ---------------------------------------------------------------------------

// Only one page acts on ShowArtifact / OpenTab: the one the person used last
// (focused now, or most recently, when they are in another app watching the
// agent work). Pages share the origin's localStorage, so each records itself
// there when it gains focus or becomes visible; the others offer a toast.
const PAGE_ID = Math.random().toString(36).slice(2);
const LAST_FOCUS = "neko.lastFocusedPage";

function claimFocus() {
  try { localStorage.setItem(LAST_FOCUS, PAGE_ID); } catch { /* private mode: every page acts */ }
}

function trackFocus(): () => void {
  if (document.hasFocus() || !localStorage.getItem(LAST_FOCUS)) claimFocus();
  const onVisible = () => { if (document.visibilityState === "visible") claimFocus(); };
  // A page that closes hands the claim back, so the next request is not lost on a dead page.
  const onHide = () => { try { if (localStorage.getItem(LAST_FOCUS) === PAGE_ID) localStorage.removeItem(LAST_FOCUS); } catch { /* ignore */ } };
  addEventListener("focus", claimFocus);
  addEventListener("pagehide", onHide);
  document.addEventListener("visibilitychange", onVisible);
  return () => {
    removeEventListener("focus", claimFocus);
    removeEventListener("pagehide", onHide);
    document.removeEventListener("visibilitychange", onVisible);
  };
}

/** Whether this page is the one an agent's request should move. */
export function isActivePage(): boolean {
  if (document.hasFocus()) return true;
  try {
    const last = localStorage.getItem(LAST_FOCUS);
    return !last || last === PAGE_ID;
  } catch {
    return true;
  }
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

// ---------------------------------------------------------------------------
// What drawing an artifact did, for the agent that made it
// ---------------------------------------------------------------------------

/** Where a version is drawn: beside Home's chat, in its own tab, or inside a reply. */
export type Place = "canvas" | "tab" | "reply";

export interface OnScreen {
  id: string;
  version: number;
  kind: string;
  viewer: string;
  place: Place;
}

const onScreen = new Map<symbol, OnScreen>();
const screenListeners = new Set<() => void>();

/** Records a version as drawn until the returned function is called (InspectApp lists them). */
export function trackOnScreen(entry: OnScreen): () => void {
  const key = Symbol(entry.id);
  onScreen.set(key, entry);
  screenListeners.forEach((l) => l());
  return () => {
    onScreen.delete(key);
    screenListeners.forEach((l) => l());
  };
}

export const listOnScreen = (): OnScreen[] => [...onScreen.values()];
export function subscribeOnScreen(l: () => void): () => void {
  screenListeners.add(l);
  return () => { screenListeners.delete(l); };
}

const shownAt = new Map<string, number>();
const problemsSent = new Map<string, number>();
const PROBLEMS_PER_VERSION = 50;

/** Tells the daemon a page drew this version (`ReportArtifactShown`), at most every 5 s per viewer. */
export function reportShown(id: string, version: number, viewer: string) {
  if (!id || !(version > 0)) return;
  const key = `${id}:${version}:${viewer}`;
  const now = Date.now();
  if (now - (shownAt.get(key) ?? 0) < 5000) return;
  shownAt.set(key, now);
  request({ ReportArtifactShown: { id, version, viewer } }).catch(() => {});
}

export type ProblemKind = "error" | "rejection" | "console" | "viewer";

/** Tells the daemon what went wrong drawing this version (`ReportArtifactProblem`); at most 50 per version from one page. */
export function reportProblem(id: string, version: number, kind: ProblemKind, message: string, viewer?: string) {
  if (!id || !(version > 0) || !message.trim()) return;
  const key = `${id}:${version}`;
  const sent = problemsSent.get(key) ?? 0;
  if (sent >= PROBLEMS_PER_VERSION) return;
  problemsSent.set(key, sent + 1);
  request({ ReportArtifactProblem: { id, version, kind, message: message.slice(0, 500), viewer: viewer ?? null } }).catch(() => {});
}

// ---------------------------------------------------------------------------
// Artifacts that talk back: data-neko-send
// ---------------------------------------------------------------------------

/** The longest text one click may offer. */
export const MAX_SEND = 500;

/** The line a message from an artifact ends with: the agent reads it; the chat shows it as "from <title>". */
export const fromArtifactLine = (id: string, n: number, title: string) =>
  `[from artifact ${id} v${n}: "${title.replace(/["\n\r\]]/g, "'").slice(0, 80)}"]`;
const FROM = /\n?\[from artifact ([A-Za-z0-9_-]{1,64}) v(\d+): "([^"\n]*)"\]\s*$/;

/** A message's own text and, when it carries an artifact's draft, which artifact. */
export function splitFromArtifact(text: string): { body: string; from: { id: string; version: number; title: string } | null } {
  const m = FROM.exec(text);
  if (!m) return { body: text, from: null };
  return { body: text.slice(0, m.index).trimEnd(), from: { id: m[1], version: Number(m[2]), title: m[3] } };
}

/**
 * What a user message shows (the chat and the queue tray): a message that
 * carries an artifact's `data-neko-send` draft says which artifact in a small
 * label, and a canvas comment's scope line (Neko's words to the agent, not
 * the person's) becomes a label too. The agent reads the message as stored.
 */
export function userMessageView(text: string): { body: string; label: string | null; from: string | null } {
  const { body, from } = splitFromArtifact(text);
  const scoped = body.endsWith(SCOPE_NOTE);
  const shown = scoped ? body.slice(0, -SCOPE_NOTE.length).trimEnd() : body;
  const label = from ? `From “${from.title}”` : scoped ? "Only the picked element" : null;
  return { body: shown, label, from: from?.id ?? null };
}

/** An offer from an artifact, waiting for Home's composer to take it. */
export interface Offer {
  text: string;
  /** What the chip says. */
  name: string;
  /** The line the message ends with (`fromArtifactLine`). */
  reference: string;
}

const offers: Offer[] = [];
export const OFFER_EVENT = "neko:artifact-offer";

/** The offers waiting for the composer, oldest first, once. */
export function takeOffers(): Offer[] {
  return offers.splice(0, offers.length);
}

/**
 * What a `data-neko-send` element says becomes a draft in Home's composer,
 * marked as from the artifact (a chip naming it, its line ending the
 * message), and the composer is focused. **An artifact never sends a message
 * by itself:** the person reads the draft and presses send, or not. The
 * draft goes to Home's chat because only that chat can save artifacts.
 * The caller has checked the click (frame focused, person active, rate).
 */
export function offerFromArtifact(artifact: Artifact, n: number, text: string): boolean {
  const said = text.trim();
  if (!said) return false;
  if (said.length > MAX_SEND) {
    toast(`“${artifact.title}” offered ${said.length} characters; ${MAX_SEND} at most. Nothing was added.`);
    return false;
  }
  if (offers.length >= 5) return false;
  offers.push({ text: said, name: `From “${artifact.title}”`, reference: fromArtifactLine(artifact.id, n, artifact.title) });
  if (currentRoute() !== HOME) location.hash = "/";
  dispatchEvent(new CustomEvent(OFFER_EVENT));
  return true;
}
