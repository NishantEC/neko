import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { accept, command, type Json } from "./api";
import { clock, duration, sentence, steps, type Step } from "./chatWork";
import { Decisions, pendingDecisions } from "./Decisions";
import { ArrowUp, Chevron, Copy, Square, Terminal, Tools, X, Pencil, Steer } from "./icons";
import { Markdown } from "./Markdown";
import { useWorkbench } from "./useStore";

/** Seconds since `since`, re-read every second while `active`. */
function useElapsed(since: number, active: boolean) {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (!active) return;
    setNow(Date.now());
    const timer = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(timer);
  }, [active]);
  return Math.max(0, (now - since) / 1000);
}

/** The newest step is shown with its verb shimmering: "Ran `git status`". */
function LiveStep({ text }: { text: string }) {
  const [verb, ...rest] = text.split(" ");
  return (
    <div className="work-row live">
      <span className="work-icon"><Terminal /></span>
      <span className="work-text">
        <span className="shimmer">{verb}</span> {rest.join(" ")}
      </span>
    </div>
  );
}

/** One live line while working; one summary line once finished. */
function WorkLog({ message }: { message: Json }) {
  const list: Step[] = useMemo(() => steps(message.activity), [message.activity]);
  const running = !!message.pending;
  const started = message.started_ms || message.at_ms;
  const elapsed = useElapsed(started, running);
  const [open, setOpen] = useState(false);
  const canExpand = list.length > 0;
  const summary = sentence(list);
  const worked = message.started_ms ? duration((message.at_ms - message.started_ms) / 1000) : null;

  return (
    <section className="work" data-running={running}>
      <button className="work-head" disabled={!canExpand || running} onClick={() => setOpen(!open)} aria-expanded={open}>
        <span>{running ? `Working for ${duration(elapsed)}` : worked ? `Worked for ${worked}` : "Worked"}</span>
        {canExpand && !running && <Chevron className="chev" data-open={open} />}
        <span className="grow" />
        {!running && <time>{clock(message.at_ms)}</time>}
      </button>
      {running ? (
        list.length > 0 && <LiveStep text={list[list.length - 1].text} />
      ) : (
        summary && (
          <div className="work-row">
            <span className="work-icon"><Tools /></span>
            <span className="work-text">{summary}</span>
          </div>
        )
      )}
      {open && !running && (
        <ul className="work-steps">
          {list.map((step, i) => (
            <li key={i} data-failed={step.failed}>
              <code>{step.text}</code>
              {step.failed && <span className="failed">failed</span>}
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}

/**
 * While a reply streams, only finished blocks are drawn: text up to the last
 * blank line, with an open code fence held back. A half-typed table or list
 * never reflows; the next paragraph arrives whole.
 */
export function finishedBlocks(text: string): string {
  const cut = text.lastIndexOf("\n\n");
  if (cut < 0) return "";
  let head = text.slice(0, cut);
  // A code fence still open inside the finished part is held back whole.
  const fence = /^ {0,3}(```|~~~)/gm;
  let open = -1;
  for (let m = fence.exec(head); m; m = fence.exec(head)) open = open < 0 ? m.index : -1;
  if (open >= 0) head = head.slice(0, open).trimEnd();
  return head;
}

function Actions({ actions }: { actions: Json[] }) {
  const settled = actions.filter((a) => a.status !== "Proposed");
  return (
    <>
      {settled.map((action) => (
        <div className="action-card" key={action.id}>
          <div className="action-head">
            <span>{action.kind === "message" ? "Tell its agent" : action.kind}</span>
            <span className="action-status" data-status={action.status}>{action.status}</span>
          </div>
          <p>{action.text}</p>
        </div>
      ))}
    </>
  );
}

function NekoTurn({ message }: { message: Json }) {
  const hasLog = message.pending || message.started_ms > 0 || (message.activity?.length ?? 0) > 0;
  const [copied, setCopied] = useState(false);
  const text = message.pending ? finishedBlocks(message.text) : message.text;
  return (
    <article className="turn neko" data-pending={!!message.pending}>
      {hasLog && <WorkLog message={message} />}
      {text && <Markdown text={text} />}
      {message.failed && <p className="notice">This reply did not finish. Try sending it again.</p>}
      {message.remembered?.length > 0 && (
        <p className="remembered">Remembered: {message.remembered.join(" · ")}</p>
      )}
      {message.actions?.length > 0 && <Actions actions={message.actions} />}
      {!message.pending && message.text && (
        <footer className="turn-foot">
          <button
            title="Copy"
            onClick={() => {
              navigator.clipboard?.writeText(message.text).catch(() => {});
              setCopied(true);
              setTimeout(() => setCopied(false), 1200);
            }}
          >
            <Copy />
          </button>
          <time>{copied ? "Copied" : clock(message.at_ms)}</time>
        </footer>
      )}
    </article>
  );
}

function UserTurn({ message }: { message: Json }) {
  return (
    <article className="turn user">
      <div className="bubble">{message.text}</div>
      {message.failed && <span className="bubble-note">Not sent</span>}
    </article>
  );
}

/** Messages sent while a reply was running, waiting their turn. */
function QueueTray({
  queued, onEdit, onSteer, onRemove,
}: { queued: Json[]; onEdit: (m: Json) => void; onSteer: (m: Json) => void; onRemove: (m: Json) => void }) {
  if (!queued.length) return null;
  return (
    <div className="tray" role="group" aria-label="Queued messages">
      <div className="tray-head"><span>Queued</span><span className="grow" /><span>{queued.length}</span></div>
      {queued.map((m) => (
        <div className="tray-row" key={m.id}>
          <span className="tray-text" title={m.text}>{m.text}</span>
          <button title="Edit (⌥↑)" aria-label="Edit queued message" onClick={() => onEdit(m)}><Pencil /></button>
          <button className="steer" title="Send now (⌘⇧↵)" onClick={() => onSteer(m)}><Steer /> Steer</button>
          <button title="Remove" aria-label="Remove queued message" onClick={() => onRemove(m)}><X /></button>
        </div>
      ))}
    </div>
  );
}

const LIMIT = 4096;

function Composer({
  pendingTurn, queued, decisionPending, workspaceId,
}: { pendingTurn: Json | undefined; queued: Json[]; decisionPending: boolean; workspaceId: string | null }) {
  const [text, setText] = useState("");
  const [sending, setSending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const area = useRef<HTMLTextAreaElement>(null);

  useLayoutEffect(() => {
    const el = area.current;
    if (!el) return;
    el.style.height = "0px";
    el.style.height = `${Math.min(el.scrollHeight, 220)}px`;
  }, [text]);

  const trimmed = text.trim();
  const tooLong = trimmed.length > LIMIT;

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
    if (!trimmed || sending || tooLong) return;
    setSending(true);
    // The same command the native app sends from Home; while a reply is
    // running the daemon queues it, and the tray above shows it waiting.
    if (await run(() => command("SendMessage", { text: trimmed, workspace_id: workspaceId }))) setText("");
    setSending(false);
  }

  const edit = async (m: Json) => {
    if (await run(() => command("DiscardQueuedMessage", { message_id: m.id }))) {
      setText((current) => (current ? `${m.text}\n${current}` : m.text));
      area.current?.focus();
    }
  };
  const steer = (m: Json) =>
    run(async () => {
      await command("DiscardQueuedMessage", { message_id: m.id });
      return command("InterruptAndSendMessage", { text: m.text, workspace_id: m.workspace_id ?? workspaceId });
    });
  const remove = (m: Json) => run(() => command("DiscardQueuedMessage", { message_id: m.id }));

  return (
    <div className="composer-wrap">
      <QueueTray queued={queued} onEdit={edit} onSteer={steer} onRemove={remove} />
      {error && <p className="composer-error">{error}</p>}
      <div className="composer" data-tray={queued.length > 0 || decisionPending}>
        <textarea
          ref={area}
          rows={1}
          value={text}
          placeholder={decisionPending ? "Queue a message while you decide…" : "Ask Neko anything · / for commands · @ for context"}
          aria-label="Message Neko"
          onChange={(e) => setText(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) {
              e.preventDefault();
              if (e.metaKey && queued.length && !trimmed) void steer(queued[queued.length - 1]);
              else if (!(e.metaKey || e.ctrlKey)) void send();
            } else if (e.key === "ArrowUp" && e.altKey && queued.length) {
              e.preventDefault();
              void edit(queued[queued.length - 1]);
            }
          }}
        />
        <div className="composer-bar">
          <span className="chip">Neko decides</span>
          {tooLong && <span className="count over">{trimmed.length}/{LIMIT}</span>}
          <span className="grow" />
          {pendingTurn && !trimmed ? (
            <button
              className="send stop"
              aria-label="Stop"
              title="Stop this reply"
              onClick={() => run(() => command("CancelChat", { turn_id: pendingTurn.id }))}
            >
              <Square />
            </button>
          ) : (
            <button className="send" aria-label={pendingTurn ? "Queue message" : "Send"} disabled={!trimmed || sending || tooLong} onClick={send}>
              <ArrowUp />
            </button>
          )}
        </div>
      </div>
    </div>
  );
}

export function Home() {
  const { snapshot, connected, error } = useWorkbench();
  const scroller = useRef<HTMLDivElement>(null);
  const stick = useRef(true);
  const [atBottom, setAtBottom] = useState(true);
  const all: Json[] = snapshot?.conversation ?? [];
  const messages = all.filter((m) => !(m.role === "user" && m.queued));
  const queued = all.filter((m) => m.role === "user" && m.queued);
  const pendingTurn = [...all].reverse().find((m) => m.pending);
  const decisions = useMemo(() => pendingDecisions(all, snapshot?.tasks ?? []), [all, snapshot?.tasks]);
  const workspaceId: string | null = null;

  // Follow the reply down only while the reader is at the bottom.
  const signature = messages.map((m) => `${m.id}:${m.text.length}:${m.activity?.length ?? 0}`).join("|");
  useLayoutEffect(() => {
    const el = scroller.current;
    if (el && stick.current) el.scrollTop = el.scrollHeight;
  }, [signature]);

  const toEnd = () => {
    const el = scroller.current;
    if (!el) return;
    stick.current = true;
    el.scrollTo({ top: el.scrollHeight, behavior: "smooth" });
    setAtBottom(true);
  };

  return (
    <div className="home">
      <div
        className="scroll"
        ref={scroller}
        onScroll={(e) => {
          const el = e.currentTarget;
          const near = el.scrollHeight - el.scrollTop - el.clientHeight < 80;
          stick.current = near;
          setAtBottom(near);
        }}
      >
        <div className="column">
          {!snapshot && <p className="muted center">{error ?? "Connecting to Neko…"}</p>}
          {snapshot && messages.length === 0 && (
            <p className="muted center">Ask Neko to look at your work, or to start something.</p>
          )}
          {messages.map((m) => (m.role === "user" ? <UserTurn key={m.id} message={m} /> : <NekoTurn key={m.id} message={m} />))}
          <div className="tail" />
        </div>
      </div>
      <div className="column dock">
        {!atBottom && (
          <button className="to-end" onClick={toEnd}><Chevron /> Scroll to end</button>
        )}
        {!connected && snapshot && <p className="composer-error">Can’t reach the Neko daemon. Showing the last known state.</p>}
        <Decisions items={decisions} />
        <Composer pendingTurn={pendingTurn} queued={queued} decisionPending={decisions.length > 0} workspaceId={workspaceId} />
      </div>
    </div>
  );
}
