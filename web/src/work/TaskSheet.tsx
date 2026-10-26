// from hushbacks apps/spa/src/pages/Builder/components/ResponseDetailPanel.tsx @5d9a64b11
//
// A task opened beside the Tasks list: hushbacks' response details sheet, with the
// task in place of the response. Same floating sheet (contained in the list's
// stage on a wide window, over the page otherwise), same header (title, then
// previous / next pebbles and close), heading + meta row, sky tabs, footer with
// "Task n of N" and ← → between tasks, same open-after-mount entry, quick in-place
// exit and swap fade. Added for Neko: copy link and full page in the header, the
// task's actions under the meta row, and the reply box above the footer.
// The full-page task is TaskBuilder (see TaskDetail).
// Neko's own shape since 2026-10-25 (owner): no "Task details" heading, the task's title
// is the heading; its actions, copy link and full page sit beside it; the tabs share a
// row with the status and meta; no previous/next buttons or footer (← → still move);
// and the sheet spans the whole page, header included.
import { type RefObject, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useWorkbench } from "@/useStore";
import { useOpenAfterMount } from "@/hooks/useOpenAfterMount";
import { ArrowsOutSimple, Check, Link as LinkIcon, X } from "@/ui/icons";
import { Button } from "@/ui/button";
import { Sheet, SheetClose, SheetContent, SheetDescription, SheetTitle } from "@/ui/sheet";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/ui/tooltip";
import { Empty, EmptyDescription, EmptyHeader, EmptyTitle } from "@/ui/empty";
import { cn } from "@/ui/lib/utils";
import { taskActions, type Ui } from "@/work/taskActions";
import { taskTitle, type Task } from "@/work/model";
import { TaskActions } from "@/work/TaskDetail";
import { repositoryOf } from "@/work/model";
import { TaskSheetBody } from "./TaskSheet.helperComponents";
import { useSwapFade } from "./TaskSheet.hooks";
import { TaskSheetSkeleton } from "./TaskSheet.skeletons";

type Props = {
  id: string;
  /** The visible tasks, in list order: previous / next walk this. */
  order: string[];
  ui: Ui;
  container?: RefObject<HTMLElement | null>;
  onSelect: (id: string) => void;
  onClose: () => void;
  onToggleFull: () => void;
  onOpenTask: (id: string) => void;
};

const IGNORE_ARROWS_IN = 'input, textarea, select, [role="tab"], [contenteditable="true"]';

export function TaskSheet({ id, order: list, ui, container, onSelect, onClose, onToggleFull, onOpenTask }: Props) {
  const { snapshot } = useWorkbench();
  const task: Task | undefined = (snapshot?.tasks ?? []).find((t: Task) => t.id === id);
  const [open, setOpen] = useOpenAfterMount();
  const closeButton = useRef<HTMLButtonElement>(null);
  const swapFade = useSwapFade(id);
  const [tab, setTab] = useState("conversation");
  useEffect(() => setTab("conversation"), [id]);
  const actions = useMemo(() => (task ? taskActions(task, snapshot) : []), [task, snapshot]);

  const order = list.includes(id) ? list : [id];
  const index = order.indexOf(id);
  const count = order.length;
  // The task a key press moves from: set the moment a move is asked for, so a second press
  // before the sheet re-renders moves on from the new task, not the old one again.
  const at = useRef(id);
  useEffect(() => { at.current = id; }, [id]);
  const select = (next: number) => {
    const target = order[next];
    if (target) { at.current = target; onSelect(target); }
  };

  const [copied, setCopied] = useState(false);
  const copyLink = useCallback(() => {
    navigator.clipboard?.writeText(`${location.origin}${location.pathname}#/tasks?task=${id}`).catch(() => {});
    setCopied(true);
    setTimeout(() => setCopied(false), 1200);
  }, [id]);

  // ← → move between tasks while the sheet is open, wherever focus is (a swap re-renders the
  // title, so focus can leave the sheet); typing in a field or on a tab is left alone.
  const move = useRef<(event: globalThis.KeyboardEvent) => void>(() => {});
  move.current = (event) => {
    if (event.defaultPrevented) return;
    if (event.target instanceof HTMLElement && event.target.closest(IGNORE_ARROWS_IN)) return;
    if (event.altKey || event.ctrlKey || event.metaKey || event.shiftKey) return;
    const from = order.indexOf(at.current) >= 0 ? order.indexOf(at.current) : index;
    if ((event.key === "ArrowLeft" || event.key === "k") && from > 0) {
      event.preventDefault();
      select(from - 1);
    }
    if ((event.key === "ArrowRight" || event.key === "j") && from < count - 1) {
      event.preventDefault();
      select(from + 1);
    }
  };
  useEffect(() => {
    const onKey = (event: globalThis.KeyboardEvent) => move.current(event);
    document.addEventListener("keydown", onKey, true);
    return () => document.removeEventListener("keydown", onKey, true);
  }, []);

  return (
    <Sheet
      open={open}
      onOpenChange={(next) => {
        if (!next) setOpen(false);
      }}
      onOpenChangeComplete={(next) => {
        if (!next) onClose();
      }}
    >
      <SheetContent
        data-task-id={id}
        aria-label={task ? taskTitle(task) : "Task details"}
        container={container}
        initialFocus={closeButton}
        showCloseButton={false}
        className={cn("app-sheet", container ? "w-[600px] max-w-[60%] sm:max-w-[60%]" : "sm:max-w-2xl")}
      >
        <div data-slot="task-detail" data-task-id={id} className="@container/sheet flex min-h-0 flex-1 flex-col">
          {/* One row: the task's title, then everything you do with it. */}
          <div className="flex shrink-0 items-start gap-3 px-6 pt-5 pb-3">
            <SheetTitle
              key={id}
              data-slot="task-title"
              className={cn("line-clamp-2 min-w-0 flex-1 pt-1 text-lg font-semibold tracking-tight break-words", swapFade)}
            >
              {task ? taskTitle(task) : "Task"}
            </SheetTitle>
            <div className="flex shrink-0 items-center gap-1">
              {task ? <TaskActions task={task} snapshot={snapshot} actions={actions} ui={ui} repo={repositoryOf(snapshot, task)} className="flex-nowrap" /> : null}
              <Tooltip>
                <TooltipTrigger asChild>
                  <Button variant="ghost" size="icon-sm" className="text-muted-foreground" aria-label="Copy a link to this task" onClick={copyLink}>
                    {copied ? <Check size={16} /> : <LinkIcon size={16} />}
                  </Button>
                </TooltipTrigger>
                <TooltipContent>{copied ? "Copied" : "Copy link"}</TooltipContent>
              </Tooltip>
              <Tooltip>
                <TooltipTrigger asChild>
                  <Button variant="ghost" size="icon-sm" className="text-muted-foreground" aria-label="Open as a full page" onClick={onToggleFull}>
                    <ArrowsOutSimple size={16} />
                  </Button>
                </TooltipTrigger>
                <TooltipContent>Full page</TooltipContent>
              </Tooltip>
              <SheetClose asChild>
                <Button ref={closeButton} variant="ghost" size="icon-sm" className="text-muted-foreground" aria-label="Close task">
                  <X size={16} />
                </Button>
              </SheetClose>
            </div>
          </div>
          <SheetDescription className="sr-only">
            Read the conversation, the changes and the details, or reply to the agent. The left and right arrow keys move between tasks.
          </SheetDescription>
          {!snapshot ? (
            <TaskSheetSkeleton />
          ) : task ? (
            <TaskSheetBody
              task={task}
              snapshot={snapshot}
              ui={ui}
              tab={tab}
              onTab={setTab}
              onOpenTask={onOpenTask}
              swapFade={swapFade}
            />
          ) : (
            <Empty className="flex-1 border-0">
              <EmptyHeader>
                <EmptyTitle>This task is gone</EmptyTitle>
                <EmptyDescription>It was deleted, or the link is from another Neko.</EmptyDescription>
              </EmptyHeader>
            </Empty>
          )}
        </div>
      </SheetContent>
    </Sheet>
  );
}
