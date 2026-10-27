// The gallery's "Board" section: ways to redesign the Tasks board
// (work/board/TaskBoard.tsx and BoardCard.tsx), drawn side by side from static
// sample data so the owner can try each before picking one. The real board is
// unchanged. Every option works on local state only: cards select, columns
// fold and open, and cards carry between columns with @dnd-kit (pointer, or
// Space then ← → and Space). Nothing reaches the daemon.
//
// What the owner's real board got wrong: five equal columns while Running and
// Up next sit empty, ~240px cards (four per column), a status chip that only
// repeats the column, and a runtime avatar that reads as noise. The density and
// the calm come from hushbacks: its notification rows (title, one muted detail
// line, the age, inline actions under the text) and its responses list (a 4%
// hover wash, 7% when selected, 13px text, 12-12.5px muted meta).
import { createContext, useContext, useMemo, useState, type ReactNode } from "react";
import {
  DndContext, DragOverlay, KeyboardSensor, PointerSensor, pointerWithin, rectIntersection, useDraggable, useDroppable, useSensor, useSensors,
  type Announcements, type CollisionDetection, type KeyboardCoordinateGetter,
} from "@dnd-kit/core";
import { CaretDown, CaretLeft, CaretRight, GitPullRequest } from "@/ui/icons";
import { Button } from "@/ui/button";
import { Chip, type ChipTone } from "@/ui/chip";
import { cn } from "@/ui/lib/utils";
import { STAGE_EMPTY, STAGE_TITLE, STAGES, type Stage } from "@/work/model";
import { STAGE_TONE, StatusGlyph, TONE_DOT } from "@/work/tone";
import { sampleBoard, shortAge, WAITING_ACTION, WAITING_LABEL, type SampleCard, type Waiting } from "@/pages/gallery/board-sample";

// ---------------------------------------------------------------------------
// One piece of state per option: the cards, the selected one, and moves.
// ---------------------------------------------------------------------------

type Mock = {
  cards: SampleCard[];
  byStage: Record<Stage, SampleCard[]>;
  selected: string | null;
  select: (id: string) => void;
  move: (id: string, to: Stage) => void;
};
const MockContext = createContext<Mock | null>(null);
const useMock = () => useContext(MockContext)!;

function useMockBoard(): Mock {
  const [cards, setCards] = useState(sampleBoard);
  const [selected, setSelected] = useState<string | null>(null);
  const byStage = useMemo(() => {
    const out = Object.fromEntries(STAGES.map((s) => [s, [] as SampleCard[]])) as Record<Stage, SampleCard[]>;
    for (const c of cards) out[c.stage].push(c);
    return out;
  }, [cards]);
  return {
    cards, byStage, selected,
    select: (id) => setSelected((s) => (s === id ? null : id)),
    move: (id, to) => setCards((all) => {
      const card = all.find((c) => c.id === id);
      if (!card || card.stage === to) return all;
      // A moved card goes to the top of its new column, freshly touched.
      const moved: SampleCard = { ...card, stage: to, age: 0, waiting: to === "needsYou" ? card.waiting ?? "stopped" : card.waiting };
      return [moved, ...all.filter((c) => c.id !== id)];
    }),
  };
}

const FOCUS = "outline-none focus-visible:ring-2 focus-visible:ring-ring/60";
const WAITING_TONE: Record<Waiting, ChipTone> = { stopped: "failed", asks: "attention", approve: "attention" };
/** Where a Needs-you card goes when you act on it: started again it queues; answered or approved it runs. */
const afterAction = (w: Waiting): Stage => (w === "stopped" ? "upNext" : "running");
const DROP = "data-[over]:bg-primary/[0.06] data-[over]:ring-1 data-[over]:ring-primary/40";

// ---------------------------------------------------------------------------
// Dragging: one DndContext per option, carrying card ids onto stage ids.
// ---------------------------------------------------------------------------

const collide: CollisionDetection = (args) => {
  const within = pointerWithin(args);
  return within.length ? within : rectIntersection(args);
};

/** Arrows step through the drop targets in reading order (left to right, then top to bottom). */
const stepTargets: KeyboardCoordinateGetter = (event, { context: { droppableRects, droppableContainers, over, active } }) => {
  const step = event.code === "ArrowRight" || event.code === "ArrowDown" ? 1 : event.code === "ArrowLeft" || event.code === "ArrowUp" ? -1 : 0;
  if (!step) return undefined;
  const targets = droppableContainers.getEnabled()
    .map((c) => ({ id: String(c.id), rect: droppableRects.get(c.id) }))
    .filter((c): c is { id: string; rect: NonNullable<typeof c.rect> } => !!c.rect)
    .sort((a, b) => a.rect.left - b.rect.left || a.rect.top - b.rect.top);
  if (!targets.length) return undefined;
  const home = (active?.data.current?.card as SampleCard | undefined)?.stage;
  const at = targets.findIndex((t) => t.id === String(over?.id ?? home ?? ""));
  const next = targets[Math.min(targets.length - 1, Math.max(0, (at < 0 ? 0 : at) + step))];
  event.preventDefault();
  return { x: next.rect.left + 16, y: next.rect.top + 16 };
};

function MockDnd({ mock, overlay, children }: { mock: Mock; overlay: (card: SampleCard) => ReactNode; children: ReactNode }) {
  const [active, setActive] = useState<SampleCard | null>(null);
  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 6 } }),
    useSensor(KeyboardSensor, { coordinateGetter: stepTargets, keyboardCodes: { start: ["Space"], cancel: ["Escape"], end: ["Space", "Enter"] } }),
  );
  const card = (id: string | number) => mock.cards.find((c) => c.id === String(id));
  const where = (id?: string | number | null) => (id ? STAGE_TITLE[String(id) as Stage] ?? "nowhere" : "nowhere");
  const announcements: Announcements = {
    onDragStart: ({ active: a }) => `Picked up ${card(a.id)?.title}.`,
    onDragOver: ({ active: a, over }) => `${card(a.id)?.title} is over ${where(over?.id)}.`,
    onDragEnd: ({ active: a, over }) => (over ? `${card(a.id)?.title} moved to ${where(over.id)}.` : `${card(a.id)?.title} stays where it was.`),
    onDragCancel: ({ active: a }) => `Move cancelled. ${card(a.id)?.title} stays where it was.`,
  };
  return (
    <MockContext.Provider value={mock}>
      <DndContext
        sensors={sensors}
        collisionDetection={collide}
        accessibility={{ announcements, screenReaderInstructions: { draggable: "Press Space to pick up the task, the arrow keys to choose a column, and Space to drop it. Escape cancels." } }}
        onDragStart={(e) => setActive(card(e.active.id) ?? null)}
        onDragEnd={(e) => { setActive(null); if (e.over) mock.move(String(e.active.id), String(e.over.id) as Stage); }}
        onDragCancel={() => setActive(null)}
      >
        {children}
        <DragOverlay dropAnimation={{ duration: 180, easing: "cubic-bezier(0.22, 1, 0.36, 1)" }}>{active ? overlay(active) : null}</DragOverlay>
      </DndContext>
    </MockContext.Provider>
  );
}

/** The card's hit area: a full-size button under the content, which drags and selects. */
function useCardHandle(card: SampleCard) {
  const { selected, select } = useMock();
  const { attributes, listeners, setNodeRef, isDragging } = useDraggable({ id: card.id, data: { card } });
  const { role: _role, "aria-pressed": _pressed, ...aria } = attributes as Record<string, any>;
  const on = selected === card.id;
  const handle = (label: string, className?: string) => (
    <button
      type="button"
      {...aria}
      {...listeners}
      aria-roledescription="draggable task"
      aria-current={on ? "true" : undefined}
      aria-label={label}
      onClick={() => select(card.id)}
      className={cn("absolute inset-0 cursor-default rounded-[inherit]", FOCUS, className)}
    />
  );
  return { setNodeRef, isDragging, on, handle };
}

// ---------------------------------------------------------------------------
// Parts shared by the column options
// ---------------------------------------------------------------------------

function cardLabel(card: SampleCard) {
  const state = card.stage === "needsYou" && card.waiting ? `${WAITING_LABEL[card.waiting]}: ${card.reason ?? ""}. ` : "";
  return `${card.title}. ${state}${card.repo}${card.source ? `, ${card.source}` : ""}, ${shortAge(card.age)} ago${card.pr ? `, pull request ${card.pr}` : ""}`;
}

/** repo · source · age, then the Neko id (on hover or focus) and the PR. */
function Meta({ card, chip, className }: { card: SampleCard; chip?: boolean; className?: string }) {
  return (
    <span className={cn("flex min-w-0 items-center gap-2 text-[12px] leading-4 text-muted-foreground tabular-nums", className)}>
      {chip ? <StateChip card={card} /> : null}
      <span className="min-w-0 flex-1 truncate">
        {card.repo}{card.source ? <> · {card.source}</> : null} · {shortAge(card.age)}
      </span>
      <span className="hidden shrink-0 group-hover/card:inline group-focus-within/card:inline">{card.key}</span>
      {card.pr ? (
        <span className="inline-flex shrink-0 items-center gap-0.5"><GitPullRequest size={12} aria-hidden />{card.pr}</span>
      ) : null}
    </span>
  );
}

/** Why it needs you: the state chip, only here, where it says something the column doesn't. */
function StateChip({ card, className }: { card: SampleCard; className?: string }) {
  if (card.stage !== "needsYou" || !card.waiting) return null;
  return <Chip tone={WAITING_TONE[card.waiting]} className={cn("h-[18px] text-[11.5px]", className)}>{WAITING_LABEL[card.waiting]}</Chip>;
}

/** The reason, one line or two; with `chip`, led by the state chip (otherwise the meta line carries it). */
function Reason({ card, lines = 1, chip = true }: { card: SampleCard; lines?: 1 | 2; chip?: boolean }) {
  if (card.stage !== "needsYou" || !card.waiting) return null;
  return (
    <span className={cn("pointer-events-none min-w-0 text-[12.5px] leading-[1.45] text-muted-foreground", lines === 1 ? "flex items-center gap-1.5" : "line-clamp-2")}>
      {chip ? <StateChip card={card} className={lines === 2 ? "me-1.5 align-[1px]" : undefined} /> : null}
      {lines === 1 ? <span className="min-w-0 truncate">{card.reason}</span> : card.reason}
    </span>
  );
}

const CARD = "group/card relative flex w-full flex-col gap-1.5 rounded-xl border bg-card px-3 py-2.5 text-start shadow-xs transition-[background-color,border-color,box-shadow] duration-150 hover:border-foreground/15";

/** A's card: title first, the reason (Needs you only), one meta line. 70-100px. */
function CompactCardFace({ card, children }: { card: SampleCard; children?: ReactNode }) {
  return (
    <>
      {children}
      <span className="pointer-events-none line-clamp-2 text-[13.5px] font-medium leading-[1.4] text-strong">{card.title}</span>
      <Reason card={card} chip={false} />
      <Meta card={card} chip className="pointer-events-none" />
    </>
  );
}

function CompactCard({ card }: { card: SampleCard }) {
  const { setNodeRef, isDragging, on, handle } = useCardHandle(card);
  return (
    <div
      ref={setNodeRef}
      data-slot="mock-card"
      data-selected={on ? "" : undefined}
      className={cn(CARD, on ? "border-primary/70 ring-1 ring-primary/40" : "border-border", isDragging && "border-dashed opacity-40 shadow-none")}
    >
      <CompactCardFace card={card}>{handle(cardLabel(card))}</CompactCardFace>
    </div>
  );
}

function OverlayCard({ card, wide }: { card: SampleCard; wide?: boolean }) {
  return (
    <div className={cn(CARD, "cursor-grabbing border-foreground/15 shadow-lg", wide && "gap-2")}>
      <CompactCardFace card={card} />
    </div>
  );
}

function Header({ stage, count, children, as: H = "h3" }: { stage: Stage; count: number; children?: ReactNode; as?: "h3" | "h4" }) {
  return (
    <div data-slot="mock-column-header" className="flex h-8 shrink-0 items-center gap-2 px-2">
      <span aria-hidden className={cn("size-2 shrink-0 rounded-full", TONE_DOT[STAGE_TONE[stage]])} />
      <H className="truncate text-[13px] font-semibold text-strong">{STAGE_TITLE[stage]}</H>
      <span className="text-[12px] tabular-nums text-muted-foreground">{count}</span>
      <span className="flex-1" />
      {children}
    </div>
  );
}

function Empty({ stage }: { stage: Stage }) {
  return <p className="px-2 py-2 text-[12.5px] leading-[1.5] text-muted-foreground">{STAGE_EMPTY[stage]}</p>;
}

function Column({ stage, className, header, children }: { stage: Stage; className?: string; header?: ReactNode; children?: ReactNode }) {
  const { byStage } = useMock();
  const { setNodeRef, isOver } = useDroppable({ id: stage });
  const cards = byStage[stage];
  return (
    <section
      ref={setNodeRef}
      data-slot="mock-column"
      data-stage={stage}
      data-over={isOver ? "" : undefined}
      aria-label={`${STAGE_TITLE[stage]}, ${cards.length}`}
      className={cn("flex min-h-0 min-w-0 flex-1 flex-col rounded-2xl bg-foreground/[0.03] p-1.5 transition-[background-color,box-shadow] duration-150", DROP, className)}
    >
      {header ?? <Header stage={stage} count={cards.length} />}
      <div className="flex min-h-0 flex-1 flex-col gap-1.5 overflow-y-auto px-0.5 pt-0.5 pb-2">
        {children ?? (cards.length ? cards.map((c) => <CompactCard key={c.id} card={c} />) : <Empty stage={stage} />)}
      </div>
    </section>
  );
}

/** A folded column: a narrow strip with the name and count. Click opens it; it still takes drops. */
function Strip({ stage, onOpen }: { stage: Stage; onOpen: () => void }) {
  const { byStage } = useMock();
  const { setNodeRef, isOver } = useDroppable({ id: stage });
  const count = byStage[stage].length;
  return (
    <button
      ref={setNodeRef}
      type="button"
      data-slot="mock-strip"
      data-stage={stage}
      data-over={isOver ? "" : undefined}
      aria-expanded={false}
      aria-label={`${STAGE_TITLE[stage]}, ${count}. Show column`}
      onClick={onOpen}
      className={cn(
        "flex w-11 shrink-0 flex-col items-center gap-2.5 rounded-2xl bg-foreground/[0.03] py-3 text-muted-foreground transition-[background-color,box-shadow] duration-150 hover:bg-foreground/[0.06] hover:text-foreground",
        DROP, FOCUS,
      )}
    >
      <span aria-hidden className={cn("size-2 shrink-0 rounded-full", TONE_DOT[STAGE_TONE[stage]])} />
      <span className="text-[13px] font-medium text-foreground/80 [writing-mode:vertical-rl]">{STAGE_TITLE[stage]}</span>
      <span className="text-[12px] tabular-nums">{count}</span>
      <CaretRight size={12} weight="bold" aria-hidden />
    </button>
  );
}

function FoldButton({ stage, onFold }: { stage: Stage; onFold: () => void }) {
  return (
    <Button variant="ghost" size="icon-xs" aria-label={`Fold ${STAGE_TITLE[stage]}`} aria-expanded title="Fold into a strip" onClick={onFold}>
      <CaretLeft weight="bold" />
    </Button>
  );
}

function Frame({ children, className }: { children: ReactNode; className?: string }) {
  return <div data-slot="board-option-frame" className={cn("flex h-[620px] w-full gap-2 overflow-hidden rounded-2xl border border-border bg-background p-2.5", className)}>{children}</div>;
}

// ---------------------------------------------------------------------------
// The options
// ---------------------------------------------------------------------------

/** A. Today's five columns, with cards a third the height. */
function OptionCompact() {
  const mock = useMockBoard();
  return (
    <MockDnd mock={mock} overlay={(c) => <OverlayCard card={c} />}>
      <Frame>{STAGES.map((s) => <Column key={s} stage={s} />)}</Frame>
    </MockDnd>
  );
}

/** B. Empty columns fold to strips; the busy ones share the width. */
function OptionFit() {
  const mock = useMockBoard();
  const [folded, setFolded] = useState<Partial<Record<Stage, boolean>>>({});
  const isFolded = (s: Stage) => folded[s] ?? mock.byStage[s].length === 0;
  const set = (s: Stage, v: boolean) => setFolded((f) => ({ ...f, [s]: v }));
  return (
    <MockDnd mock={mock} overlay={(c) => <OverlayCard card={c} />}>
      <Frame>
        {STAGES.map((s) => isFolded(s)
          ? <Strip key={s} stage={s} onOpen={() => set(s, false)} />
          : <Column key={s} stage={s} header={<Header stage={s} count={mock.byStage[s].length}><FoldButton stage={s} onFold={() => set(s, true)} /></Header>} />)}
      </Frame>
    </MockDnd>
  );
}

/** C's Needs-you card: the reason in full and the one action it is waiting for. */
function LaneCard({ card }: { card: SampleCard }) {
  const { move } = useMock();
  const { setNodeRef, isDragging, on, handle } = useCardHandle(card);
  return (
    <div
      ref={setNodeRef}
      data-slot="mock-card"
      data-selected={on ? "" : undefined}
      className={cn(CARD, "gap-2", on ? "border-primary/70 ring-1 ring-primary/40" : "border-border", isDragging && "border-dashed opacity-40 shadow-none")}
    >
      {handle(cardLabel(card))}
      <span className="pointer-events-none line-clamp-2 text-[13.5px] font-medium leading-[1.4] text-strong">{card.title}</span>
      <Reason card={card} lines={2} />
      <span className="flex items-center gap-2">
        <Meta card={card} className="pointer-events-none flex-1" />
        {card.waiting ? (
          <Button
            variant="outline"
            size="xs"
            data-slot="mock-card-action"
            className="relative z-10"
            onClick={() => move(card.id, afterAction(card.waiting!))}
          >
            {WAITING_ACTION[card.waiting]}
          </Button>
        ) : null}
      </span>
    </div>
  );
}

/** C. Needs you is the wide lane with actions; Running and Up next share one column; Done is a strip. */
function OptionLanes() {
  const mock = useMockBoard();
  const [doneOpen, setDoneOpen] = useState(false);
  const needs = mock.byStage.needsYou;
  return (
    <MockDnd mock={mock} overlay={(c) => <OverlayCard card={c} wide />}>
      <Frame>
        <Column stage="needsYou" className="flex-[1.7]">
          {needs.length ? needs.map((c) => <LaneCard key={c.id} card={c} />) : <Empty stage="needsYou" />}
        </Column>
        <Column stage="review" />
        <div data-slot="mock-flight" className="flex min-h-0 min-w-0 flex-1 flex-col gap-2">
          {(["running", "upNext"] as const).map((s) => (
            <Column key={s} stage={s} className={mock.byStage[s].length ? "flex-1" : "flex-none"} header={<Header stage={s} count={mock.byStage[s].length} as="h4" />} />
          ))}
        </div>
        {doneOpen
          ? <Column stage="done" header={<Header stage="done" count={mock.byStage.done.length}><FoldButton stage="done" onFold={() => setDoneOpen(false)} /></Header>} />
          : <Strip stage="done" onOpen={() => setDoneOpen(true)} />}
      </Frame>
    </MockDnd>
  );
}

// --- D: one list, grouped by stage ------------------------------------------------------------

const GLYPH: Record<Stage, string> = { needsYou: "AwaitingApproval", review: "ReadyForReview", running: "Building", upNext: "Queued", done: "Completed" };

function ListRow({ card }: { card: SampleCard }) {
  const { move } = useMock();
  const { setNodeRef, isDragging, on, handle } = useCardHandle(card);
  const needs = card.stage === "needsYou" && card.waiting;
  return (
    <div
      ref={setNodeRef}
      data-slot="mock-row"
      data-selected={on ? "" : undefined}
      className={cn(
        "group/card relative flex items-start gap-3 rounded-[14px] px-3 py-2 transition-colors duration-150",
        on ? "bg-foreground/[0.07]" : "hover:bg-foreground/[0.04]",
        isDragging && "opacity-40",
      )}
    >
      {handle(cardLabel(card))}
      <StatusGlyph status={card.waiting === "stopped" && card.stage === "needsYou" ? "Failed" : GLYPH[card.stage]} size={15} className="pointer-events-none mt-[3px]" />
      <span className="pointer-events-none flex min-w-0 flex-1 flex-col gap-0.5">
        <span className="flex min-w-0 items-baseline gap-4">
          <span className="min-w-0 flex-1 truncate text-[13.5px] font-medium leading-[1.45] text-strong">{card.title}</span>
          <Meta card={card} className="max-w-[45%] shrink-0" />
        </span>
        {needs ? <Reason card={card} /> : null}
      </span>
      {needs ? (
        <Button variant="outline" size="xs" data-slot="mock-card-action" className="relative z-10 mt-px" onClick={() => move(card.id, afterAction(card.waiting!))}>
          {WAITING_ACTION[card.waiting!]}
        </Button>
      ) : null}
    </div>
  );
}

function ListGroup({ stage, open, onToggle }: { stage: Stage; open: boolean; onToggle: () => void }) {
  const { byStage } = useMock();
  const { setNodeRef, isOver } = useDroppable({ id: stage });
  const cards = byStage[stage];
  return (
    <section ref={setNodeRef} data-slot="mock-group" data-stage={stage} data-over={isOver ? "" : undefined} aria-label={`${STAGE_TITLE[stage]}, ${cards.length}`} className={cn("flex flex-col rounded-2xl transition-[background-color,box-shadow] duration-150", DROP)}>
      <button
        type="button"
        aria-expanded={open}
        onClick={onToggle}
        className={cn("sticky top-0 z-20 flex h-9 items-center gap-2 rounded-xl bg-background px-3 text-start hover:bg-foreground/[0.03]", FOCUS)}
      >
        <CaretDown size={12} weight="bold" aria-hidden className={cn("text-muted-foreground transition-transform duration-200 motion-reduce:transition-none", !open && "-rotate-90")} />
        <span aria-hidden className={cn("size-2 shrink-0 rounded-full", TONE_DOT[STAGE_TONE[stage]])} />
        <span className="text-[13px] font-semibold text-strong">{STAGE_TITLE[stage]}</span>
        <span className="text-[12px] tabular-nums text-muted-foreground">{cards.length}</span>
      </button>
      {open ? (
        <div className="flex flex-col gap-0.5 pb-3">
          {cards.length ? cards.map((c) => <ListRow key={c.id} card={c} />) : <p className="px-10 py-1 text-[12.5px] text-muted-foreground">{STAGE_EMPTY[stage]}</p>}
        </div>
      ) : null}
    </section>
  );
}

/** D. No columns: one calm list grouped by stage, hushbacks' notification rows, Done folded. */
function OptionInbox() {
  const mock = useMockBoard();
  const [open, setOpen] = useState<Record<Stage, boolean>>({ needsYou: true, review: true, running: true, upNext: true, done: false });
  return (
    <MockDnd mock={mock} overlay={(c) => <OverlayCard card={c} />}>
      <Frame className="block overflow-y-auto px-3 py-2">
        <div className="mx-auto flex max-w-[960px] flex-col gap-1">
          {STAGES.map((s) => <ListGroup key={s} stage={s} open={open[s]} onToggle={() => setOpen((o) => ({ ...o, [s]: !o[s] }))} />)}
        </div>
      </Frame>
    </MockDnd>
  );
}

const OPTIONS: { id: string; name: string; caption: string; render: () => ReactNode }[] = [
  {
    id: "compact",
    name: "A. Compact cards",
    caption: "Same five columns, cards a third the height: title, the reason only in Needs you, one meta line. The status chip shows only where it says more than the column; the id shows on hover.",
    render: () => <OptionCompact />,
  },
  {
    id: "fit",
    name: "B. Columns that fit their work",
    caption: "A’s cards, and empty columns fold to a narrow strip with their name and count, so the busy ones get the room. Click a strip to open it; drop on it to move a task there.",
    render: () => <OptionFit />,
  },
  {
    id: "lanes",
    name: "C. Needs you first",
    caption: "Needs you is the wide lane, each card with its reason and the one action it waits for. Running and Up next share a column; Done folds to a count you open.",
    render: () => <OptionLanes />,
  },
  {
    id: "inbox",
    name: "D. One list, grouped",
    caption: "No columns: hushbacks’ notification rows grouped by stage, one line each (two when it needs you), the action at the end. Empty stages cost one line; Done stays folded.",
    render: () => <OptionInbox />,
  },
];

export function BoardOptions() {
  return (
    <div className="flex flex-col gap-10">
      {OPTIONS.map((o) => (
        <figure key={o.id} data-board-option={o.id} className="m-0 flex flex-col gap-3">
          <figcaption className="flex flex-col gap-1 px-1">
            <span className="text-sm font-semibold text-foreground">{o.name}</span>
            <span className="max-w-3xl text-[13px] leading-snug text-muted-foreground">{o.caption}</span>
          </figcaption>
          {o.render()}
        </figure>
      ))}
    </div>
  );
}
