// from ui-kit packages/ui/src/components/tabs.tsx
import { Tabs as TabsPrimitive } from "@base-ui/react/tabs";
import type * as React from "react";

import { cn } from "@/ui/lib/utils";

export type TabsVariant = "default" | "underline";

function Tabs({ className, ...props }: React.ComponentProps<typeof TabsPrimitive.Root>) {
  return (
    <TabsPrimitive.Root
      data-slot="tabs"
      className={cn("flex flex-col gap-2 data-[orientation=vertical]:flex-row", className)}
      {...props}
    />
  );
}

function TabsList({
  variant = "default",
  className,
  children,
  ...props
}: React.ComponentProps<typeof TabsPrimitive.List> & { variant?: TabsVariant }) {
  return (
    <TabsPrimitive.List
      data-slot="tabs-list"
      data-variant={variant}
      className={cn(
        "relative z-0 flex w-fit items-center justify-center gap-x-0.5 text-muted-foreground data-[orientation=vertical]:flex-col",
        variant === "default"
          ? "sky-well ui-round h-10 gap-x-0.5 rounded-3xl p-[3px] text-muted-foreground"
          : "data-[orientation=horizontal]:py-1 data-[orientation=vertical]:px-1 *:data-[slot=tabs-tab]:hover:bg-accent",
        className,
      )}
      {...props}
    >
      {children}
      <TabsPrimitive.Indicator
        data-slot="tab-indicator"
        className={cn(
          "absolute bottom-0 left-0 h-(--active-tab-height) w-(--active-tab-width) translate-x-(--active-tab-left) -translate-y-(--active-tab-bottom) transition-[width,translate] duration-150 ease-[var(--ease-out)]",
          variant === "underline"
            ? "z-10 bg-primary data-[orientation=horizontal]:h-0.5 data-[orientation=horizontal]:translate-y-px data-[orientation=vertical]:w-0.5 data-[orientation=vertical]:-translate-x-px"
            : "sky-pebble-fill ui-round -z-1 rounded-[21px]",
        )}
      />
    </TabsPrimitive.List>
  );
}

function TabsTab({ className, ...props }: React.ComponentProps<typeof TabsPrimitive.Tab>) {
  return (
    <TabsPrimitive.Tab
      data-slot="tabs-tab"
      className={cn(
        "relative flex h-[34px] shrink-0 grow cursor-pointer items-center justify-center gap-1.5 ui-round rounded-[21px] border border-transparent px-3.5 text-sm font-medium whitespace-nowrap outline-none transition-colors duration-150 hover:text-foreground focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-ring data-[orientation=vertical]:w-full data-[orientation=vertical]:justify-start data-disabled:pointer-events-none data-disabled:opacity-50 data-active:text-[var(--sky-pebble-fg)] [&_svg:not([class*='size-'])]:size-4 [&_svg]:pointer-events-none [&_svg]:shrink-0",
        className,
      )}
      {...props}
    />
  );
}

function TabsPanel({ className, ...props }: React.ComponentProps<typeof TabsPrimitive.Panel>) {
  return <TabsPrimitive.Panel data-slot="tabs-content" className={cn("flex-1 outline-none", className)} {...props} />;
}

export { Tabs, TabsList, TabsTab, TabsPanel, TabsTab as TabsTrigger, TabsPanel as TabsContent, TabsPrimitive };
