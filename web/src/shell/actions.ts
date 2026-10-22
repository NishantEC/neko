// Things the shell can do, in one place: the ⌘K palette, the keyboard shortcuts,
// the status bar and the sidebar all call these, so each behaves the same
// wherever it is started from.
import { accept, command, type Json } from "@/api";
import { getStore } from "@/api";
import { HOME, navigate } from "@/router";
import { toast } from "@/ui/sonner";
import { pendingDecisions, type Decision } from "@/chat/decisions";
import { confirm } from "@/shell/confirm";

/** New task: Home, with the cursor in the composer (⌘N in the native app). */
export function newTask() {
  navigate(HOME);
  focusComposer();
}

export function focusComposer() {
  let tries = 0;
  const attempt = () => {
    const area = document.querySelector<HTMLTextAreaElement>("[data-slot='composer'] textarea");
    if (area) area.focus();
    else if (tries++ < 20) setTimeout(attempt, 50);
  };
  setTimeout(attempt, 30);
}

/** CancelAllWork, after asking: it stops every unfinished task and Neko's reply (StopAllWork.swift). */
export async function stopAllWork(): Promise<boolean> {
  const snapshot = getStore().snapshot;
  const running = (snapshot?.tasks ?? []).filter((t: Json) => !["ReadyForReview", "Completed", "Failed", "Cancelled"].includes(t.status)).length;
  const replying = (snapshot?.conversation ?? []).some((m: Json) => m.pending);
  if (running === 0 && !replying) {
    toast("Nothing is running.");
    return false;
  }
  const ok = await confirm({
    title: "Stop all work?",
    description: `${running > 0 ? `${running} ${running === 1 ? "task" : "tasks"}${replying ? " and " : ""}` : ""}${replying ? "Neko’s current reply" : ""} will stop now. Worktrees and files are kept; remote tool calls that already finished cannot be undone.`,
    action: "Stop all work",
    destructive: true,
  });
  if (!ok) return false;
  try {
    accept(await command("CancelAllWork"));
    toast(running === 0 ? "Stopped Neko’s reply." : `Stopped ${running} ${running === 1 ? "task" : "tasks"}. Worktrees are kept.`);
    return true;
  } catch (e) {
    toast.error("Could not stop the work", { description: e instanceof Error ? e.message : String(e) });
    return false;
  }
}

/** What is waiting for a yes right now (tool calls and proposed actions), newest first. */
export function currentDecisions(): Decision[] {
  const snapshot = getStore().snapshot;
  return snapshot ? pendingDecisions(snapshot.conversation ?? [], snapshot.tasks ?? []) : [];
}

/** Approves the decision in front: the same one the dock above the composer shows. */
export async function approveFirstDecision(): Promise<void> {
  const first = currentDecisions()[0];
  if (!first) return void toast("Nothing is waiting for approval.");
  try {
    accept(await first.decide(true));
    toast.success("Approved", { description: first.subject });
  } catch (e) {
    toast.error("Could not approve", { description: e instanceof Error ? e.message : String(e) });
  }
}

export async function copyText(text: string, what = "Copied") {
  try {
    await navigator.clipboard.writeText(text);
    toast(what);
  } catch {
    // Clipboard access can be refused in an embedded view; fall back to a selection copy.
    const area = document.createElement("textarea");
    area.value = text;
    area.setAttribute("readonly", "");
    area.style.position = "fixed";
    area.style.opacity = "0";
    document.body.appendChild(area);
    area.select();
    const done = document.execCommand("copy");
    area.remove();
    if (done) toast(what);
    else toast.error("Could not copy", { description: text });
  }
}

export type { Json };
