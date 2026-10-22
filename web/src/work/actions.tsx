// Sending commands and asking before the ones that cannot be undone.
import { useState, type ReactNode } from "react";
import { accept, command, type Json } from "@/api";
import { toast } from "@/ui/sonner";
import { Button } from "@/ui/button";
import {
  AlertDialog, AlertDialogAction, AlertDialogCancel, AlertDialogContent, AlertDialogDescription, AlertDialogFooter,
  AlertDialogHeader, AlertDialogTitle,
} from "@/ui/alert-dialog";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/ui/dialog";
import { Input } from "@/ui/input";
import { Textarea } from "@/ui/textarea";
import { Label } from "@/ui/label";

/**
 * Sends one workbench command (the same one the native app's button sends),
 * folds the new snapshot into the store and says what happened. Returns
 * whether it worked; a refusal reaches the user as the daemon worded it.
 */
export async function run(name: string, args?: Record<string, unknown>, done?: string): Promise<boolean> {
  try {
    accept(await command(name, args));
    if (done) toast.success(done);
    return true;
  } catch (error) {
    toast.error(error instanceof Error ? error.message : String(error));
    return false;
  }
}

/** Like `run`, for a request that returns something other than a snapshot. */
export async function ask(request: () => Promise<Json>): Promise<Json | null> {
  try {
    return await request();
  } catch (error) {
    toast.error(error instanceof Error ? error.message : String(error));
    return null;
  }
}

export interface ConfirmSpec {
  title: string;
  description: ReactNode;
  confirm: string;
  cancel?: string;
  destructive?: boolean;
  onConfirm: () => void | Promise<unknown>;
}

/** The one confirmation every destructive action goes through. */
export function ConfirmDialog({ spec, onClose }: { spec: ConfirmSpec | null; onClose: () => void }) {
  return (
    <AlertDialog open={spec !== null} onOpenChange={(open) => { if (!open) onClose(); }}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>{spec?.title}</AlertDialogTitle>
          <AlertDialogDescription>{spec?.description}</AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel>{spec?.cancel ?? "Keep it"}</AlertDialogCancel>
          <AlertDialogAction variant={spec?.destructive ? "destructive" : "default"} onClick={() => void spec?.onConfirm()}>
            {spec?.confirm}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}

export interface PromptSpec {
  title: string;
  description?: ReactNode;
  label: string;
  initial?: string;
  placeholder?: string;
  confirm: string;
  multiline?: boolean;
  /** An empty answer is fine (rename: clears the name). */
  allowEmpty?: boolean;
  destructive?: boolean;
  onSubmit: (text: string) => void | Promise<unknown>;
}

/** A dialog that asks for one line of text: a new name, why a pull request is closed. */
export function PromptDialog({ spec, onClose }: { spec: PromptSpec | null; onClose: () => void }) {
  return (
    <Dialog open={spec !== null} onOpenChange={(open) => { if (!open) onClose(); }}>
      <DialogContent>{spec ? <PromptBody key={spec.title + spec.initial} spec={spec} onClose={onClose} /> : null}</DialogContent>
    </Dialog>
  );
}

function PromptBody({ spec, onClose }: { spec: PromptSpec; onClose: () => void }) {
  const [text, setText] = useState(spec.initial ?? "");
  const [busy, setBusy] = useState(false);
  const ready = spec.allowEmpty || text.trim() !== "";
  const submit = async () => {
    if (!ready || busy) return;
    setBusy(true);
    await spec.onSubmit(text.trim());
    setBusy(false);
    onClose();
  };
  return (
    <form onSubmit={(e) => { e.preventDefault(); void submit(); }} className="grid gap-4">
      <DialogHeader>
        <DialogTitle>{spec.title}</DialogTitle>
        {spec.description ? <DialogDescription>{spec.description}</DialogDescription> : null}
      </DialogHeader>
      <div className="grid gap-2">
        <Label htmlFor="work-prompt">{spec.label}</Label>
        {spec.multiline ? (
          <Textarea id="work-prompt" autoFocus rows={4} value={text} placeholder={spec.placeholder} onChange={(e) => setText(e.target.value)} />
        ) : (
          <Input id="work-prompt" autoFocus value={text} placeholder={spec.placeholder} onChange={(e) => setText(e.target.value)} />
        )}
      </div>
      <DialogFooter>
        <Button type="button" variant="outline" onClick={onClose}>Cancel</Button>
        <Button type="submit" variant={spec.destructive ? "destructive" : "default"} disabled={!ready} loading={busy}>{spec.confirm}</Button>
      </DialogFooter>
    </form>
  );
}
