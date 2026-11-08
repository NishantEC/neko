// The New tab (⌘T, `+`): every page, built-in and yours, to open in this tab.
// Picking one replaces the launcher (shell/tabs.ts); a page already open in
// another tab is focused there instead. Type to filter, Enter opens the first.
import { useMemo, useState } from "react";
import { MagnifyingGlass } from "@/ui/icons";
import { InputGroup, InputGroupAddon, InputGroupInput } from "@/ui/input-group";
import { usePages, type PageDef } from "@/registry";
import { hrefFor, navigate } from "@/router";
import { useTabs } from "@/shell/tabs";
import { PageContainer } from "@/shell/PageContainer";
import { useContextMenu } from "@/shell/context-menu";
import { openInNewTab } from "@/shell/tabs";
import { ArrowSquareOut } from "@/ui/icons";

const GROUPS: { id: PageDef["group"] | "yours"; title: string }[] = [
  { id: "main", title: "Daily" },
  { id: "work", title: "Work" },
  { id: "yours", title: "Yours" },
  { id: "system", title: "Set up" },
];

function PageLink({ page, open }: { page: PageDef; open: boolean }) {
  const menu = useContextMenu(() => [
    { label: "Open", run: () => navigate(page.id) },
    { label: "Open in a new tab", icon: ArrowSquareOut, run: () => openInNewTab(hrefFor(page.id)) },
  ]);
  return (
    <li>
      <a
        {...menu}
        href={hrefFor(page.id)}
        data-slot="launcher-page"
        className="flex h-10 items-center gap-3 rounded-[10px] px-3 text-[14px] text-foreground outline-none transition-colors hover:bg-foreground/[0.05] focus-visible:ring-2 focus-visible:ring-ring/70"
      >
        <page.icon aria-hidden size={16} className="shrink-0 text-muted-foreground" />
        <span className="min-w-0 flex-1 truncate">{page.title}</span>
        {open ? <span className="text-[12px] text-muted-foreground/70">Open</span> : null}
      </a>
    </li>
  );
}

export function Launcher() {
  const pages = usePages();
  const { tabs } = useTabs();
  const [query, setQuery] = useState("");
  const openKeys = new Set(tabs.map((t) => t.key));
  const q = query.trim().toLowerCase();
  const matches = useMemo(
    () => pages.filter((p) => !q || p.title.toLowerCase().includes(q) || p.keywords.some((k) => k.toLowerCase().includes(q))),
    [pages, q],
  );
  const groupOf = (p: PageDef) => (p.source === "user" ? "yours" : p.group);

  return (
    <PageContainer data-slot="launcher" className="max-w-[44rem]">
      <h1 className="text-[20px] font-semibold text-foreground">New tab</h1>
      <InputGroup className="sky-well sky-control mt-5 border-transparent bg-transparent shadow-none">
        <InputGroupAddon><MagnifyingGlass aria-hidden /></InputGroupAddon>
        <InputGroupInput
          autoFocus
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="Find a page"
          aria-label="Find a page"
          onKeyDown={(e) => { if (e.key === "Enter" && matches[0]) navigate(matches[0].id); }}
        />
      </InputGroup>
      {matches.length === 0 ? <p className="mt-6 text-[13.5px] text-muted-foreground">No page matches “{query}”.</p> : null}
      <div className="mt-6 grid gap-6 sm:grid-cols-2">
        {GROUPS.map((group) => {
          const list = matches.filter((p) => groupOf(p) === group.id);
          if (!list.length) return null;
          return (
            <section key={group.id} aria-label={group.title}>
              <h2 className="mb-1 px-3 text-[12.5px] font-medium text-muted-foreground">{group.title}</h2>
              <ul className="flex flex-col">
                {list.map((p) => <PageLink key={p.id} page={p} open={openKeys.has(p.id)} />)}
              </ul>
            </section>
          );
        })}
      </div>
    </PageContainer>
  );
}
