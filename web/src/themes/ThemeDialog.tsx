// "Make me a theme": asks what the theme should look like, then sends Neko one
// chat message that opens a task to write a validated theme file into the
// themes folder. The words Neko is given are the constant in prompt.ts, shown
// here before sending, like Change Neko's.
import { useEffect, useState } from "react";
import { accept, command } from "@/api";
import { navigate, HOME } from "@/router";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/ui/dialog";
import { Button } from "@/ui/button";
import { Textarea } from "@/ui/textarea";
import { CircleNotch, Sparkle } from "@/ui/icons";
import { toast } from "@/ui/sonner";
import { Kbd } from "@/ui/kbd";
import { byteLength } from "@/chat/ChatInputSlash";
import { themePrompt, themeRequestBudget } from "@/themes/prompt";

const EXAMPLES = ["Warm sunset, dark, with orange accents", "A calm light theme in sage green", "Frosted glass in deep purple"];

export function ThemeDialog({ open, onOpenChange, dir }: { open: boolean; onOpenChange(open: boolean): void; dir: string | null }) {
  const [wish, setWish] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    if (open) setError(null);
  }, [open]);
  const budget = themeRequestBudget(dir);
  const used = byteLength(wish.trim());
  const tooLong = used > budget;
  const can = wish.trim().length > 0 && !tooLong && !busy && dir !== null;

  async function send() {
    if (!can) return;
    setBusy(true);
    setError(null);
    try {
      accept(await command("SendMessage", { text: themePrompt(wish, dir), workspace_id: null }));
      toast.success("Asked Neko to make a theme", { description: "It opens a task. The theme appears here a few seconds after the file is written." });
      setWish("");
      onOpenChange(false);
      navigate(HOME);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
    setBusy(false);
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent data-slot="theme-dialog" className="sm:max-w-xl">
        <DialogHeader>
          <DialogTitle className="text-[18px] font-semibold">Make me a theme</DialogTitle>
          <DialogDescription>
            Describe how it should look. Neko opens a task, writes a theme file into your themes folder and checks it. It appears in the pickers when the file is saved, and you can remove it any time.
          </DialogDescription>
        </DialogHeader>
        <div className="flex flex-col gap-2">
          <label htmlFor="theme-wish" className="text-[13px] font-medium text-strong">What should it look like?</label>
          <Textarea
            id="theme-wish"
            value={wish}
            onChange={(e) => setWish(e.target.value)}
            rows={3}
            autoFocus
            placeholder="A dark theme in deep teal with a soft gold accent…"
            onKeyDown={(e) => {
              if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
                e.preventDefault();
                void send();
              }
            }}
          />
          <div className="flex flex-wrap items-center gap-1.5">
            {EXAMPLES.map((example) => (
              <button
                key={example}
                type="button"
                onClick={() => setWish(example)}
                className="rounded-full border border-border px-2.5 py-1 text-[12px] text-muted-foreground outline-none transition-colors hover:bg-accent hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring"
              >
                {example}
              </button>
            ))}
            <span className={`ms-auto text-[11.5px] tabular-nums ${tooLong ? "text-destructive" : "text-muted-foreground"}`} aria-live="polite">
              {used.toLocaleString()} / {budget.toLocaleString()}
            </span>
          </div>
          <details className="group rounded-xl border border-border bg-foreground/[0.02] px-3 py-2 text-[12.5px]">
            <summary className="cursor-pointer select-none text-muted-foreground outline-none focus-visible:ring-2 focus-visible:ring-ring">What Neko will be told</summary>
            <pre data-slot="theme-prompt" className="mt-2 max-h-48 overflow-auto whitespace-pre-wrap font-mono text-[11.5px] leading-snug text-muted-foreground">
              {themePrompt(wish.trim() || "…", dir)}
            </pre>
          </details>
          {error ? <p role="alert" data-slot="theme-error" className="select-text text-[13px] text-destructive">{error}</p> : null}
          {dir === null ? <p role="alert" className="text-[13px] text-warning-foreground">The daemon did not say where themes live yet, so Neko cannot be asked.</p> : null}
        </div>
        <DialogFooter>
          <Button type="button" variant="ghost" onClick={() => onOpenChange(false)}>Cancel</Button>
          <Button type="button" onClick={() => void send()} disabled={!can} data-slot="theme-send">
            {busy ? <CircleNotch className="animate-spin" /> : <Sparkle />} Ask Neko <Kbd className="ms-1 bg-primary-foreground/15 text-primary-foreground">⌘↵</Kbd>
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
