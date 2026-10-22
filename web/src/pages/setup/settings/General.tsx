import { useSyncExternalStore } from "react";
import { request } from "@/api";
import { Button } from "@/ui/button";
import { Skeleton } from "@/ui/skeleton";
import { Kbd } from "@/ui/kbd";
import { CircleHalf, Moon, Sparkle, Sun } from "@/ui/icons";
import { Panel } from "@/setup/kit";
import { getTheme, setTheme, subscribeTheme, type ThemeChoice } from "@/theme";
import { ErrorNote, Section, SettingRow, Toggle } from "@/setup/kit";
import { activate, providerRows, rec, str, useLoad, useRun } from "@/setup/lib";

const MODIFIERS: Record<string, string> = { Cmd: "⌘", Alt: "⌥", Ctrl: "⌃", Shift: "⇧" };

export function GeneralSection() {
  const theme = useSyncExternalStore(subscribeTheme, getTheme);
  const preferences = useLoad((signal) => providerRows("preference", signal));
  const hotkey = useLoad(async (signal) => rec((await request("GetHotkey", signal))?.Hotkey?.config?.combo));
  const { run, busy, error, clear } = useRun();

  const login = preferences.data?.find((r) => r.id === "launch-at-login");
  const choices: [ThemeChoice, string, typeof Sun][] = [["system", "System", CircleHalf], ["light", "Light", Sun], ["dark", "Dark", Moon], ["glass", "Glass", Sparkle]];
  const combo = hotkey.data;
  const keys = combo && Object.keys(combo).length
    ? [...(Array.isArray(combo.modifiers) ? combo.modifiers : []).map((m: string) => MODIFIERS[m] ?? m), str(combo.key).replace(/^Key/, "")]
    : null;

  return (
    <div className="flex flex-col gap-8">
      <Section eyebrow="Look" title="Appearance">
        <Panel>
          <SettingRow id="appearance" title="Appearance" description="System follows your Mac’s light or dark appearance. This changes the web app only; the native app has its own switch.">
            <div role="radiogroup" aria-label="Appearance" className="flex gap-1.5">
              {choices.map(([value, label, Icon]) => (
                <Button key={value} role="radio" aria-checked={theme === value} variant={theme === value ? "secondary" : "outline"} size="sm" onClick={() => setTheme(value)}>
                  <Icon /> {label}
                </Button>
              ))}
            </div>
          </SettingRow>
        </Panel>
      </Section>
      <Section eyebrow="Quick access" title="Summon & startup">
        <Panel>
          <div className="divide-y divide-border/70">
            <SettingRow id="summon-shortcut" title="Summon shortcut" description="Recording a new shortcut needs the native app: macOS only lets that app register global keys.">
              {hotkey.loading ? <Skeleton className="h-6 w-20" /> : keys ? <span className="flex gap-1">{keys.map((k, i) => <Kbd key={i}>{k}</Kbd>)}</span> : <span className="text-[13px] text-muted-foreground">Unavailable</span>}
            </SettingRow>
            <SettingRow id="launch-at-login" title="Launch at login" description="Opens Neko when you log in. Writes a login item for this Mac.">
              {preferences.loading ? <Skeleton className="h-5 w-9" /> : login ? (
                <Toggle
                  checked={str(login.accessory) === "On"}
                  label="Launch at login"
                  disabled={busy}
                  onChange={() => void run(async () => { await activate("preference", "launch-at-login"); preferences.reload(); return null; })}
                />
              ) : <span className="text-[13px] text-muted-foreground">Unavailable</span>}
            </SettingRow>
          </div>
          {error || preferences.error ? <div className="border-t border-border/70 p-4"><ErrorNote error={error ?? preferences.error} onDismiss={clear} /></div> : null}
        </Panel>
      </Section>
    </div>
  );
}
