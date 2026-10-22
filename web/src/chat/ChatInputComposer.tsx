// Neko's Home composer: ChatInput plus everything the native composer does
// (SharedComposer + ComposerMenus + TodayView.send): drafts per scope, `/`
// commands, `@` mentions of workspaces, connections and files, attachments,
// Ask / Plan, the runtime picker, and Return / ⇧Return / ⌘Return.
//
//   Return        sends (queues while a reply runs)       -> SendMessage
//   ⇧ Return      new line
//   ⌘ Return      stops the reply and sends this first    -> InterruptAndSendMessage
//   / and @       menu above the composer: ↑ ↓ choose, ↵ or Tab accept, Esc closes
//
// The daemon contract is unchanged: plain text. Attachments are kept by the
// daemon (POST /api/attachments) and referenced in the text exactly as the
// native composer references them.
import { useCallback, useEffect, useId, useMemo, useRef, useState } from "react";
import { accept, command, type Json } from "@/api";
import { useWorkbench } from "@/useStore";
import { navigate } from "@/router";
import { ChatInput } from "@/chat/chat-input";
import { ComposerMenu, type ComposerMenuItem } from "@/chat/ChatInputMenu";
import { RuntimePicker } from "@/chat/ChatInputRuntime";
import { QueueTray } from "@/chat/queue-tray";
import {
  attachFile, completeSubmission, getDraft, removeDraftAttachment, setDraftMode, setDraftText, submission, useDraft,
  type ComposerMode,
} from "@/chat/ChatInputDrafts";
import {
  homeText, mentionQuery, parseSlash, replaceTrailingMention, slashChatText, slashSuggestions, TAKES_ARGUMENT, validationError, MESSAGE_LIMIT_BYTES,
  type SlashCommand,
} from "@/chat/ChatInputSlash";
import { Command as CommandIcon, File as FileIcon, FolderSimple, ListNumbers, PuzzlePiece } from "@/ui/icons";
import { toast } from "@/ui/sonner";
import { cn } from "@/ui/lib/utils";
import { openOnboarding } from "@/onboarding/open";
import { stopAllWork } from "@/shell/actions";
import type { NekoMessage } from "@/chat/types";

interface FoundFile { path: string; name: string; location: string }

/** The profile Home speaks to for a workspace (TodayView.scope). */
export function profileFor(snapshot: Json | null, workspaceId: string | null): string {
  const profiles = snapshot?.agent_profiles ?? {};
  if (workspaceId) {
    return (profiles.assignments ?? []).find((a: Json) => a.workspace_id === workspaceId)?.profile_id ?? "default";
  }
  return profiles.active_profile_id || "default";
}

function ModeSwitch({ mode, onChange }: { mode: ComposerMode; onChange(mode: ComposerMode): void }) {
  const options: { id: ComposerMode; label: string; help: string }[] = [
    { id: "ask", label: "Ask", help: "Ask: Neko answers now" },
    { id: "plan", label: "Plan", help: "Plan: Neko proposes a task with a plan for your approval, without changing anything" },
  ];
  return (
    <div role="radiogroup" aria-label="Ask or Plan" data-slot="mode-switch" className="flex rounded-full bg-foreground/[0.06] p-0.5">
      {options.map((o) => (
        <button
          key={o.id}
          type="button"
          role="radio"
          aria-checked={mode === o.id}
          title={o.help}
          onClick={() => onChange(o.id)}
          className={cn(
            "h-7 rounded-full px-3 text-[12.5px] font-medium outline-none transition-colors focus-visible:ring-2 focus-visible:ring-ring",
            mode === o.id ? "bg-card text-strong shadow-xs ring-1 ring-border" : "text-muted-foreground hover:text-foreground",
          )}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}

export function NekoComposer({
  pendingTurn, queued, decisionPending, workspaceId, draftRef, tray: extraTray,
}: {
  pendingTurn: NekoMessage | undefined;
  queued: NekoMessage[];
  decisionPending: boolean;
  workspaceId: string | null;
  draftRef: React.RefObject<{ set(text: string): void; focus(): void } | null>;
  tray?: React.ReactNode;
}) {
  const { snapshot } = useWorkbench();
  const profileId = profileFor(snapshot, workspaceId);
  const scope = `${workspaceId ?? "*"}:${profileId}`;
  const draft = useDraft(scope);
  const [sending, setSending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [index, setIndex] = useState(0);
  const [dismissed, setDismissed] = useState<string | null>(null);
  const [files, setFiles] = useState<FoundFile[]>([]);
  const [modelsOpen, setModelsOpen] = useState(0);
  const area = useRef<HTMLTextAreaElement>(null);
  const menuId = useId();
  const text = draft.text;
  const planOnly = draft.mode === "plan";

  useEffect(() => {
    draftRef.current = { set: (value) => setDraftText(scope, value), focus: () => area.current?.focus() };
    return () => {
      draftRef.current = null;
    };
  }, [draftRef, scope]);

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

  // ---- slash commands ---------------------------------------------------------------
  const runSlash = useCallback(
    async (cmd: SlashCommand) => {
      const open = (route: string) => navigate(route);
      switch (cmd.kind) {
        case "stop":
          await stopAllWork();
          break;
        case "clear": {
          const before = (snapshot?.tasks ?? []).length;
          const reply = await command("ClearFinishedTasks", { workspace_id: workspaceId }).catch((e: Error) => {
            setError(e.message);
            return null;
          });
          if (reply) {
            accept(reply);
            const removed = before - (reply.Workbench?.tasks ?? []).length;
            toast(removed === 0 ? "No finished tasks to clear." : `Cleared ${removed} finished ${removed === 1 ? "task" : "tasks"}. Files and worktrees are untouched.`);
          }
          break;
        }
        case "memory": open("memory"); break;
        case "tickets": open("tasks"); break;
        case "models": setModelsOpen((n) => n + 1); break;
        case "permissions": openOnboarding("permissions"); break;
        case "setup": openOnboarding("welcome"); break;
        case "help": setDraftText(scope, "/"); area.current?.focus(); break;
        case "plan": setDraftMode(scope, "plan"); break;
        default: break;
      }
    },
    [run, snapshot?.tasks, workspaceId, scope],
  );

  // ---- sending (TodayView.send) --------------------------------------------------------
  const send = useCallback(
    async (interrupt = false) => {
      if (sending) return;
      const s = submission(scope);
      if (!s.text) return;
      const cmd = parseSlash(s.text);
      if (cmd && slashChatText(cmd) === null) {
        completeSubmission(s, true);
        await runSlash(cmd);
        return;
      }
      const body = homeText(s.text, getDraft(scope).mode === "plan");
      const invalid = validationError(body);
      if (invalid) return setError(invalid);
      setSending(true);
      const ok = await run(() => command(interrupt ? "InterruptAndSendMessage" : "SendMessage", { text: body, workspace_id: workspaceId }));
      completeSubmission(s, ok);
      setSending(false);
    },
    [sending, scope, runSlash, run, workspaceId],
  );

  // ---- the queue tray ------------------------------------------------------------------
  const edit = async (m: NekoMessage) => {
    if (await run(() => command("DiscardQueuedMessage", { message_id: m.id }))) {
      const current = getDraft(scope).text;
      setDraftText(scope, current ? `${m.text}\n${current}` : m.text);
      area.current?.focus();
    }
  };
  const steer = (m: NekoMessage) =>
    run(async () => {
      await command("DiscardQueuedMessage", { message_id: m.id });
      return command("InterruptAndSendMessage", { text: m.text, workspace_id: m.workspace_id ?? workspaceId });
    });
  const remove = (m: NekoMessage) => run(() => command("DiscardQueuedMessage", { message_id: m.id }));

  // ---- the menu ----------------------------------------------------------------------
  const query = dismissed === text ? null : mentionQuery(text);
  useEffect(() => {
    setIndex(0);
  }, [text]);
  // Files for an "@" mention, as the native composer searches them (180 ms after the last key).
  useEffect(() => {
    if (query === null || query.length < 2) {
      setFiles([]);
      return;
    }
    // A superseded search is ignored when it answers (not aborted: an abort is a failed request).
    let stale = false;
    const timer = setTimeout(async () => {
      try {
        const response = await fetch(`/api/files?q=${encodeURIComponent(query)}&workspace=${encodeURIComponent(workspaceId ?? "")}`);
        if (response.ok && !stale) setFiles((await response.json()).files ?? []);
      } catch {
        /* offline: keep what is shown */
      }
    }, 180);
    return () => {
      clearTimeout(timer);
      stale = true;
    };
  }, [query, workspaceId]);

  const items = useMemo<ComposerMenuItem[]>(() => {
    if (dismissed === text) return [];
    if (query !== null) {
      const q = query.toLowerCase();
      const out: ComposerMenuItem[] = [];
      const put = (replacement: string) => () => {
        setDraftText(scope, replaceTrailingMention(getDraft(scope).text, replacement));
        area.current?.focus();
      };
      for (const w of snapshot?.workspaces ?? []) {
        if (q && !String(w.name).toLowerCase().includes(q)) continue;
        out.push({ id: `w:${w.id}`, icon: FolderSimple, title: w.name, detail: "Workspace", apply: put(`Workspace: ${w.name}`) });
      }
      for (const c of snapshot?.mcp?.connections ?? []) {
        if (!c.enabled || (q && !String(c.label).toLowerCase().includes(q))) continue;
        const n = (c.tools ?? []).length;
        out.push({ id: `c:${c.id}`, icon: PuzzlePiece, title: c.label, detail: `${n} ${n === 1 ? "tool" : "tools"}`, apply: put(`@${c.label} `) });
      }
      for (const f of files) out.push({ id: `f:${f.path}`, icon: FileIcon, title: f.name, detail: f.location, apply: put(`${f.path} `) });
      return out.slice(0, 9);
    }
    return slashSuggestions(text).map((entry) => ({
      id: entry.name,
      icon: entry.name === "/plan" ? ListNumbers : CommandIcon,
      title: entry.name,
      detail: entry.detail,
      shortcut: entry.shortcut,
      apply: () => {
        if (entry.name === "/plan") {
          setDraftMode(scope, "plan");
          setDraftText(scope, "");
        } else if (TAKES_ARGUMENT.includes(entry.name)) {
          setDraftText(scope, `${entry.name} `);
        } else {
          setDraftText(scope, entry.name);
          void send();
        }
        area.current?.focus();
      },
    }));
  }, [dismissed, text, query, snapshot?.workspaces, snapshot?.mcp?.connections, files, scope, send]);

  const active = items.length ? Math.min(index, items.length - 1) : -1;

  const onKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.nativeEvent.isComposing) return;
    if (items.length && !(e.metaKey || e.ctrlKey || e.altKey || e.shiftKey)) {
      if (e.key === "ArrowDown") { e.preventDefault(); setIndex((active + 1) % items.length); return; }
      if (e.key === "ArrowUp") { e.preventDefault(); setIndex((active - 1 + items.length) % items.length); return; }
      if (e.key === "Enter" || e.key === "Tab") { e.preventDefault(); items[active].apply(); return; }
      if (e.key === "Escape") { e.preventDefault(); e.nativeEvent.stopPropagation(); setDismissed(text); return; }
    }
    if (e.key === "Enter" && !e.shiftKey) {
      if (e.metaKey || e.ctrlKey) {
        // Interrupt and send. The decision dock also listens for ⌘↵ (approve) on the
        // window; this key press is the composer's, so it goes no further.
        e.nativeEvent.stopPropagation();
        if (submission(scope).text) void send(true);
        else if (queued.length) void steer(queued[queued.length - 1]);
      } else if (!e.altKey) {
        void send();
      }
    } else if (e.key === "ArrowUp" && e.altKey && queued.length) {
      e.preventDefault();
      void edit(queued[queued.length - 1]);
    }
  };

  const payload = homeText(submission(scope).text, planOnly);
  const sizeError = validationError(payload);
  const state = items.length ? { id: menuId, activeId: `${menuId}-${active}` } : null;

  return (
    <ChatInput
      ref={area}
      text={text}
      onTextChange={(value) => { setDraftText(scope, value); if (error) setError(null); }}
      onSendMessage={() => void send()}
      sending={sending}
      isGenerating={!!pendingTurn}
      onStop={() => pendingTurn && void run(() => command("CancelChat", { turn_id: pendingTurn.id }))}
      error={error ?? sizeError}
      limitBytes={MESSAGE_LIMIT_BYTES}
      sizeText={payload}
      placeholder={decisionPending ? "Queue a message while you decide…" : "Ask Neko anything · / for commands · @ for context"}
      leading={<ModeSwitch mode={draft.mode} onChange={(mode) => setDraftMode(scope, mode)} />}
      controls={<RuntimePicker conversationId={`home:${profileId}:${workspaceId ?? "*"}`} openRequest={modelsOpen} />}
      menu={items.length ? <ComposerMenu id={menuId} items={items} index={active} onIndexChange={setIndex} /> : null}
      menuState={state}
      tray={
        <>
          {extraTray}
          <QueueTray queued={queued} onEdit={edit} onSteer={steer} onRemove={remove} />
        </>
      }
      onFiles={(picked) => {
        setError(null);
        for (const file of picked) void attachFile(scope, file).then((problem) => problem && setError(problem));
      }}
      attachments={draft.attachments}
      onRemoveAttachment={(id) => removeDraftAttachment(scope, id)}
      onKeyDown={onKeyDown}
    />
  );
}
