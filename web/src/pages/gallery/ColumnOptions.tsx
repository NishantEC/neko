// The gallery's "Board columns" section: four ways to lay out the Tasks page's board and its
// spacing (pages/work/Tasks.tsx, work/board/TaskBoard.tsx), each a full page mock (header,
// filter row and board) drawn from sample data with the chosen card (work/board/NotchCard.tsx,
// as-is). The real page is unchanged. Each option keeps its own local state: cards select,
// carry between columns with @dnd-kit (pointer, or Space then ← → and Space), act from their
// notch; columns fold and open; the stage tabs and the search filter. Nothing reaches the
// daemon.
//
// The spacing comes from hushbacks (origin/main, read only): its form library toolbar is one
// row of pill tabs with counts, a search well pushed right (`ms-auto w-60`) and a second pill
// group, 12px apart, 20px above the content (GalleryToolbar, FormLibrary); its sheet pads the
// page 24px and caps it at 1280px (DashboardGallery); its rows wash 4% on hover.
import { createContext, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import {
  DndContext, DragOverlay, KeyboardSensor, PointerSensor, pointerWithin, rectIntersection, useDraggable, useDroppable, useSensor, useSensors,
  type Announcements, type CollisionDetection, type KeyboardCoordinateGetter,
} from "@dnd-kit/core";
import { ArrowBendUpLeft, ArrowCounterClockwise, CaretLeft, CaretRight, Check, DotsThree, MagnifyingGlass, Plus, TreeStructure, X } from "@/ui/icons";
import { Kanban, Rows } from "@/work/icons";
import { Button } from "@/ui/button";
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger } from "@/ui/dropdown-menu";
import { EditorialHeading } from "@/editorial";
import { InputGroup, InputGroupAddon, InputGroupInput } from "@/ui/input-group";
import { Kbd } from "@/ui/kbd";
import { cn } from "@/ui/lib/utils";
import { SegmentedControl, SegmentedControlItem } from "@/ui/segmented-control";
import { Tabs, TabsList, TabsTab } from "@/ui/tabs";
import { StageFilters } from "@/work/StageFilters";
import { STAGE_EMPTY, STAGE_TITLE, STAGES, type Stage } from "@/work/model";
import { STAGE_TONE, TONE_DOT, type Tone } from "@/work/tone";
import { clock, NotchCard, type NotchCardProps } from "@/work/board/NotchCard";

// ---------------------------------------------------------------------------
// Sample tasks: a quiet day (nothing running or queued, a long Done) and a busy one.
// ---------------------------------------------------------------------------

type Kind = "stopped" | "asks" | "approve" | "review" | "running" | "upNext" | "done";
type Task = { id: string; kind: Kind; title: string; repo: string; source?: string; pr?: number; age: number; reason?: string; step?: { label: string; seconds: number } };
type Scenario = "quiet" | "busy";

const STAGE_OF: Record<Kind, Stage> = { stopped: "needsYou", asks: "needsYou", approve: "needsYou", review: "review", running: "running", upNext: "upNext", done: "done" };
const LABEL: Record<Kind, string> = { stopped: "Stopped", asks: "Asks you", approve: "Needs approval", review: "Ready to review", running: "Running", upNext: "Up next", done: "Done" };
const TONE: Record<Kind, Tone> = { stopped: "failed", asks: "attention", approve: "attention", review: "review", running: "running", upNext: "neutral", done: "done" };
const ACTION: Partial<Record<Kind, Pick<NonNullable<NotchCardProps["action"]>, "label" | "icon">>> = {
  stopped: { label: "Start again", icon: ArrowCounterClockwise },
  asks: { label: "Answer", icon: ArrowBendUpLeft },
  approve: { label: "Approve", icon: Check },
};

type Template = Omit<Task, "id" | "age">;
const NEEDS: Template[] = [
  { kind: "stopped", title: "Fix TypeError in meal logging when portion is null", repo: "athena", source: "Sentry", reason: "Actual changed files exceed verified or approved scope." },
  { kind: "asks", title: "Coach chat drops messages after reconnect", repo: "athena", source: "HEA-3004", reason: "Should reconnects replay unread messages or only the last one?" },
  { kind: "approve", title: "Bump expo-notifications to 0.29 and fix the iOS badge count", repo: "navihealth", source: "HEA-2987", pr: 4649, reason: "The plan touches 14 files across two packages." },
  { kind: "stopped", title: "Rate-limit the /v2/insights endpoint per user", repo: "athena", source: "Sentry", reason: "Tests failed three times on the same assertion." },
  { kind: "asks", title: "Daily summary email shows yesterday’s steps twice", repo: "athena", source: "HEA-3011", reason: "Which timezone should the summary use for travellers?" },
  { kind: "approve", title: "Remove the legacy onboarding flag from settings", repo: "navihealth", reason: "Deletes a migration; needs your go-ahead." },
  { kind: "stopped", title: "Water tracker resets at midnight UTC instead of local", repo: "navihealth", source: "HEA-2990", reason: "The branch has conflicts with master." },
];
const REVIEW: Template[] = [
  { kind: "review", title: "Cache food search results for 10 minutes", repo: "athena", source: "HEA-2975", pr: 4641 },
  { kind: "review", title: "Show the coach’s typing indicator in chat", repo: "navihealth", source: "HEA-2968", pr: 4638 },
  { kind: "review", title: "Fix the N+1 query in the meal plan list", repo: "athena", source: "Sentry", pr: 4652 },
  { kind: "review", title: "Dark mode colours for the progress ring", repo: "navihealth", pr: 4633 },
];
const RUNNING: Template[] = [
  { kind: "running", title: "Add retry to the Stripe webhook handler", repo: "athena", source: "HEA-3019", step: { label: "Running tests", seconds: 128 } },
  { kind: "running", title: "Profile photo upload fails above 4 MB on Android", repo: "navihealth", source: "Sentry", step: { label: "Editing 3 files", seconds: 41 } },
  { kind: "running", title: "KeyError: 'goal_id' in the weekly report worker", repo: "athena", source: "Sentry", step: { label: "Planning", seconds: 12 } },
];
const UP_NEXT: Template[] = [
  { kind: "upNext", title: "Log slow queries over 500 ms", repo: "athena" },
  { kind: "upNext", title: "Move the barcode scanner to the new camera API", repo: "navihealth", source: "HEA-3022" },
  { kind: "upNext", title: "Drop the unused analytics SDK", repo: "navihealth" },
  { kind: "upNext", title: "Paginate the coach’s client list", repo: "athena", source: "HEA-3025" },
];
const DONE: Template[] = [
  { kind: "done", title: "Update athena #4641 branch onto master", repo: "athena", pr: 4641 },
  { kind: "done", title: "ValueError in recipe import for empty ingredients", repo: "athena", source: "Sentry", pr: 4620 },
  { kind: "done", title: "Allow editing a logged meal’s time", repo: "navihealth", source: "HEA-2950", pr: 4611 },
  { kind: "done", title: "Retry push token registration on 503", repo: "navihealth", source: "Sentry", pr: 4598 },
  { kind: "done", title: "Trim the meal photo cache to 200 MB", repo: "navihealth" },
  { kind: "done", title: "Fix the streak counter after a skipped day", repo: "athena", source: "HEA-2941", pr: 4590 },
];

function fill(list: Template[], n: number, prefix: string, startAge: number): Task[] {
  return Array.from({ length: n }, (_, i) => ({ ...list[i % list.length], id: `${prefix}-${i}`, age: startAge + i * 37 + (i % 3) * 11 }));
}

function sample(s: Scenario): Task[] {
  return [
    ...fill(NEEDS, 7, "needs", 18),
    ...fill(REVIEW, 4, "review", 35),
    ...(s === "busy" ? [...fill(RUNNING, 3, "running", 2), ...fill(UP_NEXT, 4, "next", 6)] : []),
    ...fill(DONE, s === "busy" ? 13 : 81, "done", 120),
  ];
}

const age = (m: number) => (m < 1 ? "now" : m < 60 ? `${m}m` : m < 1440 ? `${Math.round(m / 60)}h` : `${Math.round(m / 1440)}d`);

// ---------------------------------------------------------------------------
// One mock per option: the tasks, the selection, the filters and moves.
// ---------------------------------------------------------------------------

type Mock = {
  tasks: Task[];
  byStage: Record<Stage, Task[]>;
  counts: Record<Stage, number>;
  selected: string | null;
  select: (id: string) => void;
  move: (id: string, to: Stage) => void;
  act: (t: Task) => void;
  stage: Stage | "all";
  setStage: (s: Stage | "all") => void;
  query: string;
  setQuery: (q: string) => void;
};
const MockContext = createContext<Mock | null>(null);
const useMock = () => useContext(MockContext)!;

function useMockPage(scenario: Scenario): Mock {
  const [tasks, setTasks] = useState(() => sample(scenario));
  const [selected, setSelected] = useState<string | null>(null);
  const [stage, setStage] = useState<Stage | "all">("all");
  const [query, setQuery] = useState("");
  const byStage = useMemo(() => {
    const q = query.trim().toLowerCase();
    const out = Object.fromEntries(STAGES.map((s) => [s, [] as Task[]])) as Record<Stage, Task[]>;
    for (const t of tasks) if (!q || `${t.title} ${t.repo} ${t.source ?? ""}`.toLowerCase().includes(q)) out[STAGE_OF[t.kind]].push(t);
    return out;
  }, [tasks, query]);
  const counts = useMemo(() => Object.fromEntries(STAGES.map((s) => [s, byStage[s].length])) as Record<Stage, number>, [byStage]);
  const move = (id: string, to: Stage) => setTasks((all) => {
    const t = all.find((x) => x.id === id);
    if (!t || STAGE_OF[t.kind] === to) return all;
    const kind: Kind = to === "needsYou" ? "approve" : to;
    const moved: Task = {
      ...t, kind, age: 0,
      reason: to === "needsYou" ? "You moved it back; approve to go on." : undefined,
      step: to === "running" ? { label: "Planning", seconds: 0 } : undefined,
    };
    return [moved, ...all.filter((x) => x.id !== id)];
  });
  return {
    tasks, byStage, counts, selected, stage, setStage, query, setQuery, move,
    select: (id) => setSelected((s) => (s === id ? null : id)),
    // Started again it queues; answered or approved it runs.
    act: (t) => move(t.id, t.kind === "stopped" ? "upNext" : "running"),
  };
}

/** Seconds since the section mounted, for running clocks; only live cards read it. */
const TickContext = createContext(0);
function TickProvider({ children }: { children: ReactNode }) {
  const [n, setN] = useState(0);
  useEffect(() => {
    const id = setInterval(() => setN((x) => x + 1), 1000);
    return () => clearInterval(id);
  }, []);
  return <TickContext.Provider value={n}>{children}</TickContext.Provider>;
}

// ---------------------------------------------------------------------------
// Dragging: one DndContext per option, carrying task ids onto stage ids.
// ---------------------------------------------------------------------------

const collide: CollisionDetection = (args) => {
  const within = pointerWithin(args);
  return within.length ? within : rectIntersection(args);
};

/** Arrows step through the drop targets in reading order (top to bottom, then left to right). */
const stepTargets: KeyboardCoordinateGetter = (event, { context: { droppableRects, droppableContainers, over, active } }) => {
  const step = event.code === "ArrowRight" || event.code === "ArrowDown" ? 1 : event.code === "ArrowLeft" || event.code === "ArrowUp" ? -1 : 0;
  if (!step) return undefined;
  const targets = droppableContainers.getEnabled()
    .map((c) => ({ id: String(c.id), rect: droppableRects.get(c.id) }))
    .filter((c): c is { id: string; rect: NonNullable<typeof c.rect> } => !!c.rect)
    .sort((a, b) => Math.round(a.rect.top / 80) - Math.round(b.rect.top / 80) || a.rect.left - b.rect.left);
  if (!targets.length) return undefined;
  const t = active?.data.current?.task as Task | undefined;
  const at = targets.findIndex((x) => x.id === String(over?.id ?? (t ? STAGE_OF[t.kind] : "")));
  const next = targets[Math.min(targets.length - 1, Math.max(0, (at < 0 ? 0 : at) + step))];
  event.preventDefault();
  return { x: next.rect.left + 16, y: next.rect.top + 16 };
};

function MockDnd({ mock, children }: { mock: Mock; children: ReactNode }) {
  const [active, setActive] = useState<Task | null>(null);
  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 6 } }),
    useSensor(KeyboardSensor, { coordinateGetter: stepTargets, keyboardCodes: { start: ["Space"], cancel: ["Escape"], end: ["Space", "Enter"] } }),
  );
  const find = (id: string | number) => mock.tasks.find((t) => t.id === String(id));
  const where = (id?: string | number | null) => (id ? STAGE_TITLE[String(id) as Stage] ?? "nowhere" : "nowhere");
  const announcements: Announcements = {
    onDragStart: ({ active: a }) => `Picked up ${find(a.id)?.title}.`,
    onDragOver: ({ active: a, over }) => `${find(a.id)?.title} is over ${where(over?.id)}.`,
    onDragEnd: ({ active: a, over }) => (over ? `${find(a.id)?.title} moved to ${where(over.id)}.` : `${find(a.id)?.title} stays where it was.`),
    onDragCancel: ({ active: a }) => `Move cancelled. ${find(a.id)?.title} stays where it was.`,
  };
  return (
    <MockContext.Provider value={mock}>
      <DndContext
        sensors={sensors}
        collisionDetection={collide}
        accessibility={{ announcements, screenReaderInstructions: { draggable: "Press Space to pick up the task, the arrow keys to choose a column, and Space to drop it. Escape cancels." } }}
        onDragStart={(e) => setActive(find(e.active.id) ?? null)}
        onDragEnd={(e) => { setActive(null); if (e.over) mock.move(String(e.active.id), String(e.over.id) as Stage); }}
        onDragCancel={() => setActive(null)}
      >
        {children}
        <DragOverlay dropAnimation={{ duration: 180, easing: "cubic-bezier(0.22, 1, 0.36, 1)" }}>
          {active ? <NotchCard {...cardProps(active, null, () => {})} lifted /> : null}
        </DragOverlay>
      </DndContext>
    </MockContext.Provider>
  );
}

// ---------------------------------------------------------------------------
// The card: NotchCard as chosen, at the board's density (compact).
// ---------------------------------------------------------------------------

function cardProps(t: Task, clockText: string | null, act: () => void): NotchCardProps {
  const verb = ACTION[t.kind];
  const done = t.kind === "done";
  return {
    tone: TONE[t.kind],
    title: t.title,
    density: "compact",
    state: done ? null : { label: LABEL[t.kind], age: age(t.age), detail: verb || t.pr ? undefined : t.repo },
    live: t.kind === "running" && t.step ? { step: t.step.label, clock: clockText ?? clock(t.step.seconds) } : null,
    reason: t.reason ?? null,
    meta: <>{t.repo}{t.source ? <> · {t.source}</> : null}{done ? <> · {age(t.age)}</> : null}</>,
    pr: t.pr ?? null,
    action: verb ? { ...verb, onClick: act } : null,
  };
}

function CardBody({ t, clockText }: { t: Task; clockText: string | null }) {
  const m = useMock();
  const { attributes, listeners, setNodeRef, isDragging } = useDraggable({ id: t.id, data: { task: t } });
  const { role: _role, "aria-pressed": _pressed, ...aria } = attributes as Record<string, any>;
  const on = m.selected === t.id;
  const label = `${t.title}. ${LABEL[t.kind]}${t.reason ? `: ${t.reason}` : ""}. ${t.repo}, ${age(t.age)}${t.pr ? `, pull request ${t.pr}` : ""}`;
  return (
    <div ref={setNodeRef} data-slot="col-card" data-kind={t.kind} data-selected={on ? "" : undefined} className="w-full min-w-0">
      <NotchCard
        {...cardProps(t, clockText, () => m.act(t))}
        selected={on}
        ghost={isDragging}
        handle={
          <button
            type="button"
            {...aria}
            {...listeners}
            aria-roledescription="draggable task"
            aria-current={on ? "true" : undefined}
            aria-label={label}
            onClick={() => m.select(t.id)}
            className="absolute inset-0 cursor-default rounded-[16px] outline-none"
          />
        }
      />
    </div>
  );
}

function LiveCard({ t }: { t: Task }) {
  const tick = useContext(TickContext);
  return <CardBody t={t} clockText={clock((t.step?.seconds ?? 0) + tick)} />;
}

function Card({ t }: { t: Task }) {
  return t.kind === "running" ? <LiveCard t={t} /> : <CardBody t={t} clockText={null} />;
}

// ---------------------------------------------------------------------------
// Parts every page shares
// ---------------------------------------------------------------------------

function Dot({ stage }: { stage: Stage }) {
  return <span aria-hidden className={cn("size-2 shrink-0 rounded-full", TONE_DOT[STAGE_TONE[stage]])} />;
}

function ColHeader({ stage, count, children, className }: { stage: Stage; count: number; children?: ReactNode; className?: string }) {
  return (
    <div data-slot="col-header" className={cn("flex h-8 shrink-0 items-center gap-2", className)}>
      <Dot stage={stage} />
      <h4 className="truncate text-[13.5px] font-semibold text-strong">{STAGE_TITLE[stage]}</h4>
      <span className="text-[12.5px] tabular-nums text-muted-foreground">{count}</span>
      <span className="flex-1" />
      {children}
    </div>
  );
}

/** ⋯ on a column: show only this stage, and whatever the option adds. With `hover`, it shows on hover or focus. */
function ColMenu({ stage, hover, children }: { stage: Stage; hover?: boolean; children?: ReactNode }) {
  const m = useMock();
  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        render={
          <Button
            variant="ghost"
            size="icon-xs"
            aria-label={`${STAGE_TITLE[stage]} options`}
            data-slot="col-menu"
            className={hover ? "opacity-0 transition-opacity duration-150 group-hover/col:opacity-100 focus-visible:opacity-100 data-[popup-open]:opacity-100 motion-reduce:transition-none" : undefined}
          />
        }
      >
        <DotsThree weight="bold" />
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end">
        <DropdownMenuItem onClick={() => m.setStage(stage)}>Show only {STAGE_TITLE[stage]}</DropdownMenuItem>
        {children}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

function EmptyNote({ stage, className }: { stage: Stage; className?: string }) {
  return <p data-slot="col-empty" className={cn("text-[12.5px] leading-[1.5] text-muted-foreground", className)}>{STAGE_EMPTY[stage]}</p>;
}

/** Done shows its latest few, and "Show all 81" opens the rest. */
function DoneCap({ cards, cap, children }: { cards: Task[]; cap: number; children: (shown: Task[]) => ReactNode }) {
  const [all, setAll] = useState(false);
  const shown = all ? cards : cards.slice(0, cap);
  return (
    <>
      {children(shown)}
      {cards.length > cap ? (
        <button
          type="button"
          data-slot="show-all"
          aria-expanded={all}
          onClick={() => setAll((v) => !v)}
          className="h-8 shrink-0 rounded-xl text-[12.5px] font-medium text-muted-foreground outline-none transition-colors duration-150 hover:bg-foreground/[0.04] hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring/60"
        >
          {all ? "Show fewer" : `Show all ${cards.length}`}
        </button>
      ) : null}
    </>
  );
}

function CardList({ stage, cap }: { stage: Stage; cap?: number }) {
  const cards = useMock().byStage[stage];
  if (!cards.length) return <EmptyNote stage={stage} className="px-1 py-1" />;
  if (cap) return <DoneCap cards={cards} cap={cap}>{(shown) => shown.map((t) => <Card key={t.id} t={t} />)}</DoneCap>;
  return <>{cards.map((t) => <Card key={t.id} t={t} />)}</>;
}

function ViewSwitch({ icons }: { icons?: boolean }) {
  const [v, setV] = useState("board");
  return (
    <SegmentedControl value={v} onValueChange={setV} aria-label="Layout">
      <SegmentedControlItem value="list" aria-label="List" title="List"><Rows />{icons ? null : " List"}</SegmentedControlItem>
      <SegmentedControlItem value="board" aria-label="Board" title="Board"><Kanban />{icons ? null : " Board"}</SegmentedControlItem>
      <SegmentedControlItem value="tree" aria-label="Tree" title="Tree"><TreeStructure />{icons ? null : " Tree"}</SegmentedControlItem>
    </SegmentedControl>
  );
}

function Search({ className, well }: { className?: string; well?: boolean }) {
  const m = useMock();
  return (
    <InputGroup className={cn(well ? "sky-well sky-control border-transparent bg-transparent shadow-none" : "h-8", className)}>
      <InputGroupAddon><MagnifyingGlass aria-hidden /></InputGroupAddon>
      <InputGroupInput value={m.query} onChange={(e) => m.setQuery(e.target.value)} placeholder="Search tasks" aria-label="Search tasks" />
      <InputGroupAddon align="inline-end">
        {m.query ? (
          <button type="button" aria-label="Clear search" onClick={() => m.setQuery("")} className="rounded-sm text-muted-foreground outline-none hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring"><X size={13} /></button>
        ) : <Kbd>/</Kbd>}
      </InputGroupAddon>
    </InputGroup>
  );
}

function MoreButton() {
  return <Button variant="ghost" size="icon-sm" aria-label="Work options"><DotsThree weight="bold" /></Button>;
}

function NewTask() {
  return <Button size="sm"><Plus /> New task</Button>;
}

/** Neko's header today (WorkHeader), minus the workspace dropdown the sidebar's switcher replaces. */
function NekoHeader() {
  const m = useMock();
  return (
    <header data-slot="mock-header" className="shrink-0 px-10 pt-8 pb-5">
      <div className="flex items-end gap-6">
        <div className="flex min-w-0 flex-col gap-2">
          <EditorialHeading as="h3" className="text-[22px]">{m.stage === "all" ? "All tasks" : STAGE_TITLE[m.stage]}</EditorialHeading>
          <p className="text-[13.5px] text-muted-foreground">Nishant · {m.tasks.length} tasks</p>
        </div>
        <div className="ms-auto flex items-center gap-2"><ViewSwitch /><MoreButton /><NewTask /></div>
      </div>
      <div className="mt-4 flex items-center gap-2" role="search" aria-label="Filter tasks">
        <StageFilters stage={m.stage} counts={m.counts} onChange={m.setStage} />
        <span className="flex-1" />
        <Search className="w-56" />
      </div>
    </header>
  );
}

function Frame({ children }: { children: ReactNode }) {
  return <div data-slot="page-mock" className="relative flex h-[720px] w-full flex-col overflow-hidden rounded-2xl border border-border bg-background">{children}</div>;
}

const DROP_RING = "ring-1 ring-primary/40 bg-primary/[0.05]";

// ---------------------------------------------------------------------------
// A. Calm columns
// ---------------------------------------------------------------------------

function CalmHeader() {
  const m = useMock();
  const total = m.tasks.length;
  return (
    <header data-slot="mock-header" className="shrink-0 px-10 pt-10 pb-6">
      <div className="flex items-baseline gap-3">
        <EditorialHeading as="h3" className="text-[22px]">Tasks</EditorialHeading>
        <span className="text-[13.5px] tabular-nums text-muted-foreground">{total}</span>
        <span className="flex-1" />
        <MoreButton />
        <NewTask />
      </div>
      <div className="mt-6 flex items-center gap-3" role="search" aria-label="Filter tasks">
        <Tabs value={m.stage} onValueChange={(v) => m.setStage(v as Stage | "all")} className="sky-tabs">
          <TabsList aria-label="Show tasks in">
            <TabsTab value="all">All<span className="text-xs tabular-nums text-muted-foreground">{total}</span></TabsTab>
            {STAGES.map((s) => (
              <TabsTab key={s} value={s}>{STAGE_TITLE[s]}<span className="text-xs tabular-nums text-muted-foreground">{m.counts[s]}</span></TabsTab>
            ))}
          </TabsList>
        </Tabs>
        <Search well className="ms-auto w-52" />
        <ViewSwitch icons />
      </div>
    </header>
  );
}

function CalmColumn({ stage }: { stage: Stage }) {
  const m = useMock();
  const cards = m.byStage[stage];
  const empty = cards.length === 0;
  const { setNodeRef, isOver } = useDroppable({ id: stage });
  return (
    <section
      ref={setNodeRef}
      data-slot="col"
      data-stage={stage}
      data-empty={empty ? "" : undefined}
      aria-label={`${STAGE_TITLE[stage]}, ${cards.length}`}
      className={cn(
        "group/col -mx-2 flex min-h-0 flex-col rounded-2xl px-2 transition-[background-color,box-shadow] duration-150",
        empty ? "w-[132px] shrink-0" : "min-w-[208px] flex-1",
        isOver && DROP_RING,
      )}
    >
      <ColHeader stage={stage} count={cards.length} className="px-0.5">{empty ? null : <ColMenu stage={stage} hover />}</ColHeader>
      <div className="-mx-1 flex min-h-0 flex-1 flex-col gap-2.5 overflow-y-auto px-1 pt-1.5 pb-6">
        <CardList stage={stage} cap={stage === "done" ? 5 : undefined} />
      </div>
    </section>
  );
}

function OptionCalm({ scenario }: { scenario: Scenario }) {
  const mock = useMockPage(scenario);
  const shown = mock.stage === "all" ? STAGES : [mock.stage];
  return (
    <MockDnd mock={mock}>
      <Frame>
        <CalmHeader />
        <div className="flex min-h-0 flex-1 gap-7 overflow-x-auto px-10">
          {shown.map((s) => <CalmColumn key={s} stage={s} />)}
        </div>
      </Frame>
    </MockDnd>
  );
}

// ---------------------------------------------------------------------------
// B. Fitted columns
// ---------------------------------------------------------------------------

function Strip({ stage, onOpen }: { stage: Stage; onOpen: () => void }) {
  const count = useMock().byStage[stage].length;
  const { setNodeRef, isOver } = useDroppable({ id: stage });
  return (
    <button
      ref={setNodeRef}
      type="button"
      data-slot="col-strip"
      data-stage={stage}
      aria-expanded={false}
      aria-label={`${STAGE_TITLE[stage]}, ${count}. Show column`}
      onClick={onOpen}
      className={cn(
        "flex h-fit w-11 shrink-0 flex-col items-center gap-2.5 rounded-2xl bg-foreground/[0.03] py-3 text-muted-foreground outline-none transition-[background-color,box-shadow] duration-150",
        "hover:bg-foreground/[0.06] hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring/60",
        isOver && DROP_RING,
      )}
    >
      <Dot stage={stage} />
      <span className="text-[13px] font-medium text-foreground/80 [writing-mode:vertical-rl]">{STAGE_TITLE[stage]}</span>
      <span className="text-[12px] tabular-nums">{count}</span>
      <CaretRight size={12} weight="bold" aria-hidden />
    </button>
  );
}

function FittedColumn({ stage, onFold }: { stage: Stage; onFold: () => void }) {
  const m = useMock();
  const cards = m.byStage[stage];
  const { setNodeRef, isOver } = useDroppable({ id: stage });
  // A busy column takes twice the share, and its cards flow two across.
  const busy = cards.length >= 6;
  return (
    <section
      ref={setNodeRef}
      data-slot="col"
      data-stage={stage}
      aria-label={`${STAGE_TITLE[stage]}, ${cards.length}`}
      style={{ flexGrow: busy ? 2 : 1 }}
      className={cn("flex min-h-0 min-w-[220px] basis-0 flex-col rounded-2xl bg-foreground/[0.03] p-1.5 transition-[background-color,box-shadow] duration-150", isOver && DROP_RING)}
    >
      <ColHeader stage={stage} count={cards.length} className="px-2">
        <Button variant="ghost" size="icon-xs" aria-label={`Fold ${STAGE_TITLE[stage]}`} aria-expanded title="Fold into a strip" onClick={onFold}><CaretLeft weight="bold" /></Button>
        <ColMenu stage={stage} />
      </ColHeader>
      <div className="grid min-h-0 flex-1 auto-rows-min content-start gap-2 overflow-y-auto px-0.5 pt-1 pb-3 [grid-template-columns:repeat(auto-fill,minmax(250px,1fr))]">
        <CardList stage={stage} />
      </div>
    </section>
  );
}

function OptionFitted({ scenario }: { scenario: Scenario }) {
  const mock = useMockPage(scenario);
  const [folded, setFolded] = useState<Partial<Record<Stage, boolean>>>({});
  const isFolded = (s: Stage) => folded[s] ?? (s === "done" || mock.byStage[s].length === 0);
  const set = (s: Stage, v: boolean) => setFolded((f) => ({ ...f, [s]: v }));
  const shown = mock.stage === "all" ? STAGES : [mock.stage];
  return (
    <MockDnd mock={mock}>
      <Frame>
        <NekoHeader />
        <div className="flex min-h-0 flex-1 gap-2.5 overflow-x-auto px-10 pb-5">
          {shown.map((s) => (isFolded(s) && mock.stage === "all"
            ? <Strip key={s} stage={s} onOpen={() => set(s, false)} />
            : <FittedColumn key={s} stage={s} onFold={() => set(s, true)} />))}
        </div>
      </Frame>
    </MockDnd>
  );
}

// ---------------------------------------------------------------------------
// C. Needs-you lane, Done as a drawer
// ---------------------------------------------------------------------------

function LaneHeader() {
  const m = useMock();
  return (
    <header data-slot="mock-header" className="flex shrink-0 items-center gap-3 px-8 pt-6 pb-5">
      <EditorialHeading as="h3" className="text-[18px]">Tasks</EditorialHeading>
      <span className="text-[13px] tabular-nums text-muted-foreground">{m.tasks.length}</span>
      <span className="flex-1" />
      <ViewSwitch icons />
      <Search className="w-52" />
      <NewTask />
    </header>
  );
}

function LaneColumn({ stage, wide }: { stage: Stage; wide?: boolean }) {
  const m = useMock();
  const cards = m.byStage[stage];
  const { setNodeRef, isOver } = useDroppable({ id: stage });
  return (
    <section
      ref={setNodeRef}
      data-slot="col"
      data-stage={stage}
      data-empty={cards.length ? undefined : ""}
      aria-label={`${STAGE_TITLE[stage]}, ${cards.length}`}
      style={{ flexGrow: wide ? 1.9 : cards.length ? 1 : 0.6 }}
      className={cn(
        "flex min-h-0 basis-0 flex-col rounded-2xl p-1.5 transition-[background-color,box-shadow] duration-150",
        wide ? "min-w-[300px] bg-warning/[0.06]" : cards.length ? "min-w-[220px] bg-foreground/[0.03]" : "min-w-[176px] bg-foreground/[0.03]",
        isOver && DROP_RING,
      )}
    >
      <ColHeader stage={stage} count={cards.length} className="px-2"><ColMenu stage={stage} /></ColHeader>
      <div className={cn("min-h-0 flex-1 overflow-y-auto px-0.5 pt-1 pb-3", wide ? "grid auto-rows-min content-start gap-2 [grid-template-columns:repeat(auto-fill,minmax(210px,1fr))]" : "flex flex-col gap-2")}>
        <CardList stage={stage} />
      </div>
    </section>
  );
}

function DoneDrawer() {
  const m = useMock();
  const [open, setOpen] = useState(false);
  const count = m.byStage.done.length;
  const { setNodeRef, isOver } = useDroppable({ id: "done" });
  if (!open) {
    return (
      <button
        ref={setNodeRef}
        type="button"
        data-slot="done-drawer-tab"
        aria-expanded={false}
        aria-label={`Done, ${count}. Open the drawer`}
        onClick={() => setOpen(true)}
        className={cn(
          "flex w-10 shrink-0 flex-col items-center gap-2.5 rounded-s-2xl border border-e-0 border-border bg-card py-4 text-muted-foreground outline-none transition-[background-color,box-shadow] duration-150",
          "hover:bg-foreground/[0.04] hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring/60",
          isOver && DROP_RING,
        )}
      >
        <CaretLeft size={12} weight="bold" aria-hidden />
        <Dot stage="done" />
        <span className="text-[13px] font-medium text-foreground/80 [writing-mode:vertical-rl]">Done</span>
        <span className="text-[12px] tabular-nums">{count}</span>
      </button>
    );
  }
  return (
    <aside
      ref={setNodeRef}
      data-slot="done-drawer"
      aria-label={`Done, ${count}`}
      className={cn("flex w-[300px] shrink-0 flex-col border-s border-border bg-card/60 ps-3 pe-4 pt-1 transition-[background-color,box-shadow] duration-150", isOver && DROP_RING)}
    >
      <ColHeader stage="done" count={count} className="px-1">
        <Button variant="ghost" size="icon-xs" aria-label="Close Done" onClick={() => setOpen(false)}><CaretRight weight="bold" /></Button>
      </ColHeader>
      <div className="-mx-1 flex min-h-0 flex-1 flex-col gap-2 overflow-y-auto px-1 pt-1 pb-4">
        <CardList stage="done" cap={12} />
      </div>
    </aside>
  );
}

function OptionLane({ scenario }: { scenario: Scenario }) {
  const mock = useMockPage(scenario);
  return (
    <MockDnd mock={mock}>
      <Frame>
        <LaneHeader />
        <div className="flex min-h-0 flex-1">
          <div className="flex min-h-0 min-w-0 flex-1 gap-2.5 overflow-x-auto ps-8 pe-4 pb-5">
            <LaneColumn stage="needsYou" wide />
            <LaneColumn stage="review" />
            <LaneColumn stage="running" />
            <LaneColumn stage="upNext" />
          </div>
          <div className="flex min-h-0 items-start pb-5"><DoneDrawer /></div>
        </div>
      </Frame>
    </MockDnd>
  );
}

// ---------------------------------------------------------------------------
// D. Two rows: Needs you as a shelf on top, the rest below with sticky headers
// ---------------------------------------------------------------------------

function Shelf() {
  const m = useMock();
  const cards = m.byStage.needsYou;
  const { setNodeRef, isOver } = useDroppable({ id: "needsYou" });
  const row = useRef<HTMLDivElement>(null);
  const page = (dir: 1 | -1) => row.current?.scrollBy({ left: dir * (row.current.clientWidth - 80), behavior: matchMedia("(prefers-reduced-motion: reduce)").matches ? "auto" : "smooth" });
  return (
    <section
      ref={setNodeRef}
      data-slot="shelf"
      data-stage="needsYou"
      aria-label={`Needs you, ${cards.length}`}
      className={cn("shrink-0 rounded-2xl bg-warning/[0.06] p-1.5 transition-[background-color,box-shadow] duration-150", isOver && DROP_RING)}
    >
      <ColHeader stage="needsYou" count={cards.length} className="px-2">
        <Button variant="ghost" size="icon-xs" aria-label="Scroll back" onClick={() => page(-1)}><CaretLeft weight="bold" /></Button>
        <Button variant="ghost" size="icon-xs" aria-label="Scroll on" onClick={() => page(1)}><CaretRight weight="bold" /></Button>
        <ColMenu stage="needsYou" />
      </ColHeader>
      <div ref={row} className="flex snap-x snap-mandatory scroll-px-1 items-start gap-2 overflow-x-auto px-1 pt-1 pb-2">
        {cards.length
          ? cards.map((t) => <div key={t.id} className="w-[272px] shrink-0 snap-start"><Card t={t} /></div>)
          : <EmptyNote stage="needsYou" className="px-1 py-1" />}
      </div>
    </section>
  );
}

function RowColumn({ stage }: { stage: Stage }) {
  const m = useMock();
  const cards = m.byStage[stage];
  const { setNodeRef, isOver } = useDroppable({ id: stage });
  return (
    <section
      ref={setNodeRef}
      data-slot="col"
      data-stage={stage}
      aria-label={`${STAGE_TITLE[stage]}, ${cards.length}`}
      className={cn("group/col flex min-w-[220px] flex-1 flex-col rounded-2xl px-1.5 transition-[background-color,box-shadow] duration-150", isOver && DROP_RING)}
    >
      <div data-slot="sticky-header" className="sticky top-0 z-10 -mx-1.5 bg-background/90 px-1.5 backdrop-blur">
        <ColHeader stage={stage} count={cards.length} className="px-1"><ColMenu stage={stage} hover /></ColHeader>
      </div>
      <div className="flex flex-col gap-2 pt-1 pb-5">
        <CardList stage={stage} cap={stage === "done" ? 6 : undefined} />
      </div>
    </section>
  );
}

function OptionRows({ scenario }: { scenario: Scenario }) {
  const mock = useMockPage(scenario);
  const lower: Stage[] = ["review", "running", "upNext", "done"];
  const shelf = mock.stage === "all" || mock.stage === "needsYou";
  const cols = mock.stage === "all" ? lower : lower.filter((s) => s === mock.stage);
  return (
    <MockDnd mock={mock}>
      <Frame>
        <NekoHeader />
        <div className="min-h-0 flex-1 overflow-y-auto px-10">
          <div className="flex flex-col gap-4">
            {shelf ? <Shelf /> : null}
            {cols.length ? <div className="flex gap-4">{cols.map((s) => <RowColumn key={s} stage={s} />)}</div> : null}
          </div>
        </div>
      </Frame>
    </MockDnd>
  );
}

// ---------------------------------------------------------------------------
// The section
// ---------------------------------------------------------------------------

const OPTIONS: { id: string; name: string; caption: string; render: (s: Scenario) => ReactNode }[] = [
  {
    id: "calm",
    name: "A. Calm columns",
    caption: "No panels, just space between columns; empty ones shrink to a slim note, Done shows its latest five, and one toolbar row (stage tabs left, search and view right) under a roomier header.",
    render: (s) => <OptionCalm scenario={s} />,
  },
  {
    id: "fitted",
    name: "B. Fitted columns",
    caption: "Panels kept but sized by their work: empty columns and Done fold to strips (click to open, drop to move), a busy column takes twice the share and lays its cards two across.",
    render: (s) => <OptionFitted scenario={s} />,
  },
  {
    id: "lane",
    name: "C. Needs-you lane",
    caption: "One-line header; Needs you is the wide, tinted first lane, Ready, Running and Up next follow, and Done is a drawer at the right edge: click to open it, drop on it while closed.",
    render: (s) => <OptionLane scenario={s} />,
  },
  {
    id: "rows",
    name: "D. Needs you on a shelf",
    caption: "Needs you runs across the top as a shelf that scrolls sideways and snaps per card; the other four sit below in one scroll with headers that stick, so no column runs off the right edge.",
    render: (s) => <OptionRows scenario={s} />,
  },
];

export function ColumnOptions() {
  const [scenario, setScenario] = useState<Scenario>("quiet");
  return (
    <TickProvider>
      <div className="flex flex-col gap-10">
        <SegmentedControl aria-label="Sample day" value={scenario} onValueChange={(v) => setScenario(v as Scenario)} className="self-start">
          <SegmentedControlItem value="quiet">A quiet day: nothing running, Done 81</SegmentedControlItem>
          <SegmentedControlItem value="busy">A busy day: every column has work</SegmentedControlItem>
        </SegmentedControl>
        {OPTIONS.map((o) => (
          <figure key={o.id} data-column-option={o.id} className="m-0 flex flex-col gap-3">
            <figcaption className="flex flex-col gap-1 px-1">
              <span className="text-sm font-semibold text-foreground">{o.name}</span>
              <span className="max-w-3xl text-[13px] leading-snug text-muted-foreground">{o.caption}</span>
            </figcaption>
            <div key={scenario} className="contents">{o.render(scenario)}</div>
          </figure>
        ))}
      </div>
    </TickProvider>
  );
}
