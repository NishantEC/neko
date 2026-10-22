// What a reply did, drawn with T3 Code's work-log rows (chat/t3/WorkLog.tsx and
// TimelineWorkRows.tsx, from t3code @3b95e7831, MIT) over the message's parts:
// reasoning, tool calls (`dynamic-tool`, with their SDK states), and the prose
// said between steps.
//
// While the turn runs: "Working for 12s" over the newest step with its shine.
// Once it is finished: one "Worked for 1m 12s ›" fold, the steps inside it.
import { useEffect, useMemo, useState, type ReactNode } from "react";
import { CircleAlertIcon } from "lucide-react";
import { cn } from "@/ui/lib/utils";
import { splitStep } from "@/chat/work";
import { WorkLogButton, WorkLogDetails, WorkLogList, WorkLogRow } from "@/chat/t3/WorkLog";
import {
  LiveActivityRow, SimpleWorkEntryRow, ToolActivityIconView, TurnFoldTimelineRow, WorkingTimelineRow, formatDuration,
  type WorkEntryIconName,
} from "@/chat/t3/TimelineWorkRows";
import { ACTION_TOOL, isToolPart, type NekoPart, type NekoUIMessage } from "@/chat/sdk/types";
import {
  connectionOf, toolDeclined, toolDetail, toolFailed, toolKind, toolRunning, toolText, toolWaiting, type Kind,
} from "@/chat/sdk/parts";

/** Seconds since `since`, re-read every second while `active`. */
export function useElapsed(since: number, active: boolean) {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (!active) return;
    setNow(Date.now());
    const timer = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(timer);
  }, [active]);
  return Math.max(0, (now - since) / 1000);
}

export const KIND_ICON: Record<Kind, WorkEntryIconName> = {
  command: "terminal",
  edit: "square-pen",
  read: "eye",
  search: "search",
  tool: "hammer",
  other: "zap",
};

/** "Ran `git status`": the verb, then the target (in mono when it was code). */
export function StepLabel({ text }: { text: string }) {
  const { verb, target, code } = splitStep(text);
  return (
    <>
      {verb}
      {target ? <> <span className={cn(code && "font-mono text-[0.92em]")}>{target}</span></> : null}
    </>
  );
}

/** A tool that only reports, said in a line. */
export function ToolLine({ children, running, className }: { children: ReactNode; running?: boolean; className?: string }) {
  return (
    <div className={className}>
      <LiveActivityRow label={children} iconName="check" active={running} />
    </div>
  );
}

/** A tool with more to say than a line, said in a line until asked. */
export function ToolFold({
  label, aside, running, declined, defaultOpen = false, children,
}: { label: string; aside?: string; running?: boolean; declined?: boolean; defaultOpen?: boolean; children: ReactNode }) {
  const [open, setOpen] = useState(defaultOpen);
  return (
    <div data-slot="tool-fold">
      <WorkLogButton
        aria-expanded={open}
        onClick={() => setOpen((v) => !v)}
        icon={<ToolActivityIconView name={declined ? "x" : "terminal"} className="size-4 shrink-0 stroke-2 text-icon-muted" muted />}
        label={
          <span className="flex min-w-0 items-baseline gap-1.5">
            <span className={cn("truncate", running && "live-tool-shine")}>{label}</span>
            {aside ? <span className="min-w-0 truncate text-muted-foreground/70">{aside}</span> : null}
          </span>
        }
        trailing={<span aria-hidden className={cn("text-icon-muted opacity-70 transition-transform duration-200", open && "rotate-90")}>›</span>}
      />
      {open ? <WorkLogList><div className="ms-7 py-0.5">{children}</div></WorkLogList> : null}
    </div>
  );
}

/** A tool that did not do what it was asked, said once. */
export function ToolFailed({ what, detail }: { what: string; detail?: string }) {
  return (
    <WorkLogRow
      wrapLabel
      icon={<CircleAlertIcon className="size-4 text-destructive" />}
      label={<span className="text-sm font-medium text-destructive">{what}</span>}
    >
      {detail ? <p className="ms-7 whitespace-pre-wrap break-words py-1 text-sm leading-relaxed text-foreground/80">{detail}</p> : null}
    </WorkLogRow>
  );
}

// ---------------------------------------------------------------------------
// The entries of a turn's work
// ---------------------------------------------------------------------------

interface Entry {
  key: string;
  text: string;
  aside?: string;
  kind: Kind;
  icon: WorkEntryIconName;
  failed: boolean;
  waiting: boolean;
  running: boolean;
  detail: string | null;
}

const STATE_WORD: Record<string, string> = {
  "approval-requested": "waiting for approval",
  "output-denied": "declined",
  "input-streaming": "starting",
};

/** The steps of a turn, in the order they happened. */
export function entriesOf(work: NekoPart[]): Entry[] {
  const out: Entry[] = [];
  work.forEach((part, index) => {
    if (isToolPart(part)) {
      if (part.toolName === ACTION_TOOL) return; // a proposal: it has its own card
      const kind = toolKind(part);
      const connection = connectionOf(part);
      const declined = toolDeclined(part);
      const aside = [connection, STATE_WORD[part.state] ?? (declined ? "declined" : null)].filter(Boolean).join(" · ") || undefined;
      out.push({
        key: part.toolCallId, text: toolText(part), aside, kind, icon: declined ? "x" : toolWaiting(part) ? "message-circle" : KIND_ICON[kind],
        failed: toolFailed(part), waiting: toolWaiting(part), running: toolRunning(part), detail: toolDetail(part),
      });
    } else if (part.type === "reasoning" && part.text.trim()) {
      out.push({ key: `r${index}`, text: "Thought", kind: "other", icon: "brain", failed: false, waiting: false, running: part.state === "streaming", detail: part.text.trim() });
    } else if (part.type === "text" && part.text.trim()) {
      const first = part.text.trim().split("\n")[0];
      const long = part.text.trim().length > first.length || first.length > 110;
      out.push({ key: `t${index}`, text: first.length > 110 ? `${first.slice(0, 109)}…` : first, kind: "other", icon: "message-circle", failed: false, waiting: false, running: false, detail: long ? part.text.trim() : null });
    }
  });
  return out;
}

function EntryRow({ entry }: { entry: Entry }) {
  return (
    <SimpleWorkEntryRow
      slot={entry.waiting ? "tool-call-row" : "work-step"}
      label={entry.kind === "command" || entry.kind === "edit" || entry.kind === "read" || entry.kind === "search" ? <StepLabel text={entry.text} /> : entry.text}
      aside={entry.aside}
      iconName={entry.icon}
      failed={entry.failed}
      warning={entry.waiting}
      detail={entry.detail}
    />
  );
}

/** One live line while working; one fold once finished. */
export function WorkLog({ message, work }: { message: NekoUIMessage; work: NekoPart[] }) {
  const entries = useMemo(() => entriesOf(work), [work]);
  const meta = message.metadata ?? {};
  const running = meta.state === "pending";
  const started = meta.startedMs || meta.atMs || Date.now();
  const [open, setOpen] = useState(false);
  const worked = meta.startedMs && meta.atMs ? formatDuration((meta.atMs - meta.startedMs) / 1000) : null;
  const latest = entries[entries.length - 1];

  return (
    <section data-slot="work-log" data-running={running} className="pb-2">
      {running ? (
        <>
          <WorkingTimelineRow startedAt={started} />
          {latest ? (
            <div data-slot="work-live" className="pt-1">
              <LiveActivityRow
                label={latest.waiting ? <>{latest.text} <span className="text-warning-foreground">· waiting for you</span></> : <StepLabel text={latest.text} />}
                iconName={latest.icon}
                failed={latest.failed}
                active={!latest.waiting}
              />
            </div>
          ) : null}
        </>
      ) : (
        <>
          <TurnFoldTimelineRow
            label={worked ? `Worked for ${worked}` : "Worked"}
            expanded={open}
            disabled={entries.length === 0}
            onToggle={() => setOpen((v) => !v)}
            createdAt={meta.atMs ?? 0}
          />
          {open && entries.length ? (
            <div data-slot="work-steps" className="pt-1">
              <WorkLogList>
                {entries.map((entry) => <EntryRow key={entry.key} entry={entry} />)}
              </WorkLogList>
            </div>
          ) : null}
        </>
      )}
    </section>
  );
}

export { WorkLogDetails };
