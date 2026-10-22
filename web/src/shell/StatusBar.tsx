// The status bar under every page: what the Mac can take (machine hold / cap),
// the one-line "is anything waiting on me?" (StatusSummary.swift) and the
// shortcut hint. The machine entry opens the same facts MachineStrip.swift shows.
import { Popover, PopoverContent, PopoverTrigger } from "@/ui/popover";
import { Kbd } from "@/ui/kbd";
import { cn } from "@/ui/lib/utils";
import { openShortcuts } from "@/shell/shortcuts";
import { elapsed, useNow, useShellStatus } from "@/shell/status";
import { MonoLabel } from "@/editorial";

const DOT = { ok: "bg-success", limited: "bg-warning", holding: "bg-destructive" } as const;
const TEXT = { ok: "text-muted-foreground", limited: "text-warning-foreground", holding: "text-destructive-foreground" } as const;

export function StatusBar() {
  const status = useShellStatus();
  const now = useNow(1000, status.working !== null);
  const { machine } = status;
  const summary = status.working
    ? `Working ${elapsed(now - status.working.startedMs)}`
    : status.needsUser > 0
      ? `${status.needsUser} ${status.needsUser === 1 ? "thing is" : "things are"} waiting on you`
      : "Nothing is waiting on you";
  const running = status.counts.running;
  const queued = status.counts.upNext;
  const tasks = running === 0 ? "Nothing running" : `${running} running`;
  return (
    <footer data-slot="status-bar" aria-label="Status" className="flex h-8 shrink-0 items-center gap-4 border-t border-border/70 px-3 text-[12px]">
      <Popover>
        <PopoverTrigger
          data-slot="machine"
          data-level={machine.level}
          aria-label={`Mac health: ${machine.title}`}
          title="How much the Mac can take right now"
          className={cn("flex items-center gap-2 rounded-md px-1.5 py-0.5 outline-none transition-colors hover:bg-accent focus-visible:ring-2 focus-visible:ring-ring", TEXT[machine.level])}
        >
          <span aria-hidden className={cn("size-1.5 rounded-full", machine.known ? DOT[machine.level] : "bg-muted-foreground/50")} />
          <span>{machine.title}</span>
        </PopoverTrigger>
        <PopoverContent side="top" align="start" className="w-80 gap-3 p-4">
          <MonoLabel className="text-[10.5px]">The Mac</MonoLabel>
          <h2 className="-mt-2 text-[15px] font-semibold text-strong">{machine.title}</h2>
          {machine.known ? (
            <dl className="grid grid-cols-[88px_1fr] gap-x-3 gap-y-1.5 text-[12.5px]">
              {machine.facts.map(([label, value]) => (
                <div key={label} className="contents">
                  <dt className="text-muted-foreground">{label}</dt>
                  <dd className="text-foreground">{value}</dd>
                </div>
              ))}
            </dl>
          ) : (
            <p className="text-[12.5px] text-muted-foreground">Neko has not read the Mac yet.</p>
          )}
          <p className="text-[11.5px] leading-snug text-muted-foreground">
            When the Mac is under strain Neko starts fewer agents, or none, until it recovers. Work that is already running is never touched.
          </p>
        </PopoverContent>
      </Popover>
      <span data-slot="summary" className={cn("flex items-center gap-2", status.needsUser > 0 ? "text-warning-foreground" : "text-muted-foreground")}>
        {summary}
      </span>
      <span className="hidden text-muted-foreground sm:inline">
        {tasks}
        {queued > 0 ? ` · ${queued} up next` : ""}
      </span>
      <span className="flex-1" />
      <button
        type="button"
        onClick={openShortcuts}
        aria-label="Show keyboard shortcuts"
        className="flex items-center gap-1.5 rounded-md px-1.5 py-0.5 text-muted-foreground outline-none transition-colors hover:bg-accent hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring"
      >
        <Kbd>?</Kbd> Shortcuts
      </button>
    </footer>
  );
}
