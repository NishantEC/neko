// Settings: everything that configures Neko, in sections, with search that
// finds both sections and individual settings, jumps to one and lights it.
// ⌘F (or "Find a setting" in ⌘K) focuses the search field.
import { Suspense, useCallback, useEffect, useRef, useState } from "react";
import { Input } from "@/ui/input";
import { Kbd } from "@/ui/kbd";
import { Skeleton } from "@/ui/skeleton";
import { ArrowRight, Command as CommandIcon, Gear, ListChecks, MagnifyingGlass, X } from "@/ui/icons";
import { Panel } from "@/setup/kit";
import { cn } from "@/ui/lib/utils";
import { EmptyBlock, Gate, Row, SetupPage, registerCommands } from "@/setup/kit";
import { useSetupCommands } from "@/setup/commands";
import { takeSettingsRequest, usePendingSettingsRequest } from "@/setup/settingsNav";
import { SECTIONS, searchSettings, type SectionId } from "@/pages/setup/settings/registry";
import { GeneralSection } from "@/pages/setup/settings/General";
import { AiSection } from "@/pages/setup/settings/Ai";
import { RulebookSection } from "@/pages/setup/settings/Rulebook";
import { AboutSection, DiagnosticsSection, PermissionsSection, SearchSection } from "@/pages/setup/settings/System";

const STORAGE = "neko.web.settings.section";

const RENDER: Record<SectionId, () => React.ReactElement> = {
  general: () => <GeneralSection />,
  ai: () => <AiSection />,
  rulebook: () => <RulebookSection />,
  search: () => <SearchSection />,
  permissions: () => <PermissionsSection />,
  diagnostics: () => <DiagnosticsSection />,
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
  const [section, setSection] = useState<SectionId>(initialSection);
  const [query, setQuery] = useState("");
  const search = useRef<HTMLInputElement>(null);
  const pending = usePendingSettingsRequest();

  const open = useCallback((id: SectionId, target?: string) => {
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
    if (request.section) open(request.section as SectionId, request.target);
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
  const current = SECTIONS.find((s) => s.id === section)!;
  const groups = [...new Set(SECTIONS.map((s) => s.group))];
  const Content = RENDER[section];

  return (
    <>
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
          className="ps-9 pe-16"
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
        <div className="grid gap-8 md:grid-cols-[11.5rem_minmax(0,1fr)]">
          <nav aria-label="Settings sections" data-slot="settings-nav" className="flex flex-col gap-5 md:sticky md:top-0 md:self-start">
            {groups.map((group) => (
              <div key={group} className="flex flex-col gap-0.5">
                <span className="px-2.5 pb-1 font-mono text-[11px] uppercase tracking-[0.18em] text-muted-foreground">{group}</span>
                {SECTIONS.filter((s) => s.group === group).map((s) => (
                  <button
                    key={s.id}
                    type="button"
                    onClick={() => open(s.id)}
                    aria-current={s.id === section ? "page" : undefined}
                    data-active={s.id === section ? "" : undefined}
                    className={cn("rounded-lg px-2.5 py-1.5 text-start text-[14px] outline-none transition-colors hover:bg-foreground/[0.05] focus-visible:ring-2 focus-visible:ring-ring", s.id === section ? "bg-foreground/[0.07] font-medium text-foreground" : "text-muted-foreground")}
                  >
                    {s.title}
                  </button>
                ))}
              </div>
            ))}
          </nav>
          <div className="min-w-0" data-slot="settings-section" data-section={section}>
            <h2 className="sr-only">{current.title}: {current.summary}</h2>
            <Suspense fallback={<Skeleton className="h-48 w-full" />}><Content /></Suspense>
          </div>
        </div>
      )}
    </>
  );
}

export default function Settings() {
  useSetupCommands();
  return (
    <SetupPage
      eyebrow="System"
      title={<>Settings, <em>your way.</em></>}
      subtitle="Everything that configures Neko. Search finds sections and individual settings."
      width="max-w-[1040px]"
    >
      <Gate>{() => <SettingsBody />}</Gate>
    </SetupPage>
  );
}
