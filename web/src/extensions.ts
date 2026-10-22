// Where a page can add to a built-in page without replacing it. Both are
// reached through the host (`host.addSection`, `host.addTab`), live while the
// plugin is loaded, and go away when it is switched off or removed.
import { useSyncExternalStore, type ComponentType } from "react";

export interface SettingsSectionExt {
  id: string;
  title: string;
  summary?: string;
  group?: string;
  keywords?: string[];
  component: ComponentType;
}

export interface TaskTabExt {
  id: string;
  title: string;
  /** Receives the task (the daemon's snapshot row) as `task`. */
  component: ComponentType<{ task: any }>;
}

function store<T extends { id: string }>() {
  let items: T[] = [];
  const listeners = new Set<() => void>();
  const emit = () => listeners.forEach((l) => l());
  return {
    add(item: T): () => void {
      items = [...items.filter((i) => i.id !== item.id), item];
      emit();
      return () => {
        if (!items.includes(item)) return;
        items = items.filter((i) => i !== item);
        emit();
      };
    },
    use(): T[] {
      return useSyncExternalStore((l) => { listeners.add(l); return () => listeners.delete(l); }, () => items);
    },
  };
}

const sections = store<SettingsSectionExt>();
const tabs = store<TaskTabExt>();

/** Adds a section to Settings (its sidebar sub-navigation and its pill bar). Returns the remover. */
export const addSettingsSection = (section: SettingsSectionExt) => sections.add({ ...section, id: `x-${section.id}` });
/** Adds a tab to the task sheet. Returns the remover. */
export const addTaskTab = (tab: TaskTabExt) => tabs.add({ ...tab, id: `x-${tab.id}` });

export const useSettingsSections = sections.use;
export const useTaskTabs = tabs.use;
