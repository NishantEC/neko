// What "Build a plugin with Neko" tells Neko. One constant, shown to the person
// in the dialog before it is sent, so nothing is said on their behalf that they
// cannot read. `{{PLUGINS_DIR}}` and `{{REQUEST}}` are filled in at send time.
//
// It has to stay under the daemon's message limit (4096 bytes, request
// included); `buildPluginPrompt` checks, and the dialog shows how much room is
// left for the request. The manifest format and a minimal example are
// the plugin contract in web/src/plugins.tsx: change them together.
export const BUILD_PLUGIN_PROMPT = `Open a task that builds a plugin for Neko's web UI.

What it should do: {{REQUEST}}

Write it as a folder in {{PLUGINS_DIR}}/<name>/ (name: lowercase letters, digits, - or _), with two files.

manifest.json
{ "title": "Short title", "description": "One sentence", "version": "0.1.0", "entry": "index.js" }

index.js (an ES module; React and Neko's own components come from the host, so no imports and no bundler)
export default function register(host) {
  const { React, request, registerPage, ui, icons } = host;
  const h = React.createElement;
  const { PageHeader, Panel, Button, Input, Badge } = ui;
  function Page() {
    return h("div", { className: "mx-auto w-full max-w-3xl px-6 py-10 flex flex-col gap-6" },
      h(PageHeader, { title: "Short title", subtitle: "One sentence" }),
      h(Panel, null, h(Button, { onClick: () => ui.toast.success("Hi") }, h(icons.Check), "Save")));
  }
  registerPage({ id: "main", title: "Short title", component: Page });
}

Build the page from host.ui, never from hand-styled elements: PageHeader, Panel, Button, Input, Textarea, Select*, Switch, Checkbox, Badge, StatusBadge, Tabs*, Table*, Dialog*, DropdownMenu*, Tooltip*, Progress, Skeleton, Spinner, Empty*, toast. Icons are host.icons.<Name> (Phosphor). Layout uses ordinary class names the app already ships (flex, gap-4, px-6, text-sm, text-muted-foreground, border, rounded-xl); for anything else use var(--foreground), var(--muted-foreground), var(--border), var(--card), var(--background).
Rules: talk to the daemon with request(...), the same JSON the app sends; call no other site. The page shows up in the sidebar by itself a few seconds after the files are saved. Open it in the browser, fix whatever breaks, then tell me what you built.`;

export const MANIFEST_EXAMPLE = `{
  "title": "PR board",
  "description": "Open pull requests by stage",
  "version": "0.1.0",
  "entry": "index.js"
}`;

const LIMIT_BYTES = 4096;
const bytes = (text: string) => new TextEncoder().encode(text).length;

/** How many bytes the request itself may use once the rest of the prompt is in. */
export function requestBudget(pluginsDir: string): number {
  return LIMIT_BYTES - bytes(BUILD_PLUGIN_PROMPT.replace("{{PLUGINS_DIR}}", pluginsDir).replace("{{REQUEST}}", ""));
}

export function buildPluginPrompt(request: string, pluginsDir: string): string {
  // Replacer functions, so a $ in the request is not read as a replacement pattern.
  return BUILD_PLUGIN_PROMPT.replace("{{PLUGINS_DIR}}", () => pluginsDir).replace("{{REQUEST}}", () => request.trim());
}
