// The gallery's "Sidebar" section: ways to tidy the real sidebar
// (shell/AppSidebar.tsx), drawn side by side from static sample data so the
// owner can try each before picking one. The owner's decisions so far: no task
// list in the sidebar (the Tasks page keeps its needs-you count), and no
// two-level rail. Every option works on local state only: rows select, Setup
// folds, the collapse button shrinks it to an icon rail. Nothing navigates and
// nothing reaches the daemon; the workspace switcher at the top picks from
// sample workspaces (Workspaces is no longer a row: "Manage workspaces…"
// covers it); Search opens the real ⌘K and the theme controls
// set the real theme.
//
// The measures are hushbacks' (enform) UnifiedAppSidebar: 40px rows on a 24px
// radius with 4px between them, no group headings, one hairline, a footer
// that is one row plus the theme control.
import { createContext, useContext, useState, useSyncExternalStore, type ReactNode } from "react";
import { AnimatePresence, motion, useReducedMotion } from "motion/react";
import { CaretDown, CaretUpDown, Check, Command as CommandIcon, Desktop, Gear, House, MagnifyingGlass, Moon, SidebarSimple, Sparkle, Stack, Sun, type Icon } from "@/ui/icons";
import {
  DropdownMenu, DropdownMenuContent, DropdownMenuGroup, DropdownMenuItem, DropdownMenuLabel, DropdownMenuRadioGroup,
  DropdownMenuRadioItem, DropdownMenuSeparator, DropdownMenuTrigger,
} from "@/ui/dropdown-menu";
import { Kbd } from "@/ui/kbd";
import { cn } from "@/ui/lib/utils";
import { Mark } from "@/shell/AppSidebar";
import { openPalette } from "@/shell/CommandPalette";
import { getTheme, setTheme, subscribeTheme, type ThemeChoice } from "@/theme";
import { PAGE, SAMPLE_MACHINE, SAMPLE_NEEDS_YOU, SAMPLE_WORKSPACES, type SamplePage } from "@/pages/gallery/sidebar-sample";

// ---------------------------------------------------------------------------
// One piece of state per option: the selected page and whether it is
// collapsed to the icon rail. Parts read it from context.
// ---------------------------------------------------------------------------

type Mock = { sel: string; select: (id: string) => void; collapsed: boolean; toggle: () => void };
const MockContext = createContext<Mock>({ sel: "", select: () => {}, collapsed: false, toggle: () => {} });
const useMock = () => useContext(MockContext);

function useMockState(initial: string): Mock {
  const [sel, select] = useState(initial);
  const [collapsed, setCollapsed] = useState(false);
  return { sel, select, collapsed, toggle: () => setCollapsed((c) => !c) };
}

const FOCUS = "outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring/60";
const HOVER = "transition-colors duration-150 hover:bg-foreground/[0.04] hover:text-foreground";
const SCROLL = "no-scrollbar flex min-h-0 flex-1 flex-col overflow-y-auto";
const EASE: [number, number, number, number] = [0.16, 1, 0.3, 1];

function Frame({ mock, children }: { mock: Mock; children: ReactNode }) {
  return (
    <MockContext.Provider value={mock}>
      <div
        data-slot="sidebar-option-frame"
        data-collapsed={mock.collapsed}
        style={{ width: mock.collapsed ? 64 : "100%" }}
        className={cn(
          "flex h-[800px] flex-col gap-2 overflow-hidden whitespace-nowrap rounded-2xl border border-border bg-sidebar pt-3 pb-3 shadow-(--sky-drop)",
          "transition-[width] duration-300 ease-[cubic-bezier(0.16,1,0.3,1)] motion-reduce:transition-none",
          mock.collapsed ? "px-2" : "px-3",
        )}
      >
        {children}
      </div>
    </MockContext.Provider>
  );
}

function CollapseButton() {
  const { collapsed, toggle } = useMock();
  return (
    <button type="button" onClick={toggle} aria-label={collapsed ? "Expand sidebar" : "Collapse sidebar"} aria-expanded={!collapsed} className={cn("flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground", HOVER, FOCUS)}>
      <SidebarSimple size={16} />
    </button>
  );
}

const MENU_ROW = "flex h-8 cursor-pointer items-center gap-2.5 rounded-md px-2 text-sm";

function WorkspaceAvatar({ name }: { name: string }) {
  return (
    <span aria-hidden className="sky-pebble flex size-5 shrink-0 items-center justify-center rounded-full text-[11px] font-semibold">
      {name.trim().charAt(0).toUpperCase()}
    </span>
  );
}

/** hushbacks' WorkspaceSwitcher: the current workspace's name and a chevron; the menu picks one (local state). */
function WorkspaceSwitcher() {
  const { select } = useMock();
  const [id, setId] = useState(SAMPLE_WORKSPACES[0].id);
  const name = SAMPLE_WORKSPACES.find((w) => w.id === id)?.name ?? "All workspaces";
  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        data-slot="mock-workspace-switcher"
        aria-label={`Workspace: ${name}`}
        className={cn("flex h-8 w-full min-w-0 items-center gap-2 rounded-[12px] px-1.5 text-start text-sm font-semibold text-foreground data-[popup-open]:bg-foreground/[0.06]", HOVER, FOCUS)}
      >
        <span className="min-w-0 flex-1 truncate">{name}</span>
        <CaretUpDown size={14} weight="bold" className="shrink-0 text-muted-foreground" />
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start" sideOffset={6} className="w-60 p-1">
        <DropdownMenuGroup>
          <DropdownMenuLabel className="px-2 py-1.5 text-xs font-normal text-muted-foreground">Workspaces</DropdownMenuLabel>
          {SAMPLE_WORKSPACES.map((w) => (
            <DropdownMenuItem key={w.id} onClick={() => setId(w.id)} className={MENU_ROW}>
              <WorkspaceAvatar name={w.name} />
              <span className="min-w-0 flex-1 truncate">{w.name}</span>
              {w.id === id ? <Check size={14} weight="bold" className="shrink-0" /> : null}
            </DropdownMenuItem>
          ))}
          <DropdownMenuItem onClick={() => setId("")} className={MENU_ROW}>
            <Stack size={15} className="text-muted-foreground" />
            <span className="min-w-0 flex-1 truncate">All workspaces</span>
            {id === "" ? <Check size={14} weight="bold" className="shrink-0" /> : null}
          </DropdownMenuItem>
        </DropdownMenuGroup>
        <DropdownMenuSeparator className="my-1" />
        <DropdownMenuItem onClick={() => select(PAGE.workspaces.id)} className={MENU_ROW}>
          <Gear size={15} className="text-muted-foreground" />
          Manage workspaces…
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

function Brand() {
  const { collapsed } = useMock();
  const mark = (
    <span className="sky-pebble flex size-7 shrink-0 items-center justify-center rounded-[9px]">
      <Mark className="size-3.5" />
    </span>
  );
  if (collapsed) return <div className="flex shrink-0 flex-col items-center gap-2">{mark}<CollapseButton /></div>;
  return (
    <div className="flex h-9 shrink-0 items-center gap-1.5">
      {mark}
      <span aria-hidden className="text-lg font-light text-muted-foreground/40">/</span>
      <div className="min-w-0 flex-1"><WorkspaceSwitcher /></div>
      <CollapseButton />
    </div>
  );
}

/** Opens the real ⌘K palette. */
function Search() {
  const { collapsed } = useMock();
  return (
    <button type="button" onClick={openPalette} aria-label="Search" title="Search (⌘K)" className={cn("sky-control flex shrink-0 items-center gap-3 text-sm text-foreground/65", collapsed ? "justify-center" : "px-3", HOVER, FOCUS)}>
      <MagnifyingGlass size={16} className="shrink-0" />
      {collapsed ? null : <><span className="min-w-0 flex-1 truncate text-start font-medium">Search</span><Kbd className="bg-transparent"><CommandIcon size={11} />K</Kbd></>}
    </button>
  );
}

function Row({ page, icon, label, trailing, active, onClick, slot, small }: {
  page?: SamplePage;
  icon?: Icon;
  label?: string;
  trailing?: ReactNode;
  active?: boolean;
  onClick?: () => void;
  slot?: string;
  /** The quieter secondary row: 32px, 13px, a 14px icon. */
  small?: boolean;
}) {
  const { sel, select, collapsed } = useMock();
  const on = active ?? sel === page?.id;
  const RowIcon = icon ?? page?.icon ?? House;
  const text = label ?? page?.title ?? "";
  return (
    <button
      type="button"
      data-slot={slot}
      title={collapsed ? text : undefined}
      aria-current={on ? "page" : undefined}
      onClick={onClick ?? (() => select(page?.id ?? ""))}
      className={cn(
        "flex w-full shrink-0 items-center text-start",
        small ? "h-8 gap-2.5 rounded-[16px] text-[13px]" : "sky-control gap-3 text-sm",
        collapsed ? "justify-center" : "px-3",
        on ? "sky-pebble text-foreground" : cn(small ? "text-foreground/55" : "text-foreground/65", HOVER),
        FOCUS,
      )}
    >
      <RowIcon size={small ? 14 : 16} weight={on ? "fill" : "regular"} className="shrink-0" />
      {collapsed ? <span className="sr-only">{text}</span> : <><span className="min-w-0 flex-1 truncate font-medium">{text}</span>{trailing}</>}
    </button>
  );
}

/** The one number left: tasks waiting on you, tinted amber. */
function NeedsYou() {
  return <span aria-label={`${SAMPLE_NEEDS_YOU} need you`} className="shrink-0 text-xs font-medium tabular-nums text-warning-foreground">{SAMPLE_NEEDS_YOU}</span>;
}

function Hairline({ className }: { className?: string }) {
  return <div aria-hidden className={cn("mx-3 h-px shrink-0 bg-border/60", className)} />;
}

const THEMES: { value: ThemeChoice; label: string; icon: Icon }[] = [
  { value: "system", label: "System", icon: Desktop },
  { value: "light", label: "Light", icon: Sun },
  { value: "dark", label: "Dark", icon: Moon },
  { value: "glass", label: "Glass", icon: Sparkle },
];
const useTheme = () => useSyncExternalStore(subscribeTheme, getTheme);

/** The footer gear: a menu that selects pages in the mock and sets the real theme. */
function GearMenu({ items }: { items: SamplePage[] }) {
  const { select } = useMock();
  const theme = useTheme();
  return (
    <DropdownMenu>
      <DropdownMenuTrigger aria-label="Settings and theme" className={cn("flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground", HOVER, FOCUS)}>
        <Gear size={15} />
      </DropdownMenuTrigger>
      <DropdownMenuContent side="top" align="end" className="min-w-44">
        {items.map((p) => (
          <DropdownMenuItem key={p.id} onClick={() => select(p.id)}><p.icon size={14} /> {p.title}</DropdownMenuItem>
        ))}
        {items.length ? <DropdownMenuSeparator /> : null}
        <DropdownMenuGroup>
          <DropdownMenuLabel>Theme</DropdownMenuLabel>
          <DropdownMenuRadioGroup value={theme} onValueChange={(v) => setTheme(v as ThemeChoice)}>
            {THEMES.map(({ value, label, icon: ThemeIcon }) => (
              <DropdownMenuRadioItem key={value} value={value}><ThemeIcon size={14} /> {label}</DropdownMenuRadioItem>
            ))}
          </DropdownMenuRadioGroup>
        </DropdownMenuGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

/** One status line (opens Usage) plus the gear; collapsed, the dot over the gear. */
function StatusFooter({ menu }: { menu: SamplePage[] }) {
  const { collapsed, select } = useMock();
  return (
    <div className="flex shrink-0 flex-col gap-2 pt-1">
      <Hairline />
      <div className={cn("flex items-center gap-1", collapsed ? "flex-col" : "pe-1")}>
        <button type="button" onClick={() => select("usage")} title="Usage & machine" className={cn("flex min-w-0 flex-1 items-center gap-2 rounded-md px-3 py-1.5 text-start text-[12.5px] text-muted-foreground", collapsed && "flex-none px-2", HOVER, FOCUS)}>
          <span aria-hidden className="size-2 shrink-0 rounded-full bg-success" />
          <span className={collapsed ? "sr-only" : "truncate"}>{SAMPLE_MACHINE.title} · {SAMPLE_NEEDS_YOU} need you</span>
        </button>
        <GearMenu items={menu} />
      </div>
    </div>
  );
}

function ThemeSegment() {
  const theme = useTheme();
  return (
    <div role="group" aria-label="Theme" className="sky-segment">
      {THEMES.slice(0, 3).map(({ value, label, icon: ThemeIcon }) => (
        <button key={value} type="button" aria-pressed={theme === value} aria-label={`${label} theme`} onClick={() => setTheme(value)} className={FOCUS}>
          <ThemeIcon size={13} weight={theme === value ? "fill" : "regular"} />
        </button>
      ))}
    </div>
  );
}

/** hushbacks' SubReveal: Setup's pages open in place under it, with a rule on the left. */
function SetupFold({ pages, slot = "mock-setup" }: { pages: SamplePage[]; slot?: string }) {
  const mock = useMock();
  const [open, setOpen] = useState(false);
  const reduce = useReducedMotion() ?? false;
  const inSetup = pages.some((p) => p.id === mock.sel);
  return (
    <>
      <Row
        slot={slot}
        icon={Gear}
        label="Setup"
        active={inSetup}
        onClick={() => {
          if (mock.collapsed) { mock.toggle(); setOpen(true); } else setOpen((o) => !o);
        }}
        trailing={<CaretDown size={12} aria-hidden className={cn("text-muted-foreground transition-transform duration-200 motion-reduce:transition-none", !open && "-rotate-90")} />}
      />
      <AnimatePresence initial={false}>
        {open && !mock.collapsed ? (
          <motion.div
            key="setup"
            initial={reduce ? { opacity: 0 } : { height: 0, opacity: 0 }}
            animate={reduce ? { opacity: 1 } : { height: "auto", opacity: 1 }}
            exit={reduce ? { opacity: 0, transition: { duration: 0.1 } } : { height: 0, opacity: 0, transition: { duration: 0.22, ease: EASE } }}
            transition={reduce ? { duration: 0.12 } : { duration: 0.3, ease: EASE }}
            className="shrink-0 overflow-hidden"
          >
            <div className="mt-1 ml-[1.15rem] flex flex-col gap-0.5 border-l border-border/60 pl-3">
              {pages.map((p) => {
                const on = mock.sel === p.id;
                return (
                  <button
                    key={p.id}
                    type="button"
                    onClick={() => mock.select(p.id)}
                    aria-current={on ? "page" : undefined}
                    className={cn("flex w-full items-center rounded-md border py-1.5 pl-3 pr-2 text-start text-[13px]", on ? "border-border/40 bg-background/70 font-medium text-foreground" : cn("border-transparent text-foreground/55", HOVER), FOCUS)}
                  >
                    <span className="truncate">{p.title}</span>
                  </button>
                );
              })}
            </div>
          </motion.div>
        ) : null}
      </AnimatePresence>
    </>
  );
}

// ---------------------------------------------------------------------------
// The options
// ---------------------------------------------------------------------------

/** A. Every page in one quiet list, work first; Settings and Usage behind the gear. */
function OptionQuiet() {
  const mock = useMockState(PAGE.home.id);
  return (
    <Frame mock={mock}>
      <Brand />
      <Search />
      <Hairline className="my-2" />
      <div className={cn(SCROLL, "gap-1")}>
        <Row page={PAGE.home} />
        <Row page={PAGE.tasks} trailing={<NeedsYou />} />
        {[PAGE.prs, PAGE.tree, PAGE.watching, PAGE.schedules, PAGE.health, PAGE.memory, PAGE.profiles, PAGE.tools].map((p) => <Row key={p.id} page={p} />)}
      </div>
      <StatusFooter menu={[PAGE.settings, PAGE.usage]} />
    </Frame>
  );
}

/** D. hushbacks' own shape: a few daily rows, Setup folds the rest, the Mac row is the account row. */
function OptionSetupFolds() {
  const mock = useMockState(PAGE.home.id);
  const theme = useTheme();
  const ThemeIcon = (THEMES.find((t) => t.value === theme) ?? THEMES[0]).icon;
  return (
    <Frame mock={mock}>
      <Brand />
      <Search />
      <Hairline className="my-2" />
      <div className={cn(SCROLL, "gap-1")}>
        <Row page={PAGE.home} />
        <Row page={PAGE.tasks} trailing={<NeedsYou />} />
        <Row page={PAGE.prs} />
        <Row page={PAGE.tree} />
        <Row page={PAGE.watching} />
        <Row page={PAGE.health} />
        <SetupFold pages={[PAGE.schedules, PAGE.memory, PAGE.profiles, PAGE.tools, PAGE.settings]} />
      </div>
      <div className="flex shrink-0 flex-col gap-2 pt-1">
        <Hairline />
        {mock.collapsed ? (
          <div className="flex flex-col items-center gap-2">
            <button type="button" onClick={() => mock.select("usage")} aria-label="Usage & machine" className={cn("sky-pebble flex size-8 items-center justify-center rounded-full", FOCUS)}><Mark className="size-3.5" /></button>
            <button type="button" aria-label={`Theme: ${theme}`} onClick={() => setTheme(theme === "dark" ? "light" : theme === "light" ? "system" : "dark")} className={cn("flex size-7 items-center justify-center rounded-md text-muted-foreground", HOVER, FOCUS)}><ThemeIcon size={15} /></button>
          </div>
        ) : (
          <>
            <button type="button" onClick={() => mock.select("usage")} className={cn("flex items-center gap-3 rounded-xl px-2 py-1 text-start", mock.sel === "usage" ? "bg-foreground/[0.06]" : HOVER, FOCUS)}>
              <span className="sky-pebble flex size-8 shrink-0 items-center justify-center rounded-full"><Mark className="size-3.5" /></span>
              <span className="min-w-0 flex-1">
                <span className="block truncate text-[13px] font-semibold leading-tight text-foreground">{SAMPLE_MACHINE.title}</span>
                <span className="block truncate text-[12px] leading-tight text-muted-foreground">{SAMPLE_MACHINE.detail} · {SAMPLE_NEEDS_YOU} need you</span>
              </span>
            </button>
            <div className="px-1"><ThemeSegment /></div>
          </>
        )}
      </div>
    </Frame>
  );
}

/** E. Daily pages at the top; the pages you set up once sit quietly at the bottom, above one status line. */
function OptionTopAndBottom() {
  const mock = useMockState(PAGE.home.id);
  return (
    <Frame mock={mock}>
      <Brand />
      <Search />
      <Hairline className="my-2" />
      <div className={cn(SCROLL, "gap-1")}>
        <Row page={PAGE.home} />
        <Row page={PAGE.tasks} trailing={<NeedsYou />} />
        <Row page={PAGE.prs} />
        <Row page={PAGE.tree} />
        <Row page={PAGE.watching} />
        <Row page={PAGE.health} />
        <div data-slot="mock-setup-pages" className="mt-auto flex flex-col gap-0.5 pt-6">
          {[PAGE.schedules, PAGE.memory, PAGE.profiles, PAGE.tools].map((p) => <Row key={p.id} page={p} small />)}
        </div>
      </div>
      <StatusFooter menu={[PAGE.settings, PAGE.usage]} />
    </Frame>
  );
}

const OPTIONS: { id: string; name: string; caption: string; render: () => ReactNode }[] = [
  {
    id: "quiet",
    name: "A. Quiet",
    caption: "Every page in one list with no headings, work first. Only Tasks carries a number. Settings, usage and theme sit behind the footer’s gear.",
    render: () => <OptionQuiet />,
  },
  {
    id: "setup-folds",
    name: "D. Setup folds away",
    caption: "hushbacks’ shape: six daily rows, and the pages you set up once fold under Setup, opening in place like enform’s Settings. The Mac row is the account row.",
    render: () => <OptionSetupFolds />,
  },
  {
    id: "top-and-bottom",
    name: "E. Work up top, setup below",
    caption: "Daily pages at the top; setup pages stay visible but smaller, pinned at the bottom above one status line and the gear. Nothing folds.",
    render: () => <OptionTopAndBottom />,
  },
];

export function SidebarOptions() {
  return (
    <div className="flex flex-wrap items-start gap-5">
      {OPTIONS.map((o) => (
        <figure key={o.id} data-sidebar-option={o.id} className="m-0 flex w-[252px] flex-col gap-3">
          {o.render()}
          <figcaption className="flex flex-col gap-1 px-1">
            <span className="text-sm font-semibold text-foreground">{o.name}</span>
            <span className="text-[13px] leading-snug text-muted-foreground">{o.caption}</span>
          </figcaption>
        </figure>
      ))}
    </div>
  );
}
