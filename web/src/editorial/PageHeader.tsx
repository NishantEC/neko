// from ui-kit packages/ui/src/components/editorial/PageHeader.tsx @da20dfaeb (eyebrow dropped and the heading in plain sans, per the owner; size kept)
// Neko (2026-11-08, DESIGN.md §8): a page header is its title and, at most, an
// inline count. No subtitle, no eyebrow, no sentence explaining the page.
import type { ReactNode } from "react";

import { EditorialHeading } from "./EditorialHeading";

export interface PageHeaderProps {
  /** Ignored: kept so older call sites and plugins still type-check. */
  eyebrow?: ReactNode;
  title: ReactNode;
  /** A number beside the title, faint and tabular ("Tasks 24"). */
  count?: ReactNode;
}

export function PageHeader({ title, count }: PageHeaderProps) {
  return (
    <div data-slot="page-header" className="flex min-w-0 items-baseline gap-2.5">
      <EditorialHeading as="h1" className="truncate text-[20px]">
        {title}
      </EditorialHeading>
      {count !== undefined && count !== null && count !== "" ? (
        <span data-slot="page-count" className="text-[14px] tabular-nums text-muted-foreground">{count}</span>
      ) : null}
    </div>
  );
}
