// from hushbacks packages/ui/src/components/input.tsx
import type * as React from "react";

import { cn } from "@/ui/lib/utils";

function Input({
  className,
  type,
  appearance = "refined",
  ...props
}: React.ComponentProps<"input"> & { appearance?: "refined" | "respondent" }) {
  return (
    <input
      type={type}
      data-slot="input"
      data-appearance={appearance}
      className={cn(
        appearance === "respondent"
          ? "dark:bg-input/30 border-input focus-visible:border-ring focus-visible:ring-ring/50 aria-invalid:ring-destructive/20 dark:aria-invalid:ring-destructive/40 aria-invalid:border-destructive dark:aria-invalid:border-destructive/50 h-9 rounded-md border bg-transparent px-2.5 py-1 text-base shadow-xs transition-[color,background-color,border-color,box-shadow] file:h-7 file:text-sm file:font-medium focus-visible:ring-[3px] aria-invalid:ring-[3px] md:text-sm file:text-foreground placeholder:text-muted-foreground w-full min-w-0 outline-none file:inline-flex file:border-0 file:bg-transparent disabled:pointer-events-none disabled:cursor-not-allowed disabled:opacity-50"
          : "ui-inset-control h-11 sm:h-10 border px-3 py-1 text-base sm:text-[13px] file:h-7 file:text-sm file:font-medium file:text-foreground w-full min-w-0 file:inline-flex file:border-0 file:bg-transparent disabled:pointer-events-none placeholder:text-muted-foreground outline-none disabled:cursor-not-allowed disabled:opacity-50",
        className,
      )}
      {...props}
    />
  );
}

export { Input };
