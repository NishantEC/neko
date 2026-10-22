// from hushbacks apps/spa/src/pages/Builder/components/BuilderPanelTabs/BuilderPanelTabs.tsx @da20dfaeb
// (+ BuilderPanelTabs.types.ts). Neko: the value is any string; the tablist is
// named "Task panel".
import { Tooltip, TooltipContent, TooltipTrigger } from "@/ui/tooltip";
import { X, type Icon } from "@/ui/icons";
import { cn } from "@/ui/lib/utils";
import { AnimatePresence, motion, useReducedMotion } from "motion/react";
import { FUSED_TAB_DURATION, FUSED_TAB_EASE, useFusedTabIndicator } from "./useFusedTabIndicator";

export type BuilderPanelTab<V extends string = string> = {
  title: string;
  icon: Icon;
  value: V;
};

export type BuilderPanelTabsProps<V extends string = string> = {
  tabs: BuilderPanelTab<V>[];
  value: V;
  collapsed: boolean;
  onChange: (value: V) => void;
  onClose: () => void;
  className?: string;
};

export function BuilderPanelTabs<V extends string>({ tabs, value, collapsed, onChange, onClose, className }: BuilderPanelTabsProps<V>) {
  const reduceMotion = useReducedMotion();
  const { position, listRef, tabRefs, labelRefs } = useFusedTabIndicator(value);

  return (
    <div
      ref={listRef}
      role="tablist"
      aria-label="Task panel"
      aria-orientation="horizontal"
      className={cn("relative flex h-[52px] shrink-0 items-end gap-0.5", className)}
    >
      {position && !collapsed ? (
        <motion.div
          aria-hidden="true"
          className={cn(
            "pointer-events-none absolute bottom-0 left-0 h-10 rounded-t-[14px] bg-[var(--builder-panel)]",
            "before:absolute before:-left-3 before:bottom-0 before:size-3 before:bg-[radial-gradient(circle_12px_at_0_0,transparent_11.5px,var(--builder-panel)_12px)] before:content-['']",
            "after:absolute after:-right-3 after:bottom-0 after:size-3 after:bg-[radial-gradient(circle_12px_at_100%_0,transparent_11.5px,var(--builder-panel)_12px)] after:content-['']",
          )}
          initial={false}
          animate={{ x: position.left, width: position.width }}
          transition={{ duration: reduceMotion ? 0 : FUSED_TAB_DURATION, ease: FUSED_TAB_EASE }}
        />
      ) : null}

      {tabs.map((tab) => {
        const isActive = !collapsed && tab.value === value;
        const TabIcon = tab.icon;

        return (
          <div
            key={tab.value}
            ref={(element) => {
              if (element) {
                tabRefs.current.set(tab.value, element);
                return;
              }

              tabRefs.current.delete(tab.value);
            }}
            className="relative z-10 flex items-end"
          >
            <Tooltip disabled={isActive} delayDuration={600}>
              <TooltipTrigger asChild>
                <button
                  id={`builder-panel-tab-${tab.value}`}
                  type="button"
                  role="tab"
                  aria-selected={isActive}
                  aria-controls="builder-panel-surface"
                  aria-label={tab.title}
                  onClick={() => {
                    if (isActive) return;
                    onChange(tab.value);
                  }}
                  className={cn(
                    "flex items-center justify-center text-sm font-medium",
                    "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60",
                    isActive
                      ? "h-10 rounded-t-[14px] pb-2 pl-4 pr-1.5 text-primary"
                      : "mb-1 h-9 rounded-lg px-2 pb-1 text-foreground/50 transition-colors duration-150 hover:bg-foreground/[0.06] hover:text-foreground",
                  )}
                >
                  <TabIcon size={18} weight={isActive ? "fill" : "regular"} />
                  <AnimatePresence initial={false}>
                    {isActive ? (
                      <motion.span
                        key={`panel-tab-label-${tab.value}`}
                        ref={(element: HTMLElement | null) => {
                          if (element) {
                            labelRefs.current.set(tab.value, element);
                            return;
                          }

                          labelRefs.current.delete(tab.value);
                        }}
                        initial={{ width: 0, opacity: 0 }}
                        animate={{ width: "auto", opacity: 1 }}
                        exit={{ width: 0, opacity: 0 }}
                        transition={{ duration: reduceMotion ? 0 : FUSED_TAB_DURATION, ease: FUSED_TAB_EASE }}
                        className="overflow-hidden whitespace-nowrap"
                      >
                        <span className="pl-2 @max-[30rem]/chathead:sr-only">{tab.title}</span>
                      </motion.span>
                    ) : null}
                  </AnimatePresence>
                </button>
              </TooltipTrigger>
              <TooltipContent side="bottom">{tab.title}</TooltipContent>
            </Tooltip>

            {isActive ? (
              <Tooltip delayDuration={600}>
                <TooltipTrigger asChild>
                  <button
                    type="button"
                    aria-label="Hide panel"
                    onClick={onClose}
                    className={cn(
                      "mb-3.5 mr-3 flex size-5 items-center justify-center rounded-full text-foreground/45",
                      "transition-colors duration-150 hover:bg-foreground/10 hover:text-foreground",
                      "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60",
                    )}
                  >
                    <X size={13} weight="bold" />
                  </button>
                </TooltipTrigger>
                <TooltipContent side="bottom">Hide panel</TooltipContent>
              </Tooltip>
            ) : null}
          </div>
        );
      })}
    </div>
  );
}
