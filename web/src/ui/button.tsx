// from hushbacks packages/ui/src/components/button.tsx
import { mergeProps } from "@base-ui/react/merge-props";
import { useRender } from "@base-ui/react/use-render";
import { cva, type VariantProps } from "class-variance-authority";
import * as React from "react";

import { Spinner } from "@/ui/spinner";
import { cn } from "@/ui/lib/utils";

const buttonVariants = cva(
  "relative inline-flex shrink-0 cursor-pointer items-center justify-center gap-2 whitespace-nowrap rounded-lg border font-medium text-base outline-none transition-shadow before:pointer-events-none before:absolute before:inset-0 before:rounded-[inherit] pointer-coarse:after:absolute pointer-coarse:after:size-full pointer-coarse:after:min-h-11 pointer-coarse:after:min-w-11 focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-1 focus-visible:ring-offset-background disabled:pointer-events-none disabled:opacity-64 data-loading:select-none data-loading:text-transparent sm:text-sm [&_svg:not([class*='opacity-'])]:opacity-80 [&_svg:not([class*='size-'])]:size-4.5 sm:[&_svg:not([class*='size-'])]:size-4 [&_svg]:pointer-events-none [&_svg]:-mx-0.5 [&_svg]:shrink-0",
  {
    defaultVariants: {
      size: "default",
      variant: "default",
    },
    variants: {
      size: {
        // Sizes follow hushbacks' running app (Sky layer: control 40, row 36, xs 32), not the
        // raw shadcn defaults (32/28/24). Same at every breakpoint: no sm: shrink.
        default: "h-10 px-4",
        icon: "size-10",
        "icon-lg": "size-10",
        "icon-sm": "size-9",
        "icon-xl": "size-11 [&_svg:not([class*='size-'])]:size-5",
        "icon-xs":
          "size-7 rounded-md before:rounded-[inherit] not-in-data-[slot=input-group]:[&_svg:not([class*='size-'])]:size-4",
        lg: "h-10 px-4",
        sm: "h-9 gap-1.5 px-3.5",
        xl: "h-11 px-[calc(--spacing(4.5)-1px)] text-base [&_svg:not([class*='size-'])]:size-5",
        xs: "h-8 gap-1 rounded-md px-2.5 text-sm before:rounded-[inherit] [&_svg:not([class*='size-'])]:size-4",
      },
      variant: {
        default:
          "not-disabled:inset-shadow-[0_1px_--theme(--color-white/16%)] border-primary bg-primary text-primary-foreground shadow-primary/24 shadow-xs hover:bg-primary/90 data-pressed:bg-primary/90 *:data-[slot=button-loading-indicator]:text-primary-foreground [:active,[data-pressed]]:inset-shadow-[0_1px_--theme(--color-black/8%)] [:disabled,:active,[data-pressed]]:shadow-none",
        destructive:
          "not-disabled:inset-shadow-[0_1px_--theme(--color-white/16%)] border-destructive bg-destructive text-white shadow-destructive/24 shadow-xs hover:bg-destructive/90 data-pressed:bg-destructive/90 *:data-[slot=button-loading-indicator]:text-white [:active,[data-pressed]]:inset-shadow-[0_1px_--theme(--color-black/8%)] [:disabled,:active,[data-pressed]]:shadow-none",
        "destructive-outline":
          "border-input bg-popover not-dark:bg-clip-padding text-destructive-foreground shadow-xs/5 not-disabled:not-active:not-data-pressed:before:shadow-[0_1px_--theme(--color-black/4%)] hover:border-destructive/32 hover:bg-destructive/4 data-pressed:border-destructive/32 data-pressed:bg-destructive/4 *:data-[slot=button-loading-indicator]:text-foreground dark:bg-input/32 dark:not-disabled:before:shadow-[0_-1px_--theme(--color-white/2%)] dark:not-disabled:not-active:not-data-pressed:before:shadow-[0_-1px_--theme(--color-white/6%)] [:disabled,:active,[data-pressed]]:shadow-none",
        ghost:
          "border-transparent text-foreground hover:bg-accent data-pressed:bg-accent *:data-[slot=button-loading-indicator]:text-foreground",
        link: "border-transparent text-foreground underline-offset-4 hover:underline data-pressed:underline *:data-[slot=button-loading-indicator]:text-foreground",
        outline:
          "border-input bg-popover not-dark:bg-clip-padding text-foreground shadow-xs/5 not-disabled:not-active:not-data-pressed:before:shadow-[0_1px_--theme(--color-black/4%)] hover:bg-accent/50 data-pressed:bg-accent/50 *:data-[slot=button-loading-indicator]:text-foreground dark:bg-input/32 dark:data-pressed:bg-input/64 dark:hover:bg-input/64 dark:not-disabled:before:shadow-[0_-1px_--theme(--color-white/2%)] dark:not-disabled:not-active:not-data-pressed:before:shadow-[0_-1px_--theme(--color-white/6%)] [:disabled,:active,[data-pressed]]:shadow-none",
        secondary:
          "border-transparent bg-secondary text-secondary-foreground hover:bg-secondary/90 data-pressed:bg-secondary/90 *:data-[slot=button-loading-indicator]:text-secondary-foreground [:active,[data-pressed]]:bg-secondary/80",
      },
    },
  },
);

interface ButtonProps extends useRender.ComponentProps<"button"> {
  variant?: VariantProps<typeof buttonVariants>["variant"];
  size?: VariantProps<typeof buttonVariants>["size"];
  loading?: boolean;
  asChild?: boolean;
}

function Button({
  className,
  variant,
  size,
  render,
  asChild,
  children,
  loading = false,
  disabled: disabledProp,
  ...props
}: ButtonProps) {
  const isDisabled = Boolean(loading || disabledProp);
  const useAsChild = asChild && React.isValidElement(children);
  const resolvedRender = render ?? (useAsChild ? (children as React.ReactElement) : undefined);
  const typeValue: React.ButtonHTMLAttributes<HTMLButtonElement>["type"] = resolvedRender ? undefined : "button";

  const defaultProps = {
    ...(useAsChild
      ? {}
      : {
          children: (
            <>
              {children}
              {loading && <Spinner className="pointer-events-none absolute" data-slot="button-loading-indicator" />}
            </>
          ),
        }),
    "aria-disabled": loading || undefined,
    className: cn(buttonVariants({ className, size, variant })),
    "data-loading": loading ? "" : undefined,
    "data-size": size ?? "default",
    "data-slot": "button",
    "data-variant": variant ?? "default",
    disabled: isDisabled,
    type: typeValue,
  };

  return useRender({
    defaultTagName: "button",
    props: mergeProps<"button">(defaultProps, props),
    render: resolvedRender,
  });
}

export { Button, buttonVariants };
