// from hushbacks apps/spa/src/pages/Builder/components/BuilderViewTabs/BuilderViewTabs.tsx @da20dfaeb
// (+ BuilderViewTabs.types.ts, BuilderViewTabs.hooks.ts)
// Neko: the view value is any string (a task's views, not a form's ViewMode),
// and the stage keeps hushbacks' ids so its tabpanel wiring is unchanged.
import { Tooltip, TooltipContent, TooltipTrigger } from "@/ui/tooltip";
import type { Icon } from "@/ui/icons";
import { cn } from "@/ui/lib/utils";
import { AnimatePresence, motion, useReducedMotion } from "motion/react";
import { type CSSProperties, type ReactNode, useState } from "react";
import { createPortal } from "react-dom";
import { BUILDER_HEADER_SLOT } from "./builderHeaderLayout";
import { FUSED_TAB_DURATION, FUSED_TAB_EASE, useFusedTabIndicator } from "./useFusedTabIndicator";
import "./BuilderViewTabs.css";

export type BuilderViewTab<V extends string = string> = {
  title: string;
  icon: Icon;
  value: V;
  badgeCount?: number;
  badgeLabel?: string;
  type?: never;
};

export type BuilderViewTabSeparator = {
  type: "separator";
  title?: never;
  icon?: never;
  value?: never;
  badgeCount?: never;
  badgeLabel?: never;
};

export type BuilderViewTabItem<V extends string = string> = BuilderViewTab<V> | BuilderViewTabSeparator;

export type BuilderViewTabsProps<V extends string = string> = {
  tabs: BuilderViewTabItem<V>[];
  value: V;
  onChange: (value: V) => void;
  onTabIntent?: (value: V) => void;
  renderContent?: (value: V) => ReactNode;
  headerMiddle?: ReactNode;
  headerEnd?: ReactNode;
  headerPortalTarget?: HTMLElement | null;
  showContent?: boolean;
  className?: string;
};

const TAB_EASE = FUSED_TAB_EASE;
const TAB_DURATION = FUSED_TAB_DURATION;

function isViewTab<V extends string>(tab: BuilderViewTabItem<V>): tab is BuilderViewTab<V> {
  return tab.type !== "separator";
}

function useViewSlideDirection<V extends string>(tabs: BuilderViewTabItem<V>[], value: V) {
  const [shown, setShown] = useState({ value, direction: 0 });
  if (shown.value !== value) {
    const order = tabs.flatMap((tab) => (tab.type === "separator" ? [] : [tab.value]));
    const delta = order.indexOf(value) - order.indexOf(shown.value);
    setShown({ value, direction: Math.sign(delta) || 1 });
  }
  return shown.direction;
}

export function BuilderViewTabs<V extends string>({
  tabs,
  value,
  onChange,
  onTabIntent,
  renderContent,
  headerMiddle,
  headerEnd,
  headerPortalTarget,
  showContent = true,
  className,
}: BuilderViewTabsProps<V>) {
  const reduceMotion = useReducedMotion();
  const activeTab = tabs.find((tab) => isViewTab(tab) && tab.value === value);
  const slideDirection = useViewSlideDirection(tabs, value);
  const {
    position: activeTabPosition,
    instant: indicatorInstant,
    listRef: tabListRef,
    tabRefs,
    labelRefs,
  } = useFusedTabIndicator(value);

  const header = (
    <div data-view={value} className="@container/header relative z-10 flex h-full min-w-0 items-end gap-3">
      <div
        ref={tabListRef}
        role="tablist"
        aria-label="Task views"
        className="relative flex h-[52px] shrink-0 items-end gap-0.5 pl-6 [scrollbar-width:none] @max-[38rem]/header:min-w-0 @max-[38rem]/header:shrink @max-[38rem]/header:overflow-x-auto @max-[38rem]/header:overflow-y-hidden @max-[38rem]/header:pl-2 @min-[56rem]/header:absolute @min-[56rem]/header:bottom-0 @min-[56rem]/header:left-1/2 @min-[56rem]/header:-translate-x-[calc(50%+12px)]"
      >
        {activeTabPosition ? (
          <motion.div
            aria-hidden="true"
            className={cn(
              "pointer-events-none absolute bottom-0 left-0 h-10 rounded-t-[14px] bg-[var(--builder-stage)]",
              "before:absolute before:-left-3 before:bottom-0 before:size-3 before:bg-[radial-gradient(circle_12px_at_0_0,transparent_11.5px,var(--builder-stage)_12px)] before:content-['']",
              "after:absolute after:-right-3 after:bottom-0 after:size-3 after:bg-[radial-gradient(circle_12px_at_100%_0,transparent_11.5px,var(--builder-stage)_12px)] after:content-['']",
            )}
            initial={false}
            animate={{ x: activeTabPosition.left, width: activeTabPosition.width }}
            transition={{ duration: reduceMotion || indicatorInstant ? 0 : TAB_DURATION, ease: TAB_EASE }}
          />
        ) : null}
        {tabs.map((tab, index) => {
          if (!isViewTab(tab)) {
            const previousTab = tabs[index - 1];
            const nextTab = tabs[index + 1];
            const previousValue = previousTab && isViewTab(previousTab) ? previousTab.value : "start";
            const nextValue = nextTab && isViewTab(nextTab) ? nextTab.value : "end";
            const touchesActiveTab = previousValue === value || nextValue === value;

            return (
              <div
                key={`separator-${previousValue}-${nextValue}`}
                className={cn("mx-0.5 mb-[15px] h-[18px] w-px bg-border", touchesActiveTab && "bg-transparent")}
                aria-hidden="true"
              />
            );
          }

          const isActive = tab.value === value;
          const showBadge = typeof tab.badgeCount === "number" && tab.badgeCount > 0;
          const tabLabel = showBadge && tab.badgeLabel ? `${tab.title}, ${tab.badgeLabel}` : tab.title;
          const TabIcon = tab.icon;

          return (
            <Tooltip key={tab.value} disabled={isActive} delayDuration={600}>
              <TooltipTrigger asChild>
                <button
                  ref={(element) => {
                    if (element) {
                      tabRefs.current.set(tab.value, element);
                      return;
                    }

                    tabRefs.current.delete(tab.value);
                  }}
                  id={`builder-view-tab-${tab.value}`}
                  type="button"
                  role="tab"
                  aria-selected={isActive}
                  aria-controls="builder-view-stage"
                  aria-label={tabLabel}
                  onClick={() => {
                    if (isActive) return;
                    onChange(tab.value);
                  }}
                  onMouseEnter={() => onTabIntent?.(tab.value)}
                  onFocus={() => onTabIntent?.(tab.value)}
                  className={cn(
                    "relative z-10 flex items-center justify-center text-sm font-medium",
                    "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60",
                    isActive
                      ? "h-10 self-end rounded-t-[14px] px-4 pb-2 text-primary"
                      : "mb-1 h-9 rounded-lg px-2 pb-1 text-foreground/50 transition-colors duration-150 hover:bg-foreground/[0.06] hover:text-foreground",
                  )}
                >
                  <TabIcon size={18} weight={isActive ? "fill" : "regular"} />
                  <AnimatePresence initial={false}>
                    {isActive ? (
                      <motion.span
                        key={`active-tab-label-${tab.value}`}
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
                        transition={{ duration: reduceMotion ? 0 : TAB_DURATION, ease: TAB_EASE }}
                        className="overflow-hidden whitespace-nowrap"
                      >
                        <span className="pl-2 @max-[38rem]/header:sr-only">{tab.title}</span>
                      </motion.span>
                    ) : null}
                  </AnimatePresence>
                  {showBadge ? (
                    <span
                      title={tab.badgeLabel}
                      className={cn(
                        "ml-1.5 inline-flex min-h-5 min-w-5 items-center justify-center rounded-full px-1.5 text-[11px] font-semibold",
                        isActive ? "bg-primary/15 text-primary" : "bg-foreground/10 text-foreground/60",
                      )}
                    >
                      {tab.badgeCount}
                    </span>
                  ) : null}
                </button>
              </TooltipTrigger>
              <TooltipContent side="bottom">{tabLabel}</TooltipContent>
            </Tooltip>
          );
        })}
      </div>

      {headerMiddle != null ? (
        <div className={cn("@container/toolbar min-w-0 flex-1 overflow-x-auto", BUILDER_HEADER_SLOT)}>
          {headerMiddle}
        </div>
      ) : (
        <div className="flex-1" />
      )}
      {headerEnd != null ? <div className={cn("shrink-0", BUILDER_HEADER_SLOT)}>{headerEnd}</div> : null}
    </div>
  );

  return (
    <div className={cn("flex min-h-0 min-w-0 flex-1 flex-col", className)}>
      {headerPortalTarget ? createPortal(header, headerPortalTarget) : header}
      {showContent && renderContent ? (
        <div
          id="builder-view-stage"
          role="tabpanel"
          aria-labelledby={activeTab && isViewTab(activeTab) ? `builder-view-tab-${activeTab.value}` : undefined}
          data-builder-view-stage="true"
          data-view={value}
          className="min-h-0 min-w-0 flex-1 overflow-hidden rounded-t-xl rounded-b-4xl bg-[var(--builder-stage)] p-1"
        >
          <div className="flex h-full min-h-0 min-w-0 flex-1 overflow-hidden rounded-t-lg rounded-b-[28px]">
            <div
              key={value}
              className={cn("min-h-0 min-w-0 flex-1 overflow-hidden", slideDirection !== 0 && "builder-view-enter")}
              style={slideDirection !== 0 ? ({ "--view-direction": slideDirection } as CSSProperties) : undefined}
            >
              {renderContent(value)}
            </div>
          </div>
        </div>
      ) : null}
    </div>
  );
}
