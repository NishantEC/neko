// `host.storage`: the small, durable place a page keeps its own data. Values
// are JSON, kept by the daemon as files under `<data dir>/plugin-data/<id>/`
// (one namespace per page or plugin), so they survive a different browser, a
// cleared cache and a reinstall of the app. Every call goes through the same
// authorised bridge as the rest of the app; the daemon bounds a value at 1 MiB
// and a namespace at 256 keys.
//
//   await host.storage.set("weights", [71.5, 71.2]);
//   const weights = await host.storage.get("weights", []);   // the fallback when unset
//   await host.storage.delete("weights");
//   await host.storage.list();                               // ["weights", …]

export interface PageStorage {
  /** The stored value, or `fallback` (default `undefined`) when the key was never set. */
  get<T = unknown>(key: string, fallback?: T): Promise<T>;
  /** Stores any JSON-serialisable value under `key`. */
  set(key: string, value: unknown): Promise<void>;
  delete(key: string): Promise<void>;
  /** Every key in this namespace, sorted. */
  list(): Promise<string[]>;
}

const KEY = /^[A-Za-z0-9_-][A-Za-z0-9._-]{0,127}$/;
const NAMESPACE = /^[A-Za-z0-9_-]{1,64}$/;

function check(key: string) {
  if (!KEY.test(key)) throw new Error(`Storage keys are letters, digits, "." "_" "-" (max 128, no leading dot); got “${key}”.`);
}

async function call(method: string, path: string, body?: string): Promise<Response> {
  const response = await fetch(`/api/storage/${path}`, { method, body, headers: body === undefined ? undefined : { "content-type": "application/json" } });
  if (response.status === 401 || response.status === 403) throw new Error("This page is not signed in to the Neko daemon.");
  if (response.status === 413) throw new Error("That value is too big to store (limit: 1 MiB a value, 256 keys).");
  return response;
}

/** Storage for one namespace (a plugin's folder name, or `_neko` for the app's own pages). */
export function storageFor(namespace: string): PageStorage {
  if (!NAMESPACE.test(namespace)) throw new Error(`“${namespace}” is not a valid storage namespace.`);
  const base = encodeURIComponent(namespace);
  return {
    async get<T>(key: string, fallback?: T) {
      check(key);
      const response = await call("GET", `${base}/${key}`);
      if (response.status === 404) return fallback as T;
      if (!response.ok) throw new Error(`The daemon answered ${response.status}.`);
      return (await response.json()) as T;
    },
    async set(key, value) {
      check(key);
      const text = JSON.stringify(value);
      if (text === undefined) throw new Error("That value cannot be stored as JSON.");
      const response = await call("PUT", `${base}/${key}`, text);
      if (!response.ok) throw new Error(`The daemon answered ${response.status}.`);
    },
    async delete(key) {
      check(key);
      const response = await call("DELETE", `${base}/${key}`);
      if (!response.ok && response.status !== 404) throw new Error(`The daemon answered ${response.status}.`);
    },
    async list() {
      const response = await call("GET", base);
      if (response.status === 404) return [];
      if (!response.ok) throw new Error(`The daemon answered ${response.status}.`);
      return ((await response.json()).keys as { key: string }[]).map((k) => k.key);
    },
  };
}

/** The app's own namespace (what built-in pages and the shell keep here). */
export const appStorage = storageFor("_neko");
