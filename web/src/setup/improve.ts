// Improvements: the wire shapes (crates/neko-protocol/src/improve.rs) and the small pure
// helpers the Improvements page and the Settings switch share. Neko notices the same
// trouble again and suggests a fix; the daemon does the applying and undoing, this only
// asks. Every request is the daemon's `Improve` command and answers with the whole
// listing, which replaces what is shown.
import { request } from "@/api";
import type { StatusKind } from "@/ui/status-glyph";

export type ImproveKind = "install_pack" | "skill_patch" | "new_skill" | "standing_order" | "code_task";
export type ImproveState = "open" | "sent" | "applied" | "helped" | "didnt_help" | "rejected";

export interface ImproveEvent { at_ms: number; text: string }
export interface Improvement {
  id: string; signal_key: string; kind: ImproveKind; title: string; why: string; change: string; default_label: string;
  target: string; workspace_id: string; evidence: string; diff: string; state: ImproveState;
  created_ms: number; applied_ms: number | null; resolved_ms: number | null; before: number; after: number;
  inbox_seq: number | null; undo_seq: number | null; history: ImproveEvent[];
  can_undo: boolean; undo_confirms: boolean; task_id: string | null;
}
export interface ImproveReply {
  enabled: boolean; dev_checkout: boolean;
  /** Newest first. */
  proposals: Improvement[];
  /** After applying a pack suggestion: the source (`catalog:<name>`) whose preview opens. */
  open_pack: string | null;
  /** One line about what the last command did. */
  note: string | null;
}

/** The words beside a title (`ImproveKind::label`). */
export const IMPROVE_KIND_LABELS: Record<ImproveKind, string> = {
  install_pack: "Install a pack", skill_patch: "Improve a skill", new_skill: "New skill", standing_order: "Standing order", code_task: "Change Neko",
};

/** The status glyph (DESIGN.md §3): amber half circle for waiting, mint check, coral cross, faint cross for rejected. */
export const improveGlyph = (state: ImproveState): StatusKind => {
  switch (state) {
    case "open":
    case "sent": return "inProgress";
    case "applied":
    case "helped": return "done";
    case "didnt_help": return "failed";
    default: return "cancelled";
  }
};

/** The glyph's name for assistive tech. */
export const IMPROVE_STATE_WORDS: Record<ImproveState, string> = {
  open: "Open", sent: "Waiting in Skills", applied: "Applied", helped: "Helped", didnt_help: "Didn’t help", rejected: "Rejected",
};

/** The muted word shown after the title; an open suggestion needs none. */
export const improveStateWord = (state: ImproveState): string | null => (state === "open" ? null : IMPROVE_STATE_WORDS[state]);

const times = (n: number) => (n === 1 ? "1 time" : `${n} times`);
/** "Fired 3 times in the 7 days before, 0 since." Only once applied. */
export const improveCounts = (p: Pick<Improvement, "before" | "after" | "applied_ms">): string | null =>
  p.applied_ms ? `Fired ${times(p.before)} in the 7 days before, ${p.after} since.` : null;

/** What the confirmation says before an undo that is not trivially reversible. */
export const undoWords = (kind: ImproveKind): string =>
  kind === "install_pack" ? "The pack is removed, with its connections, skills, plugin and routines. A copy is kept in the cleanup archive."
    : kind === "code_task" ? "The task’s changes are rolled back from its checkpoints, or, once pushed, a task is created to revert them."
      : "The change is reversed.";

// eslint-disable-next-line @typescript-eslint/no-explicit-any
const reply = (r: any): ImproveReply => {
  const l = r?.Improve;
  return {
    enabled: !!l?.enabled, dev_checkout: !!l?.dev_checkout, proposals: Array.isArray(l?.proposals) ? l.proposals : [],
    open_pack: typeof l?.open_pack === "string" ? l.open_pack : null, note: typeof l?.note === "string" ? l.note : null,
  };
};

export const listImprovements = async (signal?: AbortSignal) => reply(await request({ Improve: "List" }, signal));
/** A pack suggestion answers with `open_pack`; nothing is installed. */
export const applyImprovement = async (id: string) => reply(await request({ Improve: { Apply: { id } } }));
export const rejectImprovement = async (id: string) => reply(await request({ Improve: { Reject: { id } } }));
export const undoImprovement = async (id: string) => reply(await request({ Improve: { Undo: { id } } }));
export const setImproveEnabled = async (enabled: boolean) => reply(await request({ Improve: { SetEnabled: { enabled } } }));
