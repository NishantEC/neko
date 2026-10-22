// Drafts the composer keeps per scope while you navigate (SharedComposer.swift's
// ComposerDraftStore): text, attachments and the Ask/Plan mode survive going to
// another page and back, not a reload. A send clears only the revision it sent,
// so a message typed while the send was in flight is not wiped by its answer.
import { useSyncExternalStore } from "react";

export interface DraftAttachment {
  id: string;
  name: string;
  type: string;
  isImage: boolean;
  /** A data: URL for images (the daemon's CSP allows `data:` but not `blob:`). */
  previewUrl?: string;
  /** The markdown reference that goes into the message, once the daemon has kept the file. */
  reference?: string;
  status: "uploading" | "ready" | "error";
  error?: string;
}

export type ComposerMode = "ask" | "plan";

export interface Draft {
  text: string;
  attachments: DraftAttachment[];
  mode: ComposerMode;
  revision: number;
}

const EMPTY: Draft = { text: "", attachments: [], mode: "ask", revision: 0 };
const drafts = new Map<string, Draft>();
const listeners = new Set<() => void>();
const emit = () => listeners.forEach((l) => l());

export const getDraft = (scope: string): Draft => drafts.get(scope) ?? EMPTY;

function update(scope: string, change: (d: Draft) => Partial<Draft>) {
  const current = getDraft(scope);
  const next = { ...current, ...change(current) };
  drafts.set(scope, next);
  emit();
}

export const setDraftText = (scope: string, text: string) => {
  if (getDraft(scope).text === text) return;
  update(scope, (d) => ({ text, revision: d.revision + 1 }));
};
export const setDraftMode = (scope: string, mode: ComposerMode) => update(scope, () => ({ mode }));
export const addDraftAttachment = (scope: string, attachment: DraftAttachment) =>
  update(scope, (d) => ({ attachments: [...d.attachments, attachment], revision: d.revision + 1 }));
export const patchDraftAttachment = (scope: string, id: string, patch: Partial<DraftAttachment>) =>
  update(scope, (d) => ({ attachments: d.attachments.map((a) => (a.id === id ? { ...a, ...patch } : a)) }));
export const removeDraftAttachment = (scope: string, id: string) =>
  update(scope, (d) => ({ attachments: d.attachments.filter((a) => a.id !== id), revision: d.revision + 1 }));

export interface Submission {
  scope: string;
  /** The text as typed plus every ready attachment's reference, one per line. */
  text: string;
  revision: number;
}

export function submission(scope: string): Submission {
  const d = getDraft(scope);
  const parts = [d.text.trim(), ...d.attachments.filter((a) => a.status === "ready" && a.reference).map((a) => a.reference as string)];
  return { scope, text: parts.filter(Boolean).join("\n"), revision: d.revision };
}

/** Clears the draft when the send worked and nothing was typed since. */
export function completeSubmission(s: Submission, succeeded: boolean) {
  if (!succeeded || getDraft(s.scope).revision !== s.revision) return;
  update(s.scope, (d) => ({ text: "", attachments: [], revision: d.revision + 1 }));
}

export function useDraft(scope: string): Draft {
  return useSyncExternalStore(
    (listener) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    () => getDraft(scope),
  );
}

// ---------------------------------------------------------------------------
// Files: kept by the daemon (POST /api/attachments), referenced in the message
// ---------------------------------------------------------------------------

export const MAX_ATTACHMENT_BYTES = 32 * 1024 * 1024;

function readDataUrl(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(String(reader.result));
    reader.onerror = () => reject(reader.error ?? new Error("The image could not be read."));
    reader.readAsDataURL(file);
  });
}

/**
 * Adds a file to a scope's draft and uploads it. The chip appears at once
 * (uploading), then becomes ready with the reference the daemon answered with,
 * or shows why it failed. Returns an error to show inline when the file is
 * refused before any upload.
 */
export async function attachFile(scope: string, file: File): Promise<string | null> {
  if (file.size === 0) return `${file.name || "That file"} is empty.`;
  if (file.size > MAX_ATTACHMENT_BYTES) return "Choose a file under 32 MB.";
  const id = crypto.randomUUID();
  const isImage = file.type.startsWith("image/");
  const name = file.name || (isImage ? "Pasted image" : "File");
  addDraftAttachment(scope, { id, name, type: file.type, isImage, status: "uploading" });
  try {
    if (isImage) {
      const previewUrl = await readDataUrl(file).catch(() => undefined);
      if (previewUrl) patchDraftAttachment(scope, id, { previewUrl });
    }
    const response = await fetch("/api/attachments", {
      method: "POST",
      headers: { "content-type": "application/octet-stream", "x-neko-filename": encodeURIComponent(name) },
      body: file,
    });
    if (!response.ok) throw new Error(`The daemon refused the file (${response.status}).`);
    const kept = await response.json();
    patchDraftAttachment(scope, id, { status: "ready", reference: kept.reference, isImage: !!kept.is_image, name: kept.name ?? name });
    return null;
  } catch (e) {
    const message = e instanceof Error ? e.message : String(e);
    patchDraftAttachment(scope, id, { status: "error", error: message });
    return message;
  }
}
