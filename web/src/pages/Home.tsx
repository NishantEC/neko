// Home: the conversation with Neko. Built from the vendored chat set —
// Conversation (use-stick-to-bottom), the turns, ChatInput with its queue tray,
// and the decision dock — and nothing hand-rolled.
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { accept, command, type Json } from "@/api";
import { useWorkbench } from "@/useStore";
import { CaretDown, ChatCircle, Stop } from "@/ui/icons";
import { Badge } from "@/ui/badge";
import { Conversation, ConversationContent, ConversationEmptyState, ConversationScrollButton } from "@/chat/conversation";
import { Suggestion, Suggestions } from "@/chat/suggestion";
import { ChatInput } from "@/chat/chat-input";
import { QueueTray } from "@/chat/queue-tray";
import { Decisions, pendingDecisions } from "@/chat/decisions";
import { NekoTurn, UserTurn } from "@/chat/turns";
import type { NekoMessage } from "@/chat/types";
import { registerCommands } from "@/shell/commands";

const SUGGESTIONS = [
  "What needs my attention right now?",
  "Which pull requests are ready to review?",
  "Start a task to fix the flaky login test",
];

function Composer({
  pendingTurn, queued, decisionPending, workspaceId, draftRef,
}: {
  pendingTurn: NekoMessage | undefined;
  queued: NekoMessage[];
  decisionPending: boolean;
  workspaceId: string | null;
  draftRef: React.RefObject<{ set(text: string): void; focus(): void } | null>;
}) {
  const [text, setText] = useState("");
  const [sending, setSending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const area = useRef<HTMLTextAreaElement>(null);
  const trimmed = text.trim();

  useEffect(() => {
    draftRef.current = { set: setText, focus: () => area.current?.focus() };
    return () => {
      draftRef.current = null;
    };
  }, [draftRef]);

  const run = useCallback(async (step: () => Promise<Json>) => {
    setError(null);
    try {
      accept(await step());
      return true;
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      return false;
    }
  }, []);

  async function send() {
    if (!trimmed || sending) return;
    setSending(true);
    // The same command the native app sends from Home; while a reply is
    // running the daemon queues it, and the tray above shows it waiting.
    if (await run(() => command("SendMessage", { text: trimmed, workspace_id: workspaceId }))) setText("");
    setSending(false);
  }

  const edit = async (m: NekoMessage) => {
    if (await run(() => command("DiscardQueuedMessage", { message_id: m.id }))) {
      setText((current) => (current ? `${m.text}\n${current}` : m.text));
      area.current?.focus();
    }
  };
  const steer = (m: NekoMessage) =>
    run(async () => {
      await command("DiscardQueuedMessage", { message_id: m.id });
      return command("InterruptAndSendMessage", { text: m.text, workspace_id: m.workspace_id ?? workspaceId });
    });
  const remove = (m: NekoMessage) => run(() => command("DiscardQueuedMessage", { message_id: m.id }));

  return (
    <ChatInput
      ref={area}
      text={text}
      onTextChange={setText}
      onSendMessage={send}
      sending={sending}
      isGenerating={!!pendingTurn}
      onStop={() => pendingTurn && run(() => command("CancelChat", { turn_id: pendingTurn.id }))}
      error={error}
      placeholder={decisionPending ? "Queue a message while you decide…" : "Ask Neko anything · / for commands · @ for context"}
      leading={<Badge variant="outline" className="rounded-full border-border text-[12.5px] font-normal text-muted-foreground dark:border-border">Neko decides</Badge>}
      tray={<QueueTray queued={queued} onEdit={edit} onSteer={steer} onRemove={remove} />}
      onKeyDown={(e) => {
        if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) {
          if (e.metaKey && queued.length && !trimmed) void steer(queued[queued.length - 1]);
          else if (!(e.metaKey || e.ctrlKey)) void send();
        } else if (e.key === "ArrowUp" && e.altKey && queued.length) {
          e.preventDefault();
          void edit(queued[queued.length - 1]);
        }
      }}
    />
  );
}

export default function Home() {
  const { snapshot, connected, error } = useWorkbench();
  const draft = useRef<{ set(text: string): void; focus(): void } | null>(null);
  const all: NekoMessage[] = snapshot?.conversation ?? [];
  const messages = all.filter((m) => !(m.role === "user" && m.queued));
  const queued = all.filter((m) => m.role === "user" && m.queued);
  const pendingTurn = [...all].reverse().find((m) => m.pending);
  const decisions = useMemo(() => pendingDecisions(all, snapshot?.tasks ?? []), [all, snapshot?.tasks]);
  const workspaceId: string | null = null;

  // Actions the ⌘K palette offers while Home is open.
  useEffect(
    () =>
      registerCommands([
        { id: "home.focus", title: "Focus the composer", keywords: ["message", "type", "ask"], icon: ChatCircle, group: "Actions", run: () => draft.current?.focus() },
        ...(pendingTurn
          ? [{
              id: "home.stop",
              title: "Stop the current reply",
              keywords: ["cancel", "halt"],
              icon: Stop,
              group: "Actions",
              run: () => void command("CancelChat", { turn_id: pendingTurn.id }).then(accept).catch(() => {}),
            }]
          : []),
      ]),
    [pendingTurn?.id], // eslint-disable-line react-hooks/exhaustive-deps
  );

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <Conversation className="min-h-0 flex-1">
        <ConversationContent className="mx-auto w-full max-w-[728px] gap-0 px-6 pt-0 pb-6">
          {!snapshot && <p className="mt-24 text-center text-muted-foreground">{error ?? "Connecting to Neko…"}</p>}
          {snapshot && messages.length === 0 && (
            <ConversationEmptyState
              className="mt-24"
              icon={<ChatCircle size={22} weight="duotone" />}
              title="Ask Neko to look at your work, or to start something."
              description="It sees your tasks, pull requests and watches, and asks before it changes anything."
            >
              <Suggestions className="mt-1 max-w-md justify-center">
                {SUGGESTIONS.map((s) => (
                  <Suggestion key={s} suggestion={s} onClick={(text) => { draft.current?.set(text); draft.current?.focus(); }} />
                ))}
              </Suggestions>
            </ConversationEmptyState>
          )}
          {messages.map((m) => (m.role === "user" ? <UserTurn key={m.id} message={m} /> : <NekoTurn key={m.id} message={m} />))}
        </ConversationContent>
        <ConversationScrollButton
          data-slot="to-end"
          size="sm"
          className="gap-1.5 rounded-full bg-popover pe-3 ps-2 shadow-md dark:bg-popover dark:hover:bg-accent"
        >
          <CaretDown /> Scroll to end
        </ConversationScrollButton>
      </Conversation>
      <div className="mx-auto w-full max-w-[728px] px-6 pt-2 pb-5">
        {!connected && snapshot && (
          <p role="status" className="mb-2 ms-1.5 text-[13px] text-destructive">Can’t reach the Neko daemon. Showing the last known state.</p>
        )}
        <Decisions items={decisions} />
        <Composer pendingTurn={pendingTurn} queued={queued} decisionPending={decisions.length > 0} workspaceId={workspaceId} draftRef={draft} />
      </div>
    </div>
  );
}
