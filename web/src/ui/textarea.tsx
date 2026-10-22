// from hushbacks packages/ui/src/components/textarea.tsx
import type * as React from "react";

import { cn } from "@/ui/lib/utils";

function Textarea({
  className,
  appearance = "refined",
  ...props
}: React.ComponentProps<"textarea"> & { appearance?: "refined" | "respondent" }) {
  return (
    <textarea
      data-slot="textarea"
      data-appearance={appearance}
      className={cn(
        appearance === "respondent"
          ? "border-input focus-visible:border-ring focus-visible:ring-ring/50 aria-invalid:ring-destructive/20 dark:aria-invalid:ring-destructive/40 aria-invalid:border-destructive dark:aria-invalid:border-destructive/50 rounded-md border bg-transparent px-2.5 py-2 text-base shadow-xs transition-[color,background-color,border-color,box-shadow] focus-visible:ring-[3px] aria-invalid:ring-[3px] md:text-sm placeholder:text-muted-foreground flex field-sizing-content min-h-16 w-full outline-none disabled:cursor-not-allowed disabled:opacity-50"
          : "ui-inset-control flex field-sizing-content min-h-24 w-full border px-3 py-2.5 text-base sm:text-[13px] placeholder:text-muted-foreground outline-none disabled:cursor-not-allowed disabled:opacity-50",
        className,
      )}
      {...props}
    />
  );
}

export { Textarea };
