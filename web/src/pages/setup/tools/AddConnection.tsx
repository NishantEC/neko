// Adding a connection: by hand (a hosted URL or a trusted local executable),
// from the public MCP registry, or by linking one another app on this Mac
// already configured. Nothing is trusted or linked until you choose it.
import { useEffect, useState } from "react";
import { command, type Json } from "@/api";
import { Button } from "@/ui/button";
import { Input } from "@/ui/input";
import { Checkbox } from "@/ui/checkbox";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/ui/dialog";
import { ArrowSquareOut, ArrowsClockwise, Globe, MagnifyingGlass, Terminal } from "@/ui/icons";
import { ErrorNote, FormField, Pick, Row } from "@/setup/kit";
import { arr, foldersOf, mcp, rec, str, useRun, workspacesOf } from "@/setup/lib";
import { cn } from "@/ui/lib/utils";

export interface ConnectionForm {
  label: string;
  local: boolean;
  global: boolean;
  target: string;
  args: string;
  cwd: string;
  credentials: string;
  trust: boolean;
}
export const emptyForm = (): ConnectionForm => ({ label: "", local: false, global: false, target: "", args: "[]", cwd: "", credentials: "", trust: false });

/** The daemon's connection config, or what to fix first. */
export function configFor(form: ConnectionForm): { config?: Json; invalid?: string } {
  const destination = form.target.trim();
  if (form.local) {
    if (!destination.startsWith("/")) return { invalid: "Use an absolute executable path." };
    let args: unknown;
    try { args = JSON.parse(form.args || "[]"); } catch { args = null; }
    if (!Array.isArray(args) || !args.every((a) => typeof a === "string")) return { invalid: "Arguments must be a JSON array of strings." };
    const cwd = form.cwd.trim();
    if (cwd && !cwd.startsWith("/")) return { invalid: "Choose an existing absolute working directory." };
    return { config: { transport: "stdio", command: destination, args, cwd: cwd || null } };
  }
  try {
    const url = new URL(destination);
    if (!["http:", "https:"].includes(url.protocol) || !url.host) throw new Error("scheme");
  } catch {
    return { invalid: "Enter a complete HTTP or HTTPS server URL." };
  }
  return { config: { transport: "http", url: destination } };
}

function ScopeField({ snapshot, value, onChange }: { snapshot: Json; value: string; onChange: (v: string) => void }) {
  const workspaces = workspacesOf(snapshot);
  return (
    <FormField label="Available in">
      <Pick
        label="Available in"
        value={value || "__all"}
        onChange={(v) => onChange(v === "__all" ? "" : v)}
        options={[{ value: "__all", label: "All workspaces" }, ...workspaces.map((w) => ({ value: w.id, label: w.name }))]}
        className="w-full"
      />
    </FormField>
  );
}

export function ManualConnectionDialog({
  open, onOpenChange, snapshot, scope, onBrowse, onAdded,
}: { open: boolean; onOpenChange: (o: boolean) => void; snapshot: Json; scope: string; onBrowse: () => void; onAdded?: () => void }) {
  const { run, busy, error, clear } = useRun();
  const [form, setForm] = useState<ConnectionForm>(emptyForm());
  const [workspace, setWorkspace] = useState(scope);
  const [invalid, setInvalid] = useState<string | null>(null);
  useEffect(() => {
    if (open) { setForm(emptyForm()); setWorkspace(scope); setInvalid(null); clear(); }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open]);
  // Anything that changes what would run clears the trust you gave.
  const edit = (patch: Partial<ConnectionForm>) => setForm((f) => ({ ...f, ...patch, trust: "target" in patch || "args" in patch || "cwd" in patch || "local" in patch ? false : f.trust }));
  const canSubmit = form.label.trim() && form.target.trim() && (!form.local || form.trust) && !busy;
  const add = async () => {
    const { config, invalid: problem } = configFor(form);
    setInvalid(problem ?? null);
    if (!config) return;
    const saved = await run(() =>
      mcp("AddConnection", {
        workspace_id: workspace,
        label: form.label.trim(),
        config,
        trust_local_process: form.local && form.trust,
        credentials: form.credentials.trim() ? form.credentials.trim() : null,
      }),
    );
    if (saved) { onOpenChange(false); onAdded?.(); }
  };
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-h-[88vh] overflow-y-auto sm:max-w-lg" data-slot="add-connection">
        <DialogHeader>
          <DialogTitle>Add connection</DialogTitle>
          <DialogDescription>Connect a hosted server or a trusted executable on this Mac. Neko asks before chat actions that may change data.</DialogDescription>
        </DialogHeader>
        <form className="flex flex-col gap-5" onSubmit={(e) => { e.preventDefault(); if (canSubmit) void add(); }}>
          <FormField label="Connection name" htmlFor="conn-label">
            <Input id="conn-label" value={form.label} onChange={(e) => edit({ label: e.target.value })} placeholder="Sentry" autoComplete="off" />
          </FormField>
          <ScopeField snapshot={snapshot} value={workspace} onChange={setWorkspace} />
          <FormField label="Connection type">
            <div className="flex gap-2" role="group" aria-label="Connection type">
              {[[false, "Hosted server", Globe], [true, "Local executable", Terminal]].map(([local, text, Icon]: any) => (
                <Button key={String(local)} type="button" variant={form.local === local ? "secondary" : "outline"} size="sm" aria-pressed={form.local === local} onClick={() => edit({ local, target: "" })}>
                  <Icon /> {text}
                </Button>
              ))}
            </div>
          </FormField>
          <FormField label={form.local ? "Absolute executable path" : "Server URL"} htmlFor="conn-target">
            <Input id="conn-target" value={form.target} onChange={(e) => edit({ target: e.target.value })} placeholder={form.local ? "/path/to/executable" : "https://example.com/mcp"} className={cn(form.local && "font-mono")} autoComplete="off" />
          </FormField>
          {form.local ? (
            <>
              <FormField label="Arguments (JSON array)" htmlFor="conn-args">
                <Input id="conn-args" value={form.args} onChange={(e) => edit({ args: e.target.value })} className="font-mono" placeholder="[]" />
              </FormField>
              <FormField label="Working directory · optional" htmlFor="conn-cwd">
                <Input id="conn-cwd" value={form.cwd} onChange={(e) => edit({ cwd: e.target.value })} className="font-mono" placeholder="Default" />
              </FormField>
              <label className="flex items-start gap-2.5 rounded-xl border border-border bg-foreground/[0.03] p-3 text-[13px]">
                <Checkbox checked={form.trust} onCheckedChange={(v) => setForm((f) => ({ ...f, trust: !!v }))} aria-label="I trust this executable and its arguments" className="mt-0.5" />
                <span>
                  I trust this executable and its arguments
                  <span className="mt-0.5 block text-xs text-muted-foreground">Local servers execute code outside worker sandboxes. Nothing is downloaded automatically.</span>
                </span>
              </label>
            </>
          ) : null}
          <FormField label="Credentials · optional" htmlFor="conn-credentials" hint={<>Use {`{"bearer":"…"}`} for HTTP or {`{"environment":{"TOKEN":"…"}}`} for local servers. Stored in Keychain. Keep secrets out of URLs and arguments.</>}>
            <Input id="conn-credentials" type="password" value={form.credentials} onChange={(e) => edit({ credentials: e.target.value })} autoComplete="off" />
          </FormField>
          <ErrorNote error={invalid} title="Check the connection" />
          <ErrorNote error={error} title="Couldn’t add the connection" />
          <DialogFooter className="sm:justify-between">
            <Button type="button" variant="ghost" onClick={onBrowse}>Browse hosted MCP servers</Button>
            <div className="flex gap-2">
              <Button type="button" variant="outline" onClick={() => onOpenChange(false)}>Cancel</Button>
              <Button type="submit" disabled={!canSubmit} loading={busy}>Add connection</Button>
            </div>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}

// ---------------------------------------------------------------------------
// Registry
// ---------------------------------------------------------------------------

const REGISTRY = "https://registry.modelcontextprotocol.io/v0.1/servers";
const SENTRY = { name: "io.github.getsentry/sentry-mcp", shortName: "sentry-mcp", description: "Investigate errors, traces, and issues in Sentry.", url: "https://mcp.sentry.dev/mcp" };
interface RegistryServer { name: string; shortName: string; description: string; url: string }

function parseRegistry(body: Json): RegistryServer[] {
  return arr(body?.servers).flatMap((entry) => {
    const server = rec(entry.server);
    const name = str(server.name);
    const remote = arr(server.remotes).find((r) => r.type === "streamable-http" && /^https:\/\//.test(str(r.url)));
    if (!name || !remote) return [];
    return [{ name, shortName: name.split("/").pop() ?? name, description: str(server.description), url: str(remote.url) }];
  });
}

export function RegistryDialog({
  open, onOpenChange, snapshot, scope,
}: { open: boolean; onOpenChange: (o: boolean) => void; snapshot: Json; scope: string }) {
  const { run, busy, error, clear } = useRun();
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<RegistryServer[]>([]);
  const [searching, setSearching] = useState(false);
  const [searchError, setSearchError] = useState<string | null>(null);
  const [selected, setSelected] = useState<RegistryServer | null>(null);
  const [workspace, setWorkspace] = useState(scope);
  const [added, setAdded] = useState<string | null>(null);
  useEffect(() => {
    if (open) { setQuery(""); setResults([]); setSelected(null); setWorkspace(scope); setAdded(null); setSearchError(null); clear(); }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open]);

  const search = async () => {
    const term = query.trim();
    if (!term) return;
    setSearching(true);
    setSearchError(null);
    try {
      const response = await fetch(`${REGISTRY}?${new URLSearchParams({ search: term, version: "latest", limit: "30" })}`);
      if (!response.ok) throw new Error("The registry is unavailable. Try again or add a server URL manually.");
      setResults(parseRegistry(await response.json()).filter((r) => r.url !== SENTRY.url));
    } catch (e) {
      setResults([]);
      setSearchError(
        e instanceof TypeError
          ? "This page can’t reach the public registry (the daemon only lets the app talk to itself). Open the registry in a new tab, copy a server’s URL, and add it manually."
          : e instanceof Error ? e.message : String(e),
      );
    } finally {
      setSearching(false);
    }
  };

  const add = async (server: RegistryServer) => {
    const exists = arr(snapshot.mcp?.connections).find((c) => str(c.workspace_id) === workspace && rec(c.config).url === server.url);
    if (exists) { setAdded(exists.id); return; }
    const before = new Set(arr(snapshot.mcp?.connections).map((c) => c.id));
    const saved = await run(() => mcp("AddConnection", { workspace_id: workspace, label: server.shortName, config: { transport: "http", url: server.url }, trust_local_process: false, credentials: null }));
    if (saved) setAdded(`new:${[...before].length}:${server.url}`);
  };
  const connection = added
    ? arr(snapshot.mcp?.connections).find((c) => c.id === added || (added.startsWith("new:") && rec(c.config).url === added.split(":").slice(2).join(":") && str(c.workspace_id) === workspace))
    : null;

  const row = (server: RegistryServer, subtitle: string) => (
    <Row
      key={server.name + server.url}
      title={server.shortName}
      description={<>{subtitle}{server.description ? ` · ${server.description}` : ""}</>}
      leading={<Globe />}
      onClick={() => { setSelected(server); setAdded(null); clear(); }}
      label={`${server.name}, ${subtitle}`}
      className={cn(selected?.url === server.url && "bg-foreground/[0.04]")}
    />
  );

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-h-[88vh] overflow-y-auto sm:max-w-xl" data-slot="registry">
        <DialogHeader>
          <DialogTitle>Browse MCP servers</DialogTitle>
          <DialogDescription>Hosted servers from the public registry. Connect only a provider you trust.</DialogDescription>
        </DialogHeader>
        <form className="flex gap-2" onSubmit={(e) => { e.preventDefault(); void search(); }}>
          <div className="relative flex-1">
            <MagnifyingGlass aria-hidden className="pointer-events-none absolute start-2.5 top-1/2 size-3.5 -translate-y-1/2 text-muted-foreground" />
            <Input value={query} onChange={(e) => setQuery(e.target.value)} aria-label="Search the public registry" placeholder="Search the public registry" className="ps-8" />
          </div>
          <Button type="submit" variant="outline" className="h-10" disabled={!query.trim() || searching} loading={searching}>Search</Button>
        </form>
        <ErrorNote error={searchError} title="Registry search" />
        <div className="flex flex-col gap-1">
          <span className="font-mono text-[11px] uppercase tracking-[0.14em] text-muted-foreground">Featured</span>
          <div className="overflow-hidden rounded-xl border border-border divide-y divide-border/70">{row(SENTRY, "Official Sentry server")}</div>
        </div>
        {results.length > 0 ? (
          <div className="flex flex-col gap-1">
            <span className="font-mono text-[11px] uppercase tracking-[0.14em] text-muted-foreground">Registry results · {results.length}</span>
            <div className="overflow-hidden rounded-xl border border-border divide-y divide-border/70">{results.map((r) => row(r, "Community listing · verify publisher"))}</div>
          </div>
        ) : null}
        <a className="inline-flex items-center gap-1.5 text-[13px] text-muted-foreground underline-offset-4 hover:text-foreground hover:underline" href="https://registry.modelcontextprotocol.io" target="_blank" rel="noreferrer noopener">
          Open the registry in a new tab <ArrowSquareOut />
        </a>
        <div className="flex flex-col gap-3 border-t border-border/70 pt-4">
          {connection ? (
            <div className="flex flex-col gap-2">
              <p className="text-[14px] font-medium">{str(connection.label)} added</p>
              <p className="text-[13px] text-muted-foreground">Open it from the list to sign in and discover its tools.</p>
              <div><Button size="sm" onClick={() => onOpenChange(false)}>Done</Button></div>
            </div>
          ) : selected ? (
            <>
              <div>
                <p className="text-[14px] font-medium">{selected.name}</p>
                <p className="select-text break-all font-mono text-xs text-muted-foreground">{selected.url}</p>
              </div>
              <ScopeField snapshot={snapshot} value={workspace} onChange={setWorkspace} />
              <p className="text-xs text-muted-foreground">Its tools may read or change data in the selected workspace. Read-only work sources can start a watch; manage these in Watching.</p>
              <ErrorNote error={error} title="Couldn’t add the connection" />
              <div className="flex justify-end gap-2">
                <Button variant="outline" size="sm" onClick={() => setSelected(null)}>Cancel</Button>
                <Button size="sm" loading={busy} onClick={() => void add(selected)}>Add connection</Button>
              </div>
            </>
          ) : (
            <p className="text-[13px] text-muted-foreground">Select a hosted server to review its publisher, address and workspace access.</p>
          )}
        </div>
      </DialogContent>
    </Dialog>
  );
}

// ---------------------------------------------------------------------------
// Import
// ---------------------------------------------------------------------------

export function ImportDialog({
  open, onOpenChange, snapshot, scope,
}: { open: boolean; onOpenChange: (o: boolean) => void; snapshot: Json; scope: string }) {
  const { run, busy, error, clear } = useRun();
  const workspaces = workspacesOf(snapshot);
  const [workspace, setWorkspace] = useState(scope || workspaces[0]?.id || "");
  const [expanded, setExpanded] = useState<string | null>(null);
  useEffect(() => {
    if (open) { setWorkspace(scope || workspaces[0]?.id || ""); setExpanded(null); clear(); }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open]);
  const target = workspaces.find((w) => w.id === workspace);
  const folders = target ? foldersOf(snapshot, target) : [];
  const connections = arr(snapshot.mcp?.connections).filter((c) => !str(c.workspace_id) || c.workspace_id === workspace);
  const candidates = arr(snapshot.import_preview?.candidates).filter((c) => {
    if (c.kind !== "connection") return false;
    const path = str(c.workspace);
    if (path && !folders.includes(path)) return false;
    return rec(c.metadata).already_in_neko !== "true" && !connections.some((x) => x.source_link?.candidate_id === c.id);
  });
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-h-[88vh] overflow-y-auto sm:max-w-xl" data-slot="import-connections">
        <DialogHeader>
          <DialogTitle>Import from this Mac</DialogTitle>
          <DialogDescription>Review connections already configured in other apps. Nothing is linked until you choose it.</DialogDescription>
        </DialogHeader>
        <div className="flex items-center justify-between gap-3">
          <Pick label="Workspace for imported connection" value={workspace} onChange={setWorkspace} options={workspaces.map((w) => ({ value: w.id, label: w.name }))} />
          <Button variant="outline" size="sm" disabled={busy || !workspace} loading={busy} onClick={() => void run(() => command("SetupImport", { Discover: { repositories: folders, source_id: null } }))}>
            <ArrowsClockwise /> Scan again
          </Button>
        </div>
        <ErrorNote error={error} title="Couldn’t update connections" onDismiss={clear} />
        {candidates.length === 0 ? (
          <p className="rounded-xl border border-dashed border-border px-4 py-8 text-center text-[13px] text-muted-foreground">No new connections found. Scan again after adding a tool in Codex, Claude, or a project folder.</p>
        ) : (
          <div className="divide-y divide-border/70 overflow-hidden rounded-xl border border-border">
            {candidates.map((c) => {
              const local = rec(c.metadata).transport !== "http";
              const problem = str(c.problem);
              const open = expanded === c.id;
              return (
                <div key={c.id}>
                  <Row
                    title={str(c.name)}
                    description={str(c.source) || "Local configuration"}
                    leading={local ? <Terminal /> : <Globe />}
                    trailing={<span className={cn("text-xs", problem ? "text-warning-foreground" : "text-muted-foreground")}>{problem ? "Needs setup" : rec(c.metadata).enabled_at_source === "false" ? "Paused at source" : local ? "Local" : "Remote"}</span>}
                    onClick={() => setExpanded(open ? null : c.id)}
                    label={`${str(c.name)}, ${local ? "local" : "remote"}`}
                  />
                  {open ? (
                    <div className="flex flex-col gap-3 px-6 pb-4 ps-[3.5rem]">
                      {problem ? <ErrorNote error={problem} title="Needs setup" /> : null}
                      <pre className="select-text whitespace-pre-wrap break-all font-mono text-xs text-muted-foreground">{str(rec(c.metadata).config_summary)}</pre>
                      {local ? <p className="text-xs text-muted-foreground">This executable runs on your Mac outside the agent sandbox. Review its command before trusting it.</p> : null}
                      <div>
                        <Button size="sm" disabled={busy || !workspace || !!problem || rec(c.metadata).enabled_at_source === "false"} onClick={() => void run(() => mcp("LinkSource", { workspace_id: workspace, candidate_id: c.id, trust_local_process: local }))}>
                          {local ? "Trust and connect" : "Connect"}
                        </Button>
                      </div>
                    </div>
                  ) : null}
                </div>
              );
            })}
          </div>
        )}
        <DialogFooter><Button variant="outline" onClick={() => onOpenChange(false)}>Done</Button></DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
