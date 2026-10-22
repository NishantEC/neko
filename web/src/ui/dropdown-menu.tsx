// from hushbacks packages/ui/src/components/dropdown-menu.tsx
import { Menu as MenuPrimitive } from "@base-ui/react/menu";
import { ArrowRight, Check } from "@/ui/icons";
import * as React from "react";

import { useFormPortalContainer } from "@/ui/form-portal-context";
import { cn } from "@/ui/lib/utils";

const itemBase =
  "group/dropdown-menu-item relative flex cursor-default items-center gap-2 ui-round min-h-9 rounded-lg px-2.5 py-1.5 text-[13px] outline-hidden select-none data-disabled:pointer-events-none data-disabled:opacity-50 data-inset:pl-8 data-highlighted:bg-accent data-highlighted:text-accent-foreground [&_svg:not([class*='size-'])]:size-4 [&_svg]:pointer-events-none [&_svg]:shrink-0";

function DropdownMenu(props: React.ComponentProps<typeof MenuPrimitive.Root>) {
  return <MenuPrimitive.Root data-slot="dropdown-menu" {...props} />;
}

function DropdownMenuPortal({ container, ...props }: React.ComponentProps<typeof MenuPrimitive.Portal>) {
  const formPortal = useFormPortalContainer();
  return <MenuPrimitive.Portal data-slot="dropdown-menu-portal" container={container ?? formPortal} {...props} />;
}

function DropdownMenuTrigger({
  asChild,
  children,
  ...props
}: React.ComponentProps<typeof MenuPrimitive.Trigger> & { asChild?: boolean }) {
  if (asChild && React.isValidElement(children)) {
    return (
      <MenuPrimitive.Trigger data-slot="dropdown-menu-trigger" render={children as React.ReactElement} {...props} />
    );
  }
  return (
    <MenuPrimitive.Trigger data-slot="dropdown-menu-trigger" {...props}>
      {children}
    </MenuPrimitive.Trigger>
  );
}

function DropdownMenuContent({
  className,
  align = "start",
  side = "bottom",
  sideOffset = 4,
  children,
  ...props
}: React.ComponentProps<typeof MenuPrimitive.Popup> & {
  align?: MenuPrimitive.Positioner.Props["align"];
  side?: MenuPrimitive.Positioner.Props["side"];
  sideOffset?: number;
}) {
  const formPortal = useFormPortalContainer();
  return (
    <MenuPrimitive.Portal container={formPortal}>
      <MenuPrimitive.Positioner
        data-slot="dropdown-menu-positioner"
        side={side}
        align={align}
        sideOffset={sideOffset}
        className="z-50 max-h-(--available-height)"
      >
        <MenuPrimitive.Popup
          data-slot="dropdown-menu-content"
          className={cn(
            "origin-(--transform-origin) min-w-32 overflow-x-hidden overflow-y-auto ui-popup-surface bg-popover p-[5px] text-popover-foreground outline-none transition-[transform,scale,opacity] data-starting-style:scale-95 data-starting-style:opacity-0 data-ending-style:scale-95 data-ending-style:opacity-0",
            className,
          )}
          {...props}
        >
          {children}
        </MenuPrimitive.Popup>
      </MenuPrimitive.Positioner>
    </MenuPrimitive.Portal>
  );
}

function DropdownMenuGroup(props: React.ComponentProps<typeof MenuPrimitive.Group>) {
  return <MenuPrimitive.Group data-slot="dropdown-menu-group" {...props} />;
}

function DropdownMenuItem({
  className,
  inset,
  variant = "default",
  asChild,
  children,
  onSelect,
  onClick,
  ...props
}: Omit<React.ComponentProps<typeof MenuPrimitive.Item>, "onClick"> & {
  inset?: boolean;
  variant?: "default" | "destructive";
  asChild?: boolean;
  onSelect?: (event: Event) => void;
  onClick?: React.MouseEventHandler<HTMLDivElement>;
}) {
  const handleClick =
    onSelect || onClick
      ? (event: React.MouseEvent<HTMLDivElement>) => {
          onClick?.(event);
          onSelect?.(event.nativeEvent);
        }
      : undefined;
  const shared = {
    "data-slot": "dropdown-menu-item",
    "data-inset": inset,
    "data-variant": variant,
    onClick: handleClick,
    className: cn(
      itemBase,
      "data-[variant=destructive]:text-destructive data-[variant=destructive]:data-highlighted:bg-destructive/10 data-[variant=destructive]:data-highlighted:text-destructive",
      className,
    ),
    ...props,
  };
  if (asChild && React.isValidElement(children)) {
    return <MenuPrimitive.Item {...shared} render={children as React.ReactElement} />;
  }
  return <MenuPrimitive.Item {...shared}>{children}</MenuPrimitive.Item>;
}

function DropdownMenuCheckboxItem({
  className,
  children,
  checked,
  onSelect,
  onClick,
  ...props
}: Omit<React.ComponentProps<typeof MenuPrimitive.CheckboxItem>, "onClick"> & {
  onSelect?: (event: Event) => void;
  onClick?: React.MouseEventHandler<HTMLDivElement>;
}) {
  const handleClick =
    onSelect || onClick
      ? (event: React.MouseEvent<HTMLDivElement>) => {
          onClick?.(event);
          onSelect?.(event.nativeEvent);
        }
      : undefined;
  return (
    <MenuPrimitive.CheckboxItem
      data-slot="dropdown-menu-checkbox-item"
      className={cn(itemBase, "py-1.5 pr-8 pl-2", className)}
      checked={checked}
      onClick={handleClick}
      {...props}
    >
      <span
        className="pointer-events-none absolute right-2 flex items-center justify-center"
        data-slot="dropdown-menu-checkbox-item-indicator"
      >
        <MenuPrimitive.CheckboxItemIndicator>
          <Check weight="bold" />
        </MenuPrimitive.CheckboxItemIndicator>
      </span>
      {children}
    </MenuPrimitive.CheckboxItem>
  );
}

function DropdownMenuRadioGroup(props: React.ComponentProps<typeof MenuPrimitive.RadioGroup>) {
  return <MenuPrimitive.RadioGroup data-slot="dropdown-menu-radio-group" {...props} />;
}

function DropdownMenuRadioItem({
  className,
  children,
  onSelect,
  onClick,
  ...props
}: Omit<React.ComponentProps<typeof MenuPrimitive.RadioItem>, "onClick"> & {
  onSelect?: (event: Event) => void;
  onClick?: React.MouseEventHandler<HTMLDivElement>;
}) {
  const handleClick =
    onSelect || onClick
      ? (event: React.MouseEvent<HTMLDivElement>) => {
          onClick?.(event);
          onSelect?.(event.nativeEvent);
        }
      : undefined;
  return (
    <MenuPrimitive.RadioItem
      data-slot="dropdown-menu-radio-item"
      className={cn(itemBase, "py-1.5 pr-8 pl-2", className)}
      onClick={handleClick}
      {...props}
    >
      <span
        className="pointer-events-none absolute right-2 flex items-center justify-center"
        data-slot="dropdown-menu-radio-item-indicator"
      >
        <MenuPrimitive.RadioItemIndicator>
          <Check weight="bold" />
        </MenuPrimitive.RadioItemIndicator>
      </span>
      {children}
    </MenuPrimitive.RadioItem>
  );
}

function DropdownMenuLabel({
  className,
  inset,
  ...props
}: React.ComponentProps<typeof MenuPrimitive.GroupLabel> & { inset?: boolean }) {
  return (
    <MenuPrimitive.GroupLabel
      data-slot="dropdown-menu-label"
      data-inset={inset}
      className={cn("px-2 py-1.5 text-xs font-medium text-muted-foreground data-inset:pl-8", className)}
      {...props}
    />
  );
}

function DropdownMenuSeparator({ className, ...props }: React.ComponentProps<typeof MenuPrimitive.Separator>) {
  return (
    <MenuPrimitive.Separator
      data-slot="dropdown-menu-separator"
      className={cn("-mx-1 my-1 h-px bg-border", className)}
      {...props}
    />
  );
}

function DropdownMenuShortcut({ className, ...props }: React.ComponentProps<"span">) {
  return (
    <span
      data-slot="dropdown-menu-shortcut"
      className={cn(
        "ml-auto text-xs tracking-widest text-muted-foreground group-data-[highlighted]/dropdown-menu-item:text-accent-foreground",
        className,
      )}
      {...props}
    />
  );
}

function DropdownMenuSub(props: React.ComponentProps<typeof MenuPrimitive.SubmenuRoot>) {
  return <MenuPrimitive.SubmenuRoot data-slot="dropdown-menu-sub" {...props} />;
}

function DropdownMenuSubTrigger({
  className,
  inset,
  children,
  ...props
}: React.ComponentProps<typeof MenuPrimitive.SubmenuTrigger> & { inset?: boolean }) {
  return (
    <MenuPrimitive.SubmenuTrigger
      data-slot="dropdown-menu-sub-trigger"
      data-inset={inset}
      className={cn(itemBase, "data-popup-open:bg-accent data-popup-open:text-accent-foreground", className)}
      {...props}
    >
      {children}
      <ArrowRight weight="bold" className="ml-auto" />
    </MenuPrimitive.SubmenuTrigger>
  );
}

function DropdownMenuSubContent({
  className,
  align = "start",
  sideOffset = 0,
  children,
  ...props
}: React.ComponentProps<typeof MenuPrimitive.Popup> & {
  align?: MenuPrimitive.Positioner.Props["align"];
  sideOffset?: number;
}) {
  const formPortal = useFormPortalContainer();
  return (
    <MenuPrimitive.Portal container={formPortal}>
      <MenuPrimitive.Positioner
        data-slot="dropdown-menu-sub-positioner"
        align={align}
        sideOffset={sideOffset}
        className="z-50"
      >
        <MenuPrimitive.Popup
          data-slot="dropdown-menu-sub-content"
          className={cn(
            "origin-(--transform-origin) min-w-[8rem] overflow-hidden ui-popup-surface bg-popover p-[5px] text-popover-foreground outline-none transition-[transform,scale,opacity] data-starting-style:scale-95 data-starting-style:opacity-0 data-ending-style:scale-95 data-ending-style:opacity-0",
            className,
          )}
          {...props}
        >
          {children}
        </MenuPrimitive.Popup>
      </MenuPrimitive.Positioner>
    </MenuPrimitive.Portal>
  );
}

export {
  DropdownMenu,
  DropdownMenuPortal,
  DropdownMenuTrigger,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuLabel,
  DropdownMenuItem,
  DropdownMenuCheckboxItem,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuShortcut,
  DropdownMenuSub,
  DropdownMenuSubTrigger,
  DropdownMenuSubContent,
};
