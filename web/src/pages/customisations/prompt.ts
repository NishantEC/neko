// What "Change Neko" tells Neko. One constant, shown to the person in the dialog
// before it is sent, so nothing is said on their behalf that they cannot read.
// {{REQUEST}}, {{PLUGINS_DIR}}, {{CSS_PATH}} and {{CHECKOUT}} are filled in at
// send time.
//
// It has to stay under the daemon's message limit (4096 bytes, request
// included); `changePrompt` is measured by `requestBudget`, and the dialog shows
// how much room is left for the request. The manifest format, the host API and
// the two routes (a page in the plugins folder, or the app's own source) are the
// contract in web/src/host.ts and docs/design/one-registry.md: change them together.
export const CHANGE_NEKO_PROMPT = `Open a task that changes Neko the way I describe.

What to change: {{REQUEST}}

Use the lighter route that does the job.

A. My own page, override or styles (live in seconds, no review, only mine). For a new page, a replacement or wrapper of a built-in page, an extra Settings section or task tab, or restyling. Write a folder {{PLUGINS_DIR}}/<name>/ (lowercase letters, digits, - or _):
manifest.json
{ "title": "Short title", "description": "One sentence", "version": "0.1.0", "entry": "index.js", "group": "main" }   (group: main, work or system)
index.js (an ES module; React and Neko's components come from host, so no imports and no bundler)
export default function register(host) {
  const { React, ui } = host;
  const h = React.createElement;
  function Page() {
    return h("div", { className: "mx-auto w-full max-w-3xl px-6 py-10 flex flex-col gap-6" },
      h(ui.PageHeader, { title: "Short title" }),
      h(ui.Panel, null, h(ui.Button, { onClick: () => ui.toast.success("Hi") }, "Save")));
  }
  host.registerPage({ id: "main", title: "Short title", icon: "Heart", component: Page });
}
- A new id is a page of its own in the sidebar group, marked "yours". A built-in's id (home, tasks, prs, tree, workspaces, watching, schedules, memory, profiles, tools, settings, usage) replaces it: registerPage({ id: "tasks", component }), or wraps it: registerPage({ id: "tasks", wrap: (Original) => function Wrapped(props) { return h(React.Fragment, null, h(Banner), h(Original, props)); } }). It shows as Customised and can be reset to the original.
- host.storage.get(key, fallback) / set / delete / list keep JSON on the daemon; use it, not localStorage. host.addSection({ id, title, component }) adds to Settings; host.addTab({ id, title, component }) adds a task tab (it gets { task }); host.registerCommands([{ id, title, run }]) adds to the command palette; host.navigation.navigate(route); host.request(...) talks to the daemon, the same JSON the app sends; call no other site.
- Styles: CSS in {{CSS_PATH}} applies live. Restyle with the app's variables (--background, --foreground, --primary, --card, --border, --radius), not new colours.

B. The app's own source in the neko checkout {{CHECKOUT}} (web/src for the UI), for what a page cannot reach: the chat, the shell, the daemon, or a fix everyone should get. Work on a branch as usual. A UI-only change that lands on main goes live by itself within minutes; the previous web UI is kept (web-live.prev) and Settings, Neko updates has "Roll back the web UI". Daemon or app changes wait for a reinstall.

Choose A for anything personal and reversible, B when it should become part of Neko. Build from host.ui (PageHeader, Panel, Button, Input, Textarea, Select*, Switch, Checkbox, Badge, StatusBadge, Tabs*, Table*, Dialog*, DropdownMenu*, Tooltip*, Progress, Skeleton, Spinner, Empty*, toast), icons from host.icons, never hand-styled elements. When done, open it in the browser, fix whatever breaks, then tell me where to find it.`;

export const MANIFEST_EXAMPLE = `{
  "title": "PR board",
  "description": "Open pull requests by stage",
  "version": "0.1.0",
  "entry": "index.js",
  "group": "work"
}`;

export interface ChangePlaces {
  /** `<data dir>/plugins` */
  pluginsDir: string;
  /** `<data dir>/custom.css` */
  cssPath: string;
  /** The neko source checkout, when Neko knows it (`self_update.checkout`). */
  checkout: string | null;
}

const LIMIT_BYTES = 4096;
const bytes = (text: string) => new TextEncoder().encode(text).length;
const NO_CHECKOUT = "(its path shows in Settings, Neko updates, once “Land on main” is on for Neko)";

function fill(template: string, request: string, places: ChangePlaces): string {
  // Replacer functions, so a $ in the request or a path is not read as a replacement pattern.
  return template
    .replaceAll("{{PLUGINS_DIR}}", () => places.pluginsDir)
    .replaceAll("{{CSS_PATH}}", () => places.cssPath)
    .replaceAll("{{CHECKOUT}}", () => places.checkout ?? NO_CHECKOUT)
    .replaceAll("{{REQUEST}}", () => request);
}

/** How many bytes the request itself may use once the rest of the prompt is in. */
export function requestBudget(places: ChangePlaces): number {
  return LIMIT_BYTES - bytes(fill(CHANGE_NEKO_PROMPT, "", places));
}

export function changePrompt(request: string, places: ChangePlaces): string {
  return fill(CHANGE_NEKO_PROMPT, request.trim(), places);
}
