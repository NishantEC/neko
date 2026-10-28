// The gallery's "Task card" specimens: the real NotchCard (work/board/NotchCard.tsx) in each
// state it takes on the board, and the real board column header (work/board/TaskBoard.tsx).
// The design options these replaced (sidebar, card, columns) were decided and built; this keeps
// the pieces documented. Static data, nothing reaches the daemon.
import { useEffect, useState } from "react";
import { ArrowBendUpLeft, ArrowCounterClockwise, Check, DotsThree, Plus } from "@/ui/icons";
import { Button } from "@/ui/button";
import { clock, NotchCard, type NotchCardProps } from "@/work/board/NotchCard";
import { ColumnHeader } from "@/work/board/TaskBoard";

type Kind = "stopped" | "asks" | "approve" | "review" | "running" | "done";

const STATES: { kind: Kind; props: NotchCardProps }[] = [
  { kind: "stopped", props: { tone: "failed", title: "Fix TypeError in meal logging when portion is null", density: "compact", state: { label: "Stopped", age: "18m" }, reason: "Actual changed files exceed verified or approved scope.", action: { label: "Start again", icon: ArrowCounterClockwise, onClick: () => {} } } },
  { kind: "asks", props: { tone: "attention", title: "Coach chat drops messages after reconnect", density: "compact", state: { label: "Asks you", age: "1h" }, reason: "Should reconnects replay unread messages or only the last one?", action: { label: "Answer", icon: ArrowBendUpLeft, onClick: () => {} } } },
  { kind: "approve", props: { tone: "attention", title: "Bump expo-notifications to 0.29 and fix the iOS badge count", density: "compact", state: { label: "Needs approval", age: "2h" }, reason: "The plan touches 14 files across two packages.", action: { label: "Approve", icon: Check, onClick: () => {} } } },
  { kind: "review", props: { tone: "review", title: "Cache food search results for 10 minutes", density: "compact", state: { label: "Ready to review", age: "35m" }, pr: 4641 } },
  { kind: "running", props: { tone: "running", title: "Add retry to the Stripe webhook handler", density: "compact", state: null } },
  { kind: "done", props: { tone: "done", title: "Update athena #4641 branch onto master", density: "compact", state: null, meta: "athena · 2h", pr: 4641 } },
];

export function TaskSpecimens() {
  const [seconds, setSeconds] = useState(128);
  const [selected, setSelected] = useState<Kind | null>(null);
  useEffect(() => {
    const id = setInterval(() => setSeconds((n) => n + 1), 1000);
    return () => clearInterval(id);
  }, []);
  return (
    <div className="flex flex-col gap-8">
      <div data-slot="card-states" className="grid grid-cols-1 gap-x-5 gap-y-6 sm:grid-cols-2 lg:grid-cols-3">
        {STATES.map(({ kind, props }) => (
          <div key={kind} data-card-state={kind} className="flex min-w-0 flex-col gap-2">
            <NotchCard
              {...props}
              data-slot="card"
              data-kind={kind}
              live={kind === "running" ? { step: "Running tests", clock: clock(seconds) } : null}
              selected={selected === kind}
              handle={<button type="button" aria-label={`${props.title}, ${kind}`} aria-current={selected === kind ? "true" : undefined} onClick={() => setSelected((s) => (s === kind ? null : kind))} className="absolute inset-0 rounded-[16px] outline-none" />}
            />
          </div>
        ))}
      </div>
      <div data-slot="column-header-specimen" className="flex max-w-xs flex-col gap-1">
        <ColumnHeader stage="upNext" count={4}>
          <Button variant="ghost" size="icon-xs" aria-label="New task"><Plus /></Button>
          <Button variant="ghost" size="icon-xs" aria-label="Up next options"><DotsThree weight="bold" /></Button>
        </ColumnHeader>
        <p className="px-0.5 text-[13px] leading-snug text-muted-foreground">The column header: dot, name, count; ⋯ shows on hover on the board. No panel behind a column.</p>
      </div>
    </div>
  );
}
