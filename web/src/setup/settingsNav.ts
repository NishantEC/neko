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
