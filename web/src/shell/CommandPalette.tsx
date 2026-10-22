// ⌘K. Every action that exists anywhere in the web app, searchable:
//
//   Actions       the shell's own (new task, stop all work, approve, shortcuts,
//                 setup tour, reload plugins) and whatever the open page
//                 registers (shell/commands.ts)
//   Go to         every page in pages/index.ts and every plugin page
//   Recent tasks  the latest tasks; ↵ opens, ⌘C copies the task's ID
//   Appearance    light, dark, system
//
// Page authors: see the header of shell/commands.ts.
import { useEffect, useMemo, useState } from "react";
import { Command, CommandDialog, CommandEmpty, CommandGroup, CommandInput, CommandItem, CommandList, CommandShortcut } from "@/ui/command";
import { Kbd } from "@/ui/kbd";
import { Keyboard } from "@phosphor-icons/react";
import {
  ArrowRight, ArrowsClockwise, CheckCircle, CircleHalf, Copy, Moon, Plus, PuzzlePiece, Rocket, Sun, Stop,
} from "@/ui/icons";
import { pages } from "@/pages";
import { navigate } from "@/router";
import { refreshPlugins, usePlugins } from "@/plugins";
import { setTheme } from "@/theme";
import { useCommands, type PaletteCommand } from "@/shell/commands";
import { approveFirstDecision, copyText, newTask, stopAllWork } from "@/shell/actions";
import { openShortcuts } from "@/shell/shortcuts";
import { STAGE_TITLES, stageOf, taskHref, taskTitle, useShellStatus } from "@/shell/status";
import { openOnboarding } from "@/onboarding/open";
import { cn } from "@/ui/lib/utils";
import { useWorkbench } from "@/useStore";

const STAGE_DOT: Record<string, string> = {
  needsYou: "bg-warning",
  review: "bg-info",
  running: "bg-primary animate-pulse",
  upNext: "bg-muted-foreground/60",
  done: "bg-success",
};

export function CommandPalette() {
  const [open, setOpen] = useState(false);
  const { pages: pluginPages } = usePlugins();
  const registered = useCommands();
  const { snapshot } = useWorkbench();
  const status = useShellStatus();

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key.toLowerCase() === "k" && (e.metaKey || e.ctrlKey)) {
        e.preventDefault();
        setOpen((value) => !value);
      }
    };
    addEventListener("keydown", onKey);
    return () => removeEventListener("keydown", onKey);
  }, []);

  const run = (fn: () => void) => () => {
    setOpen(false);
    // After the dialog has released focus, so a "focus the composer" action sticks.
    setTimeout(fn, 60);
  };

  const shellActions = useMemo<PaletteCommand[]>(
    () => [
      { id: "shell.new-task", title: "New task", keywords: ["create", "compose", "ask", "start"], icon: Plus, shortcut: "⌘N", run: newTask },
      { id: "shell.stop-all", title: "Stop all work", keywords: ["cancel", "halt", "kill", "agents"], icon: Stop, shortcut: "⌘⇧Esc", run: () => void stopAllWork() },
      ...(status.approvals > 0
        ? [{ id: "shell.approve", title: "Approve what is waiting", hint: `${status.approvals} waiting`, keywords: ["yes", "allow", "decision", "pending"], icon: CheckCircle, shortcut: "⌘↵", run: () => void approveFirstDecision() }]
        : []),
      { id: "shell.shortcuts", title: "Keyboard shortcuts", keywords: ["keys", "help", "hotkeys"], icon: Keyboard, shortcut: "?", run: openShortcuts },
      { id: "shell.setup", title: "Show the first-run tour", keywords: ["onboarding", "welcome", "setup", "permissions"], icon: Rocket, run: () => openOnboarding("welcome") },
    ],
    [status.approvals],
  );

  const groups = useMemo(() => {
    const byGroup = new Map<string, PaletteCommand[]>();
    for (const c of registered) byGroup.set(c.group ?? "Actions", [...(byGroup.get(c.group ?? "Actions") ?? []), c]);
    return [...byGroup];
  }, [registered]);

  const recent = useMemo(
    () => [...(snapshot?.tasks ?? [])].sort((a, b) => b.updated_at_ms - a.updated_at_ms).slice(0, 6),
    [snapshot?.tasks],
  );

  // ⌘C on a highlighted task copies its ID.
  const onKeyDown = (e: React.KeyboardEvent) => {
    if (!(e.metaKey || e.ctrlKey) || e.key.toLowerCase() !== "c") return;
    const input = e.target as HTMLInputElement;
    if (input.selectionStart !== input.selectionEnd) return; // a real text copy
    const id = (e.currentTarget as HTMLElement).querySelector<HTMLElement>("[cmdk-item][data-selected='true']")?.dataset.taskId;
    if (!id) return;
    e.preventDefault();
    setOpen(false);
    void copyText(id, "Task ID copied");
  };

  const item = (c: PaletteCommand) => (
    <CommandItem key={c.id} value={`${c.title} ${c.id}`} keywords={c.keywords} onSelect={run(c.run)}>
      {c.icon ? <c.icon /> : null}
      <span className="min-w-0 flex-1 truncate">
        {c.title}
        {c.hint ? <span className="ms-2 text-[12px] text-muted-foreground">{c.hint}</span> : null}
      </span>
      {c.shortcut ? <CommandShortcut>{c.shortcut}</CommandShortcut> : null}
    </CommandItem>
  );

  return (
    <CommandDialog open={open} onOpenChange={setOpen} title="Command palette" description="Go to a page or run an action.">
      <Command onKeyDown={onKeyDown}>
        <CommandInput placeholder="Go to a page, find a task or run an action…" aria-label="Command palette" />
        <CommandList className="max-h-[min(440px,60vh)]">
          <CommandEmpty>Nothing matches. Try a page name or a task title.</CommandEmpty>
          <CommandGroup heading="Actions">
            {shellActions.map(item)}
            {(groups.find(([heading]) => heading === "Actions")?.[1] ?? []).map(item)}
            <CommandItem value="Reload plugins refresh" keywords={["plugins"]} onSelect={run(() => void refreshPlugins())}>
              <ArrowsClockwise /> Reload plugins
            </CommandItem>
          </CommandGroup>
          {groups.filter(([heading]) => heading !== "Actions").map(([heading, list]) => (
            <CommandGroup key={heading} heading={heading}>{list.map(item)}</CommandGroup>
          ))}
          <CommandGroup heading="Go to">
            {pages.map((p) => (
              <CommandItem key={p.id} value={`Go to ${p.title}`} keywords={p.keywords} onSelect={run(() => navigate(p.id))}>
                <p.icon />
                <span className="min-w-0 flex-1 truncate">Go to {p.title}</span>
                <ArrowRight className="opacity-40" />
              </CommandItem>
            ))}
            {pluginPages.map((p) => (
              <CommandItem key={p.key} value={`Go to ${p.title}`} keywords={["plugin", p.plugin]} onSelect={run(() => navigate(p.key))}>
                <PuzzlePiece />
                <span className="min-w-0 flex-1 truncate">Go to {p.title}</span>
                <CommandShortcut>plugin</CommandShortcut>
              </CommandItem>
            ))}
          </CommandGroup>
          {recent.length > 0 ? (
            <CommandGroup heading="Recent tasks">
              {recent.map((task) => {
                const stage = stageOf(task.status);
                return (
                  <CommandItem
                    key={task.id}
                    data-task-id={task.id}
                    value={`Task ${taskTitle(task)} ${task.id}`}
                    keywords={["task", task.id, task.status]}
                    onSelect={run(() => { location.hash = taskHref(task.id).slice(1); })}
                  >
                    <span aria-hidden className={cn("size-2 shrink-0 rounded-full", STAGE_DOT[stage])} />
                    <span className="min-w-0 flex-1 truncate">{taskTitle(task)}</span>
                    <span className="shrink-0 text-[12px] text-muted-foreground">{STAGE_TITLES[stage]}</span>
                    <button
                      type="button"
                      tabIndex={-1}
                      aria-label={`Copy ID of ${taskTitle(task)}`}
                      title="Copy ID (⌘C)"
                      onClick={(e) => { e.stopPropagation(); setOpen(false); void copyText(task.id, "Task ID copied"); }}
                      className="grid size-6 shrink-0 place-items-center rounded-md text-muted-foreground hover:bg-foreground/10 hover:text-foreground"
                    >
                      <Copy size={13} />
                    </button>
                  </CommandItem>
                );
              })}
            </CommandGroup>
          ) : null}
          <CommandGroup heading="Appearance">
            <CommandItem value="Theme: light" keywords={["appearance", "bright"]} onSelect={run(() => setTheme("light"))}>
              <Sun /> Light theme
            </CommandItem>
            <CommandItem value="Theme: dark" keywords={["appearance"]} onSelect={run(() => setTheme("dark"))}>
              <Moon /> Dark theme
            </CommandItem>
            <CommandItem value="Theme: system" keywords={["appearance", "auto"]} onSelect={run(() => setTheme("system"))}>
              <CircleHalf /> Follow the system theme
            </CommandItem>
          </CommandGroup>
        </CommandList>
        <div data-slot="palette-footer" className="flex items-center gap-3 border-t border-border/70 px-3 py-2 text-[11.5px] text-muted-foreground">
          <span className="flex items-center gap-1"><Kbd>↵</Kbd> run</span>
          <span className="flex items-center gap-1"><Kbd>⌘</Kbd><Kbd>C</Kbd> copy a task’s ID</span>
          <span className="ms-auto flex items-center gap-1"><Kbd>esc</Kbd> close</span>
        </div>
      </Command>
    </CommandDialog>
  );
}

/** Opens the palette from anywhere (the sidebar's search button). */
export function openPalette() {
  dispatchEvent(new KeyboardEvent("keydown", { key: "k", metaKey: true }));
}
