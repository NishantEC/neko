// The gallery's "Task card" section: ways to draw one task on the Tasks board
// (work/board/BoardCard.tsx), decided before the columns. One card per option,
// 300px wide, side by side; a segmented control above puts the same task in one
// state on every card at once. Everything is local state: cards select, and the
// action (Start again, Answer, Approve) switches every card to Running.
//
// Owner's brief: spacious, slotted, tidy, plain sans, Neko colours. Option 5
// copies hushbacks' form tile (Dashboard/Gallery/GalleryFormCard): an outer
// slab holding an inset well for the content, then a slim status row under it
// with a dot, the label, the time and a ⋯ menu.
import { Fragment, useEffect, useState, type ReactNode } from "react";
import { DotsThree, GitPullRequest } from "@/ui/icons";
import { Button } from "@/ui/button";
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger } from "@/ui/dropdown-menu";
import { cn } from "@/ui/lib/utils";
import { SegmentedControl, SegmentedControlItem } from "@/ui/segmented-control";
import { TONE_DOT, TONE_TEXT, type Tone } from "@/work/tone";

type Kind = "stopped" | "asks" | "approve" | "review" | "running" | "done";

type Task = {
  id: string;
  key: string;
  kind: Kind;
  title: string;
  repo: string;
  /** "Sentry", or a Linear key. */
  source?: string;
  pr?: number;
  /** Minutes since it last changed. */
  age: number;
  reason?: string;
  /** Running only: the step it is on, which of how many, and seconds in. */
  step?: { label: string; at: number; of: number; seconds: number };
};

/** One task, drawn in whichever state the switcher picks, so the options compare like for like. */
const BASE = { id: "t1", key: "NEK-8E07", title: "Bump expo-notifications to 0.29 and fix the iOS badge count", repo: "navihealth", source: "HEA-2987" };
const IN_STATE: Record<Kind, Omit<Task, keyof typeof BASE | "kind">> = {
  stopped: { age: 18, reason: "Actual changed files exceed verified or approved scope." },
  asks: { age: 64, reason: "Should the badge count include muted coach threads, or only unmuted ones?" },
  approve: { age: 125, pr: 4649, reason: "The plan touches 14 files across two packages." },
  review: { age: 35, pr: 4649 },
  running: { age: 2, step: { label: "Running tests", at: 3, of: 5, seconds: 128 } },
  done: { age: 250, pr: 4649 },
};
const taskIn = (kind: Kind, seconds: number): Task => {
  const t: Task = { ...BASE, kind, ...IN_STATE[kind] };
  return t.step ? { ...t, step: { ...t.step, seconds: t.step.seconds + seconds } } : t;
};
const KINDS: Kind[] = ["stopped", "asks", "approve", "review", "running", "done"];

const LABEL: Record<Kind, string> = { stopped: "Stopped", asks: "Asks you", approve: "Needs approval", review: "Ready to review", running: "Running", done: "Done" };
const TONE: Record<Kind, Tone> = { stopped: "failed", asks: "attention", approve: "attention", review: "review", running: "running", done: "done" };
const ACTION: Partial<Record<Kind, string>> = { stopped: "Start again", asks: "Answer", approve: "Approve" };
/** The reason well in option 4, by state. */
/** The well's light tint while a task waits on you (5a). */
const WELL_TINT: Partial<Record<Tone, string>> = { failed: "bg-destructive/[0.06]", attention: "bg-warning/[0.08]" };
const WELL: Partial<Record<Tone, string>> = { failed: "bg-destructive/[0.07] ring-destructive/15", attention: "bg-warning/[0.09] ring-warning/20" };

const age = (m: number) => (m < 60 ? `${m}m` : m < 1440 ? `${Math.round(m / 60)}h` : `${Math.round(m / 1440)}d`);
const clock = (s: number) => `${Math.floor(s / 60)}m ${String(s % 60).padStart(2, "0")}s`;
const FOCUS = "outline-none focus-visible:ring-2 focus-visible:ring-ring/60";

// ---------------------------------------------------------------------------
// Per card: the selected state; acting on it is lifted to the switcher.
// ---------------------------------------------------------------------------

type Mock = { selected: string | null; select: (id: string) => void; act: (id: string) => void };
function label(t: Task) {
  return `${t.title}. ${LABEL[t.kind]}${t.reason ? `: ${t.reason}` : ""}. ${t.repo}${t.source ? `, ${t.source}` : ""}, ${age(t.age)} ago${t.pr ? `, pull request ${t.pr}` : ""}`;
}

/** The card's hit area, under the content: selects on click, takes keyboard focus. */
function Handle({ t, m, className }: { t: Task; m: Mock; className?: string }) {
  const on = m.selected === t.id;
  return <button type="button" aria-current={on ? "true" : undefined} aria-label={label(t)} onClick={() => m.select(t.id)} className={cn("absolute inset-0 rounded-[inherit]", FOCUS, className)} />;
}

function Action({ t, m, variant = "outline", className }: { t: Task; m: Mock; variant?: "outline" | "ghost" | "default"; className?: string }) {
  const verb = ACTION[t.kind];
  if (!verb) return null;
  return <Button variant={variant} size="xs" data-slot="card-action" className={cn("relative z-10", className)} onClick={() => m.act(t.id)}>{verb}</Button>;
}

const SHELL = "group/card relative flex w-full flex-col text-start transition-[border-color,box-shadow,background-color] duration-150";
const ring = (on: boolean) => (on ? "border-primary/70 ring-1 ring-primary/40" : "border-border hover:border-foreground/20");

function Pr({ n }: { n?: number }) {
  return n ? <span className="inline-flex shrink-0 items-center gap-1 tabular-nums"><GitPullRequest size={13} aria-hidden />{n}</span> : null;
}

/** The live step as shimmering text (still under Reduce Motion). */
function LiveText({ t, className }: { t: Task; className?: string }) {
  if (!t.step) return null;
  return (
    <span className={cn("min-w-0 truncate tabular-nums", className)}>
      <span className="bg-[linear-gradient(90deg,var(--muted-foreground)_35%,var(--foreground)_50%,var(--muted-foreground)_65%)] bg-[length:200%_100%] bg-clip-text text-transparent animate-shimmer-text motion-reduce:animate-none motion-reduce:text-muted-foreground">
        {t.step.label}
      </span>
      <span className="text-muted-foreground"> · {clock(t.step.seconds)}</span>
    </span>
  );
}

// ---------------------------------------------------------------------------
// The options
// ---------------------------------------------------------------------------

/** 1. Spacious: room around everything, the reason in its own tinted well with the action. */
function SpaciousCard({ t, m, live }: { t: Task; m: Mock; live?: boolean }) {
  const tone = TONE[t.kind];
  const on = m.selected === t.id;
  return (
    <div data-slot="card" data-kind={t.kind} data-selected={on ? "" : undefined} className={cn(SHELL, "gap-3.5 overflow-hidden rounded-2xl border bg-card px-[18px] pt-4 pb-4 shadow-xs", ring(on), live && t.step && "pb-5")}>
      <Handle t={t} m={m} />
      <span className="pointer-events-none line-clamp-2 text-[15px] font-medium leading-[1.4] text-strong">{t.title}</span>
      {t.reason ? (
        <div className={cn("flex flex-col items-start gap-2.5 rounded-xl px-3 py-2.5 ring-1 ring-inset", WELL[tone])}>
          <span className="pointer-events-none flex flex-col gap-0.5">
            <span className={cn("text-[12.5px] font-medium", TONE_TEXT[tone])}>{LABEL[t.kind]}</span>
            <span className="text-[13px] leading-[1.45] text-foreground/80">{t.reason}</span>
          </span>
          <Action t={t} m={m} />
        </div>
      ) : null}
      <span className="pointer-events-none flex items-center justify-between gap-3 text-[12.5px] text-muted-foreground">
        <span className="min-w-0 truncate">{t.repo}{t.source ? <> · {t.source}</> : null}</span>
        <span className="flex shrink-0 items-center gap-2.5 tabular-nums">{live && t.step ? null : age(t.age)}<Pr n={t.pr} /></span>
      </span>
      {live && t.step ? (
        <div data-slot="card-live" className="pointer-events-none -mx-[18px] -mb-5 flex flex-col gap-2 border-t border-border/70 bg-primary/[0.04] px-[18px] pt-2.5 pb-3">
          <span className="flex items-center justify-between gap-3 text-[12.5px]">
            <LiveText t={t} />
            <span className="shrink-0 tabular-nums text-muted-foreground">{t.step.at} of {t.step.of}</span>
          </span>
          <span role="progressbar" aria-label={`Step ${t.step.at} of ${t.step.of}`} aria-valuenow={t.step.at} aria-valuemin={0} aria-valuemax={t.step.of} className="relative h-[3px] overflow-hidden rounded-full bg-primary/15">
            <span className="absolute inset-y-0 left-0 rounded-full bg-primary transition-[width] duration-500" style={{ width: `${(t.step.at / t.step.of) * 100}%` }} />
          </span>
        </div>
      ) : null}
    </div>
  );
}

/** 5. hushbacks' form tile: an outer slab, an inset well for the content, a slim status row with a menu. */
function SlabCard({ t, m }: { t: Task; m: Mock }) {
  const tone = TONE[t.kind];
  const on = m.selected === t.id;
  return (
    <div data-slot="card" data-kind={t.kind} data-selected={on ? "" : undefined} className={cn(SHELL, "gap-1 rounded-[20px] border bg-card p-1.5 shadow-xs", ring(on))}>
      <Handle t={t} m={m} />
      <div className="sky-pebble pointer-events-none flex flex-col gap-2 rounded-[15px] px-3.5 pt-3 pb-3.5">
        <span className="line-clamp-2 text-[14.5px] font-medium leading-[1.4] text-strong">{t.title}</span>
        {t.reason ? <span className="text-[13px] leading-[1.45] text-foreground/70">{t.reason}</span> : null}
        {t.step ? <LiveText t={t} className="text-[13px]" /> : null}
        <span className="truncate text-[12.5px] text-muted-foreground">
          {t.repo}{t.source ? <> · {t.source}</> : null}{t.pr ? <> · #{t.pr}</> : null}
        </span>
      </div>
      <span className="flex h-9 items-center gap-2 ps-2.5 pe-1 text-[12.5px]">
        <span aria-hidden className={cn("pointer-events-none size-1.5 rounded-full", TONE_DOT[tone], t.kind === "running" && "animate-pulse motion-reduce:animate-none")} />
        <span className={cn("pointer-events-none font-medium", TONE_TEXT[tone])}>{LABEL[t.kind]}</span>
        <span className="pointer-events-none tabular-nums text-muted-foreground">· {t.step ? clock(t.step.seconds) : age(t.age)}</span>
        <span className="flex-1" />
        <Action t={t} m={m} variant="ghost" className="text-foreground" />
        <DropdownMenu>
          <DropdownMenuTrigger render={<Button variant="ghost" size="icon-xs" aria-label={`More for ${t.key}`} className="relative z-10" />}>
            <DotsThree weight="bold" />
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end">
            <DropdownMenuItem onClick={() => m.select(t.id)}>Open {t.key}</DropdownMenuItem>
            {ACTION[t.kind] ? <DropdownMenuItem onClick={() => m.act(t.id)}>{ACTION[t.kind]}</DropdownMenuItem> : null}
          </DropdownMenuContent>
        </DropdownMenu>
      </span>
    </div>
  );
}

// --- 5a-5c: developing the slab ------------------------------------------------------------
// hushbacks' tile values: the inner surface is 16px round inside the slab
// (rounded-[16px], p-5 for its content), the slab lifts 3px on hover over
// 0.4s cubic-bezier(0.22, 1, 0.36, 1), and state reads as a pill with a 6px LED.

const SLAB = "rounded-[22px] border bg-card p-1.5 shadow-xs";
const LIFT = "transition-[transform,border-color,box-shadow] duration-[400ms] ease-[cubic-bezier(0.22,1,0.36,1)] hover:-translate-y-[3px] hover:shadow-md motion-reduce:transition-none motion-reduce:hover:translate-y-0";

function StateMark({ t, withAge = true }: { t: Task; withAge?: boolean }) {
  const tone = TONE[t.kind];
  return (
    <span className="pointer-events-none flex min-w-0 items-center gap-2 text-[12.5px]">
      <span aria-hidden className={cn("size-1.5 shrink-0 rounded-full", TONE_DOT[tone])} />
      <span className={cn("truncate font-medium", TONE_TEXT[tone])}>{LABEL[t.kind]}</span>
      {withAge ? <span className="shrink-0 tabular-nums text-muted-foreground">{t.step ? clock(t.step.seconds) : age(t.age)}</span> : null}
    </span>
  );
}

function More({ t, m }: { t: Task; m: Mock }) {
  return (
    <DropdownMenu>
      <DropdownMenuTrigger render={<Button variant="ghost" size="icon-xs" aria-label={`More for ${t.key}`} className="relative z-10 shrink-0" />}>
        <DotsThree weight="bold" />
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end">
        <DropdownMenuItem onClick={() => m.select(t.id)}>Open {t.key}</DropdownMenuItem>
        {ACTION[t.kind] ? <DropdownMenuItem onClick={() => m.act(t.id)}>{ACTION[t.kind]}</DropdownMenuItem> : null}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

/** A light state tint over the well, while the task waits on you. */
function Tint({ t }: { t: Task }) {
  const tint = WELL_TINT[TONE[t.kind]];
  return tint ? <span aria-hidden className={cn("absolute inset-0 rounded-[inherit]", tint)} /> : null;
}

/** A thin progress line along the slab's bottom edge (the slab clips it to its corners). */
function EdgeProgress({ t }: { t: Task }) {
  if (!t.step) return null;
  return (
    <span role="progressbar" aria-label={`Step ${t.step.at} of ${t.step.of}`} aria-valuenow={t.step.at} aria-valuemin={0} aria-valuemax={t.step.of} className="pointer-events-none absolute inset-x-0 bottom-0 h-[2px] bg-primary/12">
      <span className="absolute inset-y-0 left-0 bg-primary transition-[width] duration-500" style={{ width: `${(t.step.at / t.step.of) * 100}%` }} />
    </span>
  );
}

function PrChip({ n }: { n?: number }) {
  if (!n) return null;
  return (
    <span className="inline-flex h-5 shrink-0 items-center gap-1 rounded-full bg-foreground/[0.06] px-2 text-[11.5px] font-medium tabular-nums text-foreground/75">
      <GitPullRequest size={11} aria-hidden />{n}
    </span>
  );
}

/** 5a. The slab, live: the status row carries the running step; a waiting task tints the well. */
function SlabLiveCard({ t, m }: { t: Task; m: Mock }) {
  const on = m.selected === t.id;
  return (
    <div data-slot="card" data-kind={t.kind} data-selected={on ? "" : undefined} className={cn(SHELL, SLAB, "gap-1 overflow-hidden", ring(on))}>
      <Handle t={t} m={m} />
      <div className="sky-pebble pointer-events-none relative flex flex-col gap-2 rounded-[16px] px-3.5 pt-3 pb-3.5">
        <Tint t={t} />
        <span className="line-clamp-2 text-[14.5px] font-medium leading-[1.4] text-strong">{t.title}</span>
        {t.reason ? <span className="text-[13px] leading-[1.45] text-foreground/75">{t.reason}</span> : null}
        <span className="truncate text-[12.5px] text-muted-foreground">
          {t.repo}{t.source ? <> · {t.source}</> : null}{t.pr ? <> · #{t.pr}</> : null}
        </span>
      </div>
      {t.step ? (
        <span data-slot="card-live" className="flex h-9 items-center gap-2 ps-2.5 pe-1 text-[12.5px]">
          <span aria-hidden className="pointer-events-none size-1.5 shrink-0 animate-pulse rounded-full bg-primary motion-reduce:animate-none" />
          <LiveText t={t} className="pointer-events-none flex-1" />
          <span className="pointer-events-none shrink-0 tabular-nums text-muted-foreground">{t.step.at} of {t.step.of}</span>
          <More t={t} m={m} />
          <EdgeProgress t={t} />
        </span>
      ) : (
        <span className="flex h-9 items-center gap-2 ps-2.5 pe-1">
          <StateMark t={t} />
          <span className="flex-1" />
          <Action t={t} m={m} variant="ghost" className="text-foreground" />
          <More t={t} m={m} />
        </span>
      )}
    </div>
  );
}

/** 5b. A richer, roomier well (hushbacks' tile spacing); the status row splits state left, actions right. */
function SlabRichCard({ t, m }: { t: Task; m: Mock }) {
  const on = m.selected === t.id;
  return (
    <div data-slot="card" data-kind={t.kind} data-selected={on ? "" : undefined} className={cn(SHELL, SLAB, LIFT, "gap-1.5", ring(on))}>
      <Handle t={t} m={m} />
      <div className="sky-pebble pointer-events-none flex flex-col gap-2.5 rounded-[16px] p-4">
        <span className="line-clamp-2 text-[15px] font-medium leading-[1.4] text-strong">{t.title}</span>
        {t.reason ? <span className="text-[13px] leading-[1.5] text-foreground/70">{t.reason}</span> : null}
        <span className="flex min-w-0 items-center gap-2 text-[12.5px] text-muted-foreground">
          <span className="min-w-0 flex-1 truncate">{t.repo}{t.source ? <> · {t.source}</> : null}</span>
          <PrChip n={t.pr} />
        </span>
      </div>
      <span className="flex h-9 items-center gap-2 ps-3 pe-1">
        <StateMark t={t} />
        <span className="flex-1" />
        <Action t={t} m={m} variant="ghost" className="text-foreground" />
        <More t={t} m={m} />
      </span>
    </div>
  );
}

/** The chosen card's layout. `surface`: paint the well, tab and corner, with the text invisible. */
function FusedBody({ t, surface }: { t: Task; surface?: boolean }) {
  const live = !!t.step;
  // The surface copy hides its contents (they still size it) but keeps its own background and the corner.
  const bg = surface ? "bg-[var(--fuse)] transition-colors duration-150 [&>*:not([data-corner])]:invisible" : "";
  return (
    <div className="flex flex-col">
      <div className={cn("flex flex-col gap-2.5 rounded-[16px] rounded-bl-none p-4", bg)}>
        <span className="line-clamp-2 text-[15px] font-medium leading-[1.4] text-strong">{t.title}</span>
        {t.reason ? <span className="text-[13px] leading-[1.5] text-foreground/70">{t.reason}</span> : null}
        <span className="flex min-w-0 items-center gap-2 text-[12.5px] text-muted-foreground">
          <span className="min-w-0 flex-1 truncate">{t.repo}{t.source ? <> · {t.source}</> : null}</span>
          <PrChip n={t.pr} />
        </span>
      </div>
      <span className="flex h-9">
        <span
          data-slot={surface ? undefined : live ? "card-live" : "card-tab"}
          className={cn("relative flex min-w-0 items-center gap-2 rounded-b-[12px] ps-3.5 pe-3", bg, live ? "me-9 flex-1" : "max-w-[60%]")}
        >
          {/* the concave corner: surface colour with a transparent quarter-circle, so the curve cuts into the background */}
          {surface ? <span data-corner className="absolute top-0 left-full size-3 bg-[radial-gradient(circle_at_100%_100%,transparent_11.5px,var(--fuse)_12px)]" /> : null}
          {live ? (
            <>
              <span className="size-1.5 shrink-0 animate-pulse rounded-full bg-primary motion-reduce:animate-none" />
              <LiveText t={t} className="text-[12.5px]" />
              <span className="ms-auto shrink-0 text-[12px] tabular-nums text-muted-foreground">{t.step!.at} of {t.step!.of}</span>
              {/* the progress line along the tab's bottom edge, clipped to its corners */}
              {surface ? null : <span className="absolute inset-0 overflow-hidden rounded-b-[12px]"><EdgeProgress t={t} /></span>}
            </>
          ) : <StateMark t={t} />}
        </span>
      </span>
    </div>
  );
}

/**
 * 5c, chosen. The card is the well and its state tab, nothing else: no outer slab, border
 * or padding. The tab hangs off the well's bottom-left, joined by a concave corner that is
 * the surface colour with a transparent quarter-circle, so its curve cuts into whatever lies
 * behind (page or column). One `--fuse` colour paints well, tab and corner, so hover and
 * selection change all three together; selection also outlines the joined shape with four
 * 1px drop-shadows (a filter follows the shape's outline, corner included, where a border
 * or ring on each piece would draw seams).
 */
function SlabTabCard({ t, m }: { t: Task; m: Mock }) {
  const on = m.selected === t.id;
  return (
    <div
      data-slot="card"
      data-kind={t.kind}
      data-selected={on ? "" : undefined}
      className={cn(
        "group/card relative flex w-full flex-col text-start",
        // Light: a white surface (the pebble grey vanishes on a column's grey); dark: the pebble.
        "[--fuse-base:var(--card)] dark:[--fuse-base:var(--sky-pebble)]",
        "[--fuse:var(--fuse-base)] hover:[--fuse:color-mix(in_oklab,var(--foreground)_3%,var(--fuse-base))]",
        on && "[--fuse:color-mix(in_oklab,var(--primary)_7%,var(--fuse-base))] hover:[--fuse:color-mix(in_oklab,var(--primary)_10%,var(--fuse-base))]",
        "transition-transform duration-[400ms] ease-[cubic-bezier(0.22,1,0.36,1)] hover:-translate-y-[3px] motion-reduce:transition-none motion-reduce:hover:translate-y-0",
      )}
    >
      <Handle t={t} m={m} className="rounded-[16px]" />
      {/* Two copies of one layout in the same grid cell: the back one paints the surface (its
          text invisible but still sizing it) and carries the filter; the front one carries the
          text, positioned so it paints above the filtered layer. A drop-shadow on the text would fringe every glyph. */}
      <div data-slot="card-shape" className="pointer-events-none grid">
        <div
          aria-hidden
          className={cn(
            "[grid-area:1/1] transition-[filter] duration-150",
            // A soft shadow that follows the joined shape in light; selected, a 1px primary outline.
            on
              ? "[filter:drop-shadow(1px_0_0_var(--primary))_drop-shadow(-1px_0_0_var(--primary))_drop-shadow(0_1px_0_var(--primary))_drop-shadow(0_-1px_0_var(--primary))]"
              : "[filter:drop-shadow(0_0_0.5px_rgb(0_0_0/0.14))_drop-shadow(0_1px_2px_rgb(0_0_0/0.06))] dark:[filter:none]",
          )}
        >
          <FusedBody t={t} surface />
        </div>
        <div className="relative [grid-area:1/1]"><FusedBody t={t} /></div>
      </div>
      <span className="absolute end-0 bottom-0 flex h-9 items-center gap-1">
        <Action t={t} m={m} variant="ghost" className="text-foreground" />
        <More t={t} m={m} />
      </span>
    </div>
  );
}

/** Three chosen cards stacked in a board column, to see how they sit together. */
function FusedColumn({ seconds }: { seconds: number }) {
  const [kinds, setKinds] = useState<Kind[]>(["stopped", "running", "review"]);
  const [selected, setSelected] = useState<string | null>(null);
  return (
    <div data-slot="fused-column" className="flex flex-col gap-1 rounded-2xl bg-foreground/[0.03] p-1.5">
      <div className="flex h-8 items-center gap-2 px-2">
        <span className="text-[13px] font-semibold text-strong">A column</span>
        <span className="text-[12px] tabular-nums text-muted-foreground">3</span>
      </div>
      <div className="flex flex-col gap-2.5 px-0.5 pb-1">
        {kinds.map((k, i) => {
          const t = { ...taskIn(k, seconds), id: `col-${i}` };
          const m: Mock = { selected, select: (id) => setSelected((s) => (s === id ? null : id)), act: () => setKinds((ks) => ks.map((x, j) => (j === i ? "running" : x))) };
          return <SlabTabCard key={t.id} t={t} m={m} />;
        })}
      </div>
    </div>
  );
}

/** One option's card, with its own selection; acting on it sends every card to Running. */
function OptionCard({ task, onAct, render, bare }: { task: Task; onAct: () => void; render: (t: Task, m: Mock) => ReactNode; bare?: boolean }) {
  const [selected, setSelected] = useState<string | null>(null);
  const m: Mock = { selected, select: (id) => setSelected((s) => (s === id ? null : id)), act: onAct };
  return <div data-slot="card-stage" className={bare ? "p-2" : "rounded-2xl bg-foreground/[0.03] p-2"}>{render(task, m)}</div>;
}

const OPTIONS: { id: string; name: string; caption: string; render: (t: Task, m: Mock) => ReactNode }[] = [
  {
    id: "slab-tab",
    name: "Chosen: fused tab",
    caption: "The card is the well and its state tab, nothing else: no outer frame. The tab hangs off the well’s bottom-left, joined by a concave corner that cuts into whatever is behind. Running, the tab carries the live step and a progress line; selected, the shape gets a primary outline and a faint tint. In light the surface is white with a shadow that follows the shape, since grey on a grey column disappears. Shown here on the page; the column beside it shows three stacked.",
    render: (t, m) => <SlabTabCard t={t} m={m} />,
  },
  {
    id: "live",
    name: "4. Live-aware (reference)",
    caption: "Unchanged: a roomy card with a tinted reason well, and on running tasks a live strip with the current step shimmering, its clock and how far through the run it is.",
    render: (t, m) => <SpaciousCard t={t} m={m} live />,
  },
  {
    id: "slab",
    name: "5. Slab and well (base)",
    caption: "Unchanged: hushbacks’ form tile. An outer slab holds an inset well for what the task is; a slim status row under it says where it stands.",
    render: (t, m) => <SlabCard t={t} m={m} />,
  },
  {
    id: "slab-live",
    name: "5a. Slab and well + live",
    caption: "5 with 4’s live strip folded into the status row: running, it shows the step shimmering, the clock and a 2px progress line along the slab’s bottom edge. A task waiting on you tints the well lightly by state; the action stays in the row.",
    render: (t, m) => <SlabLiveCard t={t} m={m} />,
  },
  {
    id: "slab-rich",
    name: "5b. Slab and well, richer well",
    caption: "Roomier, on hushbacks’ tile values: a 16px well inside a 6px slab, 15px title, repo and source key with the PR as a small chip. State, dot and age sit left in the row; the action and ⋯ right. Lifts 3px on hover.",
    render: (t, m) => <SlabRichCard t={t} m={m} />,
  },
];

export function CardOptions() {
  const [kind, setKind] = useState<Kind>("stopped");
  const [seconds, setSeconds] = useState(0);
  useEffect(() => {
    // The live card's clock: one tick a second, only while Running is shown.
    if (kind !== "running") return;
    const id = setInterval(() => setSeconds((n) => n + 1), 1000);
    return () => clearInterval(id);
  }, [kind]);
  const task = taskIn(kind, seconds);
  const show = (k: Kind) => { setKind(k); setSeconds(0); };
  return (
    <div className="flex flex-col gap-6">
      <SegmentedControl aria-label="Task state shown on every card" value={kind} onValueChange={(v) => show(v as Kind)} className="self-start">
        {KINDS.map((k) => <SegmentedControlItem key={k} value={k}>{LABEL[k]}</SegmentedControlItem>)}
      </SegmentedControl>
      <div className="flex flex-wrap items-start gap-x-5 gap-y-10">
        {OPTIONS.map((o, i) => (
          <Fragment key={o.id}>
            <figure data-card-option={o.id} className="m-0 flex w-[316px] flex-col gap-3">
              <OptionCard task={task} onAct={() => show("running")} render={o.render} bare={o.id === "slab-tab"} />
              <figcaption className="flex flex-col gap-1 px-1">
                <span className="text-sm font-semibold text-foreground">{o.name}</span>
                <span className="text-[13px] leading-snug text-muted-foreground">{o.caption}</span>
              </figcaption>
            </figure>
            {i === 0 ? (
              <figure className="m-0 flex w-[316px] flex-col gap-3">
                <FusedColumn seconds={seconds} />
                <figcaption className="flex flex-col gap-1 px-1">
                  <span className="text-sm font-semibold text-foreground">Chosen, in a column</span>
                  <span className="text-[13px] leading-snug text-muted-foreground">Stopped, running and ready to review, stacked on a board column’s background. Click to select; Start again sets that one running.</span>
                </figcaption>
              </figure>
            ) : null}
          </Fragment>
        ))}
      </div>
    </div>
  );
}
