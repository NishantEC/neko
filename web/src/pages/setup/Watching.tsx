// Watching: what Neko keeps an eye on. Each watch (a "responsibility") is an
// instruction plus the connected tools allowed to answer it; Neko checks them
// while it runs and brings what needs you to Home.
import { useEffect, useMemo, useState } from "react";
import { command, type Json } from "@/api";
import { Button } from "@/ui/button";
import { Textarea } from "@/ui/textarea";
import { Checkbox } from "@/ui/checkbox";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/ui/collapsible";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/ui/dialog";
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuSeparator, DropdownMenuTrigger } from "@/ui/dropdown-menu";
import { ArrowsClockwise, CaretDown, CaretRight, DotsThree, Eye, Lightbulb, PencilSimple, Plus, Sparkle, Trash, WarningCircle } from "@/ui/icons";
import { Panel } from "@/setup/kit";
import { navigate } from "@/router";
import { toast } from "@/ui/sonner";
import {
  ConfirmDialog, EmptyBlock, ErrorNote, FormField, Gate, PanelFooter, Pick, Section, SetupPage, Status, Toggle, registerCommands,
} from "@/setup/kit";
import { TextPromptDialog } from "@/setup/dialogs";
import { ScopePicker, setScope, useScope } from "@/setup/scope";
import { useSetupCommands, checkAllWatches } from "@/setup/commands";
import {
  WATCH_SUGGEST_PROMPT, arr, connectionLabel, mcp, relativeTime, str, suggestedWatchIds, ungranted, useRun, watchIdea, watchStatus,
  watchTitle, workspaceName, workspacesOf,
} from "@/setup/lib";

type Draft = { value: Json; workspace: string } | null;

function reviewAccess(item: Json) {
  setScope(str(item.workspace_id));
  navigate("tools");
}

/** Turning a watch on is a decision: it allows unattended checks. */
function ActivationDialog({ snapshot, item, open, onOpenChange, onConfirm }: { snapshot: Json; item: Json; open: boolean; onOpenChange: (o: boolean) => void; onConfirm: () => void }) {
  const sources = arr(item.connection_ids).map((id) => connectionLabel(snapshot, id)).join(", ");
  return (
    <ConfirmDialog
      open={open}
      onOpenChange={onOpenChange}
      destructive={false}
      title="Turn on this watch?"
      description={
        <>
          <span className="block">{str(item.instruction)}</span>
          <span className="mt-2 block">
            Allows unattended checks every 10 minutes using {sources || "the selected connections"} in {workspaceName(snapshot, item.workspace_id, "this workspace")}. Connected tools may read or change data.
          </span>
        </>
      }
      confirmLabel="Turn on watch"
      onConfirm={onConfirm}
    />
  );
}

function useWatchActions(snapshot: Json, item: Json) {
  const { run, busy, error, clear, lastError } = useRun();
  const turnOn = async () => {
    const saved = await run(() => mcp("SaveResponsibility", { responsibility: { ...item, enabled: true } }));
    if (saved) await run(() => mcp("Wake", { responsibility_id: item.id }));
  };
  const turnOff = () => run(() => mcp("SaveResponsibility", { responsibility: { ...item, enabled: false } }));
  const wake = () => run(() => mcp("Wake", { responsibility_id: item.id }));
  const remove = () => run(() => mcp("RemoveResponsibility", { responsibility_id: item.id }));
  const rename = (name: string) => run(() => mcp("RenameResponsibility", { responsibility_id: item.id, name }));
  const missing = ungranted(snapshot, item);
  return { run, busy, error, clear, lastError, turnOn, turnOff, wake, remove, rename, missing };
}

function WatchRow({ snapshot, item, onEdit }: { snapshot: Json; item: Json; onEdit: () => void }) {
  const a = useWatchActions(snapshot, item);
  const [open, setOpen] = useState(false);
  const [confirmOn, setConfirmOn] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [renaming, setRenaming] = useState(false);
  const title = watchTitle(item);
  const status = watchStatus(item);
  const sources = arr(item.connection_ids).map((id) => connectionLabel(snapshot, id));
  const planFirst = !!snapshot.plan_first;

  const toggle = (on: boolean) => {
    if (!on) return void a.turnOff();
    if (a.missing.length === 0) setConfirmOn(true);
    else reviewAccess(item);
  };

  return (
    <div data-slot="watch" data-enabled={item.enabled ? "true" : "false"}>
      <Collapsible open={open} onOpenChange={setOpen}>
        <div className="flex items-start gap-3 px-6 py-4">
          <CollapsibleTrigger
            render={<button type="button" aria-label={`${open ? "Hide" : "Show"} details for ${title}`} className="-m-1.5 flex min-w-0 flex-1 items-start gap-2.5 rounded-lg p-1.5 text-start outline-none transition-colors hover:bg-foreground/[0.04] focus-visible:ring-2 focus-visible:ring-ring" />}
          >
            <span aria-hidden className="mt-0.5 text-muted-foreground">{open ? <CaretDown /> : <CaretRight />}</span>
            <span className="flex min-w-0 flex-1 flex-col gap-1">
              <span className="text-[14px] font-medium leading-snug text-foreground line-clamp-2">{title}</span>
              {title !== str(item.instruction) ? <span className="line-clamp-1 text-[13px] text-muted-foreground">{str(item.instruction)}</span> : null}
              <span className="flex flex-wrap items-center gap-2 pt-0.5">
                <Status tone={status.tone}>{status.text}</Status>
                <span className="text-xs text-muted-foreground">{workspaceName(snapshot, item.workspace_id)}</span>
              </span>
            </span>
          </CollapsibleTrigger>
          <div className="flex shrink-0 items-center gap-2 pt-0.5">
            <Toggle checked={!!item.enabled} onChange={toggle} label={`Watch ${title}`} disabled={a.busy} />
            <DropdownMenu>
              <DropdownMenuTrigger render={<Button variant="ghost" size="icon-sm" aria-label={`Actions for ${title}`} />}><DotsThree weight="bold" /></DropdownMenuTrigger>
              <DropdownMenuContent align="end">
                <DropdownMenuItem disabled={!item.enabled} onClick={() => void a.wake()}><ArrowsClockwise /> Check now</DropdownMenuItem>
                <DropdownMenuItem onClick={() => setRenaming(true)}><PencilSimple /> Rename…</DropdownMenuItem>
                <DropdownMenuItem onClick={onEdit}><PencilSimple /> Edit</DropdownMenuItem>
                <DropdownMenuSeparator />
                <DropdownMenuItem onClick={() => setConfirmDelete(true)} className="text-destructive-foreground"><Trash /> Delete watch…</DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
          </div>
        </div>
        <CollapsibleContent>
          <div className="flex flex-col gap-4 px-6 pb-5 ps-[3.5rem] text-[13px]">
            <p className="max-w-[70ch] whitespace-pre-wrap leading-relaxed text-foreground/85">{str(item.instruction)}</p>
            <dl className="grid grid-cols-[8rem_1fr] gap-x-4 gap-y-2 text-muted-foreground">
              <dt>Status</dt><dd className="text-foreground/85">{item.enabled ? "Watching" : "Paused"}</dd>
              <dt>Last checked</dt><dd className="text-foreground/85">{item.last_attempt_ms == null ? "Not checked yet" : relativeTime(item.last_attempt_ms)}</dd>
              <dt>Sources</dt><dd className="text-foreground/85">{sources.length ? sources.join(", ") : "No sources selected"}</dd>
              <dt>Delivery</dt>
              <dd className="text-foreground/85">
                {!planFirst ? "Tickets start working right away · reviewed results are delivered" : item.prepare_low_risk ? "Low-risk fixes allowed · reviewed results are delivered" : "Plans require approval before local work"}
              </dd>
            </dl>
            {(item.failures ?? 0) > 0 ? (
              <p className="flex items-center gap-2 text-warning-foreground"><WarningCircle /> {item.failures} consecutive failed {item.failures === 1 ? "check" : "checks"}</p>
            ) : null}
            {a.missing.length > 0 ? (
              <p className="flex items-center gap-2 text-warning-foreground"><WarningCircle /> Connect or discover tools for {a.missing.join(", ")} to turn this on. <Button variant="link" size="xs" onClick={() => reviewAccess(item)}>Review connections</Button></p>
            ) : null}
            {str(item.last_result) ? (
              <div className="flex flex-col gap-1.5">
                <span className="font-mono text-[11px] uppercase tracking-[0.14em] text-muted-foreground">Last check result</span>
                <pre className="max-h-64 overflow-auto whitespace-pre-wrap rounded-xl border border-border bg-foreground/[0.03] p-3 font-sans text-[13px] leading-relaxed text-foreground/85">{str(item.last_result)}</pre>
              </div>
            ) : null}
            <div className="flex gap-2">
              <Button variant="outline" size="sm" disabled={!item.enabled || a.busy} onClick={() => void a.wake()}><ArrowsClockwise /> Check now</Button>
              <Button variant="outline" size="sm" disabled={a.busy} onClick={onEdit}>Edit watch</Button>
            </div>
          </div>
        </CollapsibleContent>
      </Collapsible>
      {a.error ? <div className="px-6 pb-4"><ErrorNote error={a.error} onDismiss={a.clear} /></div> : null}
      <ActivationDialog snapshot={snapshot} item={item} open={confirmOn} onOpenChange={setConfirmOn} onConfirm={() => void a.turnOn()} />
      <ConfirmDialog
        open={confirmDelete}
        onOpenChange={setConfirmDelete}
        title={`Delete “${title}”?`}
        description="Neko stops checking it. Tasks it already filed stay."
        confirmLabel="Delete watch"
        onConfirm={() => void a.remove()}
      />
      <TextPromptDialog
        open={renaming}
        onOpenChange={setRenaming}
        title="Rename watch"
        description="An empty name clears it, so Neko names it again."
        label="Name"
        initial={str(item.name)}
        allowEmpty
        onSubmit={async (name) => ({ ok: await a.rename(name), error: a.lastError.current })}
      />
    </div>
  );
}

function SuggestionCard({ snapshot, item, onEdit }: { snapshot: Json; item: Json; onEdit: () => void }) {
  const a = useWatchActions(snapshot, item);
  const [confirmOn, setConfirmOn] = useState(false);
  const sources = arr(item.connection_ids).map((id) => connectionLabel(snapshot, id));
  const live = !!item.enabled;
  return (
    <div data-slot="suggestion" className="flex flex-col gap-3 px-6 py-4">
      <div className="flex flex-col gap-1">
        <p className="line-clamp-3 text-[14px] font-medium leading-snug">{str(item.instruction)}</p>
        <p className={`text-[13px] ${a.missing.length ? "text-warning-foreground" : "text-muted-foreground"}`}>
          {a.missing.length ? `Connection access needed for ${a.missing.join(", ")}` : "Ready to review"}
          {sources.length ? ` · Sources: ${sources.join(", ")}` : ""}
        </p>
      </div>
      <div className="flex flex-wrap items-center gap-2">
        {live ? (
          <Status tone="success">Watching</Status>
        ) : (
          <Button size="sm" variant="outline" disabled={a.busy} onClick={() => (a.missing.length ? reviewAccess(item) : setConfirmOn(true))}>
            {a.missing.length ? "Review connections" : "Turn on…"}
          </Button>
        )}
        <Button size="sm" variant="ghost" onClick={onEdit}>Edit</Button>
        <span className="flex-1" />
        {!live ? <Button size="sm" variant="ghost" disabled={a.busy} onClick={() => void a.remove()}>Dismiss</Button> : null}
      </div>
      {a.error ? <ErrorNote error={a.error} onDismiss={a.clear} /> : null}
      <ActivationDialog snapshot={snapshot} item={item} open={confirmOn} onOpenChange={setConfirmOn} onConfirm={() => void a.turnOn()} />
    </div>
  );
}

function WatchEditor({ snapshot, draft, onClose }: { snapshot: Json; draft: Draft; onClose: () => void }) {
  const { run, busy, error, clear } = useRun();
  const original = draft?.value ?? {};
  const isNew = !str(original.id);
  const [instruction, setInstruction] = useState("");
  const [connections, setConnections] = useState<string[]>([]);
  const [enabled, setEnabled] = useState(false);
  const [prepare, setPrepare] = useState(false);
  const workspace = isNew ? draft?.workspace ?? "" : str(original.workspace_id);
  useEffect(() => {
    if (!draft) return;
    clear();
    setInstruction(str(original.instruction));
    setConnections(arr(original.connection_ids));
    setEnabled(!!original.enabled);
    setPrepare(!!original.prepare_low_risk);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [draft]);
  const available = arr(snapshot.mcp?.connections).filter((c) => !str(c.workspace_id) || c.workspace_id === workspace);
  const missing = enabled ? ungranted(snapshot, { workspace_id: workspace, connection_ids: connections }) : [];
  const canSave = instruction.trim().length > 0 && connections.length > 0 && !busy && !(enabled && missing.length);
  const save = async () => {
    const saved = await run(() =>
      mcp("SaveResponsibility", {
        responsibility: {
          ...original,
          id: str(original.id),
          workspace_id: workspace,
          instruction: instruction.trim(),
          connection_ids: [...connections].sort(),
          enabled,
          prepare_low_risk: prepare,
          next_due_ms: isNew ? Date.now() : original.next_due_ms,
          last_attempt_ms: original.last_attempt_ms ?? null,
          last_result: str(original.last_result),
          failures: original.failures ?? 0,
        },
      }),
    );
    if (saved) onClose();
  };
  return (
    <Dialog open={!!draft} onOpenChange={(o) => { if (!o) onClose(); }}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>{isNew ? "New watch" : "Edit watch"}</DialogTitle>
          <DialogDescription>Choose what to watch, its sources, and the actions you allow.</DialogDescription>
        </DialogHeader>
        <form className="flex max-h-[70vh] flex-col gap-5 overflow-y-auto px-0.5" onSubmit={(e) => { e.preventDefault(); if (canSave) void save(); }}>
          <FormField label="What to watch" htmlFor="watch-instruction">
            <Textarea id="watch-instruction" rows={4} value={instruction} onChange={(e) => setInstruction(e.target.value)} placeholder="New Linear issues assigned to me" />
          </FormField>
          <FormField label="Connected tools" hint={available.length === 0 ? "No tools are connected to this workspace yet. Add one in Tools & skills." : undefined}>
            <ul className="flex flex-col gap-1.5">
              {available.map((c) => (
                <li key={c.id}>
                  <label className="flex cursor-pointer items-center gap-2.5 rounded-lg px-1.5 py-1 text-[13px] hover:bg-foreground/[0.04]">
                    <Checkbox
                      checked={connections.includes(c.id)}
                      onCheckedChange={(on) => setConnections(on ? [...connections, c.id] : connections.filter((id) => id !== c.id))}
                      aria-label={c.label}
                    />
                    <span>{c.label}</span>
                  </label>
                </li>
              ))}
            </ul>
          </FormField>
          <div className="flex items-center justify-between gap-4 rounded-xl border border-border bg-foreground/[0.03] px-4 py-3">
            <div>
              <p className="text-[13px] font-medium">Enabled</p>
              <p className="text-xs text-muted-foreground">{snapshot.plan_first ? "Remote tool permission and publication authority are separate." : "Tickets from this watch start working right away; the agent stops for your approval only on critical work."}</p>
            </div>
            <Toggle checked={enabled} onChange={setEnabled} label="Enabled" />
          </div>
          {enabled && missing.length ? (
            <p className="flex items-center gap-2 text-[13px] text-warning-foreground"><WarningCircle /> Choose access for {missing.join(", ")} in Tools &amp; skills before turning this on.</p>
          ) : null}
          {snapshot.plan_first ? (
            <label className="flex items-center gap-2.5 text-[13px]">
              <Checkbox checked={prepare} onCheckedChange={(v) => setPrepare(!!v)} aria-label="Allow preparation of low-risk local fixes" />
              Allow preparation of low-risk local fixes
            </label>
          ) : null}
          <ErrorNote error={error} title="Couldn’t save the watch" />
          <DialogFooter>
            <Button type="button" variant="outline" onClick={onClose}>Cancel</Button>
            <Button type="submit" disabled={!canSave} loading={busy}>Save</Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}

function NewWatchPanel({ snapshot, workspaceId }: { snapshot: Json; workspaceId: string }) {
  const { run, busy, error, clear } = useRun();
  const [ask, setAsk] = useState("");
  const [target, setTarget] = useState<string>("");
  const [askedAt, setAskedAt] = useState<number | null>(null);
  const workspaces = workspacesOf(snapshot);
  const wsId = workspaceId || target || workspaces[0]?.id || "";
  const tools = arr(snapshot.mcp?.connections).filter((c) => c.enabled && (!str(c.workspace_id) || !wsId || c.workspace_id === wsId));
  const reply = askedAt
    ? [...arr(snapshot.conversation)].reverse().find((m) => m.role === "neko" && (m.at_ms ?? 0) >= askedAt - 2000 && str(m.workspace_id) === wsId)
    : null;

  const send = async (text: string) => {
    const clean = text.trim();
    if (!clean || !wsId) return;
    const message = clean === WATCH_SUGGEST_PROMPT ? clean : `Keep an eye on this for me: ${clean}`;
    setAskedAt(Date.now());
    const sent = await run(() => command("SendMessage", { text: message, workspace_id: wsId }));
    if (sent) { if (clean !== WATCH_SUGGEST_PROMPT) setAsk(""); } else setAskedAt(null);
  };

  return (
    <Panel data-slot="new-watch">
      <div className="flex flex-col gap-3 px-6 py-4">
        <div className="flex items-center justify-between gap-3">
          <label htmlFor="watch-ask" className="font-mono text-[11px] uppercase tracking-[0.14em] text-muted-foreground">What should Neko keep an eye on?</label>
          {tools.length > 0 ? (
            <DropdownMenu>
              <DropdownMenuTrigger render={<Button variant="ghost" size="xs" />}><Lightbulb /> Use an idea</DropdownMenuTrigger>
              <DropdownMenuContent align="end">
                {tools.map((t) => (
                  <DropdownMenuItem key={t.id} onClick={() => setAsk(watchIdea(t.label))}>{watchIdea(t.label)}</DropdownMenuItem>
                ))}
              </DropdownMenuContent>
            </DropdownMenu>
          ) : null}
        </div>
        <Textarea
          id="watch-ask"
          rows={3}
          value={ask}
          onChange={(e) => setAsk(e.target.value)}
          onKeyDown={(e) => { if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) { e.preventDefault(); void send(ask); } }}
          placeholder="New Linear issues assigned to me"
        />
        <div className="flex flex-wrap items-center justify-between gap-3">
          {!workspaceId && workspaces.length > 1 ? (
            <Pick label="Workspace for the new watch" value={wsId} onChange={setTarget} options={workspaces.map((w) => ({ value: w.id, label: w.name }))} />
          ) : (
            <span className="text-xs text-muted-foreground">{workspaceName(snapshot, wsId, "Workspace")}</span>
          )}
          <div className="flex items-center gap-2">
            <Button variant="outline" size="sm" disabled={busy || tools.length === 0} onClick={() => void send(WATCH_SUGGEST_PROMPT)}><Sparkle /> Suggest from tools</Button>
            <Button size="sm" disabled={busy || !ask.trim()} loading={busy} onClick={() => void send(ask)}>Set up watch</Button>
          </div>
        </div>
        <p className="text-xs text-muted-foreground">
          {tools.length === 0 ? "Connect a source in Tools & skills to start." : `${tools.length} connected ${tools.length === 1 ? "source" : "sources"} available. Review access before turning on a suggestion.`}
        </p>
        <ErrorNote error={error} title="Couldn’t ask Neko" onDismiss={clear} />
      </div>
      {reply ? (
        <div className="border-t border-border/70 px-6 py-4">
          <div className="mb-1.5 flex items-center justify-between">
            <span className="text-[13px] font-medium">{reply.pending ? "Looking through your tools…" : "Neko’s reply"}</span>
            {!reply.pending ? <Button variant="ghost" size="xs" onClick={() => setAskedAt(null)}>Hide reply</Button> : null}
          </div>
          {!reply.pending ? <p className="max-h-40 overflow-auto whitespace-pre-wrap text-[13px] leading-relaxed text-muted-foreground">{str(reply.text)}</p> : null}
        </div>
      ) : null}
    </Panel>
  );
}

function WatchingBody({ snapshot }: { snapshot: Json }) {
  const scope = useScope(snapshot);
  const [draft, setDraft] = useState<Draft>(null);
  const suggestedIds = useMemo(() => suggestedWatchIds(snapshot), [snapshot]);
  const all: Json[] = arr(snapshot.mcp?.responsibilities).filter((r) => !scope || r.workspace_id === scope);
  const active = all.filter((r) => r.enabled);
  const suggested = all.filter((r) => suggestedIds.has(r.id));
  const paused = all.filter((r) => !r.enabled && !suggestedIds.has(r.id));
  const [showPaused, setShowPaused] = useState(false);
  const workspaces = workspacesOf(snapshot);

  if (workspaces.length === 0) {
    return <EmptyBlock icon={Eye} title="Add a workspace to create a watch" description="Watches belong to a workspace, because they use that workspace’s connected tools.">
      <Button variant="outline" onClick={() => navigate("workspaces")}>Open Workspaces</Button>
    </EmptyBlock>;
  }
  const openEditor = () => setDraft({ value: {}, workspace: scope || workspaces[0].id });

  return (
    <>
      <div className="flex flex-wrap items-center gap-2">
        <ScopePicker snapshot={snapshot} label="Show watches for" />
        <span className="flex-1" />
        <Button variant="outline" size="sm" onClick={openEditor}><Plus /> Write it yourself…</Button>
        <Button variant="outline" size="sm" disabled={active.length === 0} onClick={() => void checkAllWatches().then((n) => toast.success(`Asked ${n} ${n === 1 ? "watch" : "watches"} to check now.`))}>
          <ArrowsClockwise /> Check all now
        </Button>
      </div>
      <Section eyebrow="New" title={<>Set up a watch.</>} intro="Describe it in plain words, or ask Neko to suggest from your connected tools.">
        <NewWatchPanel snapshot={snapshot} workspaceId={scope} />
      </Section>
      {suggested.length > 0 ? (
        <Section eyebrow={`${suggested.length} to review`} title="Suggestions" intro="Neko proposed these in chat. Nothing runs until you turn it on.">
          <Panel><div className="divide-y divide-border/70">
            {suggested.map((item) => <SuggestionCard key={item.id} snapshot={snapshot} item={item} onEdit={() => setDraft({ value: item, workspace: str(item.workspace_id) })} />)}
          </div></Panel>
        </Section>
      ) : null}
      <Section eyebrow={`${active.length} active`} title="Active watches">
        {active.length === 0 ? (
          <EmptyBlock icon={Eye} title="No active watches" description={paused.length ? "Resume a paused watch or describe a new one above." : "Describe a watch above, or review a suggestion."} />
        ) : (
          <Panel><div className="divide-y divide-border/70">
            {active.map((item) => <WatchRow key={item.id} snapshot={snapshot} item={item} onEdit={() => setDraft({ value: item, workspace: str(item.workspace_id) })} />)}
          </div></Panel>
        )}
      </Section>
      {paused.length > 0 ? (
        <Section eyebrow={`${paused.length} paused`} title="Paused watches" actions={<Button variant="ghost" size="sm" aria-expanded={showPaused} onClick={() => setShowPaused(!showPaused)}>{showPaused ? "Hide" : "Show"}</Button>}>
          {showPaused ? (
            <Panel><div className="divide-y divide-border/70">
              {paused.map((item) => <WatchRow key={item.id} snapshot={snapshot} item={item} onEdit={() => setDraft({ value: item, workspace: str(item.workspace_id) })} />)}
            </div>
              <PanelFooter className="justify-start text-xs text-muted-foreground">Paused watches never check. Turn one on to confirm what it may do.</PanelFooter>
            </Panel>
          ) : null}
        </Section>
      ) : null}
      <WatchEditor snapshot={snapshot} draft={draft} onClose={() => setDraft(null)} />
    </>
  );
}

export default function Watching() {
  useSetupCommands();
  useEffect(
    () => registerCommands([{ id: "watching.suggest", title: "Suggest watches from my tools", keywords: ["ask", "neko"], icon: Sparkle, group: "Watching", run: () => document.getElementById("watch-ask")?.focus() }]),
    [],
  );
  return (
    <SetupPage
      eyebrow="Work"
      title={<>Watching</>}
      subtitle="Keep up with the sources that matter. Neko checks them while it’s running and brings what needs you to Home."
    >
      <Gate>{(snapshot) => <WatchingBody snapshot={snapshot} />}</Gate>
    </SetupPage>
  );
}
