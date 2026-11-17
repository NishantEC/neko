// Add pack: choose (a list of the bundled packs, or a folder or URL), then a
// preview of what installing would add, then Install, which is the person's
// approval. The dialog waits for its own answer: its buttons are off while a
// request is in flight, and a refusal shows the daemon's words here.
import { useEffect, useState } from "react";
import { Button } from "@/ui/button";
import { Input } from "@/ui/input";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/ui/dialog";
import { Tabs, TabsList, TabsPanel, TabsTab } from "@/ui/tabs";
import { ErrorNote, FormField } from "@/setup/kit";
import { installPack, previewPack, type CatalogEntry, type PackListing, type PackPreview } from "@/setup/packs";
import { Heading, PackBody } from "@/pages/setup/packs/parts";

function Preview({ preview, env, setEnv, busy, error, onBack, onInstall }: {
  preview: PackPreview;
  env: Record<string, string>;
  setEnv: (name: string, value: string) => void;
  busy: boolean;
  error: string | null;
  onBack: () => void;
  onInstall: () => void;
}) {
  const { info } = preview;
  const ready = info.env.every((name) => (env[name] ?? "").length > 0);
  return (
    <form className="flex flex-col gap-5" data-slot="pack-preview" onSubmit={(e) => { e.preventDefault(); if (ready && !busy) onInstall(); }}>
      <PackBody info={info} installedVersion={preview.installed_version} changes={preview.changes ?? []} />
      {info.env.length > 0 ? (
        <div className="flex flex-col gap-3" data-slot="pack-env">
          <Heading>Values it needs (kept in the Keychain)</Heading>
          {info.env.map((name) => (
            <FormField key={name} label={<span className="font-mono">{name}</span>} htmlFor={`pack-env-${name}`}>
              <Input id={`pack-env-${name}`} type="password" autoComplete="off" value={env[name] ?? ""} onChange={(e) => setEnv(name, e.target.value)} disabled={busy} />
            </FormField>
          ))}
        </div>
      ) : null}
      <ErrorNote error={error} title="Couldn’t install the pack" />
      <DialogFooter>
        <Button type="button" variant="outline" disabled={busy} onClick={onBack}>Back</Button>
        <Button type="submit" loading={busy} disabled={busy || !ready}>Install</Button>
      </DialogFooter>
    </form>
  );
}

export function AddPackDialog({ open, onOpenChange, catalog, onInstalled, autoSource }: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  catalog: CatalogEntry[];
  onInstalled: (listing: PackListing) => void;
  /** A source to preview as soon as the dialog opens (an Improvements suggestion). */
  autoSource?: string | null;
}) {
  const [tab, setTab] = useState("catalog");
  const [source, setSource] = useState("");
  const [preview, setPreview] = useState<PackPreview | null>(null);
  const [env, setEnvValues] = useState<Record<string, string>>({});
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (open) { setTab("catalog"); setSource(""); setPreview(null); setEnvValues({}); setBusy(false); setError(null); if (autoSource) void look(autoSource); }
  }, [open]); // eslint-disable-line react-hooks/exhaustive-deps

  const look = async (from: string) => {
    setBusy(true);
    setError(null);
    try {
      setPreview(await previewPack(from));
      setEnvValues({});
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const install = async () => {
    if (!preview) return;
    setBusy(true);
    setError(null);
    try {
      const values: Record<string, string> = {};
      for (const name of preview.info.env) values[name] = env[name] ?? "";
      const listing = await installPack(preview.source, preview.digest, values);
      setEnvValues({});
      onInstalled(listing);
      onOpenChange(false);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={(o) => { if (!busy) onOpenChange(o); }}>
      <DialogContent className="max-h-[88vh] overflow-y-auto sm:max-w-2xl" data-slot="add-pack">
        <DialogHeader>
          <DialogTitle>{preview ? `Add ${preview.info.title}` : "Add pack"}</DialogTitle>
          <DialogDescription>{preview ? "Check what it will do. Installing is your approval." : "A pack bundles tools, skills, a plugin and routines."}</DialogDescription>
        </DialogHeader>
        {preview ? (
          <Preview preview={preview} env={env} setEnv={(name, value) => setEnvValues((current) => ({ ...current, [name]: value }))} busy={busy} error={error} onBack={() => { setPreview(null); setError(null); }} onInstall={() => void install()} />
        ) : (
          <Tabs value={tab} onValueChange={(v) => { setTab(String(v)); setError(null); }}>
            <TabsList aria-label="Where to add a pack from">
              <TabsTab value="catalog">Catalog</TabsTab>
              <TabsTab value="source">From folder or URL</TabsTab>
            </TabsList>
            <TabsPanel value="catalog" className="flex flex-col gap-3 pt-4">
              {catalog.length === 0 ? (
                <p className="text-[13px] text-muted-foreground">No bundled packs.</p>
              ) : (
                <ul className="flex flex-col" data-slot="pack-catalog">
                  {catalog.map((entry) => (
                    <li key={entry.info.name} data-slot="catalog-entry" className="flex items-start gap-3 rounded-[10px] px-3 py-2.5 hover:bg-foreground/[0.03]">
                      <div className="flex min-w-0 flex-1 flex-col gap-0.5">
                        <div className="flex items-baseline gap-2">
                          <span className="truncate text-[13.5px] text-foreground">{entry.info.title}</span>
                          <span className="text-[12.5px] tabular-nums text-muted-foreground/70">{entry.info.version}</span>
                        </div>
                        {entry.info.description ? <span className="text-[12.5px] text-muted-foreground">{entry.info.description}</span> : null}
                      </div>
                      {entry.installed ? (
                        <span className="shrink-0 pt-1 text-[12.5px] text-muted-foreground">Installed</span>
                      ) : (
                        <Button size="xs" variant="outline" disabled={busy} aria-label={`Add ${entry.info.title}`} onClick={() => void look(entry.source)}>Add</Button>
                      )}
                    </li>
                  ))}
                </ul>
              )}
              <ErrorNote error={tab === "catalog" ? error : null} title="Couldn’t read the pack" />
            </TabsPanel>
            <TabsPanel value="source" className="pt-4">
              <form className="flex flex-col gap-4" onSubmit={(e) => { e.preventDefault(); if (source.trim() && !busy) void look(source.trim()); }}>
                <FormField label="Folder or URL" htmlFor="pack-source" hint="A folder on this Mac, or a repository URL (https or ssh).">
                  <Input id="pack-source" value={source} onChange={(e) => setSource(e.target.value)} placeholder="/path/to/pack or https://example.com/owner/pack.git" autoComplete="off" spellCheck={false} disabled={busy} />
                </FormField>
                <ErrorNote error={tab === "source" ? error : null} title="Couldn’t read the pack" />
                <DialogFooter>
                  <Button type="button" variant="outline" disabled={busy} onClick={() => onOpenChange(false)}>Cancel</Button>
                  <Button type="submit" loading={busy} disabled={busy || !source.trim()}>Preview</Button>
                </DialogFooter>
              </form>
            </TabsPanel>
          </Tabs>
        )}
      </DialogContent>
    </Dialog>
  );
}
