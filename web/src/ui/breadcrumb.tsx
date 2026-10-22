// from hushbacks packages/ui/src/components/breadcrumb.tsx
import { mergeProps } from "@base-ui/react/merge-props";
import { useRender } from "@base-ui/react/use-render";
import { CaretRight, DotsThreeOutline } from "@/ui/icons";
import * as React from "react";

import { cn } from "@/ui/lib/utils";

function Breadcrumb({ className, ...props }: React.ComponentProps<"nav">) {
  return <nav aria-label="breadcrumb" data-slot="breadcrumb" className={cn(className)} {...props} />;
}

function BreadcrumbList({ className, ...props }: React.ComponentProps<"ol">) {
  return (
    <ol
      data-slot="breadcrumb-list"
      className={cn(
        "text-muted-foreground gap-1.5 text-sm sm:gap-2.5 flex flex-wrap items-center break-words",
        className,
      )}
      {...props}
    />
  );
}

function BreadcrumbItem({ className, ...props }: React.ComponentProps<"li">) {
  return <li data-slot="breadcrumb-item" className={cn("gap-1.5 inline-flex items-center", className)} {...props} />;
}

function BreadcrumbLink({
  asChild,
  className,
  render,
  children,
  ...props
}: useRender.ComponentProps<"a"> & {
  asChild?: boolean;
}) {
  const useAsChild = asChild && React.isValidElement(children);
  const resolvedRender = render ?? (useAsChild ? (children as React.ReactElement) : undefined);
  const defaultProps = {
    "data-slot": "breadcrumb-link",
    className: cn("hover:text-foreground transition-colors", className),
    ...(useAsChild ? {} : { children }),
  };
  return useRender({
    defaultTagName: "a",
    render: resolvedRender,
    props: mergeProps<"a">(defaultProps, props),
  });
}

function BreadcrumbPage({ className, ...props }: React.ComponentProps<"span">) {
  return (
    <span
      data-slot="breadcrumb-page"
      role="link"
      aria-disabled="true"
      aria-current="page"
      className={cn("text-foreground font-normal", className)}
      {...props}
    />
  );
}

function BreadcrumbSeparator({ children, className, ...props }: React.ComponentProps<"li">) {
  return (
    <li
      data-slot="breadcrumb-separator"
      role="presentation"
      aria-hidden="true"
      className={cn("[&>svg]:size-3.5", className)}
      {...props}
    >
      {children ?? <CaretRight weight="bold" />}
    </li>
  );
}

function BreadcrumbEllipsis({ className, ...props }: React.ComponentProps<"span">) {
  return (
    <span
      data-slot="breadcrumb-ellipsis"
      role="presentation"
      aria-hidden="true"
      className={cn("size-5 [&>svg]:size-4 flex items-center justify-center", className)}
      {...props}
    >
      <DotsThreeOutline weight="bold" />
      <span className="sr-only">More</span>
    </span>
  );
}

export {
  Breadcrumb,
  BreadcrumbList,
  BreadcrumbItem,
  BreadcrumbLink,
  BreadcrumbPage,
  BreadcrumbSeparator,
  BreadcrumbEllipsis,
};
