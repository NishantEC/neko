// #/tree: who started whom. Neko on top, the watches and tasks it answers for
// below it, and the agent runs under each task (AgentTreeView.swift). The same
// tree is a layout inside Tasks; this page is that layout on its own.
import { useMemo, useState } from "react";
import { useWorkbench } from "@/useStore";
import { MagnifyingGlass, TreeStructure } from "@/ui/icons";
import { InputGroup, InputGroupAddon, InputGroupInput } from "@/ui/input-group";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/ui/select";
import { Empty, EmptyDescription, EmptyHeader, EmptyMedia, EmptyTitle } from "@/ui/empty";
import { Button } from "@/ui/button";
import { navigate } from "@/router";
import { WorkHeader, WorkSkeleton } from "@/work/PageFrame";
import { TreeView } from "@/work/TreeView";
import { buildTree, taskCount } from "@/work/model";
import { byStage, flatten } from "@/work/filters";

export default function Tree() {
  const { snapshot, error } = useWorkbench();
  const [search, setSearch] = useState("");
  const [workspace, setWorkspace] = useState("");
  const workspaces: { id: string; name: string }[] = snapshot?.workspaces ?? [];

  const tree = useMemo(() => {
    if (!snapshot) return null;
    const visible = flatten(byStage(snapshot, { workspace, stage: "all", search, includeStopped: true }));
    return buildTree(snapshot, visible, { workspace: workspace || null, showIdleWatches: !search, now: Date.now() });
  }, [snapshot, search, workspace]);

  const openTask = (id: string) => {
    location.hash = `/tasks?task=${encodeURIComponent(id)}`;
  };
  const label = workspaces.find((w) => w.id === workspace)?.name ?? "All workspaces";

  return (
    <div data-slot="tree-page" className="flex min-h-0 flex-1 flex-col">
      <WorkHeader
        eyebrow="Work"
        title={<>Agent tree</>}
        subtitle={tree ? `${label} · ${taskCount(tree.tasks)}` : "Loading…"}
        actions={
          <>
            <InputGroup className="h-8 w-56">
              <InputGroupAddon><MagnifyingGlass aria-hidden /></InputGroupAddon>
              <InputGroupInput value={search} onChange={(e) => setSearch(e.target.value)} placeholder="Search tasks" aria-label="Search tasks" />
            </InputGroup>
            {workspaces.length > 1 ? (
              <Select value={workspace || "all"} onValueChange={(v) => setWorkspace(v === "all" ? "" : v)}>
                <SelectTrigger size="sm" className="w-44" aria-label="Workspace"><SelectValue>{label}</SelectValue></SelectTrigger>
                <SelectContent>
                  <SelectItem value="all">All workspaces</SelectItem>
                  {workspaces.map((w) => <SelectItem key={w.id} value={w.id}>{w.name}</SelectItem>)}
                </SelectContent>
              </Select>
            ) : null}
            <Button variant="outline" size="sm" asChild><a href="#/tasks">Open Tasks</a></Button>
          </>
        }
      />
      <div className="min-h-0 flex-1 overflow-y-auto">
        {!tree ? (
          error ? (
            <Empty className="border-0"><EmptyHeader><EmptyTitle>Can’t reach the Neko daemon</EmptyTitle><EmptyDescription>{error} Retrying.</EmptyDescription></EmptyHeader></Empty>
          ) : <WorkSkeleton />
        ) : tree.root.children.length === 0 ? (
          <Empty className="border-0">
            <EmptyHeader>
              <EmptyMedia variant="icon"><TreeStructure /></EmptyMedia>
              <EmptyTitle>{search ? `No tasks match “${search}”` : "Nothing to look after yet"}</EmptyTitle>
              <EmptyDescription>{search ? "Try fewer words." : "Tasks, and the watches that file them, appear here once there are some."}</EmptyDescription>
            </EmptyHeader>
          </Empty>
        ) : (
          <TreeView tree={tree} onOpenTask={openTask} onOpenHome={() => navigate("home")} onOpenWatching={() => navigate("watching")} />
        )}
      </div>
    </div>
  );
}
