// from hushbacks apps/spa/src/components/UnifiedAppSidebar/index.tsx @da20dfaeb
// (+ AccountFooter.tsx's ThemeSwitch and MobileTopBar.tsx)
//
// Same structure, classes and motion as hushbacks' sidebar: an inset, icon-
// collapsible sidebar; a header with the mark, a "/" and the product name; a
// recent rail (hushbacks' Forms, Neko's live Tasks with their stage chips); the
// nav under a hairline with the sliding pebble and Settings' sub-navigation
// revealed under it; a footer with the theme segment.
//
// Neko's own content, kept: Home and every registered page by group (yours sit
// in the same groups, marked "yours"), the live
// chips (Working / Needs you / counts), load problems of your pages, the
// Connected indicator, ⌘K search. Adapted: TanStack links are hash links, the
// workspace switcher is the product name (Neko has no active workspace), the
// account row is the connection row, the usage well and notification bell are
// not taken. Colours are Neko's tokens; the pebble comes from sky-materials.css.
import { useEffect, useState, type ComponentProps, type MouseEvent, type ReactNode } from "react";
import { AnimatePresence, motion, useReducedMotion } from "motion/react";
import {
  Sidebar, SidebarContent, SidebarFooter, SidebarHeader, SidebarTrigger, useSidebar,
} from "@/ui/sidebar";
import { Button } from "@/ui/button";
import { Kbd } from "@/ui/kbd";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/ui/tooltip";
import { Command as CommandIcon, MagnifyingGlass, Plus, Warning } from "@/ui/icons";
import { cn } from "@/ui/lib/utils";
import "@/pages";
import { usePages, type PageGroup } from "@/registry";
import { useSettingsSections } from "@/extensions";
import { SECTIONS } from "@/pages/setup/settings/registry";
import { hrefFor, HOME } from "@/router";
import { usePlugins } from "@/plugins";
import { useWorkbench } from "@/useStore";
import neko from "@/assets/neko-mark.svg";
import { openPalette } from "@/shell/CommandPalette";
import { SidebarStatus } from "@/shell/StatusBar";
import { newTask } from "@/shell/actions";
import { elapsed, STAGE_TITLES, taskHref, taskTitle, useNow, useShellStatus, type Stage } from "@/shell/status";
import { requestSettings, useCurrentSettingsSection } from "@/setup/settingsNav";
import { CollapseWhenNarrow, EASE_OUT, NavItem, RecentTasksSkeleton, shellAlreadyPainted } from "@/shell/AppSidebar.parts";

export function Mark({ className }: { className?: string }) {
  return (
    <span
      aria-hidden
      className={className}
      style={{ background: "currentColor", maskImage: `url(${neko})`, WebkitMaskImage: `url(${neko})`, maskSize: "contain", WebkitMaskSize: "contain", maskRepeat: "no-repeat", WebkitMaskRepeat: "no-repeat" }}
    />
  );
}

// ---------------------------------------------------------------------------
// Live state in a row's right-hand slot (Neko's own)
// ---------------------------------------------------------------------------

type Tone = "amber" | "sky" | "primary" | "muted";
const TONES: Record<Tone, string> = {
  amber: "bg-warning/15 text-warning-foreground",
  sky: "bg-info/15 text-info-foreground",
  primary: "bg-primary/15 text-primary",
  muted: "bg-foreground/[0.06] text-muted-foreground",
};

function Chip({ tone, children, label }: { tone: Tone; children: ReactNode; label?: string }) {
  return (
    <span data-slot="state-chip" data-tone={tone} aria-label={label} className={cn("ms-auto shrink-0 rounded-full px-1.5 py-px text-[11px] font-medium leading-4 tabular-nums", TONES[tone])}>
      {children}
    </span>
  );
}

function WorkingChip({ since }: { since: number }) {
  const now = useNow(1000);
  return <Chip tone="primary" label={`Working ${elapsed(now - since)}`}>Working {elapsed(now - since)}</Chip>;
}

const STAGE_CHIP: Record<Stage, { tone: Tone; text: string } | null> = {
  needsYou: { tone: "amber", text: "Needs you" },
  review: { tone: "sky", text: "Review" },
  running: { tone: "primary", text: "Working" },
  upNext: { tone: "muted", text: "Next" },
  done: null,
};
const STAGE_DOT: Record<Stage, string> = {
  needsYou: "bg-warning",
  review: "bg-info",
  running: "bg-primary animate-pulse motion-reduce:animate-none",
  upNext: "bg-muted-foreground/50",
  done: "bg-success",
};

/** "yours" on a page you wrote, "Customised" on a built-in you replaced; nothing on Neko's own. */
function OwnerMark({ source, customised }: { source: "builtin" | "user"; customised?: boolean }) {
  if (customised) return <Chip tone="primary" label="You replaced this page">Customised</Chip>;
  if (source === "user") return <Chip tone="muted" label="A page you added">yours</Chip>;
  return null;
}

/** The live chip for a page row, from the snapshot (null when there is nothing to say). */
function PageChip({ id }: { id: string }) {
  const status = useShellStatus();
  if (id === HOME) {
    if (status.working) return <WorkingChip since={status.working.startedMs} />;
    if (status.approvals > 0) return <Chip tone="amber" label={`${status.approvals} waiting for your approval`}>Needs you{status.approvals > 1 ? ` · ${status.approvals}` : ""}</Chip>;
    return null;
  }
  if (id === "tasks" && status.waitingOnYou > 0) {
    return <Chip tone="amber" label={`${status.waitingOnYou} tasks waiting on you`}>{status.waitingOnYou}</Chip>;
  }
  return null;
}

// ---------------------------------------------------------------------------
// The recent rail: hushbacks' FormsRail / FormRailItem, listing live tasks
// ---------------------------------------------------------------------------

/** How many tasks of each stage the rail lists; the rest are one click away. */
const SHOWN_PER_STAGE: Record<Stage, number> = { needsYou: 3, review: 2, running: 3, upNext: 1, done: 0 };

const RAIL_ROW =
  "sky-row group/form flex items-center gap-2.5 px-3 text-[13px] text-foreground/70 transition-colors duration-150 hover:bg-foreground/[0.04] hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60";

function TaskRailItem({ task, stage, index, reduce }: { task: any; stage: Stage; index: number; reduce: boolean }) {
  const chip = STAGE_CHIP[stage];
  const title = taskTitle(task);
  return (
    <motion.div
      initial={reduce ? undefined : { opacity: 0, x: -6 }}
      animate={{ opacity: 1, x: 0 }}
      transition={{ duration: 0.3, ease: EASE_OUT, delay: reduce ? 0 : 0.12 + index * 0.04 }}
    >
      <a href={taskHref(task.id)} data-slot="sidebar-task" data-stage={stage} title={`${title} · ${STAGE_TITLES[stage]}`} className={RAIL_ROW}>
        <span aria-hidden className={cn("size-1.5 shrink-0 rounded-full", STAGE_DOT[stage])} />
        <span className="min-w-0 flex-1 truncate">{title}</span>
        {chip ? <Chip tone={chip.tone} label={STAGE_TITLES[stage]}>{chip.text}</Chip> : null}
      </a>
    </motion.div>
  );
}

function TasksRail({ reduce }: { reduce: boolean }) {
  const status = useShellStatus();
  const { snapshot } = useWorkbench();
  const { setOpenMobile } = useSidebar();
  const order: Stage[] = ["needsYou", "review", "running", "upNext"];
  const shown = order.flatMap((stage) => status.tasks[stage].slice(0, SHOWN_PER_STAGE[stage]).map((task) => ({ task, stage })));
  const total = order.reduce((n, s) => n + status.tasks[s].length, 0);

  return (
    <div data-slot="sidebar-tasks" className="flex max-h-[40%] min-h-24 shrink-0 flex-col gap-0.5 group-data-[collapsible=icon]:hidden">
      <motion.div
        initial={reduce ? undefined : { opacity: 0, y: 6 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.35, ease: EASE_OUT, delay: reduce ? 0 : 0.08 }}
        className="flex shrink-0 items-center justify-between pb-1 pl-3 pr-1.5 pt-1"
      >
        <span className="text-xs font-medium text-muted-foreground">Tasks</span>
        <Tooltip>
          <TooltipTrigger asChild>
            <Button
              variant="ghost"
              size="icon-xs"
              aria-label="New task"
              className="group/plus -my-1 text-muted-foreground hover:bg-foreground/[0.04] hover:text-foreground"
              onClick={() => {
                setOpenMobile(false);
                newTask();
              }}
            >
              <Plus
                size={12}
                weight="bold"
                className="transition-transform duration-300 ease-[cubic-bezier(0.16,1,0.3,1)] group-hover/plus:rotate-90 motion-reduce:transform-none"
              />
            </Button>
          </TooltipTrigger>
          <TooltipContent side="right">New task (⌘N)</TooltipContent>
        </Tooltip>
      </motion.div>

      {!snapshot ? (
        <RecentTasksSkeleton />
      ) : shown.length > 0 ? (
        <div className="no-scrollbar flex min-h-0 flex-col gap-0.5 overflow-y-auto">
          {shown.map(({ task, stage }, i) => (
            <TaskRailItem key={task.id} task={task} stage={stage} index={i} reduce={reduce} />
          ))}
          {total > shown.length ? (
            <a href={hrefFor("tasks")} className={cn(RAIL_ROW, "text-muted-foreground")}>
              <span className="min-w-0 flex-1 truncate ps-4">Show all {total}</span>
            </a>
          ) : null}
        </div>
      ) : (
        <p className="px-3 py-1.5 text-[12px] leading-snug text-foreground/45">
          Nothing in progress. Press + to start a task.
        </p>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Settings' sub-navigation: hushbacks' SubNavItem / SubReveal
// ---------------------------------------------------------------------------

function SubNavItem({ label, active, layoutId, reduce = false, onClick }: {
  label: string;
  active: boolean;
  layoutId: string;
  reduce?: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      aria-current={active ? "page" : undefined}
      className={cn(
        "group/sub relative flex w-full items-center gap-2.5 rounded-md py-1.5 pl-3 pr-2 text-start text-[13px] transition-colors duration-150",
        "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60",
        active ? "font-medium text-foreground" : "text-foreground/55 hover:text-foreground",
      )}
    >
      {active ? (
        <motion.span
          aria-hidden
          layoutId={layoutId}
          transition={reduce ? { duration: 0 } : { type: "spring", stiffness: 520, damping: 42 }}
          style={{ borderRadius: 6 }}
          className="absolute inset-0 border border-border/40 bg-background/70"
        />
      ) : (
        <span aria-hidden className="absolute inset-0 rounded-md transition-colors duration-150 group-hover/sub:bg-foreground/[0.04]" />
      )}
      <span className="relative z-10 truncate">{label}</span>
    </button>
  );
}

function SettingsSubReveal({ open, reduce }: { open: boolean; reduce: boolean }) {
  const current = useCurrentSettingsSection();
  const { state } = useSidebar();
  const extra = useSettingsSections();
  const sections = [...SECTIONS.map((s) => ({ id: s.id as string, title: s.title })), ...extra.map((s) => ({ id: s.id, title: s.title }))];
  const layoutId = "settings-sub-active";
  return (
    <AnimatePresence initial={false}>
      {open && state !== "collapsed" ? (
        <motion.div
          key={layoutId}
          initial={reduce ? { opacity: 0 } : { height: 0, opacity: 0 }}
          animate={reduce ? { opacity: 1 } : { height: "auto", opacity: 1 }}
          exit={
            reduce
              ? { opacity: 0, transition: { duration: 0.1 } }
              : { height: 0, opacity: 0, transition: { duration: 0.22, ease: EASE_OUT } }
          }
          transition={reduce ? { duration: 0.12 } : { duration: 0.3, ease: EASE_OUT }}
          className="overflow-hidden group-data-[collapsible=icon]:hidden"
        >
          <nav aria-label="Settings sections" data-slot="settings-nav" className="mt-1 ml-[1.15rem] flex flex-col gap-0.5 border-l border-border/60 pl-3">
            {sections.map((s, si) => (
              <motion.div
                key={s.id}
                initial={reduce ? undefined : { opacity: 0, x: -5 }}
                animate={{ opacity: 1, x: 0 }}
                transition={{ duration: 0.24, ease: EASE_OUT, delay: reduce ? 0 : 0.05 + si * 0.03 }}
              >
                <SubNavItem label={s.title} active={current === s.id} layoutId={layoutId} reduce={reduce} onClick={() => requestSettings({ section: s.id })} />
              </motion.div>
            ))}
          </nav>
        </motion.div>
      ) : null}
    </AnimatePresence>
  );
}

// ---------------------------------------------------------------------------
// The footer: hushbacks' AccountFooter ThemeSwitch, then the connection row
// ---------------------------------------------------------------------------



// ---------------------------------------------------------------------------

function QuickCreate() {
  const { state, setOpenMobile } = useSidebar();
  const button = (
    <Button
      data-slot="button"
      size="icon"
      aria-label="New task"
      className="group/create"
      onClick={() => {
        setOpenMobile(false);
        newTask();
      }}
    >
      <Plus
        size={14}
        weight="bold"
        className="transition-transform duration-300 ease-[cubic-bezier(0.16,1,0.3,1)] group-hover/create:rotate-90 motion-reduce:transform-none"
      />
    </Button>
  );
  if (state !== "collapsed") return button;
  return (
    <Tooltip>
      <TooltipTrigger asChild>{button}</TooltipTrigger>
      <TooltipContent side="right">New task</TooltipContent>
    </Tooltip>
  );
}

/** ⌘K: the same row shape as a nav item, so it lines up with them. */
function SearchItem() {
  const { state } = useSidebar();
  const button = (
    <button
      type="button"
      onClick={openPalette}
      aria-label="Open the command palette"
      className={cn(
        "sky-control group/nav relative flex w-full items-center gap-3 text-sm text-foreground/65 transition-colors duration-150 hover:bg-foreground/[0.04] hover:text-foreground",
        "px-3 group-data-[collapsible=icon]:justify-center group-data-[collapsible=icon]:px-2",
        "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60",
      )}
    >
      <MagnifyingGlass size={16} className="relative z-10 shrink-0" />
      <span className="min-w-0 flex-1 truncate text-start font-medium group-data-[collapsible=icon]:hidden">Search</span>
      <Kbd className="bg-transparent group-data-[collapsible=icon]:hidden"><CommandIcon size={11} />K</Kbd>
    </button>
  );
  if (state !== "collapsed") return button;
  return (
    <Tooltip>
      <TooltipTrigger asChild>{button}</TooltipTrigger>
      <TooltipContent side="right">Search (⌘K)</TooltipContent>
    </Tooltip>
  );
}

function CloseMobileSidebarOnNavigate({ route }: { route: string }) {
  const { setOpenMobile } = useSidebar();
  useEffect(() => {
    if (route) setOpenMobile(false);
  }, [route, setOpenMobile]);
  return null;
}

function NavContent(props: ComponentProps<typeof SidebarContent>) {
  const { isMobile, setOpenMobile } = useSidebar();
  const closeOnLinkClick = (event: MouseEvent<HTMLDivElement>) => {
    if (isMobile && event.target instanceof Element && event.target.closest("a[href]")) setOpenMobile(false);
  };
  return <SidebarContent {...props} onClick={closeOnLinkClick} />;
}

function GroupLabel({ children }: { children: ReactNode }) {
  return <span className="block pb-1 pl-3 pr-1.5 pt-3 text-xs font-medium text-muted-foreground group-data-[collapsible=icon]:hidden">{children}</span>;
}

const GROUPS: { id: PageGroup; label: string | null }[] = [
  { id: "main", label: null },
  { id: "work", label: "Work" },
  { id: "system", label: "System" },
];

export function AppSidebar({ route }: { route: string }) {
  const reduce = useReducedMotion() ?? false;
  const [settled] = useState(shellAlreadyPainted);
  const still = reduce || settled;
  const { problems } = usePlugins();
  const pages = usePages();

  let step = 0;
  const cascade = () => {
    const i = ++step;
    return still
      ? {}
      : {
          initial: { opacity: 0, y: 6 },
          animate: { opacity: 1, y: 0 },
          transition: { duration: 0.35, ease: EASE_OUT, delay: 0.05 + i * 0.045 },
        };
  };

  return (
    <>
      <CloseMobileSidebarOnNavigate route={route} />
      <CollapseWhenNarrow />
      <Sidebar variant="inset" collapsible="icon" mobileClassName="sky">
        <SidebarHeader className="gap-2 px-3 pt-3 pb-2 group-data-[collapsible=icon]:px-2">
          <div className="flex h-9 items-center gap-1.5 group-data-[collapsible=icon]:justify-center">
            <a
              href={hrefFor(HOME)}
              title="Neko"
              className="sky-pebble flex size-7 shrink-0 items-center justify-center rounded-[9px] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60 group-data-[collapsible=icon]:hidden"
            >
              <Mark className="size-3.5" />
              <span className="sr-only">Neko</span>
            </a>
            <span aria-hidden className="text-lg font-light text-muted-foreground/40 group-data-[collapsible=icon]:hidden">
              /
            </span>
            <div className="min-w-0 flex-1 truncate text-sm font-semibold text-foreground group-data-[collapsible=icon]:hidden">Neko</div>
            <SidebarTrigger className="size-7 shrink-0 text-muted-foreground hover:text-foreground" />
          </div>
          <motion.div
            {...cascade()}
            className="hidden group-data-[collapsible=icon]:flex group-data-[collapsible=icon]:justify-center"
          >
            <QuickCreate />
          </motion.div>
          <SearchItem />
        </SidebarHeader>

        <NavContent className="px-3 py-1 group-data-[collapsible=icon]:px-2">
          <TasksRail reduce={still} />

          <div className="mt-5 flex shrink-0 flex-col gap-1 border-t border-border/50 pt-3 group-data-[collapsible=icon]:mt-2 group-data-[collapsible=icon]:border-t-0 group-data-[collapsible=icon]:pt-0">
            {GROUPS.map(({ id, label }) => {
              const list = pages.filter((p) => p.group === id);
              if (!list.length) return null;
              return (
                <div key={id} className="flex flex-col gap-1">
                  {label ? <GroupLabel>{label}</GroupLabel> : null}
                  {list.map((p) => (
                    <motion.div key={p.id} {...cascade()} className="flex flex-col">
                      <NavItem item={{ label: p.title, icon: p.icon, href: hrefFor(p.id) }} active={route === p.id} reduce={reduce} trailing={<><PageChip id={p.id} /><OwnerMark source={p.source} customised={p.customised} /></>} />
                      {p.id === "settings" ? <SettingsSubReveal open={route === "settings"} reduce={reduce} /> : null}
                    </motion.div>
                  ))}
                </div>
              );
            })}
            {problems.length > 0 ? (
              <div className="flex flex-col gap-1">
                {problems.map((p) => (
                  <NavItem
                    key={p.plugin}
                    item={{ label: p.plugin, icon: Warning, href: hrefFor(`problem:${p.plugin}`) }}
                    active={route === `problem:${p.plugin}`}
                    reduce={reduce}
                    title={p.message}
                    className="text-warning-foreground"
                  />
                ))}
              </div>
            ) : null}
          </div>
        </NavContent>

        <SidebarFooter className="px-0 pb-2 pt-0 gap-0">
          <div aria-hidden className="mx-3 mb-2 h-px bg-border/60 group-data-[collapsible=icon]:mx-2" />
          <div className="flex flex-col gap-2 px-2">
            <SidebarStatus />
          </div>
        </SidebarFooter>
      </Sidebar>
    </>
  );
}

/** hushbacks' MobileTopBar: the trigger and the name, below md. */
export function MobileTopBar() {
  return (
    <header className="flex h-12 shrink-0 items-center gap-2 px-2 md:hidden">
      <SidebarTrigger className="size-11 shrink-0 text-foreground/70 hover:text-foreground" />
      <a href={hrefFor(HOME)} title="Neko" className="flex items-center gap-2">
        <Mark className="size-5 text-foreground" />
        <span className="text-[15px] font-medium text-foreground">Neko</span>
      </a>
    </header>
  );
}
