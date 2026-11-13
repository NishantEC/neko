import { useEffect, useState } from "react";
import { request } from "@/api";
import { Button } from "@/ui/button";
import { openOnboarding } from "@/onboarding/open";
import { Skeleton } from "@/ui/skeleton";
import { Kbd } from "@/ui/kbd";
import { Panel } from "@/setup/kit";
import { ErrorNote, Section, SettingRow, Toggle } from "@/setup/kit";
import { activate, providerRows, rec, str, useLoad, useRun } from "@/setup/lib";

const MODIFIERS: Record<string, string> = { Cmd: "⌘", Alt: "⌥", Ctrl: "⌃", Shift: "⇧" };

/** Routines and Calendar are off until switched on; the first-run setup offers both, and so does this. */
function SetupSection() {
  const [routines, setRoutines] = useState<boolean | null>(null);
  const [calendar, setCalendar] = useState<boolean | null>(null);
  const { run, busy, error, clear } = useRun();
  useEffect(() => {
    request("Routines").then((r) => setRoutines(r?.Routines?.routines_on !== false), () => setRoutines(null));
    request("CalendarStatus").then((r) => setCalendar(!!r?.CalendarStatus?.enabled), () => setCalendar(null));
  }, []);
  return (
    <Section title="Setup">
      <Panel>
        <div className="flex flex-col">
          <SettingRow id="routines-on" title="Routines" description="Background checks of messages, pull requests and your day. Off until you turn them on.">
            {routines === null ? <Skeleton className="h-5 w-9" /> : (
              <Toggle checked={routines} label="Routines" disabled={busy} onChange={(value) => void run(async () => {
                const reply = await request({ SetRoutinesOn: { enabled: value } });
                setRoutines(reply?.Routines?.routines_on !== false);
                return null;
              })} />
            )}
          </SettingRow>
          <SettingRow id="calendar-on" title="Calendar" description="Show your macOS Calendar beside Neko’s work. macOS asks once.">
            {calendar === null ? <Skeleton className="h-5 w-9" /> : (
              <Toggle checked={calendar} label="Calendar" disabled={busy} onChange={(value) => void run(async () => {
                const reply = await request({ SetCalendarEnabled: { enabled: value } });
                setCalendar(!!reply?.CalendarStatus?.enabled);
                return null;
              })} />
            )}
          </SettingRow>
          <SettingRow id="first-run-setup" title="First-run setup" description="Agents, workspaces, routines and phone reminders, one calm pass.">
            <Button variant="outline" size="sm" onClick={() => openOnboarding("welcome")}>Show again</Button>
          </SettingRow>
        </div>
        {error ? <div className="px-3 py-2"><ErrorNote error={error} onDismiss={clear} /></div> : null}
      </Panel>
    </Section>
  );
}

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
      <SetupSection />
      <Section title="Summon & startup">
        <Panel>
          <div className="flex flex-col">
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
          {error || preferences.error ? <div className="px-3 py-2"><ErrorNote error={error ?? preferences.error} onDismiss={clear} /></div> : null}
        </Panel>
      </Section>
    </div>
  );
}
