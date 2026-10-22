// The `/` and `@` menu: ComposerMenuList in ComposerMenus.swift. It hangs off
// the composer's top edge and grows UPWARD, so it never moves what is below it
// and never covers the composer. It is opaque (the transcript would show
// through anything translucent), shows seven rows and scrolls past that.
// Row markup from t3code apps/web/src/components/chat/ComposerCommandMenu.tsx
// @3b95e7831 (MIT): icon, a medium-weight label, the description in the
// secondary label colour, all at text-xs, on rounded-lg rows.
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
      className="absolute inset-x-0 bottom-full z-30 mb-2 rounded-2xl border border-border bg-popover p-1 text-popover-foreground shadow-composer dark:shadow-composer-dark"
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
              className={cn("flex cursor-pointer select-none items-center gap-2 rounded-lg px-2 text-sm outline-none", on ? "bg-accent text-accent-foreground" : "text-foreground")}
            >
              <item.icon className="size-4 shrink-0 text-secondary-label" aria-hidden />
              <span className="flex min-w-0 flex-1 items-center gap-2">
                <span className="min-w-0 max-w-[45%] shrink-0 truncate font-sans text-xs font-medium">{item.title}</span>
              <span className="min-w-0 flex-1 truncate text-left text-secondary-label text-xs">{item.detail}</span>
                {item.shortcut ? <span className="ms-auto shrink-0 text-2xs text-muted-foreground">{item.shortcut}</span> : null}
              </span>
            </div>
          );
        })}
      </div>
    </div>
  );
}
