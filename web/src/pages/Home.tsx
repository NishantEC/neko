// Home: the conversation with Neko. Conversation (use-stick-to-bottom), the
// turns drawn with T3 Code's timeline rows, and the composer in T3's chrome with
// the decision and the queue docked on its top edge. Beside it, the canvas: an
// artifact a reply made opens in the inspector and stays until it is closed.
import { useEffect, useMemo, useRef } from "react";
import { accept, command } from "@/api";
import { useWorkbench } from "@/useStore";
import { useSelectedWorkspace } from "@/shell/workspace";
import { CaretDown, ChatCircle, Stop } from "@/ui/icons";
import { Conversation, ConversationContent, ConversationEmptyState, ConversationScrollButton } from "@/chat/conversation";
import { Suggestion, Suggestions } from "@/chat/suggestion";
import { NekoComposer, type ComposerHandle } from "@/chat/ChatInputComposer";
import { Inspector } from "@/shell/inspector";
import { ArtifactPanel } from "@/canvas/ArtifactPanel";
import { closeCanvas, OFFER_EVENT, openCanvas, referenceText, SCOPE_NOTE, takeOffers, useArtifact, useCanvas } from "@/canvas/artifacts";
import { MAX_OFFERS } from "@/chat/ChatInputDrafts";
import { toast } from "@/ui/sonner";
import { Decisions, decisionsFromParts } from "@/chat/decisions";
import { NekoTurn, UserTurn } from "@/chat/turns";
import { chatSignature, useNekoChat } from "@/chat/sdk/chats";
import { registerCommands } from "@/shell/commands";
import { takeComposeRequest } from "@/shell/actions";
import { CrewHeader } from "@/chat/crew";

const SUGGESTIONS = [
  "What needs my attention right now?",
  "Which pull requests are ready to review?",
  "Start a task to fix the flaky login test",
];

export default function Home() {
  const { snapshot, connected, error } = useWorkbench();
  const draft = useRef<ComposerHandle | null>(null);
  const canvas = useCanvas();
  const { artifact: shown } = useArtifact(canvas?.id ?? null);
  // The conversation is the SDK's: `useChat` over the daemon's chat endpoints.
  // The snapshot only says when the log should be read again.
  const { chat, messages: all, loaded } = useNekoChat("neko", chatSignature(snapshot?.conversation));
  const messages = all.filter((m) => m.metadata?.state !== "queued");
  const queued = all.filter((m) => m.role === "user" && m.metadata?.state === "queued");
  const pendingTurn = [...all].reverse().find((m) => m.metadata?.state === "pending");
  const decisions = useMemo(() => decisionsFromParts(all, snapshot?.tasks ?? []), [all, snapshot?.tasks]);
  // The sidebar's workspace switcher: the chat runs in that workspace, or none for all.
  const workspaceId: string | null = useSelectedWorkspace().id || null;

  // What a reply saves opens beside the conversation through `ShowArtifact`
  // (SaveArtifact shows by default while a reply runs), which the shell obeys
  // on every page: canvas/artifacts.ts `obeyAgentRequests`.

  // "Ask Neko about this" (the context menu on selected text, shell/actions.ts): the quote
  // lands in the composer, whether Home was open already or opened for it.
  useEffect(() => {
    let timer = 0;
    const take = () => {
      const text = takeComposeRequest();
      if (!text) return;
      const fill = (tries: number) => {
        if (draft.current) {
          draft.current.set(text);
          draft.current.focus();
        } else if (tries > 0) timer = window.setTimeout(() => fill(tries - 1), 50);
      };
      fill(20);
    };
    take();
    addEventListener("neko:compose", take);
    return () => {
      removeEventListener("neko:compose", take);
      clearTimeout(timer);
    };
  }, []);

  // What an artifact's data-neko-send button offers lands here as a draft
  // (canvas/artifacts.ts offerFromArtifact); the person sends it, or not.
  useEffect(() => {
    let timer = 0;
    const take = (tries = 20) => {
      if (!draft.current) {
        if (tries > 0) timer = window.setTimeout(() => take(tries - 1), 50);
        return;
      }
      for (const offer of takeOffers()) {
        if (!draft.current.offer(offer.text, offer.name, offer.reference)) toast(`The composer already holds ${MAX_OFFERS} suggestions from artifacts; send or clear them first.`);
      }
    };
    const onOffer = () => take();
    take();
    addEventListener(OFFER_EVENT, onOffer);
    return () => {
      removeEventListener(OFFER_EVENT, onOffer);
      clearTimeout(timer);
    };
  }, []);

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
      <Inspector
        open={!!canvas}
        onClose={closeCanvas}
        title={shown?.title ?? "Canvas"}
        subtitle="Canvas"
        fill
      >
        {canvas ? (
          <ArtifactPanel
            target={canvas}
            onVersion={(n) => openCanvas(canvas.id, n)}
            onComment={(artifact, n, pick) => {
              const what = pick.text ? `“${pick.text.slice(0, 40)}${pick.text.length > 40 ? "…" : ""}”` : pick.selector;
              draft.current?.reference(`${artifact.title} v${n} · ${what}`, referenceText(artifact.id, n, pick), SCOPE_NOTE);
            }}
          />
        ) : null}
      </Inspector>
      <div className="mx-auto w-full max-w-(--chat-content-max-width) shrink-0 px-6 pt-2">
        <CrewHeader replying={!!pendingTurn} />
      </div>
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
