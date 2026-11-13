// Skills: instruction files found in a workspace's folders. A skill gives an
// agent instructions; it never grants a tool. Enabling pins the exact text you
// reviewed, so a changed file has to be enabled again.
import { useMemo, useState } from "react";
import type { Json } from "@/api";
import { Button } from "@/ui/button";
import { Input } from "@/ui/input";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/ui/collapsible";
import { ArrowSquareOut, ArrowsClockwise, Books, CaretDown, CaretRight, MagnifyingGlass, Plus, WarningCircle } from "@/ui/icons";
import { EmptyBlock, ErrorNote, FormField, PanelFooter, Row } from "@/setup/kit";
import { Panel } from "@/setup/kit";
import { arr, skillsCmd, str, useRun } from "@/setup/lib";

const CURATOR_WORDS: Record<string, string> = {
  neko: "Neko wrote this skill. Its daily curator tidies accumulated lessons by itself and keeps what it removes in an Archive section.",
  locked: "Locked: the curator only proposes changes to this skill, and you accept them.",
  edited: "You edited this by hand, so it is locked: the curator only proposes changes, and you accept them.",
  other: "Not written by Neko: the curator only proposes changes, and you accept them.",
};

function SkillRow({ skill, workspace, enabled, run, busy, curated }: { skill: Json; workspace: string; enabled: Json[]; run: (s: () => Promise<Json>) => Promise<boolean>; busy: boolean; curated?: Json }) {
  const [open, setOpen] = useState(false);
  const record = enabled.find((e) => e.workspace_id === workspace && e.path === skill.path);
  const changed = !!record && record.content_hash !== skill.content_hash;
  return (
    <Collapsible open={open} onOpenChange={setOpen} data-slot="skill">
      <CollapsibleTrigger
        render={<button type="button" aria-label={`${str(skill.name)}, ${!record ? "available" : changed ? "changed" : "enabled"}`} className="flex min-h-11 w-full items-center gap-3 rounded-[10px] px-3 py-1.5 text-start outline-none transition-colors hover:bg-foreground/[0.03] focus-visible:ring-2 focus-visible:ring-ring" />}
      >
        <span aria-hidden className="text-muted-foreground">{open ? <CaretDown size={12} /> : <CaretRight size={12} />}</span>
        <span className="flex min-w-0 flex-1 items-baseline gap-3">
          <span className="shrink-0 text-[13.5px]">{str(skill.name)}</span>
          <span className="min-w-0 truncate text-[12.5px] text-muted-foreground" title={str(skill.description)}>{str(skill.description)}</span>
        </span>
        <span className={`shrink-0 text-[12.5px] ${changed ? "text-warning-foreground" : "text-muted-foreground/70"}`}>{!record ? "Available" : changed ? "Changed" : "Enabled"}</span>
      </CollapsibleTrigger>
      <CollapsibleContent>
        <div className="flex flex-col gap-3 px-3 pb-4 ps-9 text-[13px]">
          <p className="select-text break-all font-mono text-xs text-muted-foreground">{str(skill.path)}</p>
          {changed ? <p className="flex items-center gap-2 text-warning-foreground"><WarningCircle /> Instructions changed. Review them before enabling the updated version.</p> : null}
          <div className="flex flex-wrap gap-2">
            {!record || changed ? (
              <Button size="sm" disabled={busy} onClick={() => void run(() => skillsCmd("SetEnabled", { workspace_id: workspace, path: skill.path, content_hash: skill.content_hash, enabled: true }))}>
                {changed ? "Enable reviewed changes" : "Enable current instructions"}
              </Button>
            ) : null}
            {record ? (
              <Button variant="outline" size="sm" disabled={busy} onClick={() => void run(() => skillsCmd("SetEnabled", { workspace_id: record.workspace_id, path: record.path, content_hash: record.content_hash, enabled: false }))}>
                Disable in this workspace
              </Button>
            ) : null}
          </div>
          {curated ? (
            <div data-slot="skill-curator-state" data-state={str(curated.state)} className="flex flex-wrap items-center gap-3">
              <p className="min-w-0 flex-1 text-xs text-muted-foreground">{CURATOR_WORDS[str(curated.state)] ?? CURATOR_WORDS.other}</p>
              <Button variant="outline" size="sm" disabled={busy} onClick={() => void run(() => skillsCmd("SetLocked", { path: skill.path, locked: str(curated.state) === "neko" || str(curated.state) === "other" }))}>
                {str(curated.state) === "locked" || str(curated.state) === "edited" ? "Unlock for the curator" : "Lock against the curator"}
              </Button>
            </div>
          ) : null}
          <p className="text-xs text-muted-foreground">Open the file in your editor to read it before enabling: this page cannot open local files.</p>
        </div>
      </CollapsibleContent>
    </Collapsible>
  );
}

function Proposal({ proposal, run, busy }: { proposal: Json; run: (s: () => Promise<Json>) => Promise<boolean>; busy: boolean }) {
  const key = `${proposal.id}:${proposal.content_hash}`;
  const auditUrl = str(proposal.audit_url);
  const [opened, setOpened] = useState(false);
  const reviewed = !auditUrl || proposal.audit_reviewed_hash === proposal.content_hash;
  const decide = (accept: boolean) => void run(() => skillsCmd("DecideProposal", { id: proposal.id, content_hash: proposal.content_hash, accept }));
  return (
    <Collapsible data-slot="skill-proposal" data-key={key}>
      <CollapsibleTrigger render={<button type="button" className="flex min-h-11 w-full items-center gap-3 rounded-[10px] px-3 py-1.5 text-start outline-none transition-colors hover:bg-foreground/[0.03] focus-visible:ring-2 focus-visible:ring-ring" />}>
        <span className="flex min-w-0 flex-1 items-baseline gap-3">
          <span className="shrink-0 text-[13.5px]">{str(proposal.name)}</span>
          <span className="truncate text-[12.5px] text-muted-foreground">Suggested skill · review instructions</span>
        </span>
        <CaretDown aria-hidden className="text-muted-foreground" />
      </CollapsibleTrigger>
      <CollapsibleContent>
        <div className="flex flex-col gap-3 px-3 pb-4 text-[13px]">
          <p className="select-text break-all text-muted-foreground">{str(proposal.source)}</p>
          <p className="text-muted-foreground">{str(proposal.audit_status)}</p>
          {auditUrl ? (
            <>
              <div>
                <Button variant="outline" size="sm" render={<a href={auditUrl} target="_blank" rel="noreferrer noopener" onClick={() => setOpened(true)} />}>
                  View published source and audit links <ArrowSquareOut />
                </Button>
              </div>
              <p className="text-xs text-muted-foreground">Read the published audit results. If none are available, do not confirm or install. Confirmation records your review, not a passing audit.</p>
              <div>
                <Button variant="outline" size="sm" disabled={!opened || busy || reviewed} onClick={() => void run(() => skillsCmd("ConfirmAuditReview", { id: proposal.id, content_hash: proposal.content_hash, audit_url: auditUrl }))}>
                  I reviewed the linked published audit results
                </Button>
              </div>
            </>
          ) : null}
          <pre className="max-h-60 overflow-auto whitespace-pre-wrap rounded-[10px] bg-foreground/[0.03] p-3 font-mono text-xs" aria-label="Proposed skill instructions">{str(proposal.body)}</pre>
          <p className="text-xs text-muted-foreground">Accept saves these exact instructions. Enable separately after installation.</p>
          <div className="flex gap-2">
            <Button size="sm" disabled={!reviewed || busy} onClick={() => decide(true)}>Accept &amp; save reviewed content</Button>
            <Button variant="outline" size="sm" disabled={busy} onClick={() => decide(false)}>Reject</Button>
          </div>
        </div>
      </CollapsibleContent>
    </Collapsible>
  );
}

export function SkillsTab({ snapshot, workspace }: { snapshot: Json; workspace: string }) {
  const { run, busy, error, clear } = useRun();
  const [search, setSearch] = useState("");
  const [url, setUrl] = useState("");
  const [showFind, setShowFind] = useState(false);
  const available = arr(snapshot.skills?.available);
  const enabled = arr(snapshot.skills?.enabled);
  const visible = useMemo(
    () => available.filter((s) => (!str(s.workspace_id) || s.workspace_id === workspace) && (!search || `${str(s.name)} ${str(s.description)}`.toLowerCase().includes(search.toLowerCase()))),
    [available, workspace, search],
  );
  const unavailable = enabled.filter((r) => r.workspace_id === workspace && !available.some((a) => a.path === r.path && (!str(a.workspace_id) || a.workspace_id === workspace)));
  const proposals = arr(snapshot.skills?.proposals).filter((p) => p.workspace_id === workspace);
  const curator = (snapshot.skills?.curator ?? {}) as Json;
  const curated = arr(curator.skills);
  const recent = arr(curator.recent);

  return (
    <div className="flex flex-col gap-6">
      <div className="flex items-center gap-2">
        <div className="relative flex-1">
          <MagnifyingGlass aria-hidden className="pointer-events-none absolute start-3 top-1/2 size-4 -translate-y-1/2 text-muted-foreground" />
          <Input value={search} onChange={(e) => setSearch(e.target.value)} aria-label="Search skills" placeholder="Search skills" className="ps-9!" />
        </div>
        <Button variant="ghost" size="sm" disabled={busy} loading={busy} title="Read skills again from this workspace’s folders. A skill cannot access tools on its own." onClick={() => void run(() => skillsCmd("Refresh"))}><ArrowsClockwise /> Refresh</Button>
      </div>
      <ErrorNote error={error} title="Couldn’t update skills" onDismiss={clear} />
      {unavailable.map((r) => (
        <Panel key={r.path}>
          <Row title="Unavailable enabled skill" description="Runs stop until these instructions are restored or disabled.">
            <p className="select-text break-all font-mono text-xs">{str(r.path)}</p>
            <Button className="mt-3" variant="outline" size="sm" disabled={busy} onClick={() => void run(() => skillsCmd("SetEnabled", { workspace_id: r.workspace_id, path: r.path, content_hash: r.content_hash, enabled: false }))}>Disable unavailable skill</Button>
          </Row>
        </Panel>
      ))}
      {visible.length > 0 ? (
        <Panel>
          {visible.map((s) => <SkillRow key={str(s.path)} skill={s} workspace={workspace} enabled={enabled} run={run} busy={busy} curated={curated.find((c) => c.path === s.path)} />)}
        </Panel>
      ) : (
        <EmptyBlock icon={Books} title={search ? `No skills match “${search}”` : "No local skills available yet"} description={search ? undefined : "Refresh local skills, or fetch an instruction file below."} />
      )}
      <Panel>
        <Collapsible open={showFind} onOpenChange={setShowFind}>
          <CollapsibleTrigger render={<button type="button" className="flex min-h-10 w-full items-center gap-2 rounded-[10px] px-3 text-start text-[13.5px] text-muted-foreground outline-none transition-colors hover:bg-foreground/[0.03] hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring" />}>
            <Plus aria-hidden /> Find a skill
          </CollapsibleTrigger>
          <CollapsibleContent>
            <div className="flex flex-col gap-3 px-3 py-3">
              <a className="inline-flex w-fit items-center gap-1.5 text-[13px] text-muted-foreground underline-offset-4 hover:text-foreground hover:underline" href="https://skills.sh" target="_blank" rel="noreferrer noopener">Browse skills.sh <ArrowSquareOut /></a>
              <FormField label="GitHub SKILL.md URL" htmlFor="skill-url" hint="Only the instruction file is previewed. Scripts and references are not installed.">
                <Input id="skill-url" value={url} onChange={(e) => setUrl(e.target.value)} placeholder="https://github.com/…/SKILL.md" className="font-mono" />
              </FormField>
            </div>
            <PanelFooter>
              <Button size="sm" disabled={!url.trim() || busy} onClick={() => void run(() => skillsCmd("PreviewRepository", { workspace_id: workspace, url: url.trim() }))}>Fetch for review</Button>
            </PanelFooter>
          </CollapsibleContent>
        </Collapsible>
      </Panel>
      <Panel data-slot="skill-curator">
        <Row title="Skill curator" description="Once a day, when the Mac is idle, tidies the lessons a skill has gathered: merges duplicates and drops what a newer lesson contradicts. Changes show as a diff. It applies by itself only to skills Neko wrote and you have not edited; everything else is a proposal. Nothing is deleted: removed text moves to an Archive section.">
          <div className="mt-3 flex flex-wrap items-center gap-3">
            <Button variant="outline" size="sm" disabled={busy} onClick={() => void run(() => skillsCmd("SetCurator", { on: !!curator.off }))}>{curator.off ? "Turn the curator on" : "Turn the curator off"}</Button>
            <span className="text-xs text-muted-foreground">{curator.off ? "Off." : curator.last_run_ms ? `Last pass ${new Date(Number(curator.last_run_ms)).toLocaleString()}.` : "Has not run yet."}</span>
          </div>
        </Row>
        {recent.slice(0, 3).map((r) => (
          <Collapsible key={`${str(r.path)}:${r.at_ms}`} data-slot="skill-curator-applied">
            <CollapsibleTrigger render={<button type="button" className="flex min-h-10 w-full items-center gap-3 rounded-[10px] px-3 text-start text-[13px] outline-none transition-colors hover:bg-foreground/[0.03] focus-visible:ring-2 focus-visible:ring-ring" />}>
              <span className="min-w-0 flex-1 truncate">Tidied {str(r.name)}</span>
              <span className="shrink-0 text-xs text-muted-foreground">{new Date(Number(r.at_ms)).toLocaleDateString()}</span>
            </CollapsibleTrigger>
            <CollapsibleContent>
              <pre className="mx-3 mb-3 max-h-60 overflow-auto whitespace-pre-wrap rounded-[10px] bg-foreground/[0.03] p-3 font-mono text-xs" aria-label="Applied changes">{str(r.diff)}</pre>
            </CollapsibleContent>
          </Collapsible>
        ))}
      </Panel>
      {proposals.length > 0 ? (
        <Panel>
          {proposals.map((p) => <Proposal key={str(p.id)} proposal={p} run={run} busy={busy} />)}
        </Panel>
      ) : null}
    </div>
  );
}
