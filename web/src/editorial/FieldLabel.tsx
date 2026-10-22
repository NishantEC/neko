// from hushbacks packages/ui/src/components/editorial/FieldLabel.tsx
import { cn } from "@/ui/lib/utils";
import type { ReactNode } from "react";

export interface FieldLabelProps {
  children: ReactNode;
  className?: string;
}

export function FieldLabel({ children, className }: FieldLabelProps) {
  return (
    <span className={cn("text-[14px] font-medium text-foreground", className)}>
      {children}
    </span>
  );
}
