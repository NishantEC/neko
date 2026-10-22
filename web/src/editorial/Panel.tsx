// from hushbacks packages/ui/src/components/editorial/Panel.tsx
import { cn } from "@/ui/lib/utils";
import type { ReactNode } from "react";

export interface PanelProps {
  children: ReactNode;
  className?: string;
}

export function Panel({ children, className }: PanelProps) {
  return <div className={cn("overflow-hidden rounded-2xl border border-border bg-card", className)}>{children}</div>;
}
