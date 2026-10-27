// The sidebar's footer (it used to be a bar under every page): one status line,
// what the Mac can take and what waits on you (StatusSummary.swift), which opens
// the same facts MachineStrip.swift shows; then a gear with the theme and the
// keyboard shortcuts.
import { useSyncExternalStore } from "react";
import { Popover, PopoverContent, PopoverTrigger } from "@/ui/popover";
import { DropdownMenu, DropdownMenuContent, DropdownMenuGroup, DropdownMenuItem, DropdownMenuLabel, DropdownMenuRadioGroup, DropdownMenuRadioItem, DropdownMenuSeparator, DropdownMenuShortcut, DropdownMenuTrigger } from "@/ui/dropdown-menu";
import { Keyboard } from "@phosphor-icons/react";
import { Desktop, Gear, Moon, Sparkle, Sun, type Icon } from "@/ui/icons";
import { getTheme, setTheme, subscribeTheme, type ThemeChoice } from "@/theme";
import { useWorkbench } from "@/useStore";
import { cn } from "@/ui/lib/utils";
import { openShortcuts } from "@/shell/shortcuts";
import { elapsed, useNow, useShellStatus } from "@/shell/status";

const DOT = { ok: "bg-success", limited: "bg-warning", holding: "bg-destructive" } as const;
const TEXT = { ok: "text-muted-foreground", limited: "text-warning-foreground", holding: "text-destructive-foreground" } as const;

const THEMES: { value: ThemeChoice; label: string; icon: Icon }[] = [
  { value: "system", label: "System", icon: Desktop },
  { value: "light", label: "Light", icon: Sun },
  { value: "dark", label: "Dark", icon: Moon },
  { value: "glass", label: "Glass", icon: Sparkle },
];

const ICON_BUTTON = "flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground outline-none transition-colors hover:bg-accent hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring";

/** The footer's gear: the theme (System, Light, Dark or Glass) and the keyboard shortcuts. */
export function ThemeMenu() {
  const theme = useSyncExternalStore(subscribeTheme, getTheme);
  const current = THEMES.find((t) => t.value === theme) ?? THEMES[0];
  return (
    <DropdownMenu>
      <DropdownMenuTrigger data-slot="theme-menu" aria-label={`Theme and shortcuts (theme: ${current.label})`} title="Theme and shortcuts" className={ICON_BUTTON}>
        <Gear size={15} />
      </DropdownMenuTrigger>
      <DropdownMenuContent side="top" align="end" className="min-w-44">
        <DropdownMenuGroup>
          <DropdownMenuLabel>Theme</DropdownMenuLabel>
          <DropdownMenuRadioGroup value={theme} onValueChange={(value) => setTheme(value as ThemeChoice)}>
            {THEMES.map(({ value, label, icon: ItemIcon }) => (
              <DropdownMenuRadioItem key={value} value={value}>
                <ItemIcon size={14} /> {label}
              </DropdownMenuRadioItem>
            ))}
          </DropdownMenuRadioGroup>
        </DropdownMenuGroup>
        <DropdownMenuSeparator />
        <DropdownMenuItem onClick={openShortcuts}>
          <Keyboard size={14} /> Keyboard shortcuts <DropdownMenuShortcut>?</DropdownMenuShortcut>
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

/** One line, "<machine> · <what waits on you>", opening the machine's facts; then the gear. */
export function SidebarStatus() {
  const status = useShellStatus();
  const { connected } = useWorkbench();
  const now = useNow(1000, status.working !== null);
  const { machine } = status;
  const waiting = !connected
    ? "Reconnecting to Neko…"
    : status.needsUser > 0
      ? `${status.needsUser} need you`
      : status.working
        ? `Working ${elapsed(now - status.working.startedMs)}`
        : status.counts.running > 0
          ? `${status.counts.running} running`
          : "Nothing waiting";
  const line = connected ? `${machine.title} · ${waiting}` : waiting;
  return (
    <div data-slot="status-bar" aria-label="Status" role="group" className="flex items-center gap-1 px-1 pb-1 text-[12.5px] group-data-[collapsible=icon]:flex-col">
      <Popover>
        <PopoverTrigger
          data-slot="machine"
          data-level={machine.level}
          aria-label={`${line}. Show what the Mac can take`}
          title="How much the Mac can take right now"
          className="flex min-w-0 flex-1 items-center gap-2 rounded-md px-2 py-1.5 text-left text-muted-foreground outline-none transition-colors hover:bg-accent focus-visible:ring-2 focus-visible:ring-ring group-data-[collapsible=icon]:flex-none group-data-[collapsible=icon]:justify-center"
        >
          <span aria-hidden data-slot="connection" data-connected={connected} className={cn("size-2 shrink-0 rounded-full", !connected ? "bg-warning" : machine.known ? DOT[machine.level] : "bg-muted-foreground/50")} />
          <span data-slot="summary" className="line-clamp-2 leading-snug group-data-[collapsible=icon]:sr-only">
            {connected ? <><span className={TEXT[machine.level]}>{machine.title}</span> · </> : null}
            <span className={!connected || status.needsUser > 0 ? "text-warning-foreground" : undefined}>{waiting}</span>
          </span>
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
    </div>
  );
}
