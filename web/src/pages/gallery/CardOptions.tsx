// The gallery's "Task card" section. The owner chose the notched card (state in a notch
// carved from the bottom-left, action on the card), drawn by work/board/NotchCard.tsx,
// the same component the board uses. This section shows it at three densities side by
// side, one task in every state (the switcher drives all of them), a sample column at the
// board's density (compact), and four small specimens of what could come next.
// Everything is local state; nothing reaches the daemon.
import { useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { Bug, GithubLogo } from "@phosphor-icons/react";
import { ArrowBendUpLeft, ArrowCounterClockwise, ArrowSquareOut, Check, DotsThree } from "@/ui/icons";
import { cn } from "@/ui/lib/utils";
import { SegmentedControl, SegmentedControlItem } from "@/ui/segmented-control";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/ui/tooltip";
import { clock, NotchCard, type Density, type NotchCardProps } from "@/work/board/NotchCard";
import type { Tone } from "@/work/tone";

type Kind = "stopped" | "asks" | "approve" | "review" | "running" | "done";

type Task = {
  id: string;
  kind: Kind;
  title: string;
  repo: string;
  source?: string;
  pr?: number;
  age: number;
  reason?: string;
  step?: { label: string; seconds: number };
};

/** One task, drawn in whichever state the switcher picks, so the densities compare like for like. */
const BASE = { id: "t1", title: "Bump expo-notifications to 0.29 and fix the iOS badge count", repo: "navihealth", source: "HEA-2987" };
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
const ACTION = {
  stopped: { label: "Start again", icon: ArrowCounterClockwise },
  asks: { label: "Answer", icon: ArrowBendUpLeft },
  approve: { label: "Approve", icon: Check },
} as const;

const age = (m: number) => (m < 60 ? `${m}m` : m < 1440 ? `${Math.round(m / 60)}h` : `${Math.round(m / 1440)}d`);

/** The sample task as the card's props, at a density. */
function cardProps(t: Task, density: Density, act: () => void): NotchCardProps {
  const done = t.kind === "done";
  const verb = ACTION[t.kind as keyof typeof ACTION];
  return {
    tone: TONE[t.kind],
    title: t.title,
    density,
    state: done ? null : { label: LABEL[t.kind], age: age(t.age), detail: density === "comfortable" ? undefined : t.repo },
    live: t.step ? { step: t.step.label, clock: clock(t.step.seconds) } : null,
    reason: t.reason,
    meta: <>{t.repo}{t.source ? <> · {t.source}</> : null}{done ? <> · {age(t.age)}</> : null}</>,
    pr: t.pr,
    action: verb ? { ...verb, onClick: act } : null,
  };
}

/** A card the gallery can select: a full-size button under the content. Dense cards name their reason in a tooltip. */
function SampleCard({ t, density, onAct, extra }: { t: Task; density: Density; onAct: () => void; extra?: Partial<NotchCardProps> }) {
  const [on, setOn] = useState(false);
  const props = cardProps(t, density, onAct);
  const label = `${t.title}. ${LABEL[t.kind]}${t.reason ? `: ${t.reason}` : ""}.`;
  const button = (
    <button type="button" aria-current={on ? "true" : undefined} aria-label={label} onClick={() => setOn((v) => !v)} className="absolute inset-0 rounded-[16px] outline-none" />
  );
  const handle = density === "dense" && t.reason ? (
    <Tooltip>
      <TooltipTrigger render={button} />
      <TooltipContent className="max-w-64">{t.reason}</TooltipContent>
    </Tooltip>
  ) : button;
  return <NotchCard {...props} {...extra} data-slot="card" data-kind={t.kind} selected={on} handle={handle} />;
}

/** The rendered height of what it wraps, to the pixel. */
function useHeight() {
  const ref = useRef<HTMLDivElement>(null);
  const [h, setH] = useState(0);
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setH(Math.round(el.getBoundingClientRect().height)));
    ro.observe(el);
    return () => ro.disconnect();
  }, []);
  return { ref, h };
}

const DENSITIES: { id: Density; name: string; caption: string }[] = [
  { id: "comfortable", name: "Comfortable", caption: "Today’s card: 16px padding, a 15px title over two lines, the full reason, a meta line, and a 40px notch row with 12px curves." },
  { id: "compact", name: "Compact (the board)", caption: "12px padding, a 14px title over two lines, the reason on one line, and repo folded into the 26px notch with 10px curves." },
  { id: "dense", name: "Dense", caption: "The title on one line and the notch row. The reason shows in a tooltip on hover or focus." },
];

function DensityFigure({ d, task, onAct }: { d: (typeof DENSITIES)[number]; task: Task; onAct: () => void }) {
  const { ref, h } = useHeight();
  return (
    <figure data-card-option={d.id} className="m-0 flex w-[316px] flex-col gap-3">
      <div ref={ref} className="mx-2 mt-2"><SampleCard t={task} density={d.id} onAct={onAct} /></div>
      <figcaption className="flex flex-col gap-1 px-1">
        <span className="text-sm font-semibold text-foreground">{d.name} <span data-slot="card-height" className="font-normal tabular-nums text-muted-foreground">· about {h}px</span></span>
        <span className="text-[13px] leading-snug text-muted-foreground">{d.caption}</span>
      </figcaption>
    </figure>
  );
}

/** Four compact cards stacked in a board column, to see how they sit together. */
function SampleColumn({ seconds }: { seconds: number }) {
  const [kinds, setKinds] = useState<Kind[]>(["stopped", "running", "review", "done"]);
  return (
    <div data-slot="fused-column" className="flex flex-col gap-1 rounded-2xl bg-foreground/[0.03] p-1.5">
      <div className="flex h-8 items-center gap-2 px-2">
        <span className="text-[13px] font-semibold text-strong">A column</span>
        <span className="text-[12px] tabular-nums text-muted-foreground">{kinds.length}</span>
      </div>
      <div className="flex flex-col gap-2 px-0.5 pb-1">
        {kinds.map((k, i) => (
          <SampleCard key={i} t={{ ...taskIn(k, seconds), id: `col-${i}` }} density="compact" onAct={() => setKinds((ks) => ks.map((x, j) => (j === i ? "running" : x)))} />
        ))}
      </div>
    </div>
  );
}

// --- what more: four small specimens ---------------------------------------------------------

/** Linear's mark, roughly: a disc crossed by parallel strokes. */
function LinearGlyph() {
  return (
    <svg viewBox="0 0 16 16" width={12} height={12} aria-label="Linear" role="img" className="shrink-0 text-foreground/70">
      <circle cx="8" cy="8" r="6.5" fill="none" stroke="currentColor" strokeWidth="1.4" />
      <path d="M3.6 10.4l2 2M3 7.6l5.4 5.4M4.6 4.4l7 7" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round" />
    </svg>
  );
}

const IDEAS: { id: string; name: string; render: () => ReactNode }[] = [
  {
    id: "source-glyph",
    name: "The source as a glyph in the notch (Linear, Sentry, GitHub) instead of its name.",
    render: () => (
      <div className="flex flex-col gap-2">
        <SampleCard t={taskIn("stopped", 0)} density="compact" onAct={() => {}} extra={{ notchLead: <LinearGlyph />, state: { label: "Stopped", age: "18m", detail: "navihealth" } }} />
        <SampleCard t={{ ...taskIn("review", 0), title: "Fix KeyError in the weekly report worker", repo: "athena" }} density="compact" onAct={() => {}} extra={{ notchLead: <Bug size={12} aria-label="Sentry" className="shrink-0 text-foreground/70" />, state: { label: "Ready to review", age: "35m", detail: "athena" } }} />
        <SampleCard t={{ ...taskIn("review", 0), title: "Bump the GitHub Actions cache to v4", repo: "neko" }} density="compact" onAct={() => {}} extra={{ notchLead: <GithubLogo size={12} aria-label="GitHub" className="shrink-0 text-foreground/70" />, state: { label: "Ready to review", age: "2h", detail: "neko" } }} />
      </div>
    ),
  },
  {
    id: "agent",
    name: "Running: the agent’s mark leads the step, so you see who is on it.",
    render: () => (
      <SampleCard
        t={taskIn("running", 0)}
        density="compact"
        onAct={() => {}}
        extra={{ notchLead: <span aria-label="Builder" role="img" className="sky-pebble flex size-4 shrink-0 items-center justify-center rounded-full text-[9px] font-semibold">B</span> }}
      />
    ),
  },
  {
    id: "pr-checks",
    name: "The PR mark carries a dot: green when checks pass, red when they fail.",
    render: () => (
      <div className="flex flex-col gap-2">
        <SampleCard t={taskIn("review", 0)} density="compact" onAct={() => {}} extra={{ prChecks: "passing" }} />
        <SampleCard t={{ ...taskIn("review", 0), title: "Cache food search results for 10 minutes", pr: 4641, repo: "athena" }} density="compact" onAct={() => {}} extra={{ prChecks: "failing", state: { label: "Ready to review", age: "1h", detail: "athena" } }} />
      </div>
    ),
  },
  {
    id: "hover-open",
    name: "Hover or focus reveals Open and ⋯ in the top corner.",
    render: () => (
      <SampleCard
        t={taskIn("review", 0)}
        density="compact"
        onAct={() => {}}
        extra={{
          corner: (
            <span data-slot="card-corner" className="flex items-center gap-0.5 rounded-full bg-[var(--fuse)] p-0.5 shadow-xs ring-1 ring-foreground/[0.06]">
              <button type="button" aria-label="Open in a new tab" className="flex size-6 items-center justify-center rounded-full text-foreground/75 hover:bg-foreground/[0.07] hover:text-foreground"><ArrowSquareOut size={13} /></button>
              <button type="button" aria-label="More" className="flex size-6 items-center justify-center rounded-full text-foreground/75 hover:bg-foreground/[0.07] hover:text-foreground"><DotsThree size={14} weight="bold" /></button>
            </span>
          ),
        }}
      />
    ),
  },
];

export function CardOptions() {
  const [kind, setKind] = useState<Kind>("stopped");
  const [seconds, setSeconds] = useState(0);
  useEffect(() => {
    const id = setInterval(() => setSeconds((n) => n + 1), 1000);
    return () => clearInterval(id);
  }, []);
  const task = taskIn(kind, seconds);
  return (
    <div className="flex flex-col gap-6">
      <SegmentedControl aria-label="Task state shown on every card" value={kind} onValueChange={(v) => setKind(v as Kind)} className="self-start">
        {KINDS.map((k) => <SegmentedControlItem key={k} value={k}>{LABEL[k]}</SegmentedControlItem>)}
      </SegmentedControl>
      <div data-slot="card-densities" className="flex flex-wrap items-start gap-x-5 gap-y-10">
        {DENSITIES.map((d) => <DensityFigure key={d.id} d={d} task={task} onAct={() => setKind("running")} />)}
      </div>
      <div className="flex flex-wrap items-start gap-x-5 gap-y-10 pt-2">
        <figure className="m-0 flex w-[316px] flex-col gap-3">
          <SampleColumn seconds={seconds} />
          <figcaption className="flex flex-col gap-1 px-1">
            <span className="text-sm font-semibold text-foreground">In a column, compact</span>
            <span className="text-[13px] leading-snug text-muted-foreground">Stopped, running, ready to review and done, as the board draws them. Only the stopped card has an action; done has no notch.</span>
          </figcaption>
        </figure>
        <div data-slot="card-ideas" className="flex min-w-0 flex-1 flex-col gap-4">
          <span className="text-sm font-semibold text-foreground">What more we could do</span>
          <div className="grid grid-cols-2 gap-x-5 gap-y-6">
            {IDEAS.map((idea) => (
              <figure key={idea.id} data-card-idea={idea.id} className="m-0 flex flex-col gap-2">
                {idea.render()}
                <figcaption className={cn("px-1 text-[13px] leading-snug text-muted-foreground")}>{idea.name}</figcaption>
              </figure>
            ))}
          </div>
        </div>
      </div>
    </div>
  );
}
