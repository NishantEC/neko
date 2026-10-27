// Home: the conversation with Neko. Conversation (use-stick-to-bottom), the
// turns drawn with T3 Code's timeline rows, and the composer in T3's chrome with
// the decision and the queue docked on its top edge.
import { useEffect, useMemo, useRef } from "react";
import { accept, command } from "@/api";
import { useWorkbench } from "@/useStore";
import { useSelectedWorkspace } from "@/shell/workspace";
import { CaretDown, ChatCircle, Stop } from "@/ui/icons";
import { Conversation, ConversationContent, ConversationEmptyState, ConversationScrollButton } from "@/chat/conversation";
import { Suggestion, Suggestions } from "@/chat/suggestion";
import { NekoComposer } from "@/chat/ChatInputComposer";
import { Decisions, decisionsFromParts } from "@/chat/decisions";
import { NekoTurn, UserTurn } from "@/chat/turns";
import { chatSignature, useNekoChat } from "@/chat/sdk/chats";
import { registerCommands } from "@/shell/commands";

const SUGGESTIONS = [
  "What needs my attention right now?",
  "Which pull requests are ready to review?",
  "Start a task to fix the flaky login test",
];

export default function Home() {
  const { snapshot, connected, error } = useWorkbench();
  const draft = useRef<{ set(text: string): void; focus(): void } | null>(null);
  // The conversation is the SDK's: `useChat` over the daemon's chat endpoints.
  // The snapshot only says when the log should be read again.
  const { chat, messages: all, loaded } = useNekoChat("neko", chatSignature(snapshot?.conversation));
  const messages = all.filter((m) => m.metadata?.state !== "queued");
  const queued = all.filter((m) => m.role === "user" && m.metadata?.state === "queued");
  const pendingTurn = [...all].reverse().find((m) => m.metadata?.state === "pending");
  const decisions = useMemo(() => decisionsFromParts(all, snapshot?.tasks ?? []), [all, snapshot?.tasks]);
  // The sidebar's workspace switcher: the chat runs in that workspace, or none for all.
  const workspaceId: string | null = useSelectedWorkspace().id || null;

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
        <ConversationContent className="mx-auto w-full max-w-(--chat-content-max-width) gap-0 px-6 pt-0 pb-6">
          {!snapshot && <p className="mt-24 text-center text-muted-foreground">{error ?? "Connecting to Neko…"}</p>}
          {snapshot && loaded && messages.length === 0 && (
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
          {messages.map((m, index) =>
            m.role === "user" ? (
              <UserTurn key={m.id} message={m} />
            ) : (
              <NekoTurn key={m.id} message={m} answered={messages.slice(index + 1).some((later) => later.role === "user")} />
            ),
          )}
        </ConversationContent>
        <ConversationScrollButton
          data-slot="to-end"
          size="sm"
          className="gap-1.5 rounded-full bg-popover pe-3 ps-2 shadow-md dark:bg-popover dark:hover:bg-accent"
        >
          <CaretDown /> Scroll to end
        </ConversationScrollButton>
      </Conversation>
      <div className="mx-auto w-full max-w-(--chat-content-max-width) px-6 pt-2 pb-6">
        {!connected && snapshot && (
          <p role="status" className="mb-2 ms-1.5 text-[13px] text-destructive">Can’t reach the Neko daemon. Showing the last known state.</p>
        )}
        <NekoComposer
          chat={chat}
          pendingTurn={pendingTurn}
          queued={queued}
          decisionPending={decisions.length > 0}
          workspaceId={workspaceId}
          draftRef={draft}
          tray={<Decisions items={decisions} />}
        />
      </div>
    </div>
  );
}
