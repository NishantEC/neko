// from hushbacks apps/spa/src/pages/Builder/components/ResponseDetailPanel.skeletons.tsx @5d9a64b11
// (Bar and Pill from apps/spa/src/components/RouteSkeletons/primitives.tsx, SkeletonRegion
// from pages/Settings/settings.skeletons.tsx). Adapted: the answer rows became a
// conversation's shape (a bubble on the right, prose on the left).
import { cn } from "@/ui/lib/utils";

function Bar({ className, soft }: { className?: string; soft?: boolean }) {
  return <span aria-hidden className={cn("sky-skel h-3.5 rounded-full", soft && "sky-skel-soft", className)} />;
}

function Pill({ className }: { className?: string }) {
  return <span aria-hidden className={cn("sky-skel h-10 shrink-0 rounded-[24px]", className)} />;
}

const TURNS = [
  { key: "t1", mine: true, lines: ["w-[58%]"] },
  { key: "t2", mine: false, lines: ["w-[86%]", "w-[72%]", "w-[44%]"] },
  { key: "t3", mine: false, lines: ["w-[64%]", "w-[80%]"] },
] as const;

export function TaskSheetSkeleton() {
  return (
    <div aria-busy="true" className="flex min-h-0 flex-1 flex-col overflow-hidden">
      <output className="sr-only">Loading task</output>
      <div className="shrink-0 space-y-2 px-6 pt-2 pb-5">
        <div className="flex h-7 items-center">
          <Bar className="h-5 w-56" />
        </div>
        <div className="flex items-center gap-3">
          <Pill className="h-6 w-24" />
          <Bar soft className="h-2.5 w-32" />
        </div>
      </div>
      <div className="shrink-0 px-6 pb-2">
        <Pill className="h-9 w-64" />
      </div>
      {TURNS.map((turn) => (
        <div key={turn.key} className={cn("flex flex-col gap-2 px-6 py-[18px]", turn.mine && "items-end")}>
          {turn.lines.map((width, i) => (
            <Bar key={i} soft={!turn.mine} className={cn("h-3.5", width)} />
          ))}
        </div>
      ))}
    </div>
  );
}
