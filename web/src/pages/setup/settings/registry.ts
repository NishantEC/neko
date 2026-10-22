// What Settings contains, as data: its sections and every individual setting
// that search can open. Ranking is the native app's (SettingsSearch.swift):
// title prefix, a word in the title, a keyword, then a substring.

export type SectionId = "general" | "ai" | "rulebook" | "search" | "permissions" | "diagnostics" | "updates" | "about";

export interface SectionDef { id: SectionId; title: string; summary: string; group: string; keywords: string[] }
export interface SettingDef { id: string; section: SectionId; title: string; keywords: string[] }

export const SECTIONS: SectionDef[] = [
  { id: "general", title: "General", group: "Neko", summary: "Make Neko fit the way you use your Mac.", keywords: ["preferences", "appearance", "shortcut", "hotkey", "login"] },
  { id: "ai", title: "AI & agents", group: "Neko", summary: "Choose the runtime and model for new work, and what a task may cost.", keywords: ["model", "runtime", "codex", "claude", "budget", "agents"] },
  { id: "rulebook", title: "Rulebook", group: "Neko", summary: "The rules Neko and every agent it hands work to follow.", keywords: ["rules", "standards", "engineering", "review", "instructions", "policy"] },
  { id: "search", title: "Quick panel", group: "This Mac", summary: "Choose which folders appear in the quick panel.", keywords: ["quick panel", "palette", "folders", "files", "spotlight"] },
  { id: "permissions", title: "Permissions", group: "This Mac", summary: "Review what Neko can access on this Mac.", keywords: ["privacy", "full disk access", "accessibility", "clipboard"] },
  { id: "diagnostics", title: "Diagnostics", group: "This Mac", summary: "Check local runtimes and the connection to Neko.", keywords: ["health", "debug", "logs", "troubleshoot", "connection"] },
  { id: "updates", title: "Neko updates", group: "This Mac", summary: "Neko applying its own landed changes: version, pending update, rollbacks.", keywords: ["update", "upgrade", "version", "self-update", "rollback", "reinstall", "auto-update"] },
  { id: "about", title: "About", group: "This Mac", summary: "Version, updates and where this page comes from.", keywords: ["version", "update", "build"] },
];

export const SETTINGS: SettingDef[] = [
  { id: "appearance", section: "general", title: "Appearance", keywords: ["dark", "light", "theme", "mode", "colour", "color"] },
  { id: "launch-at-login", section: "general", title: "Launch at login", keywords: ["startup", "start", "boot", "open at login"] },
  { id: "summon-shortcut", section: "general", title: "Summon shortcut", keywords: ["hotkey", "keyboard", "global"] },
  { id: "runtime", section: "ai", title: "Agent runtime", keywords: ["codex", "claude", "opencode", "provider", "runner"] },
  { id: "model", section: "ai", title: "Model", keywords: ["gpt", "sonnet", "opus", "llm", "check"] },
  { id: "effort", section: "ai", title: "Reasoning effort", keywords: ["thinking", "reasoning", "low", "high"] },
  { id: "speed", section: "ai", title: "Speed", keywords: ["fast", "tier", "priority", "service"] },
  { id: "custom-model", section: "ai", title: "Exact model ID", keywords: ["id", "custom", "manual"] },
  { id: "pins", section: "ai", title: "Home chat model pins", keywords: ["pin", "neko decides", "automatic", "paid speed", "model", "effort"] },
  { id: "budget", section: "ai", title: "Task budget", keywords: ["cost", "money", "limit", "spend", "dollars", "price"] },
  { id: "tickets-at-once", section: "ai", title: "Tasks at once", keywords: ["concurrency", "parallel", "one at a time", "queue", "limit", "workers"] },
  { id: "writable-per-repo", section: "ai", title: "Writing per repository", keywords: ["worktrees", "limit", "repository", "concurrency"] },
  { id: "plan-first", section: "ai", title: "Plan before working", keywords: ["approval", "read-only", "plan"] },
  { id: "start-over", section: "ai", title: "Start over", keywords: ["reset", "clear", "delete tasks", "delete tickets", "fresh", "wipe"] },
  { id: "rulebook", section: "rulebook", title: "Rulebook text", keywords: ["rules", "standards", "default", "reset"] },
  { id: "folders", section: "search", title: "Included folders", keywords: ["files", "quick panel", "scope", "directories"] },
  { id: "add-folder", section: "search", title: "Add a folder", keywords: ["folder", "directory", "path"] },
  { id: "clipboard-history", section: "permissions", title: "Clipboard history", keywords: ["copy", "paste", "pasteboard", "privacy"] },
  { id: "full-disk-access", section: "permissions", title: "Full Disk Access", keywords: ["privacy", "protected folders", "documents", "desktop"] },
  { id: "accessibility", section: "permissions", title: "Accessibility", keywords: ["paste", "selected text", "hotkey"] },
  { id: "diagnostics", section: "diagnostics", title: "Run diagnostics", keywords: ["health", "connection", "debug", "logs", "troubleshoot"] },
  { id: "pending-update", section: "updates", title: "Apply update", keywords: ["pending", "waiting", "safety", "approve"] },
  { id: "auto-update-ui", section: "updates", title: "Update the web UI automatically", keywords: ["auto-update", "live", "reload"] },
  { id: "auto-update-app", section: "updates", title: "Reinstall the app automatically", keywords: ["auto-update", "install", "native", "daemon"] },
  { id: "update-now", section: "updates", title: "Update Neko now", keywords: ["upgrade", "rebuild", "install"] },
  { id: "updates", section: "about", title: "Updates and version", keywords: ["version", "update", "build", "about"] },
];

export type Hit = { kind: "section"; section: SectionDef } | { kind: "setting"; setting: SettingDef; section: SectionDef };

/** Lower is better; null means no match. */
function score(title: string, keywords: string[], needle: string): number | null {
  const t = title.toLowerCase();
  if (t.startsWith(needle)) return 0;
  if (t.split(/[\s&]+/).some((w) => w.startsWith(needle))) return 1;
  if (keywords.some((k) => k.startsWith(needle))) return 2;
  if (t.includes(needle) || keywords.some((k) => k.includes(needle))) return 3;
  return null;
}

/** Sections and settings together; a section ranks ahead of an equal-scoring setting. */
export function searchSettings(query: string): Hit[] {
  const needle = query.trim().toLowerCase();
  if (!needle) return [];
  const hits: { hit: Hit; score: number; title: string }[] = [];
  for (const section of SECTIONS) {
    const s = score(section.title, section.keywords, needle);
    if (s !== null) hits.push({ hit: { kind: "section", section }, score: s * 2, title: section.title });
  }
  for (const setting of SETTINGS) {
    const section = SECTIONS.find((s) => s.id === setting.section)!;
    let s = score(setting.title, setting.keywords, needle);
    if (s === null && section.title.toLowerCase().startsWith(needle)) s = 4;
    if (s !== null) hits.push({ hit: { kind: "setting", setting, section }, score: s * 2 + 1, title: setting.title });
  }
  return hits.sort((a, b) => a.score - b.score || a.title.localeCompare(b.title)).map((h) => h.hit);
}
