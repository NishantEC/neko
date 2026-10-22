// Asks the Settings page to open a section, jump to one setting, or focus its
// search field — from ⌘K, from another page, or from its own search results.
import { useSyncExternalStore } from "react";

export interface SettingsRequest {
  id: number;
  section?: string;
  /** The id of one setting to scroll to and highlight. */
  target?: string;
  focusSearch?: boolean;
}

let pending: SettingsRequest | null = null;
let counter = 0;
const listeners = new Set<() => void>();

export function requestSettings(request: Omit<SettingsRequest, "id">) {
  pending = { ...request, id: ++counter };
  listeners.forEach((l) => l());
}

export function takeSettingsRequest() {
  const value = pending;
  pending = null;
  return value;
}

export function usePendingSettingsRequest(): SettingsRequest | null {
  return useSyncExternalStore(
    (l) => { listeners.add(l); return () => listeners.delete(l); },
    () => pending,
  );
}

// The section the Settings page shows, so the sidebar's Settings sub-navigation
// (hushbacks' SubReveal) can mark it. The page writes it; the sidebar reads it and
// asks for another with `requestSettings({ section })`.
let current: string | null = null;
const currentListeners = new Set<() => void>();

export function setCurrentSettingsSection(section: string | null) {
  if (current === section) return;
  current = section;
  currentListeners.forEach((l) => l());
}

export function useCurrentSettingsSection(): string | null {
  return useSyncExternalStore(
    (l) => { currentListeners.add(l); return () => currentListeners.delete(l); },
    () => current,
  );
}
