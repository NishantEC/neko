import { useSyncExternalStore } from "react";
import { getStore, subscribe } from "./api";

/** The workbench snapshot and the connection's state, live. */
export function useWorkbench() {
  return useSyncExternalStore(subscribe, getStore);
}
