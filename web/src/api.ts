// Everything the app knows about the daemon goes through here: one request
// function (the same JSON the Unix socket carries, via POST /api/request), a
// workbench store kept current by the held `WorkbenchIfChanged` request, and
// the daemon's event stream. Plugins receive `request` and nothing richer.

// deno-lint-ignore no-explicit-any
export type Json = any;

export class Unauthorized extends Error {
  constructor() {
    super("This page is not signed in to the Neko daemon.");
  }
}

/** One request, one response, exactly as `neko-protocol` defines them. */
export async function request(body: unknown, signal?: AbortSignal): Promise<Json> {
  const response = await fetch("/api/request", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body),
    signal,
  });
  if (response.status === 401 || response.status === 403) throw new Unauthorized();
  if (!response.ok) throw new Error(`The daemon answered ${response.status}.`);
  const json = await response.json();
  if (json && typeof json === "object" && "Error" in json) throw new Error(json.Error.message);
  return json;
}

/** `Workbench` commands: `command("SendMessage", { text })`, or a bare name. */
export function command(name: string, args?: Record<string, unknown>) {
  return request({ Workbench: args ? { [name]: args } : name });
}

// ---------------------------------------------------------------------------
// Workbench store
// ---------------------------------------------------------------------------

export interface Store {
  snapshot: Json | null;
  connected: boolean;
  unauthorized: boolean;
  error: string | null;
}

let state: Store = { snapshot: null, connected: false, unauthorized: false, error: null };
const listeners = new Set<() => void>();
let started = false;

function set(next: Partial<Store>) {
  state = { ...state, ...next };
  listeners.forEach((l) => l());
}

export function subscribe(listener: () => void) {
  listeners.add(listener);
  start();
  return () => listeners.delete(listener);
}

export const getStore = () => state;

/** A command that returned the new snapshot (every `Workbench` command does). */
export function accept(response: Json) {
  const snapshot = response?.Workbench;
  if (snapshot) {
    digest = snapshot.digest ?? digest;
    set({ snapshot, connected: true, error: null });
  }
}

let digest = "";

async function start() {
  if (started) return;
  started = true;
  let failures = 0;
  for (;;) {
    try {
      // First answer: the whole workbench. After that the daemon holds the
      // request until something changes (or 20 s pass), so a streamed word or
      // a finished step arrives as it happens.
      const asked = Date.now();
      const response = await request(
        digest
          ? { WorkbenchIfChanged: { digest, wait_ms: 20000 } }
          : { Workbench: "Snapshot" },
      );
      failures = 0;
      if (response.WorkbenchUnchanged) {
        if (!state.connected) set({ connected: true, error: null });
      } else {
        accept(response);
      }
      if (Date.now() - asked < 250) await sleep(250);
    } catch (error) {
      if (error instanceof Unauthorized) {
        set({ unauthorized: true, connected: false });
        return;
      }
      failures += 1;
      set({ connected: false, error: error instanceof Error ? error.message : String(error) });
      await sleep(Math.min(5000, 500 * failures));
    }
  }
}

const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

// ---------------------------------------------------------------------------
// Daemon events
// ---------------------------------------------------------------------------

type EventListener = (event: Json) => void;
const eventListeners = new Set<EventListener>();
let source: EventSource | null = null;

/** Every `Event` the daemon broadcasts, e.g. `{ AttentionChanged: { count } }`. */
export function onEvent(listener: EventListener) {
  eventListeners.add(listener);
  if (!source) {
    source = new EventSource("/api/events");
    source.addEventListener("daemon", (message) => {
      let event: Json;
      try {
        event = JSON.parse((message as MessageEvent).data);
      } catch {
        return;
      }
      eventListeners.forEach((l) => l(event));
    });
  }
  return () => eventListeners.delete(listener);
}

// ---------------------------------------------------------------------------
// Live reload: an agent that rebuilds the app (`pnpm watch`) sees it at once.
// ---------------------------------------------------------------------------

// Neko also rebuilds this app itself when one of its tasks lands a change
// (`WebUpdated`). Either way the page reloads only when the person is idle:
// never mid-typing, and never while a field holds an unsent draft.

let lastInput = 0;
const IDLE_MS = 5000;

/** Nothing typed for a few seconds, and no focused field holding text. */
export function idleForReload(now = Date.now(), active: Element | null = document.activeElement): boolean {
  if (now - lastInput < IDLE_MS) return false;
  const editable = active as HTMLInputElement | HTMLTextAreaElement | HTMLElement | null;
  if (!editable) return true;
  if (editable instanceof HTMLInputElement || editable instanceof HTMLTextAreaElement) return editable.value.trim() === "";
  if (editable.isContentEditable) return (editable.textContent ?? "").trim() === "";
  return true;
}

export function watchForNewBuild() {
  const bundle = (html: string) => html.match(/\/assets\/index-[\w-]+\.js/)?.[0] ?? "";
  let current: string | null = null;
  let wanted = false;
  for (const type of ["keydown", "input", "pointerdown", "compositionupdate"]) {
    window.addEventListener(type, () => { lastInput = Date.now(); }, { capture: true, passive: true });
  }
  const want = () => {
    wanted = true;
    (window as any).__nekoReloadPending = true;
  };
  onEvent((event) => {
    if (event && typeof event === "object" && "WebUpdated" in event) want();
  });
  setInterval(() => {
    if (wanted && idleForReload()) location.reload();
  }, 1000);
  setInterval(async () => {
    try {
      const html = await (await fetch("/", { cache: "no-store" })).text();
      const now = bundle(html);
      if (current === null) current = now;
      else if (now && now !== current) want();
    } catch {
      /* the daemon is restarting; try again next tick */
    }
  }, 2000);
}
