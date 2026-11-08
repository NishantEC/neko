// Tools & skills: connections provide access (MCP servers), skills provide
// instructions. A broken or signed-out connection rises to the top with a clear
// way to reconnect it.
import { useEffect, useMemo, useState } from "react";
import type { Json } from "@/api";
import { Button } from "@/ui/button";
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger } from "@/ui/dropdown-menu";
import { Tabs, TabsList, TabsPanel, TabsTab } from "@/ui/tabs";
import { ArrowDown, Globe, Plus, PlugsConnected, Terminal, Wrench, WarningCircle } from "@/ui/icons";
import { Panel } from "@/setup/kit";
import { navigate } from "@/router";
import { EmptyBlock, Gate, Pick, Row, Section, SetupPage, registerCommands } from "@/setup/kit";
import type { Tone } from "@/setup/lib";

/** A status in words, coloured only when it needs you (DESIGN.md §2). */
const TONE_TEXT: Record<Tone, string> = { neutral: "text-muted-foreground", success: "text-muted-foreground", warning: "text-warning-foreground", error: "text-destructive-foreground" };
import { ScopePicker, useScope } from "@/setup/scope";
import { useSetupCommands } from "@/setup/commands";
import {
  arr, connectionDetail, connectionStatus, connectionTone, rec, sortConnections, str, toolSummary, workspaceName, workspacesOf,
} from "@/setup/lib";
import { ConnectionDetail, useReconnect } from "@/pages/setup/tools/ConnectionDetail";
import { ImportDialog, ManualConnectionDialog, RegistryDialog } from "@/pages/setup/tools/AddConnection";
import { SkillsTab } from "@/pages/setup/tools/Skills";

type Dialogs = "manual" | "registry" | "import" | null;

function ConnectionRow({ snapshot, connection, onOpen }: { snapshot: Json; connection: Json; onOpen: () => void }) {
  const status = connectionStatus(connection);
  const summary = toolSummary(connection);
  const reconnect = useReconnect(connection);
  const scope = str(connection.workspace_id) ? workspaceName(snapshot, connection.workspace_id) : "All workspaces";
  const http = rec(connection.config).transport === "http";
  return (
    <div data-slot="mcp-connection" data-status={status} className="flex items-start">
      <div className="min-w-0 flex-1">
        <Row
          title={str(connection.label)}
          description={connectionDetail(connection)}
          leading={http ? <Globe /> : <Terminal />}
          onClick={onOpen}
          label={`${str(connection.label)}, ${scope}, ${status}, ${summary}`}
          trailing={
            <>
              {summary !== status ? <span className="text-muted-foreground/70">{summary}</span> : null}
              <span className={TONE_TEXT[connectionTone(connection)]}>{status}</span>
            </>
          }
        />
      </div>
      {reconnect.action ? (
        <div className="flex items-center py-2 pe-3">
          <Button size="xs" variant="outline" disabled={reconnect.busy} loading={reconnect.busy} onClick={() => void reconnect.go()}>
            {reconnect.action.label}
          </Button>
        </div>
      ) : null}
    </div>
  );
}

function ToolsBody({ snapshot }: { snapshot: Json }) {
  const scope = useScope(snapshot);
  const workspaces = workspacesOf(snapshot);
  const [dialog, setDialog] = useState<Dialogs>(null);
  const [openId, setOpenId] = useState<string | null>(null);
  const [tab, setTab] = useState("connections");
  const [skillsWorkspace, setSkillsWorkspace] = useState("");
  const connections: Json[] = arr(snapshot.mcp?.connections).filter((c) => !scope || !str(c.workspace_id) || c.workspace_id === scope);
  const sorted = useMemo(() => sortConnections(connections), [connections]);
  const broken = sorted.filter((c) => ["Sign in needed", "Needs attention"].includes(connectionStatus(c)));
  const opened = openId ? arr(snapshot.mcp?.connections).find((c) => c.id === openId) ?? null : null;
  const skillsScope = scope || skillsWorkspace || workspaces[0]?.id || "";

  useEffect(
    () => registerCommands([
      { id: "tools.add", title: "Add a connection", keywords: ["mcp", "server", "tool"], icon: Plus, group: "Tools", run: () => { setTab("connections"); setDialog("manual"); } },
      { id: "tools.browse", title: "Browse MCP servers", keywords: ["registry", "hosted"], icon: Globe, group: "Tools", run: () => { setTab("connections"); setDialog("registry"); } },
    ]),
    [],
  );

  return (
    <>
      <Tabs value={tab} onValueChange={(v) => setTab(String(v))}>
        <div className="flex flex-wrap items-center justify-between gap-3">
          <TabsList aria-label="Tools section">
            <TabsTab value="connections">Connections</TabsTab>
            <TabsTab value="skills">Skills</TabsTab>
          </TabsList>
          <div className="flex items-center gap-2">
            <ScopePicker snapshot={snapshot} label="Show tools for" />
            {tab === "skills" && !scope && workspaces.length > 1 ? (
              <Pick label="Workspace for skills" value={skillsScope} onChange={setSkillsWorkspace} options={workspaces.map((w) => ({ value: w.id, label: w.name }))} />
            ) : null}
            {tab === "connections" ? (
              <DropdownMenu>
                <DropdownMenuTrigger render={<Button size="sm" />}><Plus /> Add connection</DropdownMenuTrigger>
                <DropdownMenuContent align="end">
                  <DropdownMenuItem onClick={() => setDialog("registry")}><Globe /> Browse MCP servers</DropdownMenuItem>
                  <DropdownMenuItem onClick={() => setDialog("import")}><ArrowDown /> Import from this Mac</DropdownMenuItem>
                  <DropdownMenuItem onClick={() => setDialog("manual")}><Plus /> Add connection manually</DropdownMenuItem>
                </DropdownMenuContent>
              </DropdownMenu>
            ) : null}
          </div>
        </div>
        <TabsPanel value="connections" className="flex flex-col gap-6 pt-6">
          {broken.length > 0 ? (
            <div data-slot="attention" className="flex items-center gap-2.5 text-[13px]">
              <WarningCircle aria-hidden className="size-4 shrink-0 text-warning-foreground" />
              <span className="flex-1 text-warning-foreground">
                {broken.length === 1 ? `${str(broken[0].label)} needs attention.` : `${broken.length} connections need attention.`} Agents and watches that use them can’t run until they’re reconnected.
              </span>
            </div>
          ) : null}
          <Section title="Connections" count={connections.length || undefined}>
            {connections.length === 0 ? (
              <EmptyBlock icon={PlugsConnected} title="No connections yet" description="Browse MCP servers, import from this Mac, or add a URL manually.">
                <div className="flex gap-2">
                  <Button variant="outline" onClick={() => setDialog("registry")}>Browse MCP servers</Button>
                  <Button onClick={() => setDialog("manual")}><Plus /> Add connection</Button>
                </div>
              </EmptyBlock>
            ) : (
              <Panel>
                {sorted.map((c) => <ConnectionRow key={str(c.id)} snapshot={snapshot} connection={c} onOpen={() => setOpenId(str(c.id))} />)}
              </Panel>
            )}
          </Section>
        </TabsPanel>
        <TabsPanel value="skills" className="pt-6">
          {skillsScope ? <SkillsTab snapshot={snapshot} workspace={skillsScope} /> : (
            <EmptyBlock icon={Wrench} title="Add a workspace to see its skills" description="Skills are read from a workspace’s folders.">
              <Button variant="outline" onClick={() => navigate("workspaces")}>Open Workspaces</Button>
            </EmptyBlock>
          )}
        </TabsPanel>
      </Tabs>
      <ManualConnectionDialog open={dialog === "manual"} onOpenChange={(o) => setDialog(o ? "manual" : null)} snapshot={snapshot} scope={scope} onBrowse={() => setDialog("registry")} />
      <RegistryDialog open={dialog === "registry"} onOpenChange={(o) => setDialog(o ? "registry" : null)} snapshot={snapshot} scope={scope} />
      <ImportDialog open={dialog === "import"} onOpenChange={(o) => setDialog(o ? "import" : null)} snapshot={snapshot} scope={scope} />
      <ConnectionDetail snapshot={snapshot} connection={opened} onClose={() => setOpenId(null)} />
    </>
  );
}

export default function Tools() {
  useSetupCommands();
  return (
    <SetupPage title={<>Tools &amp; skills</>}>
      <Gate>{(snapshot) => <ToolsBody snapshot={snapshot} />}</Gate>
    </SetupPage>
  );
}
