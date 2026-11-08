// The stage filter as chips (BoardUI's Chip, used as a toggle): All, then one per stage with its
// count. One is on at a time; pressing the one that is on goes back to All.
import { cn } from "@/ui/lib/utils";
import { Tabs, TabsList, TabsTab } from "@/ui/tabs";
import { STAGE_TITLE, STAGES, type Stage } from "@/work/model";
import { STAGE_TONE, TONE_DOT } from "@/work/tone";

export function StageFilters({ stage, counts, onChange }: { stage: Stage | "all"; counts: Record<Stage, number>; onChange: (stage: Stage | "all") => void }) {
  const chip = (on: boolean) =>
    cn(
      "inline-flex h-7 cursor-pointer items-center gap-1.5 rounded-lg border px-2.5 text-[13px] outline-none transition-colors duration-150",
      "focus-visible:ring-2 focus-visible:ring-ring",
      on
        ? "border-foreground/15 bg-foreground/[0.08] font-medium text-strong active:bg-foreground/[0.12]"
        : "border-border text-muted-foreground hover:bg-foreground/[0.04] hover:text-foreground active:bg-foreground/[0.07]",
    );
  return (
    <div data-slot="stage-filters" role="group" aria-label="Show tasks in" className="flex flex-wrap items-center gap-1.5">
      <button type="button" aria-pressed={stage === "all"} aria-label="All tasks" className={chip(stage === "all")} onClick={() => onChange("all")}>All</button>
      {STAGES.map((s) => (
        <button
          key={s} type="button" aria-pressed={stage === s} aria-label={`${STAGE_TITLE[s]}, ${counts[s]}`} className={chip(stage === s)}
          onClick={() => onChange(stage === s ? "all" : s)}
        >
          <span aria-hidden className={cn("size-1.5 rounded-full", TONE_DOT[STAGE_TONE[s]])} />
          {STAGE_TITLE[s]}
          <span className="tabular-nums text-muted-foreground">{counts[s]}</span>
        </button>
      ))}
    </div>
  );
}

/**
 * The stage filter as quiet text tabs (DESIGN.md §8, 2026-11-08; was ui-kit' pill tabs in a
 * well): All, then one per stage with a faint count. The one that is on is a soft fill, like the
 * window's tab strip; no container, no indicator line. One is on at a time.
 */
const QUIET_TAB = "!h-7 !grow-0 !rounded-md !px-2 !text-[13px] !font-normal text-muted-foreground hover:!bg-foreground/[0.04] data-active:!bg-foreground/[0.07] data-active:text-foreground";

export function StageTabs({ stage, counts, total, onChange }: { stage: Stage | "all"; counts: Record<Stage, number>; total: number; onChange: (stage: Stage | "all") => void }) {
  return (
    <div data-slot="stage-filters" className="min-w-0">
      <Tabs value={stage} onValueChange={(v) => onChange(v as Stage | "all")}>
        <TabsList variant="underline" aria-label="Show tasks in" className="flex-wrap justify-start !py-0 [&>[data-slot=tab-indicator]]:hidden">
          <TabsTab value="all" className={QUIET_TAB} aria-label={`All tasks, ${total}`}>All<span className="text-[12px] tabular-nums text-muted-foreground/70">{total}</span></TabsTab>
          {STAGES.map((s) => (
            <TabsTab key={s} value={s} className={QUIET_TAB} aria-label={`${STAGE_TITLE[s]}, ${counts[s]}`}>
              {STAGE_TITLE[s]}<span className="text-[12px] tabular-nums text-muted-foreground/70">{counts[s]}</span>
            </TabsTab>
          ))}
        </TabsList>
      </Tabs>
    </div>
  );
}
