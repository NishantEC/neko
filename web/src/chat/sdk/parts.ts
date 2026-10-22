// Reading a message's parts for the screen: what was work (reasoning, tools,
// the prose between steps) and what is the reply, and a one-line account of
// each tool in the words of the work log.
import { shortCommand, type Receipt } from "@/work/model";
import { ACTION_TOOL, isToolPart, type NekoPart, type NekoUIMessage, type ToolPart } from "@/chat/sdk/types";
import type { Json } from "@/api";

export type Kind = "command" | "edit" | "read" | "search" | "tool" | "other";

/**
 * Work is everything up to and including the last tool or reasoning part (with
 * the prose said between steps); the reply is what follows. While a reply is
 * still working, its prose after the last tool is the reply so far.
 */
export function splitTurn(parts: NekoPart[]): { work: NekoPart[]; reply: NekoPart[] } {
  let last = -1;
  parts.forEach((part, index) => {
    if (isToolPart(part) && part.toolName !== ACTION_TOOL) last = index;
    else if (part.type === "reasoning") last = index;
  });
  const keep = (part: NekoPart) => part.type !== "step-start";
  return { work: parts.slice(0, last + 1).filter(keep), reply: parts.slice(last + 1).filter(keep) };
}

/** The reply as text (what Copy copies). */
export function replyText(parts: NekoPart[]): string {
  return parts.filter((p): p is Extract<NekoPart, { type: "text" }> => p.type === "text").map((p) => p.text).join("\n\n").trim();
}

export function messageText(message: NekoUIMessage): string {
  return replyText(message.parts);
}

const input = (part: ToolPart): Json => (part.input && typeof part.input === "object" ? part.input : {});
const output = (part: ToolPart): Json => ("output" in part && part.output && typeof part.output === "object" ? (part.output as Json) : {});

const base = (path: unknown) => (typeof path === "string" && path ? path.split("/").filter(Boolean).pop() ?? path : "");

export function toolKind(part: ToolPart): Kind {
  const name = part.toolName.toLowerCase();
  if (name === "bash" || name === "shell") return "command";
  if (["edit", "multiedit", "write", "notebookedit", "apply_patch", "patch"].includes(name)) return "edit";
  if (["read", "ls", "notebookread"].includes(name)) return "read";
  if (["grep", "glob", "websearch", "webfetch", "search"].includes(name)) return "search";
  return "tool";
}

/** "Ran `git status`", "Edited neko_chat.rs", "Read neko_chat.rs", or the tool's own name. */
export function toolText(part: ToolPart): string {
  const args = input(part);
  switch (toolKind(part)) {
    case "command": return `Ran \`${shortCommand(String(args.command ?? "")) || "a command"}\``;
    case "edit": return `Edited ${base(args.file_path ?? args.path ?? args.notebook_path) || (Array.isArray(args.changes) ? `${args.changes.length} files` : "files")}`;
    case "read": return `Read ${base(args.file_path ?? args.path) || "a file"}`;
    case "search": return `Searched ${typeof (args.pattern ?? args.query ?? args.url) === "string" ? `“${String(args.pattern ?? args.query ?? args.url).slice(0, 60)}”` : "the code"}`;
    default: return part.title && part.title !== part.toolName ? part.title : part.toolName;
  }
}

/** A command that ran and did not succeed, or a tool that errored. Declining is not failing. */
export function toolFailed(part: ToolPart): boolean {
  if (part.state === "output-error") return true;
  const code = output(part).exit_code;
  return part.state === "output-available" && typeof code === "number" && code !== 0;
}

export const toolWaiting = (part: ToolPart) => part.state === "approval-requested";
export const toolDeclined = (part: ToolPart) => part.state === "output-denied" || (part.state === "approval-responded" && part.approval?.approved === false);
export const toolRunning = (part: ToolPart) => part.state === "input-streaming" || part.state === "input-available" || (part.state === "approval-responded" && part.approval?.approved !== false);

/** What to show when a tool line is opened: the command and its output, or the arguments. */
export function toolDetail(part: ToolPart): string | null {
  const args = input(part);
  const out = output(part);
  const lines: string[] = [];
  if (toolKind(part) === "command") {
    if (args.command) lines.push(`$ ${String(args.command)}`);
    if (typeof out.output === "string") lines.push(out.output.trim() || "No output.");
    if (typeof out.exit_code === "number" && out.exit_code !== 0) lines.push(`exit ${out.exit_code}`);
  } else {
    if (Object.keys(args).length) lines.push(JSON.stringify(args, null, 2));
    if (typeof out.output === "string" && out.output.trim()) lines.push(out.output.trim());
    if ("errorText" in part && part.errorText) lines.push(String(part.errorText));
  }
  const text = lines.join("\n\n").trim();
  return text || null;
}

/** The connection a bridge tool belongs to (`toolMetadata.connectionId`), if it is one. */
export function connectionOf(part: ToolPart): string | null {
  const meta = (part as { toolMetadata?: Json }).toolMetadata;
  return typeof meta?.connectionId === "string" ? meta.connectionId : null;
}

/** A command receipt from a task's conversation, in the shape the receipt rows read. */
export function receiptOfPart(part: ToolPart): Receipt {
  const out = output(part);
  const command = typeof out.command === "string" ? out.command : null;
  const exitCode = typeof out.exit_code === "number" ? out.exit_code : null;
  const failed = out.failed === true;
  const incomplete = out.incomplete === true;
  const notices: string[] = [];
  if (incomplete) notices.push("This saved record is incomplete. It does not provide full command evidence.");
  if (out.command_truncated === true) notices.push("The command was shortened before it was saved.");
  if (out.output_truncated === true) notices.push("Output was shortened before it was saved. Only the retained output is available here.");
  return {
    command,
    output: typeof out.output === "string" ? out.output : null,
    exitCode,
    commandTruncated: out.command_truncated === true,
    outputTruncated: out.output_truncated === true,
    incomplete,
    failed,
    title: command ? shortCommand(command) : "Command details unavailable",
    status: incomplete ? (failed ? `Failed · exit ${exitCode} · incomplete record` : "Incomplete record") : failed ? `Failed · exit ${exitCode}` : "Completed · exit 0",
    notices,
  };
}
