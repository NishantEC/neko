// One installed pack: what it is, what it may do, what it owns, what it needs
// and the last health check. The same actions as its row.
import { Button } from "@/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "@/ui/dialog";
import { StatusGlyph } from "@/ui/status-glyph";
import { ArrowUp, ArrowsClockwise, Eye, Trash } from "@/ui/icons";
import { ErrorNote } from "@/setup/kit";
import { relativeTime } from "@/setup/lib";
import { PACK_STATUS_WORDS, packGlyph, shortSource, type InstalledPack } from "@/setup/packs";
import { Contents, Heading, Mono, Requirements } from "@/pages/setup/packs/parts";

export function PackDetail({ pack, busy, error, onClose, onCheck, onUpdate, onReview, onRemove, onPlugin, onCleanUp, onDismissError }: {
  pack: InstalledPack | null;
  busy: boolean;
  error: string | null;
  onClose: () => void;
  onCheck: () => void;
  onUpdate: () => void;
  onReview: () => void;
  onPlugin: (enable: boolean) => void;
  onCleanUp: () => void;
  onRemove: () => void;
  onDismissError: () => void;
}) {
  if (!pack) return null;
  const { info } = pack;
  const orphans = pack.orphans ?? [];
  return (
    <Dialog open onOpenChange={(o) => { if (!o) onClose(); }}>
      <DialogContent className="max-h-[88vh] overflow-y-auto sm:max-w-2xl" data-slot="pack-detail">
        <DialogHeader>
          <DialogTitle className="text-lg">{info.title}</DialogTitle>
          <DialogDescription>
            <span className="inline-flex items-center gap-1.5">
              <StatusGlyph status={packGlyph(pack.status)} label="" />
              {PACK_STATUS_WORDS[pack.status]}
            </span>
            {` · ${info.version} · ${shortSource(pack.source, 64)}`}
          </DialogDescription>
        </DialogHeader>
        <div className="flex flex-col gap-5">
          <div className="flex flex-wrap items-center gap-2">
            <span className="flex-1" />
            {pack.status === "Changed" ? (
              <Button size="sm" disabled={busy} onClick={onReview}><Eye /> Review</Button>
            ) : null}
            {pack.update_available ? (
              <Button size="sm" disabled={busy} onClick={onUpdate}><ArrowUp /> Update to {pack.update_available}</Button>
            ) : null}
            <Button size="sm" variant="outline" disabled={busy} loading={busy} onClick={onCheck}><ArrowsClockwise /> Check health</Button>
            <Button size="sm" variant="destructive-outline" disabled={busy} onClick={onRemove}><Trash /> Remove…</Button>
          </div>
          <ErrorNote error={error} title="That didn’t work" onDismiss={onDismissError} />
          {pack.status === "Changed" ? (
            <p data-slot="pack-changed" className="text-[13px] text-warning-foreground">The files of this pack changed on disk since you approved it, so nothing of it runs. Review it to run it again.</p>
          ) : null}
          {orphans.length > 0 ? (
            <div data-slot="pack-orphans" className="flex flex-col gap-1.5 text-[13px] text-muted-foreground">
              <div className="flex items-center gap-2">
                <span>Something could not be cleaned up automatically.</span>
                <Button size="xs" variant="outline" disabled={busy} onClick={onCleanUp}>Clean up</Button>
              </div>
              <ul className="flex flex-col gap-0.5">{orphans.map((o) => <li key={o.target}><span className="font-mono text-[12px]">{o.target}</span> · {o.reason}</li>)}</ul>
            </div>
          ) : null}
          {info.description ? <p className="text-[13px] text-foreground/85">{info.description}</p> : null}
          <p className="text-[12.5px] text-muted-foreground">
            {info.licence ? `${info.licence} · ` : ""}
            {info.homepage ? <a href={info.homepage} target="_blank" rel="noreferrer" className="underline underline-offset-4 hover:text-foreground">{info.homepage}</a> : null}
          </p>
          <div className="flex flex-col gap-2">
            <Heading>Permissions</Heading>
            <ul className="flex list-disc flex-col gap-1 ps-5 text-[13px] text-foreground/90">
              {info.permissions.map((p) => <li key={p}>{p}</li>)}
              {info.permissions.length === 0 ? <li className="list-none text-muted-foreground">No permissions listed.</li> : null}
            </ul>
          </div>
          <Contents contents={info.contents} links />
          {pack.plugin_dir ? (
            <div data-slot="pack-plugin-switch" data-disabled={pack.plugin_disabled ? "true" : "false"} className="flex items-center gap-3 text-[13px]">
              <span className="text-muted-foreground">{pack.plugin_disabled ? "The plugin is switched off. It runs inside Neko with access to Neko’s own API." : "The plugin is switched on."}</span>
              <Button size="xs" variant="outline" disabled={busy} onClick={() => onPlugin(!!pack.plugin_disabled)}>{pack.plugin_disabled ? "Enable plugin" : "Disable plugin"}</Button>
            </div>
          ) : null}
          <Requirements items={info.requirements} />
          <div className="flex flex-col gap-2" data-slot="pack-health">
            <Heading>{pack.health ? `Last health check · ${pack.health.ok ? "passed" : "failed"} · ${relativeTime(pack.health.at_ms)}` : "Last health check"}</Heading>
            {pack.health ? (
              <Mono><span className="whitespace-pre-wrap">{pack.health.output || "(no output)"}</span></Mono>
            ) : (
              <p className="text-[13px] text-muted-foreground">Not checked yet.</p>
            )}
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}
