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
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/ui/dialog";
import { ArrowRight, ArrowSquareOut, ArrowsClockwise, CaretRight, Copy, Eye, FolderSimple, Link as LinkIcon, PencilSimple, Plus, Stack, Tray, Wrench, X } from "@/ui/icons";
import { cn } from "@/ui/lib/utils";
import { ProgressRing, StatusGlyph, type StatusKind } from "@/ui/status-glyph";
import { itemProgress, routineStatus } from "@/setup/routine-status";
import { copyText, useContextMenu } from "@/shell/context-menu";
import { workspaceHref } from "@/shell/workspace";
import { useHashParams } from "@/work/location";
import { hrefFor, navigate } from "@/router";
import { ErrorNote, FormField, Gate, EmptyBlock, RowAction, RowActions, SetupPage, registerCommands } from "@/setup/kit";
import { Panel } from "@/setup/kit";
import { arr, fillOwner, foldersOf, mcp, relativeTime, str, useNow, useRun, type Workspace } from "@/setup/lib";

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

type RoutinesData = { routines: Json[]; standing_orders: Record<string, string>; scopes: Json[]; owner_name?: string };
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
        <button type="button" aria-expanded={all} onClick={() => setAll(!all)} className="rounded-md text-[12.5px] text-muted-foreground outline-none transition-colors hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring/60">
          {all ? "Show fewer" : `Show ${items.length - SHORT} more ${noun}`}
        </button>
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
  const parts = [views.length === 0 ? "No routines here yet" : `${enabled} of ${views.length} on`];
  if (running) parts.push(`${str(running.routine.name)} is running now`);
  else if (latest) parts.push(`last: ${latest.name} ${!latest.run.ok ? "failed" : latest.run.quiet ? "was quiet" : "acted"} ${relativeTime(at(latest.run), now)}`);
  else if (views.length) parts.push("none has run yet");
  return { count: views.length, needsYou, failing, views, text: parts.join(" · ") };
}

/** A status in words with its glyph (DESIGN.md §3); colour only on the glyph. */
function Said({ status, children }: { status: StatusKind; children: ReactNode }) {
  return (
    <span data-slot="said" data-status={status} className="inline-flex items-center gap-1.5 text-[12.5px] text-muted-foreground">
      <StatusGlyph status={status} label="" size={12} />
      {children}
    </span>
  );
}

/**
 * One line of a workspace (DESIGN.md §8): a label, its value in muted text and
 * quiet meta on the right. When it has more to show, the line is a button that
 * opens the rest under it.
 */
function Line({ label, value, meta, children, slot }: { label: string; value: ReactNode; meta?: ReactNode; children?: ReactNode; slot?: string }) {
  const [open, setOpen] = useState(false);
  const more = !!children;
  const head = (
    <>
      <CaretRight aria-hidden size={11} weight="bold" className={cn("shrink-0 text-muted-foreground/70 transition-transform duration-150", open && "rotate-90", !more && "invisible")} />
      <span className="w-32 shrink-0 truncate text-[13.5px] text-foreground">{label}</span>
      <span className="min-w-0 flex-1 truncate text-[12.5px] text-muted-foreground">{value}</span>
    </>
  );
  return (
    <div data-slot={slot ?? "workspace-line"} data-open={open || undefined} className="group/row rounded-[10px] px-3 py-1.5 transition-colors hover:bg-foreground/[0.03]">
      <div className="flex min-h-7 items-center gap-3">
        {more ? (
          <button
            type="button"
            aria-expanded={open}
            aria-label={`${label}: ${open ? "hide" : "show"} details`}
            onClick={() => setOpen(!open)}
            className="flex min-w-0 flex-1 items-center gap-2 rounded-md text-start outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
          >
            {head}
          </button>
        ) : (
          <div className="flex min-w-0 flex-1 items-center gap-2">{head}</div>
        )}
        {meta ? <span className="flex shrink-0 items-center gap-3 text-[12.5px] text-muted-foreground">{meta}</span> : null}
      </div>
      {more && open ? <div className="mt-1.5 mb-1 flex flex-col gap-2 ps-[calc(11px+0.5rem+8rem+0.75rem)] text-[13px] text-muted-foreground">{children}</div> : null}
    </div>
  );
}

/** A quiet link inside a line's meta. */
function MetaLink({ href, label, children }: { href: string; label?: string; children: ReactNode }) {
  return (
    <a href={href} aria-label={label} className="inline-flex items-center gap-1 rounded-md text-[12.5px] text-muted-foreground outline-none transition-colors hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring/60">
      {children}
    </a>
  );
}

/** One group of MCP servers: the whole scope's, or one repository's own. */
function ServerGroup({ label, servers }: { label: string; servers: Json[] }) {
  // A server set up in two files (a folder's .mcp.json and ~/.claude.json) is one server.
  const origins = new Map<string, string[]>();
  for (const s of servers) origins.set(str(s.name), [...(origins.get(str(s.name)) ?? []), str(s.origin)]);
  const names = [...origins.keys()].sort((a, b) => a.localeCompare(b));
  return (
    <div className="flex min-w-0 items-baseline gap-3">
      <span className="w-28 shrink-0 truncate text-[12.5px] text-muted-foreground/80">{label}</span>
      <ShortList
        items={names}
        noun="servers"
        label={label}
        className="flex flex-wrap gap-x-3 gap-y-1"
        render={(name) => (
          <li key={name} className="font-mono text-[12px] text-foreground/80" title={origins.get(name)?.filter(Boolean).join("\n")}>{name}</li>
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
  const names = [...new Set(servers.map((s) => str(s.name)))].sort((a, b) => a.localeCompare(b));
  return (
    <Line
      label="Connections"
      value={servers.length === 0 ? "No MCP servers in these folders’ config" : names.join(", ")}
      meta={servers.length ? <span className="tabular-nums">{names.length}</span> : null}
    >
      {groups.length ? (
        <ShortList
          items={groups}
          noun="repositories"
          label={`Connections for ${workspaceName}`}
          className="flex flex-col gap-1.5"
          render={(g) => <li key={g.label}><ServerGroup label={g.label} servers={g.servers} /></li>}
        />
      ) : null}
    </Line>
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
  // Shown as the agent reads them: `{owner}` is the owner's name.
  const orders = fillOwner(data?.standing_orders?.[workspace.id] ?? "", data?.owner_name);

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

  const menu = useContextMenu(() => [
    { label: "Open in a tab", icon: ArrowSquareOut, run: () => { location.hash = workspaceHref(workspace.id).slice(1); } },
    { label: "Edit…", icon: PencilSimple, run: onEdit },
    { label: "Sync now", icon: ArrowsClockwise, disabled: active === 0 || busy, run: () => void syncNow() },
    "separator",
    { label: "Open Inbox", icon: Tray, run: () => navigate("inbox") },
    { label: "Open Watching", icon: Eye, run: () => navigate("watching") },
    { label: "Open Tools & skills", icon: Wrench, run: () => navigate("tools") },
    "separator",
    { label: "Copy name", icon: Copy, run: () => void copyText(workspace.name, "Copied the name") },
    { label: "Copy link", icon: LinkIcon, run: () => void copyText(`${location.origin}${location.pathname}${workspaceHref(workspace.id)}`, "Copied the link") },
    { label: "Copy id", icon: Copy, run: () => void copyText(workspace.id, "Copied the id") },
  ]);

  const watchState = watches.length === 0 ? null
    : failing ? <span className="text-warning-foreground">Needs attention</span>
    : lastChecked ? `Synced ${relativeTime(lastChecked)}` : "Not checked yet";

  return (
    <section data-slot="workspace" data-workspace={workspace.name} role="region" aria-labelledby={`ws-title-${workspace.id}`} {...menu} className="flex flex-col gap-1">
      <div className="group/row flex min-h-8 items-center gap-3">
        <h2 id={`ws-title-${workspace.id}`} className="min-w-0 truncate text-[15px] font-medium text-foreground">{workspace.name}</h2>
        {open > 0 ? <span className="text-[12.5px] tabular-nums text-muted-foreground/70">{open} open</span> : null}
        <span className="flex-1" />
        <RowActions>
          <RowAction label="Sync now" icon={ArrowsClockwise} disabled={active === 0 || busy} onClick={() => void syncNow()} />
          <RowAction label={`Edit ${workspace.name}`} icon={PencilSimple} onClick={onEdit} />
        </RowActions>
      </div>
      <Panel>
        <Line
          slot="workspace-routines"
          label="Routines"
          value={summary ? summary.text : routines.error ? "Couldn’t load routines right now" : "Loading routines…"}
          meta={summary ? (
            <>
              {summary.needsYou > 0 ? <Said status="inProgress">{summary.needsYou} need{summary.needsYou === 1 ? "s" : ""} you</Said>
                : summary.failing.length ? <Said status="failed">{summary.failing.length === 1 ? `${summary.failing[0]} failed` : `${summary.failing.length} failed`}</Said>
                : null}
              {summary.count ? <MetaLink href={hrefFor("inbox")} label={`Open the Inbox for ${workspace.name}`}>Inbox <ArrowRight size={12} /></MetaLink> : null}
            </>
          ) : null}
        >
          {summary && summary.views.length ? (
            <ul data-slot="routine-summary" aria-label={`Routines in ${workspace.name}`} className="flex flex-wrap gap-x-4 gap-y-1.5">
              {summary.views.map((v) => {
                const [kind, word] = routineStatus(v);
                const parts = itemProgress(v);
                return (
                  <li key={str(v.routine.id)} data-status={kind} className="inline-flex items-center gap-1.5 text-[13px] text-foreground/80">
                    <StatusGlyph status={kind} label={word} />
                    <span>{str(v.routine.name)}</span>
                    {parts.total ? <ProgressRing done={parts.done} total={parts.total} /> : null}
                  </li>
                );
              })}
            </ul>
          ) : null}
        </Line>
        {problems.length ? (
          <Line
            label="Setup notes"
            value={<span className="text-warning-foreground">{problems.length} thing{problems.length === 1 ? "" : "s"} to tidy in the MCP config on disk; routines still run</span>}
          >
            <ShortList items={problems} noun="notes" label={`Setup notes for ${workspace.name}`} className="flex list-disc flex-col gap-1 ps-5" render={(p) => <li key={p}>{p}</li>} />
          </Line>
        ) : null}
        <Line
          label="Folders"
          value={folders.length === 0 ? "No folders saved. Edit the workspace to add one." : <span className="font-mono text-[12px]" title={folders.join("\n")}>{folders.join(", ")}</span>}
          meta={folders.length > 1 ? <span className="tabular-nums">{folders.length}</span> : null}
        >
          {folders.length > 1 ? (
            <ShortList items={folders} noun="folders" label={`Folders in ${workspace.name}`} className="flex flex-col gap-1" render={(f) => <li key={f} className="truncate font-mono text-[12px] text-foreground/80" title={f}>{f}</li>} />
          ) : null}
        </Line>
        {scope ? <Connections scope={scope} workspaceName={workspace.name} /> : null}
        <Line
          label="Watching"
          value={watches.length === 0 ? "Nothing yet. Add a watch in Watching." : `${active} active${watches.length - active > 0 ? ` · ${watches.length - active} paused or suggested` : ""}`}
          meta={<>{watchState}<MetaLink href={hrefFor("watching")} label={`Open Watching for ${workspace.name}`}><ArrowRight size={12} /></MetaLink></>}
        />
        <Line
          label="Tools in Neko"
          value={tools.length === 0 ? "Connect tools in Tools & skills to make them available here." : (
            <>
              {broken.length ? <span className="text-warning-foreground">{broken.length} need{broken.length === 1 ? "s" : ""} attention · </span> : null}
              {tools.map((c) => str(c.label)).join(", ")}
            </>
          )}
          meta={<>
            {schedules.length ? <span className="tabular-nums">{schedules.length} {schedules.length === 1 ? "schedule" : "schedules"}</span> : null}
            <MetaLink href={hrefFor("tools")} label={`Open Tools & skills for ${workspace.name}`}><ArrowRight size={12} /></MetaLink>
          </>}
        >
          {tools.length ? (
            <ShortList
              items={tools}
              noun="tools"
              label={`Tools in Neko for ${workspace.name}`}
              className="flex flex-wrap gap-x-3 gap-y-1"
              render={(c) => (
                <li key={c.id} className={`text-[12.5px] ${c.enabled && str(c.error) ? "text-warning-foreground" : "text-foreground/80"}`} title={str(c.error) || undefined}>{c.label}</li>
              )}
            />
          ) : null}
        </Line>
        {/* Like the Inbox, which is where they are edited: standing orders matter only where routines run. */}
        {(summary?.count ?? 0) > 0 ? (
          <Line
            slot="standing-orders"
            label="Standing orders"
            value={orders || "None yet."}
            meta={<MetaLink href={hrefFor("inbox")} label={`Edit standing orders for ${workspace.name} in the Inbox`}>Edit in Inbox</MetaLink>}
          >
            {orders ? <p className="max-w-[70ch] whitespace-pre-wrap leading-relaxed text-foreground/80">{orders}</p> : null}
          </Line>
        ) : null}
        {workspace.instructions ? (
          <Line label="Instructions" value={workspace.instructions.split("\n")[0]}>
            <p className="max-w-[70ch] whitespace-pre-wrap leading-relaxed text-foreground/80">{workspace.instructions}</p>
          </Line>
        ) : null}
      </Panel>
      {error ? <ErrorNote error={error} title="Couldn’t sync" onDismiss={clear} /> : null}
    </section>
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
      title={<>Workspaces</>}
      actions={<Button size="sm" onClick={() => setDialog({ open: true, id: null })}><Plus /> Add workspace</Button>}
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
  // `#/workspaces?workspace=<id>`: that workspace's own tab (shell/workspace.ts workspaceHref).
  const wanted = useHashParams().get("workspace");
  const only: Workspace | null | undefined = wanted ? workspaces.find((w) => w.id === wanted) ?? undefined : null;
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
      ) : wanted ? (
        <div data-slot="workspace-detail" className="flex flex-col gap-4">
          <a href="#/workspaces" className="self-start rounded text-[13px] text-muted-foreground underline-offset-2 outline-none hover:text-foreground hover:underline focus-visible:ring-2 focus-visible:ring-ring">All workspaces</a>
          {only ? (
            <WorkspacePanel snapshot={snapshot} workspace={only} onEdit={() => setDialog({ open: true, id: only.id })} routines={routines} now={now} />
          ) : (
            <p className="text-[13px] text-muted-foreground">That workspace is not here any more. It may have been removed.</p>
          )}
        </div>
      ) : (
        <div className="flex flex-col gap-8">
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
