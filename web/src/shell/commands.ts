// What a page can add to ⌘K while it is open, and to the `?` shortcut overlay.
//
//   // In a page component:
//   useRegisterCommands([
//     { id: "tasks.new", title: "New task", icon: Plus, shortcut: "⌘N", keywords: ["create"], run: newTask },
//     { id: "tasks.copy", title: "Copy this task's ID", group: "This task", run: () => copyText(id) },
//   ], [id]);
//
//   // Without a hook, e.g. from a plugin:
//   const remove = registerCommands([{ id: "x", title: "Do x", run }]);  // call remove() to take them back
//
// The palette itself already lists "Go to <page>" for every page in
// pages/index.ts and every plugin page, "Recent tasks", the theme switch, the
// shell's own actions (new task, stop all work, approve, reload plugins, the
// shortcut list) and everything registered here. `id` must be unique: a
// command registered again under the same id replaces the old one.
//
// Fields: `title` is what is shown and searched; `keywords` are extra words that
// find it; `group` is the palette heading (default "Actions"); `shortcut` is a
// hint shown on the right (it does NOT bind the key; use registerShortcuts for
// the overlay and your own key handler for the binding); `hint` is a quieter
// second line; `run` is called after the palette has closed and released focus.
import { useEffect, useSyncExternalStore } from "react";
import type { Icon } from "@/ui/icons";

export interface PaletteCommand {
  id: string;
  title: string;
  /** Extra words that should find it. */
  keywords?: string[];
  icon?: Icon;
  /** Shown on the right, e.g. "⌘↵". */
  shortcut?: string;
  /** A quieter line under the title. */
  hint?: string;
  group?: string;
  run: () => void;
}

let commands: PaletteCommand[] = [];
const listeners = new Set<() => void>();
const emit = () => listeners.forEach((l) => l());

/** Adds commands; returns the function that removes exactly these. */
export function registerCommands(list: PaletteCommand[]) {
  commands = [...commands.filter((c) => !list.some((n) => n.id === c.id)), ...list];
  emit();
  return () => {
    commands = commands.filter((c) => !list.includes(c));
    emit();
  };
}

/** `registerCommands` for the life of a component; re-registers when `deps` change. */
export function useRegisterCommands(list: PaletteCommand[], deps: unknown[]) {
  // eslint-disable-next-line react-hooks/exhaustive-deps
  useEffect(() => registerCommands(list), deps);
}

export function useCommands(): PaletteCommand[] {
  return useSyncExternalStore(
    (listener) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    () => commands,
  );
}
