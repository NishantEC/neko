// Pieces the pack detail, the Add pack preview and the Update/Review dialogs
// share. Copy is the daemon's words; nothing here sends a request.
import { Fragment, type ReactNode } from "react";
import { Button } from "@/ui/button";
import { Copy, WarningCircle } from "@/ui/icons";
import { copyText } from "@/shell/actions";
import { navigate } from "@/router";
import type { PackContents, PackInfo, PackRequirement, PluginPower, RoutineText } from "@/setup/packs";

export function Heading({ children }: { children: ReactNode }) {
  return <h4 className="text-[12.5px] font-medium text-muted-foreground">{children}</h4>;
}

/** A command or path, in monospace, selectable. */
export function Mono({ children }: { children: ReactNode }) {
  return <code className="block select-text break-all rounded-lg bg-foreground/[0.05] px-3 py-2 font-mono text-[12px] text-foreground">{children}</code>;
}

export function Requirements({ items }: { items: PackRequirement[] }) {
  if (items.length === 0) return null;
  return (
    <div className="flex flex-col gap-2" data-slot="pack-requirements">
      <Heading>Requirements</Heading>
      <ul className="flex flex-col gap-3">
        {items.map((r) => (
          <li key={r.name} data-slot="pack-requirement" data-found={r.found ? "true" : "false"} className="flex flex-col gap-1.5 text-[13px]">
            <div className="flex items-baseline gap-2">
              <span className="font-mono text-[13px] text-foreground">{r.name}</span>
              <span className={r.found ? "text-muted-foreground" : "text-warning-foreground"}>{r.found ? "Found" : "Missing"}</span>
              {r.found && r.path ? <span className="min-w-0 truncate font-mono text-[12px] text-muted-foreground/70" title={r.path}>{r.path}</span> : null}
            </div>
            {!r.found && r.hint ? (
              <div className="flex items-start gap-2">
                <div className="min-w-0 flex-1"><Mono>{r.hint}</Mono></div>
                <Button variant="outline" size="xs" aria-label={`Copy the command for ${r.name}`} onClick={() => void copyText(r.hint, "Copied the command")}><Copy /> Copy</Button>
              </div>
            ) : null}
          </li>
        ))}
      </ul>
    </div>
  );
}

function Names({ label, names, to, linkLabel }: { label: string; names: string[]; to?: string; linkLabel?: string }) {
  if (names.length === 0) return null;
  return (
    <div className="flex flex-col gap-0.5" data-slot="pack-contents-group">
      <div className="flex items-center gap-2">
        <span className="text-[13px] text-foreground">{label}</span>
        {to ? <Button variant="link" size="xs" onClick={() => navigate(to)}>{linkLabel}</Button> : null}
      </div>
      <ul className="flex flex-col text-[13px] text-muted-foreground">
        {names.map((n) => <li key={n} className="truncate">{n}</li>)}
      </ul>
    </div>
  );
}

/** What a pack adds (or owns), with links to the Setup pages that manage each kind. */
export function Contents({ contents, links = false }: { contents: PackContents; links?: boolean }) {
  const { connections, skills, plugin, routines } = contents;
  const luau = contents.luau ?? [];
  if (!connections.length && !skills.length && !plugin && !routines.length && !luau.length) return null;
  return (
    <div className="flex flex-col gap-2" data-slot="pack-contents">
      <Heading>{links ? "Contents" : "It will add"}</Heading>
      <div className="flex flex-col gap-3">
        <Names label="Connections" names={connections} to={links ? "tools" : undefined} linkLabel="Open Tools" />
        <Names label="Skills" names={skills} to={links ? "tools" : undefined} linkLabel="Open Skills" />
        <Names label="Plugin" names={plugin ? [plugin] : []} to={links ? "customisations" : undefined} linkLabel="Open Customisations" />
        <Names label="Routines" names={routines} to={links ? "inbox" : undefined} linkLabel="Open Routines" />
        <Names label="Luau files (stored, nothing runs them yet)" names={luau} />
      </div>
    </div>
  );
}

/** Things to read twice, one highlighted line each. */
export function Warnings({ items }: { items: string[] }) {
  if (items.length === 0) return null;
  return (
    <ul data-slot="pack-warnings" className="flex flex-col gap-1.5">
      {items.map((w) => (
        <li key={w} className="flex items-start gap-2 rounded-lg bg-warning/10 px-3 py-2 text-[13px] text-warning-foreground">
          <WarningCircle aria-hidden className="mt-0.5 size-4 shrink-0" />
          <span>{w}</span>
        </li>
      ))}
    </ul>
  );
}

/** What changed in what the pack can do, compared with the installed copy. */
export function Changes({ items }: { items: string[] }) {
  if (items.length === 0) return null;
  return (
    <div className="flex flex-col gap-2" data-slot="pack-changes">
      <Heading>What changed</Heading>
      <ul className="flex list-disc flex-col gap-1 ps-5 text-[13px] text-foreground/90">
        {items.map((c) => <li key={c}>{c}</li>)}
      </ul>
    </div>
  );
}

function PluginPowerBlock({ plugin }: { plugin: PluginPower }) {
  const rows: [string, string[]][] = [["Pages", plugin.pages], ["Viewers", plugin.viewers], ["Reply blocks", plugin.fences]];
  return (
    <div className="flex flex-col gap-2" data-slot="pack-plugin">
      <Heading>Plugin: {plugin.title}</Heading>
      <p className="text-[12.5px] text-muted-foreground">It runs inside Neko with access to Neko’s own API. It is installed switched off.</p>
      <dl className="grid grid-cols-[7rem_1fr] gap-x-4 gap-y-1 text-[13px]">
        {rows.filter(([, v]) => v.length > 0).map(([k, v]) => (
          <Fragment key={k}><dt className="text-muted-foreground">{k}</dt><dd>{v.join(", ")}</dd></Fragment>
        ))}
      </dl>
      {plugin.prompt ? (
        <div className="flex flex-col gap-1">
          <span className="text-[12.5px] text-muted-foreground">Text it adds to the model’s instructions</span>
          <Mono><span className="whitespace-pre-wrap">{plugin.prompt}</span></Mono>
        </div>
      ) : null}
    </div>
  );
}

function RoutineTexts({ items }: { items: RoutineText[] }) {
  if (items.length === 0) return null;
  return (
    <div className="flex flex-col gap-2" data-slot="pack-routine-texts">
      <Heading>Routine instructions</Heading>
      <ul className="flex flex-col gap-3">
        {items.map((r) => (
          <li key={r.name} className="flex flex-col gap-1 text-[13px]">
            <div className="flex items-baseline gap-2"><span className="text-foreground">{r.name}</span><span className="text-muted-foreground">{r.cadence}</span></div>
            <Mono><span className="whitespace-pre-wrap">{r.instruction}</span></Mono>
          </li>
        ))}
      </ul>
    </div>
  );
}

/**
 * Everything a person should read before saying yes: warnings, what changed,
 * permissions, what it runs, what it adds, plugin power, routine texts and
 * requirements. Shared by Add pack, Update and Review.
 */
export function PackBody({ info, installedVersion, changes = [] }: { info: PackInfo; installedVersion?: string | null; changes?: string[] }) {
  const runs = info.runs ?? [];
  return (
    <div className="flex flex-col gap-5" data-slot="pack-body">
      <div className="flex flex-col gap-1">
        <p className="text-[13px] text-muted-foreground">
          {info.title} · {info.version}
          {info.licence ? ` · ${info.licence}` : ""}
          {info.homepage ? <> · <a href={info.homepage} target="_blank" rel="noreferrer" className="underline underline-offset-4 hover:text-foreground">{info.homepage}</a></> : null}
        </p>
        {info.description ? <p className="text-[13px] text-foreground/85">{info.description}</p> : null}
        {installedVersion ? <p data-slot="pack-already" className="text-[13px] text-warning-foreground">Version {installedVersion} is already installed.</p> : null}
      </div>
      <Warnings items={info.warnings ?? []} />
      <Changes items={changes} />
      <div className="flex flex-col gap-2">
        <Heading>It will be allowed to</Heading>
        <ul className="flex list-disc flex-col gap-1 ps-5 text-[13px] text-foreground/90">
          {info.permissions.map((p) => <li key={p}>{p}</li>)}
          {info.permissions.length === 0 ? <li className="list-none text-muted-foreground">No permissions listed.</li> : null}
        </ul>
      </div>
      {runs.length > 0 ? (
        <div className="flex flex-col gap-2" data-slot="pack-runs">
          <Heading>What it runs</Heading>
          <div className="flex flex-col gap-1.5">{runs.map((line) => <Mono key={line}>{line}</Mono>)}</div>
        </div>
      ) : null}
      <Contents contents={info.contents} />
      {info.plugin ? <PluginPowerBlock plugin={info.plugin} /> : null}
      <RoutineTexts items={info.routine_texts ?? []} />
      <p className="text-[12.5px] text-muted-foreground">Routines are installed switched off. Tools are granted to no workspace until you grant them in Tools.</p>
      <Requirements items={info.requirements} />
    </div>
  );
}
