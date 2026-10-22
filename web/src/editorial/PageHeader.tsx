// from hushbacks packages/ui/src/components/editorial/PageHeader.tsx
import type { ReactNode } from "react";

import { EditorialHeading } from "./EditorialHeading";
import { MonoLabel } from "./MonoLabel";

export interface PageHeaderProps {
  eyebrow: ReactNode;
  title: ReactNode;
  subtitle?: ReactNode;
}

export function PageHeader({ eyebrow, title, subtitle }: PageHeaderProps) {
  return (
    <div className="flex flex-col gap-2">
      <MonoLabel className="text-foreground/45">{eyebrow}</MonoLabel>
      <EditorialHeading as="h1" className="text-[clamp(1.75rem,2.5vw+0.5rem,2.5rem)]">
        {title}
      </EditorialHeading>
      {subtitle ? <p className="text-muted-foreground">{subtitle}</p> : null}
    </div>
  );
}
