// What a plugin may use to look like Neko: the same components the app's own
// pages are built from, handed over as `host.ui` so a plugin needs no bundler
// and ships no copy of them. Names are the components' own (Button, Input,
// Panel, Badge, Tabs, …); phosphor icons come as `host.icons`.
//
// Add a component here and it appears for every plugin; the build-a-plugin
// prompt (pages/plugins/prompt.ts) lists the ones agents should reach for.
import * as button from "@/ui/button";
import * as badge from "@/ui/badge";
import * as statusBadge from "@/ui/status-badge";
import * as card from "@/ui/card";
import * as input from "@/ui/input";
import * as textarea from "@/ui/textarea";
import * as label from "@/ui/label";
import * as field from "@/ui/field";
import * as select from "@/ui/select";
import * as switchUi from "@/ui/switch";
import * as checkbox from "@/ui/checkbox";
import * as tabs from "@/ui/tabs";
import * as table from "@/ui/table";
import * as dialog from "@/ui/dialog";
import * as alertDialog from "@/ui/alert-dialog";
import * as dropdown from "@/ui/dropdown-menu";
import * as popover from "@/ui/popover";
import * as tooltip from "@/ui/tooltip";
import * as progress from "@/ui/progress";
import * as skeleton from "@/ui/skeleton";
import * as spinner from "@/ui/spinner";
import * as separator from "@/ui/separator";
import * as empty from "@/ui/empty";
import * as kbd from "@/ui/kbd";
import * as alert from "@/ui/alert";
import * as scrollArea from "@/ui/scroll-area";
import * as editorial from "@/editorial";
import { toast } from "@/ui/sonner";
import { cn } from "@/ui/lib/utils";

const modules = [button, badge, statusBadge, card, input, textarea, label, field, select, switchUi, checkbox, tabs, table, dialog, alertDialog, dropdown, popover, tooltip, progress, skeleton, spinner, separator, empty, kbd, alert, scrollArea, editorial];

/** Components only: PascalCase exports, no variant helpers or primitives' internals. */
export const pluginUi: Record<string, unknown> = { toast, cn };
for (const module of modules) {
  for (const [name, value] of Object.entries(module)) {
    if (/^[A-Z]/.test(name) && !name.endsWith("Primitive") && !(name in pluginUi)) pluginUi[name] = value;
  }
}

/**
 * Every icon in `ui/icons`, by name. Filled by `loadPluginIcons()` before a
 * plugin's module runs (`plugins.tsx`): the full set is ~450 KB the app's own
 * first paint does not need, so it lives in its own chunk (`ui/allIcons.ts`,
 * only ever imported dynamically). The object is the same one throughout, so
 * a host handed out earlier sees the icons once they are in.
 */
export const pluginIcons: Record<string, unknown> = {};
let iconsLoading: Promise<void> | null = null;
export function loadPluginIcons(): Promise<void> {
  return (iconsLoading ??= import("@/ui/allIcons").then((icons) => {
    for (const [name, value] of Object.entries(icons)) if (/^[A-Z]/.test(name)) pluginIcons[name] = value;
  }));
}
