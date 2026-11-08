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
import { PencilSimple, Plus, Stack, Star, Users } from "@/ui/icons";
import { useContextMenu } from "@/shell/context-menu";
import { Panel, RowAction, RowActions } from "@/setup/kit";
import { Checkbox } from "@/ui/checkbox";
import { EmptyBlock, ErrorNote, FormField, Gate, SetupPage, registerCommands } from "@/setup/kit";
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
  const [open, setOpen] = useState(false);
  const info = snapshot.agent_profiles ?? {};
  const profiles: Json[] = arr(info.profiles);
  const workspaces = workspacesOf(snapshot);
  const active = info.active_profile_id === profile.id;
  const assigned = arr(info.assignments).filter((a) => a.profile_id === profile.id).map((a) => workspaces.find((w) => w.id === a.workspace_id)?.name).filter(Boolean);
  const others = profiles.filter((p) => p.id !== profile.id);
  const granted = (source: Json) => arr(info.read_grants).some((g) => g.reader_id === profile.id && g.source_id === source.id);
  const makeDefault = () => void run(() => profilesCmd("SetActive", { profile_id: profile.id }));
  const assign = (workspaceId: string) => void run(() => profilesCmd("AssignWorkspace", { workspace_id: workspaceId, profile_id: profile.id }));
  const instructions = str(profile.instructions);
  const menu = useContextMenu(() => [
    { label: "Edit…", icon: PencilSimple, run: onEdit },
    active ? null : { label: "Make default", icon: Star, run: makeDefault },
    ...(workspaces.length ? ["separator" as const, ...workspaces.map((w) => ({ label: `Use in ${w.name}`, run: () => assign(w.id) }))] : []),
  ]);
  return (
    <div data-slot="profile" data-profile={str(profile.name)} {...menu} className="group/row flex flex-col gap-1 rounded-[10px] px-3 py-1.5 transition-colors hover:bg-foreground/[0.03]">
      <div className="flex min-h-8 items-center gap-3">
        <button type="button" aria-expanded={open} aria-label={`${open ? "Hide" : "Show"} details for ${str(profile.name)}`} onClick={() => setOpen(!open)} className="flex min-w-0 flex-1 items-baseline gap-3 rounded-md text-start outline-none focus-visible:ring-2 focus-visible:ring-ring/60">
          <h3 className="shrink-0 truncate text-[13.5px] font-normal text-foreground">{str(profile.name)}</h3>
          {active ? <span className="shrink-0 text-[12.5px] text-primary">Default</span> : null}
          <span className="min-w-0 flex-1 truncate text-[12.5px] text-muted-foreground" title={instructions || undefined}>{instructions || "No special instructions"}</span>
          <span className="shrink-0 text-[12.5px] text-muted-foreground/70">{assigned.length ? `Used in ${assigned.join(", ")}` : null}</span>
        </button>
        <RowActions>
          {!active ? <RowAction label="Make default" icon={Star} disabled={busy} onClick={makeDefault} /> : null}
          <DropdownMenu>
            <DropdownMenuTrigger render={<Button variant="ghost" size="icon-sm" aria-label={`Use ${str(profile.name)} in a workspace`} title="Use in workspace" disabled={workspaces.length === 0} className="text-muted-foreground" />}><Stack /></DropdownMenuTrigger>
            <DropdownMenuContent align="end">
              {workspaces.map((w) => (
                <DropdownMenuItem key={w.id} onClick={() => assign(w.id)}>{w.name}</DropdownMenuItem>
              ))}
            </DropdownMenuContent>
          </DropdownMenu>
          <RowAction label="Edit" icon={PencilSimple} onClick={onEdit} />
        </RowActions>
      </div>
      {open ? (
        <div className="flex flex-col gap-2 pb-1 text-[13px]">
          <p className={`max-w-[70ch] whitespace-pre-wrap leading-relaxed ${instructions ? "text-foreground/85" : "text-muted-foreground"}`}>
            {instructions || "No special instructions. Edit this profile to add a voice or working rules."}
          </p>
          {others.length > 0 ? (
            <div className="flex flex-col gap-1.5">
              {others.map((source) => (
                <label key={source.id} className="flex items-center gap-2.5 text-muted-foreground" title="This only shares memories. It never shares tools or workspace access.">
                  <Checkbox checked={granted(source)} disabled={busy} aria-label={`Can read ${str(source.name)}’s general memories`} onCheckedChange={(on) => void run(() => profilesCmd("SetReadGrant", { reader_id: profile.id, source_id: source.id, allowed: !!on }))} />
                  Can read {str(source.name)}’s general memories
                </label>
              ))}
            </div>
          ) : null}
        </div>
      ) : null}
      {error ? <ErrorNote error={error} onDismiss={clear} className="w-full" /> : null}
    </div>
  );
}

function ProfilesBody({ snapshot, onEdit }: { snapshot: Json; onEdit: (draft: Draft) => void }) {
  const profiles: Json[] = arr(snapshot.agent_profiles?.profiles);
  if (profiles.length === 0) {
    return <EmptyBlock icon={Users} title="No profiles" description="A profile bundles instructions and memories for a kind of work."><Button onClick={() => onEdit({ value: {} })}><Plus /> New profile</Button></EmptyBlock>;
  }
  return (
    <Panel>
      {profiles.map((p) => <ProfilePanel key={str(p.id)} snapshot={snapshot} profile={p} onEdit={() => onEdit({ value: p })} />)}
    </Panel>
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
      title={<>Profiles</>}
      actions={<Button size="sm" onClick={() => setDraft({ value: {} })}><Plus /> New profile</Button>}
    >
      <Gate>{(snapshot) => <ProfilesBody snapshot={snapshot} onEdit={setDraft} />}</Gate>
      <ProfileEditor draft={draft} onClose={() => setDraft(null)} />
    </SetupPage>
  );
}
