// The `/` and `@` menu: ComposerMenuList in ComposerMenus.swift. It hangs off
// the composer's top edge and grows UPWARD, so it never moves what is below it
// and never covers the composer. It is opaque (the transcript would show
// through anything translucent), shows seven rows and scrolls past that.
import { useEffect, useRef } from "react";
import type { Icon } from "@/ui/icons";
import { cn } from "@/ui/lib/utils";

export interface ComposerMenuItem {
  id: string;
  icon: Icon;
  title: string;
  detail: string;
  shortcut?: string;
  apply(): void;
}

export const MENU_ROW_PX = 40;
export const MENU_MAX_ROWS = 7;

export function ComposerMenu({
  id, items, index, onIndexChange,
}: {
  id: string;
  items: ComposerMenuItem[];
  index: number;
  onIndexChange(next: number): void;
}) {
  const list = useRef<HTMLDivElement>(null);
  const active = Math.min(index, items.length - 1);
  // Keep the highlighted row in view as the arrows move it.
  useEffect(() => {
    const row = list.current?.querySelector<HTMLElement>(`[data-active='true']`);
    row?.scrollIntoView({ block: "nearest" });
  }, [active, items.length]);
  if (!items.length) return null;
  return (
    <div
      data-slot="composer-menu"
      className="absolute inset-x-0 bottom-full z-30 mb-2 rounded-2xl border border-border bg-popover p-1.5 text-popover-foreground shadow-lg ring-1 ring-foreground/5"
    >
      <div
        ref={list}
        id={id}
        role="listbox"
        aria-label="Composer suggestions"
        style={{ maxHeight: MENU_MAX_ROWS * MENU_ROW_PX }}
        className="overflow-y-auto overscroll-contain"
      >
        {items.map((item, i) => {
          const on = i === active;
          return (
            <div
              key={item.id}
              id={`${id}-${i}`}
              role="option"
              aria-selected={on}
              data-slot="composer-menu-row"
              data-active={on}
              onMouseEnter={() => onIndexChange(i)}
              // The textarea keeps focus: a click must not blur it before the row is applied.
              onMouseDown={(e) => e.preventDefault()}
              onClick={item.apply}
              style={{ height: MENU_ROW_PX }}
              className={cn("flex cursor-pointer items-center gap-3 rounded-xl px-3 text-[13.5px]", on ? "bg-accent text-strong" : "text-foreground")}
            >
              <item.icon className={cn("size-4 shrink-0", on ? "text-foreground" : "text-muted-foreground")} aria-hidden />
              <span className="w-[112px] shrink-0 truncate font-medium">{item.title}</span>
              <span className={cn("min-w-0 flex-1 truncate text-[13px]", on ? "text-foreground/75" : "text-muted-foreground")}>{item.detail}</span>
              {item.shortcut ? <span className="shrink-0 text-[12px] text-muted-foreground">{item.shortcut}</span> : null}
            </div>
          );
        })}
      </div>
    </div>
  );
}
