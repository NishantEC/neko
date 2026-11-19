// from ui-kit packages/ui/src/components/button.tsx @da20dfaeb
// Design system 2026-11 (4.1): 40 high at radius 24 (sm 36, xs 32, xl 44), text 14/500, no border; `default` is the ink (the one primary action),
// outline and secondary are pebbles, ghost is transparent. The gradients and the lit top edge are painted by styles/sky-materials.css.
import { mergeProps } from "@base-ui/react/merge-props";
import { useRender } from "@base-ui/react/use-render";
import { cva, type VariantProps } from "class-variance-authority";
import * as React from "react";

import { Spinner } from "@/ui/spinner";
import { cn } from "@/ui/lib/utils";

const buttonVariants = cva(
  "relative inline-flex shrink-0 cursor-pointer items-center justify-center gap-2 whitespace-nowrap rounded-3xl border border-transparent text-sm font-medium outline-none transition-[transform,opacity] duration-100 before:pointer-events-none before:absolute before:inset-0 before:rounded-[inherit] pointer-coarse:after:absolute pointer-coarse:after:size-full pointer-coarse:after:min-h-11 pointer-coarse:after:min-w-11 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring disabled:pointer-events-none disabled:opacity-50 data-loading:select-none data-loading:text-transparent [&_svg:not([class*='opacity-'])]:opacity-80 [&_svg:not([class*='size-'])]:size-4 [&_svg]:pointer-events-none [&_svg]:-mx-0.5 [&_svg]:shrink-0",
  {
    defaultVariants: {
      size: "default",
      variant: "default",
    },
    variants: {
      size: {
        default: "h-10 px-4",
        icon: "size-10",
        "icon-lg": "size-10",
        "icon-sm": "size-9",
        "icon-xl": "size-11 [&_svg:not([class*='size-'])]:size-5",
        "icon-xs": "size-7 [&_svg:not([class*='size-'])]:size-4",
        lg: "h-10 px-4",
        sm: "h-9 gap-1.5 px-3.5",
        xl: "h-11 px-[18px] [&_svg:not([class*='size-'])]:size-5",
        xs: "h-8 gap-1 rounded-2xl px-2.5 before:rounded-[inherit]",
      },
      variant: {
        default:
          "bg-primary text-primary-foreground *:data-[slot=button-loading-indicator]:text-primary-foreground",
        destructive:
          "bg-destructive text-white *:data-[slot=button-loading-indicator]:text-white",
        "destructive-outline":
          "bg-secondary text-destructive-foreground *:data-[slot=button-loading-indicator]:text-foreground",
        ghost:
          "text-foreground hover:bg-accent data-pressed:bg-accent *:data-[slot=button-loading-indicator]:text-foreground",
        link: "text-foreground underline-offset-4 hover:underline data-pressed:underline *:data-[slot=button-loading-indicator]:text-foreground",
        outline:
          "bg-secondary text-foreground *:data-[slot=button-loading-indicator]:text-foreground",
        secondary:
          "bg-secondary text-secondary-foreground *:data-[slot=button-loading-indicator]:text-secondary-foreground",
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
