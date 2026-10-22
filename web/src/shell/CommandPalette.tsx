// ⌘K. "Go to <page>" for every registered and plugin page, the actions the
// open page contributes (shell/commands.ts), and the theme switch.
import { useEffect, useMemo, useState } from "react";
import { Command, CommandDialog, CommandEmpty, CommandGroup, CommandInput, CommandItem, CommandList, CommandShortcut } from "@/ui/command";
import { ArrowRight, Moon, PuzzlePiece, Sun, CircleHalf, ArrowsClockwise } from "@/ui/icons";
import { pages } from "@/pages";
import { navigate } from "@/router";
import { refreshPlugins, usePlugins } from "@/plugins";
import { setTheme } from "@/theme";
import { useCommands } from "@/shell/commands";

export function CommandPalette() {
  const [open, setOpen] = useState(false);
  const { pages: pluginPages } = usePlugins();
  const commands = useCommands();

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

  const groups = useMemo(() => {
    const byGroup = new Map<string, typeof commands>();
    for (const c of commands) byGroup.set(c.group ?? "Actions", [...(byGroup.get(c.group ?? "Actions") ?? []), c]);
    return [...byGroup];
  }, [commands]);

  return (
    <CommandDialog open={open} onOpenChange={setOpen} title="Command palette" description="Go to a page or run an action.">
      <Command>
        <CommandInput placeholder="Go to a page or run an action…" aria-label="Command palette" />
        <CommandList>
          <CommandEmpty>Nothing matches. Try a page name.</CommandEmpty>
          <CommandGroup heading="Go to">
            {pages.map((p) => (
              <CommandItem key={p.id} value={`Go to ${p.title}`} keywords={p.keywords} onSelect={run(() => navigate(p.id))}>
                <p.icon /> Go to {p.title}
                <ArrowRight className="ms-auto opacity-40" />
              </CommandItem>
            ))}
            {pluginPages.map((p) => (
              <CommandItem key={p.key} value={`Go to ${p.title}`} keywords={["plugin", p.plugin]} onSelect={run(() => navigate(p.key))}>
                <PuzzlePiece /> Go to {p.title}
                <CommandShortcut>plugin</CommandShortcut>
              </CommandItem>
            ))}
          </CommandGroup>
          {groups.map(([heading, list]) => (
            <CommandGroup key={heading} heading={heading}>
              {list.map((c) => (
                <CommandItem key={c.id} value={c.title} keywords={c.keywords} onSelect={run(c.run)}>
                  {c.icon ? <c.icon /> : null}
                  {c.title}
                  {c.shortcut ? <CommandShortcut>{c.shortcut}</CommandShortcut> : null}
                </CommandItem>
              ))}
            </CommandGroup>
          ))}
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
          <CommandGroup heading="Plugins">
            <CommandItem value="Reload plugins" keywords={["refresh"]} onSelect={run(() => void refreshPlugins())}>
              <ArrowsClockwise /> Reload plugins
            </CommandItem>
          </CommandGroup>
        </CommandList>
      </Command>
    </CommandDialog>
  );
}

/** Opens the palette from anywhere (the sidebar's search button). */
export function openPalette() {
  dispatchEvent(new KeyboardEvent("keydown", { key: "k", metaKey: true }));
}
