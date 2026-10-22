// A string setting kept in localStorage, shared across tabs of the app.
import { useCallback, useSyncExternalStore } from "react";

export function useLocal(key: string, fallback: string): [string, (value: string) => void] {
  const value = useSyncExternalStore(
    (listener) => {
      addEventListener("storage", listener);
      addEventListener(`local:${key}`, listener);
      return () => {
        removeEventListener("storage", listener);
        removeEventListener(`local:${key}`, listener);
      };
    },
    () => {
      try {
        return localStorage.getItem(key) ?? fallback;
      } catch {
        return fallback;
      }
    },
  );
  const set = useCallback(
    (next: string) => {
      try {
        localStorage.setItem(key, next);
      } catch {
        /* private mode: the choice lasts until reload */
      }
      dispatchEvent(new Event(`local:${key}`));
    },
    [key],
  );
  return [value, set];
}
