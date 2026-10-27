// The gallery's "Sidebar" section: four ways to tidy the real sidebar
// (shell/AppSidebar.tsx), drawn side by side from static sample data so the
// owner can pick one. Nothing here is wired to the app; the frames are inert.
//
// The measures are hushbacks' (enform) UnifiedAppSidebar: 36px rows on a 24px
// radius, a 13px task rail with a 6px stage dot and a plain number on the
// right, one hairline between rail and nav, no group headings, a footer that
// is one row plus the theme segment.
import type { ReactNode } from "react";
import {
  CaretRight, CaretUpDown, Command as CommandIcon, Desktop, Eye, Gear, House, ListChecks, MagnifyingGlass, Moon, Plus,
  SidebarSimple, SlidersHorizontal, Sun, type Icon,
} from "@/ui/icons";
import { Kbd } from "@/ui/kbd";
import { cn } from "@/ui/lib/utils";
import { Mark } from "@/shell/AppSidebar";
import type { Stage } from "@/shell/status";
import {
  PAGE, SAMPLE_MACHINE, SAMPLE_NEEDS_YOU, SAMPLE_TASK_TOTAL, SAMPLE_TASKS, SAMPLE_UP_NEXT, type SamplePage, type SampleTask,
} from "@/pages/gallery/sidebar-sample";

// ---------------------------------------------------------------------------
// Small shared parts
// ---------------------------------------------------------------------------

const DOT: Record<Stage, string> = {
  needsYou: "bg-warning",
  review: "bg-info",
  running: "bg-primary",
  upNext: "bg-muted-foreground/50",
  done: "bg-success",
};

function Frame({ children, className }: { children: ReactNode; className?: string }) {
  return (
    <div
      inert
      data-slot="sidebar-option-frame"
      className={cn("flex h-[800px] w-full flex-col overflow-hidden rounded-2xl border border-border bg-sidebar shadow-(--sky-drop)", className)}
    >
      {children}
    </div>
  );
}

function Brand({ name = "Neko" }: { name?: string }) {
  return (
    <div className="flex h-9 shrink-0 items-center gap-1.5">
      <span className="sky-pebble flex size-7 shrink-0 items-center justify-center rounded-[9px]">
        <Mark className="size-3.5" />
      </span>
      <span aria-hidden className="text-lg font-light text-muted-foreground/40">/</span>
      <span className="min-w-0 flex-1 truncate text-sm font-semibold text-foreground">{name}</span>
      <span className="flex size-7 items-center justify-center text-muted-foreground"><SidebarSimple size={16} /></span>
    </div>
  );
}

function Search() {
  return (
    <div className="sky-row flex shrink-0 items-center gap-3 px-3 text-sm text-foreground/65">
      <MagnifyingGlass size={16} className="shrink-0" />
      <span className="min-w-0 flex-1 truncate font-medium">Search</span>
      <Kbd className="bg-transparent"><CommandIcon size={11} />K</Kbd>
    </div>
  );
}

function Row({ page, icon, label, active, trailing, className }: {
  page?: SamplePage;
  icon?: Icon;
  label?: string;
  active?: boolean;
  trailing?: ReactNode;
  className?: string;
}) {
  const RowIcon = icon ?? page?.icon ?? House;
  return (
    <div
      className={cn(
        "sky-row flex shrink-0 items-center gap-3 px-3 text-sm",
        active ? "sky-pebble text-foreground" : "text-foreground/65",
        className,
      )}
    >
      <RowIcon size={16} weight={active ? "fill" : "regular"} className="shrink-0" />
      <span className="min-w-0 flex-1 truncate font-medium">{label ?? page?.title}</span>
      {trailing}
    </div>
  );
}

function TaskRow({ task, trailing }: { task: SampleTask; trailing?: ReactNode }) {
  return (
    <div className="sky-row flex shrink-0 items-center gap-2.5 px-3 text-[13px] text-foreground/70">
      <span aria-hidden className={cn("size-1.5 shrink-0 rounded-full", DOT[task.stage])} />
      <span className="min-w-0 flex-1 truncate">{task.title}</span>
      {trailing}
    </div>
  );
}

/** hushbacks' rail count: a plain tabular number, tinted only when it asks for you. */
function Count({ n, tone = "muted" }: { n: number | string; tone?: "muted" | "amber" }) {
  return <span className={cn("shrink-0 text-xs tabular-nums", tone === "amber" ? "font-medium text-warning-foreground" : "text-foreground/40")}>{n}</span>;
}

function Label({ children, action }: { children: ReactNode; action?: boolean }) {
  return (
    <div className="flex shrink-0 items-center justify-between pb-1 pl-3 pr-1.5 pt-1">
      <span className="text-xs font-medium text-muted-foreground">{children}</span>
      {action ? <span className="flex size-6 items-center justify-center text-muted-foreground"><Plus size={12} weight="bold" /></span> : null}
    </div>
  );
}

function Hairline({ className }: { className?: string }) {
  return <div aria-hidden className={cn("mx-3 h-px shrink-0 bg-border/60", className)} />;
}

function Machine({ text }: { text: ReactNode }) {
  return (
    <span className="flex min-w-0 flex-1 items-center gap-2 px-3 text-[12.5px] text-muted-foreground">
      <span aria-hidden className="size-2 shrink-0 rounded-full bg-success" />
      <span className="truncate">{text}</span>
    </span>
  );
}

function IconButton({ icon: ButtonIcon, label }: { icon: Icon; label: string }) {
  return (
    <span aria-label={label} className="flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground">
      <ButtonIcon size={15} />
    </span>
  );
}

function ThemeSegment() {
  const themes: [Icon, string, boolean][] = [[Desktop, "System", true], [Sun, "Light", false], [Moon, "Dark", false]];
  return (
    <div role="group" aria-label="Theme" className="sky-segment">
      {themes.map(([ThemeIcon, label, on]) => (
        <button key={label} type="button" aria-pressed={on} aria-label={`${label} theme`}>
          <ThemeIcon size={13} weight={on ? "fill" : "regular"} />
        </button>
      ))}
    </div>
  );
}

const byStage = (stage: Stage) => SAMPLE_TASKS.filter((t) => t.stage === stage);
const meta = (t: SampleTask) => (t.meta ? <Count n={t.meta} /> : null);

// ---------------------------------------------------------------------------
// A. Quiet
// ---------------------------------------------------------------------------

function OptionQuiet() {
  const top = SAMPLE_TASKS.slice(0, 3);
  return (
    <Frame className="gap-2 px-3 pt-3 pb-3">
      <Brand />
      <Search />
      <div className="mt-2 flex flex-col gap-0.5">
        <Label action>Tasks</Label>
        {top.map((t) => <TaskRow key={t.id} task={t} trailing={meta(t)} />)}
        <div className="sky-row flex items-center px-3 ps-7 text-[13px] text-muted-foreground">All {SAMPLE_TASK_TOTAL} tasks</div>
      </div>
      <Hairline className="mt-3 mb-1" />
      <div className="flex flex-col gap-0.5">
        <Row page={PAGE.home} active />
        <Row page={PAGE.tasks} trailing={<Count n={SAMPLE_NEEDS_YOU} tone="amber" />} />
        <Row page={PAGE.prs} />
        <Row page={PAGE.tree} />
        <Row page={PAGE.watching} />
        <Row page={PAGE.schedules} />
        <Row page={PAGE.health} />
        <Row page={PAGE.workspaces} />
        <Row page={PAGE.memory} />
        <Row page={PAGE.profiles} />
        <Row page={PAGE.tools} />
      </div>
      <div className="mt-auto flex flex-col gap-2">
        <Hairline />
        <div className="flex items-center gap-1 pe-1">
          <Machine text={<>{SAMPLE_MACHINE.title} · {SAMPLE_NEEDS_YOU} need you</>} />
          <IconButton icon={Gear} label="Settings, usage and theme" />
        </div>
      </div>
    </Frame>
  );
}

// ---------------------------------------------------------------------------
// B. Two levels
// ---------------------------------------------------------------------------

const RAIL: { icon: Icon; label: string; active?: boolean; dot?: boolean }[] = [
  { icon: House, label: "Home" },
  { icon: ListChecks, label: "Work", active: true, dot: true },
  { icon: Eye, label: "Watching" },
  { icon: SlidersHorizontal, label: "Setup" },
];

function OptionTwoLevels() {
  return (
    <Frame className="flex-row">
      <div className="flex w-[56px] shrink-0 flex-col items-center gap-1 border-e border-border/60 py-3">
        <span className="sky-pebble mb-3 flex size-8 items-center justify-center rounded-[10px]"><Mark className="size-4" /></span>
        {RAIL.map(({ icon: RailIcon, label, active, dot }) => (
          <div key={label} className={cn("flex w-12 flex-col items-center gap-1 rounded-xl py-2", active ? "sky-pebble text-foreground" : "text-foreground/60")}>
            <span className="relative">
              <RailIcon size={18} weight={active ? "fill" : "regular"} />
              {dot ? <span aria-hidden className="absolute -top-0.5 -end-1 size-1.5 rounded-full bg-warning" /> : null}
            </span>
            <span className="text-[11px] font-medium">{label}</span>
          </div>
        ))}
        <div className="mt-auto flex flex-col items-center gap-2">
          <IconButton icon={MagnifyingGlass} label="Search" />
          <span aria-label={SAMPLE_MACHINE.title} className="size-2 rounded-full bg-success" />
        </div>
      </div>
      <div className="flex min-w-0 flex-1 flex-col gap-2 px-2 py-3">
        <div className="flex h-9 shrink-0 items-center px-3">
          <span className="flex-1 text-sm font-semibold text-foreground">Work</span>
          <span className="flex size-7 items-center justify-center text-muted-foreground"><SidebarSimple size={16} /></span>
        </div>
        <div className="flex flex-col gap-0.5">
          <Row page={PAGE.tasks} active trailing={<Count n={SAMPLE_NEEDS_YOU} tone="amber" />} />
          <Row page={PAGE.prs} />
          <Row page={PAGE.tree} />
        </div>
        <div className="mt-4 flex flex-col gap-0.5">
          <Label action>Recent</Label>
          {SAMPLE_TASKS.slice(0, 6).map((t) => <TaskRow key={t.id} task={t} trailing={meta(t)} />)}
        </div>
        <p className="mt-auto px-3 text-[12px] leading-snug text-muted-foreground">
          Watching holds Watching, Schedules and Health. Setup holds Workspaces, Memory, Profiles, Tools, Usage and Settings.
        </p>
      </div>
    </Frame>
  );
}

// ---------------------------------------------------------------------------
// C. Tasks first
// ---------------------------------------------------------------------------

function OptionTasksFirst() {
  const groups: { stage: Stage; label: string }[] = [
    { stage: "needsYou", label: "Needs you" },
    { stage: "running", label: "Running" },
    { stage: "review", label: "Review" },
  ];
  return (
    <Frame className="gap-2 px-3 pt-3 pb-3">
      <Brand />
      <div className="sky-pebble sky-row flex shrink-0 items-center gap-2.5 px-3 text-sm font-medium text-foreground">
        <Plus size={14} weight="bold" />
        <span className="flex-1">New task</span>
        <Kbd className="bg-transparent"><CommandIcon size={11} />N</Kbd>
      </div>
      <Search />
      {groups.map(({ stage, label }) => {
        const list = byStage(stage);
        return (
          <div key={stage} className="mt-2 flex flex-col gap-0.5">
            <Label>{label} <span className="ms-1 tabular-nums text-foreground/40">{list.length}</span></Label>
            {list.map((t) => <TaskRow key={t.id} task={t} trailing={meta(t)} />)}
          </div>
        );
      })}
      <div className="sky-row mt-1 flex shrink-0 items-center gap-2.5 px-3 text-[13px] text-muted-foreground">
        <CaretRight size={12} className="shrink-0" />
        <span className="flex-1">Up next</span>
        <Count n={SAMPLE_UP_NEXT} />
      </div>
      <div className="mt-auto flex flex-col gap-0.5">
        <Hairline className="mb-2" />
        <Row page={PAGE.home} />
        <Row page={PAGE.prs} />
        <Row icon={CaretUpDown} label="More pages" trailing={<Count n={10} />} />
        <div className="mt-2 flex items-center gap-1 pe-1">
          <Machine text={<>{SAMPLE_MACHINE.title} · {SAMPLE_MACHINE.detail}</>} />
          <IconButton icon={Gear} label="Settings and theme" />
        </div>
      </div>
    </Frame>
  );
}

// ---------------------------------------------------------------------------
// D. Setup folds away (hushbacks' own shape)
// ---------------------------------------------------------------------------

function SubRow({ label, active }: { label: string; active?: boolean }) {
  return (
    <div className={cn("relative flex items-center rounded-md py-1 pl-3 pr-2 text-[13px]", active ? "border border-border/40 bg-background/70 font-medium text-foreground" : "text-foreground/55")}>
      <span className="truncate">{label}</span>
    </div>
  );
}

function OptionSetupFolds() {
  const setup = [PAGE.workspaces, PAGE.schedules, PAGE.memory, PAGE.profiles, PAGE.tools, PAGE.settings];
  return (
    <Frame className="gap-2 px-3 pt-3 pb-2">
      <Brand />
      <Search />
      <div className="mt-2 flex flex-col gap-0.5">
        <Label action>Tasks</Label>
        {SAMPLE_TASKS.slice(0, 3).map((t) => <TaskRow key={t.id} task={t} trailing={meta(t)} />)}
      </div>
      <div className="mt-3 flex flex-col gap-0.5 border-t border-border/50 pt-3">
        <Row page={PAGE.home} />
        <Row page={PAGE.tasks} trailing={<Count n={SAMPLE_TASK_TOTAL} />} />
        <Row page={PAGE.prs} />
        <Row page={PAGE.watching} />
        <Row page={PAGE.health} />
        <Row icon={Gear} label="Setup" active />
        <div className="mt-1 ml-[1.15rem] flex flex-col border-l border-border/60 pl-3">
          {setup.map((p) => <SubRow key={p.id} label={p.title} active={p.id === "memory"} />)}
        </div>
      </div>
      <div className="mt-auto flex flex-col gap-2">
        <Hairline />
        <div className="flex items-center gap-3 px-2">
          <span className="sky-pebble flex size-8 shrink-0 items-center justify-center rounded-full"><Mark className="size-3.5" /></span>
          <div className="min-w-0 flex-1">
            <p className="truncate text-[13px] font-semibold leading-tight text-foreground">{SAMPLE_MACHINE.title}</p>
            <p className="truncate text-[12px] leading-tight text-muted-foreground">{SAMPLE_MACHINE.detail} · Usage &amp; machine</p>
          </div>
        </div>
        <div className="px-1"><ThemeSegment /></div>
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
