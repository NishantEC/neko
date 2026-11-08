// #/customisations: everything that is yours in Neko, in one place. Pages you
// (or Neko's agents) added, built-in pages you replaced or wrapped, and your
// own stylesheet. There is no separate "plugin world": these pages sit in the
// normal sidebar groups and register through the same host as Neko's own (see
// docs/design/one-registry.md). A page is a folder `<data dir>/plugins/<name>/`
// with a manifest.json and an ES module; the list comes straight from the
// daemon (GET /api/plugins) and is managed through POST /api/plugins/<name>/
// enable|disable and DELETE /api/plugins/<name>. `#/plugins` still lands here.
import { useEffect, useMemo, useState } from "react";
import { PageHeader } from "@/editorial";
import { Panel, RowAction, RowActions } from "@/setup/kit";
import { useContextMenu } from "@/shell/context-menu";
import { PageContainer } from "@/shell/PageContainer";
import { Button } from "@/ui/button";
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
  ok: "text-muted-foreground",
  off: "text-muted-foreground/70",
  error: "text-destructive-foreground",
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
        <pre className="max-h-72 overflow-auto whitespace-pre-wrap break-words rounded-[10px] bg-foreground/[0.03] p-3 font-mono text-[12px] text-foreground">{manifest ?? "Reading…"}</pre>
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
  const copyPath = () => void copyText(listing.path as string, "Folder path copied");
  const menu = useContextMenu(() => [
    { label: "Details", icon: Info, run: onDetails },
    { label: "Reload this one", icon: ArrowsClockwise, disabled: busy || listing.disabled, run: onReload },
    { label: listing.disabled ? "Enable" : "Disable", run: () => onToggle(!!listing.disabled) },
    listing.path ? { label: "Copy the folder path", icon: Copy, run: copyPath } : null,
    "separator",
    { label: "Remove…", icon: Trash, disabled: busy, run: onRemove },
  ]);
  return (
    <li data-slot="plugin-row" data-plugin={listing.id} data-status={status.tone} {...menu} className="group/row flex flex-col gap-1 rounded-[10px] px-3 py-1.5 transition-colors hover:bg-foreground/[0.03]">
      <div className="flex min-h-8 items-center gap-3">
        {status.tone === "error" ? <Warning aria-hidden className="size-4 shrink-0 text-destructive-foreground" /> : null}
        <h3 className="shrink-0 text-[13.5px] font-normal text-foreground" title={listing.id}>{label}</h3>
        {listing.manifest_version ? <span className="shrink-0 text-[12px] tabular-nums text-muted-foreground/70">v{listing.manifest_version}</span> : null}
        <span className="min-w-0 flex-1 truncate text-[12.5px] text-muted-foreground" title={listing.description || undefined}>{listing.description}</span>
        <span data-slot="plugin-status" className={cn("shrink-0 text-[12.5px]", TONE[status.tone])}>{status.label}</span>
        <RowActions>
          <RowAction label={`Details of ${label}`} icon={Info} onClick={onDetails} />
          <RowAction label={`Reload ${label}`} icon={ArrowsClockwise} disabled={busy || listing.disabled} onClick={onReload} />
          {listing.path ? <RowAction label={`Copy the folder path of ${label}`} icon={Copy} onClick={copyPath} /> : null}
          <RowAction label={`Remove ${label}`} icon={Trash} disabled={busy} onClick={onRemove} />
        </RowActions>
        <Switch
          checked={!listing.disabled}
          disabled={busy}
          onCheckedChange={(on) => onToggle(!!on)}
          aria-label={`${listing.disabled ? "Enable" : "Disable"} ${label}`}
        />
      </div>
      {entries.length > 0 ? (
        <p className="flex flex-wrap items-center gap-1.5 text-[12px] text-muted-foreground">
          {entries.map((entry) => <EntryLine key={entry.pageId} entry={entry} />)}
        </p>
      ) : null}
      {lastError ? (
        <div role="alert" data-slot="plugin-error" className="flex flex-col gap-0.5">
          <span className="text-[12px] text-destructive-foreground">Last error</span>
          <pre className="select-text whitespace-pre-wrap font-mono text-[12px] text-destructive-foreground">{lastError}</pre>
        </div>
      ) : null}
    </li>
  );
}

function PathRow({ slot, label, path, note, onCopy }: { slot: string; label: string; path: string | undefined; note?: string; onCopy(): void }) {
  return (
    <div className="group/row flex min-h-9 items-center gap-3 rounded-[10px] px-3 py-1 transition-colors hover:bg-foreground/[0.03]">
      <span className="w-20 shrink-0 text-[13.5px] text-foreground">{label}</span>
      <code data-slot={slot} className="min-w-0 flex-1 truncate font-mono text-[12px] text-muted-foreground" title={path}>{path ?? "…"}</code>
      {note ? <span className="shrink-0 text-[12px] text-muted-foreground">{note}</span> : null}
      <RowActions>
        <RowAction label={`Copy the ${label.toLowerCase()} path`} icon={Copy} disabled={!path} onClick={onCopy} />
      </RowActions>
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
          />
          <div className="flex items-center gap-1.5">
            <Button type="button" variant="ghost" size="icon-sm" aria-label="Reload all customisations" title="Reload all" onClick={() => void reloadAll()} disabled={reloading}>
              <ArrowsClockwise className={cn(reloading && "animate-spin")} />
            </Button>
            <Button type="button" size="sm" onClick={() => setBuilding(true)} data-slot="change-neko">
              <Sparkle /> Change Neko
            </Button>
          </div>
        </div>

        <Panel>
          <div className="flex flex-col">
            <PathRow slot="plugin-folder" label="Pages" path={custom?.plugins_dir} onCopy={() => custom && void copyText(custom.plugins_dir, "Folder path copied")} />
            <PathRow
              slot="css-path"
              label="Styles"
              path={custom?.css_path}
              note={custom ? (custom.css_exists ? "In use" : "Not created yet") : undefined}
              onCopy={() => custom && void copyText(custom.css_path, "File path copied")}
            />
          </div>
          <p className="px-3 pt-1 text-[12px] text-muted-foreground/80" title="Restyle anything with the app’s variables: --background, --foreground, --primary, --card, --border, --radius.">
            Go to Folder (⇧⌘G) in Finder opens a path. <code className="font-mono text-[11.5px]">custom.css</code> applies as you save.
          </p>
        </Panel>

        {!listed && !failed ? (
          <Panel>
            <div className="space-y-4 px-3 py-3" aria-busy="true" role="status" aria-label="Loading customisations">
              <Skeleton className="h-5 w-48" />
              <Skeleton className="h-4 w-full" />
              <Skeleton className="h-5 w-40" />
              <Skeleton className="h-4 w-3/4" />
            </div>
          </Panel>
        ) : failed && listings.length === 0 ? (
          <Empty data-slot="plugins-error" className="border-0">
            <EmptyHeader>
              <EmptyMedia variant="icon"><Warning /></EmptyMedia>
              <EmptyTitle>Neko’s daemon did not answer</EmptyTitle>
              <EmptyDescription>The list could not be read. It tries again every few seconds.</EmptyDescription>
            </EmptyHeader>
            <EmptyContent><Button type="button" variant="outline" onClick={() => void reloadAll()}>Try now</Button></EmptyContent>
          </Empty>
        ) : listings.length === 0 ? (
          <Empty data-slot="plugins-empty" className="border-0">
            <EmptyHeader>
              <EmptyMedia variant="icon"><SlidersHorizontal /></EmptyMedia>
              <EmptyTitle>Nothing customised yet</EmptyTitle>
              <EmptyDescription>
                A page of yours is a folder with a <code className="font-mono text-[12px]">manifest.json</code> and an ES module. Give it a built-in page’s id and it replaces that page. Describe what you want and Neko writes it for you.
              </EmptyDescription>
            </EmptyHeader>
            <pre className="max-w-full overflow-x-auto rounded-[10px] bg-foreground/[0.03] px-4 py-3 text-start font-mono text-[12px] text-muted-foreground">{MANIFEST_EXAMPLE}</pre>
            <EmptyContent><Button type="button" onClick={() => setBuilding(true)}><Sparkle /> Change Neko</Button></EmptyContent>
          </Empty>
        ) : (
          <Panel>
            <ul aria-label="Your customisations" className="flex flex-col">
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
            Roll back the web UI in Settings.
          </button>
        </p>
      </>
      <Inspector open={detail !== null} onClose={() => setDetailId(null)} title={detail?.title || detail?.id} subtitle="Customisation">
        {detail ? <Details listing={detail} /> : null}
      </Inspector>
      <ChangeDialog open={building} onOpenChange={setBuilding} places={places} />
    </PageContainer>
  );
}
