// The blocks that follow a task's conversation (TicketDetail.swift): the plan,
// the result with its independent review, and where the work went. The
// conversation itself is chat/TaskConversation.tsx.
import { useState, type ReactNode } from "react";
import { Check, Copy, GitPullRequest, Hammer, Question, Sparkle, ShieldCheck, WarningCircle, ArrowSquareOut } from "@/ui/icons";
import { Brain, GitMerge, MapTrifold, ShieldWarning } from "@/work/icons";
import { Button } from "@/ui/button";
import { cn } from "@/ui/lib/utils";
import { MonoLabel } from "@/editorial";
import { MessageResponse } from "@/chat/message";
import { CommandRow } from "@/chat/CommandRow";
import { StatusGlyph } from "@/work/tone";
import {
  OUTCOME_NOTE, OUTCOME_TITLE, parseResult, receiptFor, receiptOf,
  type Outcome, type Receipt, type Review, type Task,
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

// The conversation itself (bubbles, prose, the commands an agent ran, status
// lines) is chat/TaskConversation.tsx, drawn with T3 Code's timeline rows.
export { CommandRow };

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
