// from hushbacks packages/ui/src/components/context-menu.tsx
import { ContextMenu as ContextMenuPrimitive } from "@base-ui/react/context-menu";
import { ArrowRight, Check } from "@/ui/icons";
import * as React from "react";

import { cn } from "@/ui/lib/utils";

const itemBase =
  "group/context-menu-item relative flex cursor-default items-center gap-2 rounded-sm px-2 py-1.5 text-sm outline-hidden select-none data-disabled:pointer-events-none data-disabled:opacity-50 data-inset:pl-8 data-highlighted:bg-accent data-highlighted:text-accent-foreground [&_svg:not([class*='size-'])]:size-4 [&_svg]:pointer-events-none [&_svg]:shrink-0";

function ContextMenu(props: React.ComponentProps<typeof ContextMenuPrimitive.Root>) {
  return <ContextMenuPrimitive.Root data-slot="context-menu" {...props} />;
}

function ContextMenuTrigger({
  className,
  asChild,
  children,
  ...props
}: React.ComponentProps<typeof ContextMenuPrimitive.Trigger> & { asChild?: boolean }) {
  if (asChild && React.isValidElement(children)) {
    return (
      <ContextMenuPrimitive.Trigger
        data-slot="context-menu-trigger"
        className={cn("select-none", className)}
        render={children as React.ReactElement}
        {...props}
      />
    );
  }
  return (
    <ContextMenuPrimitive.Trigger data-slot="context-menu-trigger" className={cn("select-none", className)} {...props}>
      {children}
    </ContextMenuPrimitive.Trigger>
  );
}

// Base UI's group label throws outside a group (error #31). The group marks
// itself, and a label outside one renders as a plain heading instead.
const InGroup = React.createContext(false);

function ContextMenuGroup(props: React.ComponentProps<typeof ContextMenuPrimitive.Group>) {
  return <InGroup.Provider value={true}>{<ContextMenuPrimitive.Group data-slot="context-menu-group" {...props} />}</InGroup.Provider>;
}

function ContextMenuPortal(props: React.ComponentProps<typeof ContextMenuPrimitive.Portal>) {
  return <ContextMenuPrimitive.Portal data-slot="context-menu-portal" {...props} />;
}

function ContextMenuSub(props: React.ComponentProps<typeof ContextMenuPrimitive.SubmenuRoot>) {
  return <ContextMenuPrimitive.SubmenuRoot data-slot="context-menu-sub" {...props} />;
}

function ContextMenuRadioGroup(props: React.ComponentProps<typeof ContextMenuPrimitive.RadioGroup>) {
  return <ContextMenuPrimitive.RadioGroup data-slot="context-menu-radio-group" {...props} />;
}

function ContextMenuContent({
  className,
  children,
  ...props
}: React.ComponentProps<typeof ContextMenuPrimitive.Popup>) {
  return (
    <ContextMenuPrimitive.Portal>
      <ContextMenuPrimitive.Positioner data-slot="context-menu-positioner" className="z-50 max-h-(--available-height)">
        <ContextMenuPrimitive.Popup
          data-slot="context-menu-content"
          className={cn(
            "origin-(--transform-origin) min-w-36 overflow-x-hidden overflow-y-auto rounded-md bg-popover p-1 text-popover-foreground smooth-shadow-ring-md outline-none transition-[transform,scale,opacity] data-starting-style:scale-95 data-starting-style:opacity-0 data-ending-style:scale-95 data-ending-style:opacity-0",
            className,
          )}
          {...props}
        >
          {children}
        </ContextMenuPrimitive.Popup>
      </ContextMenuPrimitive.Positioner>
    </ContextMenuPrimitive.Portal>
  );
}

function ContextMenuItem({
  className,
  inset,
  variant = "default",
  asChild,
  children,
  onSelect,
  onClick,
  ...props
}: Omit<React.ComponentProps<typeof ContextMenuPrimitive.Item>, "onClick"> & {
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
    "data-slot": "context-menu-item",
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
    return <ContextMenuPrimitive.Item {...shared} render={children as React.ReactElement} />;
  }
  return <ContextMenuPrimitive.Item {...shared}>{children}</ContextMenuPrimitive.Item>;
}

function ContextMenuSubTrigger({
  className,
  inset,
  children,
  ...props
}: React.ComponentProps<typeof ContextMenuPrimitive.SubmenuTrigger> & { inset?: boolean }) {
  return (
    <ContextMenuPrimitive.SubmenuTrigger
      data-slot="context-menu-sub-trigger"
      data-inset={inset}
      className={cn(itemBase, "data-popup-open:bg-accent data-popup-open:text-accent-foreground", className)}
      {...props}
    >
      {children}
      <ArrowRight weight="bold" className="ml-auto" />
    </ContextMenuPrimitive.SubmenuTrigger>
  );
}

function ContextMenuSubContent({
  className,
  children,
  ...props
}: React.ComponentProps<typeof ContextMenuPrimitive.Popup>) {
  return (
    <ContextMenuPrimitive.Portal>
      <ContextMenuPrimitive.Positioner data-slot="context-menu-sub-positioner" className="z-50">
        <ContextMenuPrimitive.Popup
          data-slot="context-menu-sub-content"
          className={cn(
            "origin-(--transform-origin) min-w-32 overflow-hidden rounded-md bg-popover p-1 text-popover-foreground smooth-shadow-ring-lg outline-none transition-[transform,scale,opacity] data-starting-style:scale-95 data-starting-style:opacity-0 data-ending-style:scale-95 data-ending-style:opacity-0",
            className,
          )}
          {...props}
        >
          {children}
        </ContextMenuPrimitive.Popup>
      </ContextMenuPrimitive.Positioner>
    </ContextMenuPrimitive.Portal>
  );
}

function ContextMenuCheckboxItem({
  className,
  children,
  checked,
  onSelect,
  onClick,
  ...props
}: Omit<React.ComponentProps<typeof ContextMenuPrimitive.CheckboxItem>, "onClick"> & {
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
    <ContextMenuPrimitive.CheckboxItem
      data-slot="context-menu-checkbox-item"
      className={cn(itemBase, "py-1.5 pr-8 pl-2", className)}
      checked={checked}
      onClick={handleClick}
      {...props}
    >
      <span className="pointer-events-none absolute right-2 flex items-center justify-center">
        <ContextMenuPrimitive.CheckboxItemIndicator>
          <Check weight="bold" />
        </ContextMenuPrimitive.CheckboxItemIndicator>
      </span>
      {children}
    </ContextMenuPrimitive.CheckboxItem>
  );
}

function ContextMenuRadioItem({
  className,
  children,
  onSelect,
  onClick,
  ...props
}: Omit<React.ComponentProps<typeof ContextMenuPrimitive.RadioItem>, "onClick"> & {
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
    <ContextMenuPrimitive.RadioItem
      data-slot="context-menu-radio-item"
      className={cn(itemBase, "py-1.5 pr-8 pl-2", className)}
      onClick={handleClick}
      {...props}
    >
      <span className="pointer-events-none absolute right-2 flex items-center justify-center">
        <ContextMenuPrimitive.RadioItemIndicator>
          <Check weight="bold" />
        </ContextMenuPrimitive.RadioItemIndicator>
      </span>
      {children}
    </ContextMenuPrimitive.RadioItem>
  );
}

function ContextMenuLabel({
  className,
  inset,
  ...props
}: React.ComponentProps<typeof ContextMenuPrimitive.GroupLabel> & { inset?: boolean }) {
  const inGroup = React.useContext(InGroup);
  const classes = cn("px-2 py-1.5 text-xs font-medium text-muted-foreground data-inset:pl-8", className);
  if (!inGroup) {
    return <div role="presentation" data-slot="context-menu-label" data-inset={inset} className={classes} {...(props as React.ComponentProps<"div">)} />;
  }
  return <ContextMenuPrimitive.GroupLabel data-slot="context-menu-label" data-inset={inset} className={classes} {...props} />;
}

function ContextMenuSeparator({ className, ...props }: React.ComponentProps<typeof ContextMenuPrimitive.Separator>) {
  return (
    <ContextMenuPrimitive.Separator
      data-slot="context-menu-separator"
      className={cn("-mx-1 my-1 h-px bg-border", className)}
      {...props}
    />
  );
}

function ContextMenuShortcut({ className, ...props }: React.ComponentProps<"span">) {
  return (
    <span
      data-slot="context-menu-shortcut"
      className={cn(
        "ml-auto text-xs tracking-widest text-muted-foreground group-data-[highlighted]/context-menu-item:text-accent-foreground",
        className,
      )}
      {...props}
    />
  );
}

export {
  ContextMenu,
  ContextMenuTrigger,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuCheckboxItem,
  ContextMenuRadioItem,
  ContextMenuLabel,
  ContextMenuSeparator,
  ContextMenuShortcut,
  ContextMenuGroup,
  ContextMenuPortal,
  ContextMenuSub,
  ContextMenuSubContent,
  ContextMenuSubTrigger,
  ContextMenuRadioGroup,
};
