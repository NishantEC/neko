// A tiny hash router. A route is the hash without its leading slash:
//
//   #/            -> "home"
//   #/ui          -> "ui"
//   #/pr-board:board  -> "pr-board:board"   (a plugin page: <plugin>:<page>)
//
// Pages register in pages/index.ts; plugin pages come from plugins.tsx.
import { useSyncExternalStore } from "react";

export const HOME = "home";

function read(): string {
  const raw = location.hash.replace(/^#\/?/, "").split("?")[0];
  return raw === "" ? HOME : decodeURIComponent(raw);
}

export function hrefFor(route: string): string {
  return route === HOME ? "#/" : `#/${encodeURIComponent(route).replace(/%3A/gi, ":")}`;
}

export function navigate(route: string) {
  location.hash = hrefFor(route).slice(1);
}

function subscribe(listener: () => void) {
  addEventListener("hashchange", listener);
  return () => removeEventListener("hashchange", listener);
}

/** The current route, re-read on every hashchange. */
export function useRoute(): string {
  return useSyncExternalStore(subscribe, read);
}
