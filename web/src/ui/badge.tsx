// from hushbacks packages/ui/src/components/badge.tsx
import { mergeProps } from "@base-ui/react/merge-props";
import { useRender } from "@base-ui/react/use-render";
import { cva, type VariantProps } from "class-variance-authority";
import * as React from "react";

import { cn } from "@/ui/lib/utils";

const badgeVariants = cva(
  "inline-flex items-center rounded-full border border-border/100 dark:border-border/20 text-xs font-semibold transition-colors focus:outline-hidden focus:ring-2 focus:ring-ring focus:ring-offset-2 gap-2",
  {
    variants: {
      variant: {
        default: "border-transparent bg-primary text-primary-foreground dark:shadow-sm dark:border-transparent",
        brand: "border-transparent bg-brand text-primary-foreground dark:shadow-sm dark:border-transparent",
        "brand-secondary": "border-transparent bg-brand-foreground/20 text-brand dark:border-transparent",
        secondary: "border-transparent bg-secondary text-secondary-foreground dark:shadow-sm dark:border-transparent",
        destructive:
          "border-transparent bg-destructive/30 text-destructive-foreground dark:shadow-sm dark:border-transparent",
        outline: "text-foreground",
        ghost: "hover:bg-muted hover:text-muted-foreground dark:hover:bg-muted/50",
        link: "text-primary underline-offset-4 hover:underline",
      },
      size: {
        default: "px-2.5 py-1",
        sm: "px-1",
      },
    },
    defaultVariants: {
      variant: "default",
      size: "default",
    },
  },
);

interface BadgeProps extends useRender.ComponentProps<"span">, VariantProps<typeof badgeVariants> {
  asChild?: boolean;
}

function Badge({ className, variant, size, asChild, render, children, ...props }: BadgeProps) {
  const useAsChild = asChild && React.isValidElement(children);
  const resolvedRender = render ?? (useAsChild ? (children as React.ReactElement) : undefined);
  const defaultProps = {
    "data-slot": "badge",
    className: cn(badgeVariants({ variant, size }), className),
    ...(useAsChild ? {} : { children }),
  };
  return useRender({
    defaultTagName: "span",
    render: resolvedRender,
    props: mergeProps<"span">(defaultProps, props),
  });
}

export { Badge, badgeVariants };
