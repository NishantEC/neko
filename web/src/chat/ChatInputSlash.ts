// Composer shortcuts the app handles itself, never sent to a model as typed.
// A port of native/NekoKit/Sources/NekoNative/SlashCommands.swift (the list,
// the wording, the parsing) and of ComposerPayload (Plan wrapper, size limit).

export interface SlashEntry {
  name: string;
  detail: string;
  shortcut?: string;
}

/** `/plan` first, then the same list SlashCommands.swift offers. */
export const PLAN_ENTRY: SlashEntry = { name: "/plan", detail: "Plan first, start after you approve" };

export const SLASH_ENTRIES: SlashEntry[] = [
  { name: "/stop", detail: "Stop all work (⌘⇧Esc)", shortcut: "⌘⇧Esc" },
  { name: "/remember", detail: "Remember a fact, e.g. /remember tests use pnpm" },
  { name: "/forget", detail: "Forget a memory, e.g. /forget pnpm" },
  { name: "/recall", detail: "Show what Neko remembers" },
  { name: "/memory", detail: "Open the Memory page" },
  { name: "/tickets", detail: "Open All tasks" },
  { name: "/clear", detail: "Clear finished tasks" },
  { name: "/models", detail: "Choose and check the agent model" },
  { name: "/permissions", detail: "What Neko is allowed to do on this Mac" },
  { name: "/setup", detail: "Show the first-run tour again" },
  { name: "/help", detail: "List these commands" },
];

/** Commands whose argument is typed after the name: choosing one only fills it in. */
export const TAKES_ARGUMENT = ["/remember", "/forget"];

/** Commands matching a partial "/re…" draft; empty once a space is typed. */
export function slashSuggestions(draft: string): SlashEntry[] {
  const text = draft.trim().toLowerCase();
  if (!text.startsWith("/") || text.includes(" ")) return [];
  return [PLAN_ENTRY, ...SLASH_ENTRIES].filter((entry) => entry.name.startsWith(text));
}

export type SlashCommand =
  | { kind: "stop" | "clear" | "memory" | "recall" | "models" | "setup" | "permissions" | "tickets" | "help" | "plan" }
  | { kind: "remember" | "forget"; argument: string };

/** null for ordinary messages, including ones that merely contain a slash. */
export function parseSlash(draft: string): SlashCommand | null {
  const text = draft.trim();
  if (!text.startsWith("/")) return null;
  const name = (text.match(/^\S*/)?.[0] ?? "").toLowerCase();
  const argument = text.slice(name.length).trim();
  switch (name) {
    case "/stop": return { kind: "stop" };
    case "/clear": return { kind: "clear" };
    case "/memory": return { kind: "memory" };
    case "/remember": return argument ? { kind: "remember", argument } : null;
    case "/forget": return argument ? { kind: "forget", argument } : null;
    case "/recall": return { kind: "recall" };
    case "/models":
    case "/model": return { kind: "models" };
    case "/setup": return { kind: "setup" };
    case "/permissions": return { kind: "permissions" };
    case "/tickets":
    case "/steps": return { kind: "tickets" };
    case "/plan": return { kind: "plan" };
    case "/help":
    case "/": return { kind: "help" };
    default: return null;
  }
}

/** Commands that become a normal chat message (the daemon answers these in code, without a model). */
export function slashChatText(command: SlashCommand): string | null {
  switch (command.kind) {
    case "remember": return `Remember that ${command.argument}`;
    case "forget": return `Forget ${command.argument}`;
    case "recall": return "What do you remember?";
    default: return null;
  }
}

export const PLAN_PREFIX =
  "Plan only, and change nothing yet: propose a task with a short step-by-step plan for my approval.\n\n";

/** The text that goes to the daemon (ComposerPayload.homeText). */
export function homeText(raw: string, planOnly: boolean): string {
  const command = parseSlash(raw);
  const text = (command && slashChatText(command)) ?? raw;
  return planOnly && !command ? PLAN_PREFIX + text : text;
}

/** The daemon's limit is in UTF-8 bytes, attachment references included. */
export const MESSAGE_LIMIT_BYTES = 4096;
export const byteLength = (text: string) => new TextEncoder().encode(text).length;

export function validationError(text: string, limit = MESSAGE_LIMIT_BYTES): string | null {
  const bytes = byteLength(text);
  return bytes > limit
    ? `Message is ${bytes} bytes; the limit is ${limit}, including attachment references and context. Shorten it before sending.`
    : null;
}

/** Swaps the trailing "@word" for `replacement` (ComposerMention.replacingTrailingMention). */
export function replaceTrailingMention(text: string, replacement: string): string {
  const words = text.split(" ");
  if (words[words.length - 1]?.startsWith("@")) words[words.length - 1] = replacement.trim();
  const joined = words.join(" ").trim();
  return joined ? `${joined} ` : "";
}

/** The "@word" being typed at the end of the draft, if any (without the "@"). */
export function mentionQuery(draft: string): string | null {
  if (draft.endsWith(" ")) return null;
  const last = draft.split(" ").pop() ?? "";
  return last.startsWith("@") ? last.slice(1) : null;
}
