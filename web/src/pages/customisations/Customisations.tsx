// #/customisations: everything that is yours in Neko, in one place. Pages you
// (or Neko's agents) added, built-in pages you replaced or wrapped, and your
// own stylesheet. There is no separate "plugin world": these pages sit in the
// normal sidebar groups and register through the same host as Neko's own (see
// docs/design/one-registry.md). A page is a folder `<data dir>/plugins/<name>/`
// with a manifest.json and an ES module; the list comes straight from the
// daemon (GET /api/plugins) and is managed through POST /api/plugins/<name>/
// enable|disable and DELETE /api/plugins/<name>. `#/plugins` still lands here.
import { useEffect, useMemo, useState } from "react";
import { PageHeader, Panel } from "@/editorial";
import { PageContainer } from "@/shell/PageContainer";
import { Button } from "@/ui/button";
import { Badge } from "@/ui/badge";
import { Switch } from "@/ui/switch";
import { Skeleton } from "@/ui/skeleton";
import { Empty, EmptyContent, EmptyDescription, EmptyHeader, EmptyMedia, EmptyTitle } from "@/ui/empty";
import { ArrowCounterClockwise, ArrowsClockwise, Copy, Info, SlidersHorizontal, Sparkle, Trash, Warning } from "@/ui/icons";
import { toast } from "@/ui/sonner";
import { cn } from "@/ui/lib/utils";
import { reloadPlugin, removePlugin, setPluginEnabled, usePlugins, type PluginListing } from "@/plugins";
import { setOverrideOff, useUserEntries, type UserEntry } from "@/registry";
import { confirm } from "@/shell/confirm";
import { copyText } from "@/shell/actions";
import { registerCommands } from "@/shell/commands";
import { navigate } from "@/router";
import { Inspector } from "@/shell/inspector";
import { requestSettings } from "@/setup/settingsNav";
import { useWorkbench } from "@/useStore";
import { ChangeDialog } from "@/pages/customisations/ChangeDialog";
import { MANIFEST_EXAMPLE } from "@/pages/customisations/prompt";

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

const LABEL = "text-[12px] font-medium text-muted-foreground";

/** What the inspector shows for a customisation: its manifest as written, and where it lives. */
function Details({ listing }: { listing: PluginListing }) {
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
        <span className={LABEL}>manifest.json</span>
        <pre className="max-h-72 overflow-auto whitespace-pre-wrap break-words rounded-xl border border-border bg-foreground/[0.03] p-3 font-mono text-[12px] text-foreground">{manifest ?? "Reading…"}</pre>
      </section>
      {listing.path ? (
        <section className="flex flex-col gap-1">
          <span className={LABEL}>Folder</span>
          <code className="break-all font-mono text-[12px] text-muted-foreground">{listing.path}</code>
        </section>
      ) : null}
      <p className="text-[12px] text-muted-foreground">Edit the files in that folder (or ask Neko to); the page reloads by itself a few seconds after a file changes.</p>
    </div>
  );
}

/** One page a customisation registered: a link, or for a built-in it replaced, the way back. */
function EntryLine({ entry }: { entry: UserEntry }) {
  const [busy, setBusy] = useState(false);
  if (!entry.overrides) {
    return (
      <button type="button" data-slot="own-page" onClick={() => navigate(entry.route)} className="rounded-full bg-foreground/[0.06] px-2 py-0.5 text-[12px] text-foreground outline-none transition-colors hover:bg-foreground/10 focus-visible:ring-2 focus-visible:ring-ring">
        {entry.title}
      </button>
    );
  }
  const toggle = async () => {
    setBusy(true);
    try {
      await setOverrideOff(entry.overrides!, !entry.off);
      toast(entry.off ? `Using your version of ${entry.title} again.` : `${entry.title} is back to the original.`);
    } catch (e) {
      toast.error("That did not work", { description: e instanceof Error ? e.message : String(e) });
    }
    setBusy(false);
  };
  return (
    <span data-slot="override" data-builtin={entry.overrides} data-off={entry.off ? "true" : "false"} className="inline-flex items-center gap-1.5 rounded-full bg-foreground/[0.06] py-0.5 pe-0.5 ps-2.5 text-[12px] text-foreground">
      {entry.off ? "Original" : entry.wraps ? "Wraps" : "Replaces"} {entry.title}
      <Button type="button" variant="ghost" size="xs" disabled={busy} onClick={() => void toggle()} className="h-5 rounded-full px-2 text-[11.5px]">
        {entry.off ? "Use my version" : (<><ArrowCounterClockwise /> Reset to original</>)}
      </Button>
    </span>
  );
}

function CustomisationRow({ listing, problem, entries, busy, onToggle, onReload, onRemove, onDetails }: {
  listing: PluginListing;
  problem: string | undefined;
  entries: UserEntry[];
  busy: boolean;
  onToggle(enabled: boolean): void;
  onReload(): void;
  onRemove(): void;
  onDetails(): void;
}) {
  const status = statusOf(listing, problem, entries.length);
  const lastError = listing.error ?? problem;
  const label = listing.title || listing.id;
  return (
    <li data-slot="plugin-row" data-plugin={listing.id} data-status={status.tone} className="flex flex-col gap-3 px-6 py-4">
      <div className="flex items-start gap-3.5">
        <span className="mt-0.5 grid size-9 shrink-0 place-items-center rounded-xl bg-foreground/[0.06] text-foreground">
          {status.tone === "error" ? <Warning className="text-warning-foreground" /> : <SlidersHorizontal />}
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
          {entries.length > 0 ? (
            <p className="mt-2 flex flex-wrap items-center gap-1.5 text-[12px] text-muted-foreground">
              Pages:
              {entries.map((entry) => <EntryLine key={entry.pageId} entry={entry} />)}
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
          <Button type="button" variant="ghost" size="icon-sm" aria-label={`Reload ${label}`} title="Reload this one" disabled={busy || listing.disabled} onClick={onReload}>
            <ArrowsClockwise />
          </Button>
          {listing.path ? (
            <Button type="button" variant="ghost" size="icon-sm" aria-label={`Copy the folder path of ${label}`} title="Copy the folder path" onClick={() => void copyText(listing.path as string, "Folder path copied")}>
              <Copy />
            </Button>
          ) : null}
          <Button type="button" variant="ghost" size="icon-sm" aria-label={`Remove ${label}`} title="Remove this" disabled={busy} className="text-destructive-foreground hover:bg-destructive/10" onClick={onRemove}>
            <Trash />
          </Button>
        </div>
      </div>
      {lastError ? (
        <div role="alert" data-slot="plugin-error" className="rounded-xl border border-destructive/30 bg-destructive/5 px-3 py-2">
          <span className="text-[12px] font-medium text-destructive-foreground">Last error</span>
          <pre className="mt-1 select-text whitespace-pre-wrap font-mono text-[12px] text-destructive-foreground">{lastError}</pre>
        </div>
      ) : null}
    </li>
  );
}

function PathRow({ slot, label, path, note, onCopy }: { slot: string; label: string; path: string | undefined; note?: string; onCopy(): void }) {
  return (
    <div className="flex items-center gap-3 px-6 py-3">
      <span className={cn(LABEL, "w-20 shrink-0")}>{label}</span>
      <code data-slot={slot} className="min-w-0 flex-1 truncate font-mono text-[12.5px] text-foreground" title={path}>{path ?? "…"}</code>
      {note ? <span className="shrink-0 text-[12px] text-muted-foreground">{note}</span> : null}
      <Button type="button" variant="ghost" size="xs" disabled={!path} onClick={onCopy}>
        <Copy /> Copy path
      </Button>
    </div>
  );
}

export default function Customisations() {
  const { problems, listings, listed, failed, custom } = usePlugins();
  const entries = useUserEntries();
  const { snapshot } = useWorkbench();
  const [building, setBuilding] = useState(false);
  const [reloading, setReloading] = useState(false);
  const [busy, setBusy] = useState<string | null>(null);
  const [detailId, setDetailId] = useState<string | null>(null);
  const detail = listings.find((l) => l.id === detailId) ?? null;
  const checkout = typeof snapshot?.self_update?.checkout === "string" ? snapshot.self_update.checkout : null;
  const places = custom ? { pluginsDir: custom.plugins_dir, cssPath: custom.css_path, checkout } : null;

  useEffect(
    () => registerCommands([{ id: "customisations.change", title: "Change Neko", keywords: ["create", "extend", "new page", "build a plugin", "customise", "style"], icon: Sparkle, group: "Customisations", run: () => setBuilding(true) }]),
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
      action: "Remove",
      destructive: true,
    });
    if (ok) await act(listing.id, () => removePlugin(listing.id), `Removed ${listing.title || listing.id}.`);
  }

  async function reloadAll() {
    setReloading(true);
    await reloadPlugin();
    setReloading(false);
    toast("Customisations reloaded.");
  }

  return (
    <PageContainer data-slot="plugins-page" className="flex max-w-[52rem] flex-col gap-8">
      <>
        <div className="flex flex-wrap items-end justify-between gap-4">
          <PageHeader
            eyebrow={null}
            title="Customisations"
            subtitle="Your own pages, the built-in pages you replaced, and your styles. They live in a folder on this Mac, and Neko’s agents can change them without a rebuild."
          />
          <div className="flex items-center gap-2">
            <Button type="button" variant="outline" onClick={() => void reloadAll()} disabled={reloading}>
              <ArrowsClockwise className={cn(reloading && "animate-spin")} /> Reload
            </Button>
            <Button type="button" onClick={() => setBuilding(true)} data-slot="change-neko">
              <Sparkle /> Change Neko
            </Button>
          </div>
        </div>

        <Panel>
          <div className="divide-y divide-border/70">
            <PathRow slot="plugin-folder" label="Pages" path={custom?.plugins_dir} onCopy={() => custom && void copyText(custom.plugins_dir, "Folder path copied")} />
            <PathRow
              slot="css-path"
              label="Styles"
              path={custom?.css_path}
              note={custom ? (custom.css_exists ? "In use" : "Not created yet") : undefined}
              onCopy={() => custom && void copyText(custom.css_path, "File path copied")}
            />
          </div>
          <p className="border-t border-border/70 px-6 py-3 text-[12.5px] text-muted-foreground">
            Paste a path into Finder with Go to Folder (⇧⌘G). Styles in <code className="font-mono text-[12px]">custom.css</code> apply as you save; restyle anything with the app’s variables
            (<code className="font-mono text-[12px]">--background</code>, <code className="font-mono text-[12px]">--foreground</code>, <code className="font-mono text-[12px]">--primary</code>, <code className="font-mono text-[12px]">--card</code>, <code className="font-mono text-[12px]">--border</code>, <code className="font-mono text-[12px]">--radius</code>).
          </p>
        </Panel>

        {!listed && !failed ? (
          <Panel>
            <div className="space-y-4 p-5" aria-busy="true" role="status" aria-label="Loading customisations">
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
              <EmptyDescription>The list could not be read. It tries again every few seconds.</EmptyDescription>
            </EmptyHeader>
            <EmptyContent><Button type="button" variant="outline" onClick={() => void reloadAll()}>Try now</Button></EmptyContent>
          </Empty>
        ) : listings.length === 0 ? (
          <Empty data-slot="plugins-empty" className="border border-dashed">
            <EmptyHeader>
              <EmptyMedia variant="icon"><SlidersHorizontal /></EmptyMedia>
              <EmptyTitle>Nothing customised yet</EmptyTitle>
              <EmptyDescription>
                A page of yours is a folder with a <code className="font-mono text-[12px]">manifest.json</code> and an ES module. Give it a built-in page’s id and it replaces that page. Describe what you want and Neko writes it for you.
              </EmptyDescription>
            </EmptyHeader>
            <pre className="max-w-full overflow-x-auto rounded-xl border border-border bg-foreground/[0.03] px-4 py-3 text-start font-mono text-[12px] text-muted-foreground">{MANIFEST_EXAMPLE}</pre>
            <EmptyContent><Button type="button" onClick={() => setBuilding(true)}><Sparkle /> Change Neko</Button></EmptyContent>
          </Empty>
        ) : (
          <Panel>
            <ul aria-label="Your customisations" className="divide-y divide-border/70">
              {listings.map((listing) => (
                <CustomisationRow
                  key={listing.id}
                  listing={listing}
                  problem={problemFor.get(listing.id)}
                  entries={entries.filter((e) => e.plugin === listing.id)}
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

        <p className="text-[12.5px] text-muted-foreground">
          Changes Neko makes to its own source go live after they land, and the previous web UI is kept.{" "}
          <button type="button" onClick={() => { navigate("settings"); requestSettings({ section: "updates" }); }} className="rounded underline underline-offset-2 outline-none hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring">
            Roll back the web UI in Settings
          </button>
          .
        </p>
      </>
      <Inspector open={detail !== null} onClose={() => setDetailId(null)} title={detail?.title || detail?.id} subtitle="Customisation">
        {detail ? <Details listing={detail} /> : null}
      </Inspector>
      <ChangeDialog open={building} onOpenChange={setBuilding} places={places} />
    </PageContainer>
  );
}
