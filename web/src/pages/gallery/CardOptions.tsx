// The gallery's "Task card" section. The owner chose the fused-tab card: the card is a
// well (what the task is) with a state tab hanging off its bottom-left, joined by a
// concave corner. A task that waits on you keeps its action, always visible. The section shows
// four places for it side by side (a full-width chin, twin tabs, in the well, inside the
// tab), one task in every state (the switcher above drives all of them), and a sample column.
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
/** Where the action lives on the card. */
type Variant = "chin" | "twin" | "well" | "split";

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

/** A concave corner joining a tab's side to the well above: surface colour with a transparent quarter-circle. */
function Corner({ side }: { side: "right" | "left" }) {
  return side === "right"
    ? <span data-corner className="absolute top-0 left-full size-3 bg-[radial-gradient(circle_at_100%_100%,transparent_11.5px,var(--fuse)_12px)]" />
    : <span data-corner className="absolute top-0 right-full size-3 bg-[radial-gradient(circle_at_0%_100%,transparent_11.5px,var(--fuse)_12px)]" />;
}

/**
 * The card's layout, one per variant. `surface`: paint the well, tabs and corners with
 * every content invisible (the filter goes on this copy); otherwise draw the contents.
 */
function FusedBody({ t, m, variant, surface }: { t: Task; m: Mock; variant: Variant; surface?: boolean }) {
  const done = t.kind === "done";
  const hasAction = !!ACTION[t.kind];
  const paint = surface ? "bg-[var(--fuse)] transition-colors duration-150 [&>*:not([data-corner])]:invisible" : "";
  const tabSlot = surface ? undefined : t.step ? "card-live" : "card-tab";
  const meta = (
    <span className="min-w-0 flex-1 truncate">
      {t.repo}{t.source ? <> · {t.source}</> : null}{done ? <> · {age(t.age)}</> : null}
    </span>
  );
  // 3: the action is the well's last line, beside the meta.
  const actionInWell = variant === "well" && hasAction;
  const wellRound = done ? "rounded-[16px]"
    : variant === "chin" ? "rounded-t-[16px]"
    : variant === "twin" && hasAction ? "rounded-t-[16px]"
    : "rounded-[16px] rounded-bl-none";
  return (
    <div className="flex flex-col">
      <div className={cn("flex flex-col gap-2.5 p-4", wellRound, actionInWell && "pb-3", paint)}>
        <span className="line-clamp-2 text-[15px] font-medium leading-[1.4] text-strong">{t.title}</span>
        {t.reason ? <span className="text-[13px] leading-[1.5] text-foreground/70">{t.reason}</span> : null}
        <span data-slot={actionInWell && !surface ? "action-row" : undefined} className={cn("flex min-w-0 items-center gap-2 text-[12.5px] text-muted-foreground", actionInWell && "-me-1.5")}>
          {meta}
          <PrChip n={t.pr} />
          {actionInWell ? <ActionButton t={t} m={m} surface={surface} /> : null}
        </span>
      </div>
      {done ? null : variant === "chin" ? (
        // 1: one chin the whole width of the well, a hairline under the well.
        <span data-slot={surface ? undefined : "action-row"} className={cn("flex h-10 items-center gap-2 rounded-b-[16px] ps-4 pe-1.5", paint, !surface && "shadow-[inset_0_1px_0_color-mix(in_oklab,var(--foreground)_7%,transparent)]")}>
          <span data-slot={tabSlot} className="flex min-w-0 flex-1 items-center"><TabContent t={t} /></span>
          <ActionButton t={t} m={m} surface={surface} />
        </span>
      ) : (
        <span data-slot={surface ? undefined : "action-row"} className="flex h-9 justify-between">
          <span data-slot={tabSlot} className={cn("relative flex min-w-0 items-center gap-2 rounded-b-[12px] ps-3.5", variant === "split" && hasAction ? "pe-1" : "pe-3", hasAction && variant === "twin" ? "max-w-[52%]" : "max-w-[85%]", paint)}>
            {surface ? <Corner side="right" /> : null}
            <TabContent t={t} />
            {variant === "split" && hasAction ? (
              // 4: the action joins the state inside the tab, past a hairline.
              <>
                <span aria-hidden className="mx-0.5 h-4 w-px shrink-0 bg-foreground/[0.12]" />
                <ActionButton t={t} m={m} surface={surface} />
              </>
            ) : null}
          </span>
          {variant === "twin" && hasAction ? (
            // 2: a second tab, bottom-right, that is the action.
            <span className={cn("relative flex items-center rounded-b-[12px] px-1", paint)}>
              {surface ? <Corner side="left" /> : null}
              <ActionButton t={t} m={m} surface={surface} tone />
            </span>
          ) : null}
        </span>
      )}
    </div>
  );
}

/**
 * The chosen card. No outer slab: the well and its tab are the card. One `--fuse` colour
 * paints well, tab and corner, so hover and selection change them together. In light the
 * surface is white with a shadow that follows the shape (grey vanished on a grey column);
 * selected, four 1px drop-shadows outline the joined shape. The filter sits on a surface
 * copy behind the text, since on the text it would fringe every glyph.
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
    id: "chin",
    name: "1. Full-width chin",
    caption: "The tab spans the well’s whole width and becomes a footer chin under a hairline: state on the left, the action as a ghost button on the right. No cavity; every card has the same silhouette. Done has no chin.",
  },
  {
    id: "twin",
    name: "2. Twin tabs",
    caption: "Two tabs hang off the well, each joined by a concave corner: the state bottom-left, the action bottom-right in the state’s colour. The gap between them is deliberate. With no action, only the state tab.",
  },
  {
    id: "well",
    name: "3. Action in the well",
    caption: "The action moves up into the well as the last line’s end, beside repo and key. The tab below stays narrow and only says the state; nothing sits in the space beside it.",
  },
  {
    id: "split",
    name: "4. The tab holds both",
    caption: "One tab at its natural width holds the state and, past a hairline, the action. They read as one sentence (“Stopped 18m | ↻ Start again”), and the space beside it is the card’s own edge, not a gap waiting for something.",
  },
];

/** The column uses 2, twin tabs: it keeps the narrow fused tab the owner chose and balances the row. */
const COLUMN_VARIANT: Variant = "twin";

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
            <span className="text-sm font-semibold text-foreground">In a column, with 2</span>
            <span className="text-[13px] leading-snug text-muted-foreground">Stopped, running, ready to review and done, stacked on a board column’s background. Only the stopped card has an action; the others leave the space empty. Click a card to select it.</span>
          </figcaption>
        </figure>
      </div>
    </div>
  );
}
