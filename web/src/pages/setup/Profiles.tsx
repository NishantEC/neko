// Profiles: named sets of instructions and memories for different kinds of
// work. A profile is assigned to workspaces; sharing memories between profiles
// never shares tools or workspace access.
import { useEffect, useState } from "react";
import type { Json } from "@/api";
import { Button } from "@/ui/button";
import { Input } from "@/ui/input";
import { Textarea } from "@/ui/textarea";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/ui/dialog";
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger } from "@/ui/dropdown-menu";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/ui/collapsible";
import { CaretDown, PencilSimple, Plus, Users } from "@/ui/icons";
import { Panel } from "@/setup/kit";
import { Checkbox } from "@/ui/checkbox";
import { EmptyBlock, ErrorNote, FormField, Gate, PanelFooter, SetupPage, Status, registerCommands } from "@/setup/kit";
import { useSetupCommands } from "@/setup/commands";
import { arr, profilesCmd, str, useRun, workspacesOf } from "@/setup/lib";

type Draft = { value: Json } | null;

function ProfileEditor({ draft, onClose }: { draft: Draft; onClose: () => void }) {
  const { run, busy, error, clear } = useRun();
  const original = draft?.value ?? {};
  const isNew = !str(original.id);
  const [name, setName] = useState("");
  const [instructions, setInstructions] = useState("");
  useEffect(() => {
    if (!draft) return;
    clear();
    setName(str(original.name));
    setInstructions(str(original.instructions));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [draft]);
  const save = async () => {
    const saved = await run(() => profilesCmd("Save", { profile: { id: str(original.id), name: name.trim(), instructions } }));
    if (saved) onClose();
  };
  return (
    <Dialog open={!!draft} onOpenChange={(o) => { if (!o) onClose(); }}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>{isNew ? "New profile" : "Edit profile"}</DialogTitle>
          <DialogDescription>Set the instructions Neko uses for this kind of work.</DialogDescription>
        </DialogHeader>
        <form className="flex flex-col gap-5" onSubmit={(e) => { e.preventDefault(); if (name.trim() && !busy) void save(); }}>
          <FormField label="Name" htmlFor="profile-name"><Input id="profile-name" value={name} onChange={(e) => setName(e.target.value)} placeholder="Reviewer" autoComplete="off" /></FormField>
          <FormField label="Instructions" htmlFor="profile-instructions"><Textarea id="profile-instructions" rows={6} value={instructions} onChange={(e) => setInstructions(e.target.value)} placeholder="Be terse. Review like a staff engineer." /></FormField>
          <ErrorNote error={error} title="Couldn’t save the profile" />
          <DialogFooter>
            <Button type="button" variant="outline" onClick={onClose}>Cancel</Button>
            <Button type="submit" disabled={!name.trim() || busy} loading={busy}>Save</Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}

function ProfilePanel({ snapshot, profile, onEdit }: { snapshot: Json; profile: Json; onEdit: () => void }) {
  const { run, busy, error, clear } = useRun();
  const info = snapshot.agent_profiles ?? {};
  const profiles: Json[] = arr(info.profiles);
  const workspaces = workspacesOf(snapshot);
  const active = info.active_profile_id === profile.id;
  const assigned = arr(info.assignments).filter((a) => a.profile_id === profile.id).map((a) => workspaces.find((w) => w.id === a.workspace_id)?.name).filter(Boolean);
  const others = profiles.filter((p) => p.id !== profile.id);
  const granted = (source: Json) => arr(info.read_grants).some((g) => g.reader_id === profile.id && g.source_id === source.id);
  return (
    <Panel data-slot="profile" data-profile={str(profile.name)}>
      <div className="flex flex-wrap items-center justify-between gap-3 px-6 py-4">
        <div className="flex min-w-0 items-center gap-3">
          <span aria-hidden className={`size-2 shrink-0 rounded-full ${active ? "bg-primary" : "bg-muted-foreground/50"}`} />
          <h3 className="truncate text-[17px] font-medium text-strong">{str(profile.name)}</h3>
          {active ? <Status tone="success">Default</Status> : null}
        </div>
        <div className="flex items-center gap-2">
          {!active ? <Button variant="outline" size="sm" disabled={busy} onClick={() => void run(() => profilesCmd("SetActive", { profile_id: profile.id }))}>Make default</Button> : null}
          <DropdownMenu>
            <DropdownMenuTrigger render={<Button variant="outline" size="sm" disabled={workspaces.length === 0} />}>Use in workspace <CaretDown /></DropdownMenuTrigger>
            <DropdownMenuContent align="end">
              {workspaces.map((w) => (
                <DropdownMenuItem key={w.id} onClick={() => void run(() => profilesCmd("AssignWorkspace", { workspace_id: w.id, profile_id: profile.id }))}>{w.name}</DropdownMenuItem>
              ))}
            </DropdownMenuContent>
          </DropdownMenu>
          <Button variant="outline" size="sm" onClick={onEdit}><PencilSimple /> Edit</Button>
        </div>
      </div>
      <div className="flex flex-col gap-3 border-t border-border/70 px-6 py-4 text-[13px]">
        <p className={`max-w-[70ch] whitespace-pre-wrap leading-relaxed ${str(profile.instructions) ? "text-foreground/85" : "text-muted-foreground"}`}>
          {str(profile.instructions) || "No special instructions. Edit this profile to add a voice or working rules."}
        </p>
        <p className="text-muted-foreground">{assigned.length ? `Used in ${assigned.join(", ")}` : "Not used by a specific workspace."}</p>
        {others.length > 0 ? (
          <Collapsible>
            <CollapsibleTrigger render={<Button variant="ghost" size="sm" className="-ms-2" />}>Share memories from other profiles <CaretDown /></CollapsibleTrigger>
            <CollapsibleContent>
              <div className="flex flex-col gap-2 pt-2">
                {others.map((source) => (
                  <label key={source.id} className="flex items-center gap-2.5">
                    <Checkbox checked={granted(source)} disabled={busy} aria-label={`Can read ${str(source.name)}’s general memories`} onCheckedChange={(on) => void run(() => profilesCmd("SetReadGrant", { reader_id: profile.id, source_id: source.id, allowed: !!on }))} />
                    Can read {str(source.name)}’s general memories
                  </label>
                ))}
                <p className="text-xs text-muted-foreground">This only shares memories. It never shares tools or workspace access.</p>
              </div>
            </CollapsibleContent>
          </Collapsible>
        ) : null}
      </div>
      {error ? <PanelFooter className="justify-start"><ErrorNote error={error} onDismiss={clear} className="w-full" /></PanelFooter> : null}
    </Panel>
  );
}

function ProfilesBody({ snapshot, onEdit }: { snapshot: Json; onEdit: (draft: Draft) => void }) {
  const profiles: Json[] = arr(snapshot.agent_profiles?.profiles);
  if (profiles.length === 0) {
    return <EmptyBlock icon={Users} title="No profiles" description="A profile bundles instructions and memories for a kind of work."><Button onClick={() => onEdit({ value: {} })}><Plus /> New profile</Button></EmptyBlock>;
  }
  return (
    <div className="flex flex-col gap-5">
      {profiles.map((p) => <ProfilePanel key={str(p.id)} snapshot={snapshot} profile={p} onEdit={() => onEdit({ value: p })} />)}
    </div>
  );
}

export default function Profiles() {
  useSetupCommands();
  const [draft, setDraft] = useState<Draft>(null);
  useEffect(
    () => registerCommands([{ id: "profiles.new", title: "New profile", keywords: ["agent", "persona"], icon: Plus, group: "Profiles", run: () => setDraft({ value: {} }) }]),
    [],
  );
  return (
    <SetupPage
      eyebrow="Knowledge & tools"
      title={<>Profiles</>}
      subtitle="Instructions and memories for each kind of work."
      actions={<Button onClick={() => setDraft({ value: {} })}><Plus /> New profile</Button>}
    >
      <Gate>{(snapshot) => <ProfilesBody snapshot={snapshot} onEdit={setDraft} />}</Gate>
      <ProfileEditor draft={draft} onClose={() => setDraft(null)} />
    </SetupPage>
  );
}
