// from hushbacks packages/ui/src/components/skeleton.tsx
import { cn } from "@/ui/lib/utils";

function Skeleton({ className, ...props }: React.ComponentProps<"div">) {
  return <div data-slot="skeleton" className={cn("bg-muted rounded-md animate-pulse", className)} {...props} />;
}

export { Skeleton };
