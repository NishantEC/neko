// from hushbacks packages/ui/src/components/sheet.tsx
import { Dialog as SheetPrimitive } from "@base-ui/react/dialog";
import { X } from "@/ui/icons";
import * as React from "react";

import { Button } from "@/ui/button";
import { useFormPortalContainer } from "@/ui/form-portal-context";
import { cn } from "@/ui/lib/utils";
import { SHEET_OVERLAY, type SheetSide, type SheetVariant, sheetSurfaceClass } from "./sheet.variants";

const SheetVariantContext = React.createContext<SheetVariant>("floating");

function Sheet({
  variant = "floating",
  ...props
}: React.ComponentProps<typeof SheetPrimitive.Root> & { variant?: SheetVariant }) {
  return (
    <SheetVariantContext.Provider value={variant}>
      <SheetPrimitive.Root data-slot="sheet" {...props} />
    </SheetVariantContext.Provider>
  );
}

function SheetTrigger({
  asChild,
  children,
  ...props
}: React.ComponentProps<typeof SheetPrimitive.Trigger> & { asChild?: boolean }) {
  if (asChild && React.isValidElement(children)) {
    return <SheetPrimitive.Trigger data-slot="sheet-trigger" render={children as React.ReactElement} {...props} />;
  }
  return (
    <SheetPrimitive.Trigger data-slot="sheet-trigger" {...props}>
      {children}
    </SheetPrimitive.Trigger>
  );
}

function SheetClose({
  asChild,
  children,
  ...props
}: React.ComponentProps<typeof SheetPrimitive.Close> & { asChild?: boolean }) {
  if (asChild && React.isValidElement(children)) {
    return <SheetPrimitive.Close data-slot="sheet-close" render={children as React.ReactElement} {...props} />;
  }
  return (
    <SheetPrimitive.Close data-slot="sheet-close" {...props}>
      {children}
    </SheetPrimitive.Close>
  );
}

function SheetPortal({ container, ...props }: React.ComponentProps<typeof SheetPrimitive.Portal>) {
  const formPortal = useFormPortalContainer();
  return <SheetPrimitive.Portal data-slot="sheet-portal" container={container ?? formPortal} {...props} />;
}

function SheetOverlay({
  className,
  variant = "floating",
  contained = false,
  ...props
}: React.ComponentProps<typeof SheetPrimitive.Backdrop> & { variant?: SheetVariant; contained?: boolean }) {
  return (
    <SheetPrimitive.Backdrop
      data-slot="sheet-overlay"
      data-variant={variant}
      className={cn(
        "transition-opacity data-starting-style:opacity-0 data-ending-style:opacity-0",
        variant === "floating" && (contained ? "absolute" : "fixed"),
        SHEET_OVERLAY[variant],
        className,
      )}
      {...props}
    />
  );
}

function SheetContent({
  className,
  children,
  side = "right",
  variant,
  container,
  showCloseButton = true,
  keepMounted = false,
  ...props
}: React.ComponentProps<typeof SheetPrimitive.Popup> & {
  side?: SheetSide;
  variant?: SheetVariant;
  container?: React.ComponentProps<typeof SheetPrimitive.Portal>["container"];
  showCloseButton?: boolean;
  keepMounted?: boolean;
}) {
  const inherited = React.useContext(SheetVariantContext);
  const resolved = variant ?? inherited;
  const contained = container != null;
  return (
    <SheetVariantContext.Provider value={resolved}>
      <SheetPortal keepMounted={keepMounted} container={container}>
        <SheetOverlay variant={resolved} contained={contained} />
        <SheetPrimitive.Popup
          data-slot="sheet-content"
          data-side={side}
          data-variant={resolved}
          className={cn(sheetSurfaceClass(resolved, side, contained), className)}
          {...props}
        >
          {children}
          {showCloseButton && <SheetCloseButton variant={resolved} />}
        </SheetPrimitive.Popup>
      </SheetPortal>
    </SheetVariantContext.Provider>
  );
}

function SheetCloseButton({ variant }: { variant: SheetVariant }) {
  const button =
    variant === "floating" ? (
      <Button
        variant="ghost"
        size="icon-sm"
        aria-label="Close"
        className="absolute top-4 right-6 text-muted-foreground"
      >
        <X size={16} />
      </Button>
    ) : (
      <Button variant="ghost" className="absolute top-4 right-4" size="icon-sm">
        <X weight="bold" />
        <span className="sr-only">Close</span>
      </Button>
    );
  return <SheetPrimitive.Close data-slot="sheet-close" render={button} />;
}

function SheetHeader({ className, ...props }: React.ComponentProps<"div">) {
  const floating = React.useContext(SheetVariantContext) === "floating";
  return (
    <div
      data-slot="sheet-header"
      className={cn(
        floating ? "flex shrink-0 flex-col gap-1 px-6 pt-6 pb-4 pe-16" : "gap-1.5 p-4 flex flex-col",
        className,
      )}
      {...props}
    />
  );
}

function SheetFooter({ className, ...props }: React.ComponentProps<"div">) {
  const floating = React.useContext(SheetVariantContext) === "floating";
  return (
    <div
      data-slot="sheet-footer"
      className={cn(
        floating
          ? "mt-auto flex shrink-0 flex-wrap items-center justify-between gap-2 px-6 py-4 text-xs text-muted-foreground [&>:only-child]:ms-auto"
          : "gap-2 p-4 mt-auto flex flex-col",
        className,
      )}
      {...props}
    />
  );
}

function SheetTitle({ className, ...props }: React.ComponentProps<typeof SheetPrimitive.Title>) {
  const floating = React.useContext(SheetVariantContext) === "floating";
  return (
    <SheetPrimitive.Title
      data-slot="sheet-title"
      className={cn(floating ? "text-sm font-semibold text-foreground" : "text-foreground font-medium", className)}
      {...props}
    />
  );
}

function SheetDescription({ className, ...props }: React.ComponentProps<typeof SheetPrimitive.Description>) {
  const floating = React.useContext(SheetVariantContext) === "floating";
  return (
    <SheetPrimitive.Description
      data-slot="sheet-description"
      className={cn(
        floating ? "text-xs leading-relaxed text-muted-foreground" : "text-muted-foreground text-sm",
        className,
      )}
      {...props}
    />
  );
}

export type { SheetSide, SheetVariant };
export { Sheet, SheetTrigger, SheetClose, SheetContent, SheetHeader, SheetFooter, SheetTitle, SheetDescription };
