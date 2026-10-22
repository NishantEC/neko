// A tiny hash router. A route is the hash without its leading slash:
//
//   #/            -> "home"
//   #/ui          -> "ui"
//   #/pr-board:board  -> "pr-board:board"   (a user page: <plugin>:<page>)
//
// Pages (built-in and yours) register in registry.ts through the host. Old
// routes that moved keep working: #/plugins is now #/customisations.
import { useSyncExternalStore } from "react";

export const HOME = "home";

/** Routes that were renamed; the address is rewritten to the new name. */
const MOVED: Record<string, string> = { plugins: "customisations" };

function read(): string {
  const raw = location.hash.replace(/^#\/?/, "").split("?")[0];
  return raw === "" ? HOME : decodeURIComponent(raw);
}

function redirectMoved() {
  const route = read();
  const to = MOVED[route];
  if (to) history.replaceState(null, "", `${location.pathname}${location.search}${hrefFor(to)}`);
  if (to) dispatchEvent(new HashChangeEvent("hashchange"));
}
addEventListener("hashchange", redirectMoved);
redirectMoved();

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
