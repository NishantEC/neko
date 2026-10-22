// One connection: its status, sign-in and pause controls, and its tools.
import { useMemo, useState } from "react";
import type { Json } from "@/api";
import { Button } from "@/ui/button";
import { Input } from "@/ui/input";
import { Badge } from "@/ui/badge";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/ui/collapsible";
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "@/ui/dialog";
import { ArrowsClockwise, CaretDown, CaretRight, Key, MagnifyingGlass, Pause, Play } from "@/ui/icons";
import { JsonViewer } from "@/chat/json-viewer";
import { ErrorNote, Status } from "@/setup/kit";
import { confirm } from "@/shell/confirm";
import { toast } from "@/ui/sonner";
import { arr, connectionStatus, connectionTone, mcp, rec, str, toolSummary, useRun, workspaceName } from "@/setup/lib";

export function needsSignIn(c: Json) {
  return connectionStatus(c) === "Sign in needed";
}

/** What "reconnect" means for this connection: sign in again, approve a changed
 * source definition again, or discover its tools again. Null when nothing the
 * button could do would help (paused, or the source definition is gone). */
export function reconnectAction(c: Json): { label: string; kind: "signin" | "relink" | "discover" } | null {
  if (!c.enabled) return null;
  const status = connectionStatus(c);
  if (status !== "Sign in needed" && status !== "Needs attention") return null;
  const error = str(c.error).toLowerCase();
  if (error.includes("definition is unavailable")) return null;
  if (error.includes("definition changed") && c.source_link) return { label: "Review & approve", kind: "relink" };
  if (rec(c.config).transport === "http" && status === "Sign in needed") return { label: "Sign in again", kind: "signin" };
  return { label: "Reconnect", kind: "discover" };
}

/** Runs the reconnect for a connection and says what really happened: the
 * daemon answers with a snapshot, and the connection's own error in it is the
 * truth, not the fact that the request was accepted. */
export function useReconnect(c: Json) {
  const { run, busy, error, clear } = useRun();
  const action = reconnectAction(c);
  const go = async () => {
    if (!action) return;
    const label = str(c.label);
    if (action.kind === "relink") {
      const config = rec(c.config);
      const local = config.transport !== "http";
      const what = local ? [str(config.command), ...arr(config.args).map(String)].join(" ") : str(config.url);
      const ok = await confirm({
        title: `Approve ${label} again?`,
        description: (
          <span className="flex flex-col gap-2">
            <span>Its definition in {str(c.source_link?.source_path) || "its source file"} no longer matches what you approved. Neko will run it as it is now:</span>
            <code className="block select-text break-all rounded-lg bg-foreground/[0.05] px-3 py-2 font-mono text-[12px] text-foreground">{what}</code>
            {local ? <span>This is a program on this Mac. Approve it only if you recognise it.</span> : null}
          </span>
        ),
        action: "Approve again",
      });
      if (!ok) return;
    }
    let after: Json | null = null;
    const done = await run(async () => {
      if (action.kind === "relink") {
        await mcp("LinkSource", { workspace_id: c.workspace_id, candidate_id: c.source_link.candidate_id, trust_local_process: rec(c.config).transport !== "http" });
      }
      const result = action.kind === "signin"
        ? await mcp("Authenticate", { connection_id: c.id, client_id: null })
        : await mcp("Discover", { connection_id: c.id });
      after = result;
      return result;
    });
    if (!done) return;
    if (action.kind === "signin") return toast.success(`Finish signing in to ${label} in your browser.`);
    const latest = arr((after as Json | null)?.mcp?.connections).find((x) => x.id === c.id);
    const problem = latest ? str(latest.error) : "";
    if (problem) toast.error(`${label} still can’t connect: ${problem}`);
    else toast.success(`${label} is connected${latest ? ` · ${toolSummary(latest)}` : ""}.`);
  };
  return { action, go, busy, error, clear };
}

function ToolRow({ tool }: { tool: Json }) {
  const [open, setOpen] = useState(false);
  const schema = useMemo(() => {
    const raw = tool.input_schema;
    if (typeof raw !== "string" || !raw) return null;
    try { return JSON.parse(raw); } catch { return raw; }
  }, [tool.input_schema]);
  return (
    <Collapsible open={open} onOpenChange={setOpen}>
      <CollapsibleTrigger
        render={<button type="button" className="flex w-full items-start gap-2.5 px-1 py-3 text-start outline-none transition-colors hover:bg-foreground/[0.04] focus-visible:ring-2 focus-visible:ring-ring" aria-label={`${str(tool.name)}, ${tool.read_only ? "declared read-only" : "may change data"}`} />}
      >
        <span aria-hidden className="mt-0.5 text-muted-foreground">{open ? <CaretDown /> : <CaretRight />}</span>
        <span className="flex min-w-0 flex-1 flex-col gap-0.5">
          <span className="flex items-center gap-2">
            <span className="font-mono text-[13px] font-medium text-foreground">{str(tool.name)}</span>
            <Badge variant="outline" className={`text-[11px] font-normal ${tool.read_only ? "text-muted-foreground" : "text-warning-foreground"}`}>{tool.read_only ? "Read-only" : "May change data"}</Badge>
          </span>
          {str(tool.description) && !open ? <span className="line-clamp-2 text-[13px] text-muted-foreground">{str(tool.description)}</span> : null}
        </span>
      </CollapsibleTrigger>
      <CollapsibleContent>
        <div className="flex flex-col gap-3 pb-4 ps-7 text-[13px] text-muted-foreground">
          {str(tool.description) ? <p className="select-text whitespace-pre-wrap">{str(tool.description)}</p> : null}
          <p>{tool.read_only ? "The server declares this tool read-only. Chat lookups can run without another prompt." : "Chat asks before each call. Turned-on watches may use their selected connections unattended."}</p>
          {schema ? (
            <div className="flex flex-col gap-1.5">
              <span className="text-[12.5px] font-medium">Input schema</span>
              <div className="max-h-72 overflow-auto rounded-xl border border-border bg-foreground/[0.03] p-3">
                {typeof schema === "string" ? <pre className="whitespace-pre-wrap font-mono text-xs">{schema}</pre> : <JsonViewer data={schema} rootName="input" defaultExpanded />}
              </div>
            </div>
          ) : null}
        </div>
      </CollapsibleContent>
    </Collapsible>
  );
}

export function ConnectionDetail({ snapshot, connection, onClose }: { snapshot: Json; connection: Json | null; onClose: () => void }) {
  const { run, busy, error, clear } = useRun();
  const [search, setSearch] = useState("");
  const [clientId, setClientId] = useState("");
  const c = connection;
  // A hook, so it runs before the early return (React #310 otherwise, when a connection is first picked).
  const reconnect = useReconnect(c ?? {});
  if (!c) return null;
  const config = rec(c.config);
  const tools = arr(c.tools);
  const filtered = tools.filter((t) => !search || `${str(t.name)} ${str(t.description)}`.toLowerCase().includes(search.toLowerCase()));
  const status = connectionStatus(c);
  const send = (name: string, fields: Record<string, unknown>) => void run(() => mcp(name, fields));
  return (
    <Dialog open onOpenChange={(o) => { if (!o) { setSearch(""); clear(); onClose(); } }}>
      <DialogContent className="max-h-[88vh] overflow-y-auto sm:max-w-2xl" data-slot="connection-detail">
        <DialogHeader>
          <DialogTitle className="text-lg">{str(c.label)}</DialogTitle>
          <DialogDescription>{status} · {toolSummary(c)}</DialogDescription>
        </DialogHeader>
        <div className="flex flex-col gap-5">
          {str(c.error) ? (
            <ErrorNote title={status === "Sign in needed" ? "Signed out" : "This connection needs attention"} error={str(c.error)} />
          ) : null}
          <div className="flex flex-wrap items-center gap-2">
            <Status tone={connectionTone(c)}>{status}</Status>
            <span className="flex-1" />
            {reconnect.action ? (
              <Button size="sm" disabled={busy || reconnect.busy} loading={reconnect.busy} onClick={() => void reconnect.go()}>
                {reconnect.action.kind === "signin" ? <Key /> : <ArrowsClockwise />} {reconnect.action.label}
              </Button>
            ) : null}
            <Button variant="outline" size="sm" disabled={busy || !c.enabled} onClick={() => send("Discover", { connection_id: c.id })}><ArrowsClockwise /> Refresh tools</Button>
            <Button variant="outline" size="sm" disabled={busy} onClick={() => send("SetEnabled", { connection_id: c.id, enabled: !c.enabled })}>
              {c.enabled ? <><Pause /> Pause connection</> : <><Play /> Enable connection</>}
            </Button>
            {config.transport === "http" && !reconnect ? (
              <Button variant="outline" size="sm" disabled={busy || !c.enabled} onClick={() => send("Authenticate", { connection_id: c.id, client_id: clientId.trim() || null })}><Key /> Sign in</Button>
            ) : null}
          </div>
          <ErrorNote error={error} title="Couldn’t update the connection" onDismiss={clear} />
          <Collapsible>
            <CollapsibleTrigger render={<Button variant="ghost" size="sm" className="-ms-2" />}>Connection details <CaretDown /></CollapsibleTrigger>
            <CollapsibleContent>
              <dl className="grid grid-cols-[9rem_1fr] gap-x-4 gap-y-2 pt-2 text-[13px]">
                <dt className="text-muted-foreground">Workspace</dt><dd>{str(c.workspace_id) ? workspaceName(snapshot, c.workspace_id) : "All workspaces"}</dd>
                <dt className="text-muted-foreground">{config.transport === "http" ? "URL" : "Command"}</dt><dd className="select-text break-all font-mono text-xs">{str(config.url) || str(config.command)}</dd>
                {config.transport !== "http" ? (<>
                  <dt className="text-muted-foreground">Arguments</dt><dd className="select-text break-all font-mono text-xs">{arr(config.args).map(str).join(" ") || "None"}</dd>
                  {str(config.cwd) ? (<><dt className="text-muted-foreground">Working directory</dt><dd className="select-text break-all font-mono text-xs">{str(config.cwd)}</dd></>) : null}
                  <dt className="text-muted-foreground">Trust</dt><dd>{c.trusted ? "Trusted local executable" : "Local executable trust required"}</dd>
                </>) : (<>
                  <dt className="text-muted-foreground"><label htmlFor="oauth-client">OAuth client ID</label></dt>
                  <dd><Input id="oauth-client" value={clientId} onChange={(e) => setClientId(e.target.value)} placeholder="Only if the server asks for one" className="h-8" /></dd>
                </>)}
                <dt className="text-muted-foreground">Credentials</dt><dd>{c.has_credentials ? "Stored in Keychain" : "None"}</dd>
              </dl>
            </CollapsibleContent>
          </Collapsible>
          <div className="flex flex-col gap-2">
            <div className="flex items-center justify-between gap-3">
              <h4 className="text-[14px] font-medium">Tools</h4>
              {tools.length > 0 ? (
                <div className="relative w-56">
                  <MagnifyingGlass aria-hidden className="pointer-events-none absolute start-2.5 top-1/2 size-3.5 -translate-y-1/2 text-muted-foreground" />
                  <Input value={search} onChange={(e) => setSearch(e.target.value)} aria-label="Search tools" placeholder="Search tools" className="h-8 ps-8!" />
                </div>
              ) : null}
            </div>
            {tools.length === 0 ? (
              <p className="rounded-xl border border-dashed border-border px-4 py-6 text-center text-[13px] text-muted-foreground">
                {!c.enabled ? "Enable this connection to discover its tools." : status === "Sign in needed" ? "Sign in to discover this server’s tools." : str(c.error) ? "Fix the problem above, then refresh tools." : "Refresh tools to discover what this server offers."}
              </p>
            ) : filtered.length === 0 ? (
              <p className="px-1 py-4 text-[13px] text-muted-foreground">No tools match “{search}”.</p>
            ) : (
              <div className="divide-y divide-border/70 rounded-xl border border-border px-3">
                {filtered.map((t) => <ToolRow key={str(t.name)} tool={t} />)}
              </div>
            )}
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}
