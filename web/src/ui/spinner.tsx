// from hushbacks packages/ui/src/components/spinner.tsx
import { CircleNotch } from "@/ui/icons";
import { cn } from "@/ui/lib/utils";

function Spinner({ className, strokeWidth, ...props }: React.ComponentProps<"svg">) {
  return (
    <CircleNotch
      weight="bold"
      role="status"
      aria-label="Loading"
      className={cn("size-4 animate-spin", className)}
      {...props}
    />
  );
}

export { Spinner };
