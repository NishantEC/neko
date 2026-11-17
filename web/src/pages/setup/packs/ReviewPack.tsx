// Update and Review. Both show a fresh preview of a pack that is already
// installed and send back that preview's digest, so what the person read is
// exactly what gets approved:
//   update  Preview(source of the pack) -> "Update" or, when what the pack can
//           do changed, "Update needs approval" with the changes and
//           "Approve and update"; sends Update { name, digest }.
//   review  Preview("installed:<name>") of a pack whose files changed on disk
//           -> "Approve" sends Approve { name, digest }.
// The dialog waits for its own answer; a refusal shows the daemon's words here.
import { useEffect, useState } from "react";
import { Button } from "@/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/ui/dialog";
import { ErrorNote } from "@/setup/kit";
import { approvePack, updatePack, type PackListing, type PackPreview } from "@/setup/packs";
import { Changes, PackBody } from "@/pages/setup/packs/parts";

export type Review = { kind: "update" | "review"; name: string; preview: PackPreview };

export function ReviewPackDialog({ review, onClose, onDone }: { review: Review | null; onClose: () => void; onDone: (listing: PackListing) => void }) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => { setBusy(false); setError(null); }, [review]);
  if (!review) return null;
  const { kind, name, preview } = review;
  const { info } = preview;
  const needsApproval = kind === "update" && !!preview.needs_approval;
  const title = kind === "review" ? `Review ${info.title}` : needsApproval ? "Update needs approval" : `Update ${info.title}`;
  const action = kind === "review" ? "Approve" : needsApproval ? "Approve and update" : "Update";
  const go = async () => {
    setBusy(true);
    setError(null);
    try {
      const listing = await (kind === "review" ? approvePack(name, preview.digest) : updatePack(name, preview.digest));
      onDone(listing);
      onClose();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };
  return (
    <Dialog open onOpenChange={(o) => { if (!o && !busy) onClose(); }}>
      <DialogContent className="max-h-[88vh] overflow-y-auto sm:max-w-2xl" data-slot="review-pack" data-kind={kind}>
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
          <DialogDescription>
            {kind === "review"
              ? "The files of this pack changed on disk, so nothing of it runs. Read what it would do now; approving lets it run again."
              : preview.installed_version
                ? `Version ${preview.installed_version} to ${info.version}.`
                : `Version ${info.version}.`}
          </DialogDescription>
        </DialogHeader>
        <form className="flex flex-col gap-5" onSubmit={(e) => { e.preventDefault(); if (!busy) void go(); }}>
          {kind === "update" && !needsApproval ? (
            <Changes items={preview.changes ?? []} />
          ) : (
            <PackBody info={info} changes={preview.changes ?? []} />
          )}
          <ErrorNote error={error} title={kind === "review" ? "Couldn’t approve the pack" : "Couldn’t update the pack"} />
          <DialogFooter>
            <Button type="button" variant="outline" disabled={busy} onClick={onClose}>Cancel</Button>
            <Button type="submit" loading={busy} disabled={busy}>{action}</Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
