// The work pages keep their state in the hash query, so a task is a link:
//   #/tasks?task=<id>            the task open beside the list
//   #/tasks?task=<id>&view=full  the same task as a full page
// The shared router ignores everything after "?", so the route stays "tasks".
import { useMemo, useSyncExternalStore } from "react";

function subscribe(listener: () => void) {
  addEventListener("hashchange", listener);
  return () => removeEventListener("hashchange", listener);
}

export function useHashParams(): URLSearchParams {
  const hash = useSyncExternalStore(subscribe, () => location.hash);
  return useMemo(() => new URLSearchParams(hash.split("?")[1] ?? ""), [hash]);
}

/** Changes the query of the current route; `null` removes a key. */
export function setHashParams(route: string, params: Record<string, string | null>) {
  const next = new URLSearchParams(location.hash.split("?")[1] ?? "");
  for (const [key, value] of Object.entries(params)) {
    if (value === null || value === "") next.delete(key);
    else next.set(key, value);
  }
  const query = next.toString();
  location.hash = `/${route}${query ? `?${query}` : ""}`;
}
