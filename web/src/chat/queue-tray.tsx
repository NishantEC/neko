// Messages sent while a reply was running, waiting their turn. Docks onto the
// composer's top edge (ChatInput's `tray` slot).
//
// from t3code apps/web/src/components/chat/QueuedRunsControl.tsx @3b95e7831 (MIT)
// (adapted: Neko's queued messages and its Edit / Steer / Remove commands; no
// drag reordering or thumbnails, which the daemon's queue does not have)
import { useId, useState } from "react";
import { CornerUpRightIcon, ListOrderedIcon, PencilIcon } from "lucide-react";
import { Button } from "@/ui/button";
import { Kbd } from "@/ui/kbd";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/ui/tooltip";
import { ComposerBanner } from "@/chat/t3/ComposerBanner";
import type { NekoUIMessage } from "@/chat/sdk/types";
import { messageText } from "@/chat/sdk/parts";

const ghostMuted = "text-muted-foreground hover:text-foreground";

export function QueueTray({
  queued, onEdit, onSteer, onRemove,
}: {
  queued: NekoUIMessage[];
  onEdit: (m: NekoUIMessage) => void;
  onSteer: (m: NekoUIMessage) => void;
  onRemove: (m: NekoUIMessage) => void;
}) {
  const [expanded, setExpanded] = useState(true);
  const queueListId = useId();
  if (!queued.length) return null;
  return (
    <ComposerBanner.Attachment>
      <ComposerBanner.Root
        data-slot="queue-tray"
        role="region"
        aria-label={`${queued.length} queued message${queued.length === 1 ? "" : "s"}`}
        aria-live="polite"
        className="relative z-0"
      >
        <ComposerBanner.Row
          render={<button type="button" />}
          aria-label={expanded ? "Collapse queued messages" : "Expand queued messages"}
          aria-expanded={expanded}
          aria-controls={queueListId}
          onPointerDown={(event) => event.preventDefault()}
          onClick={() => setExpanded((value) => !value)}
        >
          <ComposerBanner.Icon>
            <ListOrderedIcon />
          </ComposerBanner.Icon>
          <ComposerBanner.Content className="text-muted-foreground">Queued</ComposerBanner.Content>
          <ComposerBanner.Actions>
            <ComposerBanner.Count>{queued.length}</ComposerBanner.Count>
            <ComposerBanner.ToggleIcon expanded={expanded} />
          </ComposerBanner.Actions>
        </ComposerBanner.Row>
        <ComposerBanner.Scroll className={expanded ? "max-h-32" : "hidden"}>
          <ComposerBanner.Children render={<ol />} id={queueListId}>
            {queued.map((m, index) => (
              <ComposerBanner.Row render={<li />} key={m.id} data-slot="queue-row" className="relative rounded-sm">
                <ComposerBanner.Icon aria-hidden />
                <ComposerBanner.Content className="text-foreground/80">
                  <Tooltip>
                    <TooltipTrigger render={<span className="min-w-0 flex-1 truncate" />}>{messageText(m)}</TooltipTrigger>
                    <TooltipContent side="top" className="max-w-96 break-words">{messageText(m)}</TooltipContent>
                  </Tooltip>
                </ComposerBanner.Content>
                <ComposerBanner.Actions>
                  <Tooltip>
                    <TooltipTrigger
                      render={
                        <Button size="icon-xs" variant="ghost" className={`size-6 ${ghostMuted}`} aria-label="Edit queued message" onClick={() => onEdit(m)} />
                      }
                    >
                      <PencilIcon className="size-3.5" />
                    </TooltipTrigger>
                    <TooltipContent>
                      Edit in the composer{index === queued.length - 1 ? <> <Kbd>⌥↑</Kbd></> : null}
                    </TooltipContent>
                  </Tooltip>
                  <Tooltip>
                    <TooltipTrigger render={<span className="flex shrink-0" />}>
                      <Button size="xs" variant="ghost" className={`h-6 gap-1 px-1.5 text-xs ${ghostMuted}`} aria-label="Send now" onClick={() => onSteer(m)}>
                        <CornerUpRightIcon className="size-3.5" />
                        Steer
                      </Button>
                    </TooltipTrigger>
                    <TooltipContent>
                      Stop the reply and send this now <Kbd>⌘⇧↵</Kbd>
                    </TooltipContent>
                  </Tooltip>
                  <Tooltip>
                    <TooltipTrigger
                      render={<ComposerBanner.Dismiss className={`size-6 ${ghostMuted}`} aria-label="Remove queued message" onClick={() => onRemove(m)} />}
                    />
                    <TooltipContent>Remove from queue</TooltipContent>
                  </Tooltip>
                </ComposerBanner.Actions>
              </ComposerBanner.Row>
            ))}
          </ComposerBanner.Children>
        </ComposerBanner.Scroll>
      </ComposerBanner.Root>
    </ComposerBanner.Attachment>
  );
}
