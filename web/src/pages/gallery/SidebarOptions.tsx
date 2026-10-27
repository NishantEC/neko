// The gallery's "Sidebar" section: four ways to tidy the real sidebar
// (shell/AppSidebar.tsx), drawn side by side from static sample data so the
// owner can try each before picking one. Every option works like a sidebar on
// local state only: rows select, groups fold, the collapse button shrinks it
// to an icon rail. Nothing navigates and nothing reaches the daemon; Search
// opens the real ⌘K and the theme controls set the real theme.
//
// The measures are hushbacks' (enform) UnifiedAppSidebar: 36px rows on a 24px
// radius, a 13px task rail with a 6px stage dot and a plain number on the
// right, one hairline between rail and nav, no group headings, a footer that
// is one row plus the theme segment.
import { createContext, useContext, useState, useSyncExternalStore, type ReactNode } from "react";
import { AnimatePresence, motion, useReducedMotion } from "motion/react";
import {
  CaretDown, CaretRight, CaretUpDown, Command as CommandIcon, Desktop, Eye, Gear, House, ListChecks, MagnifyingGlass, Moon, Plus,
  SidebarSimple, SlidersHorizontal, Sparkle, Sun, type Icon,
} from "@/ui/icons";
import {
  DropdownMenu, DropdownMenuContent, DropdownMenuGroup, DropdownMenuItem, DropdownMenuLabel, DropdownMenuRadioGroup,
  DropdownMenuRadioItem, DropdownMenuSeparator, DropdownMenuTrigger,
} from "@/ui/dropdown-menu";
import { Kbd } from "@/ui/kbd";
import { cn } from "@/ui/lib/utils";
import { Mark } from "@/shell/AppSidebar";
import { openPalette } from "@/shell/CommandPalette";
import type { Stage } from "@/shell/status";
import { getTheme, setTheme, subscribeTheme, type ThemeChoice } from "@/theme";
import {
  PAGE, SAMPLE_MACHINE, SAMPLE_NEEDS_YOU, SAMPLE_TASK_TOTAL, SAMPLE_TASKS, SAMPLE_UP_NEXT, type SamplePage, type SampleTask,
} from "@/pages/gallery/sidebar-sample";

// ---------------------------------------------------------------------------
// One piece of state per option: what is selected (a page or a task id) and
// whether it is collapsed to the icon rail. Parts read it from context.
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

const DOT: Record<Stage, string> = {
  needsYou: "bg-warning",
  review: "bg-info",
  running: "bg-primary",
  upNext: "bg-muted-foreground/50",
  done: "bg-success",
};

function Frame({ mock, children, className }: { mock: Mock; children: ReactNode; className?: string }) {
  return (
    <MockContext.Provider value={mock}>
      <div
        data-slot="sidebar-option-frame"
        data-collapsed={mock.collapsed}
        style={{ width: mock.collapsed ? 64 : "100%" }}
        className={cn(
          "flex h-[800px] flex-col overflow-hidden whitespace-nowrap rounded-2xl border border-border bg-sidebar shadow-(--sky-drop)",
          "transition-[width] duration-300 ease-[cubic-bezier(0.16,1,0.3,1)] motion-reduce:transition-none",
          className,
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
      <span className="min-w-0 flex-1 truncate text-sm font-semibold text-foreground">Neko</span>
      <CollapseButton />
    </div>
  );
}

/** Opens the real ⌘K palette. */
function Search() {
  const { collapsed } = useMock();
  return (
    <button type="button" onClick={openPalette} aria-label="Search" title="Search (⌘K)" className={cn("sky-row flex shrink-0 items-center gap-3 text-sm text-foreground/65", collapsed ? "justify-center" : "px-3", HOVER, FOCUS)}>
      <MagnifyingGlass size={16} className="shrink-0" />
      {collapsed ? null : <><span className="min-w-0 flex-1 truncate text-start font-medium">Search</span><Kbd className="bg-transparent"><CommandIcon size={11} />K</Kbd></>}
    </button>
  );
}

function Row({ page, id, icon, label, trailing, active, onClick, className, slot }: {
  page?: SamplePage;
  id?: string;
  icon?: Icon;
  label?: string;
  trailing?: ReactNode;
  active?: boolean;
  onClick?: () => void;
  className?: string;
  slot?: string;
}) {
  const { sel, select, collapsed } = useMock();
  const key = id ?? page?.id ?? "";
  const on = active ?? sel === key;
  const RowIcon = icon ?? page?.icon ?? House;
  const text = label ?? page?.title ?? "";
  return (
    <button
      type="button"
      data-slot={slot}
      title={collapsed ? text : undefined}
      aria-current={on ? "page" : undefined}
      onClick={onClick ?? (() => select(key))}
      className={cn(
        "sky-row flex w-full shrink-0 items-center gap-3 text-start text-sm",
        collapsed ? "justify-center" : "px-3",
        on ? "sky-pebble text-foreground" : cn("text-foreground/65", HOVER),
        FOCUS,
        className,
      )}
    >
      <RowIcon size={16} weight={on ? "fill" : "regular"} className="shrink-0" />
      {collapsed ? <span className="sr-only">{text}</span> : <><span className="min-w-0 flex-1 truncate font-medium">{text}</span>{trailing}</>}
    </button>
  );
}

function TaskRow({ task, trailing }: { task: SampleTask; trailing?: ReactNode }) {
  const { sel, select, collapsed } = useMock();
  if (collapsed) return null;
  const on = sel === task.id;
  return (
    <button
      type="button"
      onClick={() => select(task.id)}
      aria-current={on ? "true" : undefined}
      title={task.title}
      className={cn("sky-row flex w-full shrink-0 items-center gap-2.5 px-3 text-start text-[13px]", on ? "bg-foreground/[0.07] text-foreground" : cn("text-foreground/70", HOVER), FOCUS)}
    >
      <span aria-hidden className={cn("size-1.5 shrink-0 rounded-full", DOT[task.stage])} />
      <span className="min-w-0 flex-1 truncate">{task.title}</span>
      {trailing}
    </button>
  );
}

/** hushbacks' rail count: a plain tabular number, tinted only when it asks for you. */
function Count({ n, tone = "muted" }: { n: number | string; tone?: "muted" | "amber" }) {
  return <span className={cn("shrink-0 text-xs tabular-nums", tone === "amber" ? "font-medium text-warning-foreground" : "text-foreground/40")}>{n}</span>;
}

/** A small inline "new task" field: Enter or Esc closes it (nothing is filed). */
function NewTaskField({ onDone }: { onDone: () => void }) {
  return (
    <input
      autoFocus
      aria-label="New task title"
      placeholder="What needs doing?"
      onKeyDown={(e) => { if (e.key === "Enter" || e.key === "Escape") onDone(); }}
      onBlur={onDone}
      className="mx-1 h-8 shrink-0 rounded-lg border border-border bg-background/70 px-2.5 text-[13px] text-foreground outline-none placeholder:text-muted-foreground focus-visible:ring-2 focus-visible:ring-ring/60"
    />
  );
}

/** A section label with an optional "+" that opens the inline new-task field. */
function Label({ children, add }: { children: ReactNode; add?: boolean }) {
  const { collapsed } = useMock();
  const [adding, setAdding] = useState(false);
  if (collapsed) return null;
  return (
    <>
      <div className="flex shrink-0 items-center justify-between pb-1 pl-3 pr-1.5 pt-1">
        <span className="text-xs font-medium text-muted-foreground">{children}</span>
        {add ? (
          <button type="button" aria-label="New task" aria-pressed={adding} onClick={() => setAdding((a) => !a)} className={cn("flex size-6 items-center justify-center rounded-md text-muted-foreground", adding && "bg-foreground/[0.06] text-foreground", HOVER, FOCUS)}>
            <Plus size={12} weight="bold" className={cn("transition-transform duration-300 motion-reduce:transition-none", adding && "rotate-45")} />
          </button>
        ) : null}
      </div>
      {adding ? <NewTaskField onDone={() => setAdding(false)} /> : null}
    </>
  );
}

/** A label that folds its group (option C). */
function FoldLabel({ label, count, open, onToggle }: { label: string; count: number; open: boolean; onToggle: () => void }) {
  const Caret = open ? CaretDown : CaretRight;
  return (
    <button type="button" aria-expanded={open} onClick={onToggle} className={cn("flex w-full shrink-0 items-center gap-1.5 rounded-md py-1 pl-3 pr-1.5 text-xs font-medium text-muted-foreground", HOVER, FOCUS)}>
      <span>{label}</span>
      <span className="tabular-nums text-foreground/40">{count}</span>
      <Caret size={10} className="ms-auto" />
    </button>
  );
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

/** The footer gear: a menu that selects Settings or Usage in the mock, and sets the real theme. */
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

/** The footer status line plus the gear; collapsed, just the dot and the gear. */
function StatusFooter({ text, menu }: { text: ReactNode; menu: SamplePage[] }) {
  const { collapsed, select } = useMock();
  return (
    <div className="flex shrink-0 flex-col gap-2 pt-2">
      <Hairline />
      <div className={cn("flex items-center gap-1", collapsed ? "flex-col" : "pe-1")}>
        <button type="button" onClick={() => select("usage")} title="Usage & machine" className={cn("flex min-w-0 flex-1 items-center gap-2 rounded-md px-3 py-1 text-start text-[12.5px] text-muted-foreground", collapsed && "flex-none px-2", HOVER, FOCUS)}>
          <span aria-hidden className="size-2 shrink-0 rounded-full bg-success" />
          <span className={collapsed ? "sr-only" : "truncate"}>{text}</span>
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

const byStage = (stage: Stage) => SAMPLE_TASKS.filter((t) => t.stage === stage);
const meta = (t: SampleTask) => (t.meta ? <Count n={t.meta} /> : null);
const padding = (collapsed: boolean) => (collapsed ? "gap-2 px-2 pt-3 pb-3" : "gap-2 px-3 pt-3 pb-3");

// ---------------------------------------------------------------------------
// A. Quiet
// ---------------------------------------------------------------------------

function OptionQuiet() {
  const mock = useMockState(PAGE.home.id);
  const [all, setAll] = useState(false);
  const shown = all ? SAMPLE_TASKS : SAMPLE_TASKS.slice(0, 3);
  return (
    <Frame mock={mock} className={padding(mock.collapsed)}>
      <Brand />
      <Search />
      <div className={cn(SCROLL, "gap-0.5")}>
        {mock.collapsed ? null : (
          <div className="mt-2 flex flex-col gap-0.5">
            <Label add>Tasks</Label>
            {shown.map((t) => <TaskRow key={t.id} task={t} trailing={meta(t)} />)}
            <button type="button" aria-expanded={all} onClick={() => setAll((a) => !a)} className={cn("sky-row flex shrink-0 items-center px-3 ps-7 text-start text-[13px] text-muted-foreground", HOVER, FOCUS)}>
              {all ? "Show fewer" : `All ${SAMPLE_TASK_TOTAL} tasks`}
            </button>
          </div>
        )}
        <Hairline className={mock.collapsed ? "my-1" : "mt-3 mb-1"} />
        <Row page={PAGE.home} />
        <Row page={PAGE.tasks} trailing={<Count n={SAMPLE_NEEDS_YOU} tone="amber" />} />
        {[PAGE.prs, PAGE.tree, PAGE.watching, PAGE.schedules, PAGE.health, PAGE.workspaces, PAGE.memory, PAGE.profiles, PAGE.tools].map((p) => <Row key={p.id} page={p} />)}
      </div>
      <StatusFooter text={<>{SAMPLE_MACHINE.title} · {SAMPLE_NEEDS_YOU} need you</>} menu={[PAGE.settings, PAGE.usage]} />
    </Frame>
  );
}

// ---------------------------------------------------------------------------
// B. Two levels
// ---------------------------------------------------------------------------

const SECTIONS: { id: string; icon: Icon; label: string; pages: SamplePage[]; recent: SampleTask[] }[] = [
  { id: "home", icon: House, label: "Home", pages: [PAGE.home], recent: SAMPLE_TASKS.slice(0, 6) },
  { id: "work", icon: ListChecks, label: "Work", pages: [PAGE.tasks, PAGE.prs, PAGE.tree], recent: SAMPLE_TASKS.slice(0, 6) },
  { id: "watching", icon: Eye, label: "Watching", pages: [PAGE.watching, PAGE.schedules, PAGE.health], recent: SAMPLE_TASKS.filter((t) => /^[A-Z]+-\d+:/.test(t.title) || t.id === "t4") },
  { id: "setup", icon: SlidersHorizontal, label: "Setup", pages: [PAGE.workspaces, PAGE.memory, PAGE.profiles, PAGE.tools, PAGE.usage, PAGE.settings], recent: [] },
];

function OptionTwoLevels() {
  const mock = useMockState(PAGE.tasks.id);
  const [sectionId, setSection] = useState("work");
  const section = SECTIONS.find((s) => s.id === sectionId) ?? SECTIONS[1];
  return (
    <Frame mock={mock} className="flex-row">
      <div className="flex w-[62px] shrink-0 flex-col items-center gap-1 border-e border-border/60 py-3">
        <span className="sky-pebble mb-2 flex size-8 items-center justify-center rounded-[10px]"><Mark className="size-4" /></span>
        {mock.collapsed ? <div className="mb-2"><CollapseButton /></div> : <div className="mb-1" />}
        {SECTIONS.map(({ id, icon: RailIcon, label, pages }) => {
          const on = id === sectionId;
          return (
            <button
              key={id}
              type="button"
              aria-label={label}
              aria-current={on ? "page" : undefined}
              onClick={() => { setSection(id); mock.select(pages[0].id); }}
              className={cn("flex w-12 flex-col items-center gap-1 rounded-xl py-2", on ? "sky-pebble text-foreground" : cn("text-foreground/60", HOVER), FOCUS)}
            >
              <span className="relative">
                <RailIcon size={18} weight={on ? "fill" : "regular"} />
                {id === "work" && SAMPLE_NEEDS_YOU > 0 ? <span aria-hidden className="absolute -top-0.5 -end-1 size-1.5 rounded-full bg-warning" /> : null}
              </span>
              <span aria-hidden className="text-[11px] font-medium">{label}</span>
            </button>
          );
        })}
        <div className="mt-auto flex flex-col items-center gap-2">
          <button type="button" onClick={openPalette} aria-label="Search" title="Search (⌘K)" className={cn("flex size-7 items-center justify-center rounded-md text-muted-foreground", HOVER, FOCUS)}>
            <MagnifyingGlass size={15} />
          </button>
          <GearMenu items={[]} />
          <span aria-label={SAMPLE_MACHINE.title} className="mt-1 size-2 rounded-full bg-success" />
        </div>
      </div>
      {mock.collapsed ? null : (
        <div className="flex min-w-0 flex-1 flex-col gap-2 px-2 py-3">
          <div className="flex h-9 shrink-0 items-center ps-3">
            <span data-slot="panel-title" className="flex-1 text-sm font-semibold text-foreground">{section.label}</span>
            <CollapseButton />
          </div>
          <div className={cn(SCROLL, "gap-0.5")}>
            {section.pages.map((p) => <Row key={p.id} page={p} trailing={p.id === "tasks" ? <Count n={SAMPLE_NEEDS_YOU} tone="amber" /> : null} />)}
            {section.recent.length ? (
              <div className="mt-4 flex flex-col gap-0.5">
                <Label add>{section.id === "watching" ? "Filed by watches" : "Recent"}</Label>
                {section.recent.map((t) => <TaskRow key={t.id} task={t} trailing={meta(t)} />)}
              </div>
            ) : null}
          </div>
        </div>
      )}
    </Frame>
  );
}

// ---------------------------------------------------------------------------
// C. Tasks first
// ---------------------------------------------------------------------------

const C_GROUPS: { stage: Stage; label: string }[] = [
  { stage: "needsYou", label: "Needs you" },
  { stage: "running", label: "Running" },
  { stage: "review", label: "Review" },
  { stage: "upNext", label: "Up next" },
];
const MORE = [PAGE.tasks, PAGE.tree, PAGE.watching, PAGE.schedules, PAGE.health, PAGE.workspaces, PAGE.memory, PAGE.profiles, PAGE.tools, PAGE.usage];

function OptionTasksFirst() {
  const mock = useMockState(PAGE.home.id);
  const [open, setOpen] = useState<Record<string, boolean>>({ needsYou: true, running: true, review: true, upNext: false });
  const [adding, setAdding] = useState(false);
  const picked = MORE.find((p) => p.id === mock.sel);
  return (
    <Frame mock={mock} className={padding(mock.collapsed)}>
      <Brand />
      <button
        type="button"
        aria-label="New task"
        aria-pressed={adding}
        onClick={() => setAdding((a) => !a)}
        className={cn("sky-pebble sky-row flex shrink-0 items-center gap-2.5 text-sm font-medium text-foreground transition-transform active:scale-[0.98] motion-reduce:transform-none", mock.collapsed ? "justify-center" : "px-3", adding && "ring-1 ring-border", FOCUS)}
      >
        <Plus size={14} weight="bold" className={cn("transition-transform duration-300 motion-reduce:transition-none", adding && "rotate-45")} />
        {mock.collapsed ? null : <><span className="flex-1 text-start">New task</span><Kbd className="bg-transparent"><CommandIcon size={11} />N</Kbd></>}
      </button>
      {adding && !mock.collapsed ? <NewTaskField onDone={() => setAdding(false)} /> : null}
      <Search />
      <div className={cn(SCROLL, "gap-0.5")}>
        {mock.collapsed ? null : C_GROUPS.map(({ stage, label }) => {
          const list = byStage(stage);
          const count = stage === "upNext" ? SAMPLE_UP_NEXT : list.length;
          return (
            <div key={stage} className="mt-2 flex flex-col gap-0.5">
              <FoldLabel label={label} count={count} open={open[stage]} onToggle={() => setOpen((o) => ({ ...o, [stage]: !o[stage] }))} />
              {open[stage] ? list.map((t) => <TaskRow key={t.id} task={t} trailing={meta(t)} />) : null}
              {open[stage] && count > list.length ? <span className="px-3 ps-7 py-1 text-[12px] text-muted-foreground">and {count - list.length} more</span> : null}
            </div>
          );
        })}
      </div>
      <div className="flex shrink-0 flex-col gap-0.5 pt-2">
        <Hairline className="mb-2" />
        <Row page={PAGE.home} />
        <Row page={PAGE.prs} />
        {picked ? <Row page={picked} /> : null}
        <DropdownMenu>
          <DropdownMenuTrigger
            title={mock.collapsed ? "More pages" : undefined}
            className={cn("sky-row flex w-full shrink-0 items-center gap-3 text-start text-sm text-foreground/65", mock.collapsed ? "justify-center" : "px-3", HOVER, FOCUS)}
          >
            <CaretUpDown size={16} className="shrink-0" />
            {mock.collapsed ? <span className="sr-only">More pages</span> : <><span className="min-w-0 flex-1 truncate font-medium">More pages</span><Count n={MORE.length} /></>}
          </DropdownMenuTrigger>
          <DropdownMenuContent side="top" align="start" className="min-w-52">
            {MORE.map((p) => (
              <DropdownMenuItem key={p.id} onClick={() => mock.select(p.id)}><p.icon size={14} /> {p.title}</DropdownMenuItem>
            ))}
          </DropdownMenuContent>
        </DropdownMenu>
      </div>
      <StatusFooter text={<>{SAMPLE_MACHINE.title} · {SAMPLE_MACHINE.detail}</>} menu={[PAGE.settings]} />
    </Frame>
  );
}

// ---------------------------------------------------------------------------
// D. Setup folds away (hushbacks' own shape)
// ---------------------------------------------------------------------------

const SETUP = [PAGE.workspaces, PAGE.schedules, PAGE.memory, PAGE.profiles, PAGE.tools, PAGE.settings];

function SubRow({ page }: { page: SamplePage }) {
  const { sel, select } = useMock();
  const on = sel === page.id;
  return (
    <button
      type="button"
      onClick={() => select(page.id)}
      aria-current={on ? "page" : undefined}
      className={cn("relative flex w-full items-center rounded-md border py-1 pl-3 pr-2 text-start text-[13px]", on ? "border-border/40 bg-background/70 font-medium text-foreground" : cn("border-transparent text-foreground/55", HOVER), FOCUS)}
    >
      <span className="truncate">{page.title}</span>
    </button>
  );
}

function OptionSetupFolds() {
  const mock = useMockState(PAGE.home.id);
  const [open, setOpen] = useState(false);
  const reduce = useReducedMotion() ?? false;
  const inSetup = SETUP.some((p) => p.id === mock.sel);
  const theme = useTheme();
  const ThemeIcon = (THEMES.find((t) => t.value === theme) ?? THEMES[0]).icon;
  return (
    <Frame mock={mock} className={padding(mock.collapsed)}>
      <Brand />
      <Search />
      <div className={cn(SCROLL, "gap-0.5")}>
        {mock.collapsed ? null : (
          <div className="mt-2 flex flex-col gap-0.5">
            <Label add>Tasks</Label>
            {SAMPLE_TASKS.slice(0, 3).map((t) => <TaskRow key={t.id} task={t} trailing={meta(t)} />)}
          </div>
        )}
        <div className={cn("flex flex-col gap-0.5", !mock.collapsed && "mt-3 border-t border-border/50 pt-3")}>
          <Row page={PAGE.home} />
          <Row page={PAGE.tasks} trailing={<Count n={SAMPLE_TASK_TOTAL} />} />
          <Row page={PAGE.prs} />
          <Row page={PAGE.watching} />
          <Row page={PAGE.health} />
          <Row
            id="setup"
            slot="mock-setup"
            icon={Gear}
            label="Setup"
            active={inSetup}
            onClick={() => { if (mock.collapsed) mock.toggle(); setOpen((o) => !o || mock.collapsed); }}
            trailing={<CaretDown size={12} className={cn("text-muted-foreground transition-transform duration-200 motion-reduce:transition-none", !open && "-rotate-90")} />}
          />
          <AnimatePresence initial={false}>
            {open && !mock.collapsed ? (
              <motion.div
                key="setup"
                initial={reduce ? { opacity: 0 } : { height: 0, opacity: 0 }}
                animate={reduce ? { opacity: 1 } : { height: "auto", opacity: 1 }}
                exit={reduce ? { opacity: 0, transition: { duration: 0.1 } } : { height: 0, opacity: 0, transition: { duration: 0.22, ease: [0.16, 1, 0.3, 1] } }}
                transition={reduce ? { duration: 0.12 } : { duration: 0.3, ease: [0.16, 1, 0.3, 1] }}
                className="overflow-hidden"
              >
                <div data-slot="mock-setup-list" className="mt-1 ml-[1.15rem] flex flex-col border-l border-border/60 pl-3">
                  {SETUP.map((p) => <SubRow key={p.id} page={p} />)}
                </div>
              </motion.div>
            ) : null}
          </AnimatePresence>
        </div>
      </div>
      <div className="flex shrink-0 flex-col gap-2 pt-2">
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
                <span className="block truncate text-[12px] leading-tight text-muted-foreground">{SAMPLE_MACHINE.detail} · Usage &amp; machine</span>
              </span>
            </button>
            <div className="px-1"><ThemeSegment /></div>
          </>
        )}
      </div>
    </Frame>
  );
}

// ---------------------------------------------------------------------------

const OPTIONS: { id: string; name: string; caption: string; render: () => ReactNode }[] = [
  {
    id: "quiet",
    name: "A. Quiet",
    caption: "One list with no headings, work pages first. Only “needs you” gets a number. Settings, usage and theme move into the footer’s gear.",
    render: () => <OptionQuiet />,
  },
  {
    id: "two-levels",
    name: "B. Two levels",
    caption: "A rail of four sections; the panel lists only that section’s pages and its recent tasks, so each screen shows a handful of rows.",
    render: () => <OptionTwoLevels />,
  },
  {
    id: "tasks-first",
    name: "C. Tasks first",
    caption: "The sidebar is the work, grouped by who acts next. Pages shrink to Home, Pull requests and a More menu; ⌘K reaches the rest.",
    render: () => <OptionTasksFirst />,
  },
  {
    id: "setup-folds",
    name: "D. Setup folds away",
    caption: "hushbacks’ shape: a few daily rows; what you set up once folds under Setup, opening in place like enform’s Settings. The Mac row is the account row.",
    render: () => <OptionSetupFolds />,
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
