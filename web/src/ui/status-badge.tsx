// from hushbacks packages/ui/src/components/status-badge.tsx
import { cn } from "@/ui/lib/utils";
import type { ComponentProps } from "react";

type StatusBadgeProps = ComponentProps<"span"> & {
  tone?: "neutral" | "success" | "warning" | "error";
};

export function StatusBadge({ tone = "neutral", className, ...props }: StatusBadgeProps) {
  return (
    <span
      data-slot="status-badge"
      data-tone={tone}
      className={cn(
        "ui-status ui-round inline-flex items-center gap-1.5 whitespace-nowrap rounded-md px-[7px] py-[3px] text-[11px] font-medium leading-[17px]",
        className,
      )}
      {...props}
    />
  );
}
