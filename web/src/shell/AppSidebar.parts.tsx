// from hushbacks apps/spa/src/components/UnifiedAppSidebar/UnifiedAppSidebar.parts.tsx @da20dfaeb
// Neko: TanStack's <Link> is a plain hash <a href>, the nav item carries
// `data-slot="nav-item"` and a right-hand slot for live chips (`trailing`), and
// the recent-forms skeleton is the task rail's. The active pill is the same
// motion layout pebble.
import { useSidebar } from "@/ui/sidebar";
import { Skeleton } from "@/ui/skeleton";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/ui/tooltip";
import type { Icon } from "@/ui/icons";
import { cn } from "@/ui/lib/utils";
import { motion } from "motion/react";
import { useEffect, useRef, type ReactNode } from "react";

export const EASE_OUT: [number, number, number, number] = [0.16, 1, 0.3, 1];

export type NavEntry = { label: string; icon: Icon; href: string };

export function NavItem({
  item,
  active,
  indicatorId = "sidebar-active",
  reduce = false,
  trailing,
  title,
  className,
}: {
  item: NavEntry;
  active: boolean;
  indicatorId?: string;
  reduce?: boolean;
  trailing?: ReactNode;
  title?: string;
  className?: string;
}) {
  const ItemIcon = item.icon;
  const { state } = useSidebar();
  const link = (
    <a
      href={item.href}
      title={title}
      data-slot="nav-item"
      aria-current={active ? "page" : undefined}
      className={cn(
        "sky-control group/nav relative flex items-center gap-3 text-sm transition-colors duration-150",
        "px-3 group-data-[collapsible=icon]:px-2 group-data-[collapsible=icon]:justify-center",
        "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60",
        active ? "text-foreground" : "text-foreground/65 hover:text-foreground hover:bg-foreground/[0.04]",
        className,
      )}
    >
      {active ? (
        <motion.span
          aria-hidden
          layoutId={indicatorId}
          transition={reduce ? { duration: 0 } : { type: "spring", stiffness: 480, damping: 40 }}
          style={{ borderRadius: 24 }}
          className="sky-pebble absolute inset-0"
        />
      ) : null}
      <ItemIcon
        size={16}
        weight={active ? "fill" : "regular"}
        className="relative z-10 shrink-0 transition-transform duration-150 group-active/nav:scale-90"
      />
      <span className="relative z-10 min-w-0 flex-1 truncate font-medium group-data-[collapsible=icon]:hidden">{item.label}</span>
      {trailing ? <span className="relative z-10 flex shrink-0 group-data-[collapsible=icon]:hidden">{trailing}</span> : null}
    </a>
  );

  if (state !== "collapsed") return link;
  return (
    <Tooltip>
      <TooltipTrigger asChild>{link}</TooltipTrigger>
      <TooltipContent side="right">{item.label}</TooltipContent>
    </Tooltip>
  );
}

export function CollapseWhenNarrow() {
  const { setOpen } = useSidebar();
  const setOpenRef = useRef(setOpen);
  setOpenRef.current = setOpen;
  useEffect(() => {
    const narrow = window.matchMedia("(max-width: 899px)");
    const collapse = () => {
      if (narrow.matches) setOpenRef.current(false);
    };
    collapse();
    narrow.addEventListener("change", collapse);
    return () => narrow.removeEventListener("change", collapse);
  }, []);
  return null;
}

const SKELETON_WIDTHS = ["w-4/5", "w-3/5", "w-2/3"];

export function RecentTasksSkeleton() {
  return (
    <div className="flex flex-col gap-0.5" aria-hidden>
      {SKELETON_WIDTHS.map((width) => (
        <div key={width} className="sky-row flex items-center gap-2.5 px-3">
          <span aria-hidden className="size-1.5 shrink-0 rounded-full bg-foreground/15" />
          <Skeleton className={cn("h-3", width)} />
        </div>
      ))}
    </div>
  );
}

let shellPainted = false;

export function markShellPainted() {
  shellPainted = true;
}

export function shellAlreadyPainted() {
  return shellPainted;
}
