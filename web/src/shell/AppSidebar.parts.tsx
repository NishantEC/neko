// from hushbacks apps/spa/src/components/UnifiedAppSidebar/UnifiedAppSidebar.parts.tsx @da20dfaeb
// Neko: TanStack's <Link> is a plain hash <a href>, the nav item carries
// `data-slot="nav-item"` and a right-hand slot (`trailing`, the Tasks count),
// and a `small` row for the pages you set up once. The active pill is the same
// motion layout pebble.
import { useSidebar } from "@/ui/sidebar";
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
  small = false,
}: {
  item: NavEntry;
  active: boolean;
  indicatorId?: string;
  reduce?: boolean;
  trailing?: ReactNode;
  title?: string;
  className?: string;
  /** The quieter row for the pages you set up once: 32px, 13px, a 14px icon. */
  small?: boolean;
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
        "group/nav relative flex items-center transition-colors duration-150",
        small ? "h-8 gap-2.5 rounded-[16px] text-[13px]" : "sky-control gap-3 text-sm",
        "px-3 group-data-[collapsible=icon]:px-2 group-data-[collapsible=icon]:justify-center",
        "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60",
        active ? "text-foreground" : cn(small ? "text-foreground/55" : "text-foreground/65", "hover:text-foreground hover:bg-foreground/[0.04]"),
        className,
      )}
    >
      {active ? (
        <motion.span
          aria-hidden
          layoutId={indicatorId}
          transition={reduce ? { duration: 0 } : { type: "spring", stiffness: 480, damping: 40 }}
          style={{ borderRadius: small ? 16 : 24 }}
          className="sky-pebble absolute inset-0"
        />
      ) : null}
      <ItemIcon
        size={small ? 14 : 16}
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

let shellPainted = false;

export function markShellPainted() {
  shellPainted = true;
}

export function shellAlreadyPainted() {
  return shellPainted;
}
