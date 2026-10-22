// ⌘K actions every setup page offers while it is open, so "Open Rulebook" or
// "Check all watches now" work from any of them.
import { useEffect } from "react";
import { toast } from "@/ui/sonner";
import { accept, getStore } from "@/api";
import { Eye, Gear, ListChecks } from "@/ui/icons";
import { navigate } from "@/router";
import { registerCommands } from "@/shell/commands";
import { arr, mcp } from "@/setup/lib";
import { requestSettings } from "@/setup/settingsNav";

export async function checkAllWatches(): Promise<number> {
  const watches = arr(getStore().snapshot?.mcp?.responsibilities).filter((r) => r.enabled);
  let count = 0;
  for (const w of watches) {
    try {
      accept(await mcp("Wake", { responsibility_id: w.id }));
      count += 1;
    } catch (e) {
      toast.error(e instanceof Error ? e.message : String(e));
    }
  }
  return count;
}

export function useSetupCommands() {
  useEffect(
    () =>
      registerCommands([
        {
          id: "setup.rulebook", title: "Open Rulebook", keywords: ["rules", "standards", "agents", "instructions"], icon: ListChecks, group: "Setup",
          run: () => { requestSettings({ section: "rulebook" }); navigate("settings"); },
        },
        {
          id: "setup.checkall", title: "Check all watches now", keywords: ["sync", "watching", "refresh"], icon: Eye, group: "Setup",
          run: () => {
            void checkAllWatches().then((n) => toast.success(n ? `Asked ${n} ${n === 1 ? "watch" : "watches"} to check now.` : "No active watches to check."));
          },
        },
        {
          id: "setup.settings.find", title: "Find a setting", keywords: ["search", "preferences"], icon: Gear, group: "Setup",
          run: () => { requestSettings({ focusSearch: true }); navigate("settings"); },
        },
      ]),
    [],
  );
}
