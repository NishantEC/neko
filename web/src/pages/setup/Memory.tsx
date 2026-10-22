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
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/ui/collapsible";
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuSeparator, DropdownMenuTrigger } from "@/ui/dropdown-menu";
import { Tabs, TabsList, TabsPanel, TabsTab } from "@/ui/tabs";
import { Brain } from "@phosphor-icons/react";
import { CaretDown, DotsThree, PencilSimple, Plus, Trash } from "@/ui/icons";
import { Panel } from "@/setup/kit";
import {
  ConfirmDialog, EmptyBlock, ErrorNote, FormField, Gate, Pick, Row, Section, SetupPage, Status, Toggle, registerCommands,
} from "@/setup/kit";
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
  const source = str(entry.source);
  return (
    <div data-slot="memory" className="flex flex-col gap-2.5 px-6 py-4">
      <div className="flex items-start gap-3">
        <p className="min-w-0 flex-1 select-text whitespace-pre-wrap text-[14px] leading-relaxed text-foreground/90">{str(entry.text)}</p>
        <DropdownMenu>
          <DropdownMenuTrigger render={<Button variant="ghost" size="icon-sm" aria-label="Memory actions" />}><DotsThree weight="bold" /></DropdownMenuTrigger>
          <DropdownMenuContent align="end">
            <DropdownMenuItem onClick={onEdit}><PencilSimple /> Edit</DropdownMenuItem>
            <DropdownMenuSeparator />
            <DropdownMenuItem className="text-destructive-foreground" onClick={() => setConfirm(true)}><Trash /> Forget memory…</DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </div>
      <div className="flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-muted-foreground">
        <span className="font-mono uppercase tracking-[0.12em]">{kindLabel}</span>
        <span>{entry.workspace_id ? workspaceName(snapshot, entry.workspace_id, "Workspace") : "Everywhere"}</span>
        {source ? (
          <Collapsible>
            <CollapsibleTrigger render={<button type="button" className="inline-flex items-center gap-1 rounded text-xs outline-none hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring" />}>
              {sourceLabel(source)} <CaretDown aria-hidden className="size-3" />
            </CollapsibleTrigger>
            <CollapsibleContent><p className="select-text pt-1 font-mono text-[11px]">{source}</p></CollapsibleContent>
          </Collapsible>
        ) : null}
      </div>
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
    <Section eyebrow={`${proposals.length} waiting`} title="Suggested" intro="Neko noticed these. Nothing is remembered until you keep it.">
      <Panel>
        <div className="divide-y divide-border/70">
          {proposals.map((p) => (
            <div key={str(p.id)} data-slot="memory-proposal" className="flex flex-col gap-3 px-6 py-4">
              <p className="select-text whitespace-pre-wrap text-[14px] leading-relaxed">{str(p.text)}</p>
              <div className="flex flex-wrap items-center gap-2">
                <Button size="sm" disabled={busy} onClick={() => void run(() => command("DecideMemoryProposal", { id: p.id, accept: true }))}>Keep memory</Button>
                <Button size="sm" variant="outline" disabled={busy} onClick={() => void run(() => command("DecideMemoryProposal", { id: p.id, accept: false }))}>Dismiss</Button>
                <span className="ms-auto text-xs text-muted-foreground">{p.workspace_id ? workspaceName(snapshot, p.workspace_id, "Workspace") : "Everywhere"} · {sourceLabel(str(p.source))}</span>
              </div>
            </div>
          ))}
        </div>
        {error ? <div className="border-t border-border/70 p-4"><ErrorNote error={error} onDismiss={clear} /></div> : null}
      </Panel>
    </Section>
  );
}

function MemoryOptions({ snapshot }: { snapshot: Json }) {
  const { run, busy, error, clear } = useRun();
  const options = snapshot.memory_options ?? {};
  const learning = options.learning ?? true;
  const useMemory = options.use_memory ?? true;
  const set = (patch: Record<string, boolean>) => void run(() => command("SetMemoryOptions", { options: { learning, use_memory: useMemory, ...patch } }));
  return (
    <Section eyebrow="Settings" title="Memory settings">
      <Panel>
        <div className="divide-y divide-border/70">
          <Row title="Suggest new memories" description="Propose context from chats and agents. Direct requests to remember something still work when this is off." trailing={<Toggle checked={learning} onChange={(v) => set({ learning: v })} label="Suggest new memories" disabled={busy} />} />
          <Row title="Use saved memory" description="Include memory in replies and tasks. Turning this off keeps saved memories here." trailing={<Toggle checked={useMemory} onChange={(v) => set({ use_memory: v })} label="Use saved memory" disabled={busy} />} />
        </div>
        {error ? <div className="border-t border-border/70 p-4"><ErrorNote error={error} onDismiss={clear} /></div> : null}
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
  return (
    <div data-slot="preference" className="flex flex-col gap-2.5 px-6 py-4">
      <p className="select-text whitespace-pre-wrap text-[14px] leading-relaxed">{str(item.instruction)}</p>
      <p className="text-xs text-muted-foreground">Applies to: {applies || "—"}{arr(item.exceptions).length ? ` · Except: ${arr(item.exceptions).join(", ")}` : ""}</p>
      <p className="text-xs text-muted-foreground">{workspaceName(snapshot, item.workspace_id, "Workspace")} · version {item.version}</p>
      <div className="flex flex-wrap items-center gap-2">
        {item.state !== "confirmed" ? <Button size="sm" disabled={busy} onClick={() => void run(() => decisionCmd("KeepPreference", fields))}>Keep guidance</Button> : null}
        <Button size="sm" variant="outline" onClick={onEdit}>Edit</Button>
        {item.state !== "dismissed" ? <Button size="sm" variant="outline" disabled={busy} onClick={() => void run(() => decisionCmd("DismissPreference", fields))}>Dismiss</Button> : null}
        <span className="flex-1" />
        <Button size="sm" variant="ghost" className="text-destructive-foreground" onClick={() => setForget(true)}><Trash /> Forget…</Button>
      </div>
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
    <div data-slot="decision" className="flex flex-col gap-2 px-6 py-4">
      <div className="flex items-start gap-3">
        <div className="min-w-0 flex-1">
          <p className="text-[14px] font-medium">{decisionTitle(record.action)}</p>
          <p className="text-[13px] text-muted-foreground">{str(record.observed_outcome) || "Outcome not yet observed"}</p>
          {str(record.correction?.text) ? <p className="mt-1 text-[13px]"><span className="text-muted-foreground">Your correction: </span>{str(record.correction.text)}</p> : null}
        </div>
        {record.delivery_stage === "local_accepted" ? <Status tone="success">Accepted locally</Status> : null}
        <Button size="sm" variant="outline" onClick={() => setOpen(true)}>Correct this decision</Button>
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
  const sections: [string, string, string][] = [["proposed", "Needs your confirmation", "warning"], ["confirmed", "Confirmed guidance", "neutral"], ["dismissed", "Dismissed guidance", "neutral"]];
  return (
    <div className="flex flex-col gap-8">
      <Section
        eyebrow="Working style"
        title="Working guidance"
        intro="Contextual rules guide choices within the permissions you already gave."
        actions={<Button variant="outline" size="sm" disabled={!target} onClick={() => setEditor({ value: { workspace_id: target } })}><Plus /> Add guidance</Button>}
      >
        {prefs.length === 0 ? <EmptyBlock icon={Brain} title="No guidance yet" description="Add guidance for a recurring situation, or correct an agent decision to suggest a rule." /> : null}
      </Section>
      {sections.map(([state, title]) => {
        const items = group(state);
        if (!items.length) return null;
        return (
          <Section key={state} eyebrow={`${items.length}`} title={title}>
            <Panel><div className="divide-y divide-border/70">{items.map((p) => <PreferenceRow key={str(p.id)} snapshot={snapshot} item={p} onEdit={() => setEditor({ value: p })} />)}</div></Panel>
          </Section>
        );
      })}
      {records.length ? (
        <Section eyebrow={`${records.length} most recent`} title="Recent decisions" intro="Scope is recorded at the time of the decision. Current permissions govern every action.">
          <Panel><div className="divide-y divide-border/70">{records.map((r) => <DecisionCard key={str(r.id)} record={r} />)}</div></Panel>
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
            {tab === "memories" ? <Button onClick={() => setDraft({ value: {} })}><Plus /> Add memory</Button> : null}
          </div>
        </div>
        <TabsPanel value="memories" className="flex flex-col gap-8 pt-6">
          <MemoryOptions snapshot={snapshot} />
          {proposals.length ? <Proposals snapshot={snapshot} proposals={proposals} /> : null}
          <Section eyebrow={memories.length ? `${memories.length} saved` : undefined} title="Remembered" intro="Saved context, grouped by where it applies. Forgetting is permanent.">
            {memories.length === 0 ? (
              <EmptyBlock icon={Brain} title="Nothing yet" description="Tell Neko “remember that…” in Home, or add one here.">
                <Button variant="outline" onClick={() => setDraft({ value: {} })}><Plus /> Add memory</Button>
              </EmptyBlock>
            ) : (
              scopes.map(([name, list]) => (
                <div key={name} className="flex flex-col gap-2">
                  <span className="font-mono text-[11px] uppercase tracking-[0.18em] text-muted-foreground">{name} · {list.length}</span>
                  <Panel><div className="divide-y divide-border/70">
                    {list.map((m) => <MemoryRow key={str(m.id)} snapshot={snapshot} entry={m} kindLabel={KINDS.find((k) => k.value === m.kind)?.label ?? str(m.kind)} onEdit={() => setDraft({ value: m })} />)}
                  </div></Panel>
                </div>
              ))
            )}
          </Section>
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
    <SetupPage
      eyebrow="Knowledge & tools"
      title={<>Memory</>}
      subtitle="Saved context and working guidance. Suggestions wait for your approval."
    >
      <Gate>{(snapshot) => <MemoryBody snapshot={snapshot} />}</Gate>
    </SetupPage>
  );
}
