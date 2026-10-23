// Neko's chat on the AI SDK: one `Chat` per conversation, a transport against
// the daemon's chat endpoints, and the hook that keeps them honest.
//
// The daemon owns the conversation. `useChat`'s state is a view of it: it is
// filled from `GET /api/chat/<conversation>`, follows a reply in flight over the
// SDK's own resume path (`GET .../stream`, a full replay), and is refilled
// whenever the daemon says something changed (the workbench snapshot's chat
// signature) and this window is not itself in the middle of a stream.
import { Chat, useChat } from "@ai-sdk/react";
import { DefaultChatTransport } from "ai";
import { useEffect, useRef, useState } from "react";
import type { Json } from "@/api";
import type { NekoUIMessage } from "@/chat/sdk/types";

/**
 * The SDK's HTTP transport pointed at `/api/chat/<conversation>/stream`, with
 * the body the daemon reads: the one new message, and what Neko adds around it
 * (`workspaceId`, `interrupt`). The daemon keeps the history; it is never sent.
 * A reconnect is the SDK's default `GET /api/chat/<conversation>/stream`.
 */
export class NekoChatTransport extends DefaultChatTransport<NekoUIMessage> {
  constructor() {
    super({
      api: "/api/chat",
      prepareSendMessagesRequest: ({ id, messages, trigger, body }) => ({
        api: `/api/chat/${encodeURIComponent(id)}/stream`,
        body: { trigger, message: messages[messages.length - 1], ...body },
      }),
    });
  }
}

const transport = new NekoChatTransport();
const chats = new Map<string, Chat<NekoUIMessage>>();

export function chatFor(conversation: string): Chat<NekoUIMessage> {
  let chat = chats.get(conversation);
  if (!chat) {
    chat = new Chat<NekoUIMessage>({ id: conversation, transport, messages: [] });
    chats.set(conversation, chat);
  }
  return chat;
}

const busy = (chat: Chat<NekoUIMessage>) => chat.status === "submitted" || chat.status === "streaming";

interface Log {
  messages: NekoUIMessage[];
  active: string | null;
}

const loading = new Map<string, "running" | "again">();

/** Fills the chat from the daemon's log, and follows a reply that is in flight. */
export async function loadChat(conversation: string): Promise<void> {
  const chat = chatFor(conversation);
  if (loading.has(conversation)) {
    loading.set(conversation, "again");
    return;
  }
  loading.set(conversation, "running");
  try {
    const response = await fetch(`/api/chat/${encodeURIComponent(conversation)}`);
    if (!response.ok) return;
    const log = (await response.json()) as Log;
    if (busy(chat)) {
      // This window is following a reply: that message is current, and stays as
      // the stream has it. Everything around it (a message queued behind it, a
      // settled approval) is read from the log.
      const following = [...chat.messages].reverse().find((m) => m.role === "assistant");
      if (following && log.messages.some((m) => m.id === following.id)) {
        chat.messages = log.messages.map((m) => (m.id === following.id ? following : m));
      } else if (following) {
        // The reply this window follows is not in the log (yet): keep it, and
        // still take everything else from the log, so a message queued behind
        // it that was edited or removed leaves the tray.
        chat.messages = [...log.messages.filter((m) => m.id !== following.id), following];
      } else {
        // Nothing of the reply has arrived here yet: the log is the whole truth.
        chat.messages = log.messages;
      }
      return;
    }
    chat.messages = log.messages;
    if (log.active) void chat.resumeStream();
  } catch {
    /* offline: keep what is shown; the next change tries again */
  } finally {
    const again = loading.get(conversation) === "again";
    loading.delete(conversation);
    if (again) void loadChat(conversation);
  }
}

/**
 * A fingerprint of everything in the snapshot's conversation that is not a
 * token: messages, their lifecycle, and what waits for a click. It changes when
 * the chat log should be read again.
 */
export function chatSignature(conversation: Json[] | undefined): string {
  return (conversation ?? [])
    .map((m) => [
      m.id, m.pending ? "p" : "", m.queued ? "q" : "", m.failed ? "f" : "",
      (m.tool_calls ?? []).map((c: Json) => c.status).join(","),
      (m.actions ?? []).map((a: Json) => a.status).join(","),
    ].join(":"))
    .join("|");
}

/** The task conversation changes when its events or status do. */
export function taskSignature(task: Json | undefined): string {
  return task ? `${task.id}:${task.status}:${(task.events ?? []).length}` : "";
}

/**
 * `useChat` over one conversation. `signature` says when the daemon's log has
 * changed (see `chatSignature`); the log is also read when a stream of this
 * window ends, so what is shown is what is stored.
 */
export function useNekoChat(conversation: string, signature: string) {
  const chat = chatFor(conversation);
  const helpers = useChat<NekoUIMessage>({ chat, throttle: 40 });
  const [loaded, setLoaded] = useState(false);
  useEffect(() => {
    void loadChat(conversation).then(() => setLoaded(true));
  }, [conversation, signature]);
  const before = useRef(helpers.status);
  useEffect(() => {
    const was = before.current;
    before.current = helpers.status;
    if ((was === "streaming" || was === "submitted") && helpers.status === "ready") void loadChat(conversation);
  }, [conversation, helpers.status]);
  return { chat, loaded, ...helpers };
}

/**
 * Sends a message the SDK's way: the transport posts it and the reply streams
 * back on the same response. Returns whether the daemon took it; on a refusal
 * the daemon's words are in the error.
 */
export async function sendText(
  chat: Chat<NekoUIMessage>,
  text: string,
  options: { workspaceId?: string | null; interrupt?: boolean } = {},
): Promise<{ ok: true } | { ok: false; error: string }> {
  await chat.sendMessage({ text }, { body: { workspaceId: options.workspaceId ?? null, interrupt: options.interrupt === true } });
  if (chat.status === "error") {
    const error = chat.error?.message || "The message was not sent.";
    chat.clearError();
    // The optimistic copy of the message goes; the log is the truth.
    void loadChat(chat.id);
    return { ok: false, error };
  }
  return { ok: true };
}
