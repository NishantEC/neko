// The gallery's "Task card" section. The owner chose the fused-tab card: the card is a
// well (what the task is) with a state tab hanging off its bottom-left, joined by a
// concave corner. A task that waits on you keeps its action, always visible, in the space
// beside the tab (below the well, outside the fused shape). The section shows that card
// with two styles of the action side by side, one task in every state (the switcher above
// drives both), and a sample column.
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
/** How the action beside the tab is drawn. */
type Variant = "ghost" | "pill";

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

/** (1)'s ghost button. */
const GHOST = "text-foreground/85 hover:bg-foreground/[0.07] hover:text-foreground active:bg-foreground/[0.1]";
/** (2)'s pill, tinted by state. */
const PILL: Partial<Record<Tone, string>> = {
  failed: "bg-destructive/12 text-destructive-foreground hover:bg-destructive/18 active:bg-destructive/24",
  attention: "bg-warning/14 text-warning-foreground hover:bg-warning/20 active:bg-warning/26",
};

/**
 * The action, in the space beside the tab: always visible when the task waits on you,
 * 28px tall and centred in the tab's 36px row (so their centres match), its right edge
 * on the well's right edge.
 */
function CavityAction({ t, m, variant }: { t: Task; m: Mock; variant: Variant }) {
  const verb = ACTION[t.kind];
  if (!verb) return null;
  const Icon = ACTION_ICON[t.kind as keyof typeof ACTION_ICON];
  return (
    <span className="absolute end-0 bottom-0 flex h-9 items-center">
      <button
        type="button"
        data-slot="card-action"
        data-style={variant}
        onClick={() => m.act(t.id)}
        className={cn(
          "relative z-10 inline-flex h-7 cursor-pointer items-center gap-1.5 rounded-full px-3 text-[12.5px] font-medium leading-none transition-colors duration-150",
          variant === "pill" ? PILL[TONE[t.kind]] : GHOST,
          FOCUS,
        )}
      >
        <Icon size={13} weight="bold" aria-hidden />
        {verb}
      </button>
    </span>
  );
}

/** The tab's contents: the state, or (running) the shimmering step and its clock. */
function TabContent({ t }: { t: Task }) {
  return t.step ? <LiveText t={t} /> : <StateMark t={t} />;
}

/** The card's layout. `surface`: paint the well, tab and corner, with the contents invisible. */
function FusedBody({ t, surface }: { t: Task; surface?: boolean }) {
  const done = t.kind === "done";
  const bg = surface ? "bg-[var(--fuse)] transition-colors duration-150 [&>*:not([data-corner])]:invisible" : "";
  return (
    <div className="flex flex-col">
      <div className={cn("flex flex-col gap-2.5 rounded-[16px] p-4", !done && "rounded-bl-none", bg)}>
        <span className="line-clamp-2 text-[15px] font-medium leading-[1.4] text-strong">{t.title}</span>
        {t.reason ? <span className="text-[13px] leading-[1.5] text-foreground/70">{t.reason}</span> : null}
        <span className="flex min-w-0 items-center gap-2 text-[12.5px] text-muted-foreground">
          <span className="min-w-0 flex-1 truncate">
            {t.repo}{t.source ? <> · {t.source}</> : null}{done ? <> · {age(t.age)}</> : null}
          </span>
          <PrChip n={t.pr} />
        </span>
      </div>
      {done ? null : (
        <span className="flex h-9">
          <span
            data-slot={surface ? undefined : t.step ? "card-live" : "card-tab"}
            className={cn("relative flex min-w-0 items-center gap-2 rounded-b-[12px] ps-3.5 pe-3", ACTION[t.kind] ? "max-w-[58%]" : "max-w-[85%]", bg)}
          >
            {/* the concave corner: surface colour with a transparent quarter-circle, so the curve cuts into the background */}
            {surface ? <span data-corner className="absolute top-0 left-full size-3 bg-[radial-gradient(circle_at_100%_100%,transparent_11.5px,var(--fuse)_12px)]" /> : null}
            <TabContent t={t} />
          </span>
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
          <FusedBody t={t} surface />
        </div>
        {/* positioned, so it paints above the filtered surface */}
        <div className="relative [grid-area:1/1]"><FusedBody t={t} /></div>
      </div>
      <CavityAction t={t} m={m} variant={variant} />
    </div>
  );
}

// ---------------------------------------------------------------------------
// The section
// ---------------------------------------------------------------------------

const VARIANTS: { id: Variant; name: string; caption: string }[] = [
  {
    id: "ghost",
    name: "(1) Ghost button with an icon",
    caption: "The action sits in the space beside the tab, always there when the task waits on you: icon and word, no fill until you hover. Quiet enough that the tab’s colour still says the state.",
  },
  {
    id: "pill",
    name: "(2) Soft pill tinted by state",
    caption: "The same place and size, as a soft pill in the state’s colour (red for Stopped, amber for Asks you and Approve). Easier to spot down a long column; a little louder.",
  },
];

/** The column uses (1), the ghost button. */
const COLUMN_VARIANT: Variant = "ghost";

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
            <span className="text-sm font-semibold text-foreground">In a column, with (1)</span>
            <span className="text-[13px] leading-snug text-muted-foreground">Stopped, running, ready to review and done, stacked on a board column’s background. Only the stopped card has an action; the others leave the space empty. Click a card to select it.</span>
          </figcaption>
        </figure>
      </div>
    </div>
  );
}
