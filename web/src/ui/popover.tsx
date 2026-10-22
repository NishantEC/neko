// from hushbacks packages/ui/src/components/popover.tsx
import { Popover as PopoverPrimitive } from "@base-ui/react/popover";
import * as React from "react";

import { useFormPortalContainer } from "@/ui/form-portal-context";
import { cn } from "@/ui/lib/utils";

function Popover(props: React.ComponentProps<typeof PopoverPrimitive.Root>) {
  return <PopoverPrimitive.Root data-slot="popover" {...props} />;
}

function PopoverTrigger({
  asChild,
  children,
  ...props
}: React.ComponentProps<typeof PopoverPrimitive.Trigger> & { asChild?: boolean }) {
  if (asChild && React.isValidElement(children)) {
    return <PopoverPrimitive.Trigger data-slot="popover-trigger" render={children as React.ReactElement} {...props} />;
  }
  return (
    <PopoverPrimitive.Trigger data-slot="popover-trigger" {...props}>
      {children}
    </PopoverPrimitive.Trigger>
  );
}

function PopoverContent({
  className,
  side = "bottom",
  align = "center",
  sideOffset = 4,
  collisionPadding,
  asChild,
  children,
  ...props
}: React.ComponentProps<typeof PopoverPrimitive.Popup> & {
  side?: PopoverPrimitive.Positioner.Props["side"];
  align?: PopoverPrimitive.Positioner.Props["align"];
  sideOffset?: number;
  collisionPadding?: PopoverPrimitive.Positioner.Props["collisionPadding"];
  asChild?: boolean;
}) {
  const formPortal = useFormPortalContainer();
  const renderAsChild = asChild && React.isValidElement(children);
  return (
    <PopoverPrimitive.Portal container={formPortal}>
      <PopoverPrimitive.Positioner
        data-slot="popover-positioner"
        side={side}
        align={align}
        sideOffset={sideOffset}
        collisionPadding={collisionPadding}
        className="z-50"
      >
        <PopoverPrimitive.Popup
          data-slot="popover-content"
          className={cn(
            "origin-(--transform-origin) z-50 flex w-72 flex-col gap-4 ui-popup-surface bg-popover p-4 text-sm text-popover-foreground outline-hidden transition-[transform,scale,opacity] data-starting-style:scale-95 data-starting-style:opacity-0 data-ending-style:scale-95 data-ending-style:opacity-0",
            className,
          )}
          {...(renderAsChild ? { render: children as React.ReactElement } : {})}
          {...props}
        >
          {renderAsChild ? undefined : children}
        </PopoverPrimitive.Popup>
      </PopoverPrimitive.Positioner>
    </PopoverPrimitive.Portal>
  );
}

function PopoverHeader({ className, ...props }: React.ComponentProps<"div">) {
  return <div data-slot="popover-header" className={cn("flex flex-col gap-1 text-sm", className)} {...props} />;
}

function PopoverTitle({ className, ...props }: React.ComponentProps<"h2">) {
  return <div data-slot="popover-title" className={cn("font-medium", className)} {...props} />;
}

function PopoverDescription({ className, ...props }: React.ComponentProps<"p">) {
  return <p data-slot="popover-description" className={cn("text-muted-foreground", className)} {...props} />;
}

export { Popover, PopoverContent, PopoverDescription, PopoverHeader, PopoverTitle, PopoverTrigger, PopoverPrimitive };
