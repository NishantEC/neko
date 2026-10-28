// Settings: everything that configures Neko, in sections, with search that
// finds both sections and individual settings, jumps to one and lights it.
// ⌘F (or "Find a setting" in ⌘K) focuses the search field.
//
// The layout is hushbacks' settings route (apps/spa/src/routes/_app.settings.tsx
// @da20dfaeb): a breadcrumb bar, then one 52rem column of sections on the
// settings surface. The sections are a rail on this page, not in the app
// sidebar (owner, 2026-10-28: nothing in the sidebar expands): search at its
// top, then the sections by group, 36px rows with a soft active fill; arrow
// keys move between them. Below 900px the rail is a strip of pills under the
// search. The section is in the address (#/settings?section=ai), so links,
// reloads and ⌘K "Settings: …" land on it; the last one is remembered.
import { Suspense, useCallback, useEffect, useRef, useState } from "react";
import { Input } from "@/ui/input";
import { Kbd } from "@/ui/kbd";
import { Skeleton } from "@/ui/skeleton";
import { ArrowRight, Command as CommandIcon, Gear, ListChecks, MagnifyingGlass, X } from "@/ui/icons";
import { Panel } from "@/setup/kit";
import { cn } from "@/ui/lib/utils";
import { EmptyBlock, Gate, Row, registerCommands } from "@/setup/kit";
import { setHashParams, useHashParams } from "@/work/location";
import { useSetupCommands } from "@/setup/commands";
import { takeSettingsRequest, usePendingSettingsRequest } from "@/setup/settingsNav";
import "@/pages/setup/settings/settings-surface.css";
import { SECTIONS, searchSettings, type SectionId } from "@/pages/setup/settings/registry";
import { useSettingsSections } from "@/extensions";
import { GeneralSection } from "@/pages/setup/settings/General";
import { AiSection } from "@/pages/setup/settings/Ai";
import { RulebookSection } from "@/pages/setup/settings/Rulebook";
import { AboutSection, DiagnosticsSection, PermissionsSection, SearchSection, UpdatesSection } from "@/pages/setup/settings/System";

const STORAGE = "neko.web.settings.section";

const RENDER: Record<SectionId, () => React.ReactElement> = {
  general: () => <GeneralSection />,
  ai: () => <AiSection />,
  rulebook: () => <RulebookSection />,
  search: () => <SearchSection />,
  permissions: () => <PermissionsSection />,
  diagnostics: () => <DiagnosticsSection />,
  updates: () => <UpdatesSection />,
  about: () => <AboutSection />,
};

function storedSection(): string | null {
  try {
    return localStorage.getItem(STORAGE);
  } catch {
    return null; /* storage can be blocked */
  }
}

/** Scrolls to a setting once its section has mounted, and lights it for a moment. */
function reveal(target: string) {
  const started = Date.now();
  const attempt = () => {
    const el = document.getElementById(`setting-${target}`);
    if (!el) {
      if (Date.now() - started < 1500) setTimeout(attempt, 50);
      return;
    }
    el.scrollIntoView({ block: "center", behavior: "smooth" });
    el.setAttribute("data-highlighted", "");
    setTimeout(() => el.removeAttribute("data-highlighted"), 1800);
  };
  requestAnimationFrame(attempt);
}

function SettingsBody() {
  const params = useHashParams();
  const extras = useSettingsSections();
  // A section a page added (`host.addSection`) is listed after Neko's own, under
  // "Added"; one that went away falls back to General.
  const sections = [
    ...SECTIONS.map((s) => ({ id: s.id as string, title: s.title, group: s.group })),
    ...extras.map((s) => ({ id: s.id, title: s.title, group: "Added" })),
  ];
  const known = (id: string | null) => (id && sections.some((s) => s.id === id) ? id : null);
  const section = known(params.get("section")) ?? known(storedSection()) ?? "general";
  const [query, setQuery] = useState("");
  const search = useRef<HTMLInputElement>(null);
  const pending = usePendingSettingsRequest();

  const open = useCallback((id: string, target?: string) => {
    setHashParams("settings", { section: id });
    setQuery("");
    try { localStorage.setItem(STORAGE, id); } catch { /* ignore */ }
    if (target) reveal(target);
    else document.querySelector("[data-slot='setup-page']")?.scrollTo({ top: 0 });
  }, []);

  // Requests from ⌘K and other pages.
  useEffect(() => {
    if (!pending) return;
    const request = takeSettingsRequest();
    if (!request) return;
    if (request.section) open(request.section, request.target);
    if (request.focusSearch) setTimeout(() => search.current?.focus(), 30);
  }, [pending, open]);

  // ⌘F finds a setting instead of the browser's find.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key.toLowerCase() === "f" && (e.metaKey || e.ctrlKey) && !e.shiftKey && !e.altKey) {
        e.preventDefault();
        search.current?.focus();
        search.current?.select();
      }
    };
    addEventListener("keydown", onKey);
    return () => removeEventListener("keydown", onKey);
  }, []);

  useEffect(
    () => registerCommands([
      ...SECTIONS.map((s) => ({ id: `settings.open.${s.id}`, title: `Settings: ${s.title}`, keywords: s.keywords, icon: Gear, group: "Settings", run: () => open(s.id) })),
    ]),
    [open],
  );

  const hits = searchSettings(query);
  const extra = extras.find((s) => s.id === section);
  const current = SECTIONS.find((s) => s.id === section) ?? { title: extra?.title ?? "General", summary: extra?.summary ?? "" };
  const Content = extra ? extra.component : RENDER[section as SectionId];

  const searchBox = (
    <div className="relative">
      <MagnifyingGlass aria-hidden className="pointer-events-none absolute start-3 top-1/2 size-4 -translate-y-1/2 text-muted-foreground" />
      <Input
        ref={search}
        value={query}
        onChange={(e) => setQuery(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Escape") setQuery("");
          if (e.key === "Enter" && hits[0]) {
            const hit = hits[0];
            open(hit.section.id, hit.kind === "setting" ? hit.setting.id : undefined);
          }
        }}
        aria-label="Find a setting"
        placeholder="Find a setting"
        className="ps-9! pe-12!"
      />
      <span className="pointer-events-none absolute end-2.5 top-1/2 flex -translate-y-1/2 items-center gap-1">
        {query ? null : <Kbd className="bg-transparent"><CommandIcon size={11} />F</Kbd>}
      </span>
      {query ? (
        <button type="button" onClick={() => { setQuery(""); search.current?.focus(); }} aria-label="Clear search" className="absolute end-2 top-1/2 -translate-y-1/2 rounded p-1 text-muted-foreground outline-none hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring">
          <X size={14} />
        </button>
      ) : null}
    </div>
  );

  return (
    <div className="flex h-full w-full flex-1 flex-col overflow-hidden">
      <header className="hidden h-12 shrink-0 items-center border-b border-border px-6 md:flex">
        <nav aria-label="Breadcrumb" className="flex items-center gap-2 text-sm">
          <span className="text-muted-foreground">Settings</span>
          <span aria-hidden className="text-muted-foreground/50">
            /
          </span>
          <span aria-current="page" className="font-medium text-foreground">
            {query.trim() ? "Search" : current.title}
          </span>
        </nav>
      </header>
      <div className="flex min-h-0 flex-1 flex-col min-[900px]:flex-row">
        <aside
          data-slot="settings-rail"
          className="flex shrink-0 flex-col gap-3 border-b border-border px-4 pt-3 pb-2 min-[900px]:w-60 min-[900px]:gap-4 min-[900px]:overflow-y-auto min-[900px]:border-e min-[900px]:border-b-0 min-[900px]:px-3 min-[900px]:pt-6 min-[900px]:pb-6"
        >
          {searchBox}
          <SettingsRail current={query.trim() ? null : section} sections={sections} onOpen={open} />
        </aside>
        <div data-slot="setup-page" className="settings-surface min-h-0 flex-1 overflow-y-auto">
          <div className="mx-auto w-full max-w-[52rem] px-7 pt-6 pb-24">
            <div className="settings-stack flex flex-col">
              {query.trim() ? (
                <section aria-label="Search results" data-slot="settings-results" className="flex flex-col gap-3">
                  <p className="text-[13px] text-muted-foreground">{hits.length ? "Choose a setting to jump to it." : `No setting matches “${query.trim()}”.`}</p>
                  {hits.length ? (
                    <Panel><div className="divide-y divide-border/70">
                      {hits.map((hit) => (
                        <Row
                          key={hit.kind === "section" ? `s.${hit.section.id}` : `t.${hit.setting.id}`}
                          title={hit.kind === "section" ? hit.section.title : hit.setting.title}
                          description={hit.kind === "section" ? hit.section.summary : `${hit.section.title}`}
                          leading={hit.kind === "section" ? <ListChecks /> : <Gear />}
                          trailing={<ArrowRight aria-hidden className="text-muted-foreground" />}
                          onClick={() => open(hit.section.id, hit.kind === "setting" ? hit.setting.id : undefined)}
                          label={hit.kind === "section" ? `Open ${hit.section.title}` : `${hit.setting.title} in ${hit.section.title}`}
                        />
                      ))}
                    </div></Panel>
                  ) : <EmptyBlock icon={MagnifyingGlass} title="No matches" description="Try another word: a runtime, a limit, “dark”, “folders”." />}
                </section>
              ) : (
                <div className="min-w-0" data-slot="settings-section" data-section={section}>
                  <h2 className="sr-only">{current.title}: {current.summary}</h2>
                  <Suspense fallback={<Skeleton className="h-48 w-full" />}><Content /></Suspense>
                </div>
              )}
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

/**
 * The sections, as enform's settings nav: a rail of 36px rows grouped under
 * plain labels with a soft fill on the current one; below 900px, a strip of
 * pills. Up/Down (or Left/Right in the strip), Home and End move between them.
 */
function SettingsRail({ current, sections, onOpen }: { current: string | null; sections: { id: string; title: string; group: string }[]; onOpen: (id: string) => void }) {
  const groups = [...new Set(sections.map((s) => s.group))];
  const onKeyDown = (e: React.KeyboardEvent<HTMLElement>) => {
    const keys = ["ArrowDown", "ArrowUp", "ArrowRight", "ArrowLeft", "Home", "End"];
    if (!keys.includes(e.key)) return;
    const buttons = [...e.currentTarget.querySelectorAll<HTMLButtonElement>("button[data-section-id]")];
    const at = buttons.indexOf(document.activeElement as HTMLButtonElement);
    if (at < 0) return;
    e.preventDefault();
    const next =
      e.key === "Home" ? 0
      : e.key === "End" ? buttons.length - 1
      : e.key === "ArrowDown" || e.key === "ArrowRight" ? Math.min(buttons.length - 1, at + 1)
      : Math.max(0, at - 1);
    buttons[next]?.focus();
  };
  return (
    <nav
      aria-label="Settings sections"
      data-slot="settings-nav"
      onKeyDown={onKeyDown}
      className="no-scrollbar -mx-1 flex gap-1 overflow-x-auto px-1 pb-1 min-[900px]:mx-0 min-[900px]:flex-col min-[900px]:gap-4 min-[900px]:overflow-visible min-[900px]:px-0 min-[900px]:pb-0"
    >
      {groups.map((group) => (
        <div key={group} role="group" aria-label={group} className="contents min-[900px]:flex min-[900px]:flex-col min-[900px]:gap-0.5">
          <span aria-hidden className="hidden px-3 pb-1 text-xs font-medium text-muted-foreground min-[900px]:block">{group}</span>
          {sections.filter((s) => s.group === group).map((entry) => {
            const active = entry.id === current;
            return (
              <button
                key={entry.id}
                type="button"
                data-section-id={entry.id}
                onClick={() => onOpen(entry.id)}
                aria-current={active ? "page" : undefined}
                className={cn(
                  "inline-flex h-9 shrink-0 items-center rounded-full px-4 text-start text-[13.5px] outline-none transition-colors duration-150 focus-visible:ring-2 focus-visible:ring-ring/60",
                  "min-[900px]:w-full min-[900px]:rounded-lg min-[900px]:px-3",
                  active ? "bg-foreground/[0.07] font-medium text-foreground" : "text-foreground/65 hover:bg-foreground/[0.04] hover:text-foreground",
                )}
              >
                <span className="truncate">{entry.title}</span>
              </button>
            );
          })}
        </div>
      ))}
    </nav>
  );
}

export default function Settings() {
  useSetupCommands();
  return (
    <Gate>{() => <SettingsBody />}</Gate>
  );
}
