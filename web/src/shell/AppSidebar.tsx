// from hushbacks apps/spa/src/components/UnifiedAppSidebar/index.tsx @da20dfaeb
// (+ WorkspaceSwitcher, AccountFooter.tsx and MobileTopBar.tsx)
//
// Option E of the gallery's sidebar proposals (#/ui, "Sidebar"), the one the
// owner picked (2026-10-28): an inset, icon-collapsible sidebar; a header with
// the mark, a "/" and hushbacks' workspace switcher (shell/workspace.ts, which
// Home, Tasks, Pull requests and the Agent tree follow); ⌘K search; the daily
// pages under a hairline (Home, Tasks, Pull requests, Agent tree, Watching,
// then pages of yours and plugins: one registry, no marks); the pages you set
// up once in smaller rows at the bottom, Settings' sub-navigation revealed
// under its row; a footer that is one status line and a gear. No task list:
// New task is ⌘N, ⌘K and the Tasks page. The only badge is the Tasks count of
// what waits on you; the status line says the rest.
import { useEffect, useState, type ComponentProps, type MouseEvent } from "react";
import { AnimatePresence, motion, useReducedMotion } from "motion/react";
import {
  Sidebar, SidebarContent, SidebarFooter, SidebarHeader, SidebarTrigger, useSidebar,
} from "@/ui/sidebar";
import {
  DropdownMenu, DropdownMenuContent, DropdownMenuGroup, DropdownMenuItem, DropdownMenuLabel, DropdownMenuSeparator, DropdownMenuTrigger,
} from "@/ui/dropdown-menu";
import { Kbd } from "@/ui/kbd";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/ui/tooltip";
import { CaretUpDown, Check, Command as CommandIcon, Gear, MagnifyingGlass, Stack, Warning } from "@/ui/icons";
import { cn } from "@/ui/lib/utils";
import "@/pages";
import { usePages, type PageDef } from "@/registry";
import { useSettingsSections } from "@/extensions";
import { SECTIONS } from "@/pages/setup/settings/registry";
import { hrefFor, HOME, navigate } from "@/router";
import { usePlugins } from "@/plugins";
import neko from "@/assets/neko-mark.svg";
import { openPalette } from "@/shell/CommandPalette";
import { SidebarStatus } from "@/shell/StatusBar";
import { useShellStatus } from "@/shell/status";
import { useSelectedWorkspace } from "@/shell/workspace";
import { requestSettings, useCurrentSettingsSection } from "@/setup/settingsNav";
import { CollapseWhenNarrow, EASE_OUT, NavItem, shellAlreadyPainted } from "@/shell/AppSidebar.parts";

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
// Where each page sits
// ---------------------------------------------------------------------------

/** The daily pages, in this order; pages of yours and plugins follow them. */
const TOP = [HOME, "tasks", "prs", "tree", "watching"];
/** The pages you set up once, smaller, at the bottom; other system pages (Customisations, UI gallery) follow them. */
const BOTTOM = ["schedules", "memory", "profiles", "tools", "settings", "usage"];
/** Reached from the switcher's "Manage workspaces…", not listed. */
const HIDDEN = ["workspaces"];

function place(pages: PageDef[]): { top: PageDef[]; bottom: PageDef[] } {
  const known = (ids: string[]) => ids.map((id) => pages.find((p) => p.id === id)).filter((p): p is PageDef => !!p);
  const rest = pages.filter((p) => ![...TOP, ...BOTTOM, ...HIDDEN].includes(p.id));
  return {
    top: [...known(TOP), ...rest.filter((p) => p.group !== "system")],
    bottom: [...known(BOTTOM), ...rest.filter((p) => p.group === "system")],
  };
}

/** The one badge: how many tasks wait on you, a plain amber number. */
function NeedsYouCount() {
  const { waitingOnYou } = useShellStatus();
  if (waitingOnYou <= 0) return null;
  return (
    <span data-slot="state-chip" data-tone="amber" aria-label={`${waitingOnYou} waiting on you`} className="ms-auto shrink-0 text-xs font-medium tabular-nums text-warning-foreground">
      {waitingOnYou}
    </span>
  );
}

// ---------------------------------------------------------------------------
// The workspace switcher: hushbacks' WorkspaceSwitcher, over Neko's workspaces
// ---------------------------------------------------------------------------

function WorkspaceAvatar({ name }: { name: string }) {
  return (
    <span aria-hidden className="sky-pebble flex size-5 shrink-0 items-center justify-center rounded-full text-[11px] font-semibold">
      {name.trim().charAt(0).toUpperCase() || "N"}
    </span>
  );
}

const MENU_ROW = "flex h-8 cursor-pointer items-center gap-2.5 rounded-md px-2 text-sm";

function WorkspaceSwitcher() {
  const { id, current, workspaces, select } = useSelectedWorkspace();
  const name = current?.name ?? "All workspaces";
  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        data-slot="workspace-switcher"
        aria-label={`Workspace: ${name}`}
        className={cn(
          "flex h-8 w-full min-w-0 items-center gap-1 rounded-[12px] px-1 text-start text-sm font-medium text-foreground transition-colors duration-150",
          "hover:bg-foreground/[0.04] data-[popup-open]:bg-foreground/[0.06]",
          "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60",
        )}
      >
        <span className="min-w-0 flex-1 truncate">{name}</span>
        <CaretUpDown size={14} weight="bold" className="shrink-0 text-muted-foreground" />
      </DropdownMenuTrigger>
      <DropdownMenuContent data-slot="workspace-menu" align="start" sideOffset={6} className="w-64 p-1">
        <DropdownMenuGroup>
          <DropdownMenuLabel className="px-2 py-1.5 text-xs font-normal text-muted-foreground">Workspaces</DropdownMenuLabel>
          {workspaces.map((w) => (
            <DropdownMenuItem key={w.id} onClick={() => select(w.id)} className={MENU_ROW}>
              <WorkspaceAvatar name={w.name} />
              <span className="min-w-0 flex-1 truncate">{w.name}</span>
              {w.id === id ? <Check size={14} weight="bold" className="shrink-0" aria-label="current" /> : null}
            </DropdownMenuItem>
          ))}
          <DropdownMenuItem onClick={() => select("")} className={MENU_ROW}>
            <Stack size={15} className="text-muted-foreground" />
            <span className="min-w-0 flex-1 truncate">All workspaces</span>
            {id === "" ? <Check size={14} weight="bold" className="shrink-0" aria-label="current" /> : null}
          </DropdownMenuItem>
        </DropdownMenuGroup>
        <DropdownMenuSeparator className="my-1" />
        <DropdownMenuItem onClick={() => navigate("workspaces")} className={MENU_ROW}>
          <Gear size={15} className="text-muted-foreground" />
          Manage workspaces…
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
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

export function AppSidebar({ route }: { route: string }) {
  const reduce = useReducedMotion() ?? false;
  const [settled] = useState(shellAlreadyPainted);
  const still = reduce || settled;
  const { problems } = usePlugins();
  const { top, bottom } = place(usePages());

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
  const row = (p: PageDef, small = false) => (
    <motion.div key={p.id} {...cascade()} className="flex flex-col">
      <NavItem
        item={{ label: p.title, icon: p.icon, href: hrefFor(p.id) }}
        active={route === p.id}
        reduce={reduce}
        small={small}
        trailing={p.id === "tasks" ? <NeedsYouCount /> : undefined}
      />
      {p.id === "settings" ? <SettingsSubReveal open={route === "settings"} reduce={reduce} /> : null}
    </motion.div>
  );

  return (
    <>
      <CloseMobileSidebarOnNavigate route={route} />
      <CollapseWhenNarrow />
      <Sidebar variant="inset" collapsible="icon" mobileClassName="sky">
        <SidebarHeader className="gap-2 px-3 pt-3 pb-2 group-data-[collapsible=icon]:px-2">
          <div className="flex h-9 items-center gap-1 group-data-[collapsible=icon]:h-auto group-data-[collapsible=icon]:flex-col group-data-[collapsible=icon]:gap-2">
            <a
              href={hrefFor(HOME)}
              title="Neko"
              className="sky-pebble flex size-7 shrink-0 items-center justify-center rounded-[9px] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
            >
              <Mark className="size-3.5" />
              <span className="sr-only">Neko</span>
            </a>
            <span aria-hidden className="text-lg font-light text-muted-foreground/40 group-data-[collapsible=icon]:hidden">
              /
            </span>
            <div className="min-w-0 flex-1 group-data-[collapsible=icon]:hidden">
              <WorkspaceSwitcher />
            </div>
            <SidebarTrigger className="size-7 shrink-0 text-muted-foreground hover:text-foreground" />
          </div>
          <SearchItem />
        </SidebarHeader>

        <NavContent className="px-3 pt-1 pb-2 group-data-[collapsible=icon]:px-2">
          <div aria-hidden className="mx-3 mb-2 h-px shrink-0 bg-border/60 group-data-[collapsible=icon]:mx-1" />
          <div data-slot="sidebar-daily" className="flex shrink-0 flex-col gap-1">
            {top.map((p) => row(p))}
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
          <div data-slot="sidebar-setup" className="mt-auto flex shrink-0 flex-col gap-0.5 pt-6">
            {bottom.map((p) => row(p, true))}
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
