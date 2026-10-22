// "Change Neko": asks what should be different, then sends Neko one chat message
// that opens a task to make the change, either as a page in the plugins folder
// (live in seconds, only yours) or in the neko checkout (goes live through
// self-update once it lands, with the previous UI kept). The words Neko is
// given are the constant in prompt.ts, shown here before sending.
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
import { changePrompt, CHANGE_NEKO_PROMPT, requestBudget, type ChangePlaces } from "@/pages/customisations/prompt";

const EXAMPLES = ["A board of my open pull requests by stage", "Make the Tasks page show cost per task", "Make the accent colour green"];

export function ChangeDialog({ open, onOpenChange, places }: { open: boolean; onOpenChange(open: boolean): void; places: ChangePlaces | null }) {
  const [wish, setWish] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    if (open) setError(null);
  }, [open]);
  const shown: ChangePlaces = places ?? { pluginsDir: "<data dir>/plugins", cssPath: "<data dir>/custom.css", checkout: null };
  const budget = requestBudget(shown);
  const used = byteLength(wish.trim());
  const tooLong = used > budget;
  const can = wish.trim().length > 0 && !tooLong && !busy && places !== null;

  async function send() {
    if (!can || !places) return;
    setBusy(true);
    setError(null);
    try {
      accept(await command("SendMessage", { text: changePrompt(wish, places), workspace_id: null }));
      toast.success("Asked Neko to make the change", { description: "It opens a task and tells you where the change appears." });
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
      <DialogContent data-slot="change-dialog" className="sm:max-w-xl">
        <DialogHeader>
          <DialogTitle className="text-[18px] font-semibold">Change Neko</DialogTitle>
          <DialogDescription>
            Describe what should be different. Neko opens a task and picks the lighter way: a page or style of yours, live in seconds, or a change to Neko’s own source.
          </DialogDescription>
        </DialogHeader>
        <div className="flex flex-col gap-2">
          <label htmlFor="change-wish" className="text-[13px] font-medium text-strong">What should change?</label>
          <Textarea
            id="change-wish"
            value={wish}
            onChange={(e) => setWish(e.target.value)}
            rows={4}
            autoFocus
            placeholder="Add a page that shows… / Make the sidebar…"
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
          <div data-slot="change-where" className="rounded-xl border border-border bg-foreground/[0.02] px-3 py-2 text-[12.5px] leading-relaxed text-muted-foreground">
            <p><span className="font-medium text-foreground">A page, override or style of yours</span> appears in the sidebar and in Customisations a few seconds after the files are saved. It is only yours and can be switched off or reset.</p>
            <p className="mt-1.5"><span className="font-medium text-foreground">A change to Neko’s source</span> goes live by itself once it lands on main. The web UI keeps the previous version, and Settings, Neko updates has a button to roll it back.</p>
          </div>
          <details className="group rounded-xl border border-border bg-foreground/[0.02] px-3 py-2 text-[12.5px]">
            <summary className="cursor-pointer select-none text-muted-foreground outline-none focus-visible:ring-2 focus-visible:ring-ring">What Neko will be told</summary>
            <pre data-slot="change-prompt" className="mt-2 max-h-48 overflow-auto whitespace-pre-wrap font-mono text-[11.5px] leading-snug text-muted-foreground">
              {changePrompt(wish.trim() || "…", shown) || CHANGE_NEKO_PROMPT}
            </pre>
          </details>
          {error ? <p role="alert" data-slot="change-error" className="select-text text-[13px] text-destructive">{error}</p> : null}
          {places === null ? <p role="alert" className="text-[13px] text-warning-foreground">The daemon did not say where your customisations live, so Neko cannot be asked yet.</p> : null}
        </div>
        <DialogFooter>
          <Button type="button" variant="ghost" onClick={() => onOpenChange(false)}>Cancel</Button>
          <Button type="button" onClick={() => void send()} disabled={!can} data-slot="change-send">
            {busy ? <CircleNotch className="animate-spin" /> : <Sparkle />} Ask Neko <Kbd className="ms-1 bg-primary-foreground/15 text-primary-foreground">⌘↵</Kbd>
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
