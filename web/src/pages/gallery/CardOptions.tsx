// The gallery's "Task card" section. The owner chose the fused-tab card: the card is a
// well (what the task is) with a state tab hanging off its bottom-left, joined by a
// concave corner. The tab is inverted: the card is one solid surface with a notch carved from
// its bottom edge (the fused-tab curves as negative space), the background showing through.
// Two variations side by side, one task in every state (the switcher drives both), and a
// sample column.
//
// The card's rules, settled with the owner:
// - Done has no tab: just the well, with the age and PR in its meta line.
// - Running: the tab is only the step, shimmering, and its clock. No progress line, no "3 of 5".
// - Selected: the surface tints and the joined shape gets a 1px primary outline.
// Everything is local state; nothing reaches the daemon.
import { Fragment, useEffect, useState } from "react";
import { ArrowBendUpLeft, ArrowCounterClockwise, Check, GitPullRequest } from "@/ui/icons";
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
  /** Running only: the step it is on and seconds in. */
  step?: { label: string; seconds: number };
};

/** One task, drawn in whichever state the switcher picks, so the variants compare like for like. */
const BASE = { id: "t1", key: "NEK-8E07", title: "Bump expo-notifications to 0.29 and fix the iOS badge count", repo: "navihealth", source: "HEA-2987" };
const IN_STATE: Record<Kind, Omit<Task, keyof typeof BASE | "kind">> = {
  stopped: { age: 18, reason: "Actual changed files exceed verified or approved scope." },
  asks: { age: 64, reason: "Should the badge count include muted coach threads, or only unmuted ones?" },
  approve: { age: 125, pr: 4649, reason: "The plan touches 14 files across two packages." },
  review: { age: 35, pr: 4649 },
  running: { age: 2, step: { label: "Running tests", seconds: 128 } },
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
const ACTION_ICON = { stopped: ArrowCounterClockwise, asks: ArrowBendUpLeft, approve: Check } as const;

const age = (m: number) => (m < 60 ? `${m}m` : m < 1440 ? `${Math.round(m / 60)}h` : `${Math.round(m / 1440)}d`);
const clock = (s: number) => `${Math.floor(s / 60)}m ${String(s % 60).padStart(2, "0")}s`;
const FOCUS = "outline-none focus-visible:ring-2 focus-visible:ring-ring/60";

type Mock = { selected: string | null; select: (id: string) => void; act: (id: string) => void };
/** Which corner the notch is carved from. */
type Variant = "left" | "right";

function label(t: Task) {
  return `${t.title}. ${LABEL[t.kind]}${t.reason ? `: ${t.reason}` : ""}. ${t.repo}${t.source ? `, ${t.source}` : ""}, ${age(t.age)} ago${t.pr ? `, pull request ${t.pr}` : ""}. Opens the task.`;
}

function PrChip({ n }: { n?: number }) {
  if (!n) return null;
  return (
    <span className="inline-flex h-5 shrink-0 items-center gap-1 rounded-full bg-foreground/[0.06] px-2 text-[11.5px] font-medium tabular-nums text-foreground/75">
      <GitPullRequest size={11} aria-hidden />{n}
    </span>
  );
}

/** The running step, shimmering (still under Reduce Motion), and its clock. */
function LiveText({ t }: { t: Task }) {
  if (!t.step) return null;
  return (
    <span className="min-w-0 truncate text-[12.5px] tabular-nums">
      <span className="bg-[linear-gradient(90deg,var(--muted-foreground)_35%,var(--foreground)_50%,var(--muted-foreground)_65%)] bg-[length:200%_100%] bg-clip-text text-transparent animate-shimmer-text motion-reduce:animate-none motion-reduce:text-muted-foreground">
        {t.step.label}
      </span>
      <span className="text-muted-foreground"> · {clock(t.step.seconds)}</span>
    </span>
  );
}

function StateMark({ t }: { t: Task }) {
  const tone = TONE[t.kind];
  return (
    <span className="flex min-w-0 items-center gap-2 text-[12.5px]">
      <span aria-hidden className={cn("size-1.5 shrink-0 rounded-full", TONE_DOT[tone])} />
      <span className={cn("truncate font-medium", TONE_TEXT[tone])}>{LABEL[t.kind]}</span>
      <span className="shrink-0 tabular-nums text-muted-foreground">{age(t.age)}</span>
    </span>
  );
}

/** A quiet ghost button: icon and word, no fill until hover. 28px. */
const GHOST = "inline-flex h-7 shrink-0 cursor-pointer items-center gap-1.5 rounded-full px-2.5 text-[12.5px] font-medium leading-none text-foreground/85 transition-colors duration-150 hover:bg-foreground/[0.07] hover:text-foreground active:bg-foreground/[0.1]";

/**
 * The action as an element. In the surface copy it is an inert, invisible twin (same
 * size, so both copies lay out alike); in the front copy it is the real button.
 */
function ActionButton({ t, m, surface, className, tone }: { t: Task; m: Mock; surface?: boolean; className?: string; tone?: boolean }) {
  const verb = ACTION[t.kind];
  if (!verb) return null;
  const Icon = ACTION_ICON[t.kind as keyof typeof ACTION_ICON];
  const inner = <><Icon size={13} weight="bold" aria-hidden />{verb}</>;
  const cls = cn(GHOST, tone && cn(TONE_TEXT[TONE[t.kind]], "hover:text-current"), className);
  if (surface) return <span aria-hidden className={cls}>{inner}</span>;
  return (
    <button type="button" data-slot="card-action" onClick={() => m.act(t.id)} className={cn(cls, "pointer-events-auto relative z-10", FOCUS)}>
      {inner}
    </button>
  );
}

/** The tab's state: the dot, label and age, or (running) the shimmering step and its clock. */
function TabContent({ t }: { t: Task }) {
  return t.step ? <LiveText t={t} /> : <StateMark t={t} />;
}

/**
 * The notch's concave corner, at its top inner corner: card colour with a transparent
 * quarter-circle, so the notch's corner is rounded. `at`: which side of the card piece it
 * sits on (the notch is on that side).
 */
function NotchCorner({ at }: { at: "left" | "right" }) {
  return at === "left"
    ? <span data-corner className="absolute top-0 right-full size-3 bg-[radial-gradient(circle_at_0%_100%,transparent_11.5px,var(--fuse)_12px)] shadow-[0_-1px_0_var(--fuse)]" />
    : <span data-corner className="absolute top-0 left-full size-3 bg-[radial-gradient(circle_at_100%_100%,transparent_11.5px,var(--fuse)_12px)] shadow-[0_-1px_0_var(--fuse)]" />;
}

/**
 * The card's layout. The card is one surface with a notch carved out of its bottom edge:
 * the top piece (rounded top, a convex 12px corner where the notch begins), and beside the
 * notch a bottom piece (the card's bottom corner, a convex 12px corner at the notch, and a
 * concave corner at the notch's top). The bottom piece paints 1px up into the top piece,
 * so a fractional height can never leave a hairline seam between them.
 * `surface`: paint the pieces with every content invisible (the filter goes on this copy).
 */
function FusedBody({ t, m, variant, surface }: { t: Task; m: Mock; variant: Variant; surface?: boolean }) {
  const done = t.kind === "done";
  const hasAction = !!ACTION[t.kind];
  const paint = surface ? "bg-[var(--fuse)] transition-colors duration-150 [&>*:not([data-corner])]:invisible" : "";
  const stateSlot = surface ? undefined : t.step ? "card-live" : "card-tab";
  // (b) only carves a notch when there is an action to put in it.
  const notched = !done && (variant === "left" || hasAction);
  const top = done ? "rounded-[16px]"
    : !notched ? "rounded-t-[16px]"
    : variant === "left" ? "rounded-t-[16px] rounded-bl-[12px]" : "rounded-t-[16px] rounded-br-[12px]";
  const piece = cn("relative flex min-w-0 flex-1 items-center", surface && "shadow-[0_-1px_0_var(--fuse)]", paint);
  return (
    <div className="flex flex-col">
      <div className={cn("flex flex-col gap-2.5 p-4", !done && "pb-2.5", top, paint)}>
        <span className="line-clamp-2 text-[15px] font-medium leading-[1.4] text-strong">{t.title}</span>
        {t.reason ? <span className="text-[13px] leading-[1.5] text-foreground/70">{t.reason}</span> : null}
        <span className="flex min-w-0 items-center gap-2 text-[12.5px] text-muted-foreground">
          <span className="min-w-0 flex-1 truncate">
            {t.repo}{t.source ? <> · {t.source}</> : null}{done ? <> · {age(t.age)}</> : null}
          </span>
          <PrChip n={t.pr} />
        </span>
      </div>
      {done ? null : variant === "left" ? (
        // (a) the notch bottom-left holds the state; the card beside it holds the action.
        <span data-slot={surface ? undefined : "action-row"} className="flex h-10">
          <span data-slot={stateSlot} data-notch={surface ? undefined : ""} className="flex min-w-0 max-w-[62%] shrink-0 items-center ps-3.5 pe-3"><TabContent t={t} /></span>
          <span className={cn(piece, "justify-end rounded-b-[16px] rounded-bl-[12px] pe-2")}>
            {surface ? <NotchCorner at="left" /> : null}
            <ActionButton t={t} m={m} surface={surface} />
          </span>
        </span>
      ) : (
        // (b) the state sits on the card; the notch bottom-right holds the action.
        <span data-slot={surface ? undefined : "action-row"} className="flex h-10">
          <span className={cn(piece, notched ? "rounded-b-[16px] rounded-br-[12px]" : "rounded-b-[16px]", "ps-4 pe-2")}>
            {surface && notched ? <NotchCorner at="right" /> : null}
            <span data-slot={stateSlot} className="flex min-w-0 items-center"><TabContent t={t} /></span>
          </span>
          {notched ? (
            <span data-notch={surface ? undefined : ""} className="flex shrink-0 items-center ps-2 pe-0.5">
              <ActionButton t={t} m={m} surface={surface} tone />
            </span>
          ) : null}
        </span>
      )}
    </div>
  );
}

/**
 * The card: one surface with a notch carved from its bottom edge. One `--fuse` colour paints
 * every piece, so hover and selection change them together. In light the surface is white
 * with a shadow that follows the shape, notch included; selected, four 1px drop-shadows
 * outline it. The filter sits on a surface copy behind the text, since on the text it
 * would fringe every glyph.
 */
function FusedCard({ t, m, variant }: { t: Task; m: Mock; variant: Variant }) {
  const on = m.selected === t.id;
  return (
    <div
      data-slot="card"
      data-kind={t.kind}
      data-selected={on ? "" : undefined}
      className={cn(
        "group/card relative flex w-full flex-col text-start",
        "[--fuse-base:var(--card)] dark:[--fuse-base:var(--sky-pebble)]",
        "[--fuse:var(--fuse-base)] hover:[--fuse:color-mix(in_oklab,var(--foreground)_3%,var(--fuse-base))]",
        on && "[--fuse:color-mix(in_oklab,var(--primary)_7%,var(--fuse-base))] hover:[--fuse:color-mix(in_oklab,var(--primary)_10%,var(--fuse-base))]",
        "transition-transform duration-[400ms] ease-[cubic-bezier(0.22,1,0.36,1)] hover:-translate-y-[3px] motion-reduce:transition-none motion-reduce:hover:translate-y-0",
      )}
    >
      <button type="button" aria-current={on ? "true" : undefined} aria-label={label(t)} onClick={() => m.select(t.id)} className={cn("absolute inset-0 rounded-[16px]", FOCUS)} />
      <div data-slot="card-shape" className="pointer-events-none grid">
        <div
          aria-hidden
          className={cn(
            "[grid-area:1/1] transition-[filter] duration-150",
            on
              ? "[filter:drop-shadow(1px_0_0_var(--primary))_drop-shadow(-1px_0_0_var(--primary))_drop-shadow(0_1px_0_var(--primary))_drop-shadow(0_-1px_0_var(--primary))]"
              : "[filter:drop-shadow(0_0_0.5px_rgb(0_0_0/0.14))_drop-shadow(0_1px_2px_rgb(0_0_0/0.06))] dark:[filter:none]",
          )}
        >
          <FusedBody t={t} m={m} variant={variant} surface />
        </div>
        {/* positioned, so it paints above the filtered surface */}
        <div className="relative [grid-area:1/1]"><FusedBody t={t} m={m} variant={variant} /></div>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// The section
// ---------------------------------------------------------------------------

const VARIANTS: { id: Variant; name: string; caption: string }[] = [
  {
    id: "left",
    name: "(a) State in the notch, action on the card",
    caption: "A notch carved from the bottom-left shows the column through it, and the state sits in it: off the card, like a stamp on the ticket. The action stays on the card at the right, always there when the task waits on you; otherwise that corner is just card.",
  },
  {
    id: "right",
    name: "(b) Action in the notch, state on the card",
    caption: "The mirror: the state is the card’s last line, and when the task waits on you a notch is carved bottom-right to hold the action, in the state’s colour. With no action there is no notch, so only cards that need you change shape.",
  },
];

/** The column uses (a): every live card shares one silhouette, and the state reads at the same place down the column. */
const COLUMN_VARIANT: Variant = "left";

function VariantCard({ task, onAct, variant }: { task: Task; onAct: () => void; variant: Variant }) {
  const [selected, setSelected] = useState<string | null>(null);
  const m: Mock = { selected, select: (id) => setSelected((s) => (s === id ? null : id)), act: onAct };
  return <div data-slot="card-stage" className="p-2"><FusedCard t={task} m={m} variant={variant} /></div>;
}

/** Four chosen cards stacked in a board column, to see how they sit together. */
function FusedColumn({ seconds }: { seconds: number }) {
  const [kinds, setKinds] = useState<Kind[]>(["stopped", "running", "review", "done"]);
  const [selected, setSelected] = useState<string | null>(null);
  return (
    <div data-slot="fused-column" className="flex flex-col gap-1 rounded-2xl bg-foreground/[0.03] p-1.5">
      <div className="flex h-8 items-center gap-2 px-2">
        <span className="text-[13px] font-semibold text-strong">A column</span>
        <span className="text-[12px] tabular-nums text-muted-foreground">{kinds.length}</span>
      </div>
      <div className="flex flex-col gap-2.5 px-0.5 pb-1">
        {kinds.map((k, i) => {
          const t = { ...taskIn(k, seconds), id: `col-${i}` };
          const m: Mock = { selected, select: (id) => setSelected((s) => (s === id ? null : id)), act: () => setKinds((ks) => ks.map((x, j) => (j === i ? "running" : x))) };
          return <FusedCard key={t.id} t={t} m={m} variant={COLUMN_VARIANT} />;
        })}
      </div>
    </div>
  );
}

export function CardOptions() {
  const [kind, setKind] = useState<Kind>("stopped");
  const [seconds, setSeconds] = useState(0);
  useEffect(() => {
    // The live clock: one tick a second.
    const id = setInterval(() => setSeconds((n) => n + 1), 1000);
    return () => clearInterval(id);
  }, []);
  const task = taskIn(kind, seconds);
  const show = (k: Kind) => setKind(k);
  return (
    <div className="flex flex-col gap-6">
      <SegmentedControl aria-label="Task state shown on every card" value={kind} onValueChange={(v) => show(v as Kind)} className="self-start">
        {KINDS.map((k) => <SegmentedControlItem key={k} value={k}>{LABEL[k]}</SegmentedControlItem>)}
      </SegmentedControl>
      <div className="flex flex-wrap items-start gap-x-5 gap-y-10">
        {VARIANTS.map((v) => (
          <Fragment key={v.id}>
            <figure data-card-option={v.id} className="m-0 flex w-[316px] flex-col gap-3">
              <VariantCard task={task} onAct={() => show("running")} variant={v.id} />
              <figcaption className="flex flex-col gap-1 px-1">
                <span className="text-sm font-semibold text-foreground">{v.name}</span>
                <span className="text-[13px] leading-snug text-muted-foreground">{v.caption}</span>
              </figcaption>
            </figure>
          </Fragment>
        ))}
        <figure className="m-0 flex w-[316px] flex-col gap-3">
          <FusedColumn seconds={seconds} />
          <figcaption className="flex flex-col gap-1 px-1">
            <span className="text-sm font-semibold text-foreground">In a column, with (a)</span>
            <span className="text-[13px] leading-snug text-muted-foreground">Stopped, running, ready to review and done, stacked on a board column’s background. Only the stopped card has an action; the others keep that corner as plain card. Done has no notch. Click a card to select it.</span>
          </figcaption>
        </figure>
      </div>
    </div>
  );
}
