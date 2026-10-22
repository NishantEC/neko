// from hushbacks packages/ui/src/components/editorial/FieldLabel.tsx
import { cn } from "@/ui/lib/utils";
import type { ReactNode } from "react";

export interface FieldLabelProps {
  children: ReactNode;
  className?: string;
}

export function FieldLabel({ children, className }: FieldLabelProps) {
  return (
    <span className={cn("font-mono text-[11px] uppercase tracking-[0.14em] text-muted-foreground", className)}>
      {children}
    </span>
  );
}
