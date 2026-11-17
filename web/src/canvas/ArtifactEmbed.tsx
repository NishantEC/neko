// One artifact, alone, for embed mode (shell/embed.ts): the same panel the artifact's tab shows (its
// version switcher, its viewer, Download, Comment) without the tab's header and without the shell
// around it. Comment mode and an artifact's `data-neko-send` need somewhere to put what they make: in
// the app that is Home's composer; here a composer is docked under the panel the first time one of
// them has something to say, and it is the same composer (nothing is sent until the person sends it).
import { useEffect, useRef, useState } from "react";
import { NekoComposer, type ComposerHandle } from "@/chat/ChatInputComposer";
import { MAX_OFFERS } from "@/chat/ChatInputDrafts";
import { chatSignature, useNekoChat } from "@/chat/sdk/chats";
import { useSelectedWorkspace } from "@/shell/workspace";
import { toast } from "@/ui/sonner";
import { useWorkbench } from "@/useStore";
import { ArtifactPanel } from "@/canvas/ArtifactPanel";
import { OFFER_EVENT, referenceText, SCOPE_NOTE, takeOffers } from "@/canvas/artifacts";

type Task = (draft: ComposerHandle) => void;

/**
 * The Rune client's web view hands the page `window.ipc`. In it a picked comment and an artifact's `data-neko-send` text go to the
 * window (`{ type: "quote" | "send", text }`) instead of a docked composer: the window quotes the comment into the open session's
 * composer and posts the send as the person's message. Without `window.ipc` nothing here changes.
 */
export const hostBridge = (): { postMessage(message: string): void } | null => {
  const ipc = (window as unknown as { ipc?: { postMessage?: unknown } }).ipc;
  return ipc && typeof ipc.postMessage === "function" ? (ipc as { postMessage(message: string): void }) : null;
};
export const tellHost = (type: "quote" | "send", text: string): boolean => {
  const host = hostBridge();
  if (!host) return false;
  host.postMessage(JSON.stringify({ type, text }));
  return true;
};

/** The composer, docked at the foot of the embedded panel. */
function EmbedComposer({ queue, ticks }: { queue: React.RefObject<Task[]>; ticks: number }) {
  const { snapshot } = useWorkbench();
  const draft = useRef<ComposerHandle | null>(null);
  const { chat, messages: all } = useNekoChat("neko", chatSignature(snapshot?.conversation));
  const queued = all.filter((m) => m.role === "user" && m.metadata?.state === "queued");
  const pendingTurn = [...all].reverse().find((m) => m.metadata?.state === "pending");
  const workspaceId: string | null = useSelectedWorkspace().id || null;
  useEffect(() => {
    let timer = 0;
    const flush = (tries = 20) => {
      const handle = draft.current;
      if (!handle) {
        if (tries > 0) timer = window.setTimeout(() => flush(tries - 1), 50);
        return;
      }
      for (const task of queue.current.splice(0)) task(handle);
      handle.focus();
    };
    flush();
    return () => clearTimeout(timer);
  }, [ticks, queue]);
  return (
    <div data-slot="embed-composer" className="shrink-0 border-t border-border/60 px-3 pt-2 pb-3">
      <NekoComposer chat={chat} pendingTurn={pendingTurn} queued={queued} decisionPending={false} workspaceId={workspaceId} draftRef={draft} />
    </div>
  );
}

export function ArtifactEmbed({ id, version }: { id: string | null; version: number | null }) {
  // `#/artifact?id=<id>&v=<n>` (the tab's own address) works too.
  const [hash, setHash] = useState(location.hash);
  useEffect(() => {
    const on = () => setHash(location.hash);
    addEventListener("hashchange", on);
    return () => removeEventListener("hashchange", on);
  }, []);
  const query = new URLSearchParams(hash.split("?")[1] ?? "");
  const shownId = id ?? query.get("id");
  const v = version ?? (Number(query.get("v")) || null);

  const [composing, setComposing] = useState(false);
  const [ticks, setTicks] = useState(0);
  const queue = useRef<Task[]>([]);
  const add = (task: Task) => {
    queue.current.push(task);
    setComposing(true);
    setTicks((t) => t + 1);
  };

  // What an artifact's data-neko-send button offers becomes a draft in the docked composer.
  useEffect(() => {
    const take = () => {
      for (const offer of takeOffers()) {
        if (tellHost("send", offer.text)) continue;
        add((draft) => {
          if (!draft.offer(offer.text, offer.name, offer.reference)) toast(`The composer already holds ${MAX_OFFERS} suggestions from artifacts; send or clear them first.`);
        });
      }
    };
    take();
    addEventListener(OFFER_EVENT, take);
    return () => removeEventListener(OFFER_EVENT, take);
  }, []);

  if (!shownId) return <p className="p-6 text-sm text-muted-foreground">No artifact named in this address.</p>;
  return (
    <div data-slot="artifact-embed" className="flex min-h-0 flex-1 flex-col">
      <div className="min-h-0 flex-1">
        <ArtifactPanel
          target={{ id: shownId, version: v }}
          place="tab"
          onVersion={(n) => {
            // Stay in embed mode: the other parameters of the address (embed, theme) are kept.
            const rest = new URLSearchParams(location.hash.split("?")[1] ?? "");
            rest.delete("id");
            rest.delete("v");
            const tail = rest.toString();
            location.hash = `/artifact/${shownId}/${n}${tail ? `?${tail}` : ""}`;
          }}
          onComment={(artifact, n, pick) => {
            if (tellHost("quote", referenceText(artifact.id, n, pick))) return;
            const what = pick.text ? `“${pick.text.slice(0, 40)}${pick.text.length > 40 ? "…" : ""}”` : pick.selector;
            add((draft) => draft.reference(`${artifact.title} v${n} · ${what}`, referenceText(artifact.id, n, pick), SCOPE_NOTE));
          }}
        />
      </div>
      {composing ? <EmbedComposer queue={queue} ticks={ticks} /> : null}
    </div>
  );
}
