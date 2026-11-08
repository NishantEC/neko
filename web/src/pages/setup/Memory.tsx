// Memory: what Neko has learned and been told to remember, by scope; the
// suggestions waiting for your yes or no; and "working style" — contextual
// guidance that shapes choices but never grants a permission.
import { useEffect, useMemo, useState } from "react";
import type { Json } from "@/api";
import { command } from "@/api";
import { Button } from "@/ui/button";
import { Input } from "@/ui/input";
import { Textarea } from "@/ui/textarea";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/ui/dialog";
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuSeparator, DropdownMenuTrigger } from "@/ui/dropdown-menu";
import { Tabs, TabsList, TabsPanel, TabsTab } from "@/ui/tabs";
import { Brain } from "@phosphor-icons/react";
import { Check, DotsThree, PencilSimple, Plus, Trash, X } from "@/ui/icons";
import { Panel, RowAction, RowActions } from "@/setup/kit";
import {
  ConfirmDialog, EmptyBlock, ErrorNote, FormField, Gate, Pick, Row, Section, SetupPage, Toggle, registerCommands,
} from "@/setup/kit";
import { GlyphButton } from "@/ui/status-glyph";
import { copyText, useContextMenu } from "@/shell/context-menu";
import { cn } from "@/ui/lib/utils";

/** A one-line row whose long text opens in full on click (DESIGN.md §8). */
function OneLine({ text, open, onToggle, className }: { text: string; open: boolean; onToggle: () => void; className?: string }) {
  return (
    <button
      type="button"
      aria-expanded={open}
      onClick={onToggle}
      title={open ? undefined : text}
      className={cn("min-w-0 flex-1 select-text rounded-md text-start text-[13.5px] text-foreground outline-none focus-visible:ring-2 focus-visible:ring-ring/60", open ? "whitespace-pre-wrap leading-relaxed" : "truncate", className)}
    >
      {text}
    </button>
  );
}

const ROW = "group/row flex flex-col gap-1 rounded-[10px] px-3 py-1.5 transition-colors hover:bg-foreground/[0.03]";
import { ScopePicker, useScope } from "@/setup/scope";
import { useSetupCommands } from "@/setup/commands";
import { arr, decisionCmd, profileFor, str, useRun, workspaceName, workspacesOf } from "@/setup/lib";

type MemoryDraft = { value: Json } | null;

const KINDS = [
  { value: "profile", label: "Profile", hint: "About you" },
  { value: "workspace", label: "Workspace", hint: "About one workspace" },
  { value: "decision", label: "Decision", hint: "Guides similar work" },
];

function MemoryEditor({ snapshot, draft, scope, profileId, onClose }: { snapshot: Json; draft: MemoryDraft; scope: string; profileId: string; onClose: () => void }) {
  const { run, busy, error, clear } = useRun();
  const original = draft?.value ?? {};
  const isNew = !str(original.id);
  const workspaces = workspacesOf(snapshot);
  const [kind, setKind] = useState("profile");
  const [text, setText] = useState("");
  const [workspace, setWorkspace] = useState("");
  useEffect(() => {
    if (!draft) return;
    clear();
    setKind(str(original.kind) || "profile");
    setText(str(original.text));
    setWorkspace(str(original.workspace_id) || scope || workspaces[0]?.id || "");
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [draft]);
  const needsWorkspace = kind === "workspace";
  const canSave = text.trim() && !(needsWorkspace && !workspace) && !busy;
  const save = async () => {
    const now = Date.now();
    const saved = await run(() =>
      command("SaveMemory", {
        entry: {
          ...original,
          id: str(original.id),
          agent_profile_id: isNew ? profileId : original.agent_profile_id,
          kind,
          workspace_id: kind === "profile" ? null : workspace || null,
          text: text.trim(),
          source: isNew ? "user" : original.source,
          created_at_ms: isNew ? now : original.created_at_ms,
          updated_at_ms: now,
        },
      }),
    );
    if (saved) onClose();
  };
  return (
    <Dialog open={!!draft} onOpenChange={(o) => { if (!o) onClose(); }}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>{isNew ? "Add memory" : "Edit memory"}</DialogTitle>
          <DialogDescription>Save context in your own words.</DialogDescription>
        </DialogHeader>
        <form className="flex flex-col gap-5" onSubmit={(e) => { e.preventDefault(); if (canSave) void save(); }}>
          <div className="grid gap-4 sm:grid-cols-2">
            <FormField label="Kind">
              <Pick label="Kind" value={kind} onChange={setKind} options={KINDS} disabled={!isNew} className="w-full" />
            </FormField>
            {kind !== "profile" ? (
              <FormField label="Workspace">
                <Pick label="Workspace" value={workspace} onChange={setWorkspace} options={workspaces.map((w) => ({ value: w.id, label: w.name }))} className="w-full" placeholder="Choose a workspace" />
              </FormField>
            ) : null}
          </div>
          <FormField label="What to remember" htmlFor="mem-text">
            <Textarea id="mem-text" rows={5} value={text} onChange={(e) => setText(e.target.value)} placeholder="I prefer small pull requests, with tests first." />
          </FormField>
          <ErrorNote error={error} title="Couldn’t save the memory" />
          <DialogFooter>
            <Button type="button" variant="outline" onClick={onClose}>Cancel</Button>
            <Button type="submit" disabled={!canSave} loading={busy}>Save</Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}

function sourceLabel(source: string): string {
  if (source === "user") return "Added by you";
  if (source === "chat" || source.startsWith("chat:")) return "From a Home conversation";
  if (source.startsWith("ticket:")) return "From an agent conversation";
  return "Source details";
}

function MemoryRow({ snapshot, entry, onEdit, kindLabel }: { snapshot: Json; entry: Json; onEdit: () => void; kindLabel: string }) {
  const { run, busy, error, clear } = useRun();
  const [confirm, setConfirm] = useState(false);
  const [open, setOpen] = useState(false);
  const source = str(entry.source);
  const menu = useContextMenu(() => [
    { label: "Edit…", icon: PencilSimple, run: onEdit },
    { label: "Copy", run: () => void copyText(str(entry.text)) },
    source ? { label: "Copy source", run: () => void copyText(source, "Copied the source") } : null,
    "separator",
    { label: "Forget memory…", icon: Trash, run: () => setConfirm(true) },
  ]);
  return (
    <div data-slot="memory" {...menu} className={ROW}>
      <div className="flex min-h-8 items-center gap-3">
        <OneLine text={str(entry.text)} open={open} onToggle={() => setOpen(!open)} />
        <span className="shrink-0 text-[12.5px] text-muted-foreground/70" title={source || undefined}>{kindLabel}{source ? ` · ${sourceLabel(source)}` : ""}</span>
        <RowActions>
          <RowAction label="Edit memory" icon={PencilSimple} onClick={onEdit} />
          <DropdownMenu>
            <DropdownMenuTrigger render={<Button variant="ghost" size="icon-sm" aria-label="Memory actions" title="More actions" />}><DotsThree weight="bold" /></DropdownMenuTrigger>
            <DropdownMenuContent align="end">
              <DropdownMenuItem onClick={onEdit}><PencilSimple /> Edit</DropdownMenuItem>
              <DropdownMenuSeparator />
              <DropdownMenuItem className="text-destructive-foreground" onClick={() => setConfirm(true)}><Trash /> Forget memory…</DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        </RowActions>
      </div>
      {open && source ? <p className="select-text font-mono text-[11px] text-muted-foreground">{source} · {entry.workspace_id ? workspaceName(snapshot, entry.workspace_id, "Workspace") : "Everywhere"}</p> : null}
      <ErrorNote error={error} onDismiss={clear} />
      <ConfirmDialog
        open={confirm}
        onOpenChange={setConfirm}
        title="Forget this memory?"
        description="Agents stop using it. This can’t be undone."
        confirmLabel="Forget memory"
        onConfirm={() => void run(() => command("DeleteMemory", { id: entry.id }))}
      />
      {busy ? <span className="sr-only">Working…</span> : null}
    </div>
  );
}

function Proposals({ snapshot, proposals }: { snapshot: Json; proposals: Json[] }) {
  const { run, busy, error, clear } = useRun();
  return (
    <Section title="Suggested" count={proposals.length}>
      <Panel>
        {proposals.map((p) => <Proposal key={str(p.id)} snapshot={snapshot} p={p} busy={busy} decide={(accept) => void run(() => command("DecideMemoryProposal", { id: p.id, accept }))} />)}
        {error ? <div className="px-3 pt-2"><ErrorNote error={error} onDismiss={clear} /></div> : null}
      </Panel>
    </Section>
  );
}

/** A suggestion Neko noticed: nothing is remembered until you keep it (the glyph keeps it). */
function Proposal({ snapshot, p, busy, decide }: { snapshot: Json; p: Json; busy: boolean; decide: (accept: boolean) => void }) {
  const [open, setOpen] = useState(false);
  const menu = useContextMenu(() => [
    { label: "Keep memory", icon: Check, run: () => decide(true) },
    { label: "Dismiss", icon: X, run: () => decide(false) },
    "separator",
    { label: "Copy", run: () => void copyText(str(p.text)) },
  ]);
  return (
    <div data-slot="memory-proposal" {...menu} className={ROW}>
      <div className="flex min-h-8 items-center gap-3">
        <GlyphButton status="todo" disabled={busy} label="Keep memory" onClick={() => decide(true)} />
        <OneLine text={str(p.text)} open={open} onToggle={() => setOpen(!open)} />
        <span className="shrink-0 text-[12.5px] text-muted-foreground/70">{p.workspace_id ? workspaceName(snapshot, p.workspace_id, "Workspace") : "Everywhere"} · {sourceLabel(str(p.source))}</span>
        <RowActions>
          <RowAction label="Keep memory" icon={Check} disabled={busy} onClick={() => decide(true)} />
          <RowAction label="Dismiss" icon={X} disabled={busy} onClick={() => decide(false)} />
        </RowActions>
      </div>
    </div>
  );
}

function MemoryOptions({ snapshot }: { snapshot: Json }) {
  const { run, busy, error, clear } = useRun();
  const options = snapshot.memory_options ?? {};
  const learning = options.learning ?? true;
  const useMemory = options.use_memory ?? true;
  const set = (patch: Record<string, boolean>) => void run(() => command("SetMemoryOptions", { options: { learning, use_memory: useMemory, ...patch } }));
  return (
    <Section title="Settings">
      <Panel>
        <Row title={<span title="Propose context from chats and agents. Direct requests to remember something still work when this is off.">Suggest new memories</span>} description="from chats and agents" trailing={<Toggle checked={learning} onChange={(v) => set({ learning: v })} label="Suggest new memories" disabled={busy} />} />
        <Row title={<span title="Include memory in replies and tasks. Turning this off keeps saved memories here.">Use saved memory</span>} description="in replies and tasks" trailing={<Toggle checked={useMemory} onChange={(v) => set({ use_memory: v })} label="Use saved memory" disabled={busy} />} />
        {error ? <div className="px-3 pt-2"><ErrorNote error={error} onDismiss={clear} /></div> : null}
      </Panel>
    </Section>
  );
}

// ---------------------------------------------------------------------------
// Working style
// ---------------------------------------------------------------------------

const DECISION_TITLES: Record<string, string> = {
  prepare_fix: "Prepare a local fix", ask_user: "Ask for your decision", skip: "Defer this work",
  approve_local_build: "You approved a local build", accept_local_result: "You accepted the local result",
  cancel_task: "You stopped this task", report_failure: "Work stopped", review_local_result: "Local work is ready to review",
  add_note: "Your direction was recorded",
};
const decisionTitle = (action: string) => DECISION_TITLES[action] ?? "Prepare a plan";

function latestDecisions(records: Json[]): Json[] {
  const episodes = new Map<string, Json>();
  for (const r of records) {
    const prior = episodes.get(r.episode_id);
    if (!prior || r.version > prior.version) episodes.set(r.episode_id, r);
  }
  return [...episodes.values()].sort((a, b) => b.created_at_ms - a.created_at_ms);
}

const phrases = (text: string) => text.split(",").map((p) => p.trim()).filter(Boolean);

function PreferenceEditor({ draft, workspaceId, onClose }: { draft: MemoryDraft; workspaceId: string; onClose: () => void }) {
  const { run, busy, error, clear } = useRun();
  const original = draft?.value ?? {};
  const [instruction, setInstruction] = useState("");
  const [terms, setTerms] = useState("");
  const [exceptions, setExceptions] = useState("");
  useEffect(() => {
    if (!draft) return;
    clear();
    setInstruction(str(original.instruction));
    setTerms(arr(original.applicability?.terms).join(", "));
    setExceptions(arr(original.exceptions).join(", "));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [draft]);
  const hasTasks = arr(original.applicability?.task_ids).length > 0;
  const canSave = instruction.trim() && (phrases(terms).length > 0 || hasTasks) && !busy;
  const save = async () => {
    const saved = await run(() =>
      decisionCmd("SavePreference", {
        workspace_id: str(original.workspace_id) || workspaceId,
        id: str(original.id),
        expected_version: original.version ?? null,
        applicability: { terms: phrases(terms), task_ids: arr(original.applicability?.task_ids) },
        instruction: instruction.trim(),
        supporting_record_ids: arr(original.supporting_record_ids),
        exceptions: phrases(exceptions),
      }),
    );
    if (saved) onClose();
  };
  return (
    <Dialog open={!!draft} onOpenChange={(o) => { if (!o) onClose(); }}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>{str(original.id) ? "Edit working guidance" : "Add working guidance"}</DialogTitle>
          <DialogDescription>Describe a recurring situation and how you want Neko to approach it. Saved guidance returns to Suggested until you keep it; it does not grant new permissions.</DialogDescription>
        </DialogHeader>
        <form className="flex flex-col gap-5" onSubmit={(e) => { e.preventDefault(); if (canSave) void save(); }}>
          <FormField label="Instruction" htmlFor="pref-instruction">
            <Textarea id="pref-instruction" rows={3} value={instruction} onChange={(e) => setInstruction(e.target.value)} placeholder="Prefer the smallest fix that makes the failing test pass." />
          </FormField>
          <FormField label="Words or phrases" htmlFor="pref-terms" hint="Separate phrases with commas. Guidance matches complete words in a task.">
            <Input id="pref-terms" value={terms} onChange={(e) => setTerms(e.target.value)} placeholder="flaky test, login" />
          </FormField>
          <FormField label="Exceptions" htmlFor="pref-exceptions" hint="Exceptions narrow when guidance applies.">
            <Input id="pref-exceptions" value={exceptions} onChange={(e) => setExceptions(e.target.value)} />
          </FormField>
          <ErrorNote error={error} title="Couldn’t save the guidance" />
          <DialogFooter>
            <Button type="button" variant="outline" onClick={onClose}>Cancel</Button>
            <Button type="submit" disabled={!canSave} loading={busy}>Save suggestion</Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}

function PreferenceRow({ snapshot, item, onEdit }: { snapshot: Json; item: Json; onEdit: () => void }) {
  const { run, busy, error, clear } = useRun();
  const [forget, setForget] = useState(false);
  const fields = { workspace_id: item.workspace_id, id: item.id, expected_version: item.version };
  const applies = [...arr(item.applicability?.terms), ...arr(item.applicability?.task_ids).map((t) => `NEK-${str(t).slice(0, 4).toUpperCase()}`)].join(", ");
  const [open, setOpen] = useState(false);
  const keep = () => void run(() => decisionCmd("KeepPreference", fields));
  const dismiss = () => void run(() => decisionCmd("DismissPreference", fields));
  const menu = useContextMenu(() => [
    item.state !== "confirmed" ? { label: "Keep guidance", icon: Check, run: keep } : null,
    { label: "Edit…", icon: PencilSimple, run: onEdit },
    item.state !== "dismissed" ? { label: "Dismiss", icon: X, run: dismiss } : null,
    "separator",
    { label: "Forget…", icon: Trash, run: () => setForget(true) },
  ]);
  return (
    <div data-slot="preference" {...menu} className={ROW}>
      <div className="flex min-h-8 items-center gap-3">
        {item.state !== "confirmed" ? <GlyphButton status="todo" disabled={busy} label="Keep guidance" onClick={keep} /> : null}
        <OneLine text={str(item.instruction)} open={open} onToggle={() => setOpen(!open)} />
        <span className="max-w-[35%] shrink-0 truncate text-[12.5px] text-muted-foreground/70" title={`Applies to: ${applies || "—"}`}>{applies || "—"}</span>
        <RowActions>
          {item.state !== "confirmed" ? <RowAction label="Keep guidance" icon={Check} disabled={busy} onClick={keep} /> : null}
          <RowAction label="Edit guidance" icon={PencilSimple} onClick={onEdit} />
          {item.state !== "dismissed" ? <RowAction label="Dismiss guidance" icon={X} disabled={busy} onClick={dismiss} /> : null}
          <RowAction label="Forget guidance…" icon={Trash} onClick={() => setForget(true)} />
        </RowActions>
      </div>
      {open ? (
        <p className="text-[12.5px] text-muted-foreground">
          Applies to: {applies || "—"}{arr(item.exceptions).length ? ` · Except: ${arr(item.exceptions).join(", ")}` : ""} · {workspaceName(snapshot, item.workspace_id, "Workspace")} · version {item.version}
        </p>
      ) : null}
      <ErrorNote error={error} onDismiss={clear} />
      <ConfirmDialog open={forget} onOpenChange={setForget} title="Forget this guidance?" description="Neko stops applying it. This can’t be undone." confirmLabel="Forget guidance" onConfirm={() => void run(() => decisionCmd("ForgetPreference", fields))} />
    </div>
  );
}

function DecisionCard({ record }: { record: Json }) {
  const { run, busy, error, clear } = useRun();
  const [open, setOpen] = useState(false);
  const [text, setText] = useState("");
  return (
    <div data-slot="decision" className={ROW}>
      <div className="flex min-h-8 items-center gap-3">
        <span className="shrink-0 text-[13.5px] text-foreground">{decisionTitle(record.action)}</span>
        <span className="min-w-0 flex-1 truncate text-[12.5px] text-muted-foreground" title={str(record.observed_outcome) || undefined}>
          {str(record.correction?.text) ? `Your correction: ${str(record.correction.text)}` : str(record.observed_outcome) || "Outcome not yet observed"}
        </span>
        {record.delivery_stage === "local_accepted" ? <span className="shrink-0 text-[12.5px] text-muted-foreground/70">Accepted locally</span> : null}
        <RowActions>
          <RowAction label="Correct this decision" icon={PencilSimple} onClick={() => setOpen(true)} />
        </RowActions>
      </div>
      <Dialog open={open} onOpenChange={setOpen}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>Correct this decision</DialogTitle>
            <DialogDescription>The original observation stays in history. This correction applies to this decision; broader guidance needs your confirmation in Working style.</DialogDescription>
          </DialogHeader>
          <FormField label="What should Neko do differently?" htmlFor="correction"><Textarea id="correction" rows={4} value={text} onChange={(e) => setText(e.target.value)} /></FormField>
          <ErrorNote error={error} />
          <DialogFooter>
            <Button variant="outline" onClick={() => setOpen(false)}>Cancel</Button>
            <Button disabled={!text.trim() || busy} loading={busy} onClick={async () => {
              const ok = await run(() => decisionCmd("CorrectDecision", { workspace_id: record.workspace_id, record_id: record.id, expected_version: record.version, correction: text.trim() }));
              if (ok) { setOpen(false); setText(""); }
            }}>Save correction</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
      <ErrorNote error={error && !open ? error : null} onDismiss={clear} />
    </div>
  );
}

function WorkingStyle({ snapshot, scope, profileId }: { snapshot: Json; scope: string; profileId: string }) {
  const [editor, setEditor] = useState<MemoryDraft>(null);
  const workspaces = workspacesOf(snapshot);
  const target = scope || workspaces[0]?.id || "";
  const prefs: Json[] = arr(snapshot.working_preferences).filter((p) => (!scope || p.workspace_id === scope) && p.agent_profile_id === profileId);
  const records = latestDecisions(arr(snapshot.decision_records).filter((r) => !scope || r.workspace_id === scope)).slice(0, 12);
  const group = (state: string) => prefs.filter((p) => p.state === state);
  const sections: [string, string, string][] = [["proposed", "Needs your confirmation", "warning"], ["confirmed", "Confirmed", "neutral"], ["dismissed", "Dismissed", "neutral"]];
  return (
    <div className="flex flex-col gap-8">
      <div className="flex items-center justify-between gap-3">
        <p className="text-[13.5px] text-muted-foreground">{prefs.length === 0 ? "No guidance yet. Add some for a recurring situation, or correct a decision below." : "Guidance shapes choices within the permissions you already gave."}</p>
        <Button variant="ghost" size="sm" disabled={!target} onClick={() => setEditor({ value: { workspace_id: target } })}><Plus /> Add guidance</Button>
      </div>
      {sections.map(([state, title]) => {
        const items = group(state);
        if (!items.length) return null;
        return (
          <Section key={state} title={title} count={items.length} collapsible defaultOpen={state !== "dismissed"}>
            <Panel>{items.map((p) => <PreferenceRow key={str(p.id)} snapshot={snapshot} item={p} onEdit={() => setEditor({ value: p })} />)}</Panel>
          </Section>
        );
      })}
      {records.length ? (
        <Section title="Recent decisions" count={records.length}>
          <Panel>{records.map((r) => <DecisionCard key={str(r.id)} record={r} />)}</Panel>
        </Section>
      ) : null}
      <PreferenceEditor draft={editor} workspaceId={target} onClose={() => setEditor(null)} />
    </div>
  );
}

function MemoryBody({ snapshot }: { snapshot: Json }) {
  const scope = useScope(snapshot);
  const [tab, setTab] = useState("memories");
  const [draft, setDraft] = useState<MemoryDraft>(null);
  const profileId = profileFor(snapshot, scope || null);
  const inScope = (item: Json) => item.agent_profile_id === profileId && (item.workspace_id == null || !scope || item.workspace_id === scope);
  const memories = useMemo(() => arr(snapshot.memory).filter(inScope), [snapshot, scope, profileId]); // eslint-disable-line react-hooks/exhaustive-deps
  const proposals = useMemo(() => arr(snapshot.memory_proposals).filter(inScope), [snapshot, scope, profileId]); // eslint-disable-line react-hooks/exhaustive-deps
  const scopes = useMemo(() => {
    const order = ["Everywhere", ...workspacesOf(snapshot).map((w) => w.name)];
    const byScope = new Map<string, Json[]>();
    for (const m of memories) {
      const key = m.workspace_id == null ? "Everywhere" : workspaceName(snapshot, m.workspace_id, "Workspace");
      byScope.set(key, [...(byScope.get(key) ?? []), m]);
    }
    return [...byScope].sort((a, b) => (order.indexOf(a[0]) + 1 || 99) - (order.indexOf(b[0]) + 1 || 99));
  }, [memories, snapshot]);

  useEffect(
    () => registerCommands([{ id: "memory.add", title: "Add a memory", keywords: ["remember", "note"], icon: Plus, group: "Memory", run: () => { setTab("memories"); setDraft({ value: {} }); } }]),
    [],
  );

  return (
    <>
      <Tabs value={tab} onValueChange={(v) => setTab(String(v))}>
        <div className="flex flex-wrap items-center justify-between gap-3">
          <TabsList aria-label="Memory view">
            <TabsTab value="memories">Saved memories</TabsTab>
            <TabsTab value="style">Working style</TabsTab>
          </TabsList>
          <div className="flex items-center gap-2">
            <ScopePicker snapshot={snapshot} label="Show memory for" />
            {tab === "memories" ? <Button size="sm" onClick={() => setDraft({ value: {} })}><Plus /> Add memory</Button> : null}
          </div>
        </div>
        <TabsPanel value="memories" className="flex flex-col gap-8 pt-6">
          {proposals.length ? <Proposals snapshot={snapshot} proposals={proposals} /> : null}
          {memories.length === 0 ? (
            <EmptyBlock icon={Brain} title="Nothing remembered yet" description="Tell Neko “remember that…” in Home, or add one here. Forgetting is permanent.">
              <Button variant="outline" onClick={() => setDraft({ value: {} })}><Plus /> Add memory</Button>
            </EmptyBlock>
          ) : (
            scopes.map(([name, list]) => (
              <Section key={name} title={name} count={list.length} collapsible>
                <Panel>
                  {list.map((m) => <MemoryRow key={str(m.id)} snapshot={snapshot} entry={m} kindLabel={KINDS.find((k) => k.value === m.kind)?.label ?? str(m.kind)} onEdit={() => setDraft({ value: m })} />)}
                </Panel>
              </Section>
            ))
          )}
          <MemoryOptions snapshot={snapshot} />
        </TabsPanel>
        <TabsPanel value="style" className="pt-6"><WorkingStyle snapshot={snapshot} scope={scope} profileId={profileId} /></TabsPanel>
      </Tabs>
      <MemoryEditor snapshot={snapshot} draft={draft} scope={scope} profileId={profileId} onClose={() => setDraft(null)} />
    </>
  );
}

export default function Memory() {
  useSetupCommands();
  return (
    <SetupPage title={<>Memory</>}>
      <Gate>{(snapshot) => <MemoryBody snapshot={snapshot} />}</Gate>
    </SetupPage>
  );
}
