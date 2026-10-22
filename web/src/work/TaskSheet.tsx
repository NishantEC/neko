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
import { type KeyboardEvent, type RefObject, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useWorkbench } from "@/useStore";
import { useOpenAfterMount } from "@/hooks/useOpenAfterMount";
import { ArrowsOutSimple, CaretLeft, CaretRight, Check, Link as LinkIcon, X } from "@/ui/icons";
import { Button } from "@/ui/button";
import { Sheet, SheetClose, SheetContent, SheetDescription, SheetFooter, SheetTitle } from "@/ui/sheet";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/ui/tooltip";
import { Empty, EmptyDescription, EmptyHeader, EmptyTitle } from "@/ui/empty";
import { cn } from "@/ui/lib/utils";
import { taskActions, type Ui } from "@/work/taskActions";
import { taskTitle, type Task } from "@/work/model";
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
  const select = (next: number) => {
    const target = order[next];
    if (target) onSelect(target);
  };

  const [copied, setCopied] = useState(false);
  const copyLink = useCallback(() => {
    navigator.clipboard?.writeText(`${location.origin}${location.pathname}#/tasks?task=${id}`).catch(() => {});
    setCopied(true);
    setTimeout(() => setCopied(false), 1200);
  }, [id]);

  const moveWithArrows = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.target instanceof HTMLElement && event.target.closest(IGNORE_ARROWS_IN)) return;
    if (event.altKey || event.ctrlKey || event.metaKey || event.shiftKey) return;
    if ((event.key === "ArrowLeft" || event.key === "k") && index > 0) {
      event.preventDefault();
      select(index - 1);
    }
    if ((event.key === "ArrowRight" || event.key === "j") && index < count - 1) {
      event.preventDefault();
      select(index + 1);
    }
  };

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
        onKeyDown={moveWithArrows}
        className={cn("app-sheet", container ? "w-[520px] max-w-[50%] sm:max-w-[50%]" : "sm:max-w-xl")}
      >
        <div data-slot="task-detail" data-task-id={id} className="flex min-h-0 flex-1 flex-col">
          <div className="flex h-16 shrink-0 items-center justify-between gap-2 px-6 pt-1">
            <SheetTitle>Task details</SheetTitle>
            <div className="flex items-center gap-1.5">
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
                  <Button variant="ghost" size="icon-sm" className="me-1 text-muted-foreground" aria-label="Open as a full page" onClick={onToggleFull}>
                    <ArrowsOutSimple size={16} />
                  </Button>
                </TooltipTrigger>
                <TooltipContent>Full page</TooltipContent>
              </Tooltip>
              <Button variant="ghost" size="icon-sm" className="sky-pebble" aria-label="Previous task" disabled={index <= 0} onClick={() => select(index - 1)}>
                <CaretLeft size={16} />
              </Button>
              <Button variant="ghost" size="icon-sm" className="sky-pebble" aria-label="Next task" disabled={index >= count - 1} onClick={() => select(index + 1)}>
                <CaretRight size={16} />
              </Button>
              <SheetClose asChild>
                <Button ref={closeButton} variant="ghost" size="icon-sm" className="ms-1 text-muted-foreground" aria-label="Close task">
                  <X size={16} />
                </Button>
              </SheetClose>
            </div>
          </div>
          <SheetDescription className="sr-only">
            Read the conversation, the changes and the details, reply to the agent, or move between tasks.
          </SheetDescription>
          {!snapshot ? (
            <TaskSheetSkeleton />
          ) : task ? (
            <TaskSheetBody
              task={task}
              snapshot={snapshot}
              ui={ui}
              actions={actions}
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
          <SheetFooter className="mt-0 flex-nowrap">
            <span className="whitespace-nowrap tabular-nums" aria-live="polite">
              Task {index + 1} of {count}
            </span>
            <span className="hidden lg:inline">← → to move between tasks</span>
          </SheetFooter>
        </div>
      </SheetContent>
    </Sheet>
  );
}
