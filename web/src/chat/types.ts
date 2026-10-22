// Local stand-ins for the few types hushbacks takes from the Vercel `ai` package.
export type Role = "user" | "assistant";

export interface FileUIPart {
  type: "file";
  mediaType: string;
  filename?: string;
  url: string;
}

export interface SourceDocumentUIPart {
  type: "source-document";
  sourceId: string;
  mediaType: string;
  title: string;
  filename?: string;
}

// ---------------------------------------------------------------------------
// Neko's own chat data (the daemon's `ChatMessage`, neko-protocol workbench).
// ---------------------------------------------------------------------------

export type ToolStatus = "awaiting_approval" | "approved" | "running" | "succeeded" | "failed" | "denied";

export interface ToolCall {
  id: string;
  connection_id: string;
  tool_name: string;
  arguments_json: string;
  status: ToolStatus;
}

export interface ChatAction {
  id: string;
  kind: string;
  task_id: string;
  text: string;
  status: "Proposed" | "Accepted" | "Declined" | "Failed" | string;
}

export interface NekoMessage {
  id: string;
  at_ms: number;
  role: "user" | "assistant" | string;
  text: string;
  pending?: boolean;
  queued?: boolean;
  failed?: boolean;
  remembered?: string[];
  actions?: ChatAction[];
  /** What Neko did while working, newest last: "Ran `git status`". */
  activity?: string[];
  /** When Neko started on the reply; `at_ms` is the finish time once done. */
  started_ms?: number;
  tool_calls?: ToolCall[];
  workspace_id?: string | null;
}
