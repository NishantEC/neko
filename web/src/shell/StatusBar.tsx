// The sidebar's status block (it used to be a bar under every page): what the
// Mac can take (machine hold / cap), the one-line "is anything waiting on me?"
// (StatusSummary.swift), the theme menu and the shortcut hint. The machine
// entry opens the same facts MachineStrip.swift shows.
import { useSyncExternalStore } from "react";
import { Popover, PopoverContent, PopoverTrigger } from "@/ui/popover";
import { DropdownMenu, DropdownMenuContent, DropdownMenuLabel, DropdownMenuRadioGroup, DropdownMenuRadioItem, DropdownMenuTrigger } from "@/ui/dropdown-menu";
import { Desktop, Moon, Sun, type Icon } from "@/ui/icons";
import { getTheme, setTheme, subscribeTheme, type ThemeChoice } from "@/theme";
import { useWorkbench } from "@/useStore";
import { Kbd } from "@/ui/kbd";
import { cn } from "@/ui/lib/utils";
import { openShortcuts } from "@/shell/shortcuts";
import { elapsed, useNow, useShellStatus } from "@/shell/status";

const DOT = { ok: "bg-success", limited: "bg-warning", holding: "bg-destructive" } as const;
const TEXT = { ok: "text-muted-foreground", limited: "text-warning-foreground", holding: "text-destructive-foreground" } as const;

const THEMES: { value: ThemeChoice; label: string; icon: Icon }[] = [
  { value: "system", label: "System", icon: Desktop },
  { value: "light", label: "Light", icon: Sun },
  { value: "dark", label: "Dark", icon: Moon },
];

const ICON_BUTTON = "flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground outline-none transition-colors hover:bg-accent hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring";

/** One button showing the current theme; its menu picks System, Light or Dark. */
export function ThemeMenu() {
  const theme = useSyncExternalStore(subscribeTheme, getTheme);
  const current = THEMES.find((t) => t.value === theme) ?? THEMES[0];
  const CurrentIcon = current.icon;
  return (
    <DropdownMenu>
      <DropdownMenuTrigger data-slot="theme-menu" aria-label={`Theme: ${current.label}`} title={`Theme: ${current.label}`} className={ICON_BUTTON}>
        <CurrentIcon size={15} />
      </DropdownMenuTrigger>
      <DropdownMenuContent side="top" align="end" className="min-w-40">
        <DropdownMenuLabel>Theme</DropdownMenuLabel>
        <DropdownMenuRadioGroup value={theme} onValueChange={(value) => setTheme(value as ThemeChoice)}>
          {THEMES.map(({ value, label, icon: ItemIcon }) => (
            <DropdownMenuRadioItem key={value} value={value}>
              <ItemIcon size={14} /> {label}
            </DropdownMenuRadioItem>
          ))}
        </DropdownMenuRadioGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

export function SidebarStatus() {
  const status = useShellStatus();
  const { connected } = useWorkbench();
  const now = useNow(1000, status.working !== null);
  const { machine } = status;
  const summary = !connected
    ? "Reconnecting to Neko…"
    : status.working
      ? `Working ${elapsed(now - status.working.startedMs)}`
      : status.needsUser > 0
        ? `${status.needsUser} waiting on you`
        : "Nothing waiting on you";
  const running = status.counts.running;
  const queued = status.counts.upNext;
  const tasks = (running === 0 ? "Nothing running" : `${running} running`) + (queued > 0 ? ` · ${queued} up next` : "");
  return (
    <div data-slot="status-bar" aria-label="Status" role="group" className="flex flex-col gap-1 px-1 text-[12.5px]">
      <div className="flex items-center gap-1 group-data-[collapsible=icon]:flex-col">
        <Popover>
          <PopoverTrigger
            data-slot="machine"
            data-level={machine.level}
            aria-label={`Mac health: ${machine.title}`}
            title="How much the Mac can take right now"
            className={cn("flex min-w-0 flex-1 items-center gap-2 rounded-md px-2 py-1 text-left outline-none transition-colors hover:bg-accent focus-visible:ring-2 focus-visible:ring-ring group-data-[collapsible=icon]:flex-none group-data-[collapsible=icon]:justify-center", TEXT[machine.level])}
          >
            <span aria-hidden data-slot="connection" data-connected={connected} className={cn("size-2 shrink-0 rounded-full", !connected ? "bg-warning" : machine.known ? DOT[machine.level] : "bg-muted-foreground/50")} />
            <span className="truncate group-data-[collapsible=icon]:sr-only">{machine.title}</span>
          </PopoverTrigger>
          <PopoverContent side="top" align="start" className="w-80 gap-3 p-4">
            <h2 className="text-[15px] font-semibold text-strong">{machine.title}</h2>
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
        <ThemeMenu />
        <button type="button" onClick={openShortcuts} aria-label="Show keyboard shortcuts" title="Keyboard shortcuts (?)" className={cn(ICON_BUTTON, "group-data-[collapsible=icon]:hidden")}>
          <Kbd>?</Kbd>
        </button>
      </div>
      <p data-slot="summary" className="truncate px-2 pb-1 text-[12px] group-data-[collapsible=icon]:hidden">
        <span className={!connected ? "text-warning-foreground" : status.needsUser > 0 ? "text-warning-foreground" : "text-muted-foreground"}>{summary}</span>
        <span className="text-muted-foreground"> · {tasks}</span>
      </p>
    </div>
  );
}
