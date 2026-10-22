// Skills: instruction files found in a workspace's folders. A skill gives an
// agent instructions; it never grants a tool. Enabling pins the exact text you
// reviewed, so a changed file has to be enabled again.
import { useMemo, useState } from "react";
import type { Json } from "@/api";
import { Button } from "@/ui/button";
import { Input } from "@/ui/input";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/ui/collapsible";
import { ArrowSquareOut, ArrowsClockwise, Books, CaretDown, CaretRight, MagnifyingGlass, Plus, WarningCircle } from "@/ui/icons";
import { EmptyBlock, ErrorNote, FormField, PanelFooter, Row, Status } from "@/setup/kit";
import { Panel } from "@/editorial";
import { arr, skillsCmd, str, useRun } from "@/setup/lib";

function SkillRow({ skill, workspace, enabled, run, busy }: { skill: Json; workspace: string; enabled: Json[]; run: (s: () => Promise<Json>) => Promise<boolean>; busy: boolean }) {
  const [open, setOpen] = useState(false);
  const record = enabled.find((e) => e.workspace_id === workspace && e.path === skill.path);
  const changed = !!record && record.content_hash !== skill.content_hash;
  return (
    <Collapsible open={open} onOpenChange={setOpen} data-slot="skill">
      <CollapsibleTrigger
        render={<button type="button" aria-label={`${str(skill.name)}, ${!record ? "available" : changed ? "changed" : "enabled"}`} className="flex w-full items-start gap-3 px-5 py-4 text-start outline-none transition-colors hover:bg-foreground/[0.04] focus-visible:ring-2 focus-visible:ring-ring" />}
      >
        <span aria-hidden className="mt-0.5 text-muted-foreground">{open ? <CaretDown /> : <CaretRight />}</span>
        <span className="flex min-w-0 flex-1 flex-col gap-0.5">
          <span className="text-[14px] font-medium">{str(skill.name)}</span>
          <span className="line-clamp-2 text-[13px] text-muted-foreground">{str(skill.description)}</span>
        </span>
        <Status tone={changed ? "warning" : record ? "success" : "neutral"}>{!record ? "Available" : changed ? "Changed" : "Enabled"}</Status>
      </CollapsibleTrigger>
      <CollapsibleContent>
        <div className="flex flex-col gap-3 px-5 pb-5 ps-[3.25rem] text-[13px]">
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
      <CollapsibleTrigger render={<button type="button" className="flex w-full items-start gap-3 px-5 py-4 text-start outline-none transition-colors hover:bg-foreground/[0.04] focus-visible:ring-2 focus-visible:ring-ring" />}>
        <span className="flex min-w-0 flex-1 flex-col gap-0.5">
          <span className="text-[14px] font-medium">{str(proposal.name)}</span>
          <span className="text-[13px] text-muted-foreground">Suggested skill · Review instructions</span>
        </span>
        <CaretDown aria-hidden className="text-muted-foreground" />
      </CollapsibleTrigger>
      <CollapsibleContent>
        <div className="flex flex-col gap-3 px-5 pb-5 text-[13px]">
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
          <pre className="max-h-60 overflow-auto whitespace-pre-wrap rounded-xl border border-border bg-foreground/[0.03] p-3 font-mono text-xs" aria-label="Proposed skill instructions">{str(proposal.body)}</pre>
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

  return (
    <div className="flex flex-col gap-6">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <p className="max-w-[60ch] text-[13px] text-muted-foreground">Instructions from this workspace’s folders. A skill cannot access tools on its own.</p>
        <Button variant="outline" size="sm" disabled={busy} loading={busy} onClick={() => void run(() => skillsCmd("Refresh"))}><ArrowsClockwise /> Refresh</Button>
      </div>
      <ErrorNote error={error} title="Couldn’t update skills" onDismiss={clear} />
      <div className="relative">
        <MagnifyingGlass aria-hidden className="pointer-events-none absolute start-3 top-1/2 size-4 -translate-y-1/2 text-muted-foreground" />
        <Input value={search} onChange={(e) => setSearch(e.target.value)} aria-label="Search skills" placeholder="Search skills" className="ps-9" />
      </div>
      {unavailable.map((r) => (
        <Panel key={r.path}>
          <Row title="Unavailable enabled skill" description="Runs stop until these instructions are restored or disabled.">
            <p className="select-text break-all font-mono text-xs">{str(r.path)}</p>
            <Button className="mt-3" variant="outline" size="sm" disabled={busy} onClick={() => void run(() => skillsCmd("SetEnabled", { workspace_id: r.workspace_id, path: r.path, content_hash: r.content_hash, enabled: false }))}>Disable unavailable skill</Button>
          </Row>
        </Panel>
      ))}
      {visible.length > 0 ? (
        <Panel><div className="divide-y divide-border/70">
          {visible.map((s) => <SkillRow key={str(s.path)} skill={s} workspace={workspace} enabled={enabled} run={run} busy={busy} />)}
        </div></Panel>
      ) : (
        <EmptyBlock icon={Books} title={search ? `No skills match “${search}”` : "No local skills available yet"} description={search ? undefined : "Refresh local skills, or fetch an instruction file below."} />
      )}
      <Panel>
        <Collapsible open={showFind} onOpenChange={setShowFind}>
          <CollapsibleTrigger render={<button type="button" className="flex w-full items-center gap-2 px-5 py-4 text-start text-[14px] font-medium outline-none hover:bg-foreground/[0.04] focus-visible:ring-2 focus-visible:ring-ring" />}>
            <Plus aria-hidden /> Find a skill
          </CollapsibleTrigger>
          <CollapsibleContent>
            <div className="flex flex-col gap-3 border-t border-border/70 px-5 py-4">
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
      {proposals.length > 0 ? (
        <Panel><div className="divide-y divide-border/70">
          {proposals.map((p) => <Proposal key={str(p.id)} proposal={p} run={run} busy={busy} />)}
        </div></Panel>
      ) : null}
    </div>
  );
}
