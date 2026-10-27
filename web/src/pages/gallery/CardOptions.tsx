// The gallery's "Task card" section: ways to draw one task on the Tasks board
// (work/board/BoardCard.tsx), decided before the columns. Each option draws the
// same six sample tasks, one per state the owner sees, as a single 300px column.
// Everything is local state: cards select, the action moves a task on (Start
// again, Answer and Approve set it running), the ⋯ menu acts on the copy.
//
// Owner's brief: spacious, slotted, tidy, plain sans, Neko colours. Option E
// copies hushbacks' form tile (Dashboard/Gallery/GalleryFormCard): an outer
// slab holding an inset well for the content, then a slim status row under it
// with a dot, the label, the time and a ⋯ menu.
import { useEffect, useState, type ReactNode } from "react";
import { ArrowRight, Clock, DotsThree, FolderSimple, GitPullRequest, Hash, WarningCircle } from "@/ui/icons";
import { Button } from "@/ui/button";
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger } from "@/ui/dropdown-menu";
import { cn } from "@/ui/lib/utils";
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

const SAMPLE: Task[] = [
  { id: "t1", key: "NEK-5AFF", kind: "stopped", title: "Fix TypeError in meal logging when portion is null", repo: "athena", source: "Sentry", age: 18, reason: "Actual changed files exceed verified or approved scope." },
  { id: "t2", key: "NEK-31C2", kind: "asks", title: "Coach chat drops messages after reconnect", repo: "athena", source: "HEA-3004", age: 64, reason: "Should reconnects replay every unread message, or only the last one?" },
  { id: "t3", key: "NEK-8E07", kind: "approve", title: "Bump expo-notifications to 0.29 and fix the iOS badge count", repo: "navihealth", source: "HEA-2987", pr: 4649, age: 125, reason: "The plan touches 14 files across two packages." },
  { id: "t4", key: "NEK-C410", kind: "review", title: "Cache food search results for 10 minutes", repo: "athena", source: "HEA-2975", pr: 4641, age: 35 },
  { id: "t5", key: "NEK-0B9D", kind: "running", title: "Rate-limit the /v2/insights endpoint per user", repo: "athena", source: "Sentry", age: 2, step: { label: "Running tests", at: 3, of: 5, seconds: 128 } },
  { id: "t6", key: "NEK-77A1", kind: "done", title: "Drop the unused analytics SDK", repo: "navihealth", pr: 4602, age: 250 },
];

const LABEL: Record<Kind, string> = { stopped: "Stopped", asks: "Asks you", approve: "Needs approval", review: "Ready to review", running: "Running", done: "Done" };
const TONE: Record<Kind, Tone> = { stopped: "failed", asks: "attention", approve: "attention", review: "review", running: "running", done: "done" };
const ACTION: Partial<Record<Kind, string>> = { stopped: "Start again", asks: "Answer", approve: "Approve" };
/** The 3px edge and the reason well, by state. */
const EDGE: Record<Tone, string> = { failed: "bg-destructive", attention: "bg-warning", review: "bg-info", running: "bg-primary", done: "bg-success", neutral: "bg-muted-foreground/50" };
const WELL: Partial<Record<Tone, string>> = { failed: "bg-destructive/[0.07] ring-destructive/15", attention: "bg-warning/[0.09] ring-warning/20" };

const age = (m: number) => (m < 60 ? `${m}m` : m < 1440 ? `${Math.round(m / 60)}h` : `${Math.round(m / 1440)}d`);
const clock = (s: number) => `${Math.floor(s / 60)}m ${String(s % 60).padStart(2, "0")}s`;
const FOCUS = "outline-none focus-visible:ring-2 focus-visible:ring-ring/60";

// ---------------------------------------------------------------------------
// State per option: the six tasks, the selected one, the action, a ticking clock.
// ---------------------------------------------------------------------------

type Mock = { tasks: Task[]; selected: string | null; select: (id: string) => void; act: (id: string) => void };

function useCards(): Mock {
  const [tasks, setTasks] = useState(SAMPLE);
  const [selected, setSelected] = useState<string | null>(null);
  useEffect(() => {
    // The live card's clock: one tick a second, only while something runs.
    if (!tasks.some((t) => t.kind === "running")) return;
    const id = setInterval(() => setTasks((all) => all.map((t) => (t.step ? { ...t, step: { ...t.step, seconds: t.step.seconds + 1 } } : t))), 1000);
    return () => clearInterval(id);
  }, [tasks.some((t) => t.kind === "running")]); // eslint-disable-line react-hooks/exhaustive-deps
  return {
    tasks, selected,
    select: (id) => setSelected((s) => (s === id ? null : id)),
    act: (id) => setTasks((all) => all.map((t) => (t.id === id
      ? { ...t, kind: "running", age: 0, reason: undefined, step: { label: t.kind === "asks" ? "Reading your answer" : "Planning", at: 1, of: 5, seconds: 0 } }
      : t))),
  };
}

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

/** 2. Header, body, footer: a quiet state row, the title, then hairline-separated slots. */
function SlottedCard({ t, m }: { t: Task; m: Mock }) {
  const tone = TONE[t.kind];
  const on = m.selected === t.id;
  const slots = [
    { key: "repo", icon: <FolderSimple size={13} aria-hidden />, text: t.repo },
    t.source ? { key: "source", icon: t.source === "Sentry" ? <WarningCircle size={13} aria-hidden /> : <Hash size={13} aria-hidden />, text: t.source } : null,
    t.pr ? { key: "pr", icon: <GitPullRequest size={13} aria-hidden />, text: String(t.pr) } : null,
  ].filter(Boolean) as { key: string; icon: ReactNode; text: string }[];
  return (
    <div data-slot="card" data-kind={t.kind} data-selected={on ? "" : undefined} className={cn(SHELL, "overflow-hidden rounded-xl border bg-card shadow-xs", ring(on))}>
      <Handle t={t} m={m} />
      <span className="pointer-events-none flex h-9 items-center gap-2 px-3.5 text-[12.5px]">
        <span aria-hidden className={cn("size-2 rounded-full", TONE_DOT[tone])} />
        <span className={cn("font-medium", TONE_TEXT[tone])}>{LABEL[t.kind]}</span>
        <span className="flex-1" />
        <span className="tabular-nums text-muted-foreground">{t.step ? clock(t.step.seconds) : age(t.age)}</span>
      </span>
      <span className="flex flex-col gap-1.5 px-3.5 pb-3.5">
        <span className="pointer-events-none line-clamp-2 text-[14.5px] font-medium leading-[1.4] text-strong">{t.title}</span>
        {t.reason ? <span className="pointer-events-none text-[13px] leading-[1.45] text-muted-foreground">{t.reason}</span> : null}
        {t.step ? <LiveText t={t} className="pointer-events-none text-[13px]" /> : null}
        {ACTION[t.kind] ? <span className="pt-1.5"><Action t={t} m={m} /></span> : null}
      </span>
      <span className="pointer-events-none flex h-9 items-stretch divide-x divide-border/70 border-t border-border/70 text-[12.5px] text-muted-foreground">
        {slots.map((s) => (
          <span key={s.key} className={cn("flex items-center gap-1.5 px-3", s.key === "repo" ? "min-w-0 flex-1 ps-3.5" : "shrink-0")}>
            {s.icon}<span className="truncate tabular-nums">{s.text}</span>
          </span>
        ))}
      </span>
    </div>
  );
}

/** 3. Accent edge: colour on a 3px edge instead of chips; meta as icon + text pairs. */
function EdgeCard({ t, m }: { t: Task; m: Mock }) {
  const on = m.selected === t.id;
  return (
    <div data-slot="card" data-kind={t.kind} data-selected={on ? "" : undefined} className={cn(SHELL, "gap-2 rounded-xl border bg-card py-3.5 ps-5 pe-4 shadow-xs", ring(on))}>
      <Handle t={t} m={m} />
      <span aria-hidden className={cn("pointer-events-none absolute inset-y-3 start-2 w-[3px] rounded-full", EDGE[TONE[t.kind]])} />
      <span className="sr-only">{LABEL[t.kind]}</span>
      <span className="pointer-events-none line-clamp-2 text-[14.5px] font-medium leading-[1.4] text-strong">{t.title}</span>
      {t.reason ? <span className="pointer-events-none text-[13px] leading-[1.5] text-muted-foreground">{t.reason}</span> : null}
      {t.step ? <LiveText t={t} className="pointer-events-none text-[13px]" /> : null}
      <span className="pointer-events-none flex flex-wrap items-center gap-x-3.5 gap-y-1 pt-0.5 text-[12.5px] text-muted-foreground">
        <span className="inline-flex items-center gap-1.5"><FolderSimple size={13} aria-hidden />{t.repo}</span>
        {t.source ? <span className="inline-flex items-center gap-1.5">{t.source === "Sentry" ? <WarningCircle size={13} aria-hidden /> : <Hash size={13} aria-hidden />}{t.source}</span> : null}
        <Pr n={t.pr} />
        <span className="inline-flex items-center gap-1.5 tabular-nums"><Clock size={13} aria-hidden />{age(t.age)}</span>
      </span>
      {ACTION[t.kind] ? (
        <span className="-ms-2 pt-0.5">
          <Button variant="ghost" size="xs" data-slot="card-action" className="relative z-10 text-foreground" onClick={() => m.act(t.id)}>
            {ACTION[t.kind]}<ArrowRight aria-hidden />
          </Button>
        </span>
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

function Column({ render }: { render: (t: Task, m: Mock) => ReactNode }) {
  const m = useCards();
  return (
    <div data-slot="card-column" className="flex flex-col gap-2.5 rounded-2xl bg-foreground/[0.03] p-2">
      {m.tasks.map((t) => <div key={t.id}>{render(t, m)}</div>)}
    </div>
  );
}

const OPTIONS: { id: string; name: string; caption: string; render: (t: Task, m: Mock) => ReactNode }[] = [
  {
    id: "spacious",
    name: "1. Spacious",
    caption: "18px of room, a 15px title, then one footer: where it lives on the left, age and PR on the right. A task that needs you gets a tinted well with the reason and its one action, so the ask is the loudest thing on the card. The column names every other state.",
    render: (t, m) => <SpaciousCard t={t} m={m} />,
  },
  {
    id: "slotted",
    name: "2. Header, body, footer",
    caption: "Linear’s issue card: a quiet header with the state and its age, the title as the body, and a footer of hairline-divided slots (repo, source, PR) that always sit in the same place.",
    render: (t, m) => <SlottedCard t={t} m={m} />,
  },
  {
    id: "edge",
    name: "3. Accent edge",
    caption: "The calmest: no chips, the state is a 3px coloured edge (and read out for screen readers). Meta is small icon and text pairs; the reason is a plain second paragraph; the action is a quiet link.",
    render: (t, m) => <EdgeCard t={t} m={m} />,
  },
  {
    id: "live",
    name: "4. Live-aware",
    caption: "Option 1, plus a live strip on running cards: the current step shimmering, its clock ticking, and how far through the run it is. Shows a card can carry what an agent is doing without opening it.",
    render: (t, m) => <SpaciousCard t={t} m={m} live />,
  },
  {
    id: "slab",
    name: "5. Slab and well",
    caption: "hushbacks’ form tile: an outer slab holding an inset well for what the task is, and a slim status row under it for where it stands (dot, state, time, action, ⋯ menu). Two clear slots, nothing competing.",
    render: (t, m) => <SlabCard t={t} m={m} />,
  },
];

export function CardOptions() {
  return (
    <div className="flex flex-wrap items-start gap-x-6 gap-y-10">
      {OPTIONS.map((o) => (
        <figure key={o.id} data-card-option={o.id} className="m-0 flex w-[316px] flex-col gap-3">
          <Column render={o.render} />
          <figcaption className="flex flex-col gap-1 px-1">
            <span className="text-sm font-semibold text-foreground">{o.name}</span>
            <span className="text-[13px] leading-snug text-muted-foreground">{o.caption}</span>
          </figcaption>
        </figure>
      ))}
    </div>
  );
}
