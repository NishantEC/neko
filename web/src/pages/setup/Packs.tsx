// Packs: one installable unit that bundles tools, skills, a plugin and routines.
// The list shows what is installed and how it is doing; Add pack previews a pack
// (nothing changes until you say Install). Every request is the daemon's
// `Packs` command (neko-protocol packs.rs); it answers with the whole listing,
// which replaces what is shown.
import { useCallback, useEffect, useRef, useState } from "react";
import { Button } from "@/ui/button";
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuSeparator, DropdownMenuTrigger } from "@/ui/dropdown-menu";
import { toast } from "@/ui/sonner";
import { StatusGlyph } from "@/ui/status-glyph";
import { ArrowUp, ArrowsClockwise, DotsThree, Eye, Plus, Trash } from "@/ui/icons";
import { ErrorNote, LoadingRows, Panel, RowAction, RowActions, Section, SetupPage, registerCommands } from "@/setup/kit";
import { useContextMenu } from "@/shell/context-menu";
import { confirm } from "@/shell/confirm";
import { useSetupCommands } from "@/setup/commands";
import {
  PACK_STATUS_WORDS, checkPackHealth, cleanUpPack, listPacks, packGlyph, pluginIdOf, previewPack, removePack, shortSource,
  type InstalledPack, type PackListing,
} from "@/setup/packs";
import { setPluginEnabled } from "@/plugins";
import { AddPackDialog } from "@/pages/setup/packs/AddPack";
import { PackDetail } from "@/pages/setup/packs/PackDetail";
import { ReviewPackDialog, type Review } from "@/pages/setup/packs/ReviewPack";

/** The listing is also reloaded on this slow interval, in case a pack changed outside this page. */
const REFRESH_MS = 30_000;

type Act = "check" | "update" | "review" | "remove" | "cleanup";

function PackRow({ pack, busy, error, onOpen, onAct, onDismissError }: {
  pack: InstalledPack;
  busy: boolean;
  error: string | null;
  onOpen: () => void;
  onAct: (act: Act) => void;
  onDismissError: () => void;
}) {
  const title = pack.info.title;
  const update = pack.update_available ?? null;
  const changed = pack.status === "Changed";
  const menu = useContextMenu(() => [
    { label: "Check health", icon: ArrowsClockwise, disabled: busy, run: () => onAct("check") },
    changed ? { label: "Review…", icon: Eye, disabled: busy, run: () => onAct("review") } : null,
    update ? { label: `Update to ${update}`, icon: ArrowUp, disabled: busy, run: () => onAct("update") } : null,
    "separator",
    { label: "Remove…", icon: Trash, destructive: true, disabled: busy, run: () => onAct("remove") },
  ]);
  return (
    <div data-slot="pack" data-pack={pack.info.name} data-status={pack.status} {...menu} className="group/row flex flex-col gap-1 rounded-[10px] px-3 py-1.5 transition-colors hover:bg-foreground/[0.03]">
      <div className="flex min-h-8 items-center gap-3">
        <StatusGlyph status={packGlyph(pack.status)} label={PACK_STATUS_WORDS[pack.status]} />
        <button
          type="button"
          onClick={onOpen}
          aria-label={`${title}, ${PACK_STATUS_WORDS[pack.status]}, version ${pack.info.version}. Open details`}
          className="flex min-w-0 flex-1 items-baseline gap-3 rounded-md text-start outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
        >
          <span className="min-w-0 shrink-0 truncate text-[13.5px] text-foreground">{title}</span>
          <span className="shrink-0 text-[12.5px] tabular-nums text-muted-foreground/60">{pack.info.version}</span>
          <span className="min-w-0 flex-1 truncate text-[12.5px] text-muted-foreground" title={pack.source}>{shortSource(pack.source)}</span>
        </button>
        {changed ? (
          <Button size="xs" variant="outline" disabled={busy} aria-label={`Review ${title}`} onClick={() => onAct("review")}>Review</Button>
        ) : null}
        {update ? (
          <Button size="xs" variant="outline" disabled={busy} aria-label={`Update ${title} to ${update}`} onClick={() => onAct("update")}>Update to {update}</Button>
        ) : null}
        <RowActions>
          <RowAction label={`Check health of ${title}`} icon={ArrowsClockwise} disabled={busy} onClick={() => onAct("check")} />
          <DropdownMenu>
            <DropdownMenuTrigger render={<Button variant="ghost" size="icon-sm" aria-label={`Actions for ${title}`} title="More actions" className="text-muted-foreground" />}><DotsThree weight="bold" /></DropdownMenuTrigger>
            <DropdownMenuContent align="end">
              <DropdownMenuItem disabled={busy} onClick={() => onAct("check")}><ArrowsClockwise /> Check health</DropdownMenuItem>
              {changed ? <DropdownMenuItem disabled={busy} onClick={() => onAct("review")}><Eye /> Review…</DropdownMenuItem> : null}
              {update ? <DropdownMenuItem disabled={busy} onClick={() => onAct("update")}><ArrowUp /> Update to {update}</DropdownMenuItem> : null}
              <DropdownMenuSeparator />
              <DropdownMenuItem disabled={busy} onClick={() => onAct("remove")} className="text-destructive-foreground"><Trash /> Remove…</DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        </RowActions>
      </div>
      <ErrorNote error={error} onDismiss={onDismissError} />
    </div>
  );
}

function PacksBody() {
  const [listing, setListing] = useState<PackListing | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [adding, setAdding] = useState(false);
  const [openName, setOpenName] = useState<string | null>(null);
  const [busyName, setBusyName] = useState<string | null>(null);
  const [errors, setErrors] = useState<Record<string, string>>({});
  const [review, setReview] = useState<Review | null>(null);
  const busyRef = useRef(false);

  const reload = useCallback(async (signal?: AbortSignal) => {
    try {
      const next = await listPacks(signal);
      if (signal?.aborted || busyRef.current) return;
      setListing(next);
      setLoadError(null);
    } catch (e) {
      if (!signal?.aborted) setLoadError(e instanceof Error ? e.message : String(e));
    }
  }, []);

  // On open, and slowly after that.
  useEffect(() => {
    const controller = new AbortController();
    void reload(controller.signal);
    const id = setInterval(() => void reload(controller.signal), REFRESH_MS);
    return () => { controller.abort(); clearInterval(id); };
  }, [reload]);

  useEffect(
    () => registerCommands([{ id: "packs.add", title: "Add a pack", keywords: ["install", "bundle", "tools", "skills", "plugin"], icon: Plus, group: "Packs", run: () => setAdding(true) }]),
    [],
  );

  const act = async (pack: InstalledPack, what: Act) => {
    const name = pack.info.name;
    if (busyRef.current) return;
    if (what === "remove") {
      const ok = await confirm({
        title: `Remove ${pack.info.title}?`,
        description: "Its connections, skills, plugin and routines are removed. A copy is kept in the cleanup archive.",
        action: "Remove",
        destructive: true,
      });
      if (!ok) return;
    }
    busyRef.current = true;
    setBusyName(name);
    setErrors((e) => { const { [name]: _gone, ...rest } = e; return rest; });
    try {
      if (what === "update" || what === "review") {
        // Read what the pack would do now; the dialog sends that preview's digest.
        const preview = await previewPack(what === "review" ? `installed:${name}` : pack.source);
        setReview({ kind: what, name, preview });
        return;
      }
      const next = await (what === "check" ? checkPackHealth(name) : what === "cleanup" ? cleanUpPack(name) : removePack(name));
      setListing(next);
      setLoadError(null);
      if (what === "remove") {
        setOpenName((current) => (current === name ? null : current));
        toast.success(`Removed ${pack.info.title}.`);
      }
    } catch (e) {
      setErrors((current) => ({ ...current, [name]: e instanceof Error ? e.message : String(e) }));
    } finally {
      busyRef.current = false;
      setBusyName(null);
    }
  };

  const plugin = async (pack: InstalledPack, enable: boolean) => {
    const name = pack.info.name;
    if (busyRef.current) return;
    busyRef.current = true;
    setBusyName(name);
    setErrors((e) => { const { [name]: _gone, ...rest } = e; return rest; });
    try {
      await setPluginEnabled(pluginIdOf(pack.plugin_dir), enable);
      busyRef.current = false;
      await reload();
    } catch (e) {
      setErrors((current) => ({ ...current, [name]: e instanceof Error ? e.message : String(e) }));
    } finally {
      busyRef.current = false;
      setBusyName(null);
    }
  };

  const installed = listing?.installed ?? [];
  const opened = openName ? installed.find((p) => p.info.name === openName) ?? null : null;
  const dismiss = (name: string) => setErrors((e) => { const { [name]: _gone, ...rest } = e; return rest; });

  return (
    <SetupPage title="Packs" count={installed.length || undefined} actions={<Button size="sm" onClick={() => setAdding(true)}><Plus /> Add pack</Button>}>
      {loadError && !listing ? <ErrorNote title="Couldn’t load packs" error={loadError} /> : null}
      {!listing && !loadError ? <LoadingRows rows={3} /> : null}
      {listing ? (
        <Section title="Installed" count={installed.length || undefined}>
          {installed.length === 0 ? (
            <p data-slot="packs-empty" className="text-[13px] text-muted-foreground">No packs installed.</p>
          ) : (
            <Panel>
              {installed.map((p) => (
                <PackRow
                  key={p.info.name}
                  pack={p}
                  busy={busyName === p.info.name}
                  error={errors[p.info.name] ?? null}
                  onOpen={() => setOpenName(p.info.name)}
                  onAct={(what) => void act(p, what)}
                  onDismissError={() => dismiss(p.info.name)}
                />
              ))}
            </Panel>
          )}
        </Section>
      ) : null}
      <AddPackDialog
        open={adding}
        onOpenChange={setAdding}
        catalog={listing?.catalog ?? []}
        onInstalled={(next) => { setListing(next); setLoadError(null); }}
      />
      <ReviewPackDialog review={review} onClose={() => setReview(null)} onDone={(next) => { setListing(next); setLoadError(null); }} />
      <PackDetail
        pack={opened}
        busy={opened ? busyName === opened.info.name : false}
        error={opened ? errors[opened.info.name] ?? null : null}
        onClose={() => setOpenName(null)}
        onCheck={() => opened && void act(opened, "check")}
        onUpdate={() => opened && void act(opened, "update")}
        onReview={() => opened && void act(opened, "review")}
        onPlugin={(enable) => opened && void plugin(opened, enable)}
        onCleanUp={() => opened && void act(opened, "cleanup")}
        onRemove={() => opened && void act(opened, "remove")}
        onDismissError={() => opened && dismiss(opened.info.name)}
      />
    </SetupPage>
  );
}

export default function Packs() {
  useSetupCommands();
  return <PacksBody />;
}
