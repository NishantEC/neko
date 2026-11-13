// "This Mac": the quick panel's folders, permissions, diagnostics, Neko
// updates and About.
import { useEffect, useMemo, useState } from "react";
import { command, request } from "@/api";
import { Button } from "@/ui/button";
import { Input } from "@/ui/input";
import { Skeleton } from "@/ui/skeleton";
import { Stethoscope } from "@phosphor-icons/react";
import { ArrowCounterClockwise, ArrowSquareOut, ArrowsClockwise, CheckCircle, FolderSimple, WarningCircle, X } from "@/ui/icons";
import { Panel } from "@/setup/kit";
import { confirm } from "@/shell/confirm";
import { useWorkbench } from "@/useStore";
import { ErrorNote, Row, Section, SettingRow, Status, Toggle } from "@/setup/kit";
import { activate, arr, foldersOf, num, providerRows, rec, relativeTime, str, useLoad, useNow, useRun, workspacesOf } from "@/setup/lib";
const PROJECT_REPOSITORY = "https://github.com/NishantEC/neko"; // privacy-ok: the project's own public repository

// ---------------------------------------------------------------------------
// Quick panel folders
// ---------------------------------------------------------------------------

export function SearchSection() {
  const folders = useLoad((signal) => providerRows("folder-scope", signal));
  const { run, busy, error, clear } = useRun();
  const [path, setPath] = useState("");
  const act = (id: string, action: string | null) =>
    run(async () => { await activate("folder-scope", id, action); folders.reload(); return null; });
  return (
    <div className="flex flex-col gap-8">
      <Section title="Included folders">
        <Panel>
          <div className="flex flex-col" id="setting-folders" data-setting="folders">
            {folders.loading && !folders.data ? (
              <div className="p-5"><Skeleton className="h-4 w-64" /></div>
            ) : arr(folders.data).length === 0 ? (
              <p className="flex items-center gap-2 px-3 py-5 text-[13px] text-muted-foreground"><FolderSimple aria-hidden /> No folders added</p>
            ) : arr(folders.data).map((f) => (
              <Row
                key={str(f.id)}
                title={<span className="select-text break-all font-mono text-[13px] font-normal">{str(f.title)}</span>}
                leading={<FolderSimple />}
                trailing={<Button variant="ghost" size="sm" disabled={busy} aria-label={`Remove ${str(f.title)} from search`} onClick={() => void act(str(f.id), null)}><X /> Remove</Button>}
              />
            ))}
          </div>
          <SettingRow id="add-folder" title="Add a folder" description="An absolute path on this Mac. The daemon checks that it is a folder." stacked>
            <form className="flex w-full gap-2" onSubmit={(e) => { e.preventDefault(); if (path.trim()) void act(path.trim(), "add").then(() => setPath("")); }}>
              <Input aria-label="Folder path" value={path} onChange={(e) => setPath(e.target.value)} placeholder="/Users/you/Documents" className="font-mono" />
              <Button type="submit" className="h-10" disabled={!path.trim() || busy} loading={busy}>Add folder</Button>
            </form>
          </SettingRow>
          {error || folders.error ? <div className="px-3 py-2"><ErrorNote error={error ?? folders.error} onDismiss={clear} /></div> : null}
        </Panel>
      </Section>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Permissions
// ---------------------------------------------------------------------------

const PROTECTED = /^(\/Users\/[^/]+)\/(Desktop|Documents|Downloads|Library\/Mobile Documents)(\/|$)/;

export function PermissionsSection() {
  const { snapshot } = useWorkbench();
  const clipboard = useLoad(async (signal) => !!(await request("GetClipboardHistoryEnabled", signal))?.ClipboardHistoryEnabled?.enabled);
  const { run, busy, error, clear } = useRun();
  const protectedFolders = useMemo(() => {
    const paths = workspacesOf(snapshot).flatMap((w) => foldersOf(snapshot, w));
    const map = new Map<string, number>();
    for (const p of paths) {
      const m = p.match(PROTECTED);
      if (m) map.set(m[2] === "Library/Mobile Documents" ? "iCloud Drive" : m[2], (map.get(m[2] === "Library/Mobile Documents" ? "iCloud Drive" : m[2]) ?? 0) + 1);
    }
    return [...map];
  }, [snapshot]);
  return (
    <div className="flex flex-col gap-8">
      <Section title="What Neko can access">
        <Panel>
          <div className="flex flex-col">
            <SettingRow id="clipboard-history" title="Clipboard history" description="Saves what you copy, only on this Mac. Turning it off stops new capture; existing history stays until you clear it.">
              {clipboard.loading && clipboard.data === null ? <Skeleton className="h-5 w-9" /> : (
                <Toggle
                  checked={!!clipboard.data}
                  label="Clipboard history"
                  disabled={busy || clipboard.data === null}
                  onChange={(enabled) => void run(async () => { await request({ SetClipboardHistoryEnabled: { enabled } }); clipboard.setData(enabled); return null; })}
                />
              )}
            </SettingRow>
            <SettingRow id="full-disk-access" title="Full Disk Access" description="Allows access to protected folders, including Documents, Desktop and iCloud Drive. macOS grants this to the Neko app itself, so it is managed in the native app.">
              <Status tone="neutral">Managed in the native app</Status>
            </SettingRow>
            <SettingRow id="accessibility" title="Accessibility" description="Lets Neko paste for you and read the text you have selected. Nothing else on screen is read. Granted to the native app in System Settings.">
              <Status tone="neutral">Managed in the native app</Status>
            </SettingRow>
          </div>
          {error || clipboard.error ? <div className="px-3 py-2"><ErrorNote error={error ?? clipboard.error} title="Clipboard history" onDismiss={clear} /></div> : null}
        </Panel>
      </Section>
      <Section title="Workspace folders">
        <Panel>
          <div className="flex flex-col">
            {protectedFolders.length === 0 ? (
              <p className="px-3 py-4 text-[13px] text-muted-foreground">None of your workspaces are in a folder macOS protects, so tasks open them without asking.</p>
            ) : protectedFolders.map(([folder, count]) => (
              <Row key={folder} title={folder} description={`${count === 1 ? "1 workspace is" : `${count} workspaces are`} here. macOS asks once, the first time a task opens it, and there is no way to check the answer without asking.`} trailing={<Status tone="success">Asked on first use</Status>} />
            ))}
          </div>
        </Panel>
      </Section>
      <Section title="Agent access">
        <Panel><p className="px-3 py-4 text-[13px] leading-relaxed text-muted-foreground">Neko does not record your screen or control your mouse. Authorized agents are local processes with full filesystem access; their worktrees are separate copies, not a security boundary. Publishing or changing external systems needs separate authority.</p></Panel>
      </Section>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Diagnostics
// ---------------------------------------------------------------------------

export function DiagnosticsSection() {
  const [checks, setChecks] = useState<any[] | null>(null);
  const [roundTrip, setRoundTrip] = useState<number | null>(null);
  const [running, setRunning] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const runChecks = async () => {
    setRunning(true);
    setError(null);
    try {
      const started = performance.now();
      await request("Ping");
      setRoundTrip(Math.round(performance.now() - started));
      setChecks(arr((await request("Diagnostics"))?.Diagnostics?.checks));
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setRunning(false);
    }
  };
  return (
    <div className="flex flex-col gap-8">
      <Section title="Diagnostics">
        <Panel>
          <SettingRow id="diagnostics" title="Run diagnostics" description="Times each runtime and shows which installs Neko uses.">
            <Button onClick={() => void runChecks()} loading={running}><Stethoscope /> {running ? "Running…" : "Run diagnostics"}</Button>
          </SettingRow>
          {roundTrip !== null ? <div className="flex items-center justify-between px-3 py-3.5 text-[13px]"><span className="text-muted-foreground">Web page ↔ daemon</span><span className="font-mono">{roundTrip} ms</span></div> : null}
          {error ? <div className="px-3 py-2"><ErrorNote error={error} title="Diagnostics failed" onDismiss={() => setError(null)} /></div> : null}
          {checks === null && !running ? (
            <p className="border-t border-border/70 px-3 py-6 text-center text-[13px] text-muted-foreground">Run a check to see connection and runtime details.</p>
          ) : null}
          <div className="flex flex-col empty:hidden" data-slot="diagnostic-checks">
            {(checks ?? []).map((c) => (
              <Row
                key={str(c.name)}
                title={str(c.name)}
                leading={c.ok ? <CheckCircle className="text-success" /> : <WarningCircle className="text-warning-foreground" />}
                description={<span className="select-text whitespace-pre-wrap">{str(c.detail)}</span>}
                trailing={num(c.millis) > 0 ? <span className="font-mono text-xs text-muted-foreground">{num(c.millis)} ms</span> : null}
              />
            ))}
          </div>
        </Panel>
      </Section>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Neko updates: the installed app following the published repository
// ---------------------------------------------------------------------------

const shortSha = (sha: unknown) => str(sha).slice(0, 8);
const DEFAULT_SOURCE = PROJECT_REPOSITORY.replace("https://github.com/", "");

/** The folder where Neko's own tasks change Neko. Empty means none. */
function DevCheckoutRow({ current, busy, onSave }: { current: string; busy: boolean; onSave: (path: string) => void }) {
  const [path, setPath] = useState(current);
  useEffect(() => setPath(current), [current]);
  return (
    <SettingRow id="dev-checkout" title="Development checkout" description="Where Neko’s own tasks change Neko. They push to the published main; the updater never touches it." stacked>
      <form className="flex w-full gap-2" onSubmit={(e) => { e.preventDefault(); if (path.trim() !== current) onSave(path.trim()); }}>
        <Input aria-label="Development checkout" value={path} onChange={(e) => setPath(e.target.value)} placeholder="None" className="font-mono" />
        <Button type="submit" variant="outline" className="h-10" disabled={busy || path.trim() === current}>Save</Button>
      </form>
    </SettingRow>
  );
}

export function UpdatesSection() {
  const { snapshot } = useWorkbench();
  const now = useNow();
  const { run, busy, error, clear } = useRun();
  if (!snapshot) return <Panel><div className="p-5"><Skeleton className="h-4 w-64" /></div></Panel>;
  const u = rec(snapshot.self_update);
  const check = rec(u.check);
  const autoUi = u.auto_ui !== false;
  const autoNative = u.auto_native !== false;
  const pending = u.pending ? rec(u.pending) : null;
  const working = u.installing ? `Reinstalling at ${shortSha(rec(u.installing).to)}…` : str(u.status);
  const failure = u.last_failure ? rec(u.last_failure) : null;
  const rollback = u.last_rollback ? rec(u.last_rollback) : null;
  const update = u.last_update ? rec(u.last_update) : null;
  // The newest thing that went wrong, when it is newer than the last success.
  const trouble = [failure, rollback].filter((r): r is Record<string, any> => !!r && num(r.at_ms) > num(update?.at_ms)).sort((x, y) => num(y.at_ms) - num(x.at_ms))[0] ?? null;
  const setAuto = (ui: boolean, native: boolean) => void run(() => command("SetSelfUpdate", { auto_ui: ui, auto_native: native }));
  return (
    <div className="flex flex-col gap-8">
      <Section title="Neko updates">
        <Panel>
          <div className="flex flex-col" data-slot="neko-updates">
            <Row id="setting-update-source" title="Updates come from" description={<span className="select-text font-mono text-xs">{str(u.source) || DEFAULT_SOURCE}</span>} />
            <Row
              id="setting-update-check"
              title="Last check"
              description={<span className="select-text">{check.error ? `Couldn’t reach it: ${str(check.error)}` : num(check.at_ms) > 0 ? relativeTime(num(check.at_ms), now) : "Not yet"}</span>}
              trailing={<Button variant="outline" size="sm" onClick={() => void run(() => command("UpdateNekoNow"))} disabled={busy || !!u.requested || !!u.installing}><ArrowsClockwise /> Update now</Button>}
            />
            <Row
              id="setting-update-version"
              title="In use"
              description={update ? <span className="select-text">{str(update.summary)}</span> : undefined}
              trailing={<>
                <Status tone="neutral">App {u.native_commit ? shortSha(u.native_commit) : "unknown"}</Status>
                <Status tone={u.web_commit ? "success" : "neutral"}>UI {u.web_commit ? shortSha(u.web_commit) : "from the app"}</Status>
              </>}
            />
            {pending ? (
              <Row
                id="setting-pending-update"
                title={`Update waiting: ${shortSha(pending.to)}`}
                description={<span className="select-text">{str(pending.reason)}</span>}
                trailing={<Button size="sm" onClick={() => void run(() => command("ApplyPendingUpdate"))} disabled={busy} loading={busy}><ArrowsClockwise /> Apply update</Button>}
              />
            ) : working ? (
              <Row id="setting-update-pending" title="Now" description={<span data-slot="update-status">{working}</span>} />
            ) : trouble ? (
              <Row
                id="setting-update-failure"
                title={trouble === rollback ? "Rolled back" : "Failed"}
                description={<span className="select-text">{str(trouble.summary)}</span>}
                trailing={<><Status tone={trouble === rollback ? "error" : "warning"}>{shortSha(trouble.commit)}</Status><span className="font-mono text-xs text-muted-foreground">{relativeTime(num(trouble.at_ms), now)}</span></>}
              />
            ) : (
              <Row id="setting-update-pending" title="Pending" description="Nothing waiting or failed." />
            )}
            <SettingRow id="auto-update-ui" title="Update the web UI automatically" description="Builds the published web UI, checks it and swaps it in.">
              <Toggle checked={autoUi} label="Update the web UI automatically" disabled={busy} onChange={(v) => setAuto(v, autoNative)} />
            </SettingRow>
            <SettingRow id="auto-update-app" title="Reinstall the app automatically" description="When Neko is idle and the Mac is calm. An unhealthy update is rolled back.">
              <Toggle checked={autoNative} label="Reinstall the app automatically" disabled={busy} onChange={(v) => setAuto(autoUi, v)} />
            </SettingRow>
            <DevCheckoutRow current={str(u.dev_checkout)} busy={busy} onSave={(path) => void run(() => command("SetDevCheckout", { path }))} />
            <SettingRow id="roll-back-web" title="Roll back the web UI" description="Back to the build it replaced, or the one the app shipped with.">
              <Button
                variant="outline"
                disabled={busy}
                onClick={() => void confirm({
                  title: "Roll back the web UI?",
                  description: "The web UI goes back to the previous build. Pages reload on their own. The next published change brings the newer one back.",
                  action: "Roll back",
                }).then((ok) => ok && run(() => command("RollBackWebUi")))}
              >
                <ArrowCounterClockwise /> Roll back
              </Button>
            </SettingRow>
          </div>
          {error ? <div className="px-3 py-2"><ErrorNote error={error} title="Update" onDismiss={clear} /></div> : null}
        </Panel>
      </Section>
    </div>
  );
}

// ---------------------------------------------------------------------------
// About & updates
// ---------------------------------------------------------------------------

export function AboutSection() {
  const { snapshot, connected } = useWorkbench();
  const tasks = arr(snapshot?.tasks).length;
  return (
    <div className="flex flex-col gap-8">
      <Section title="About">
        <Panel>
          <div className="flex flex-col">
            <Row title="Web interface" description={`Served by the Neko daemon at ${location.origin}. Everything on these pages is read from, and written back to, that daemon.`} trailing={<Status tone={connected ? "success" : "warning"}>{connected ? "Connected" : "Reconnecting…"}</Status>} />
            <Row title="This Mac" description={`${arr(snapshot?.workspaces).length} workspaces · ${tasks} tasks in history · ${arr(snapshot?.mcp?.connections).length} connections`} />
            <SettingRow id="updates" title="Updates and version" description="Neko is built from source, so an update is newer commits on main. The installed app follows the published main by itself (see Neko updates), and the native app can check GitHub too.">
              <Button variant="outline" render={<a href={`${PROJECT_REPOSITORY}/commits/main`} target="_blank" rel="noreferrer noopener" />}>Open commits on GitHub <ArrowSquareOut /></Button>
            </SettingRow>
            <Row title="Update by hand" description="From a source checkout on main with nothing uncommitted:">
              <code className="select-text rounded-lg border border-border bg-foreground/[0.03] px-2.5 py-1.5 font-mono text-xs text-foreground/85">git pull &amp;&amp; bash scripts/install-native.sh</code>
            </Row>
          </div>
        </Panel>
      </Section>
    </div>
  );
}
