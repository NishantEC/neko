// One place that owns the confirm and prompt dialogs a page opens, so a page
// writes `ui.confirm({...})` and mounts `{dialogs}` once.
import { useMemo, useState, type ReactNode } from "react";
import { ConfirmDialog, PromptDialog, type ConfirmSpec, type PromptSpec } from "@/work/actions";
import type { Ui } from "@/work/taskActions";

export function useDialogs(): { ui: Ui; dialogs: ReactNode } {
  const [confirm, setConfirm] = useState<ConfirmSpec | null>(null);
  const [prompt, setPrompt] = useState<PromptSpec | null>(null);
  const ui = useMemo<Ui>(() => ({ confirm: setConfirm, prompt: setPrompt }), []);
  const dialogs = (
    <>
      <ConfirmDialog spec={confirm} onClose={() => setConfirm(null)} />
      <PromptDialog spec={prompt} onClose={() => setPrompt(null)} />
    </>
  );
  return { ui, dialogs };
}
