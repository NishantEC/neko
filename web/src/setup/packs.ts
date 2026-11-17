// Packs: the wire shapes (crates/neko-protocol/src/packs.rs) and the small pure
// helpers the Packs page and its dialogs share. A pack bundles tools, skills,
// a web plugin and routines; the daemon does the installing, this only asks.
import { request } from "@/api";
import type { StatusKind } from "@/ui/status-glyph";

export type PackStatus = "Healthy" | "NeedsSetup" | "Failing" | "Changed";

export interface PackRequirement { name: string; hint: string; found: boolean; path?: string | null }
export interface PackContents { connections: string[]; skills: string[]; plugin: string | null; routines: string[]; luau?: string[] }
export interface PluginPower { title: string; pages: string[]; viewers: string[]; fences: string[]; prompt: string; hash: string }
export interface RoutineText { name: string; instruction: string; cadence: string }
export interface PackInfo {
  name: string; title: string; version: string; description: string; homepage: string; licence: string;
  permissions: string[]; requirements: PackRequirement[]; env: string[]; contents: PackContents; health_command: string[]; runs?: string[];
  warnings?: string[]; plugin?: PluginPower | null; routine_texts?: RoutineText[]; skills_hash?: string; luau_hash?: string;
}
export interface PackOrphan { target: string; reason: string }
export interface PackHealth { ok: boolean; at_ms: number; output: string }
export interface InstalledPack {
  info: PackInfo; source: string; status: PackStatus; installed_ms: number; approved_ms: number; health: PackHealth | null;
  connection_ids: string[]; skill_paths: string[]; plugin_dir: string | null; plugin_disabled?: boolean; routine_ids: string[];
  update_available?: string | null; orphans?: PackOrphan[]; approved_digest?: string;
}
export interface CatalogEntry { info: PackInfo; source: string; installed: boolean }
export interface PackListing { installed: InstalledPack[]; catalog: CatalogEntry[] }
export interface PackPreview {
  info: PackInfo; source: string; installed_version: string | null;
  /** Sent back, exactly as shown, with Install, Update or Approve. */
  digest: string; changes?: string[]; needs_approval?: boolean;
}

/** The status glyph (DESIGN.md §3): mint check, amber half circle, coral cross. */
export const packGlyph = (status: PackStatus): StatusKind => (status === "Healthy" ? "done" : status === "Failing" ? "failed" : "inProgress");

export const PACK_STATUS_WORDS: Record<PackStatus, string> = { Healthy: "Healthy", NeedsSetup: "Needs setup", Failing: "Failing", Changed: "Changed on disk" };

/** `catalog:<name>` reads as "catalog"; a path or URL keeps its end, which is the part that names it. */
export function shortSource(source: string, max = 48): string {
  if (source.startsWith("catalog:")) return "catalog";
  if (source.length <= max) return source;
  return `…${source.slice(source.length - (max - 1))}`;
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
const listing = (reply: any): PackListing => {
  const l = reply?.Packs;
  return { installed: Array.isArray(l?.installed) ? l.installed : [], catalog: Array.isArray(l?.catalog) ? l.catalog : [] };
};

export const listPacks = async (signal?: AbortSignal) => listing(await request({ Packs: "List" }, signal));
export const previewPack = async (source: string): Promise<PackPreview> => {
  const reply = await request({ Packs: { Preview: { source } } });
  if (!reply?.PackPreview) throw new Error("The daemon did not answer with a preview.");
  return reply.PackPreview;
};
/** `digest` is the one the preview returned. `env` carries secrets: it is sent once and never kept or logged. */
export const installPack = async (source: string, digest: string, env: Record<string, string>) => {
  const fields: Record<string, unknown> = { source, digest };
  if (Object.keys(env).length > 0) fields.env = env;
  return listing(await request({ Packs: { Install: fields } }));
};
export const removePack = async (name: string) => listing(await request({ Packs: { Remove: { name } } }));
export const updatePack = async (name: string, digest: string) => listing(await request({ Packs: { Update: { name, digest } } }));
export const approvePack = async (name: string, digest: string) => listing(await request({ Packs: { Approve: { name, digest } } }));
export const cleanUpPack = async (name: string) => listing(await request({ Packs: { CleanUp: { name } } }));
/** A plugin's id is the last part of its folder's path. */
export const pluginIdOf = (dir: string | null | undefined) => (dir ?? "").replace(/\/+$/, "").split("/").pop() ?? "";
export const checkPackHealth = async (name: string) => listing(await request({ Packs: { Health: { name } } }));
