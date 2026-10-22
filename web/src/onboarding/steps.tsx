// The six steps of the first-run tour (OnboardingView.swift has three; the web
// tour adds the runtime and the optional import, as the plan asks).
import { useEffect, useMemo, useState } from "react";
import { accept, command, request, type Json } from "@/api";
import { useWorkbench } from "@/useStore";
import { navigate } from "@/router";
import { Button } from "@/ui/button";
import { Input } from "@/ui/input";
import { Switch } from "@/ui/switch";
import { Skeleton } from "@/ui/skeleton";
import { ArrowRight, ArrowSquareOut, Check, CheckCircle, CircleNotch, Clipboard, FolderSimple, Lock, Plus, Terminal } from "@/ui/icons";
import { Hand } from "@phosphor-icons/react";
import { EditorialHeading, MonoLabel, Panel } from "@/editorial";
import { cn } from "@/ui/lib/utils";
import { parseCatalog, type Catalog } from "@/chat/ChatInputRuntime";
import neko from "@/assets/neko-mark.svg";
import type { OnboardingStep } from "@/onboarding/open";

export const STEPS: { id: OnboardingStep; label: string }[] = [
  { id: "welcome", label: "Welcome" },
  { id: "permissions", label: "Permissions" },
  { id: "workspace", label: "Workspace" },
  { id: "runtime", label: "Runtime" },
  { id: "import", label: "Import" },
  { id: "finish", label: "Done" },
];

export function StepHead({ title, subtitle, headingRef }: { eyebrow: string; title: React.ReactNode; subtitle: string; headingRef?: React.Ref<HTMLDivElement> }) {
  return (
    <div className="flex flex-col gap-2">
      <div ref={headingRef} tabIndex={-1} className="outline-none">
        <EditorialHeading as="h2" className="text-[clamp(1.6rem,2vw+0.6rem,2.25rem)]">
          {title}
        </EditorialHeading>
      </div>
      <p className="max-w-[56ch] text-[14.5px] leading-relaxed text-muted-foreground">{subtitle}</p>
    </div>
  );
}

function Row({ icon, title, detail, children }: { icon: React.ReactNode; title: string; detail: React.ReactNode; children?: React.ReactNode }) {
  return (
    <div className="flex items-start gap-3.5 px-5 py-4">
      <span className="mt-0.5 grid size-8 shrink-0 place-items-center rounded-full bg-foreground/[0.06] text-foreground">{icon}</span>
      <div className="min-w-0 flex-1">
        <div className="text-[14px] font-medium text-strong">{title}</div>
        <div className="mt-0.5 text-[13px] leading-snug text-muted-foreground">{detail}</div>
      </div>
      {children ? <div className="shrink-0 self-center">{children}</div> : null}
    </div>
  );
}

// ---------------------------------------------------------------------------
// 1. Welcome
// ---------------------------------------------------------------------------

export function Welcome({ onNext, headingRef }: { onNext(): void; headingRef: React.Ref<HTMLDivElement> }) {
  return (
    <div className="flex flex-col items-center gap-6 py-6 text-center">
      <span
        aria-hidden
        className="size-16 rounded-2xl bg-card p-3.5 text-strong shadow-xs ring-1 ring-border"
        style={{ backgroundImage: "none" }}
      >
        <span className="block size-full" style={{ background: "currentColor", maskImage: `url(${neko})`, WebkitMaskImage: `url(${neko})`, maskSize: "contain", WebkitMaskSize: "contain", maskRepeat: "no-repeat", WebkitMaskRepeat: "no-repeat" }} />
      </span>
      <div className="flex flex-col items-center gap-3">
        <div ref={headingRef} tabIndex={-1} className="outline-none">
          <EditorialHeading as="h2" className="text-[clamp(1.9rem,3vw+0.5rem,2.75rem)]">
            A little more off your mind.
          </EditorialHeading>
        </div>
        <p className="max-w-[46ch] text-[15px] leading-relaxed text-muted-foreground">
          Connect your projects and tools, hand off work, and see what needs your attention. Neko works in the background and asks before it changes anything.
        </p>
      </div>
      <Button type="button" size="lg" className="mt-1" onClick={onNext}>
        Get started <ArrowRight />
      </Button>
    </div>
  );
}

// ---------------------------------------------------------------------------
// 2. Permissions
// ---------------------------------------------------------------------------

const PROTECTED = /^(\/Users\/[^/]+)\/(Desktop|Documents|Downloads|Library\/Mobile Documents)(\/|$)/;
const SETTINGS_FULL_DISK = "x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles";
const SETTINGS_ACCESSIBILITY = "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility";

/** Folders macOS protects per app that hold a workspace (PermissionCatalog.protectedFolders). */
export function protectedFolders(snapshot: Json | null): { folder: string; count: number }[] {
  const paths: string[] = (snapshot?.workspaces ?? []).flatMap((w: Json) => {
    const listed: string[] = snapshot?.workspace_folders?.[w.id] ?? [];
    return listed.length ? listed : [w.repository];
  });
  const found = new Map<string, number>();
  for (const path of paths) {
    const m = PROTECTED.exec(path ?? "");
    if (m) found.set(m[2] === "Library/Mobile Documents" ? "iCloud Drive" : m[2], (found.get(m[2]) ?? 0) + 1);
  }
  return [...found].map(([folder, count]) => ({ folder, count }));
}

export function Permissions({ headingRef }: { headingRef: React.Ref<HTMLDivElement> }) {
  const { snapshot } = useWorkbench();
  const [clipboard, setClipboard] = useState<boolean | null>(null);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    request("GetClipboardHistoryEnabled").then((r) => setClipboard(!!r?.ClipboardHistoryEnabled?.enabled)).catch((e) => setError(e.message));
  }, []);
  const protectedList = useMemo(() => protectedFolders(snapshot), [snapshot]);
  async function toggle(value: boolean) {
    setPending(true);
    setError(null);
    try {
      const reply = await request({ SetClipboardHistoryEnabled: { enabled: value } });
      if (reply?.ClipboardHistoryEnabled?.enabled !== value) throw new Error("Clipboard setting was not acknowledged.");
      setClipboard(value);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
    setPending(false);
  }
  return (
    <div className="flex flex-col gap-6">
      <StepHead eyebrow="Permissions" title={<>Let Neko work on your Mac.</>} subtitle="Each of these is optional, and you can change them later in Settings." headingRef={headingRef} />
      <Panel>
        <div className="divide-y divide-border/70">
          <Row
            icon={<Lock />}
            title="Full Disk Access"
            detail={
              <>
                Lets tasks open protected folders — Documents, Desktop, Downloads, iCloud Drive — without macOS asking every time.
                <ol className="mt-2 list-decimal space-y-0.5 ps-4 text-[12.5px]">
                  <li>Open System Settings → Privacy &amp; Security → Full Disk Access.</li>
                  <li>Switch Neko on (use + to add it if it is not listed).</li>
                  <li>Quit and reopen Neko if macOS asks you to.</li>
                </ol>
                <span className="mt-2 block text-[12px]">
                  {protectedList.length
                    ? `${protectedList.map((p) => `${p.folder} (${p.count})`).join(", ")} hold your workspaces, so macOS will ask on first use without it.`
                    : "None of your workspaces are in a folder macOS protects, so tasks open them without asking."}{" "}
                  A web page cannot read this permission; the Neko app shows whether it is on.
                </span>
              </>
            }
          >
            <Button type="button" variant="outline" size="sm" render={<a href={SETTINGS_FULL_DISK} />}>
              Open Settings <ArrowSquareOut />
            </Button>
          </Row>
          <Row
            icon={<Hand />}
            title="Accessibility"
            detail="Lets Neko paste for you and read the text you have selected. Nothing else on screen is read. macOS asks you to confirm."
          >
            <Button type="button" variant="outline" size="sm" render={<a href={SETTINGS_ACCESSIBILITY} />}>
              Open Settings <ArrowSquareOut />
            </Button>
          </Row>
          <Row icon={<Clipboard />} title="Clipboard history" detail="Saved only on this Mac. Off by default; turning it off stops new capture.">
            <Switch
              checked={clipboard ?? false}
              disabled={clipboard === null || pending}
              onCheckedChange={(on) => void toggle(!!on)}
              aria-label="Clipboard history"
            />
          </Row>
        </div>
      </Panel>
      {error ? <p role="alert" className="text-[13px] text-destructive">{error}</p> : null}
    </div>
  );
}

// ---------------------------------------------------------------------------
// 3. Workspace
// ---------------------------------------------------------------------------

export function Workspace({ headingRef }: { headingRef: React.Ref<HTMLDivElement> }) {
  const { snapshot } = useWorkbench();
  const workspaces: Json[] = snapshot?.workspaces ?? [];
  const [path, setPath] = useState("");
  const [name, setName] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  // The home folder is only knowable from a path that already sits in it.
  const home = useMemo(() => {
    for (const w of workspaces) {
      const m = /^(\/Users\/[^/]+)(\/|$)/.exec(w.repository ?? "");
      if (m) return m[1];
    }
    return null;
  }, [workspaces]);
  const homeAdded = home ? workspaces.some((w) => w.repository === home) : false;

  async function add(folder: string, label?: string) {
    const clean = folder.trim().replace(/\/+$/, "");
    if (!clean.startsWith("/")) return setError("Type the full path of a folder, starting with /, for example /Users/you/Projects/app.");
    setBusy(true);
    setError(null);
    try {
      const title = (label ?? name).trim() || clean.split("/").pop() || "Workspace";
      accept(await command("SaveWorkspaceWithFolders", {
        workspace: { id: "", name: title, repository: clean, instructions: "", away_enabled: false },
        folders: [clean],
      }));
      setPath("");
      setName("");
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
    setBusy(false);
  }

  return (
    <div className="flex flex-col gap-6">
      <StepHead
        eyebrow="Workspace"
        title={<>Where do you work?</>}
        subtitle="Choose your folders. Each workspace keeps its own tools and instructions. You can add more later."
        headingRef={headingRef}
      />
      <Panel>
        <div className="divide-y divide-border/70" data-slot="workspace-list">
          {workspaces.length === 0 ? (
            <p className="px-5 py-4 text-[13px] text-muted-foreground">No workspace yet. Add a folder below to give Neko somewhere to work.</p>
          ) : (
            workspaces.map((w) => (
              <div key={w.id} className="flex items-center gap-3 px-5 py-3">
                <FolderSimple className="size-4 text-muted-foreground" />
                <div className="min-w-0 flex-1">
                  <div className="truncate text-[14px] font-medium text-strong">{w.name}</div>
                  <div className="truncate font-mono text-[11.5px] text-muted-foreground">{w.repository}</div>
                </div>
                <CheckCircle weight="fill" className="size-4 text-success" aria-label="Added" />
              </div>
            ))
          )}
          {home && !homeAdded ? (
            <button
              type="button"
              disabled={busy}
              onClick={() => void add(home, home.split("/").pop())}
              className="flex w-full items-center gap-3 px-5 py-3 text-start outline-none transition-colors hover:bg-accent focus-visible:bg-accent"
            >
              <FolderSimple className="size-4 text-muted-foreground" />
              <span className="flex-1">
                <span className="block text-[14px] font-medium text-strong">Use my home folder</span>
                <span className="block text-[12.5px] text-muted-foreground">Recommended · one default workspace for everything in {home}</span>
              </span>
              <ArrowRight className="size-4 text-muted-foreground" />
            </button>
          ) : null}
        </div>
        <form
          className="flex flex-col gap-2 border-t border-border/70 bg-foreground/[0.02] px-5 py-4 sm:flex-row sm:items-end"
          onSubmit={(e) => {
            e.preventDefault();
            void add(path);
          }}
        >
          <label className="flex min-w-0 flex-[2] flex-col gap-1 text-[12px] text-muted-foreground">
            Folder path
            <Input value={path} onChange={(e) => setPath(e.target.value)} placeholder="/Users/you/Projects/app" spellCheck={false} autoComplete="off" className="font-mono" />
          </label>
          <label className="flex min-w-0 flex-1 flex-col gap-1 text-[12px] text-muted-foreground">
            Name (optional)
            <Input value={name} onChange={(e) => setName(e.target.value)} placeholder="app" autoComplete="off" />
          </label>
          <Button type="submit" disabled={busy || !path.trim()}>
            {busy ? <CircleNotch className="animate-spin" /> : <Plus />} Add workspace
          </Button>
        </form>
      </Panel>
      {error ? <p role="alert" data-slot="onboarding-error" className="select-text text-[13px] text-destructive">{error}</p> : null}
    </div>
  );
}

// ---------------------------------------------------------------------------
// 4. Runtime
// ---------------------------------------------------------------------------

export interface RuntimeChoice { provider: string; model: string }

export function Runtime({ choice, onChoice, headingRef }: { choice: RuntimeChoice | null; onChoice(next: RuntimeChoice | null): void; headingRef: React.Ref<HTMLDivElement> }) {
  const { snapshot } = useWorkbench();
  const [catalog, setCatalog] = useState<Catalog | null>(null);
  const [error, setError] = useState<string | null>(null);
  const load = (refresh: boolean) => {
    setError(null);
    request({ AgentModels: { refresh } })
      .then((r) => setCatalog(parseCatalog(r.AgentModels)))
      .catch((e) => setError(e instanceof Error ? e.message : String(e)));
  };
  useEffect(() => load(false), []);
  const current = snapshot?.agent_runtime;
  return (
    <div className="flex flex-col gap-6">
      <StepHead
        eyebrow="Runtime"
        title={<>Which agent does the work?</>}
        subtitle="Neko can choose for each task, or you can pin a model now. You can change it for any conversation from the composer."
        headingRef={headingRef}
      />
      <Panel>
        <div role="radiogroup" aria-label="Agent runtime" className="divide-y divide-border/70" data-slot="runtime-choices">
          <ChoiceRow
            selected={choice === null}
            title="Neko decides"
            detail={current?.provider || current?.model ? `Keeps the current default: ${[current.provider || "codex", current.model].filter(Boolean).join(" · ")}.` : "Chooses a compatible model for each task. Recommended."}
            onSelect={() => onChoice(null)}
          />
          {!catalog && !error ? (
            <div className="space-y-2 px-5 py-4" aria-busy="true" role="status" aria-label="Loading models">
              <Skeleton className="h-4 w-40" />
              <Skeleton className="h-10 w-full" />
              <Skeleton className="h-10 w-full" />
            </div>
          ) : null}
          {catalog?.sources.map((source) => (
            <div key={source.provider}>
              <div className="px-5 pt-3"><MonoLabel className="text-[10px]">{source.label}{source.connection ? ` · ${source.connection}` : ""}</MonoLabel></div>
              {!source.ready ? <p className="px-5 py-2 text-[12.5px] text-muted-foreground">{source.note ?? "This source is not ready."}</p> : null}
              {source.ready ? source.models.filter((m) => m.usable).map((m) => (
                <ChoiceRow
                  key={m.id}
                  selected={choice?.provider === source.provider && choice.model === m.id}
                  title={m.label}
                  detail={m.description ?? (m.recommended ? "Recommended" : "")}
                  onSelect={() => onChoice({ provider: source.provider, model: m.id })}
                />
              )) : null}
            </div>
          ))}
        </div>
        {error ? (
          <div className="flex items-center gap-3 border-t border-border/70 px-5 py-3">
            <p role="alert" className="flex-1 select-text text-[12.5px] text-warning-foreground">Models could not be loaded: {error}</p>
            <Button type="button" variant="outline" size="sm" onClick={() => load(true)}>Try again</Button>
          </div>
        ) : null}
      </Panel>
    </div>
  );
}

function ChoiceRow({ selected, title, detail, onSelect }: { selected: boolean; title: string; detail?: string; onSelect(): void }) {
  return (
    <button
      type="button"
      role="radio"
      aria-checked={selected}
      onClick={onSelect}
      className={cn("flex w-full items-start gap-3 px-5 py-3 text-start outline-none transition-colors hover:bg-accent focus-visible:bg-accent focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring", selected && "bg-foreground/[0.04]")}
    >
      <span className={cn("mt-0.5 grid size-4 shrink-0 place-items-center rounded-full border", selected ? "border-primary bg-primary text-primary-foreground" : "border-input")}>
        {selected ? <Check weight="bold" className="size-3" /> : null}
      </span>
      <span className="min-w-0 flex-1">
        <span className="block text-[14px] font-medium text-strong">{title}</span>
        {detail ? <span className="block text-[12.5px] leading-snug text-muted-foreground">{detail}</span> : null}
      </span>
    </button>
  );
}

// ---------------------------------------------------------------------------
// 5. Import (optional)
// ---------------------------------------------------------------------------

export function ImportStep({ headingRef }: { headingRef: React.Ref<HTMLDivElement> }) {
  const { snapshot } = useWorkbench();
  const [looking, setLooking] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [looked, setLooked] = useState(false);
  const sources: { id: string; name: string }[] = snapshot?.import_preview?.sources ?? [];
  async function look() {
    setLooking(true);
    setError(null);
    try {
      accept(await request({ Workbench: { SetupImport: "ListSources" } }));
      setLooked(true);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
    setLooking(false);
  }
  return (
    <div className="flex flex-col gap-6">
      <StepHead
        eyebrow="Optional"
        title={<>Bring your existing setup.</>}
        subtitle="Neko can read the tools, skills and schedules you already set up in other agents on this Mac, and offer them to you. Nothing is imported, and no secret is copied, until you choose it."
        headingRef={headingRef}
      />
      <Panel>
        <div className="px-5 py-4">
          <div className="flex items-start gap-3.5">
            <span className="mt-0.5 grid size-8 shrink-0 place-items-center rounded-full bg-foreground/[0.06]"><Terminal /></span>
            <div className="min-w-0 flex-1 text-[13px] leading-snug text-muted-foreground">
              <div className="text-[14px] font-medium text-strong">Look for setups on this Mac</div>
              Only names are listed here. You review and pick what to bring in on the Tools &amp; skills page.
            </div>
            <Button type="button" variant="outline" size="sm" disabled={looking} onClick={() => void look()}>
              {looking ? <CircleNotch className="animate-spin" /> : null} {looked ? "Look again" : "Look"}
            </Button>
          </div>
          {looked ? (
            <ul data-slot="import-sources" className="mt-4 flex flex-wrap gap-2" aria-label="Setups found">
              {sources.length ? sources.map((s) => (
                <li key={s.id} className="rounded-full bg-foreground/[0.06] px-3 py-1 text-[12.5px] text-foreground">{s.name}</li>
              )) : <li className="text-[13px] text-muted-foreground">Nothing found. You can skip this step.</li>}
            </ul>
          ) : null}
        </div>
        <div className="flex items-center justify-end gap-2 border-t border-border/70 bg-foreground/[0.02] px-5 py-3">
          <Button type="button" variant="ghost" size="sm" onClick={() => { navigate("tools"); }}>Open Tools &amp; skills</Button>
        </div>
      </Panel>
      {error ? <p role="alert" className="select-text text-[13px] text-destructive">{error}</p> : null}
    </div>
  );
}

// ---------------------------------------------------------------------------
// 6. Finish
// ---------------------------------------------------------------------------

export function Finish({ headingRef, runtime }: { headingRef: React.Ref<HTMLDivElement>; runtime: RuntimeChoice | null }) {
  const { snapshot } = useWorkbench();
  const count = (snapshot?.workspaces ?? []).length;
  const tips = [
    ["/", "in the composer lists commands: /plan, /stop, /remember …"],
    ["@", "mentions a workspace, a tool or a file"],
    ["⌘K", "finds any page, task or action"],
    ["?", "shows every keyboard shortcut"],
  ];
  return (
    <div className="flex flex-col gap-6">
      <StepHead eyebrow="All set" title={<>Neko is ready.</>} subtitle="Tell it what you want done. It plans, asks before anything risky, and tells you when something needs you." headingRef={headingRef} />
      <Panel>
        <dl className="divide-y divide-border/70 text-[13.5px]">
          <div className="flex items-center justify-between px-5 py-3"><dt className="text-muted-foreground">Workspaces</dt><dd className="text-strong">{count === 0 ? "None yet — add one from the Neko app or the Workspaces page" : count}</dd></div>
          <div className="flex items-center justify-between px-5 py-3"><dt className="text-muted-foreground">Agent runtime</dt><dd className="text-strong">{runtime ? runtime.model : "Neko decides"}</dd></div>
        </dl>
        <ul className="grid gap-2 border-t border-border/70 bg-foreground/[0.02] px-5 py-4 text-[13px] text-muted-foreground sm:grid-cols-2">
          {tips.map(([key, text]) => (
            <li key={key} className="flex items-baseline gap-2"><kbd className="rounded-md bg-foreground/[0.07] px-1.5 py-0.5 font-mono text-[11.5px] text-foreground">{key}</kbd><span>{text}</span></li>
          ))}
        </ul>
      </Panel>
    </div>
  );
}
