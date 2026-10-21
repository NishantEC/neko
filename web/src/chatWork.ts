// The pure half of the work log: what a turn did in a sentence and how long
// it took. Same rules as native/NekoKit/Sources/NekoNative/ChatWork.swift.

type Kind = "command" | "edit" | "read" | "search" | "tool" | "other";
const ORDER: Kind[] = ["command", "edit", "read", "search", "tool", "other"];

export interface Step {
  text: string;
  kind: Kind;
  failed: boolean;
}

const VERBS: Record<string, Kind> = {
  ran: "command", run: "command", running: "command", executed: "command", started: "command",
  edited: "edit", wrote: "edit", created: "edit", updated: "edit", changed: "edit", patched: "edit",
  deleted: "edit", moved: "edit", renamed: "edit", saved: "edit", applied: "edit",
  read: "read", opened: "read", viewed: "read", listed: "read", inspected: "read", loaded: "read",
  searched: "search", searching: "search", grepped: "search", looked: "search", found: "search", globbed: "search",
};

export function kindOf(line: string): Kind {
  const verb = (line.match(/^[A-Za-z]+/)?.[0] ?? "").toLowerCase();
  return VERBS[verb] ?? "other";
}

export function steps(activity: string[] = []): Step[] {
  return activity
    .map((raw) => raw.trim())
    .filter(Boolean)
    .map((line) => {
      const failed = line.endsWith("· failed");
      return { text: failed ? line.slice(0, -"· failed".length).trim() : line, kind: kindOf(line), failed };
    });
}

function segment(kind: Kind, n: number): string {
  const s = n === 1 ? "" : "s";
  switch (kind) {
    case "command": return `ran ${n} command${s}`;
    case "edit": return `made ${n} edit${s}`;
    case "read": return `read ${n} file${s}`;
    case "search": return n === 1 ? "searched once" : `searched ${n} times`;
    case "tool": return `used ${n} tool${s}`;
    default: return `${n} other action${s}`;
  }
}

/** "Read 2 files and ran 1 command", or null when nothing ran. */
export function sentence(list: Step[]): string | null {
  const counts = new Map<Kind, number>();
  let failed = 0;
  for (const step of list) {
    counts.set(step.kind, (counts.get(step.kind) ?? 0) + 1);
    if (step.failed) failed += 1;
  }
  const parts = ORDER.filter((k) => counts.get(k)).map((k) => segment(k, counts.get(k)!));
  if (!parts.length) return null;
  const shown = parts.slice(0, 3);
  shown[0] = shown[0][0].toUpperCase() + shown[0].slice(1);
  let text = shown.length === 2 ? shown.join(" and ") : shown.join(", ");
  if (parts.length > 3) text += ` and ${parts.length - 3} more`;
  if (failed) text += ` · ${failed} failed`;
  return text;
}

/** "12s", "1m 12s", "1h 5m". */
export function duration(seconds: number): string {
  const s = Math.max(0, Math.round(seconds));
  if (s < 60) return `${s}s`;
  const m = Math.floor(s / 60);
  if (m < 60) return `${m}m ${s % 60}s`;
  return `${Math.floor(m / 60)}h ${m % 60}m`;
}

export function clock(ms: number): string {
  return new Date(ms).toLocaleTimeString([], { hour: "numeric", minute: "2-digit", hour12: false });
}
