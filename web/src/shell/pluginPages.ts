// What the plugins' pages are, told to the daemon (`ReportPluginPages`).
import { useEffect } from "react";
import { request } from "@/api";
import { usePages } from "@/registry";
import { usePlugins } from "@/plugins";

/**
 * Tells the daemon which pages the plugins loaded in this window registered (`ReportPluginPages`), so
 * the Rune client can list them under More without a web window of its own. Once plugins are listed,
 * and again when the set changes.
 */
export function useReportPluginPages() {
  const { listed } = usePlugins();
  const pages = usePages().filter((p) => p.source === "user" && p.plugin);
  const key = JSON.stringify(pages.map((p) => [p.plugin, p.id, p.title]));
  useEffect(() => {
    if (!listed) return;
    const timer = setTimeout(() => {
      request({ ReportPluginPages: { pages: pages.map((p) => ({ plugin: p.plugin, id: p.id, title: p.title })) } }).catch(() => {});
    }, 1200);
    return () => clearTimeout(timer);
  }, [listed, key]); // eslint-disable-line react-hooks/exhaustive-deps
}
