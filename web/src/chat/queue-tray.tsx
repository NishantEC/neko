// Messages sent while a reply was running, waiting their turn. Docks onto the
// composer's top edge (ChatInput's `tray` slot).
import { ArrowElbowDownRight, PencilSimple, X } from "@/ui/icons";
import { Button } from "@/ui/button";
import { Kbd } from "@/ui/kbd";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/ui/tooltip";
import { MonoLabel } from "@/editorial";
import type { NekoMessage } from "@/chat/types";

export function QueueTray({
  queued, onEdit, onSteer, onRemove,
}: {
  queued: NekoMessage[];
  onEdit: (m: NekoMessage) => void;
  onSteer: (m: NekoMessage) => void;
  onRemove: (m: NekoMessage) => void;
}) {
  if (!queued.length) return null;
  return (
    <div data-slot="queue-tray" role="group" aria-label="Queued messages" className="mb-2 rounded-2xl border border-border bg-card pb-1.5 shadow-xs">
      <div className="flex items-center px-3.5 pt-2 pb-0.5">
        <MonoLabel className="text-[10.5px]">Queued</MonoLabel>
        <span className="flex-1" />
        <span className="text-xs tabular-nums text-muted-foreground">{queued.length}</span>
      </div>
      {queued.map((m) => (
        <div key={m.id} data-slot="queue-row" className="flex min-h-8 items-center gap-1 ps-3.5 pe-2">
          <span className="min-w-0 flex-1 truncate text-[13px] text-foreground" title={m.text}>{m.text}</span>
          <Tooltip>
            <TooltipTrigger asChild>
              <Button variant="ghost" size="icon-xs" aria-label="Edit queued message" onClick={() => onEdit(m)}>
                <PencilSimple />
              </Button>
            </TooltipTrigger>
            <TooltipContent>Edit <Kbd>⌥↑</Kbd></TooltipContent>
          </Tooltip>
          <Tooltip>
            <TooltipTrigger asChild>
              <Button variant="ghost" size="xs" aria-label="Send now" onClick={() => onSteer(m)} className="gap-1 text-muted-foreground hover:text-foreground">
                <ArrowElbowDownRight /> Steer
              </Button>
            </TooltipTrigger>
            <TooltipContent>Send now <Kbd>⌘⇧↵</Kbd></TooltipContent>
          </Tooltip>
          <Button variant="ghost" size="icon-xs" aria-label="Remove queued message" onClick={() => onRemove(m)}>
            <X />
          </Button>
        </div>
      ))}
    </div>
  );
}
