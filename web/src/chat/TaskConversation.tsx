// A task's conversation with its agents, drawn with T3 Code's timeline rows
// (chat/t3/*, from t3code @3b95e7831, MIT), and the reply box under it.
// Self-contained so a page or a frame can mount it:
//
//   <TaskTimeline rows={threadRows(task)} status={task.status} />   the rows only
//   <TaskConversation task={task} />                               rows + plan, result, delivery, scrolling
//   <TaskComposer task={task} ui={ui} />                           the reply box
//
// The brief and your replies are bubbles; what each agent said is prose under
// its name; the commands it ran fold into one work-log entry with each command
// and its output on demand; status lines are T3's system dividers.
import { useEffect, useState } from "react";
import { CaretDown } from "@/ui/icons";
import { CircleAlertIcon, MessageCircleQuestionIcon } from "lucide-react";
import { cn } from "@/ui/lib/utils";
import { Conversation, ConversationContent, ConversationScrollButton } from "@/chat/conversation";
import { ChatInput } from "@/chat/chat-input";
import { ComposerBanner } from "@/chat/t3/ComposerBanner";
import { UserTimelineRow } from "@/chat/t3/TimelineUserMessageRow";
import { AssistantTimelineRow } from "@/chat/t3/TimelineAssistantRows";
import { TimelineSystemDivider } from "@/chat/t3/TimelineSystemDivider";
import { LiveActivityRow, ToolActivityIconView, WorkLogList } from "@/chat/t3/TimelineWorkRows";
import { WorkLogButton } from "@/chat/t3/WorkLog";
import { DeliveryCard, PlanBlock, ResultBlock } from "@/work/Thread";
import { CommandRow } from "@/chat/CommandRow";
import { taskActions, type Ui } from "@/work/taskActions";
import { conversationState, stopReason, waitingReason, workingLine, type Receipt, type Task } from "@/work/model";
import { chatFor, sendText, taskSignature, useNekoChat } from "@/chat/sdk/chats";
import { isToolPart, type NekoUIMessage } from "@/chat/sdk/types";
import { messageText, receiptOfPart } from "@/chat/sdk/parts";

const RECENT = 3;

/** What one agent ran during a stretch of work, folded into one entry. */
function StepsBlock({ author, commands, startsOpen }: { author: string; commands: Receipt[]; startsOpen: boolean }) {
  const [open, setOpen] = useState(startsOpen);
  const [all, setAll] = useState(false);
  const failed = commands.filter((c) => c.failed).length;
  const hidden = all ? 0 : Math.max(commands.length - RECENT, 0);
  const label = `${author} ran ${commands.length} ${commands.length === 1 ? "command" : "commands"}`;
  return (
    <div>
      <WorkLogButton
        aria-expanded={open}
        aria-label={failed ? `${label}, ${failed} failed` : label}
        onClick={() => setOpen((v) => !v)}
        icon={<ToolActivityIconView name="terminal" className="size-4 shrink-0 stroke-2 text-icon-muted" muted />}
        label={
          <span className="flex min-w-0 items-baseline gap-1.5">
            <span className="truncate">{label}</span>
            {failed ? <span className="shrink-0 text-destructive">{failed} failed</span> : null}
          </span>
        }
        trailing={<span aria-hidden className={cn("px-1 text-icon-muted opacity-70 transition-transform duration-200", open && "rotate-90")}>›</span>}
      />
      {open ? (
        <div className="ms-7">
          {hidden > 0 ? (
            <button type="button" className="py-0.5 text-xs text-muted-foreground hover:text-foreground" onClick={() => setAll(true)}>
              Show {hidden} earlier
            </button>
          ) : null}
          <WorkLogList>
            {commands.slice(hidden).map((receipt, i) => (
              <CommandRow key={i + hidden} receipt={receipt} />
            ))}
          </WorkLogList>
        </div>
      ) : null}
    </div>
  );
}

/** The agent's name above what it said, the way T3 attributes a message. */
function Author({ name }: { name: string }) {
  return <p data-slot="speaker" className="px-1 pb-1 text-2xs font-medium text-muted-foreground">{name}</p>;
}

/** The speaker line shows only when the speaker changes. */
function speakerOf(message: NekoUIMessage): string {
  return message.role === "assistant" ? message.metadata?.author ?? "Neko" : message.role === "user" ? "you" : "";
}

/** A task's conversation as the SDK's messages (the daemon's projection of its events). */
export function TaskTimeline({ messages, status }: { messages: NekoUIMessage[]; status: string }) {
  const working = workingLine(status);
  let previous = "";
  return (
    <div data-slot="thread" className="flex flex-col">
      {messages.map((message, index) => {
        const speaker = speakerOf(message);
        const showsSpeaker = speaker !== "" && speaker !== previous;
        if (speaker) previous = speaker;
        const at = message.metadata?.atMs ?? 0;
        if (message.role === "user") {
          const brief = message.metadata?.kind === "brief";
          return <UserTimelineRow key={message.id} text={messageText(message)} createdAt={at} label={brief ? "Request" : undefined} className={brief ? "pt-2" : "pt-6"} />;
        }
        if (message.role === "system") {
          const part = message.parts.find((p) => p.type === "data-neko-status");
          return <TimelineSystemDivider key={message.id} label={part && part.type === "data-neko-status" ? part.data.text ?? "" : ""} className="pt-5" />;
        }
        const author = message.metadata?.author ?? "Neko";
        // Prose and the commands between it, in the order they happened.
        const blocks: React.ReactNode[] = [];
        let run: Receipt[] = [];
        const flush = (key: string, last: boolean) => {
          if (!run.length) return;
          blocks.push(<div key={key} data-slot="agent-steps"><StepsBlock author={author} commands={run} startsOpen={last} /></div>);
          run = [];
        };
        message.parts.forEach((part, i) => {
          if (isToolPart(part)) run.push(receiptOfPart(part));
          else if (part.type === "text") {
            flush(`s${i}`, false);
            blocks.push(<AssistantTimelineRow key={`t${i}`} text={part.text} createdAt={at} author={author} />);
          }
        });
        flush("end", index === messages.length - 1);
        return (
          <div key={message.id} data-slot="agent-turn" className={showsSpeaker ? "pt-6" : "pt-2"}>
            {showsSpeaker ? <Author name={author} /> : null}
            {blocks}
          </div>
        );
      })}
      {working ? (
        <div data-slot="working" role="status" className="pt-6">
          <LiveActivityRow label={working} iconName="zap" active shimmer />
        </div>
      ) : null}
    </div>
  );
}

/** The whole conversation, scrolling, sticking to the end as the agent writes. */
export function TaskConversation({ task, className }: { task: Task; className?: string }) {
  const { messages } = useNekoChat(`task:${task.id}`, taskSignature(task));
  const state = conversationState(task);
  // A saved result from an earlier attempt belongs where that attempt ended: what you and the agents
  // said since comes after it, not above it.
  const stale = state.outcome === "stopped" || state.outcome === "previous";
  const lastReply = stale ? messages.findLastIndex((m) => m.role === "user" && m.metadata?.kind !== "brief") : -1;
  const before = lastReply > 0 ? messages.slice(0, lastReply) : messages;
  const after = lastReply > 0 ? messages.slice(lastReply) : [];
  const hasPlanBlock = String(task.plan ?? "").trim() !== "" && (state.showsPlan || state.outcome === "previous");
  return (
    <Conversation className={cn("min-h-0 flex-1", className)}>
      <ConversationContent className="mx-auto w-full max-w-(--chat-content-max-width) gap-0 px-5 pt-1 pb-6">
        <TaskTimeline messages={before} status={task.status} />
        {hasPlanBlock ? <PlanBlock title={state.showsPlan ? state.planTitle : "Plan"} text={task.plan} /> : null}
        {String(task.result ?? "") !== "" ? <ResultBlock task={task} outcome={state.outcome} reviewIsCurrent={state.reviewIsCurrent} /> : null}
        <DeliveryCard task={task} />
        {after.length ? <TaskTimeline messages={after} status={task.status} /> : null}
        {messages.length === 0 && !hasPlanBlock && !task.result ? (
          <p className="mt-10 text-center text-[13px] text-muted-foreground">Nothing has happened on this task yet.</p>
        ) : null}
      </ConversationContent>
      <ConversationScrollButton size="sm" className="gap-1.5 rounded-full bg-popover pe-3 ps-2 shadow-md">
        <CaretDown /> Scroll to end
      </ConversationScrollButton>
    </Conversation>
  );
}

const PLACEHOLDER: Record<string, string> = {
  AwaitingApproval: "Answer the agent, or tell it what to change…",
  ReadyForReview: "Ask a question or request changes…",
  Failed: "Tell the agent how to try again…",
  Cancelled: "Tell the agent how to try again…",
  Completed: "Reopen with a follow-up…",
};

// Drafts survive switching tasks (not a reload), like the native app's.
const drafts = new Map<string, string>();

/** A note on the composer's top edge: the planner's question, or why a task stopped. */
export function TaskTrayNote({ title, body, tone = "warning" }: { title: string; body: string; tone?: "warning" | "error" }) {
  const [open, setOpen] = useState(false);
  const first = body.split("\n")[0];
  const long = body.length > first.length || body.length > 140;
  const Icon = tone === "error" ? CircleAlertIcon : MessageCircleQuestionIcon;
  return (
    <ComposerBanner.Attachment>
      <ComposerBanner.Root data-slot="task-tray" role="note" variant={tone} density="comfortable">
        <ComposerBanner.Row layout="wrap-actions">
          <ComposerBanner.Icon className={tone === "error" ? "text-destructive" : "text-warning"}>
            <Icon />
          </ComposerBanner.Icon>
          <ComposerBanner.Content className="flex-col items-start gap-0.5 py-0.5">
            <span className={cn("font-medium", tone === "error" ? "text-destructive" : "text-warning")}>{title}</span>
            <span className={cn("text-foreground", !open && "line-clamp-2")}>{open ? body : first}</span>
          </ComposerBanner.Content>
          {long ? (
            <ComposerBanner.Actions>
              <button
                type="button"
                onClick={() => setOpen(!open)}
                aria-expanded={open}
                className="rounded-sm px-1.5 text-xs text-muted-foreground outline-none hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring"
              >
                {open ? "Less" : "Full text"}
              </button>
            </ComposerBanner.Actions>
          ) : null}
        </ComposerBanner.Row>
      </ComposerBanner.Root>
    </ComposerBanner.Attachment>
  );
}

/** The reply box under a task's conversation (TicketComposer in the SwiftUI app). */
export function TaskComposer({ task, ui }: { task: Task; ui: Ui }) {
  const id: string = task.id;
  const [text, setText] = useState(() => drafts.get(id) ?? "");
  const [sending, setSending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    setText(drafts.get(id) ?? "");
    setError(null);
  }, [id]);
  const change = (value: string) => {
    setText(value);
    drafts.set(id, value);
  };
  const running = ["Queued", "Planning", "Building", "Reviewing"].includes(task.status);
  const stop = taskActions(task, {}).find((a) => a.id === "stop");

  const send = async () => {
    const trimmed = text.trim();
    if (!trimmed || sending) return;
    setSending(true);
    setError(null);
    // The reply goes the SDK's way; the daemon runs `ReplyToTask` and the
    // conversation refills from its log.
    const sent = await sendText(chatFor(`task:${id}`), trimmed);
    if (sent.ok) change("");
    else setError(`${sent.error} Your text is still here.`);
    setSending(false);
  };

  const reason = waitingReason(task);
  const tray = reason ? (
    <TaskTrayNote title="The planner has a question" body={reason} />
  ) : task.status === "Failed" ? (
    <TaskTrayNote title="Why it stopped" body={stopReason(task)} tone="error" />
  ) : null;

  return (
    <ChatInput
      text={text}
      onTextChange={change}
      onSendMessage={send}
      sending={sending}
      isGenerating={running}
      onStop={() => stop?.run?.(ui)}
      error={error}
      maxLength={2048}
      placeholder={PLACEHOLDER[task.status] ?? "Redirect the agent. It pauses and replans…"}
      tray={tray}
      onKeyDown={(e) => {
        if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) void send();
      }}
    />
  );
}
