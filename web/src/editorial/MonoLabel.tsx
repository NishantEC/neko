// from hushbacks packages/ui/src/components/editorial/MonoLabel.tsx
import { cn } from "@/ui/lib/utils";
import type { ComponentPropsWithoutRef, ReactNode } from "react";

export interface MonoLabelProps extends Omit<ComponentPropsWithoutRef<"span">, "children"> {
  children: ReactNode;
}

export const MonoLabel: React.FC<MonoLabelProps> = ({ className, children, ...rest }) => (
  <span
    {...rest}
    className={cn(
      "inline-block font-mono text-[11px] font-medium uppercase tracking-[0.18em] text-muted-foreground",
      className,
    )}
  >
    {children}
  </span>
);
