// Phone: how Neko reaches you when something needs you and you are away from
// the Mac. Apple Reminders (the installed app puts a reminder in a "Neko"
// list) and ntfy (optional). One line per row, no subtitles (DESIGN.md §8).
// docs/knowledge/phone-push.md.
import { useCallback, useEffect, useState } from "react";
import { request, type Json } from "@/api";
import { Button } from "@/ui/button";
import { Input } from "@/ui/input";
import { Panel } from "@/setup/kit";
import { ErrorNote, Pick, Section, SettingRow, Toggle, type Option } from "@/setup/kit";
import { relativeTime } from "@/setup/lib";

interface Settings {
  reminders: boolean;
  ntfy: boolean;
  ntfy_server: string;
  ntfy_topic: string;
  titles_only: boolean;
  quiet_enabled: boolean;
  quiet_from_hour: number;
  quiet_to_hour: number;
  open_url: string;
  digest: boolean;
  digest_hour: number;
  digest_minute: number;
}
interface Status {
  reminders_access: "NotAsked" | "Granted" | "Denied";
  last_push_ms: number | null;
  last_channel: string;
  last_error: string | null;
  held: number;
  subscribe_link: string;
}

const HOURS: Option[] = Array.from({ length: 24 }, (_, h) => ({ value: String(h), label: `${String(h).padStart(2, "0")}:00` }));

function accessLine(settings: Settings, status: Status): string {
  if (status.reminders_access === "Granted") return "Reminders allowed";
  if (status.reminders_access === "Denied") return "Reminders blocked: allow Neko in System Settings → Privacy & Security → Reminders";
  return settings.reminders ? "Waiting for the app to ask for Reminders access" : "Reminders not asked yet";
}

function CopyButton({ text, label }: { text: string; label: string }) {
  const [copied, setCopied] = useState(false);
  return (
    <Button
      variant="ghost"
      size="sm"
      aria-label={label}
      onClick={() => {
        void navigator.clipboard?.writeText(text).then(() => {
          setCopied(true);
          setTimeout(() => setCopied(false), 1500);
        });
      }}
    >
      {copied ? "Copied" : "Copy"}
    </Button>
  );
}

export function PhoneSection() {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [status, setStatus] = useState<Status | null>(null);
  const [server, setServer] = useState("");
  const [openUrl, setOpenUrl] = useState("");
  const [digestTime, setDigestTime] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [test, setTest] = useState<string[] | null>(null);

  const take = useCallback((reply: Json) => {
    if (!reply?.Phone) return;
    setSettings(reply.Phone.settings);
    setStatus(reply.Phone.status);
    setServer((current) => current || reply.Phone.settings.ntfy_server);
    setOpenUrl((current) => current || reply.Phone.settings.open_url || "");
    setDigestTime((current) => current || `${String(reply.Phone.settings.digest_hour).padStart(2, "0")}:${String(reply.Phone.settings.digest_minute).padStart(2, "0")}`);
  }, []);

  useEffect(() => {
    let alive = true;
    const load = () => request("PhoneSettings").then((r) => { if (alive) take(r); }, (e) => { if (alive) setError(e instanceof Error ? e.message : String(e)); });
    void load();
    const id = setInterval(() => { if (!document.hidden) void load(); }, 10_000);
    return () => { alive = false; clearInterval(id); };
  }, [take]);

  const save = async (change: Partial<Settings>) => {
    if (!settings) return;
    setBusy(true);
    setError(null);
    try {
      const next = { ...settings, ...change };
      const reply = await request({ SetPhoneSettings: { settings: next } });
      take(reply);
      if ("ntfy_server" in change) setServer(reply.Phone.settings.ntfy_server);
      if ("open_url" in change) setOpenUrl(reply.Phone.settings.open_url);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const sendTest = async () => {
    setBusy(true);
    setError(null);
    setTest(null);
    try {
      const reply = await request("PhoneTest");
      setTest(reply?.PhoneTest?.lines ?? []);
      take(await request("PhoneSettings"));
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  if (!settings || !status) {
    return <div className="flex flex-col gap-8">{error ? <ErrorNote error={error} onDismiss={() => setError(null)} /> : <p className="text-[13px] text-muted-foreground">Loading…</p>}</div>;
  }

  return (
    <div className="flex flex-col gap-8" data-slot="phone-settings">
      <Section title="Channels">
        <Panel>
          <div className="flex flex-col">
            <SettingRow id="phone-reminders" title="Apple Reminders">
              <Toggle checked={settings.reminders} label="Apple Reminders" disabled={busy} onChange={(v) => void save({ reminders: v })} />
            </SettingRow>
            <SettingRow id="phone-ntfy" title="ntfy">
              <Toggle checked={settings.ntfy} label="ntfy" disabled={busy} onChange={(v) => void save({ ntfy: v })} />
            </SettingRow>
          </div>
        </Panel>
        <p data-slot="phone-replies" className="px-3 pt-2 text-[12.5px] text-muted-foreground">
          Replies need Apple Reminders: complete a reminder to take its default, or add a line that starts with &gt; to answer in words. ntfy only notifies, because answering through it would mean opening your Mac to the internet.
        </p>
      </Section>
      {settings.ntfy ? (
        <Section title="ntfy">
          <Panel>
            <div className="flex flex-col" data-slot="phone-ntfy">
              <SettingRow id="phone-ntfy-server" title="Server">
                <Input
                  aria-label="ntfy server"
                  className="w-64"
                  value={server}
                  disabled={busy}
                  onChange={(e) => setServer(e.target.value)}
                  onBlur={() => { if (server.trim() !== settings.ntfy_server) void save({ ntfy_server: server }); }}
                  onKeyDown={(e) => { if (e.key === "Enter") (e.target as HTMLInputElement).blur(); }}
                />
              </SettingRow>
              <SettingRow id="phone-ntfy-topic" title="Topic">
                <span className="select-text break-all font-mono text-[12.5px] text-muted-foreground" data-slot="phone-topic">{settings.ntfy_topic}</span>
                <CopyButton text={settings.ntfy_topic} label="Copy topic" />
                <Button variant="ghost" size="sm" disabled={busy} onClick={() => void save({ ntfy_topic: "" })}>New topic</Button>
              </SettingRow>
              <SettingRow id="phone-ntfy-link" title="Subscribe">
                <a className="select-text break-all font-mono text-[12.5px] text-primary hover:underline" href={status.subscribe_link} data-slot="phone-link">{status.subscribe_link}</a>
                <CopyButton text={status.subscribe_link} label="Copy subscribe link" />
              </SettingRow>
              <SettingRow id="phone-ntfy-titles" title="Send titles only">
                <Toggle checked={settings.titles_only} label="Send titles only" disabled={busy} onChange={(v) => void save({ titles_only: v })} />
              </SettingRow>
              <SettingRow id="phone-ntfy-open" title="Open in Neko address">
                <Input
                  aria-label="Open in Neko address"
                  className="w-64"
                  placeholder="Empty: no button"
                  value={openUrl}
                  disabled={busy}
                  onChange={(e) => setOpenUrl(e.target.value)}
                  onBlur={() => { if (openUrl.trim() !== settings.open_url) void save({ open_url: openUrl }); }}
                  onKeyDown={(e) => { if (e.key === "Enter") (e.target as HTMLInputElement).blur(); }}
                />
              </SettingRow>

            </div>
          </Panel>
        </Section>
      ) : null}
      <Section title="Morning digest">
        <Panel>
          <div className="flex flex-col" data-slot="phone-digest">
            <SettingRow id="phone-digest" title="Morning digest">
              <Toggle checked={settings.digest} label="Morning digest" disabled={busy} onChange={(v) => void save({ digest: v })} />
              <Input
                aria-label="Digest time"
                type="time"
                className="w-28"
                value={digestTime}
                disabled={busy || !settings.digest}
                onChange={(e) => setDigestTime(e.target.value)}
                onBlur={() => {
                  const [hour, minute] = digestTime.split(":").map((n) => Number(n));
                  if (Number.isFinite(hour) && Number.isFinite(minute) && (hour !== settings.digest_hour || minute !== settings.digest_minute)) void save({ digest_hour: hour, digest_minute: minute });
                }}
              />
            </SettingRow>
          </div>
        </Panel>
      </Section>
      <Section title="When">
        <Panel>
          <div className="flex flex-col">
            <SettingRow id="phone-quiet" title="Quiet hours">
              <Toggle checked={settings.quiet_enabled} label="Quiet hours" disabled={busy} onChange={(v) => void save({ quiet_enabled: v })} />
              <Pick label="Quiet hours start" className="min-w-24" value={String(settings.quiet_from_hour)} options={HOURS} disabled={busy || !settings.quiet_enabled} onChange={(v) => void save({ quiet_from_hour: Number(v) })} />
              <span className="text-[12.5px] text-muted-foreground">to</span>
              <Pick label="Quiet hours end" className="min-w-24" value={String(settings.quiet_to_hour)} options={HOURS} disabled={busy || !settings.quiet_enabled} onChange={(v) => void save({ quiet_to_hour: Number(v) })} />
            </SettingRow>
            <SettingRow id="phone-test" title="Send a test">
              <Button size="sm" variant="outline" disabled={busy || (!settings.reminders && !settings.ntfy)} onClick={() => void sendTest()}>Send a test</Button>
            </SettingRow>
          </div>
        </Panel>
      </Section>
      <Section title="Status">
        <div className="flex flex-col gap-1 px-0 text-[13px] text-muted-foreground" data-slot="phone-status">
          <p data-slot="phone-access">{accessLine(settings, status)}</p>
          <p data-slot="phone-last-push">{status.last_push_ms ? `Last push: ${status.last_channel}, ${relativeTime(status.last_push_ms)}` : "No push sent yet"}{status.held ? `, ${status.held} waiting for quiet hours to end` : ""}</p>
          {status.last_error ? <p data-slot="phone-error" className="text-warning-foreground">Last error: {status.last_error}</p> : null}
          {test ? test.map((line) => <p key={line} data-slot="phone-test-line">{line}</p>) : null}
        </div>
      </Section>
      {error ? <ErrorNote error={error} title="Couldn’t save" onDismiss={() => setError(null)} /> : null}
    </div>
  );
}
