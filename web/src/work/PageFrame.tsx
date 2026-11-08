// The frame the work pages share: the title and an inline count (DESIGN.md
// §8: no subtitle, no eyebrow), with the page's one primary action and its
// overflow menu to the right.
import type { ReactNode } from "react";
import { cn } from "@/ui/lib/utils";
import { PageHeader } from "@/editorial";
import { Skeleton } from "@/ui/skeleton";

export function WorkHeader({ title, count, actions, children, className }: {
  className?: string;
  /** Ignored (no eyebrows); kept so call sites still type-check. */
  eyebrow?: ReactNode;
  title: ReactNode;
  count?: ReactNode;
  actions?: ReactNode;
  children?: ReactNode;
}) {
  return (
    <header data-slot="work-header" className={cn("shrink-0 px-7 pt-6 pb-4", className)}>
      <div className="flex min-h-9 flex-wrap items-center gap-x-6 gap-y-3">
        <PageHeader title={title} count={count} />
        <div className="ms-auto flex flex-wrap items-center gap-1.5">{actions}</div>
      </div>
      {children}
    </header>
  );
}

/** What a work page shows until the first snapshot arrives. */
export function WorkSkeleton({ rows = 8 }: { rows?: number }) {
  return (
    <div data-slot="work-skeleton" aria-busy="true" aria-label="Loading" role="status" className="mx-auto w-full max-w-[1040px] space-y-3 px-7 pt-4">
      {Array.from({ length: rows }, (_, i) => (
        <div key={i} className="flex items-center gap-3 px-3">
          <Skeleton className="size-3.5 rounded-full" />
          <Skeleton className="h-4" style={{ width: `${38 + ((i * 17) % 40)}%` }} />
          <span className="flex-1" />
          <Skeleton className="h-4 w-16" />
        </div>
      ))}
    </div>
  );
}
