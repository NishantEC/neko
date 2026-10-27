// The workspace you are looking at: one choice for the whole app, picked in the
// sidebar's switcher and read by Home (the chat's workspace), Tasks, Pull
// requests and the Agent tree. "" means all workspaces. Kept in localStorage
// under the key the Tasks page's own filter already used, so an existing
// choice carries over, and shared live across tabs (useLocal).
import { useWorkbench } from "@/useStore";
import { useLocal } from "@/work/useLocal";

export const WORKSPACE_KEY = "neko.work.workspace";

export type WorkspaceRef = { id: string; name: string };

export function useSelectedWorkspace(): {
  /** The chosen workspace's id, or "" for all (also when the choice no longer exists). */
  id: string;
  current: WorkspaceRef | null;
  workspaces: WorkspaceRef[];
  select: (id: string) => void;
} {
  const { snapshot } = useWorkbench();
  const [raw, select] = useLocal(WORKSPACE_KEY, "");
  const workspaces: WorkspaceRef[] = snapshot?.workspaces ?? [];
  const current = workspaces.find((w) => w.id === raw) ?? null;
  return { id: current?.id ?? "", current, workspaces, select };
}
