// from hushbacks packages/ui/src/components/hover-card.tsx
import { PreviewCard as PreviewCardPrimitive } from "@base-ui/react/preview-card";
import * as React from "react";

import { useFormPortalContainer } from "@/ui/form-portal-context";
import { cn } from "@/ui/lib/utils";

function HoverCard(props: React.ComponentProps<typeof PreviewCardPrimitive.Root>) {
  return <PreviewCardPrimitive.Root data-slot="hover-card" {...props} />;
}

function HoverCardTrigger({
  asChild,
  children,
  ...props
}: React.ComponentProps<typeof PreviewCardPrimitive.Trigger> & { asChild?: boolean }) {
  if (asChild && React.isValidElement(children)) {
    return (
      <PreviewCardPrimitive.Trigger data-slot="hover-card-trigger" render={children as React.ReactElement} {...props} />
    );
  }
  return (
    <PreviewCardPrimitive.Trigger data-slot="hover-card-trigger" {...props}>
      {children}
    </PreviewCardPrimitive.Trigger>
  );
}

function HoverCardContent({
  className,
  align = "center",
  side,
  sideOffset = 4,
  children,
  ...props
}: React.ComponentProps<typeof PreviewCardPrimitive.Popup> & {
  align?: PreviewCardPrimitive.Positioner.Props["align"];
  side?: PreviewCardPrimitive.Positioner.Props["side"];
  sideOffset?: number;
}) {
  const formPortal = useFormPortalContainer();
  return (
    <PreviewCardPrimitive.Portal container={formPortal}>
      <PreviewCardPrimitive.Positioner
        data-slot="hover-card-positioner"
        align={align}
        side={side}
        sideOffset={sideOffset}
        className="z-50"
      >
        <PreviewCardPrimitive.Popup
          data-slot="hover-card-content"
          className={cn(
            "origin-(--transform-origin) w-64 rounded-lg bg-popover p-4 text-sm text-popover-foreground smooth-shadow-ring-md outline-hidden transition-[scale,opacity] data-starting-style:scale-95 data-starting-style:opacity-0 data-ending-style:scale-95 data-ending-style:opacity-0",
            className,
          )}
          {...props}
        >
          {children}
        </PreviewCardPrimitive.Popup>
      </PreviewCardPrimitive.Positioner>
    </PreviewCardPrimitive.Portal>
  );
}

export { HoverCard, HoverCardTrigger, HoverCardContent, PreviewCardPrimitive };
