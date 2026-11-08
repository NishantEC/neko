// What this window shows, told to the daemon so an agent can check its work
// (`InspectApp`; docs/superpowers/specs/2026-11-08-everything-renders-design.md B1).
//
//   ReportApp { report }   debounced on every change, and once a minute while open
//
// Only the window the person used last (`isActivePage`, the same rule that
// decides which window obeys ShowArtifact) sends everything: its tabs and
// their titles as drawn, the route, every registered page, viewer and fence,
// how each plugin loaded, the artifacts drawn and where, the canvas and the
// theme. The others say only where they are, so the daemon can list them. A
// closing page says so. Nothing here is stored on disk, by the page or the daemon.
import { request } from "@/api";
import { getPages, subscribePages } from "@/registry";
import { getTabs, subscribeTabs } from "@/shell/tabs";
import { tabElementId } from "@/shell/TabStrip";
import { listFences, listViewers, subscribeViewers } from "@/canvas/viewers";
import { getCanvas, isActivePage, listOnScreen, subscribeCanvas, subscribeOnScreen } from "@/canvas/artifacts";
import { pluginReport, subscribePluginReport } from "@/plugins";
import { getThemeState, subscribeTheme } from "@/theme";

const PAGE_ID = `w${Math.random().toString(36).slice(2, 12)}`;
const DEBOUNCE_MS = 400;
const HEARTBEAT_MS = 60_000;

const route = () => location.hash.replace(/^#\/?/, "");

/** What this window reports now; `full` only when it is the one used last. */
export function appReport(full = isActivePage()) {
  const base = {
    page_id: PAGE_ID,
    full,
    closed: false,
    focused: document.hasFocus(),
    visible: document.visibilityState === "visible",
    route: route(),
    title: document.title,
  };
  if (!full) return base;
  const { tabs, active } = getTabs();
  const canvas = getCanvas();
  const theme = getThemeState();
  return {
    ...base,
    tabs: tabs.map((t) => ({
      key: t.key,
      title: document.getElementById(tabElementId(t.id))?.textContent?.trim() ?? "",
      href: t.href,
      active: t.id === active,
      pinned: !!t.pinned,
    })),
    pages: getPages().map((p) => ({ id: p.id, title: p.title, plugin: p.plugin ?? null })),
    viewers: listViewers().map((v) => ({ id: v.id, title: v.title, kinds: v.kinds, priority: Math.trunc(v.priority), plugin: v.plugin ?? null })),
    fences: listFences().map((f) => ({ language: f.language, viewer: f.viewer ?? null, plugin: f.plugin ?? null })),
    plugins: pluginReport(),
    artifacts: listOnScreen().map((a) => ({ id: a.id, version: a.version, kind: a.kind, viewer: a.viewer, place: a.place })),
    canvas: canvas ? { id: canvas.id, version: canvas.version ?? 0, kind: "", viewer: null, place: "canvas" } : null,
    theme: { mode: theme.mode, appearance: theme.appearance, id: theme.id, name: theme.name },
  };
}

let started = false;

/** Called once by the shell; returns what stops it. */
export function startAppReports(): () => void {
  if (started) return () => {};
  started = true;
  let timer = 0;
  let last = "";
  let lastAt = 0;
  const send = () => {
    timer = 0;
    const report = appReport();
    const json = JSON.stringify(report);
    // Unchanged and recent: nothing to say (the heartbeat still goes out once a minute).
    if (json === last && Date.now() - lastAt < HEARTBEAT_MS - 1000) return;
    last = json;
    lastAt = Date.now();
    request({ ReportApp: { report } }).catch(() => {});
  };
  const schedule = () => {
    if (!timer) timer = window.setTimeout(send, DEBOUNCE_MS);
  };
  const closing = () => {
    // keepalive lets the request outlive the page.
    void fetch("/api/request", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ ReportApp: { report: { page_id: PAGE_ID, closed: true } } }),
      keepalive: true,
    }).catch(() => {});
  };
  const stops = [
    subscribePages(schedule),
    subscribeTabs(schedule),
    subscribeViewers(schedule),
    subscribeCanvas(schedule),
    subscribeOnScreen(schedule),
    subscribePluginReport(schedule),
    subscribeTheme(schedule),
  ];
  // Tab titles arrive after the tab (an artifact's or task's name is read later).
  const strip = new MutationObserver(schedule);
  const watchStrip = () => {
    const list = document.querySelector("[role=tablist][aria-label='Open tabs']");
    if (list) strip.observe(list, { subtree: true, childList: true, characterData: true });
    return !!list;
  };
  const stripTimer = window.setInterval(() => { if (watchStrip()) clearInterval(stripTimer); }, 500);
  const onVisibility = () => schedule();
  addEventListener("hashchange", schedule);
  addEventListener("focus", schedule);
  addEventListener("blur", schedule);
  addEventListener("pagehide", closing);
  document.addEventListener("visibilitychange", onVisibility);
  const heartbeat = window.setInterval(send, HEARTBEAT_MS);
  schedule();
  return () => {
    started = false;
    stops.forEach((stop) => stop());
    strip.disconnect();
    clearInterval(stripTimer);
    clearInterval(heartbeat);
    clearTimeout(timer);
    removeEventListener("hashchange", schedule);
    removeEventListener("focus", schedule);
    removeEventListener("blur", schedule);
    removeEventListener("pagehide", closing);
    document.removeEventListener("visibilitychange", onVisibility);
  };
}
