// The Rulebook: one editable document of engineering and review standards, this
// machine's limits and repository policy. It goes into Neko's chat prompt and
// every task agent's prompt; empty means the built-in default.
import { useEffect, useState } from "react";
import { command } from "@/api";
import { Button } from "@/ui/button";
import { Textarea } from "@/ui/textarea";
import { Kbd } from "@/ui/kbd";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/ui/collapsible";
import { ArrowCounterClockwise, CaretDown } from "@/ui/icons";
import { Panel } from "@/editorial";
import { useWorkbench } from "@/useStore";
import { ConfirmDialog, ErrorNote, PanelFooter, Section, SettingRow, Status } from "@/setup/kit";
import { str, useRun } from "@/setup/lib";

export function RulebookSection() {
  const { snapshot } = useWorkbench();
  const { run, busy, error, clear } = useRun();
  const custom = str(snapshot?.rulebook);
  const builtIn = str(snapshot?.rulebook_default);
  const saved = custom || builtIn;
  const usingDefault = !custom;
  const [draft, setDraft] = useState(saved);
  const [confirmReset, setConfirmReset] = useState(false);
  const [lastSaved, setLastSaved] = useState(saved);
  // Follow the daemon unless this draft has edits of its own.
  useEffect(() => {
    setDraft((d) => (d === lastSaved ? saved : d));
    setLastSaved(saved);
  }, [saved]); // eslint-disable-line react-hooks/exhaustive-deps

  if (!snapshot) return null;
  const dirty = draft !== saved;
  const save = (text: string) => run(() => command("SetRulebook", { text }));

  return (
    <div className="flex flex-col gap-8">
      <Section
        eyebrow="Every agent"
        title={<>The <em>rulebook.</em></>}
        intro={<>Neko and every agent it hands work to follow these rules: how to engineer a change, how to review one, how to treat this Mac and your repositories. Write them in plain words.</>}
        actions={<Status tone={usingDefault ? "neutral" : "success"}>{usingDefault ? "Using Neko’s default rules" : "Using your own rules"}</Status>}
      >
        <Panel data-slot="rulebook">
          <SettingRow id="rulebook" title="Rulebook" description="Changes apply to the next agent that starts; agents already running keep the rules they started with." stacked>
            <Textarea
              aria-label="Rulebook"
              value={draft}
              onChange={(e) => setDraft(e.target.value)}
              onKeyDown={(e) => { if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "s") { e.preventDefault(); if (dirty && !busy) void save(draft); } }}
              rows={18}
              className="min-h-[24rem] w-full text-[14px] leading-[1.65]"
            />
          </SettingRow>
          {error ? <div className="border-t border-border/70 p-4"><ErrorNote error={error} title="Couldn’t save the rulebook" onDismiss={clear} /></div> : null}
          <PanelFooter className="justify-between">
            <span className="flex items-center gap-2 text-xs text-muted-foreground">
              {dirty ? "Unsaved changes" : "Saved"} · <Kbd>⌘S</Kbd> saves
            </span>
            <div className="flex flex-wrap gap-2">
              {dirty ? <Button variant="ghost" size="sm" onClick={() => setDraft(saved)}><ArrowCounterClockwise /> Revert changes</Button> : null}
              {!usingDefault ? <Button variant="outline" size="sm" disabled={busy} onClick={() => setConfirmReset(true)}>Restore default</Button> : null}
              <Button size="sm" disabled={busy || !dirty} loading={busy} onClick={() => void save(draft)}>Save rulebook</Button>
            </div>
          </PanelFooter>
        </Panel>
        <Collapsible>
          <CollapsibleTrigger render={<Button variant="ghost" size="sm" className="-ms-2" />}>View Neko’s built-in default <CaretDown /></CollapsibleTrigger>
          <CollapsibleContent>
            <pre className="mt-2 max-h-96 select-text overflow-auto whitespace-pre-wrap rounded-xl border border-border bg-foreground/[0.03] p-4 font-sans text-[13px] leading-relaxed text-foreground/85">{builtIn || "The daemon didn’t send the built-in default."}</pre>
          </CollapsibleContent>
        </Collapsible>
      </Section>
      <ConfirmDialog
        open={confirmReset}
        onOpenChange={setConfirmReset}
        destructive={false}
        title="Restore the default rulebook?"
        description="Your own rules are removed and Neko’s built-in rules are used again from the next agent that starts."
        confirmLabel="Restore default"
        onConfirm={() => void save("")}
      />
    </div>
  );
}
