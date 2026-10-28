// #/tasks: every task as a list, a board or a tree, with the open task beside it
// (TicketsView.swift, TicketPeek.swift) in hushbacks' response details sheet
// (work/TaskSheet). A task is a link: #/tasks?task=<id>, and `&view=full` shows it
// as a page. j / k move between tasks, Enter opens one, / searches; in the sheet
// ← → (or j / k) move to the previous / next task and Esc closes it.
import { useCallback, useEffect, useMemo, useRef, useState, useSyncExternalStore } from "react";
import { useWorkbench } from "@/useStore";
import { registerCommands, type PaletteCommand } from "@/shell/commands";
import { DotsThree, ListChecks, MagnifyingGlass, Plus, TreeStructure, X } from "@/ui/icons";
import { Kanban, Rows } from "@/work/icons";
import { Button } from "@/ui/button";
import { InputGroup, InputGroupAddon, InputGroupInput } from "@/ui/input-group";
import { SegmentedControl, SegmentedControlItem } from "@/ui/segmented-control";
import { StageTabs } from "@/work/StageFilters";
import { EditorialHeading } from "@/editorial";
import { isBoardDragging } from "@/work/board/moves";
import { Kbd } from "@/ui/kbd";
import {
  DropdownMenu, DropdownMenuCheckboxItem, DropdownMenuContent, DropdownMenuGroup, DropdownMenuItem, DropdownMenuLabel, DropdownMenuSeparator, DropdownMenuTrigger,
} from "@/ui/dropdown-menu";
import { Empty, EmptyContent, EmptyDescription, EmptyHeader, EmptyMedia, EmptyTitle } from "@/ui/empty";
import { cn } from "@/ui/lib/utils";
import { run } from "@/work/actions";
import { useDialogs } from "@/work/dialogs";
import { byStage, flatten, stageCounts, type Filters } from "@/work/filters";
import { setHashParams, useHashParams } from "@/work/location";
import { WorkSkeleton } from "@/work/PageFrame";
import { TaskDetail } from "@/work/TaskDetail";
import { TaskSheet } from "@/work/TaskSheet";
import { taskActions } from "@/work/taskActions";
import { TaskBoard, TaskList } from "@/work/TaskViews";
import { TreeView } from "@/work/TreeView";
import { useLocal } from "@/work/useLocal";
import { useSelectedWorkspace } from "@/shell/workspace";
import { buildTree, taskCount, taskTitle, type Stage } from "@/work/model";
import { navigate } from "@/router";

type Layout = "list" | "board" | "tree";
const isLayout = (v: string): v is Layout => v === "list" || v === "board" || v === "tree";
const ROUTE = "tasks";
const COUNTED: Stage[] = ["needsYou", "review", "running", "upNext", "done"];

// hushbacks docks the sheet in the list's stage on a wide window and floats it over the page otherwise.
const DOCK_QUERY = "(min-width: 1024px)";
const subscribeDock = (onChange: () => void) => {
  const mql = matchMedia(DOCK_QUERY);
  mql.addEventListener("change", onChange);
  return () => mql.removeEventListener("change", onChange);
};
const canDock = () => matchMedia(DOCK_QUERY).matches;

const typing = (target: EventTarget | null) =>
  target instanceof HTMLElement && (target.isContentEditable || ["INPUT", "TEXTAREA", "SELECT"].includes(target.tagName));

export default function Tasks() {
  const { snapshot, connected, error, unauthorized } = useWorkbench();
  const params = useHashParams();
  const taskId = params.get("task");
  const full = params.get("view") === "full";
  const { ui, dialogs } = useDialogs();

  const [layoutRaw, setLayout] = useLocal("neko.work.layout", "list");
  const layout: Layout = isLayout(layoutRaw) ? layoutRaw : "list";
  const [doneAll, setDoneAllRaw] = useLocal("neko.board.doneShowAll", "false");
  // The sidebar's workspace switcher is the only workspace filter (shell/workspace.ts).
  const { id: workspace, select: setWorkspace } = useSelectedWorkspace();
  const [includeStoppedRaw, setIncludeStopped] = useLocal("neko.work.includeStopped", "true");
  const [stage, setStage] = useState<Stage | "all">("all");
  const [search, setSearch] = useState("");
  const searchBox = useRef<HTMLInputElement>(null);
  const page = useRef<HTMLDivElement>(null);
  const detailStage = useRef<HTMLDivElement>(null);
  const dock = useSyncExternalStore(subscribeDock, canDock);
  const [focusedId, setFocusedId] = useState<string | null>(null);

  const workspaces: { id: string; name: string }[] = snapshot?.workspaces ?? [];
  const workspaceId = workspaces.some((w) => w.id === workspace) ? workspace : "";
  const filters: Filters = { workspace: workspaceId, stage, search, includeStopped: includeStoppedRaw !== "false" };
  const allTasks: any[] = snapshot?.tasks ?? [];
  const grouped = useMemo(() => byStage(snapshot, filters), [snapshot, workspaceId, stage, search, includeStoppedRaw]); // eslint-disable-line react-hooks/exhaustive-deps
  const visible = useMemo(() => flatten(grouped), [grouped]);
  const counts = useMemo(() => stageCounts(snapshot, filters), [snapshot, workspaceId, search, includeStoppedRaw]); // eslint-disable-line react-hooks/exhaustive-deps
  const inWorkspace = workspaceId ? allTasks.filter((t) => t.workspace_id === workspaceId).length : allTasks.length;
  const workspaceName = workspaces.find((w) => w.id === workspaceId)?.name ?? null;
  const planFirst = snapshot?.plan_first === true;

  const open = useCallback((id: string) => setHashParams(ROUTE, { task: id }), []);
  const close = useCallback(() => setHashParams(ROUTE, { task: null, view: null }), []);
  const toggleFull = useCallback(() => setHashParams(ROUTE, { view: full ? null : "full" }), [full]);

  // The tree answers to the same filters; idle watches would only add noise to a search.
  const tree = useMemo(
    () => (layout === "tree" && snapshot ? buildTree(snapshot, visible, { workspace: workspaceId || null, showIdleWatches: !search && stage === "all", now: Date.now() }) : null),
    [layout, snapshot, visible, workspaceId, search, stage],
  );

  // --- keyboard: j / k, / and Esc --------------------------------------------------
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.metaKey || e.ctrlKey || e.altKey || typing(e.target)) return;
      if (document.querySelector("[role=alertdialog],[role=dialog]:not([data-type]),[data-slot='command']")) return;
      if (e.key === "/") { e.preventDefault(); searchBox.current?.focus(); return; }
      if (e.key === "Escape" && taskId && !isBoardDragging()) { e.preventDefault(); close(); return; }
      if (e.key !== "j" && e.key !== "k") return;
      const rows = [...(page.current?.querySelectorAll<HTMLElement>("[data-slot='task-row']") ?? [])];
      if (!rows.length) return;
      const at = rows.findIndex((r) => r === document.activeElement);
      const next = at < 0 ? rows.findIndex((r) => r.dataset.taskId === taskId) + (e.key === "j" ? 1 : -1) : at + (e.key === "j" ? 1 : -1);
      const row = rows[Math.min(rows.length - 1, Math.max(0, at < 0 && taskId ? next : at < 0 ? 0 : next))];
      e.preventDefault();
      row.focus();
      row.scrollIntoView({ block: "nearest" });
    };
    addEventListener("keydown", onKey);
    return () => removeEventListener("keydown", onKey);
  }, [taskId, close]);

  // --- ⌘K ---------------------------------------------------------------------------
  const target = allTasks.find((t) => t.id === (taskId ?? focusedId));
  const targetKey = target ? `${target.id}:${target.status}:${target.display_title ?? ""}` : "";
  useEffect(() => {
    const list: PaletteCommand[] = [
      { id: "tasks.search", title: "Search tasks", keywords: ["filter", "find"], icon: MagnifyingGlass, shortcut: "/", group: "Tasks", run: () => searchBox.current?.focus() },
      { id: "tasks.list", title: "Show tasks as a list", keywords: ["layout", "rows"], icon: Rows, group: "Tasks", run: () => setLayout("list") },
      { id: "tasks.board", title: "Show tasks as a board", keywords: ["layout", "kanban", "columns"], icon: Kanban, group: "Tasks", run: () => setLayout("board") },
      { id: "tasks.tree", title: "Show who started each task (tree)", keywords: ["layout", "agents"], icon: TreeStructure, group: "Tasks", run: () => setLayout("tree") },
    ];
    if (target && snapshot) {
      for (const action of taskActions(target, snapshot)) {
        if (!action.palette) continue;
        list.push({
          id: `tasks.action.${action.id}`, title: action.palette, keywords: [taskTitle(target), "task"], icon: ListChecks, group: "Tasks",
          run: () => (action.href ? window.open(action.href, "_blank", "noreferrer") : action.run?.(ui)),
        });
      }
    }
    return registerCommands(list);
  }, [targetKey, setLayout, ui]); // eslint-disable-line react-hooks/exhaustive-deps

  const isEmpty = !!snapshot && allTasks.length === 0;
  const noMatches = !!snapshot && allTasks.length > 0 && visible.length === 0;
  const filtered = search !== "" || stage !== "all" || workspaceId !== "" || includeStoppedRaw === "false";
  // "24", or "12 of 24" while a filter is on; the workspace's name when the sidebar has one chosen.
  const total = COUNTED.reduce((n, s) => n + counts[s], 0);
  const countText = !snapshot ? "" : visible.length === inWorkspace ? `${inWorkspace}` : `${visible.length} of ${inWorkspace}`;
  const countLabel = !snapshot ? "Loading" : `${visible.length === inWorkspace ? taskCount(inWorkspace) : `${visible.length} of ${taskCount(inWorkspace)}`}${workspaceName ? ` in ${workspaceName}` : ""}`;

  const toolbar = (
    <div className="mt-5 flex flex-wrap items-center gap-3" role="search" aria-label="Filter tasks">
      <StageTabs stage={stage} counts={counts} total={total} onChange={setStage} />
      <InputGroup className="sky-well sky-control ms-auto w-52 border-transparent bg-transparent shadow-none">
        <InputGroupAddon><MagnifyingGlass aria-hidden /></InputGroupAddon>
        <InputGroupInput
          ref={searchBox} value={search} onChange={(e) => setSearch(e.target.value)} placeholder="Search tasks" aria-label="Search tasks"
          onKeyDown={(e) => { if (e.key === "Escape") { if (search) setSearch(""); else e.currentTarget.blur(); } }}
        />
        {search ? (
          <InputGroupAddon align="inline-end">
            <button type="button" aria-label="Clear search" onClick={() => { setSearch(""); searchBox.current?.focus(); }} className="rounded-sm text-muted-foreground outline-none hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring"><X size={13} /></button>
          </InputGroupAddon>
        ) : <InputGroupAddon align="inline-end"><Kbd>/</Kbd></InputGroupAddon>}
      </InputGroup>
      <SegmentedControl value={layout} onValueChange={(v) => isLayout(v) && setLayout(v)} aria-label="Layout">
        <SegmentedControlItem value="list" aria-label="List" title="Show tasks as a list"><Rows /></SegmentedControlItem>
        <SegmentedControlItem value="board" aria-label="Board" title="Show tasks as a board"><Kanban /></SegmentedControlItem>
        <SegmentedControlItem value="tree" aria-label="Tree" title="Show which agent started each task"><TreeStructure /></SegmentedControlItem>
      </SegmentedControl>
    </div>
  );

  const headerActions = (
    <>
      <DropdownMenu>
        <DropdownMenuTrigger render={<Button variant="ghost" size="icon-sm" aria-label="Work options" title="Work options" />}><DotsThree weight="bold" /></DropdownMenuTrigger>
        <DropdownMenuContent align="end" className="min-w-72">
          <DropdownMenuGroup>
          <DropdownMenuLabel>New tasks</DropdownMenuLabel>
          <DropdownMenuCheckboxItem checked={planFirst} onCheckedChange={(enabled) => void run("SetPlanFirst", { enabled })}>
            Plan before working
          </DropdownMenuCheckboxItem>
          <p className="px-2 pb-1.5 text-[12px] leading-[1.45] text-muted-foreground">
            {planFirst ? "Every task first writes a read-only plan for you to approve." : "Tasks start working right away. The agent stops for your approval only on critical work."}
          </p>
          {planFirst ? (
            <DropdownMenuCheckboxItem checked={snapshot?.start_without_approval === true} onCheckedChange={(enabled) => void run("SetStartWithoutApproval", { enabled })}>
              Start tasks without asking
            </DropdownMenuCheckboxItem>
          ) : null}
          </DropdownMenuGroup>
          <DropdownMenuSeparator />
          <DropdownMenuCheckboxItem checked={includeStoppedRaw !== "false"} onCheckedChange={(on) => setIncludeStopped(on ? "true" : "false")}>
            Show failed and cancelled
          </DropdownMenuCheckboxItem>
          <DropdownMenuItem
            disabled={!allTasks.some((t) => ["Completed", "Cancelled"].includes(t.status))}
            onClick={() => ui.confirm({
              title: "Clear finished tasks?",
              description: "Removes every completed or cancelled task from history. Failed tasks stay so you can retry them. Your files and worktrees on disk stay as they are.",
              confirm: "Clear finished tasks", destructive: true,
              onConfirm: () => run("ClearFinishedTasks", { workspace_id: workspaceId || null }, "Cleared finished tasks."),
            })}
          >
            Clear finished tasks…
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
      <Button size="sm" onClick={() => navigate("home")}><Plus /> New task</Button>
    </>
  );

  const viewProps = { snapshot, byStage: grouped, selectedId: taskId, onOpen: open, ui, showWorkspace: !workspaceId && workspaces.length > 1 };

  const body = !snapshot ? (
    unauthorized ? (
      <Empty className="border-0"><EmptyHeader><EmptyTitle>Not signed in</EmptyTitle><EmptyDescription>This page is not signed in to the Neko daemon. Open it from Neko.</EmptyDescription></EmptyHeader></Empty>
    ) : error ? (
      <Empty className="border-0"><EmptyHeader><EmptyTitle>Can’t reach the Neko daemon</EmptyTitle><EmptyDescription>{error} Retrying.</EmptyDescription></EmptyHeader></Empty>
    ) : <WorkSkeleton />
  ) : isEmpty ? (
    <Empty className="border-0">
      <EmptyHeader>
        <EmptyMedia variant="icon"><ListChecks /></EmptyMedia>
        <EmptyTitle>No tasks yet</EmptyTitle>
        <EmptyDescription>Describe what you want done in Home and Neko files it here.</EmptyDescription>
      </EmptyHeader>
      <EmptyContent><Button onClick={() => navigate("home")}><Plus /> New task</Button></EmptyContent>
    </Empty>
  ) : noMatches || (layout === "tree" && tree && tree.root.children.length === 0) ? (
    <Empty className="border-0">
      <EmptyHeader>
        <EmptyMedia variant="icon"><MagnifyingGlass /></EmptyMedia>
        <EmptyTitle>{search ? `No tasks match “${search}”` : "No tasks here"}</EmptyTitle>
        <EmptyDescription>{filtered ? "Nothing fits the filters you have on." : "Nothing to show."}</EmptyDescription>
      </EmptyHeader>
      {filtered ? (
        <EmptyContent><Button variant="outline" onClick={() => { setSearch(""); setStage("all"); setWorkspace(""); setIncludeStopped("true"); }}>Clear filters</Button></EmptyContent>
      ) : null}
    </Empty>
  ) : layout === "board" ? (
    <TaskBoard
      snapshot={snapshot} byStage={grouped} selectedId={taskId} onOpen={open} ui={ui}
      doneAll={doneAll === "true"} setDoneAll={(v) => setDoneAllRaw(v ? "true" : "false")} only={stage === "all" ? null : stage}
      onShowOnly={setStage} onNewTask={() => navigate("home")}
    />
  ) : layout === "tree" && tree ? (
    <TreeView tree={tree} onOpenTask={open} onOpenHome={() => navigate("home")} onOpenWatching={() => navigate("watching")} />
  ) : (
    <TaskList {...viewProps} />
  );

  if (taskId && full) {
    return (
      <div ref={page} data-slot="tasks-page" data-layout={layout} className="flex min-h-0 flex-1 flex-col">
        <TaskDetail key={taskId} id={taskId} ui={ui} onClose={close} onToggleFull={toggleFull} onOpenTask={open} />
        {dialogs}
      </div>
    );
  }

  return (
    <div
      ref={page}
      data-slot="tasks-page"
      data-layout={layout}
      onFocus={(e) => { const row = (e.target as HTMLElement).closest<HTMLElement>("[data-slot='task-row']"); if (row) setFocusedId(row.dataset.taskId ?? null); }}
      className="relative flex min-h-0 flex-1 flex-col"
    >
      {/* The Tasks header: tighter than WorkHeader (24px top, 28px sides), title and count on one line, then one toolbar row. */}
      <header data-slot="work-header" className="shrink-0 px-7 pt-6 pb-4">
        <div className="flex items-center gap-3">
          <EditorialHeading as="h1" className="text-[22px]">Tasks</EditorialHeading>
          <span data-slot="work-subtitle" aria-label={countLabel} className="text-[13.5px] tabular-nums text-muted-foreground">
            {countText}{workspaceName ? <> · {workspaceName}</> : null}
          </span>
          <div className="ms-auto flex items-center gap-2">{headerActions}</div>
        </div>
        {toolbar}
      </header>
      {!connected && snapshot ? <p role="status" className="px-7 pb-2 text-[13px] text-destructive">Can’t reach the Neko daemon. Showing the last known state.</p> : null}
      <div ref={detailStage} data-slot="tasks-stage" className="relative flex min-h-0 flex-1">
        <div className={cn("h-full min-w-0 flex-1", layout === "board" ? "overflow-x-auto overflow-y-hidden" : "overflow-y-auto")}>{body}</div>
        {taskId ? (
          <TaskSheet
            id={taskId}
            order={visible.map((t: any) => t.id)}
            ui={ui}
            container={dock ? page : undefined}
            onSelect={open}
            onClose={close}
            onToggleFull={toggleFull}
            onOpenTask={open}
          />
        ) : null}
      </div>
      {dialogs}
    </div>
  );
}
