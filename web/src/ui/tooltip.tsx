// from hushbacks packages/ui/src/components/tooltip.tsx
import { Tooltip as TooltipPrimitive } from "@base-ui/react/tooltip";
import * as React from "react";

import { useFormPortalContainer } from "@/ui/form-portal-context";
import { cn } from "@/ui/lib/utils";

function TooltipProvider({
  delay = 0,
  delayDuration,
  ...props
}: React.ComponentProps<typeof TooltipPrimitive.Provider> & { delayDuration?: number }) {
  return <TooltipPrimitive.Provider data-slot="tooltip-provider" delay={delayDuration ?? delay} {...props} />;
}

function Tooltip({
  delayDuration,
  ...props
}: React.ComponentProps<typeof TooltipPrimitive.Root> & { delayDuration?: number }) {
  return (
    <TooltipProvider delay={delayDuration}>
      <TooltipPrimitive.Root data-slot="tooltip" {...props} />
    </TooltipProvider>
  );
}

function TooltipTrigger({
  asChild,
  children,
  ...props
}: React.ComponentProps<typeof TooltipPrimitive.Trigger> & { asChild?: boolean }) {
  if (asChild && React.isValidElement(children)) {
    return <TooltipPrimitive.Trigger data-slot="tooltip-trigger" render={children as React.ReactElement} {...props} />;
  }
  return (
    <TooltipPrimitive.Trigger data-slot="tooltip-trigger" {...props}>
      {children}
    </TooltipPrimitive.Trigger>
  );
}

function TooltipContent({
  className,
  side = "top",
  align = "center",
  sideOffset = 6,
  collisionPadding,
  children,
  ...props
}: React.ComponentProps<typeof TooltipPrimitive.Popup> & {
  side?: TooltipPrimitive.Positioner.Props["side"];
  align?: TooltipPrimitive.Positioner.Props["align"];
  sideOffset?: number;
  collisionPadding?: TooltipPrimitive.Positioner.Props["collisionPadding"];
}) {
  const formPortal = useFormPortalContainer();
  return (
    <TooltipPrimitive.Portal container={formPortal}>
      <TooltipPrimitive.Positioner
        data-slot="tooltip-positioner"
        side={side}
        align={align}
        sideOffset={sideOffset}
        collisionPadding={collisionPadding}
        className="z-50"
      >
        <TooltipPrimitive.Popup
          data-slot="tooltip-content"
          className={cn(
            "origin-(--transform-origin) z-50 w-fit max-w-70 text-balance rounded-xl bg-popover p-4 text-xs text-popover-foreground smooth-shadow-ring-2xl transition-[transform,scale,opacity] data-starting-style:scale-95 data-starting-style:opacity-0 data-ending-style:scale-95 data-ending-style:opacity-0",
            className,
          )}
          {...props}
        >
          {children}
          <TooltipPrimitive.Arrow className="size-2.5 -translate-y-1/2 rotate-45 rounded-[2px] border border-t-0 border-l-0 border-border/30 bg-popover" />
        </TooltipPrimitive.Popup>
      </TooltipPrimitive.Positioner>
    </TooltipPrimitive.Portal>
  );
}

export { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger, TooltipPrimitive };
