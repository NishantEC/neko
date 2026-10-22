// What a reply did, drawn with T3 Code's work-log rows (chat/t3/WorkLog.tsx and
// TimelineWorkRows.tsx, from t3code @3b95e7831, MIT) over Neko's data:
// `activity` strings, `tool_calls`, and `started_ms` / `at_ms`.
//
// While the turn runs: "Working for 12s" over the newest step with its shine.
// Once it is finished: one "Worked for 1m 12s ›" fold, the steps inside it.
// A tool call is an entry with its arguments on demand.
import { useEffect, useMemo, useState, type ReactNode } from "react";
import { CircleAlertIcon } from "lucide-react";
import { cn } from "@/ui/lib/utils";
import { splitStep, steps, type Step } from "@/chat/work";
import { WorkLogButton, WorkLogDetails, WorkLogList, WorkLogRow } from "@/chat/t3/WorkLog";
import {
  LiveActivityRow, SimpleWorkEntryRow, ToolActivityIconView, TurnFoldTimelineRow, WorkingTimelineRow, formatDuration,
  type WorkEntryIconName,
} from "@/chat/t3/TimelineWorkRows";
import type { NekoMessage, ToolCall } from "@/chat/types";

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

export const KIND_ICON: Record<Step["kind"], WorkEntryIconName> = {
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

const STATUS_WORD: Record<string, string> = {
  awaiting_approval: "waiting for approval",
  approved: "approved",
  running: "running",
  succeeded: "done",
  failed: "failed",
  denied: "declined",
};

function prettyArgs(json: string): string {
  try {
    return JSON.stringify(JSON.parse(json), null, 2);
  } catch {
    return json;
  }
}

/** One MCP tool call the reply made: an entry, with its arguments on demand. */
export function ToolCallLine({ call }: { call: ToolCall }) {
  return (
    <div data-slot="tool-call" data-status={call.status}>
      <SimpleWorkEntryRow
        slot="tool-call-row"
        label={call.tool_name}
        aside={`${call.connection_id} · ${STATUS_WORD[call.status] ?? call.status}`}
        iconName={call.status === "awaiting_approval" ? "message-circle" : call.status === "denied" ? "x" : "hammer"}
        failed={call.status === "failed"}
        warning={call.status === "awaiting_approval"}
        detail={prettyArgs(call.arguments_json)}
      />
    </div>
  );
}

/** One live line while working; one fold once finished. */
export function WorkLog({ message }: { message: NekoMessage }) {
  const list: Step[] = useMemo(() => steps(message.activity), [message.activity]);
  const running = !!message.pending;
  const started = message.started_ms || message.at_ms;
  const [open, setOpen] = useState(false);
  const worked = message.started_ms ? formatDuration((message.at_ms - message.started_ms) / 1000) : null;
  const latest = list[list.length - 1];

  return (
    <section data-slot="work-log" data-running={running} className="pb-2">
      {running ? (
        <>
          <WorkingTimelineRow startedAt={started} />
          {latest ? (
            <div data-slot="work-live" className="pt-1">
              <LiveActivityRow label={<StepLabel text={latest.text} />} iconName={KIND_ICON[latest.kind]} failed={latest.failed} active />
            </div>
          ) : null}
        </>
      ) : (
        <>
          <TurnFoldTimelineRow
            label={worked ? `Worked for ${worked}` : "Worked"}
            expanded={open}
            disabled={list.length === 0}
            onToggle={() => setOpen((v) => !v)}
            createdAt={message.at_ms}
          />
          {open && list.length ? (
            <div data-slot="work-steps" className="pt-1">
              <WorkLogList>
                {list.map((step, i) => (
                  <SimpleWorkEntryRow key={i} label={<StepLabel text={step.text} />} iconName={KIND_ICON[step.kind]} failed={step.failed} />
                ))}
              </WorkLogList>
            </div>
          ) : null}
        </>
      )}
    </section>
  );
}

export { WorkLogDetails };
