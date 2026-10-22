// "Build a plugin with Neko": asks what it should do, then sends Neko one chat
// message that opens a task to write the plugin into the plugins folder. The
// words Neko is given are the constant in prompt.ts, shown here before sending.
import { useEffect, useState } from "react";
import { accept, command } from "@/api";
import { navigate, HOME } from "@/router";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/ui/dialog";
import { Button } from "@/ui/button";
import { Textarea } from "@/ui/textarea";
import { CircleNotch, Sparkle } from "@/ui/icons";
import { toast } from "@/ui/sonner";
import { Kbd } from "@/ui/kbd";
import { MonoLabel } from "@/editorial";
import { byteLength } from "@/chat/ChatInputSlash";
import { buildPluginPrompt, BUILD_PLUGIN_PROMPT, requestBudget } from "@/pages/plugins/prompt";

const EXAMPLES = ["A board of my open pull requests by stage", "A page that shows what the machine is doing right now", "A list of tasks that failed this week"];

export function BuildDialog({ open, onOpenChange, folder }: { open: boolean; onOpenChange(open: boolean): void; folder: string | null }) {
  const [wish, setWish] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    if (open) setError(null);
  }, [open]);
  const dir = folder ?? "<data dir>/plugins";
  const budget = requestBudget(dir);
  const used = byteLength(wish.trim());
  const tooLong = used > budget;
  const can = wish.trim().length > 0 && !tooLong && !busy && folder !== null;

  async function send() {
    if (!can || !folder) return;
    setBusy(true);
    setError(null);
    try {
      accept(await command("SendMessage", { text: buildPluginPrompt(wish, folder), workspace_id: null }));
      toast.success("Asked Neko to build it", { description: "It opens a task and tells you when the plugin is in place." });
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
      <DialogContent data-slot="build-dialog" className="sm:max-w-xl">
        <DialogHeader>
          <MonoLabel className="text-[10.5px]">Plugins</MonoLabel>
          <DialogTitle className="text-[18px] font-semibold">Build a plugin with Neko</DialogTitle>
          <DialogDescription>
            Describe what it should do. Neko opens a task that writes the plugin into your plugins folder; it shows up in the sidebar when it is ready.
          </DialogDescription>
        </DialogHeader>
        <div className="flex flex-col gap-2">
          <label htmlFor="plugin-wish" className="text-[13px] font-medium text-strong">What should it do?</label>
          <Textarea
            id="plugin-wish"
            value={wish}
            onChange={(e) => setWish(e.target.value)}
            rows={4}
            autoFocus
            placeholder="A page that shows…"
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
            <pre data-slot="build-prompt" className="mt-2 max-h-48 overflow-auto whitespace-pre-wrap font-mono text-[11.5px] leading-snug text-muted-foreground">
              {BUILD_PLUGIN_PROMPT.replace("{{PLUGINS_DIR}}", dir).replace("{{REQUEST}}", wish.trim() || "…")}
            </pre>
          </details>
          {error ? <p role="alert" data-slot="build-error" className="select-text text-[13px] text-destructive">{error}</p> : null}
          {folder === null ? <p role="alert" className="text-[13px] text-warning-foreground">The daemon did not say where plugins live, so Neko cannot be asked yet.</p> : null}
        </div>
        <DialogFooter>
          <Button type="button" variant="ghost" onClick={() => onOpenChange(false)}>Cancel</Button>
          <Button type="button" onClick={() => void send()} disabled={!can} data-slot="build-send">
            {busy ? <CircleNotch className="animate-spin" /> : <Sparkle />} Ask Neko <Kbd className="ms-1 bg-primary-foreground/15 text-primary-foreground">⌘↵</Kbd>
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
