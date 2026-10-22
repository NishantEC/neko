// Neko's chat in the AI SDK's types (docs/design/chat-on-ai-sdk.md).
//
// A message is the SDK's `UIMessage`. The daemon owns the lifecycle of a turn
// (queued, pending, done, failed), so it travels as `metadata`; the structured
// views a reply can carry are typed `data-neko-*` parts.
import type { DynamicToolUIPart, UIMessage } from "ai";

export interface NekoMeta {
  atMs?: number;
  /** When Neko started on the reply; `atMs` is the finish time once it is done. */
  startedMs?: number;
  /** The daemon's lifecycle for the turn. Task conversations are always "done". */
  state?: "queued" | "pending" | "done" | "failed";
  workspaceId?: string;
  /** Tasks the reply opened. */
  tickets?: string[];
  /** Task conversations: who spoke (`Planner`, `Builder`, ...) and their role. */
  author?: string;
  authorRole?: string;
  kind?: "brief" | "note";
}

export type NekoData = {
  "neko-choices": { question?: string; options?: { label?: string; detail?: string }[] };
  "neko-form": { title?: string; submit?: string; fields?: { label?: string; kind?: string; value?: unknown; options?: string[]; placeholder?: string }[] };
  "neko-plan": { title?: string; estimate?: string; steps?: (string | { title?: string })[] };
  "neko-chart": { title?: string; source?: string; unit?: string; bars?: { label?: string; value?: number; highlight?: boolean }[] };
  "neko-files": { files?: (string | { path?: string; note?: string })[] };
  "neko-sources": { sources?: string[] };
  "neko-memory": { items?: string[] };
  "neko-status": { text?: string };
};

export type NekoUIMessage = UIMessage<NekoMeta, NekoData>;
export type NekoPart = NekoUIMessage["parts"][number];
export type ToolPart = DynamicToolUIPart;

/** The tool part the daemon uses for an action Neko proposes on a task. */
export const ACTION_TOOL = "neko_task_action";

export const isToolPart = (part: NekoPart): part is ToolPart => part.type === "dynamic-tool";
