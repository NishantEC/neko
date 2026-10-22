// Small dialogs the setup pages share.
import { useEffect, useState, type ReactNode } from "react";
import { Button } from "@/ui/button";
import { Input } from "@/ui/input";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/ui/dialog";
import { ErrorNote, FormField } from "@/setup/kit";

/** Asks for one line of text (a name), and reports a failure with the daemon's own words. */
export function TextPromptDialog({
  open, onOpenChange, title, description, label, initial = "", placeholder, confirmLabel = "Save", onSubmit, allowEmpty = false,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  title: ReactNode;
  description?: ReactNode;
  label: string;
  initial?: string;
  placeholder?: string;
  confirmLabel?: string;
  allowEmpty?: boolean;
  /** Resolves when saved; reject (or throw) with a message to keep the dialog open. */
  onSubmit: (value: string) => Promise<{ ok: boolean; error?: string | null }>;
}) {
  const [value, setValue] = useState(initial);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    if (open) { setValue(initial); setError(null); }
  }, [open, initial]);
  const submit = async () => {
    setBusy(true);
    const result = await onSubmit(value.trim());
    setBusy(false);
    if (result.ok) onOpenChange(false);
    else setError(result.error ?? "That didn’t work.");
  };
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
          {description ? <DialogDescription>{description}</DialogDescription> : null}
        </DialogHeader>
        <form className="flex flex-col gap-4" onSubmit={(e) => { e.preventDefault(); void submit(); }}>
          <FormField label={label} htmlFor="prompt-input">
            <Input id="prompt-input" value={value} onChange={(e) => setValue(e.target.value)} placeholder={placeholder} autoComplete="off" autoFocus />
          </FormField>
          <ErrorNote error={error} />
          <DialogFooter>
            <Button type="button" variant="outline" onClick={() => onOpenChange(false)}>Cancel</Button>
            <Button type="submit" loading={busy} disabled={!allowEmpty && !value.trim()}>{confirmLabel}</Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
