// The steps of the first-run setup. Calm and skippable (DESIGN.md §8): a
// heading, one line, then rows with no lines or cards between them. Everything
// here is a choice the person makes; nothing is switched on for them.
//
//   welcome -> agents -> workspaces -> routines -> mac -> done
//
// Data comes from requests the daemon already answers: AgentModels (which
// agent CLIs are installed and signed in), Routines (the scopes found on disk,
// their MCP servers, the owner's first name, the routines switch), PhoneSettings,
// CalendarStatus and GetClipboardHistoryEnabled. docs/knowledge/first-run.md.
import { useCallback, useEffect, useState } from "react";
import { accept, command, request, type Json } from "@/api";
import { useWorkbench } from "@/useStore";
import { Button } from "@/ui/button";
import { Input } from "@/ui/input";
import { ArrowRight, ArrowSquareOut, CheckCircle, CircleNotch, Copy, Plus } from "@/ui/icons";
import { EditorialHeading } from "@/editorial";
import { Panel, Row, Toggle } from "@/setup/kit";
import neko from "@/assets/neko-mark.svg";
import type { OnboardingStep } from "@/onboarding/open";

export const STEPS: { id: OnboardingStep; label: string }[] = [
  { id: "welcome", label: "Welcome" },
  { id: "agents", label: "Agents" },
  { id: "workspaces", label: "Workspaces" },
  { id: "routines", label: "Routines" },
  { id: "mac", label: "This Mac" },
  { id: "finish", label: "Done" },
];

const SETTINGS_ACCESSIBILITY = "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility";

const message = (e: unknown) => (e instanceof Error ? e.message : String(e));

export function StepHead({ title, subtitle, headingRef }: { title: React.ReactNode; subtitle?: string; headingRef?: React.Ref<HTMLDivElement> }) {
  return (
    <div className="flex flex-col gap-2">
      <div ref={headingRef} tabIndex={-1} className="outline-none">
        <EditorialHeading as="h2" className="text-[clamp(1.6rem,2vw+0.6rem,2.25rem)]">
          {title}
        </EditorialHeading>
      </div>
      {subtitle ? <p className="max-w-[56ch] text-[14.5px] leading-relaxed text-muted-foreground">{subtitle}</p> : null}
    </div>
  );
}

/** A quiet group heading, the same as a page's (DESIGN.md §8 rule 2). */
function Group({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section className="flex flex-col gap-1.5">
      <h3 className="text-[12.5px] font-medium text-muted-foreground">{title}</h3>
      {children}
    </section>
  );
}

function CopyCommand({ text }: { text: string }) {
  const [copied, setCopied] = useState(false);
  return (
    <span className="flex items-center gap-1.5">
      <code className="rounded-md bg-foreground/[0.06] px-2 py-0.5 font-mono text-[12px] text-foreground select-text">{text}</code>
      <Button
        type="button"
        variant="ghost"
        size="sm"
        aria-label={`Copy ${text}`}
        onClick={() => {
          void navigator.clipboard?.writeText(text).then(() => {
            setCopied(true);
            setTimeout(() => setCopied(false), 1500);
          });
        }}
      >
        {copied ? "Copied" : <Copy aria-hidden />}
      </Button>
    </span>
  );
}

// ---------------------------------------------------------------------------
// Shared data
// ---------------------------------------------------------------------------

export interface RoutinesData {
  routines: Json[];
  scopes: Json[];
  owner_name?: string;
  routines_on?: boolean;
}

/** `Routines`: the scopes found on disk, the first name, and the routines switch. */
export function useRoutinesData() {
  const [data, setData] = useState<RoutinesData | null>(null);
  const [error, setError] = useState<string | null>(null);
  const reload = useCallback(async () => {
    try {
      const reply = await request("Routines");
      if (reply?.Routines) setData(reply.Routines as RoutinesData);
      setError(null);
    } catch (e) {
      setError(message(e));
    }
  }, []);
  useEffect(() => {
    void reload();
  }, [reload]);
  return { data, error, reload, set: setData };
}

// ---------------------------------------------------------------------------
// 1. Welcome
// ---------------------------------------------------------------------------

export function Welcome({ owner, onNext, onSkip, headingRef }: { owner?: string; onNext(): void; onSkip(): void; headingRef: React.Ref<HTMLDivElement> }) {
  return (
    <div className="flex flex-col items-center gap-6 py-6 text-center">
      <span aria-hidden className="size-16 rounded-2xl bg-card p-3.5 text-strong shadow-xs ring-1 ring-border" style={{ backgroundImage: "none" }}>
        <span className="block size-full" style={{ background: "currentColor", maskImage: `url(${neko})`, WebkitMaskImage: `url(${neko})`, maskSize: "contain", WebkitMaskSize: "contain", maskRepeat: "no-repeat", WebkitMaskRepeat: "no-repeat" }} />
      </span>
      <div className="flex flex-col items-center gap-3">
        <div ref={headingRef} tabIndex={-1} className="outline-none">
          <EditorialHeading as="h2" className="text-[clamp(1.9rem,3vw+0.5rem,2.75rem)]">
            {owner ? `Hi ${owner}. A little more off your mind.` : "A little more off your mind."}
          </EditorialHeading>
        </div>
        <p className="max-w-[46ch] text-[15px] leading-relaxed text-muted-foreground">
          Neko hands your work to the coding agents already on this Mac and tells you when something needs you. Nothing runs on its own until you say so, and every step here can be skipped.
        </p>
      </div>
      <div className="mt-1 flex items-center gap-2">
        <Button type="button" size="lg" onClick={onNext}>
          Get started <ArrowRight />
        </Button>
        <Button type="button" size="lg" variant="ghost" onClick={onSkip}>
          Skip setup
        </Button>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// 2. Agents
// ---------------------------------------------------------------------------

/** What to type to install or sign in to each agent CLI. */
export const AGENT_CLIS = [
  { provider: "claude", name: "Claude Code", install: "npm install -g @anthropic-ai/claude-code", signIn: "claude /login" },
  { provider: "codex", name: "Codex", install: "npm install -g @openai/codex", signIn: "codex login" },
  { provider: "opencode", name: "OpenCode", install: "npm install -g opencode-ai", signIn: "opencode auth login" },
] as const;

export type AgentState = "ready" | "signin" | "missing" | "error";

/** One agent CLI's state from the model catalog's `status` (snake_case). */
export function agentState(source: Json | undefined): AgentState {
  switch (source?.status) {
    case "ready": return "ready";
    case "sign_in_required": return "signin";
    case "not_installed": return "missing";
    default: return source ? "error" : "missing";
  }
}

export function Agents({ headingRef, onReady }: { headingRef: React.Ref<HTMLDivElement>; onReady(count: number): void }) {
  const [sources, setSources] = useState<Json[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [checking, setChecking] = useState(false);
  const load = useCallback(async (refresh: boolean) => {
    setChecking(true);
    try {
      const reply = await request({ AgentModels: { refresh } });
      const list: Json[] = reply?.AgentModels?.sources ?? [];
      setSources(list);
      onReady(AGENT_CLIS.filter((c) => agentState(list.find((s) => s.provider === c.provider)) === "ready").length);
      setError(null);
    } catch (e) {
      setError(message(e));
    }
    setChecking(false);
  }, [onReady]);
  useEffect(() => {
    void load(false);
  }, [load]);
  const states = AGENT_CLIS.map((c) => ({ cli: c, source: sources?.find((s) => s.provider === c.provider) }));
  const ready = states.filter((s) => agentState(s.source) === "ready").length;
  return (
    <div className="flex flex-col gap-6">
      <StepHead
        title="Which agents are on this Mac?"
        subtitle="Neko does not bring its own model. It runs the agent CLIs you already have, with your sign-in."
        headingRef={headingRef}
      />
      <Panel data-slot="agent-list">
        {sources === null && !error ? (
          <p role="status" className="flex items-center gap-2 px-3 text-[13px] text-muted-foreground"><CircleNotch className="animate-spin" aria-hidden /> Looking for agents…</p>
        ) : null}
        {sources !== null ? states.map(({ cli, source }) => {
          const state = agentState(source);
          return (
            <Row
              key={cli.provider}
              title={cli.name}
              description={
                state === "ready" ? String(source?.connection ?? "Ready")
                : state === "signin" ? "Found, not signed in"
                : state === "missing" ? "Not installed"
                : String(source?.connection ?? "Could not be read")
              }
              trailing={
                state === "ready" ? <span className="flex items-center gap-1 text-success"><CheckCircle weight="fill" aria-hidden /> Ready</span>
                : state === "signin" ? <CopyCommand text={cli.signIn} />
                : state === "missing" ? <CopyCommand text={cli.install} />
                : null
              }
            >
              {state === "signin" ? "Run this in Terminal, finish signing in, then check again." : state === "missing" ? "Optional. Install it in Terminal, then check again." : null}
            </Row>
          );
        }) : null}
        <div className="flex items-center gap-3 px-3 pt-2 text-[13px] text-muted-foreground">
          <Button type="button" variant="outline" size="sm" disabled={checking} onClick={() => void load(true)}>
            {checking ? <CircleNotch className="animate-spin" aria-hidden /> : null} Check again
          </Button>
          {sources !== null && ready === 0 ? <span data-slot="agents-none">Neko needs one of these to do work. You can finish setup now and come back.</span> : null}
        </div>
      </Panel>
      {error ? <p role="alert" data-slot="onboarding-error" className="select-text text-[13px] text-destructive">{error}</p> : null}
    </div>
  );
}

// ---------------------------------------------------------------------------
// 3. Workspaces
// ---------------------------------------------------------------------------

const homeRelative = (path: string, home?: string) => (home && path.startsWith(home) ? `~${path.slice(home.length)}` : path);

/** The names of a scope's MCP servers, each once. */
export function serverNames(scope: Json): string[] {
  return [...new Set<string>((scope.servers ?? []).map((s: Json) => String(s.name)))];
}

export function Workspaces({ headingRef, routines }: { headingRef: React.Ref<HTMLDivElement>; routines: ReturnType<typeof useRoutinesData> }) {
  const { snapshot } = useWorkbench();
  const [path, setPath] = useState("");
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const known: Json[] = snapshot?.workspaces ?? [];
  const scopes: Json[] = routines.data?.scopes ?? [];
  // Scopes with no workspace yet, then workspaces made by hand (not a scope on disk).
  const hasWorkspace = (scope: Json) => !!scope.workspace_id;
  const home = known.map((w) => /^(\/Users\/[^/]+)(\/|$)/.exec(w.repository ?? "")?.[1]).find(Boolean) ?? scopes.map((s) => /^(\/Users\/[^/]+)(\/|$)/.exec(s.folders?.[0] ?? "")?.[1]).find(Boolean);

  async function add(folder: string, name?: string) {
    const clean = folder.trim().replace(/\/+$/, "");
    if (!clean.startsWith("/")) return setError("Type the full path of a folder, starting with /, for example /Users/you/Projects/app.");
    setBusy(clean);
    setError(null);
    try {
      accept(await command("SaveWorkspaceWithFolders", {
        workspace: { id: "", name: (name ?? "").trim() || clean.split("/").pop() || "Workspace", repository: clean, instructions: "", away_enabled: false },
        folders: [clean],
      }));
      setPath("");
      await routines.reload();
    } catch (e) {
      setError(message(e));
    }
    setBusy(null);
  }

  // Workspaces the person made that no scope on disk covers.
  const covered = new Set(scopes.map((s) => s.workspace_id).filter(Boolean));
  const own = known.filter((w) => !covered.has(w.id));

  return (
    <div className="flex flex-col gap-6">
      <StepHead
        title="Where do you work?"
        subtitle="A workspace is a folder and the tools that belong to it. Neko found these on this Mac; add the ones you want, or any other folder."
        headingRef={headingRef}
      />
      <Panel data-slot="workspace-list">
        {routines.data === null && !routines.error ? (
          <p role="status" className="flex items-center gap-2 px-3 text-[13px] text-muted-foreground"><CircleNotch className="animate-spin" aria-hidden /> Looking for your folders…</p>
        ) : null}
        {scopes.map((scope) => {
          const folder = String(scope.folders?.[0] ?? "");
          const servers = serverNames(scope);
          return (
            <Row
              key={folder}
              title={String(scope.name)}
              description={`${homeRelative(folder, home)} · ${servers.length ? `${servers.length} tool server${servers.length === 1 ? "" : "s"}: ${servers.slice(0, 4).join(", ")}${servers.length > 4 ? `, +${servers.length - 4}` : ""}` : "no tool servers"}`}
              trailing={
                hasWorkspace(scope)
                  ? <span className="flex items-center gap-1 text-success"><CheckCircle weight="fill" aria-hidden /> Added</span>
                  : <Button type="button" size="sm" variant="outline" disabled={busy !== null} onClick={() => void add(folder, String(scope.name))}>{busy === folder ? <CircleNotch className="animate-spin" aria-hidden /> : <Plus aria-hidden />} Add</Button>
              }
            />
          );
        })}
        {own.map((w) => (
          <Row key={w.id} title={String(w.name)} description={homeRelative(String(w.repository), home)} trailing={<span className="flex items-center gap-1 text-success"><CheckCircle weight="fill" aria-hidden /> Added</span>} />
        ))}
        {routines.data !== null && scopes.length === 0 && own.length === 0 ? (
          <p className="px-3 text-[13px] text-muted-foreground">No project folders with tools set up were found. Add a folder below.</p>
        ) : null}
      </Panel>
      <Group title="Add a folder">
        <form
          className="flex gap-2"
          onSubmit={(e) => {
            e.preventDefault();
            void add(path);
          }}
        >
          <Input aria-label="Folder path" value={path} onChange={(e) => setPath(e.target.value)} placeholder={`${home ?? "/Users/you"}/Projects/app`} spellCheck={false} autoComplete="off" className="font-mono" />
          <Button type="submit" variant="outline" disabled={busy !== null || !path.trim()}>
            <Plus aria-hidden /> Add workspace
          </Button>
        </form>
      </Group>
      {error ? <p role="alert" data-slot="onboarding-error" className="select-text text-[13px] text-destructive">{error}</p> : null}
    </div>
  );
}

// ---------------------------------------------------------------------------
// 4. Routines
// ---------------------------------------------------------------------------

export function Routines({ headingRef, routines, agentsReady }: { headingRef: React.Ref<HTMLDivElement>; routines: ReturnType<typeof useRoutinesData>; agentsReady: number }) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const on = routines.data?.routines_on === true;
  async function change(enabled: boolean) {
    setBusy(true);
    setError(null);
    try {
      const reply = await request({ SetRoutinesOn: { enabled } });
      if (reply?.Routines) routines.set(reply.Routines as RoutinesData);
    } catch (e) {
      setError(message(e));
    }
    setBusy(false);
  }
  const scopes = (routines.data?.scopes ?? []).length;
  return (
    <div className="flex flex-col gap-6">
      <StepHead
        title="Let Neko look after your day?"
        subtitle="Routines are standing orders that run in the background: check messages, a morning brief, a sweep of pull requests, learning how you work. They use your agents and your tools, so they are off until you turn them on."
        headingRef={headingRef}
      />
      <Panel>
        <Row title="Run routines" description={on ? "On. They follow each scope’s own tools." : "Off"} trailing={<Toggle checked={on} label="Run routines" disabled={routines.data === null || busy} onChange={(value) => void change(value)} />}>
          {on ? "A workspace is added for every scope found on this Mac, and each gets its routines. Pause any of them from the Inbox." : null}
          {!on && scopes > 0 ? `Turning them on adds a workspace for each of the ${scopes} scope${scopes === 1 ? "" : "s"} found, and your home folder as Personal.` : null}
        </Row>
      </Panel>
      {agentsReady === 0 ? <p data-slot="routines-no-agent" className="text-[13px] text-muted-foreground">No agent is signed in yet, so routines would wait until one is.</p> : null}
      {error ? <p role="alert" data-slot="onboarding-error" className="select-text text-[13px] text-destructive">{error}</p> : null}
    </div>
  );
}

// ---------------------------------------------------------------------------
// 5. This Mac: Calendar, phone reminders, clipboard
// ---------------------------------------------------------------------------

export function Mac({ headingRef }: { headingRef: React.Ref<HTMLDivElement> }) {
  const [calendar, setCalendar] = useState<boolean | null>(null);
  const [phone, setPhone] = useState<Json | null>(null);
  const [clipboard, setClipboard] = useState<boolean | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    request("CalendarStatus").then((r) => setCalendar(!!r?.CalendarStatus?.enabled), (e) => setError(message(e)));
    request("PhoneSettings").then((r) => setPhone(r?.Phone?.settings ?? null), (e) => setError(message(e)));
    request("GetClipboardHistoryEnabled").then((r) => setClipboard(!!r?.ClipboardHistoryEnabled?.enabled), (e) => setError(message(e)));
  }, []);
  async function change(name: string, run: () => Promise<void>) {
    setBusy(name);
    setError(null);
    try {
      await run();
    } catch (e) {
      setError(message(e));
    }
    setBusy(null);
  }
  return (
    <div className="flex flex-col gap-6">
      <StepHead title="Reach you, and read your day" subtitle="Each of these is optional. macOS asks you once, when you switch one on." headingRef={headingRef} />
      <Panel>
        <Row
          title="Calendar"
          description="Show your meetings beside Neko’s work"
          trailing={
            <Toggle
              checked={calendar ?? false}
              label="Calendar"
              disabled={calendar === null || busy !== null}
              onChange={(value) => void change("calendar", async () => {
                const reply = await request({ SetCalendarEnabled: { enabled: value } });
                setCalendar(!!reply?.CalendarStatus?.enabled);
              })}
            />
          }
        />
        <Row
          title="Phone reminders"
          description="A reminder on your iPhone when something needs you"
          trailing={
            <Toggle
              checked={phone?.reminders ?? false}
              label="Phone reminders"
              disabled={phone === null || busy !== null}
              onChange={(value) => void change("phone", async () => {
                const reply = await request({ SetPhoneSettings: { settings: { ...phone, reminders: value } } });
                setPhone(reply?.Phone?.settings ?? phone);
              })}
            />
          }
        >
          Uses Apple Reminders through iCloud, only when you are away from the Mac. No account with Neko. Settings → Phone has quiet hours and an optional ntfy channel.
        </Row>
        <Row
          title="Clipboard history"
          description="Saved only on this Mac"
          trailing={
            <Toggle
              checked={clipboard ?? false}
              label="Clipboard history"
              disabled={clipboard === null || busy !== null}
              onChange={(value) => void change("clipboard", async () => {
                const reply = await request({ SetClipboardHistoryEnabled: { enabled: value } });
                if (reply?.ClipboardHistoryEnabled?.enabled !== value) throw new Error("Clipboard setting was not acknowledged.");
                setClipboard(value);
              })}
            />
          }
        />
        <Row
          title="Accessibility"
          description="Lets the quick panel paste for you"
          trailing={<Button type="button" variant="outline" size="sm" render={<a href={SETTINGS_ACCESSIBILITY} />}>Open Settings <ArrowSquareOut aria-hidden /></Button>}
        />
      </Panel>
      {error ? <p role="alert" data-slot="onboarding-error" className="select-text text-[13px] text-destructive">{error}</p> : null}
    </div>
  );
}

// ---------------------------------------------------------------------------
// 6. Done
// ---------------------------------------------------------------------------

export function Finish({ headingRef, agentsReady, routines }: { headingRef: React.Ref<HTMLDivElement>; agentsReady: number; routines: ReturnType<typeof useRoutinesData> }) {
  const { snapshot } = useWorkbench();
  const count = (snapshot?.workspaces ?? []).length;
  const [calendar, setCalendar] = useState<boolean | null>(null);
  const [phone, setPhone] = useState<boolean | null>(null);
  useEffect(() => {
    request("CalendarStatus").then((r) => setCalendar(!!r?.CalendarStatus?.enabled), () => setCalendar(false));
    request("PhoneSettings").then((r) => setPhone(!!r?.Phone?.settings?.reminders), () => setPhone(false));
    void routines.reload();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  const lines: [string, string][] = [
    ["Agents ready", agentsReady ? String(agentsReady) : "None yet"],
    ["Workspaces", count ? String(count) : "None yet"],
    ["Routines", routines.data?.routines_on ? "On" : "Off"],
    ["Calendar", calendar === null ? "…" : calendar ? "On" : "Off"],
    ["Phone reminders", phone === null ? "…" : phone ? "On" : "Off"],
  ];
  return (
    <div className="flex flex-col gap-6">
      <StepHead title="Neko is ready." subtitle="Tell it what you want done. It plans, asks before anything risky, and tells you when something needs you. You can open this setup again from Settings → General." headingRef={headingRef} />
      <dl className="flex flex-col gap-1" data-slot="setup-summary">
        {lines.map(([label, value]) => (
          <div key={label} className="flex items-baseline gap-3 px-3 text-[13.5px]">
            <dt className="w-40 shrink-0 text-muted-foreground">{label}</dt>
            <dd className="text-strong">{value}</dd>
          </div>
        ))}
      </dl>
      <p className="px-3 text-[13px] text-muted-foreground">
        <kbd className="rounded-md bg-foreground/[0.07] px-1.5 py-0.5 font-mono text-[11.5px] text-foreground">⌘K</kbd> finds any page or action. The quick panel opens from anywhere with <kbd className="rounded-md bg-foreground/[0.07] px-1.5 py-0.5 font-mono text-[11.5px] text-foreground">⌥ Space</kbd>.
      </p>
    </div>
  );
}
