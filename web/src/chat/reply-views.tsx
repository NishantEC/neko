// The structured views Neko's own replies can carry, drawn here as the native
// app draws them (ReplyViews.swift): choices, form, plan, chart, files and
// sources. They arrive as typed `data-neko-*` parts of the message (the daemon
// reads the block the model wrote and types it once), so nothing here parses a
// fence. The reply prompt (REPLY_VIEWS in neko_chat.rs) asks the model for
// these instead of a paragraph when it needs an answer.
//
// A view answers by sending a message as the person, or by filling the
// composer; `NekoComposer` registers both for as long as it is mounted.
import { createContext, useContext, useState, type ReactNode } from "react";
import { Button } from "@/ui/button";
import { Input } from "@/ui/input";
import { Switch } from "@/ui/switch";
import { Check, Circle, CheckCircle } from "@/ui/icons";
import { cn } from "@/ui/lib/utils";
import type { Json } from "@/api";

export interface ReplyActions {
  send(text: string): void;
  draft(text: string): void;
}
let actions: ReplyActions = { send: () => undefined, draft: () => undefined };
export function setReplyActions(next: ReplyActions): () => void {
  actions = next;
  return () => { if (actions === next) actions = { send: () => undefined, draft: () => undefined }; };
}

/** Whether a later message already answered the one this view is in. */
const AnswerContext = createContext(false);
export const ReplyAnswered = AnswerContext.Provider;

const arr = (v: unknown): Json[] => (Array.isArray(v) ? v : []);
const str = (v: unknown): string => (typeof v === "string" ? v : v == null ? "" : String(v));

function Group({ kind, children }: { kind: string; children: ReactNode }) {
  return <div data-slot="reply-block" data-kind={kind} className="not-prose my-3 flex flex-col gap-4 rounded-2xl border border-border bg-card p-4 text-[14px] text-foreground">{children}</div>;
}
const Title = ({ children }: { children: ReactNode }) => <h3 className="text-[15px] font-semibold leading-snug">{children}</h3>;

function Choices({ value }: { value: Json }) {
  const options = arr(value.options);
  const [selected, setSelected] = useState<number | null>(null);
  const [clicked, setSent] = useState(false);
  const answered = useContext(AnswerContext);
  const sent = clicked || answered;
  return (
    <Group kind="choices">
      <Title>{str(value.question)}</Title>
      <div role="radiogroup" aria-label={str(value.question)} className="flex flex-col gap-1">
        {options.map((option, index) => {
          const on = selected === index;
          return (
            <button
              key={index}
              type="button"
              role="radio"
              aria-checked={on}
              disabled={sent}
              onClick={() => setSelected(index)}
              className={cn("flex items-start gap-2.5 rounded-lg px-2.5 py-2 text-start outline-none transition-colors hover:bg-accent focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-60", on && "bg-accent")}
            >
              <span aria-hidden className={cn("mt-0.5 shrink-0", on ? "text-primary" : "text-muted-foreground")}>{on ? <CheckCircle weight="fill" size={18} /> : <Circle size={18} />}</span>
              <span className="flex min-w-0 flex-col gap-0.5">
                <span>{str(option.label)}</span>
                {str(option.detail) ? <span className="text-[13px] text-muted-foreground">{str(option.detail)}</span> : null}
              </span>
            </button>
          );
        })}
      </div>
      <div className="flex justify-end gap-2">
        <Button variant="outline" size="sm" disabled={sent} onClick={() => actions.draft("")}>Other…</Button>
        <Button size="sm" disabled={selected === null || sent} onClick={() => { if (selected === null) return; setSent(true); actions.send(str(options[selected].label)); }}>
          {sent ? <><Check /> Sent</> : "Continue"}
        </Button>
      </div>
    </Group>
  );
}

function Form({ value }: { value: Json }) {
  const fields = arr(value.fields);
  const [values, setValues] = useState<Record<number, string>>({});
  const [toggles, setToggles] = useState<Record<number, boolean>>({});
  const [clicked, setSent] = useState(false);
  const answered = useContext(AnswerContext);
  const sent = clicked || answered;
  const current = (field: Json, index: number) => values[index] ?? (str(field.value) || (field.kind === "choice" ? str(arr(field.options)[0]) : ""));
  const submit = () => {
    const lines = [str(value.title) ? `${str(value.title)}:` : "Here are the details:"];
    fields.forEach((field, index) => {
      const answer = field.kind === "toggle" ? ((toggles[index] ?? field.value === true) ? "Yes" : "No") : current(field, index);
      lines.push(`- ${str(field.label)}: ${answer || "(left empty)"}`);
    });
    setSent(true);
    actions.send(lines.join("\n"));
  };
  return (
    <Group kind="form">
      {str(value.title) ? <Title>{str(value.title)}</Title> : null}
      <div className="flex flex-col gap-3">
        {fields.map((field, index) => (
          <label key={index} className="flex items-center justify-between gap-4">
            <span className="shrink-0">{str(field.label)}</span>
            {field.kind === "toggle" ? (
              <Switch checked={toggles[index] ?? field.value === true} disabled={sent} onCheckedChange={(on) => setToggles({ ...toggles, [index]: on })} />
            ) : field.kind === "choice" ? (
              <select
                disabled={sent}
                value={current(field, index)}
                onChange={(e) => setValues({ ...values, [index]: e.target.value })}
                className="h-8 min-w-40 rounded-lg border border-input bg-background px-2 text-[13px] outline-none focus-visible:ring-2 focus-visible:ring-ring"
              >
                {arr(field.options).map((option) => <option key={str(option)} value={str(option)}>{str(option)}</option>)}
              </select>
            ) : (
              <Input className="max-w-72" disabled={sent} value={current(field, index)} placeholder={str(field.placeholder)} onChange={(e) => setValues({ ...values, [index]: e.target.value })} />
            )}
          </label>
        ))}
      </div>
      <div className="flex justify-end"><Button size="sm" disabled={sent} onClick={submit}>{sent ? <><Check /> Sent</> : str(value.submit) || "Send"}</Button></div>
    </Group>
  );
}

function Plan({ value }: { value: Json }) {
  const steps = arr(value.steps).map((s) => (typeof s === "string" ? s : str(s.title)));
  const [clicked, setSent] = useState(false);
  const answered = useContext(AnswerContext);
  const sent = clicked || answered;
  return (
    <Group kind="plan">
      <div className="flex items-baseline justify-between gap-3">
        <Title>{str(value.title) || "Plan"}</Title>
        <span className="text-[12.5px] text-muted-foreground">{steps.length} {steps.length === 1 ? "step" : "steps"}{str(value.estimate) ? ` · ${str(value.estimate)}` : ""}</span>
      </div>
      <ol className="flex flex-col gap-2">
        {steps.map((step, index) => (
          <li key={index} className="flex gap-3"><span className="w-5 shrink-0 text-end text-[12.5px] tabular-nums text-muted-foreground">{index + 1}</span><span className="select-text">{step}</span></li>
        ))}
      </ol>
      <div className="flex justify-end gap-2">
        <Button variant="outline" size="sm" disabled={sent} onClick={() => actions.draft("Change the plan: ")}>Change plan…</Button>
        <Button size="sm" disabled={sent} onClick={() => { setSent(true); actions.send("Go ahead with this plan."); }}>{sent ? <><Check /> Sent</> : "Go ahead"}</Button>
      </div>
    </Group>
  );
}

function Chart({ value }: { value: Json }) {
  const bars = arr(value.bars).filter((b) => typeof b.value === "number" && Number.isFinite(b.value));
  const peak = Math.max(1, ...bars.map((b) => b.value as number));
  const unit = str(value.unit);
  return (
    <Group kind="chart">
      <div className="flex items-baseline justify-between gap-3">
        <Title>{str(value.title) || "Chart"}</Title>
        {str(value.source) ? <span className="text-[12.5px] text-muted-foreground">{str(value.source)}</span> : null}
      </div>
      <div className="flex flex-col gap-1.5">
        {bars.map((bar, index) => (
          <div key={index} className="flex items-center gap-2.5">
            <span className="w-28 shrink-0 truncate text-end text-[12.5px] text-muted-foreground">{str(bar.label)}</span>
            <span className="h-3 flex-1 overflow-hidden rounded-sm bg-foreground/[0.06]">
              <span className={cn("block h-full rounded-sm", bar.highlight ? "bg-warning" : "bg-primary/85")} style={{ width: `${Math.max(1, ((bar.value as number) / peak) * 100)}%` }} />
            </span>
            <span className={cn("w-16 shrink-0 text-[12.5px] tabular-nums", bar.highlight ? "text-warning-foreground" : "text-foreground/75")}>{Number.isInteger(bar.value) ? (bar.value as number).toLocaleString() : (bar.value as number).toFixed(1)}{unit}</span>
          </div>
        ))}
      </div>
    </Group>
  );
}

function Files({ value }: { value: Json }) {
  const files = arr(value.files).map((f) => (typeof f === "string" ? { path: f, note: "" } : { path: str(f.path), note: str(f.note) })).filter((f) => f.path.startsWith("/") || f.path.startsWith("~"));
  return (
    <Group kind="files">
      <Title>Files</Title>
      <ul className="flex flex-col gap-2">
        {files.map((file) => (
          <li key={file.path} className="flex flex-col gap-0.5">
            <span className="select-text font-medium">{file.path.split("/").pop()}</span>
            <span className="select-text break-all font-mono text-[12px] text-muted-foreground">{file.path}{file.note ? ` · ${file.note}` : ""}</span>
          </li>
        ))}
      </ul>
    </Group>
  );
}

function Sources({ value }: { value: Json }) {
  const items = arr(value.sources).map(str).filter(Boolean);
  if (items.length === 0) return null;
  return (
    <div data-slot="reply-block" data-kind="sources" className="not-prose my-2 flex flex-wrap items-center gap-1.5 text-[12.5px] text-muted-foreground">
      <span>Sources · {items.length}</span>
      {items.map((item) => <span key={item} className="select-text rounded-md bg-foreground/[0.05] px-1.5 py-0.5">{item}</span>)}
    </div>
  );
}

/** One `data-neko-<kind>` part, drawn. */
export function ReplyData({ kind, data }: { kind: string; data: unknown }): ReactNode {
  const value: Json = data && typeof data === "object" ? data : {};
  switch (kind) {
    case "neko-choices": return <Choices value={value} />;
    case "neko-form": return <Form value={value} />;
    case "neko-plan": return <Plan value={value} />;
    case "neko-chart": return <Chart value={value} />;
    case "neko-files": return <Files value={value} />;
    case "neko-sources": return <Sources value={value} />;
    default: return null;
  }
}
