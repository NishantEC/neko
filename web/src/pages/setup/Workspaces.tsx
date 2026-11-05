// Workspaces: the logical places Neko works in. Each one is a scope: its
// folders, the MCP servers those folders configure on disk (whole scope or one
// repository's own), the routines that run there and the standing orders they
// follow, plus the watches, tools and instructions every agent there uses.
// Workspaces come from the workbench snapshot; scope details, routines and
// standing orders from `Routines` (polled while visible), joined on workspace_id.
// Routines and standing orders are edited on the Inbox page.
import { useCallback, useEffect, useMemo, useState, type ReactNode } from "react";
import { command, request, type Json } from "@/api";
import { Button } from "@/ui/button";
import { Input } from "@/ui/input";
import { Textarea } from "@/ui/textarea";
import { Badge } from "@/ui/badge";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/ui/dialog";
import { ArrowRight, ArrowsClockwise, Eye, FolderSimple, Note, PencilSimple, Plugs, Plus, Stack, Tray, Warning, Wrench, X } from "@/ui/icons";
import { hrefFor, navigate } from "@/router";
import { ErrorNote, FormField, Gate, EmptyBlock, PanelFooter, Row, SetupPage, Status, registerCommands } from "@/setup/kit";
import { Panel } from "@/setup/kit";
import { arr, foldersOf, mcp, relativeTime, str, useNow, useRun, type Workspace } from "@/setup/lib";

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
            <Input id="ws-name" value={name} onChange={(e) => setName(e.target.value)} placeholder="acme-api" autoComplete="off" />
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

/** Lists show this many before "Show more", so a workspace fits on a screen. */
const SHORT = 5;
/** Same cadence as the Inbox; a hidden page does not poll. */
const POLL_MS = 10_000;

type RoutinesData = { routines: Json[]; standing_orders: Record<string, string>; scopes: Json[] };
type RoutinesState = { data: RoutinesData | null; error: string | null };

const folderName = (path: string) => path.split("/").filter(Boolean).pop() ?? path;

/** Scope details, routines and standing orders from `Routines`, read-only. */
function useRoutines(): RoutinesState {
  const [data, setData] = useState<RoutinesData | null>(null);
  const [error, setError] = useState<string | null>(null);
  const load = useCallback(async () => {
    try {
      const reply = await request("Routines");
      if (reply?.Routines) setData(reply.Routines as RoutinesData);
      setError(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }, []);
  useEffect(() => {
    void load();
    const id = setInterval(() => { if (!document.hidden) void load(); }, POLL_MS);
    const onVisible = () => { if (!document.hidden) void load(); };
    document.addEventListener("visibilitychange", onVisible);
    return () => { clearInterval(id); document.removeEventListener("visibilitychange", onVisible); };
  }, [load]);
  return { data, error };
}

/** The first `SHORT` items, and a toggle for the rest. */
function ShortList<T>({ items, render, noun, label, className }: { items: T[]; render: (item: T) => ReactNode; noun: string; label?: string; className?: string }) {
  const [all, setAll] = useState(false);
  const shown = all ? items : items.slice(0, SHORT);
  return (
    <div className="flex flex-col items-start gap-1">
      <ul aria-label={label} className={className}>{shown.map(render)}</ul>
      {items.length > SHORT ? (
        <Button variant="ghost" size="xs" className="-ms-2" aria-expanded={all} onClick={() => setAll(!all)}>
          {all ? "Show fewer" : `Show ${items.length - SHORT} more ${noun}`}
        </Button>
      ) : null}
    </div>
  );
}

/** What the scope's routines add up to: how many are on, how the last run went, what waits on the owner. */
function routineSummary(data: RoutinesData, workspaceId: string, now: number) {
  const views = data.routines.filter((v) => v.routine?.workspace_id === workspaceId);
  const enabled = views.filter((v) => v.routine.enabled).length;
  const running = views.find((v) => v.state?.running);
  const at = (run: Json): number => run.finished_ms ?? run.started_ms ?? 0;
  const latest = views
    .flatMap((v) => arr(v.runs).map((run) => ({ run, name: str(v.routine.name) })))
    .sort((a, b) => at(b.run) - at(a.run))[0];
  const needsYou = views.reduce((n, v) => n + arr(v.runs).reduce((m, r) => m + arr(r.needs_you).filter((i) => !i.done).length, 0), 0);
  const failing = views.filter((v) => v.runs?.[0] && !v.runs[0].ok).map((v) => str(v.routine.name));
  const parts = [views.length === 0 ? "No routines here yet" : `${enabled} of ${views.length} routines on`];
  if (running) parts.push(`${str(running.routine.name)} is running now`);
  else if (latest) parts.push(`last: ${latest.name} ${!latest.run.ok ? "failed" : latest.run.quiet ? "was quiet" : "acted"} ${relativeTime(at(latest.run), now)}`);
  else if (views.length) parts.push("none has run yet");
  return { count: views.length, needsYou, failing, text: parts.join(" · ") };
}

/** One group of MCP servers: the whole scope's, or one repository's own. */
function ServerGroup({ label, servers }: { label: string; servers: Json[] }) {
  // A server set up in two files (a folder's .mcp.json and ~/.claude.json) is one server.
  const origins = new Map<string, string[]>();
  for (const s of servers) origins.set(str(s.name), [...(origins.get(str(s.name)) ?? []), str(s.origin)]);
  const names = [...origins.keys()].sort((a, b) => a.localeCompare(b));
  return (
    <div className="flex flex-col gap-1.5">
      <p className="text-[12.5px] font-medium text-foreground/80">{label}</p>
      <ShortList
        items={names}
        noun="servers"
        label={label}
        className="flex flex-wrap gap-1.5"
        render={(name) => (
          <li key={name}>
            <Badge variant="outline" className="font-mono text-[12px] font-normal text-muted-foreground" title={origins.get(name)?.filter(Boolean).join("\n")}>{name}</Badge>
          </li>
        )}
      />
    </div>
  );
}

/** The scope's MCP servers as its folders configure them: the whole scope's first, then each repository's own. */
function Connections({ scope, workspaceName }: { scope: Json; workspaceName: string }) {
  const servers = arr(scope.servers);
  const repos = new Map<string, Json[]>();
  for (const s of servers.filter((s) => s.level === "repo")) {
    const repo = folderName(str(s.repo)) || "one repository";
    repos.set(repo, [...(repos.get(repo) ?? []), s]);
  }
  const whole = servers.filter((s) => s.level !== "repo");
  const groups: { label: string; servers: Json[] }[] = [
    ...(whole.length ? [{ label: "Whole scope", servers: whole }] : []),
    ...[...repos.entries()].sort(([a], [b]) => a.localeCompare(b)).map(([repo, list]) => ({ label: `Only in ${repo}`, servers: list })),
  ];
  return (
    <Row
      title="Connections"
      leading={<Plugs />}
      description={servers.length === 0 ? "No MCP servers in these folders’ config." : "MCP servers from these folders’ own config on disk."}
    >
      {groups.length ? (
        <ShortList
          items={groups}
          noun="repositories"
          label={`Connections for ${workspaceName}`}
          className="flex flex-col gap-3"
          render={(g) => <li key={g.label}><ServerGroup label={g.label} servers={g.servers} /></li>}
        />
      ) : null}
    </Row>
  );
}

function WorkspacePanel({ snapshot, workspace, onEdit, routines, now }: { snapshot: Json; workspace: Workspace; onEdit: () => void; routines: RoutinesState; now: number }) {
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

  const data = routines.data;
  const scope = data?.scopes.find((s) => s.workspace_id === workspace.id);
  const summary = data ? routineSummary(data, workspace.id, now) : null;
  const problems: string[] = arr(scope?.problems).map(str).filter(Boolean);
  const orders = data?.standing_orders?.[workspace.id] ?? "";
  const attention = problems.length > 0 || (summary?.needsYou ?? 0) > 0 || (summary?.failing.length ?? 0) > 0;

  // Wakes the active watches only (a paused or suggested one is refused by the daemon), and
  // one that fails does not stop the rest; the error names the ones that did.
  const syncNow = () =>
    run(async () => {
      let last: Json = null;
      const failed: string[] = [];
      for (const w of watches.filter((w) => w.enabled)) {
        try {
          last = await mcp("Wake", { responsibility_id: w.id });
        } catch (e) {
          failed.push(`${str(w.name) || "A watch"}: ${e instanceof Error ? e.message : String(e)}`);
        }
      }
      if (failed.length) throw new Error(failed.join("\n"));
      return last;
    });

  return (
    <Panel data-slot="workspace" data-workspace={workspace.name} role="region" aria-labelledby={`ws-title-${workspace.id}`}>
      <div className="flex flex-wrap items-center justify-between gap-3 px-6 py-4">
        <div className="flex min-w-0 items-center gap-3">
          <span aria-hidden className={`size-2 shrink-0 rounded-full ${attention ? "bg-warning" : "bg-success"}`} />
          <h3 id={`ws-title-${workspace.id}`} className="truncate text-[17px] font-medium text-strong">{workspace.name}</h3>
          {open > 0 ? <Badge variant="outline" className="font-mono text-[11px] font-normal text-muted-foreground">{open} open</Badge> : null}
        </div>
        <div className="flex items-center gap-2">
          <Button variant="outline" size="sm" onClick={() => void syncNow()} disabled={active === 0 || busy} loading={busy} title={active === 0 ? "Nothing here is being watched right now" : "Check everything this workspace watches"}>
            <ArrowsClockwise /> Sync now
          </Button>
          <Button variant="outline" size="sm" onClick={onEdit} aria-label={`Edit ${workspace.name}`}><PencilSimple /> Edit</Button>
        </div>
      </div>
      <div className="divide-y divide-border/70 border-t border-border/70">
        <Row
          title="Routines"
          leading={<Tray />}
          description={summary ? summary.text : routines.error ? "Couldn’t load routines right now." : "Loading routines…"}
          trailing={summary ? (
            <>
              {summary.needsYou > 0 ? <Status tone="warning">{summary.needsYou} need{summary.needsYou === 1 ? "s" : ""} you</Status>
                : summary.failing.length ? <Status tone="error">{summary.failing.length === 1 ? `${summary.failing[0]} failed` : `${summary.failing.length} failed`}</Status>
                : summary.count ? <Status tone="success">Nothing needs you</Status> : null}
              {summary.count ? <Button variant="ghost" size="sm" aria-label={`Open the Inbox for ${workspace.name}`} render={<a href={hrefFor("inbox")} />}>Inbox <ArrowRight /></Button> : null}
            </>
          ) : null}
        />
        {problems.length ? (
          <Row
            title="Setup notes"
            leading={<Warning className="text-warning" />}
            description={`${problems.length} thing${problems.length === 1 ? "" : "s"} to tidy in the MCP config on disk. Routines still run.`}
          >
            <ShortList items={problems} noun="notes" label={`Setup notes for ${workspace.name}`} className="flex list-disc flex-col gap-1 ps-5" render={(p) => <li key={p}>{p}</li>} />
          </Row>
        ) : null}
        <Row
          title="Folders"
          leading={<FolderSimple />}
          description={folders.length === 0 ? "No folders saved. Edit the workspace to add one." : undefined}
        >
          {folders.length ? (
            <ShortList items={folders} noun="folders" label={`Folders in ${workspace.name}`} className="flex flex-col gap-1" render={(f) => <li key={f} className="truncate font-mono text-[12px] text-foreground/80" title={f}>{f}</li>} />
          ) : null}
        </Row>
        {scope ? <Connections scope={scope} workspaceName={workspace.name} /> : null}
        <Row
          title="Watching"
          leading={<Eye />}
          description={watches.length === 0 ? "Nothing yet. Add a watch in Watching." : `${active} active${watches.length - active > 0 ? ` · ${watches.length - active} paused or suggested` : ""}`}
          trailing={watches.length > 0 ? <Status tone={failing ? "warning" : lastChecked ? "success" : "neutral"}>{failing ? "Needs attention" : lastChecked ? `Synced ${relativeTime(lastChecked)}` : "Not checked yet"}</Status> : null}
        />
        <Row
          title="Tools in Neko"
          leading={<Wrench />}
          description={tools.length === 0 ? "Connect tools in Tools & skills to make them available here." : broken.length ? `${broken.length} need attention` : undefined}
          trailing={schedules.length ? <span className="font-mono text-[12px] text-muted-foreground">{schedules.length} {schedules.length === 1 ? "schedule" : "schedules"}</span> : null}
        >
          {tools.length ? (
            <ShortList
              items={tools}
              noun="tools"
              label={`Tools in Neko for ${workspace.name}`}
              className="flex flex-wrap gap-1.5"
              render={(c) => (
                <li key={c.id}>
                  <Badge variant="outline" className={`text-[12px] font-normal ${c.enabled && str(c.error) ? "text-warning-foreground" : "text-muted-foreground"}`}>{c.label}</Badge>
                </li>
              )}
            />
          ) : null}
        </Row>
        {/* Like the Inbox, which is where they are edited: standing orders matter only where routines run. */}
        {(summary?.count ?? 0) > 0 ? (
          <Row
            title="Standing orders"
            leading={<Note />}
            description="Every routine here follows these."
            trailing={<Button variant="ghost" size="sm" aria-label={`Edit standing orders for ${workspace.name} in the Inbox`} render={<a href={hrefFor("inbox")} />}>Edit in Inbox</Button>}
          >
            <p className="line-clamp-3 max-w-[70ch] whitespace-pre-wrap leading-relaxed text-foreground/80">{orders || "None yet."}</p>
          </Row>
        ) : null}
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
      title={<>Workspaces</>}
      subtitle="Each workspace is a scope: its folders, the MCP servers they set up, and the routines that run there."
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
  const routines = useRoutines();
  const now = useNow();
  return (
    <>
      {routines.error ? (
        <ErrorNote
          title="Couldn’t load scope details"
          error={`${routines.error}${routines.data ? " What follows may be out of date." : " Workspaces still work; routines and connections will show when the daemon answers."}`}
        />
      ) : null}
      {workspaces.length === 0 ? (
        <EmptyBlock icon={Stack} title="No workspaces yet" description="A workspace is a name plus the folders where the work lives. Add one to start tasks, watches and schedules.">
          <Button onClick={() => setDialog({ open: true, id: null })}><Plus /> Add workspace</Button>
        </EmptyBlock>
      ) : (
        <div className="flex flex-col gap-5">
          {workspaces.map((w) => (
            <WorkspacePanel key={w.id} snapshot={snapshot} workspace={w} onEdit={() => setDialog({ open: true, id: w.id })} routines={routines} now={now} />
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
