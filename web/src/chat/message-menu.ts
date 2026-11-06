// The right-click menu of a chat message (shell/context-menu.tsx): Copy, Quote
// in reply, Edit and send again (your own messages: the text goes back into
// the composer, the way ⌥↑ takes a queued one back), and Open for every task
// the message opened or names. Each item calls what the visible controls call.
import { useWorkbench } from "@/useStore";
import { copyText, useContextMenu, type MenuEntry } from "@/shell/context-menu";
import { taskHref, taskTitle } from "@/shell/status";
import { shortId } from "@/work/board/moves";
import { ArrowSquareOut, ChatCircle, Copy, PencilSimple } from "@/ui/icons";
import { draftReply } from "@/chat/reply-views";
import { tasksIn, useTaskIndex } from "@/chat/references";
import type { NekoUIMessage } from "@/chat/sdk/types";

export const quote = (text: string) => `${text.trim().split("\n").map((line) => `> ${line}`).join("\n")}\n\n`;

export function useMessageMenu(message: NekoUIMessage, text: string, { own = false }: { own?: boolean } = {}) {
  const { snapshot } = useWorkbench();
  const index = useTaskIndex();
  return useContextMenu(() => {
    const tasks: { id: string }[] = snapshot?.tasks ?? [];
    const ids = [...new Set([...(message.metadata?.tickets ?? []), ...tasksIn(text, index)])];
    const named = ids.map((id) => tasks.find((t) => String(t.id) === id)).filter((t): t is { id: string } => !!t);
    const items: MenuEntry[] = [
      { label: own ? "Copy message" : "Copy reply", icon: Copy, disabled: !text, run: () => void copyText(text) },
      { label: "Quote in reply", icon: ChatCircle, disabled: !text, run: () => draftReply(quote(text)) },
      own ? { label: "Edit and send again", icon: PencilSimple, disabled: !text, run: () => draftReply(text) } : null,
    ];
    if (named.length) items.push("separator");
    for (const task of named.slice(0, 5)) {
      items.push({ label: `Open ${shortId(task)}: ${taskTitle(task)}`, icon: ArrowSquareOut, run: () => { location.hash = taskHref(task.id).slice(1); } });
    }
    return items;
  });
}
