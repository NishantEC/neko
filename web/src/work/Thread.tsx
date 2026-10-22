// A ticket read as a conversation with its agent (TicketThread.swift): your
// brief and replies as bubbles, what each agent said as prose, the commands it
// ran folded into one line with their output on demand, and status lines. The
// plan, the result with its independent review, and where the work went follow
// as their own blocks (TicketDetail.swift). Built from the chat set.
import { useState, type ReactNode } from "react";
import { Check, CaretDown, Copy, GitPullRequest, Hammer, Question, Sparkle, ShieldCheck, WarningCircle, X, ArrowSquareOut } from "@/ui/icons";
import { Brain, GitMerge, MapTrifold, ShieldWarning } from "@/work/icons";
import { Button } from "@/ui/button";
import { cn } from "@/ui/lib/utils";
import { MonoLabel } from "@/editorial";
import { Message, MessageContent, MessageResponse } from "@/chat/message";
import { ToolFold } from "@/chat/work-log";
import { StatusGlyph } from "@/work/tone";
import {
  agentName, OUTCOME_NOTE, OUTCOME_TITLE, parseResult, receiptFor, receiptOf, workingLine,
  type Outcome, type Receipt, type Review, type Task, type ThreadRow,
} from "@/work/model";

const SPEAKER_ICON = { Planner: MapTrifold, Builder: Hammer, Reviewer: ShieldCheck, Memory: Brain, Neko: Sparkle } as const;

export function Speaker({ name, icon }: { name: string; icon?: ReactNode }) {
  const Icon = SPEAKER_ICON[name as keyof typeof SPEAKER_ICON];
  return (
    <div data-slot="speaker" className="mb-1.5 flex items-center gap-1.5 text-muted-foreground">
      {icon ?? (Icon ? <Icon size={13} aria-hidden /> : null)}
      <MonoLabel className="text-[10.5px]">{name}</MonoLabel>
    </div>
  );
}

const LONG_REQUEST = 600;

function Bubble({ text, label, clampable }: { text: string; label?: string; clampable?: boolean }) {
  const [full, setFull] = useState(false);
  const long = !!clampable && (text.length > LONG_REQUEST || text.split("\n").length > 10);
  return (
    <Message from="user" data-slot="user-turn" className="mt-8 max-w-full first:mt-2">
      {label ? <MonoLabel className="mb-1 ml-auto text-[10.5px]">{label}</MonoLabel> : null}
      <MessageContent
        className={cn(
          "max-w-[85%] gap-0 whitespace-pre-wrap break-words text-[14px] leading-[1.625] group-[.is-user]:rounded-2xl group-[.is-user]:bg-bubble group-[.is-user]:px-4 group-[.is-user]:py-3 group-[.is-user]:text-bubble-foreground",
          long && !full && "max-h-56 overflow-hidden [mask-image:linear-gradient(to_bottom,black_75%,transparent)]",
        )}
      >
        {text}
      </MessageContent>
      {long ? (
        <Button variant="ghost" size="xs" className="ml-auto mt-1 text-muted-foreground" onClick={() => setFull(!full)} aria-expanded={full}>
          {full ? "Show less" : "Show full request"}
        </Button>
      ) : null}
    </Message>
  );
}

/** One command the agent ran: outcome and command on a line; the output opens in place. */
export function CommandRow({ receipt }: { receipt: Receipt }) {
  const [open, setOpen] = useState(false);
  const Icon = receipt.incomplete ? Question : receipt.failed ? X : Check;
  const tint = receipt.incomplete ? "text-muted-foreground" : receipt.failed ? "text-destructive-foreground" : "text-success-foreground";
  return (
    <div data-slot="command-row" data-failed={receipt.failed}>
      <button
        type="button"
        onClick={() => setOpen(!open)}
        aria-expanded={open}
        aria-label={`${receipt.title}, ${receipt.status}`}
        title={receipt.command ?? receipt.title}
        className="flex min-h-7 w-full items-center gap-2 rounded-sm text-start outline-none hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring"
      >
        <Icon size={12} weight="bold" className={cn("shrink-0", tint)} aria-hidden />
        <span className="min-w-0 flex-1 truncate font-mono text-[12px] text-foreground/80">{receipt.title}</span>
        {receipt.exitCode ? <span className="shrink-0 font-mono text-[11.5px] text-destructive-foreground">exit {receipt.exitCode}</span> : null}
        <CaretDown size={11} className={cn("shrink-0 text-muted-foreground transition-transform", open && "rotate-180")} aria-hidden />
      </button>
      {open ? (
        <div className="mb-2 ms-5">
          {receipt.notices.map((n) => (
            <p key={n} className="mb-1 text-[12px] text-warning-foreground">{n}</p>
          ))}
          <pre className="max-h-72 overflow-auto whitespace-pre-wrap break-words rounded-lg bg-foreground/[0.04] px-3 py-2 font-mono text-[11.5px] leading-[1.55] text-muted-foreground">
            {receipt.output === null ? "Output was not saved." : receipt.output === "" ? "No output." : receipt.output}
          </pre>
        </div>
      ) : null}
    </div>
  );
}

const RECENT = 3;

/** What one agent ran during a stretch of work, folded into one line. */
function StepsBlock({ author, commands, startsOpen }: { author: string; commands: Receipt[]; startsOpen: boolean }) {
  const [all, setAll] = useState(false);
  const failed = commands.filter((c) => c.failed).length;
  const hidden = all ? 0 : Math.max(commands.length - RECENT, 0);
  return (
    <ToolFold
      label={`${author} ran ${commands.length} ${commands.length === 1 ? "command" : "commands"}`}
      aside={failed ? `${failed} failed` : undefined}
      defaultOpen={startsOpen}
    >
      {hidden > 0 ? (
        <Button variant="ghost" size="xs" className="mb-1 text-muted-foreground" onClick={() => setAll(true)}>
          Show {hidden} earlier
        </Button>
      ) : null}
      {commands.slice(hidden).map((receipt, i) => (
        <CommandRow key={i + hidden} receipt={receipt} />
      ))}
    </ToolFold>
  );
}

function StatusDivider({ text }: { text: string }) {
  return (
    <div role="separator" aria-label={text} className="mt-7 flex items-center gap-3 text-[12px] text-muted-foreground">
      <span className="h-px flex-1 bg-border" aria-hidden />
      <span className="max-w-[80%] truncate" title={text}>{text}</span>
      <span className="h-px flex-1 bg-border" aria-hidden />
    </div>
  );
}

export function ThreadView({ rows, status }: { rows: ThreadRow[]; status: string }) {
  const working = workingLine(status);
  return (
    <div data-slot="thread" className="flex flex-col">
      {rows.map((row, index) => {
        const { item } = row;
        const isLast = index === rows.length - 1;
        switch (item.kind) {
          case "brief": return <Bubble key={row.id} text={item.text} label="Request" clampable />;
          case "you": return <Bubble key={row.id} text={item.text} />;
          case "status": return <StatusDivider key={row.id} text={item.text} />;
          case "agent":
            return (
              <div key={row.id} data-slot="agent-turn" className={row.showsSpeaker ? "mt-8" : "mt-3"}>
                {row.showsSpeaker ? <Speaker name={agentName(item.role)} /> : null}
                <div data-slot="prose" className="text-[14px] leading-[1.625] text-foreground"><MessageResponse>{item.text}</MessageResponse></div>
              </div>
            );
          case "steps":
            return (
              <div key={row.id} data-slot="agent-steps" className={row.showsSpeaker ? "mt-8" : "mt-2"}>
                {row.showsSpeaker ? <Speaker name={agentName(item.role)} /> : null}
                <StepsBlock author={agentName(item.role)} commands={item.commands} startsOpen={isLast} />
              </div>
            );
        }
      })}
      {working ? (
        <div data-slot="working" role="status" className="mt-6 flex min-h-9 items-center gap-2.5 text-[13px] text-foreground">
          <span aria-hidden className="size-1.5 shrink-0 animate-[pulse-dot_1.2s_ease-in-out_infinite] rounded-full bg-primary" />
          <span className="animate-shimmer-text bg-[linear-gradient(90deg,var(--muted-foreground)_0%,var(--strong)_50%,var(--muted-foreground)_100%)] bg-[length:200%_100%] bg-clip-text text-transparent motion-reduce:animate-none motion-reduce:bg-none motion-reduce:text-foreground">
            {working}
          </span>
        </div>
      ) : null}
    </div>
  );
}

/** A block of the page: a quiet card, not a heading in the middle of a chat. */
export function Block({ children, className, ...rest }: { children: ReactNode; className?: string } & React.HTMLAttributes<HTMLElement>) {
  return (
    <section {...rest} className={cn("rounded-2xl border border-border bg-card px-4 py-3.5", className)}>
      {children}
    </section>
  );
}

export function PlanBlock({ title, text }: { title: string; text: string }) {
  return (
    <div data-slot="plan" className="mt-8">
      <Speaker name={title} icon={<MapTrifold size={13} aria-hidden />} />
      <div className="text-[14px] leading-[1.625]"><MessageResponse>{text}</MessageResponse></div>
    </div>
  );
}

function CopyButton({ text, label }: { text: string; label: string }) {
  const [copied, setCopied] = useState(false);
  return (
    <Button
      variant="ghost"
      size="icon-xs"
      aria-label={label}
      title={label}
      onClick={() => {
        navigator.clipboard?.writeText(text).catch(() => {});
        setCopied(true);
        setTimeout(() => setCopied(false), 1200);
      }}
    >
      {copied ? <Check /> : <Copy />}
    </Button>
  );
}

/** The independent reviewer's verdict, with the checks it ran and their outcome. */
export function ReviewCard({ verdict, current, receipts }: { verdict: Review; current: boolean; receipts: Receipt[] }) {
  const Icon = verdict.passed ? ShieldCheck : ShieldWarning;
  const tone = !verdict.passed ? "text-warning-foreground" : current ? "text-success-foreground" : "text-muted-foreground";
  return (
    <Block data-slot="review" data-passed={verdict.passed} className="mt-4 bg-foreground/[0.02]">
      <div className={cn("flex items-center gap-2 font-medium", tone)}>
        <Icon size={16} weight="fill" aria-hidden />
        {verdict.passed ? "Independent review passed" : "Independent review found issues"}
      </div>
      {!current ? <p className="mt-1 text-[12px] text-muted-foreground">Recorded for an earlier attempt.</p> : null}
      <div className="mt-2 text-[13.5px] leading-[1.6]"><MessageResponse>{verdict.summary}</MessageResponse></div>
      {verdict.findings.length ? (
        <ul className="mt-2 grid gap-1.5">
          {verdict.findings.map((f, i) => (
            <li key={i} className="flex items-start gap-2 text-[13px] leading-[1.55]">
              <WarningCircle size={14} className="mt-0.5 shrink-0 text-warning-foreground" aria-hidden />
              <span>{f}</span>
            </li>
          ))}
        </ul>
      ) : null}
      {verdict.tests.length ? (
        <div className="mt-3">
          <MonoLabel className="text-[10.5px]">Checks</MonoLabel>
          <div className="mt-1">
            {verdict.tests.map((test, i) => {
              const receipt = receiptFor(test, receipts);
              return receipt ? (
                <CommandRow key={i} receipt={receipt} />
              ) : (
                <div key={i} title="Reported by the reviewer; no saved run matches it exactly." className="flex min-h-7 items-center gap-2">
                  <Question size={12} className="shrink-0 text-muted-foreground" aria-hidden />
                  <span className="min-w-0 truncate font-mono text-[12px] text-foreground/80">{test}</span>
                </div>
              );
            })}
          </div>
        </div>
      ) : null}
    </Block>
  );
}

/** The agent's result: what it says it did, then the independent review. */
export function ResultBlock({ task, outcome, reviewIsCurrent }: { task: Task; outcome: Outcome; reviewIsCurrent: boolean }) {
  const result: string = task.result;
  const { body, verdict } = parseResult(result);
  const receipts = (task.events ?? []).map(receiptOf).filter((r: Receipt | null): r is Receipt => r !== null);
  return (
    <section data-slot="result" data-outcome={outcome} className="mt-8">
      <div className="flex items-center">
        <Speaker name="Result" icon={<ShieldCheck size={13} aria-hidden />} />
        <span className="flex-1" />
        <CopyButton text={result} label="Copy the full saved result, including the reviewer response" />
      </div>
      <div className="flex items-start gap-2.5">
        <StatusGlyph status={task.status} size={16} className="mt-0.5" />
        <div>
          <h3 className="font-medium text-strong">{OUTCOME_TITLE[outcome]}</h3>
          <p className="text-[12.5px] text-muted-foreground">{OUTCOME_NOTE[outcome]}</p>
        </div>
      </div>
      <div data-slot="prose" className="mt-3 text-[14px] leading-[1.625]">
        {body.trim() ? <MessageResponse>{body}</MessageResponse> : <p className="text-muted-foreground">No result text was saved for this task.</p>}
      </div>
      {verdict ? (
        <ReviewCard verdict={verdict} current={reviewIsCurrent} receipts={receipts} />
      ) : outcome === "reviewing" ? (
        <p className="mt-3 text-[12.5px] text-muted-foreground">The independent reviewer is checking this now.</p>
      ) : null}
    </section>
  );
}

/** Where the reviewed work went: a pull request, or a commit on local main. */
export function DeliveryCard({ task }: { task: Task }) {
  const url: string = task.pull_request?.url ?? "";
  const landed: string = task.landed_commit ?? "";
  if (!url && !landed) return null;
  const parts = url.split("/").filter(Boolean);
  const number = parts.length ? `#${parts[parts.length - 1]}` : "";
  const repo = parts.length >= 4 ? `${parts[2]}/${parts[3]}` : url;
  const Icon = url ? GitPullRequest : GitMerge;
  return (
    <Block data-slot="delivery" className="mt-6 flex items-center gap-3">
      <Icon size={18} className="shrink-0 text-primary" aria-hidden />
      <div className="min-w-0 flex-1">
        <div className="font-medium text-strong">{url ? `Pull request ${number}` : "Landed on main"}</div>
        <div className="truncate text-[12.5px] text-muted-foreground select-text">{url ? repo : landed.slice(0, 10)}</div>
      </div>
      {url ? (
        <Button variant="outline" size="sm" asChild>
          <a href={url} target="_blank" rel="noreferrer"><ArrowSquareOut /> Open on GitHub</a>
        </Button>
      ) : null}
    </Block>
  );
}
