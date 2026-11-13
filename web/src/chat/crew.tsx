// Agents are a crew (DESIGN.md §4). Home's chat says who is working and on
// what, from what the snapshot and the transcript already know; nothing here
// asks the daemon for more.
//
//   CrewHeader   "Neko · Codex gpt-5.5 · Working on NEK-1BB3 · 3 in crew", and the Hide crew toggle
//                identity: the runtime of the latest Home turn (snapshot.runtime_selections, conversation
//                "home:…"), else the model pinned for it, else nothing
//                state: "Replying" while a turn is pending, else the newest running task, else "Idle"
//                crew: running tasks (Planning, Building, Reviewing) + Neko while it replies
//   handoffsOf   one line per delegation a Neko turn carries:
//                metadata.tickets (tasks the reply opened)      Neko → <role>  NEK-…: title
//                neko_task_action parts that went ahead          Neko → <role>  NEK-…: title · start
//                metadata.kind "brief" (a routine's run)         Routine → Neko
//   HandoffRow   the line itself; task conversations use it when the speaker changes (Scout → Builder)
//
// The role is the agent a task's status names (Planning → Scout, Building →
// Builder, Reviewing / ReadyForReview → Reviewer), else the last agent in its
// events, else Builder (a queued task is built next).
import type { Json } from "@/api";
import { useWorkbench } from "@/useStore";
import { useLocal } from "@/work/useLocal";
import { modelLabel } from "@/work/model";
import { shortId } from "@/work/board/moves";
import { taskHref, taskTitle, useShellStatus } from "@/shell/status";
import { IdLabel, StatusGlyph, statusLabel, statusOf } from "@/ui/status-glyph";
import { Users } from "@/ui/icons";
import { cn } from "@/ui/lib/utils";
import { ACTION_TOOL, isToolPart, type NekoUIMessage } from "@/chat/sdk/types";
import { UnseenDot, useUnseen } from "@/work/seen";
import { hrefFor } from "@/router";
import { goalsOf, isLive } from "@/work/goals";

const HIDE_KEY = "neko.chat.hideCrew";

/** Whether the crew lines are hidden (kept in localStorage, shared by the header and every turn). */
export function useHideCrew(): [boolean, (hide: boolean) => void] {
  const [raw, set] = useLocal(HIDE_KEY, "false");
  return [raw === "true", (hide) => set(hide ? "true" : "false")];
}

const ROLE_BY_EVENT: Record<string, string> = { scout: "Scout", builder: "Builder", reviewer: "Reviewer", coordinator: "Recovery" };

export function agentRole(task: Json): string {
  switch (task?.status) {
    case "Planning": return "Scout";
    case "Building": return "Builder";
    case "Reviewing":
    case "ReadyForReview": return "Reviewer";
  }
  const events: Json[] = Array.isArray(task?.events) ? task.events : [];
  for (let i = events.length - 1; i >= 0; i--) {
    const role = ROLE_BY_EVENT[String(events[i]?.role ?? "")];
    if (role) return role;
  }
  return "Builder";
}

/** The model Neko's Home chat ran on last, in words, or null when nothing says. */
export function homeRuntime(snapshot: Json): string | null {
  const selections: Json[] = Array.isArray(snapshot?.runtime_selections) ? snapshot.runtime_selections : [];
  const last = [...selections].reverse().find((s) => String(s?.conversation_id ?? "").startsWith("home:"));
  if (last?.runtime?.provider || last?.runtime?.model) return modelLabel(last.runtime);
  const pinned = Object.entries((snapshot?.conversation_runtime ?? {}) as Record<string, Json>).find(([id, prefs]) => id.startsWith("home:") && prefs?.model);
  return pinned ? modelLabel(pinned[1]) : null;
}

function TaskRef({ task, className }: { task: Json; className?: string }) {
  return (
    <a
      href={taskHref(String(task.id))}
      data-reference="task"
      title={`${shortId(task)}: ${taskTitle(task)}`}
      className={cn("inline-flex min-w-0 items-center gap-1 rounded-sm font-medium text-primary outline-none hover:underline focus-visible:ring-2 focus-visible:ring-ring", className)}
    >
      <StatusGlyph status={statusOf(task.status)} label={statusLabel(task.status)} size={12} />
      <span className="shrink-0 tabular-nums">{shortId(task)}</span>
    </a>
  );
}

export function CrewHeader({ replying }: { replying: boolean }) {
  const { snapshot } = useWorkbench();
  const { tasks } = useShellStatus();
  const [hidden, setHidden] = useHideCrew();
  const unseen = useUnseen();
  if (!snapshot) return null;
  const finished: Json[] = (snapshot.tasks ?? []).filter((t: Json) => unseen.has(String(t.id)));
  const running = tasks.running;
  const crew = running.length + (replying ? 1 : 0);
  // Goals that are working or parked for a reason, not the paused or finished ones.
  const activeGoals = goalsOf(snapshot).filter((g: Json) => isLive(g) && g.status !== "Paused").length;
  const runtime = homeRuntime(snapshot);
  const lead = running[0];
  return (
    <div data-slot="crew-header" className="flex min-h-7 items-center gap-1.5 text-[12.5px] text-muted-foreground">
      <span className="font-medium text-foreground">Neko</span>
      {runtime ? <><span aria-hidden>·</span><span className="truncate">{runtime}</span></> : null}
      <span aria-hidden>·</span>
      <span data-slot="crew-state" className="flex min-w-0 items-center gap-1">
        {replying ? "Replying" : lead && !hidden ? <>Working on <TaskRef task={lead} />{running.length > 1 ? <span>and {running.length - 1} more</span> : null}</> : lead ? "Working" : "Idle"}
      </span>
      {!hidden ? <><span aria-hidden>·</span><span data-slot="crew-count" className="tabular-nums">{crew} in crew</span></> : null}
      {activeGoals ? <><span aria-hidden>·</span><span data-slot="crew-goals" className="tabular-nums">{activeGoals} {activeGoals === 1 ? "goal" : "goals"}</span></> : null}
      {finished.length ? (
        <>
          <span aria-hidden>·</span>
          <a
            href={finished.length === 1 ? taskHref(String(finished[0].id)) : hrefFor("tasks")}
            data-slot="crew-unseen"
            title={finished.map((t) => `${shortId(t)}: ${taskTitle(t)} (${statusLabel(t.status)})`).join("\n")}
            className="inline-flex items-center gap-1.5 rounded-sm tabular-nums outline-none hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring"
          >
            <UnseenDot />
            {finished.length} finished, not seen
          </a>
        </>
      ) : null}
      <button
        type="button"
        aria-pressed={hidden}
        onClick={() => setHidden(!hidden)}
        title={hidden ? "Show who is working and the handoffs" : "Hide the crew lines"}
        className="ms-auto inline-flex items-center gap-1 rounded-md px-1.5 py-0.5 text-[12px] text-muted-foreground outline-none hover:bg-foreground/[0.05] hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring"
      >
        <Users size={13} aria-hidden /> {hidden ? "Show crew" : "Hide crew"}
      </button>
    </div>
  );
}

export interface Handoff {
  key: string;
  from: string;
  to: string;
  task: Json | null;
  note?: string;
}

const HANDOFF_KINDS = new Set(["start", "approve", "retry", "message", "run_next"]);

/** The delegations one Neko turn carries (see the top of this file). */
export function handoffsOf(message: NekoUIMessage, tasks: Json[]): Handoff[] {
  const byId = new Map(tasks.map((t) => [String(t.id), t]));
  const out: Handoff[] = [];
  const meta = message.metadata ?? {};
  if (meta.kind === "brief") out.push({ key: "brief", from: "Routine", to: "Neko", task: null, note: "a routine's run" });
  for (const id of meta.tickets ?? []) {
    const task = byId.get(id);
    if (task) out.push({ key: `t:${id}`, from: "Neko", to: agentRole(task), task });
  }
  for (const part of message.parts) {
    if (!isToolPart(part) || part.toolName !== ACTION_TOOL) continue;
    const input = (part.input ?? {}) as { taskId?: string; kind?: string };
    const went = part.state === "output-available" || (part.state === "approval-responded" && part.approval?.approved !== false);
    const task = input.taskId ? byId.get(input.taskId) : undefined;
    if (!went || !task || !HANDOFF_KINDS.has(input.kind ?? "")) continue;
    out.push({ key: `a:${part.toolCallId}`, from: "Neko", to: agentRole(task), task, note: input.kind === "run_next" ? "run next" : input.kind });
  }
  return out;
}

/** `Neko → Builder  NEK-1BB3: Fix the flaky login test`. */
export function HandoffRow({ handoff, className }: { handoff: Handoff; className?: string }) {
  const { task } = handoff;
  return (
    <p data-slot="handoff" className={cn("flex min-w-0 items-center gap-2 px-1 py-0.5 text-[12.5px] text-muted-foreground", className)}>
      <span className="shrink-0">
        <span className="text-foreground/80">{handoff.from}</span> <span aria-label="hands off to">→</span> <span className="text-foreground/80">{handoff.to}</span>
      </span>
      {task ? (
        <span className="flex min-w-0 items-center gap-1.5">
          <TaskRef task={task} />
          <span className="truncate">: {taskTitle(task)}</span>
        </span>
      ) : null}
      {handoff.note ? <IdLabel>{handoff.note}</IdLabel> : null}
    </p>
  );
}

export function TurnHandoffs({ message }: { message: NekoUIMessage }) {
  const { snapshot } = useWorkbench();
  const [hidden] = useHideCrew();
  if (hidden) return null;
  const list = handoffsOf(message, snapshot?.tasks ?? []);
  if (!list.length) return null;
  return (
    <div data-slot="handoffs" className="mt-1 flex flex-col">
      {list.map((h) => <HandoffRow key={h.key} handoff={h} />)}
    </div>
  );
}
