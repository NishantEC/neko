// #/plugins: what is installed in Neko's plugin folder, and a way to ask Neko to
// grow a new one. A plugin is a folder `<data dir>/plugins/<name>/` with a
// manifest.json and an ES module; the page lists them straight from the daemon
// (GET /api/plugins) and manages them through POST /api/plugins/<name>/enable|
// disable and DELETE /api/plugins/<name>.
import { useEffect, useMemo, useState } from "react";
import { PageHeader, Panel, MonoLabel } from "@/editorial";
import { Button } from "@/ui/button";
import { Badge } from "@/ui/badge";
import { Switch } from "@/ui/switch";
import { Skeleton } from "@/ui/skeleton";
import { Empty, EmptyContent, EmptyDescription, EmptyHeader, EmptyMedia, EmptyTitle } from "@/ui/empty";
import { ArrowsClockwise, Copy, Info, PuzzlePiece, Sparkle, Trash, Warning } from "@/ui/icons";
import { toast } from "@/ui/sonner";
import { cn } from "@/ui/lib/utils";
import { pluginFolder, reloadPlugin, removePlugin, setPluginEnabled, usePlugins, type PluginListing } from "@/plugins";
import { confirm } from "@/shell/confirm";
import { copyText } from "@/shell/actions";
import { registerCommands } from "@/shell/commands";
import { navigate } from "@/router";
import { Inspector } from "@/shell/inspector";
import { BuildDialog } from "@/pages/plugins/BuildDialog";
import { MANIFEST_EXAMPLE } from "@/pages/plugins/prompt";

type Status = { tone: "ok" | "off" | "error"; label: string };

function statusOf(listing: PluginListing, problem: string | undefined, pages: number): Status {
  if (listing.disabled) return { tone: "off", label: "Off" };
  if (listing.error || problem) return { tone: "error", label: "Error" };
  return { tone: "ok", label: pages > 0 ? `Running · ${pages} ${pages === 1 ? "page" : "pages"}` : "Running" };
}

const TONE: Record<Status["tone"], string> = {
  ok: "bg-success/15 text-success-foreground",
  off: "bg-foreground/[0.07] text-muted-foreground",
  error: "bg-destructive/15 text-destructive-foreground",
};

/** What the inspector shows for a plugin: its manifest as written, and where it lives. */
function PluginDetails({ listing }: { listing: PluginListing }) {
  const [manifest, setManifest] = useState<string | null>(null);
  useEffect(() => {
    setManifest(null);
    fetch(`/plugins/${encodeURIComponent(listing.id)}/manifest.json`)
      .then((r) => (r.ok ? r.text() : Promise.reject(new Error(`HTTP ${r.status}`))))
      .then((text) => {
        try {
          setManifest(JSON.stringify(JSON.parse(text), null, 2));
        } catch {
          setManifest(text);
        }
      })
      .catch((e) => setManifest(`Could not read manifest.json: ${e.message}`));
  }, [listing.id, listing.version]);
  return (
    <div data-slot="plugin-details" className="flex flex-col gap-4 px-4 py-4 text-[13px]">
      {listing.description ? <p className="text-muted-foreground">{listing.description}</p> : null}
      <section className="flex flex-col gap-1">
        <MonoLabel className="text-[10px]">manifest.json</MonoLabel>
        <pre className="max-h-72 overflow-auto whitespace-pre-wrap break-words rounded-xl border border-border bg-foreground/[0.03] p-3 font-mono text-[12px] text-foreground">{manifest ?? "Reading…"}</pre>
      </section>
      {listing.path ? (
        <section className="flex flex-col gap-1">
          <MonoLabel className="text-[10px]">Folder</MonoLabel>
          <code className="break-all font-mono text-[12px] text-muted-foreground">{listing.path}</code>
        </section>
      ) : null}
      <p className="text-[12px] text-muted-foreground">Edit the files in that folder (or ask Neko to); the page reloads by itself a few seconds after a file changes.</p>
    </div>
  );
}

function PluginRow({ listing, problem, pages, busy, onToggle, onReload, onRemove, onDetails }: {
  listing: PluginListing;
  problem: string | undefined;
  pages: { key: string; title: string }[];
  busy: boolean;
  onToggle(enabled: boolean): void;
  onReload(): void;
  onRemove(): void;
  onDetails(): void;
}) {
  const status = statusOf(listing, problem, pages.length);
  const lastError = listing.error ?? problem;
  const label = listing.title || listing.id;
  return (
    <li data-slot="plugin-row" data-plugin={listing.id} data-status={status.tone} className="flex flex-col gap-3 px-5 py-4">
      <div className="flex items-start gap-3.5">
        <span className="mt-0.5 grid size-9 shrink-0 place-items-center rounded-xl bg-foreground/[0.06] text-foreground">
          {status.tone === "error" ? <Warning className="text-warning-foreground" /> : <PuzzlePiece />}
        </span>
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-x-2.5 gap-y-1">
            <h3 className="text-[15px] font-semibold text-strong">{label}</h3>
            {listing.manifest_version ? <Badge variant="outline" className="h-5 rounded-full px-2 font-mono text-[11px] font-normal text-muted-foreground">v{listing.manifest_version}</Badge> : null}
            <span data-slot="plugin-status" className={cn("rounded-full px-2 py-0.5 text-[11.5px] font-medium", TONE[status.tone])}>{status.label}</span>
          </div>
          {listing.description ? <p className="mt-0.5 text-[13px] text-muted-foreground">{listing.description}</p> : null}
          <dl className="mt-2 grid grid-cols-[56px_1fr] gap-x-3 gap-y-0.5 text-[12px]">
            <dt className="text-muted-foreground">Name</dt><dd className="font-mono text-foreground">{listing.id}</dd>
            <dt className="text-muted-foreground">Entry</dt><dd className="truncate font-mono text-foreground">{listing.entry ? listing.entry.replace(`/plugins/${listing.id}/`, "") : "—"}</dd>
            {listing.path ? (<><dt className="text-muted-foreground">Folder</dt><dd className="truncate font-mono text-muted-foreground" title={listing.path}>{listing.path}</dd></>) : null}
          </dl>
          {pages.length > 0 ? (
            <p className="mt-2 flex flex-wrap items-center gap-1.5 text-[12px] text-muted-foreground">
              Pages:
              {pages.map((p) => (
                <button key={p.key} type="button" onClick={() => navigate(p.key)} className="rounded-full bg-foreground/[0.06] px-2 py-0.5 text-foreground outline-none transition-colors hover:bg-foreground/10 focus-visible:ring-2 focus-visible:ring-ring">{p.title}</button>
              ))}
            </p>
          ) : null}
        </div>
        <div className="flex shrink-0 items-center gap-1 self-start">
          <Switch
            checked={!listing.disabled}
            disabled={busy}
            onCheckedChange={(on) => onToggle(!!on)}
            aria-label={`${listing.disabled ? "Enable" : "Disable"} ${label}`}
            className="me-2 mt-1"
          />
          <Button type="button" variant="ghost" size="icon-sm" aria-label={`Details of ${label}`} title="Details" onClick={onDetails}>
            <Info />
          </Button>
          <Button type="button" variant="ghost" size="icon-sm" aria-label={`Reload ${label}`} title="Reload this plugin" disabled={busy || listing.disabled} onClick={onReload}>
            <ArrowsClockwise />
          </Button>
          {listing.path ? (
            <Button type="button" variant="ghost" size="icon-sm" aria-label={`Copy the folder path of ${label}`} title="Copy the folder path" onClick={() => void copyText(listing.path as string, "Folder path copied")}>
              <Copy />
            </Button>
          ) : null}
          <Button type="button" variant="ghost" size="icon-sm" aria-label={`Remove ${label}`} title="Remove this plugin" disabled={busy} className="text-destructive-foreground hover:bg-destructive/10" onClick={onRemove}>
            <Trash />
          </Button>
        </div>
      </div>
      {lastError ? (
        <div role="alert" data-slot="plugin-error" className="rounded-xl border border-destructive/30 bg-destructive/5 px-3 py-2">
          <MonoLabel className="text-[10px] text-destructive-foreground">Last error</MonoLabel>
          <pre className="mt-1 select-text whitespace-pre-wrap font-mono text-[12px] text-destructive-foreground">{lastError}</pre>
        </div>
      ) : null}
    </li>
  );
}

export default function Plugins() {
  const { pages, problems, listings, listed, failed } = usePlugins();
  const [folder, setFolder] = useState<string | null>(null);
  const [building, setBuilding] = useState(false);
  const [reloading, setReloading] = useState(false);
  const [busy, setBusy] = useState<string | null>(null);
  const [detailId, setDetailId] = useState<string | null>(null);
  const detail = listings.find((l) => l.id === detailId) ?? null;

  useEffect(() => {
    void pluginFolder().then(setFolder);
  }, []);
  useEffect(
    () => registerCommands([{ id: "plugins.build", title: "Build a plugin with Neko", keywords: ["create", "extend", "new plugin"], icon: Sparkle, group: "Plugins", run: () => setBuilding(true) }]),
    [],
  );

  const problemFor = useMemo(() => new Map(problems.map((p) => [p.plugin, p.message])), [problems]);

  async function act(id: string, work: () => Promise<void>, done?: string) {
    setBusy(id);
    try {
      await work();
      if (done) toast(done);
    } catch (e) {
      toast.error("That did not work", { description: e instanceof Error ? e.message : String(e) });
    }
    setBusy(null);
  }

  async function remove(listing: PluginListing) {
    const ok = await confirm({
      title: `Remove “${listing.title || listing.id}”?`,
      description: (
        <>
          This deletes its folder{listing.path ? <> (<span className="font-mono text-[12px]">{listing.path}</span>)</> : null} and everything in it. It cannot be undone.
          Turn it off instead if you may want it back.
        </>
      ),
      action: "Remove plugin",
      destructive: true,
    });
    if (ok) await act(listing.id, () => removePlugin(listing.id), `Removed ${listing.title || listing.id}.`);
  }

  async function reloadAll() {
    setReloading(true);
    await reloadPlugin();
    setReloading(false);
    toast("Plugins reloaded.");
  }

  return (
    <div data-slot="plugins-page" className="min-h-0 flex-1 overflow-y-auto">
      <div className="mx-auto flex w-full max-w-3xl flex-col gap-8 px-6 py-10">
        <div className="flex flex-wrap items-end justify-between gap-4">
          <PageHeader
            eyebrow="System"
            title={<>Plugins</>}
            subtitle="Pages and tools that live in a folder on this Mac. Neko’s agents can write them too, and they appear here without a rebuild."
          />
          <div className="flex items-center gap-2">
            <Button type="button" variant="outline" onClick={() => void reloadAll()} disabled={reloading}>
              <ArrowsClockwise className={cn(reloading && "animate-spin")} /> Reload
            </Button>
            <Button type="button" onClick={() => setBuilding(true)} data-slot="build-plugin">
              <Sparkle /> Build a plugin with Neko
            </Button>
          </div>
        </div>

        <Panel>
          <div className="flex items-center gap-3 px-5 py-3">
            <MonoLabel className="text-[10.5px]">Folder</MonoLabel>
            <code data-slot="plugin-folder" className="min-w-0 flex-1 truncate font-mono text-[12.5px] text-foreground" title={folder ?? undefined}>{folder ?? "…"}</code>
            <Button type="button" variant="ghost" size="xs" disabled={!folder} onClick={() => folder && void copyText(folder, "Folder path copied")}>
              <Copy /> Copy path
            </Button>
          </div>
        </Panel>

        {!listed && !failed ? (
          <Panel>
            <div className="space-y-4 p-5" aria-busy="true" role="status" aria-label="Loading plugins">
              <Skeleton className="h-5 w-48" />
              <Skeleton className="h-4 w-full" />
              <Skeleton className="h-5 w-40" />
              <Skeleton className="h-4 w-3/4" />
            </div>
          </Panel>
        ) : failed && listings.length === 0 ? (
          <Empty data-slot="plugins-error" className="border border-dashed">
            <EmptyHeader>
              <EmptyMedia variant="icon"><Warning /></EmptyMedia>
              <EmptyTitle>Neko’s daemon did not answer</EmptyTitle>
              <EmptyDescription>The plugin list could not be read. It tries again every few seconds.</EmptyDescription>
            </EmptyHeader>
            <EmptyContent><Button type="button" variant="outline" onClick={() => void reloadAll()}>Try now</Button></EmptyContent>
          </Empty>
        ) : listings.length === 0 ? (
          <Empty data-slot="plugins-empty" className="border border-dashed">
            <EmptyHeader>
              <EmptyMedia variant="icon"><PuzzlePiece /></EmptyMedia>
              <EmptyTitle>No plugins yet</EmptyTitle>
              <EmptyDescription>
                A plugin is a folder in the plugins folder with a <code className="font-mono text-[12px]">manifest.json</code> and an ES module. Describe what you want and Neko writes one for you.
              </EmptyDescription>
            </EmptyHeader>
            <pre className="max-w-full overflow-x-auto rounded-xl border border-border bg-foreground/[0.03] px-4 py-3 text-start font-mono text-[12px] text-muted-foreground">{MANIFEST_EXAMPLE}</pre>
            <EmptyContent><Button type="button" onClick={() => setBuilding(true)}><Sparkle /> Build a plugin with Neko</Button></EmptyContent>
          </Empty>
        ) : (
          <Panel>
            <ul aria-label="Installed plugins" className="divide-y divide-border/70">
              {listings.map((listing) => (
                <PluginRow
                  key={listing.id}
                  listing={listing}
                  problem={problemFor.get(listing.id)}
                  pages={pages.filter((p) => p.plugin === listing.id).map((p) => ({ key: p.key, title: p.title }))}
                  busy={busy === listing.id}
                  onToggle={(enabled) => void act(listing.id, () => setPluginEnabled(listing.id, enabled), undefined)}
                  onReload={() => void act(listing.id, () => reloadPlugin(listing.id))}
                  onRemove={() => void remove(listing)}
                  onDetails={() => setDetailId(listing.id)}
                />
              ))}
            </ul>
          </Panel>
        )}
      </div>
      <Inspector open={detail !== null} onClose={() => setDetailId(null)} title={detail?.title || detail?.id} subtitle="Plugin">
        {detail ? <PluginDetails listing={detail} /> : null}
      </Inspector>
      <BuildDialog open={building} onOpenChange={setBuilding} folder={folder} />
    </div>
  );
}
