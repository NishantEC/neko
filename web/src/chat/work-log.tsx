// The WorkLog / ToolLine / ToolFold pattern from hushbacks
// (apps/spa/src/components/agentchat/components/Tools), ported to Neko's data:
// `activity` strings, `tool_calls`, and `started_ms` / `at_ms`.
//
// The rule it keeps: the transcript says what happened in a sentence, not which
// function ran. A line reports, a caret offers the detail, and what is
// underneath hangs off a rail because it belongs to the line above it. While
// the turn runs there is one live line with its verb shimmering; once it is
// finished there is one summary line and a caret.
import { useEffect, useMemo, useState } from "react";
import {
  CaretDown, Check, Clock, Eye, MagnifyingGlass, PencilSimple, Terminal, Wrench, WarningCircle, X,
} from "@/ui/icons";
import { cn } from "@/ui/lib/utils";
import { JsonViewer } from "@/chat/json-viewer";
import { clock, duration, sentence, splitStep, steps, type Step } from "@/chat/work";
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

const KIND_ICON = {
  command: Terminal,
  edit: PencilSimple,
  read: Eye,
  search: MagnifyingGlass,
  tool: Wrench,
  other: Check,
} as const;

/** A tool that only reports, said in a line. */
export function ToolLine({
  children, running, className,
}: { children: React.ReactNode; running?: boolean; className?: string }) {
  return (
    <div className={cn("my-1.5 flex items-center gap-2.5 py-1 text-[12.5px]", className)}>
      {running ? (
        <span aria-hidden className="size-1.5 shrink-0 animate-[pulse-dot_1.2s_ease-in-out_infinite] rounded-full bg-primary" />
      ) : (
        <Check size={11} weight="bold" className="shrink-0 text-foreground/45" aria-hidden="true" />
      )}
      <span className={cn("min-w-0", running ? "text-foreground" : "text-muted-foreground")}>{children}</span>
    </div>
  );
}

/** A tool with more to say than a line, said in a line until asked. */
export function ToolFold({
  label, aside, running, declined, defaultOpen = false, children,
}: { label: string; aside?: string; running?: boolean; declined?: boolean; defaultOpen?: boolean; children: React.ReactNode }) {
  const [open, setOpen] = useState(defaultOpen);
  return (
    <div className="my-1.5">
      <button
        type="button"
        onClick={() => setOpen((value) => !value)}
        aria-expanded={open}
        className="flex max-w-full items-center gap-2.5 rounded-sm py-1 text-start text-[12.5px] outline-none transition-colors hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring"
      >
        {running ? (
          <span aria-hidden className="size-1.5 shrink-0 animate-[pulse-dot_1.2s_ease-in-out_infinite] rounded-full bg-primary" />
        ) : declined ? (
          <X size={11} weight="bold" className="shrink-0 text-foreground/45" aria-hidden="true" />
        ) : (
          <Check size={11} weight="bold" className="shrink-0 text-foreground/45" aria-hidden="true" />
        )}
        <span className={cn("shrink-0", running ? "text-foreground" : "text-muted-foreground")}>{label}</span>
        {aside ? <span className="min-w-0 truncate text-muted-foreground/60">{aside}</span> : null}
        <CaretDown
          size={11}
          weight="bold"
          className={cn("shrink-0 text-muted-foreground transition-transform", open && "rotate-180")}
          aria-hidden="true"
        />
      </button>
      {open ? (
        <div className="ms-[5px] mt-0.5 border-s border-border ps-3.5 text-[12px] text-muted-foreground">{children}</div>
      ) : null}
    </div>
  );
}

/** A tool that did not do what it was asked, said once. */
export function ToolFailed({ what, detail }: { what: string; detail?: string }) {
  return (
    <div className="my-2 flex items-start gap-2 rounded-lg bg-destructive/10 px-3 py-2">
      <WarningCircle size={15} weight="bold" className="mt-px shrink-0 text-destructive" aria-hidden="true" />
      <div className="min-w-0">
        <p className="text-[13px] text-foreground">{what}</p>
        {detail ? <p className="mt-0.5 text-[12px] leading-5 text-muted-foreground">{detail}</p> : null}
      </div>
    </div>
  );
}

/** The newest step, with its verb shimmering: "Ran `git status`". */
function LiveStep({ text }: { text: string }) {
  const { verb, target, code } = splitStep(text);
  return (
    <div data-slot="work-live" className="flex min-h-9 items-center gap-2.5 text-[13px] text-foreground">
      <span aria-hidden className="size-1.5 shrink-0 animate-[pulse-dot_1.2s_ease-in-out_infinite] rounded-full bg-primary" />
      <span className="min-w-0 truncate tabular-nums">
        <span
          className="animate-shimmer-text bg-[linear-gradient(90deg,var(--muted-foreground)_0%,var(--strong)_50%,var(--muted-foreground)_100%)] bg-[length:200%_100%] bg-clip-text text-transparent motion-reduce:animate-none motion-reduce:bg-none motion-reduce:text-foreground"
        >
          {verb}
        </span>
        {target ? <> <span className={cn(code && "font-mono text-[12px]")}>{target}</span></> : null}
      </span>
    </div>
  );
}

function StepRow({ step }: { step: Step }) {
  const { verb, target, code } = splitStep(step.text);
  const Icon = KIND_ICON[step.kind];
  return (
    <li data-slot="work-step" data-failed={step.failed} className="flex min-h-6 items-center gap-2 py-px text-[12px]">
      {step.failed ? (
        <WarningCircle size={12} weight="bold" className="shrink-0 text-destructive" aria-hidden />
      ) : (
        <Icon size={12} className="shrink-0 text-foreground/40" aria-hidden />
      )}
      <span className="shrink-0 text-muted-foreground">{verb}</span>
      {target ? (
        <span className={cn("min-w-0 truncate text-foreground/80", code && "font-mono text-[11.5px]")}>{target}</span>
      ) : null}
      {step.failed ? <span className="ms-auto shrink-0 text-destructive">failed</span> : null}
    </li>
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

function parseArgs(json: string): unknown {
  try {
    return JSON.parse(json);
  } catch {
    return json;
  }
}

/** One MCP tool call the reply made: a line, with its arguments on demand. */
export function ToolCallLine({ call }: { call: ToolCall }) {
  const label = `${call.tool_name}`;
  const aside = `${call.connection_id} · ${STATUS_WORD[call.status] ?? call.status}`;
  if (call.status === "failed") {
    return <ToolFailed what={`${label} did not finish`} detail={`${call.connection_id} · failed`} />;
  }
  if (call.status === "awaiting_approval") {
    return (
      <div className="my-1.5 flex items-center gap-2.5 py-1 text-[12.5px] text-warning-foreground">
        <Clock size={11} weight="bold" className="shrink-0" aria-hidden />
        <span>{label}</span>
        <span className="text-muted-foreground/70">{aside}</span>
      </div>
    );
  }
  return (
    <ToolFold label={label} aside={aside} declined={call.status === "denied"} running={call.status === "running" || call.status === "approved"}>
      <JsonViewer data={parseArgs(call.arguments_json)} rootName="arguments" className="py-1 text-[11.5px]" />
    </ToolFold>
  );
}

/** One live line while working; one summary line once finished. */
export function WorkLog({ message }: { message: NekoMessage }) {
  const list: Step[] = useMemo(() => steps(message.activity), [message.activity]);
  const running = !!message.pending;
  const started = message.started_ms || message.at_ms;
  const elapsed = useElapsed(started, running);
  const [open, setOpen] = useState(false);
  const canExpand = list.length > 0;
  const summary = sentence(list);
  const worked = message.started_ms ? duration((message.at_ms - message.started_ms) / 1000) : null;

  return (
    <section data-slot="work-log" data-running={running} className="mb-3.5">
      <button
        type="button"
        data-slot="work-head"
        disabled={!canExpand || running}
        onClick={() => setOpen(!open)}
        aria-expanded={open}
        className="flex min-h-9 w-full items-center gap-1.5 border-b border-border text-start text-muted-foreground outline-none transition-colors enabled:hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring"
      >
        <span>{running ? `Working for ${duration(elapsed)}` : worked ? `Worked for ${worked}` : "Worked"}</span>
        {canExpand && !running ? (
          <CaretDown size={12} className={cn("transition-transform", open && "rotate-180")} aria-hidden />
        ) : null}
        <span className="flex-1" />
        {!running ? <time className="text-xs tabular-nums text-muted-foreground/70">{clock(message.at_ms)}</time> : null}
      </button>
      {running ? (
        list.length > 0 && <LiveStep text={list[list.length - 1].text} />
      ) : summary ? (
        <ToolLine className="my-0 min-h-9 py-0 text-[13px]">{summary}</ToolLine>
      ) : null}
      {open && !running ? (
        <ul data-slot="work-steps" className="ms-[5px] mb-1 border-s border-border ps-3.5">
          {list.map((step, i) => (
            <StepRow key={i} step={step} />
          ))}
        </ul>
      ) : null}
    </section>
  );
}
