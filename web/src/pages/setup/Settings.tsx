// Settings: everything that configures Neko, in sections, with search that
// finds both sections and individual settings, jumps to one and lights it.
// ⌘F (or "Find a setting" in ⌘K) focuses the search field.
//
// The layout is hushbacks' settings route (apps/spa/src/routes/_app.settings.tsx
// @da20dfaeb): a breadcrumb bar, then one 52rem column of sections on the
// settings surface. The section list lives in the sidebar under Settings (its
// SubReveal); with the sidebar collapsed, the bar shows the pill nav instead.
import { Suspense, useCallback, useEffect, useRef, useState } from "react";
import { Input } from "@/ui/input";
import { Kbd } from "@/ui/kbd";
import { Skeleton } from "@/ui/skeleton";
import { ArrowRight, Command as CommandIcon, Gear, ListChecks, MagnifyingGlass, X } from "@/ui/icons";
import { Panel } from "@/setup/kit";
import { cn } from "@/ui/lib/utils";
import { EmptyBlock, Gate, Row, registerCommands } from "@/setup/kit";
import { useSidebar } from "@/ui/sidebar";
import { useSetupCommands } from "@/setup/commands";
import { setCurrentSettingsSection, takeSettingsRequest, usePendingSettingsRequest } from "@/setup/settingsNav";
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

function initialSection(): SectionId {
  try {
    const stored = localStorage.getItem(STORAGE) as SectionId | null;
    if (stored && SECTIONS.some((s) => s.id === stored)) return stored;
  } catch { /* storage can be blocked */ }
  return "general";
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
  const [chosen, setSection] = useState<string>(initialSection);
  const extras = useSettingsSections();
  // A section a page added (`host.addSection`) is listed after Neko's own; one that went away falls back to General.
  const sections = [...SECTIONS.map((s) => ({ id: s.id as string, title: s.title })), ...extras.map((s) => ({ id: s.id, title: s.title }))];
  const section = sections.some((s) => s.id === chosen) ? chosen : "general";
  const [query, setQuery] = useState("");
  const search = useRef<HTMLInputElement>(null);
  const pending = usePendingSettingsRequest();

  const open = useCallback((id: string, target?: string) => {
    setSection(id);
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

  useEffect(() => {
    setCurrentSettingsSection(section);
    return () => setCurrentSettingsSection(null);
  }, [section]);

  const hits = searchSettings(query);
  const extra = extras.find((s) => s.id === section);
  const current = SECTIONS.find((s) => s.id === section) ?? { title: extra?.title ?? "General", summary: extra?.summary ?? "" };
  const Content = extra ? extra.component : RENDER[section as SectionId];

  return (
    <div className="flex h-full w-full flex-1 flex-col overflow-hidden">
      <header className="flex shrink-0 items-center border-b border-border px-4 py-1.5 md:h-12 md:px-6 md:py-0">
        <nav aria-label="Breadcrumb" className="hidden items-center gap-2 text-sm md:flex">
          <span className="text-muted-foreground">Settings</span>
          <span aria-hidden className="text-muted-foreground/50">
            /
          </span>
          <span aria-current="page" className="font-medium text-foreground">
            {query.trim() ? "Search" : current.title}
          </span>
        </nav>
        <SettingsPhoneNav current={section} sections={sections} onOpen={open} />
      </header>
      <div data-slot="setup-page" className="settings-surface min-h-0 flex-1 overflow-y-auto">
        <div className="mx-auto w-full max-w-[52rem] px-6 pt-8 pb-24">
          <div className="settings-stack flex flex-col">
            <div className="relative max-w-md">
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
                className="ps-9! pe-16!"
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
  );
}

/**
 * hushbacks' SettingsPhoneNav: the section pills in the bar, for when the
 * sidebar cannot show its sub-navigation (collapsed, or on a phone).
 */
function SettingsPhoneNav({ current, sections, onOpen }: { current: string; sections: { id: string; title: string }[]; onOpen: (id: string) => void }) {
  const { state, isMobile } = useSidebar();
  if (state !== "collapsed" && !isMobile) return null;
  return (
    <nav aria-label="Settings sections" data-slot="settings-nav" className="-mx-2 flex min-w-0 flex-1 gap-1 overflow-x-auto px-2 md:ms-6">
      {sections.map((entry) => {
        const active = entry.id === current;
        return (
          <button
            key={entry.id}
            type="button"
            onClick={() => onOpen(entry.id)}
            aria-current={active ? "page" : undefined}
            className={cn(
              "inline-flex h-11 shrink-0 items-center rounded-full px-4 text-sm outline-none focus-visible:ring-2 focus-visible:ring-ring md:h-9",
              active ? "sky-pebble font-medium text-foreground" : "text-muted-foreground hover:text-foreground",
            )}
          >
            {entry.title}
          </button>
        );
      })}
    </nav>
  );
}

export default function Settings() {
  useSetupCommands();
  return (
    <Gate>{() => <SettingsBody />}</Gate>
  );
}
