// from hushbacks packages/ui/src/components/editorial/PageHeader.tsx @da20dfaeb (eyebrow dropped and the heading in plain sans, per the owner; size kept)
import type { ReactNode } from "react";

import { EditorialHeading } from "./EditorialHeading";

export interface PageHeaderProps {
  eyebrow: ReactNode;
  title: ReactNode;
  subtitle?: ReactNode;
}

export function PageHeader({ title, subtitle }: PageHeaderProps) {
  return (
    <div className="flex flex-col gap-2">
      <EditorialHeading as="h1" className="text-[22px]">
        {title}
      </EditorialHeading>
      {subtitle ? <p className="text-[13.5px] text-muted-foreground">{subtitle}</p> : null}
    </div>
  );
}
