// Workspaces: the logical places Neko works in — their folders, the tools and
// watches that belong to them, and the instructions every agent there follows.
import { useEffect, useMemo, useState } from "react";
import { command, type Json } from "@/api";
import { Button } from "@/ui/button";
import { Input } from "@/ui/input";
import { Textarea } from "@/ui/textarea";
import { Badge } from "@/ui/badge";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/ui/dialog";
import { ArrowsClockwise, Eye, FolderSimple, PencilSimple, Plus, Stack, Wrench, X } from "@/ui/icons";
import { navigate } from "@/router";
import { ErrorNote, FormField, Gate, EmptyBlock, PanelFooter, Row, SetupPage, Status, registerCommands } from "@/setup/kit";
import { Panel } from "@/setup/kit";
import { arr, foldersOf, mcp, relativeTime, str, useRun, type Workspace } from "@/setup/lib";

function WorkspaceDialog({ open, onOpenChange, workspace, folders: initialFolders }: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  workspace: Workspace | null;
  folders: string[];
}) {
  const { run, busy, error, clear } = useRun();
  const [name, setName] = useState("");
  const [instructions, setInstructions] = useState("");
  const [folders, setFolders] = useState<string[]>([]);
  const [path, setPath] = useState("");

  useEffect(() => {
    if (!open) return;
    clear();
    setName(workspace?.name ?? "");
    setInstructions(workspace?.instructions ?? "");
    setFolders(initialFolders);
    setPath("");
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, workspace?.id]);

  const addPath = () => {
    const value = path.trim();
    if (!value) return;
    if (!folders.includes(value)) setFolders([...folders, value]);
    if (!name.trim()) setName(value.replace(/\/+$/, "").split("/").pop() ?? "");
    setPath("");
  };
  const pending = path.trim();
  const all = pending && !folders.includes(pending) ? [...folders, pending] : folders;
  const canSave = name.trim().length > 0 && all.length > 0 && !busy;

  const save = async () => {
    const saved = await run(() =>
      command("SaveWorkspaceWithFolders", {
        workspace: {
          id: workspace?.id ?? "",
          name: name.trim(),
          repository: all[0],
          instructions,
          away_enabled: workspace?.away_enabled ?? false,
        },
        folders: all,
      }),
    );
    if (saved) onOpenChange(false);
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>{workspace ? "Workspace settings" : "Create workspace"}</DialogTitle>
          <DialogDescription>Give this work a name, a home, and its own instructions.</DialogDescription>
        </DialogHeader>
        <form
          className="flex flex-col gap-5"
          onSubmit={(e) => { e.preventDefault(); void save(); }}
        >
          <FormField label="Name" htmlFor="ws-name">
            <Input id="ws-name" value={name} onChange={(e) => setName(e.target.value)} placeholder="athena" autoComplete="off" />
          </FormField>
          <FormField label="Source folders" htmlFor="ws-path" hint="Absolute paths on this Mac. Folders do not need to be Git repositories; the daemon checks each one when you save.">
            {all.length === 0 ? <p className="text-[13px] text-muted-foreground">Add at least one folder where this work lives.</p> : null}
            <ul className="flex flex-col gap-1">
              {folders.map((folder) => (
                <li key={folder} className="flex items-center gap-2 rounded-lg border border-border bg-foreground/[0.03] py-1 ps-2.5 pe-1">
                  <FolderSimple className="size-4 shrink-0 text-muted-foreground" aria-hidden />
                  <span className="min-w-0 flex-1 truncate font-mono text-[12px]" title={folder}>{folder}</span>
                  <Button type="button" variant="ghost" size="icon-xs" aria-label={`Remove ${folder}`} onClick={() => setFolders(folders.filter((f) => f !== folder))}>
                    <X />
                  </Button>
                </li>
              ))}
            </ul>
            <div className="flex gap-2">
              <Input
                id="ws-path"
                value={path}
                onChange={(e) => setPath(e.target.value)}
                onKeyDown={(e) => { if (e.key === "Enter") { e.preventDefault(); addPath(); } }}
                placeholder="/Users/you/Documents/project"
                className="font-mono"
                autoComplete="off"
              />
              <Button type="button" variant="outline" className="h-10" onClick={addPath} disabled={!path.trim()}>Add folder</Button>
            </div>
          </FormField>
          <FormField label="Instructions · optional" htmlFor="ws-instructions">
            <Textarea id="ws-instructions" value={instructions} onChange={(e) => setInstructions(e.target.value)} placeholder="How should Neko work here?" rows={4} />
          </FormField>
          <ErrorNote error={error} title="Couldn’t save the workspace" />
          <DialogFooter>
            <Button type="button" variant="outline" onClick={() => onOpenChange(false)}>Cancel</Button>
            <Button type="submit" disabled={!canSave} loading={busy}>Save workspace</Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}

function WorkspacePanel({ snapshot, workspace, onEdit }: { snapshot: Json; workspace: Workspace; onEdit: () => void }) {
  const { run, busy, error, clear } = useRun();
  const folders = foldersOf(snapshot, workspace);
  const watches = arr(snapshot.mcp?.responsibilities).filter((r) => r.workspace_id === workspace.id);
  const tools = arr(snapshot.mcp?.connections).filter((c) => !str(c.workspace_id) || c.workspace_id === workspace.id);
  const open = arr(snapshot.tasks).filter((t) => t.workspace_id === workspace.id && !["Completed", "Cancelled"].includes(t.status)).length;
  const schedules = arr(snapshot.schedules).filter((s) => s.workspace_id === workspace.id);
  const active = watches.filter((w) => w.enabled).length;
  const lastChecked = Math.max(0, ...watches.map((w) => w.last_attempt_ms ?? 0));
  const failing = watches.some((w) => (w.failures ?? 0) > 0);
  const broken = tools.filter((c) => c.enabled && str(c.error));

  const syncNow = () =>
    run(async () => {
      let last: Json = null;
      for (const w of watches) last = await mcp("Wake", { responsibility_id: w.id });
      return last;
    });

  return (
    <Panel data-slot="workspace" data-workspace={workspace.name}>
      <div className="flex flex-wrap items-center justify-between gap-3 px-5 py-4">
        <div className="flex min-w-0 items-center gap-3">
          <span aria-hidden className="size-2 shrink-0 rounded-full bg-success" />
          <h3 className="truncate text-[17px] font-medium text-strong">{workspace.name}</h3>
          {open > 0 ? <Badge variant="outline" className="font-mono text-[11px] font-normal text-muted-foreground">{open} open</Badge> : null}
        </div>
        <div className="flex items-center gap-2">
          <Button variant="outline" size="sm" onClick={() => void syncNow()} disabled={watches.length === 0 || busy} loading={busy} title={watches.length === 0 ? "Add something to watch first" : "Check everything this workspace watches"}>
            <ArrowsClockwise /> Sync now
          </Button>
          <Button variant="outline" size="sm" onClick={onEdit}><PencilSimple /> Edit</Button>
        </div>
      </div>
      <div className="divide-y divide-border/70 border-t border-border/70">
        <Row
          title="Watching"
          leading={<Eye />}
          description={watches.length === 0 ? "Nothing yet. Add a watch in Watching." : `${active} active${watches.length - active > 0 ? ` · ${watches.length - active} paused or suggested` : ""}`}
          trailing={watches.length > 0 ? <Status tone={failing ? "warning" : lastChecked ? "success" : "neutral"}>{failing ? "Needs attention" : lastChecked ? `Synced ${relativeTime(lastChecked)}` : "Not checked yet"}</Status> : null}
        />
        <Row
          title="Folders"
          leading={<FolderSimple />}
          description={folders.length === 0 ? "No folders saved. Edit the workspace to add one." : undefined}
        >
          <ul className="flex flex-col gap-1">
            {folders.map((f) => <li key={f} className="truncate font-mono text-[12px] text-foreground/80" title={f}>{f}</li>)}
          </ul>
        </Row>
        <Row
          title="Connected tools"
          leading={<Wrench />}
          description={tools.length === 0 ? "Connect tools in Tools & skills to make them available here." : broken.length ? `${broken.length} need attention` : undefined}
          trailing={schedules.length ? <span className="font-mono text-[12px] text-muted-foreground">{schedules.length} {schedules.length === 1 ? "schedule" : "schedules"}</span> : null}
        >
          {tools.length ? (
            <div className="flex flex-wrap gap-1.5">
              {tools.map((c) => (
                <Badge key={c.id} variant="outline" className={`text-[12px] font-normal ${c.enabled && str(c.error) ? "text-warning-foreground" : "text-muted-foreground"}`}>{c.label}</Badge>
              ))}
            </div>
          ) : null}
        </Row>
        {workspace.instructions ? (
          <Row title="Instructions">
            <p className="max-w-[70ch] whitespace-pre-wrap leading-relaxed text-foreground/80">{workspace.instructions}</p>
          </Row>
        ) : null}
      </div>
      {error ? <div className="border-t border-border/70 p-4"><ErrorNote error={error} title="Couldn’t sync" onDismiss={clear} /></div> : null}
      <PanelFooter className="justify-between">
        <span className="font-mono text-[11px] text-muted-foreground">{workspace.id.slice(0, 8)}</span>
        <div className="flex gap-2">
          <Button variant="ghost" size="sm" onClick={() => navigate("watching")}>Open Watching</Button>
          <Button variant="ghost" size="sm" onClick={() => navigate("tools")}>Open Tools &amp; skills</Button>
        </div>
      </PanelFooter>
    </Panel>
  );
}

export default function Workspaces() {
  const [dialog, setDialog] = useState<{ open: boolean; id: string | null }>({ open: false, id: null });

  useEffect(
    () => registerCommands([
      { id: "workspaces.add", title: "Add a workspace", keywords: ["create", "folder", "project"], icon: Plus, group: "Workspaces", run: () => setDialog({ open: true, id: null }) },
    ]),
    [],
  );

  return (
    <SetupPage
      eyebrow="Work"
      title={<>Where Neko <em>works.</em></>}
      subtitle="Project folders, connected tools, and the work Neko watches."
      actions={<Button onClick={() => setDialog({ open: true, id: null })}><Plus /> Add workspace</Button>}
    >
      <Gate>
        {(snapshot) => <WorkspaceList snapshot={snapshot} dialog={dialog} setDialog={setDialog} />}
      </Gate>
    </SetupPage>
  );
}

function WorkspaceList({ snapshot, dialog, setDialog }: { snapshot: Json; dialog: { open: boolean; id: string | null }; setDialog: (d: { open: boolean; id: string | null }) => void }) {
  const workspaces: Workspace[] = arr(snapshot.workspaces);
  const editing = useMemo(() => workspaces.find((w) => w.id === dialog.id) ?? null, [workspaces, dialog.id]);
  return (
    <>
      {workspaces.length === 0 ? (
        <EmptyBlock icon={Stack} title="No workspaces yet" description="A workspace is a name plus the folders where the work lives. Add one to start tasks, watches and schedules.">
          <Button onClick={() => setDialog({ open: true, id: null })}><Plus /> Add workspace</Button>
        </EmptyBlock>
      ) : (
        <div className="flex flex-col gap-5">
          {workspaces.map((w) => (
            <WorkspacePanel key={w.id} snapshot={snapshot} workspace={w} onEdit={() => setDialog({ open: true, id: w.id })} />
          ))}
        </div>
      )}
      <WorkspaceDialog
        open={dialog.open}
        onOpenChange={(open) => setDialog({ open, id: open ? dialog.id : null })}
        workspace={editing}
        folders={editing ? foldersOf(snapshot, editing) : []}
      />
    </>
  );
}
