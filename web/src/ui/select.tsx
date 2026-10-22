// from hushbacks packages/ui/src/components/select.tsx
import { Select as SelectPrimitive } from "@base-ui/react/select";
import { CaretDown, CaretUp, CaretUpDown, Check } from "@/ui/icons";
import type * as React from "react";

import { useFormPortalContainer } from "@/ui/form-portal-context";
import { cn } from "@/ui/lib/utils";

function Select({
  onValueChange,
  ...props
}: Omit<SelectPrimitive.Root.Props<string>, "onValueChange"> & {
  onValueChange?(value: string): void;
}) {
  return (
    <SelectPrimitive.Root
      data-slot="select"
      {...props}
      onValueChange={onValueChange ? (value) => onValueChange(value ?? "") : undefined}
    />
  );
}

function SelectGroup(props: React.ComponentProps<typeof SelectPrimitive.Group>) {
  return <SelectPrimitive.Group data-slot="select-group" {...props} />;
}

function SelectValue({ className, ...props }: React.ComponentProps<typeof SelectPrimitive.Value>) {
  return (
    <SelectPrimitive.Value
      data-slot="select-value"
      className={cn("line-clamp-1 flex items-center gap-1.5 data-placeholder:text-muted-foreground", className)}
      {...props}
    />
  );
}

function SelectTrigger({
  className,
  size = "default",
  appearance = "refined",
  children,
  ...props
}: React.ComponentProps<typeof SelectPrimitive.Trigger> & {
  size?: "sm" | "default";
  appearance?: "refined" | "respondent";
}) {
  return (
    <SelectPrimitive.Trigger
      data-slot="select-trigger"
      data-size={size}
      data-appearance={appearance}
      className={cn(
        appearance === "respondent"
          ? "border-input data-placeholder:text-muted-foreground dark:bg-input/30 dark:hover:bg-input/50 focus-visible:border-ring focus-visible:ring-ring/50 aria-invalid:ring-destructive/20 dark:aria-invalid:ring-destructive/40 aria-invalid:border-destructive dark:aria-invalid:border-destructive/50 gap-1.5 rounded-md border bg-transparent py-2 pr-2 pl-2.5 text-sm shadow-xs transition-[color,background-color,border-color,box-shadow] focus-visible:ring-[3px] aria-invalid:ring-[3px] data-[size=default]:h-9 data-[size=sm]:h-9 *:data-[slot=select-value]:flex *:data-[slot=select-value]:items-center *:data-[slot=select-value]:gap-1.5 [&_svg:not([class*='size-'])]:size-4 flex w-fit items-center justify-between whitespace-nowrap outline-none data-disabled:cursor-not-allowed data-disabled:opacity-50 [&_svg]:pointer-events-none [&_svg]:shrink-0"
          : "ui-inset-control data-placeholder:text-muted-foreground gap-1.5 border py-2 px-3 text-base sm:text-[13px] data-[size=default]:h-11 sm:data-[size=default]:h-10 data-[size=sm]:h-9 *:data-[slot=select-value]:flex *:data-[slot=select-value]:items-center *:data-[slot=select-value]:gap-1.5 [&_svg:not([class*='size-'])]:size-4 flex w-fit items-center justify-between whitespace-nowrap outline-none data-disabled:cursor-not-allowed data-disabled:opacity-50 [&_svg]:pointer-events-none [&_svg]:shrink-0",
        className,
      )}
      {...props}
    >
      {children}
      <SelectPrimitive.Icon data-slot="select-icon">
        <CaretUpDown weight="bold" className="text-muted-foreground size-4 pointer-events-none" />
      </SelectPrimitive.Icon>
    </SelectPrimitive.Trigger>
  );
}

function SelectContent({
  className,
  children,
  position: _position,
  align = "start",
  side = "bottom",
  sideOffset = 4,
  ...props
}: React.ComponentProps<typeof SelectPrimitive.Popup> & {
  position?: "item-aligned" | "popper";
  align?: SelectPrimitive.Positioner.Props["align"];
  side?: SelectPrimitive.Positioner.Props["side"];
  sideOffset?: number;
}) {
  const formPortal = useFormPortalContainer();
  return (
    <SelectPrimitive.Portal container={formPortal}>
      <SelectPrimitive.Positioner
        data-slot="select-positioner"
        side={side}
        align={align}
        sideOffset={sideOffset}
        alignItemWithTrigger={false}
        className="z-[1200]"
      >
        <SelectPrimitive.Popup
          data-slot="select-content"
          className={cn(
            "origin-(--transform-origin) ui-popup-surface min-w-36 overflow-hidden bg-popover text-popover-foreground outline-none transition-[transform,scale,opacity] data-starting-style:scale-95 data-starting-style:opacity-0 data-ending-style:scale-95 data-ending-style:opacity-0",
            className,
          )}
          {...props}
        >
          <SelectPrimitive.ScrollUpArrow
            data-slot="select-scroll-up-button"
            className="z-10 flex h-6 cursor-default items-center justify-center bg-popover [&_svg:not([class*='size-'])]:size-4"
          >
            <CaretUp weight="bold" />
          </SelectPrimitive.ScrollUpArrow>
          <SelectPrimitive.List className="max-h-(--available-height) overflow-y-auto p-[5px]">
            {children}
          </SelectPrimitive.List>
          <SelectPrimitive.ScrollDownArrow
            data-slot="select-scroll-down-button"
            className="z-10 flex h-6 cursor-default items-center justify-center bg-popover [&_svg:not([class*='size-'])]:size-4"
          >
            <CaretDown weight="bold" />
          </SelectPrimitive.ScrollDownArrow>
        </SelectPrimitive.Popup>
      </SelectPrimitive.Positioner>
    </SelectPrimitive.Portal>
  );
}

function SelectLabel({ className, ...props }: React.ComponentProps<typeof SelectPrimitive.GroupLabel>) {
  return (
    <SelectPrimitive.GroupLabel
      data-slot="select-label"
      className={cn("text-muted-foreground px-2 py-1.5 text-xs", className)}
      {...props}
    />
  );
}

function SelectItem({ className, children, ...props }: React.ComponentProps<typeof SelectPrimitive.Item>) {
  return (
    <SelectPrimitive.Item
      data-slot="select-item"
      className={cn(
        "relative flex min-h-9 w-full cursor-default items-center gap-2 rounded-lg py-2 pr-8 pl-2.5 text-[13px] ui-round outline-hidden select-none data-disabled:pointer-events-none data-disabled:opacity-50 data-highlighted:bg-accent data-highlighted:text-accent-foreground [&_svg:not([class*='size-'])]:size-4 [&_svg]:pointer-events-none [&_svg]:shrink-0",
        className,
      )}
      {...props}
    >
      <span className="pointer-events-none absolute right-2 flex size-4 items-center justify-center">
        <SelectPrimitive.ItemIndicator>
          <Check weight="bold" className="pointer-events-none" />
        </SelectPrimitive.ItemIndicator>
      </span>
      <SelectPrimitive.ItemText>{children}</SelectPrimitive.ItemText>
    </SelectPrimitive.Item>
  );
}

function SelectSeparator({ className, ...props }: React.ComponentProps<typeof SelectPrimitive.Separator>) {
  return (
    <SelectPrimitive.Separator
      data-slot="select-separator"
      className={cn("-mx-1 my-1 h-px bg-border pointer-events-none", className)}
      {...props}
    />
  );
}

function SelectScrollUpButton({ className, ...props }: React.ComponentProps<typeof SelectPrimitive.ScrollUpArrow>) {
  return (
    <SelectPrimitive.ScrollUpArrow
      data-slot="select-scroll-up-button"
      className={cn(
        "z-10 flex h-6 cursor-default items-center justify-center [&_svg:not([class*='size-'])]:size-4",
        className,
      )}
      {...props}
    >
      <CaretUp weight="bold" />
    </SelectPrimitive.ScrollUpArrow>
  );
}

function SelectScrollDownButton({ className, ...props }: React.ComponentProps<typeof SelectPrimitive.ScrollDownArrow>) {
  return (
    <SelectPrimitive.ScrollDownArrow
      data-slot="select-scroll-down-button"
      className={cn(
        "z-10 flex h-6 cursor-default items-center justify-center [&_svg:not([class*='size-'])]:size-4",
        className,
      )}
      {...props}
    >
      <CaretDown weight="bold" />
    </SelectPrimitive.ScrollDownArrow>
  );
}

export {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectLabel,
  SelectScrollDownButton,
  SelectScrollUpButton,
  SelectSeparator,
  SelectTrigger,
  SelectValue,
};
