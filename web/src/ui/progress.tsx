// from hushbacks packages/ui/src/components/progress.tsx
import { Progress as ProgressPrimitive } from "@base-ui/react/progress";
import type * as React from "react";

import { cn } from "@/ui/lib/utils";

function Progress({ className, value, ...props }: React.ComponentProps<typeof ProgressPrimitive.Root>) {
  return (
    <ProgressPrimitive.Root data-slot="progress" value={value} {...props}>
      <ProgressPrimitive.Track
        data-slot="progress-track"
        className={cn("bg-muted h-1.5 rounded-full relative w-full overflow-x-hidden", className)}
      >
        <ProgressPrimitive.Indicator data-slot="progress-indicator" className="bg-primary h-full transition-all" />
      </ProgressPrimitive.Track>
    </ProgressPrimitive.Root>
  );
}

export { Progress };
