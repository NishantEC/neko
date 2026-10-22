// Actions a page contributes to the ⌘K palette while it is mounted.
//
//   useEffect(() => registerCommands([{ id: "x", title: "Do x", run }]), []);
//
// The palette also lists "Go to <page>" for every page in pages/index.ts and
// every plugin page; those need no registration.
import { useSyncExternalStore } from "react";
import type { Icon } from "@/ui/icons";

export interface PaletteCommand {
  id: string;
  title: string;
  /** Extra words that should find it. */
  keywords?: string[];
  icon?: Icon;
  /** Shown on the right, e.g. "⌘↵". */
  shortcut?: string;
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

export function useCommands(): PaletteCommand[] {
  return useSyncExternalStore(
    (listener) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    () => commands,
  );
}
