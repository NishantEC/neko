// Which workspace a setup page is looking at. The native app keeps this in its
// sidebar; here it is one remembered choice shared by Watching, Schedules,
// Tools & skills and Memory ("" = every workspace).
import { useSyncExternalStore } from "react";
import type { Json } from "@/api";
import { Pick } from "@/setup/kit";
import { workspacesOf } from "@/setup/lib";

const KEY = "neko.web.workspace";
const listeners = new Set<() => void>();
const read = () => {
  try { return localStorage.getItem(KEY) ?? ""; } catch { return ""; }
};

export function setScope(id: string) {
  try { localStorage.setItem(KEY, id); } catch { /* private mode */ }
  listeners.forEach((l) => l());
}

export function useScope(snapshot: Json): string {
  const stored = useSyncExternalStore((l) => { listeners.add(l); return () => listeners.delete(l); }, read);
  // A workspace that no longer exists falls back to "all".
  return stored && workspacesOf(snapshot).some((w) => w.id === stored) ? stored : "";
}

export function ScopePicker({ snapshot, label = "Workspace" }: { snapshot: Json; label?: string }) {
  const scope = useScope(snapshot);
  const workspaces = workspacesOf(snapshot);
  if (workspaces.length < 2) return null;
  return (
    <Pick
      label={label}
      value={scope || "__all"}
      onChange={(v) => setScope(v === "__all" ? "" : v)}
      options={[{ value: "__all", label: "All workspaces" }, ...workspaces.map((w) => ({ value: w.id, label: w.name }))]}
    />
  );
}
