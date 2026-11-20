// Goals that keep working (docs/knowledge/goals-that-keep-working.md), as the
// web shows them: one line per goal (DESIGN.md §8), on its task, in Home's
// chat and in the Inbox. The goals come in the snapshot (`snapshot.goals`);
// every action is a `Goals` workbench command and each is also in the line's
// right-click menu.
//
//   ◐ Make the export handle empty rows        turn 4/20 · check failing (exit 101)
//   expand: the evidence ledger (command, exit code, the end of its output)
//           and the journal (what each turn did and what is next)
import { useState } from "react";
import { command, type Json } from "@/api";
import { useWorkbench } from "@/useStore";
import { useContextMenu } from "@/shell/context-menu";
import { RowAction, RowActions } from "@/setup/kit";
import { relativeTime, useNow } from "@/setup/lib";
import { StatusGlyph, type StatusKind } from "@/ui/status-glyph";
import { Button } from "@/ui/button";
import { Input } from "@/ui/input";
import { ArrowCounterClockwise, Check, Pause, Play, Plus, Stop } from "@/ui/icons";
import { toast } from "@/ui/sonner";
import { cn } from "@/ui/lib/utils";
import { artifactHref } from "@/canvas/artifacts";
import { ConfirmDialog, type ConfirmSpec } from "@/work/actions";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/ui/select";

export type Goal = Json;

const FINISHED = new Set(["Done", "Stopped"]);
export const isLive = (goal: Goal) => !FINISHED.has(String(goal?.status));

/** A goal's status as the one glyph set every status uses. */
export function goalKind(status: string): StatusKind {
  switch (status) {
    case "Running": return "inProgress";
    case "Blocked": return "review";
    case "Done": return "done";
    case "Stopped": return "cancelled";
    case "Waiting":
    case "Held": return "backlog";
    default: return "todo";
  }
}

const WORDS: Record<string, string> = {
  Running: "Working", Waiting: "Waiting", Blocked: "Needs you", Held: "Held", Paused: "Paused", Done: "Done", Stopped: "Stopped",
};
export const goalWord = (status: string) => WORDS[status] ?? status;

/** "check passed", "check failing (exit 101)", or nothing when the goal has no check yet. */
export function checkText(goal: Goal): string {
  const check = goal.last_check;
  if (!check) return goal.done_check ? "check not run yet" : "no check";
  return check.exit_code === 0 ? "check passed" : `check failing (${check.exit_code == null ? "no exit" : `exit ${check.exit_code}`})`;
}

/** Where the goal stands, in the words a person would use. */
export function stateText(goal: Goal, now: number): string {
  switch (goal.status) {
    case "Waiting": return goal.wait?.until_ms ? `waiting until ${new Date(goal.wait.until_ms).toLocaleTimeString([], { hour: "numeric", minute: "2-digit" })}` : `waiting: ${goal.wait?.reason || "for something outside"}`;
    case "Held": return goal.hold_until_ms ? `usage limit, resumes ${relativeTime(goal.hold_until_ms, now)}` : "usage limit";
    case "Blocked": return "needs your answer";
    case "Paused": return goal.paused_reason || "paused";
    case "Stopped": return goal.paused_reason || "stopped";
    default: return checkText(goal);
  }
}

export const turnText = (goal: Goal) => `turn ${goal.turns}/${goal.budget?.max_turns ?? 20}`;
export const extendBy = (goal: Goal) => Math.max(5, Math.floor((goal.budget?.max_turns ?? 20) / 2));

export function goalsOf(snapshot: Json | null | undefined): Goal[] {
  return Array.isArray(snapshot?.goals) ? snapshot.goals : [];
}

export const goalsOfTask = (snapshot: Json | null | undefined, taskId: string) => goalsOf(snapshot).filter((g) => g.ticket_id === taskId);

/** Goals set in the chat Home shows (their conversation id starts with `home:`), live ones first. */
export function homeGoals(snapshot: Json | null | undefined): Goal[] {
  return goalsOf(snapshot).filter((g) => String(g.conversation_id ?? "").startsWith("home:") && isLive(g));
}

async function send(done: string | null, body: Record<string, unknown>) {
  try {
    await command("Goals", body);
    if (done) toast.success(done);
  } catch (error) {
    toast.error(error instanceof Error ? error.message : String(error));
  }
}

/** The waits a blocked goal can take its default after: the stored number of hours (0 is off). */
const DEADLINE_HOURS = [0, 1, 4, 8, 24, 72];
const deadlineLabel = (hours: number) => (hours === 0 ? "Off" : hours === 72 ? "3 days" : hours === 1 ? "1 hour" : `${hours} hours`);

/** The deadline default is set by hand and only for a question the judge gave its own default to. */
function DeadlineLine({ goal }: { goal: Goal }) {
  const id = String(goal.id);
  const hours: number = goal.default_after_hours ?? 0;
  const blocked = goal.status === "Blocked" ? goal.blocked : null;
  if (blocked && !blocked.real_default) {
    return <p data-slot="goal-deadline" className="text-muted-foreground">Neko has no default for this question, so it will wait for you.</p>;
  }
  const options = DEADLINE_HOURS.includes(hours) ? DEADLINE_HOURS : [...DEADLINE_HOURS, hours].sort((a, b) => a - b);
  return (
    <div data-slot="goal-deadline" className="flex min-w-0 items-center gap-2 text-muted-foreground">
      <span>If you do not answer, Neko takes the default after</span>
      <Select value={String(hours)} onValueChange={(v) => void send(null, { SetDefaultAfter: { id, hours: Number(v) === 0 ? null : Number(v) } })} items={options.map((h) => ({ value: String(h), label: deadlineLabel(h) }))}>
        <SelectTrigger size="sm" aria-label="Take the default after" className="h-7 min-w-24 text-[12.5px]"><SelectValue /></SelectTrigger>
        <SelectContent>{options.map((h) => <SelectItem key={h} value={String(h)}>{deadlineLabel(h)}</SelectItem>)}</SelectContent>
      </Select>
    </div>
  );
}

/** Whether a deadline answer can still be taken back. */
export const undoableDefault = (goal: Goal, now: number) => !!goal.default_taken && Number(goal.default_taken.expires_ms) > now && isLive(goal);

export interface GoalAction { key: string; label: string; icon: typeof Pause; run: () => void; destructive?: boolean }

/** The actions a goal offers in its state: Pause, Resume, Extend, Mark done, Stop. */
export function goalActions(goal: Goal, onUndoDefault?: () => void): GoalAction[] {
  const id = String(goal.id);
  const out: GoalAction[] = [];
  if (goal.status === "Paused") out.push({ key: "resume", label: "Resume", icon: Play, run: () => void send("Resumed.", { Resume: { id } }) });
  else if (isLive(goal)) out.push({ key: "pause", label: "Pause", icon: Pause, run: () => void send("Paused.", { Pause: { id } }) });
  if (isLive(goal)) {
    const more = extendBy(goal);
    out.push({ key: "extend", label: `Extend by ${more} turns`, icon: Plus, run: () => void send(`Extended by ${more} turns.`, { Extend: { id, turns: more } }) });
    out.push({ key: "done", label: "Mark done", icon: Check, run: () => void send("Marked done.", { MarkDone: { id } }) });
    out.push({ key: "stop", label: "Stop", icon: Stop, destructive: true, run: () => void send("Stopped.", { Stop: { id } }) });
  }
  if (onUndoDefault && undoableDefault(goal, Date.now())) out.push({ key: "undo-default", label: "Undo the default answer", icon: ArrowCounterClockwise, destructive: true, run: onUndoDefault });
  return out;
}

function Ledger({ goal, now, refusal, onUndoDefault }: { goal: Goal; now: number; refusal: string | null; onUndoDefault: () => void }) {
  const ledger: Json[] = goal.ledger ?? [];
  const journal: Json[] = goal.journal ?? [];
  return (
    <div data-slot="goal-detail" className="ms-8 mt-1 flex min-w-0 flex-col gap-3 pb-2 text-[12.5px] text-muted-foreground">
      {goal.done_check ? <p className="truncate"><span className="text-foreground/80">Check</span> <code className="font-mono text-[12px]">{goal.done_check}</code></p> : <p>No check. Done needs a file or a link that exists.</p>}
      {goal.constraints ? <p><span className="text-foreground/80">Constraints</span> {goal.constraints}</p> : null}
      {undoableDefault(goal, now) ? (
        <div data-slot="goal-default-taken" className="flex min-w-0 flex-col gap-1">
          <div className="flex min-w-0 items-center gap-2">
            <span className="min-w-0 flex-1">Neko took the default after {goal.default_taken.after_hours} h: <span className="text-foreground/80">{goal.default_taken.answer}</span></span>
            <Button size="xs" variant="outline" onClick={onUndoDefault}>Undo</Button>
          </div>
          {refusal ? <p role="alert" className="text-warning-foreground">{refusal}</p> : null}
        </div>
      ) : null}
      {isLive(goal) && goal.status !== "Blocked" ? <DeadlineLine goal={goal} /> : null}
      {goal.status === "Done" && goal.receipt ? (
        <p data-slot="goal-receipt"><span className="text-foreground/80">Receipt</span> <a href={artifactHref(String(goal.receipt.artifact_id), goal.receipt.version)} className="text-foreground underline underline-offset-2">Open the receipt</a></p>
      ) : goal.status === "Done" && (goal.receipt_tries ?? 0) >= 3 ? (
        <p data-slot="goal-receipt-missing">No receipt could be written.</p>
      ) : null}
      {ledger.length ? (
        <div>
          <p className="mb-1 font-medium text-foreground/80">Evidence</p>
          <ul className="flex flex-col gap-1.5">
            {[...ledger].reverse().map((entry, i) => (
              <li key={i} className="min-w-0">
                <span className="flex min-w-0 items-baseline gap-2">
                  <span className={cn("shrink-0 tabular-nums", entry.exit_code === 0 ? "text-foreground" : "text-warning-foreground")}><span className="sr-only">{entry.exit_code === 0 ? "Passed: " : "Failed: "}</span>{entry.exit_code == null ? "no exit" : `exit ${entry.exit_code}`}</span>
                  <code className="min-w-0 truncate font-mono text-[12px]" title={entry.command}>{entry.command}</code>
                  <span className="ms-auto shrink-0 tabular-nums text-muted-foreground/70">{entry.source} · {relativeTime(entry.at_ms, now)}</span>
                </span>
                {entry.tail ? <pre className="mt-0.5 max-h-24 overflow-auto whitespace-pre-wrap font-mono text-[11.5px] text-muted-foreground/90">{entry.tail}</pre> : null}
              </li>
            ))}
          </ul>
        </div>
      ) : null}
      {journal.length ? (
        <div>
          <p className="mb-1 font-medium text-foreground/80">Journal</p>
          <ul className="flex flex-col gap-1">
            {[...journal].reverse().map((entry, i) => (
              <li key={i} className="flex min-w-0 gap-2">
                <span className="shrink-0 tabular-nums text-muted-foreground/70">{entry.turn}</span>
                <span className="min-w-0">
                  {entry.done ? <span>{entry.done}</span> : null}
                  {entry.next ? <span className="text-muted-foreground/80"> Next: {entry.next}</span> : null}
                  {entry.note ? <span className="text-muted-foreground/70"> {entry.done || entry.next ? "· " : ""}{entry.note}</span> : null}
                  {entry.plan ? <span className="text-muted-foreground/80"> Plan: {entry.plan}</span> : null}
                </span>
              </li>
            ))}
          </ul>
        </div>
      ) : null}
      {goal.reason ? <p><span className="text-foreground/80">Judge</span> {goal.verdict ? `${goal.verdict}: ` : ""}{goal.reason}</p> : null}
    </div>
  );
}

/** A blocked goal's one question, with its default answer. */
export function GoalAnswer({ goal, showQuestion = true }: { goal: Goal; showQuestion?: boolean }) {
  const [text, setText] = useState("");
  const blocked = goal.blocked;
  if (goal.status !== "Blocked" || !blocked) return null;
  const id = String(goal.id);
  return (
    <div data-slot="goal-answer" className="ms-8 mt-1 flex min-w-0 flex-col gap-2 pb-2 text-[12.5px]">
      {showQuestion ? <p className="text-foreground">{blocked.question}</p> : null}
      <DeadlineLine goal={goal} />
      <div className="flex min-w-0 items-center gap-2">
        <Input value={text} onChange={(e) => setText(e.target.value)} onKeyDown={(e) => { if (e.key === "Enter" && text.trim()) { void send("Sent.", { Answer: { id, text } }); setText(""); } }} placeholder={blocked.default_answer} aria-label="Your answer" className="h-8 min-w-0 flex-1 text-[13px]" />
        <Button size="xs" variant="outline" onClick={() => { void send("Sent.", { Answer: { id, text } }); setText(""); }} disabled={!text.trim()}>Answer</Button>
        <Button size="xs" onClick={() => void send("Using the default.", { Answer: { id, text: "" } })}>Use the default</Button>
      </div>
    </div>
  );
}

/** One goal on one line (DESIGN.md §8): glyph, objective, turn, the last check; opens to its ledger and journal. */
export function GoalLine({ goal, className }: { goal: Goal; className?: string }) {
  const [open, setOpen] = useState(false);
  const [confirm, setConfirm] = useState<ConfirmSpec | null>(null);
  const [refusal, setRefusal] = useState<string | null>(null);
  const now = useNow(30_000);
  const undoDefault = () => setConfirm({
    title: "Undo the default answer?",
    description: "Neko will put the folder back to how it was before it acted on the default, and ask you the question again. Work done since will be undone.",
    confirm: "Undo the default",
    cancel: "Cancel",
    destructive: true,
    onConfirm: async () => {
      setRefusal(null);
      try {
        await command("Goals", { UndoDefault: { goal: String(goal.id) } });
        toast.success("Undid the default answer.");
      } catch (error) {
        const text = error instanceof Error ? error.message : String(error);
        // A refusal from the checkpoint code reads as its message, under the row; anything else is a toast.
        const refused = text.match(/^checkpoint:[a-z_]+: ([\s\S]*)$/);
        if (refused) { setRefusal(refused[1]); setOpen(true); } else toast.error(text);
      }
    },
  });
  const actions = goalActions(goal, undoDefault);
  const menu = useContextMenu(() => actions.map((a) => ({ label: a.label, icon: a.icon, destructive: a.destructive, run: a.run })));
  const kind = goalKind(String(goal.status));
  return (
    <div data-slot="goal" data-status={goal.status} className={cn("flex min-w-0 flex-col", className)}>
      <div {...menu} className="group/row flex min-h-8 items-center gap-3 rounded-[10px] px-3 py-1 transition-colors hover:bg-foreground/[0.03]">
        <StatusGlyph status={kind} label={`Goal: ${goalWord(String(goal.status))}`} size={14} />
        <button
          type="button"
          aria-expanded={open}
          onClick={() => setOpen(!open)}
          className="flex min-w-0 flex-1 items-baseline gap-3 rounded-md text-start outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
        >
          <span className="min-w-0 truncate text-[13.5px] text-foreground" title={goal.objective}>{goal.objective}</span>
          <span className="ms-auto shrink-0 truncate text-[12.5px] tabular-nums text-muted-foreground group-hover/row:hidden group-focus-within/row:hidden">
            {turnText(goal)} · {stateText(goal, now)}
          </span>
        </button>
        <RowActions className="hidden group-hover/row:flex group-focus-within/row:flex [@media(hover:none)]:flex">
          {actions.map((a) => <RowAction key={a.key} label={a.label} icon={a.icon} onClick={a.run} />)}
        </RowActions>
      </div>
      <GoalAnswer goal={goal} />
      {open ? <Ledger goal={goal} now={now} refusal={refusal} onUndoDefault={undoDefault} /> : null}
      <ConfirmDialog spec={confirm} onClose={() => setConfirm(null)} />
    </div>
  );
}

/** The goals of one task, above its details. Nothing when it has none. */
export function TaskGoal({ taskId }: { taskId: string }) {
  const { snapshot } = useWorkbench();
  const goals = goalsOfTask(snapshot, taskId).filter((g) => isLive(g) || g === goalsOfTask(snapshot, taskId).at(-1));
  if (!goals.length) return null;
  return <div data-slot="task-goal" className="flex flex-col">{goals.map((g) => <GoalLine key={g.id} goal={g} />)}</div>;
}

/** The goals set in Home's chat, between the crew line and the conversation. */
export function HomeGoals() {
  const { snapshot } = useWorkbench();
  const goals = homeGoals(snapshot);
  if (!goals.length) return null;
  return <div data-slot="home-goals" className="mx-auto flex w-full max-w-(--chat-content-max-width) shrink-0 flex-col px-3">{goals.map((g) => <GoalLine key={g.id} goal={g} />)}</div>;
}

/** What an Inbox item that came from a goal offers: its answer or one-click extend. */
export function GoalItemActions({ goalId }: { goalId: string }) {
  const { snapshot } = useWorkbench();
  const goal = goalsOf(snapshot).find((g) => g.id === goalId);
  if (!goal) return null;
  const actions = goalActions(goal).filter((a) => a.key === "extend" || a.key === "stop" || a.key === "resume");
  return (
    <div data-slot="goal-item-actions" className="mt-1 flex min-w-0 flex-col gap-1.5">
      <GoalAnswer goal={goal} showQuestion={false} />
      {goal.status === "Paused" ? (
        <div className="flex items-center gap-2 text-[12.5px]">
          {actions.map((a) => <Button key={a.key} size="xs" variant={a.key === "extend" ? "default" : "outline"} onClick={a.run}>{a.label}</Button>)}
        </div>
      ) : null}
    </div>
  );
}
