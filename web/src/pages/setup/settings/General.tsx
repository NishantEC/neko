import { request } from "@/api";
import { Skeleton } from "@/ui/skeleton";
import { Kbd } from "@/ui/kbd";
import { Panel } from "@/setup/kit";
import { ErrorNote, Section, SettingRow, Toggle } from "@/setup/kit";
import { activate, providerRows, rec, str, useLoad, useRun } from "@/setup/lib";

const MODIFIERS: Record<string, string> = { Cmd: "⌘", Alt: "⌥", Ctrl: "⌃", Shift: "⇧" };

export function GeneralSection() {
  const preferences = useLoad((signal) => providerRows("preference", signal));
  const hotkey = useLoad(async (signal) => rec((await request("GetHotkey", signal))?.Hotkey?.config?.combo));
  const { run, busy, error, clear } = useRun();

  const login = preferences.data?.find((r) => r.id === "launch-at-login");
  const combo = hotkey.data;
  const keys = combo && Object.keys(combo).length
    ? [...(Array.isArray(combo.modifiers) ? combo.modifiers : []).map((m: string) => MODIFIERS[m] ?? m), str(combo.key).replace(/^Key/, "")]
    : null;

  return (
    <div className="flex flex-col gap-8">
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
