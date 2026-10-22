// The frame the work pages share: the eyebrow → heading → subtitle voice from
// the design language, at the size a working page wants, with the page's
// controls to its right.
import type { ReactNode } from "react";
import { cn } from "@/ui/lib/utils";
import { EditorialHeading } from "@/editorial";
import { Skeleton } from "@/ui/skeleton";

export function WorkHeader({ title, subtitle, actions, children, className }: {
  className?: string;
  eyebrow: ReactNode;
  title: ReactNode;
  subtitle?: ReactNode;
  actions?: ReactNode;
  children?: ReactNode;
}) {
  return (
    <header data-slot="work-header" className={cn("shrink-0 px-6 pt-8 pb-5 md:px-10", className)}>
      <div className="flex flex-wrap items-end gap-x-6 gap-y-3">
        <div className="flex min-w-0 flex-col gap-2">
          <EditorialHeading as="h1" className="text-[22px]">{title}</EditorialHeading>
          {subtitle ? <p data-slot="work-subtitle" className="text-[13.5px] text-muted-foreground">{subtitle}</p> : null}
        </div>
        <div className="ms-auto flex flex-wrap items-center gap-2">{actions}</div>
      </div>
      {children}
    </header>
  );
}

/** What a work page shows until the first snapshot arrives. */
export function WorkSkeleton({ rows = 8 }: { rows?: number }) {
  return (
    <div data-slot="work-skeleton" aria-busy="true" aria-label="Loading" role="status" className="mx-auto w-full max-w-[1040px] space-y-3 px-6 pt-4 md:px-10">
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
