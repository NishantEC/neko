// from hushbacks apps/spa/src/components/PageContainer.tsx @da20dfaeb
// Neko: `...rest` reaches the scrolling surface, so a page keeps its data-slot
// (the verify loop scrolls `[data-slot='setup-page']`).
import { cn } from "@/ui/lib/utils";
import type { ComponentProps, ReactNode } from "react";

/**
 * Standard page container for every `_app/*` route.
 *
 * Locks horizontal rhythm (max-w-[1200px], px-6 md:px-10) and the
 * default vertical rhythm (pt-12 md:pt-16, pb-24) so dashboard,
 * forms, settings, etc. all sit on the same column. Use the
 * `padding="hero"` variant for the dashboard's taller hero gutter.
 */
export function PageContainer({
  children,
  className,
  backdrop,
  padding = "default",
  scroll = true,
  ...rest
}: {
  children: ReactNode;
  className?: string;
  /** Rendered behind the column, filling the whole surface. A backdrop passed
   *  as a child would be trapped inside the max-width column and show a gutter
   *  either side of it. */
  backdrop?: ReactNode;
  padding?: "default" | "hero" | "none";
  /** Whether the wrapping surface scrolls. Set false for routes that
   *  manage their own scroll. */
  scroll?: boolean;
} & Omit<ComponentProps<"div">, "children" | "className">) {
  const padY = padding === "hero" ? "pt-16 md:pt-24 pb-32" : padding === "none" ? "" : "pt-12 md:pt-16 pb-24";

  return (
    <div {...rest} className={cn("flex-1 h-full bg-background relative overflow-x-hidden", scroll && "overflow-y-auto")}>
      {backdrop}
      <div className={cn("relative z-10 mx-auto w-full max-w-[1200px] px-6 md:px-10", padY, className)}>{children}</div>
    </div>
  );
}
