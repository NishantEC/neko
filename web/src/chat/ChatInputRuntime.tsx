// The composer's runtime picker: model, reasoning effort and speed for ONE
// conversation, the web twin of ComposerRuntime{Controls,Panel,ModelList,
// EffortControl,SpeedControl,Settings}View.swift and RuntimeSelection*.swift.
//
//   catalog  <- { AgentModels: { refresh } }              (listing never spends tokens)
//   save     -> Workbench.SetConversationRuntime { conversation_id, preferences }
//   saved    <- snapshot.conversation_runtime[id] and .conversation_runtime_revisions[id]
//
// Pins are independent per axis (null = Neko decides); a model and its provider
// are one pin. Changing the model drops only the pins it cannot honour and says
// so. The daemon's error text is shown verbatim: it is written for a person.
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { accept, command, request, type Json } from "@/api";
import { useWorkbench } from "@/useStore";
import { ArrowClockwise, Check, CaretDown, CircleNotch, Lightning, MagnifyingGlass } from "@/ui/icons";
import { Button } from "@/ui/button";
import { Badge } from "@/ui/badge";
import { Popover, PopoverContent, PopoverTrigger } from "@/ui/popover";
import { Switch } from "@/ui/switch";
import { cn } from "@/ui/lib/utils";
import { MonoLabel } from "@/editorial";

// ---------------------------------------------------------------------------
// Types and parsing (AgentModelCatalog.swift, RuntimeSelectionPreferences.swift)
// ---------------------------------------------------------------------------

export interface RuntimePrefs {
  provider: string | null;
  model: string | null;
  reasoning_effort: string | null;
  service_tier: string | null;
  allow_paid_speed: boolean;
}

export const AUTO_RUNTIME: RuntimePrefs = { provider: null, model: null, reasoning_effort: null, service_tier: null, allow_paid_speed: false };

export interface RuntimeOption { id: string; label: string; description?: string }
export interface CatalogModel {
  id: string;
  label: string;
  description?: string;
  recommended: boolean;
  access: "listed" | "checked" | "unavailable";
  reason?: string;
  /** undefined is unknown; [] explicitly means this axis is unsupported. */
  effortOptions?: RuntimeOption[];
  defaultEffort?: string;
  /** Additional tiers only; Normal is `default` and is always offered. */
  speedOptions?: RuntimeOption[];
  defaultSpeed?: string;
  usable: boolean;
}
export interface ModelSource {
  provider: string;
  label: string;
  connection: string;
  status: string;
  note?: string;
  defaultModel?: string;
  models: CatalogModel[];
  ready: boolean;
}
export interface Catalog { sources: ModelSource[] }

const text = (value: Json): string | undefined => (typeof value === "string" && value ? value : undefined);

function parseOptions(value: Json): RuntimeOption[] | undefined {
  if (!Array.isArray(value)) return undefined;
  const seen = new Set<string>();
  const out: RuntimeOption[] = [];
  for (const row of value) {
    const id = text(row?.id);
    if (!id || seen.has(id)) continue;
    seen.add(id);
    out.push({ id, label: text(row.label) ?? id, description: text(row.description) });
  }
  return out;
}

export function parseCatalog(value: Json): Catalog {
  const sources: ModelSource[] = [];
  for (const row of value?.sources ?? []) {
    const provider = text(row?.provider);
    if (!provider) continue;
    const models: CatalogModel[] = [];
    for (const model of row.models ?? []) {
      const id = text(model?.id);
      if (!id) continue;
      const access = ["listed", "checked", "unavailable"].includes(model.access) ? model.access : "listed";
      models.push({
        id,
        label: text(model.label) ?? id,
        description: text(model.description),
        recommended: !!model.recommended,
        access,
        reason: text(model.reason),
        effortOptions: parseOptions(model.effort_options),
        defaultEffort: text(model.default_effort),
        speedOptions: parseOptions(model.speed_options)?.filter((o) => o.id !== "default"),
        defaultSpeed: text(model.default_speed),
        usable: access !== "unavailable",
      });
    }
    sources.push({
      provider,
      label: text(row.label) ?? provider,
      connection: text(row.connection) ?? "",
      status: text(row.status) ?? "error",
      note: text(row.note),
      defaultModel: text(row.default_model),
      models,
      ready: row.status === "ready",
    });
  }
  return { sources };
}

export function parsePrefs(value: Json): RuntimePrefs {
  const pick = (key: string) => text(value?.[key]) ?? null;
  const provider = pick("provider");
  const model = pick("model");
  return {
    provider: provider && model ? provider : null,
    model: provider && model ? model : null,
    reasoning_effort: pick("reasoning_effort"),
    service_tier: pick("service_tier"),
    allow_paid_speed: !!value?.allow_paid_speed,
  };
}

export const prefsEqual = (a: RuntimePrefs, b: RuntimePrefs) =>
  a.provider === b.provider && a.model === b.model && a.reasoning_effort === b.reasoning_effort &&
  a.service_tier === b.service_tier && a.allow_paid_speed === b.allow_paid_speed;

const isAutomatic = (p: RuntimePrefs) => !p.model && !p.reasoning_effort && !p.service_tier;
const requestsExtraSpeed = (p: RuntimePrefs) => !!p.service_tier && p.service_tier !== "default";

function modelOf(catalog: Catalog, provider: string | null, id: string | null) {
  if (!provider || !id) return undefined;
  return catalog.sources.find((s) => s.provider === (provider || "codex"))?.models.find((m) => m.id === id);
}

/** Effort or speed levels this selection can reach: its model's, or every usable model's. */
export function optionsFor(prefs: RuntimePrefs, catalog: Catalog, speed: boolean): RuntimeOption[] {
  const models = prefs.provider && prefs.model
    ? [modelOf(catalog, prefs.provider, prefs.model)].filter((m): m is CatalogModel => !!m)
    : catalog.sources.filter((s) => s.ready).flatMap((s) => s.models).filter((m) => m.usable);
  const seen = new Set<string>();
  return models.flatMap((m) => (speed ? m.speedOptions : m.effortOptions) ?? []).filter((o) => (seen.has(o.id) ? false : (seen.add(o.id), true)));
}

// Labels (RuntimeSelectionLabel.swift)
export function modelLabel(p: RuntimePrefs, catalog: Catalog): string {
  if (!p.provider || !p.model) return "Neko decides";
  const label = modelOf(catalog, p.provider, p.model)?.label ?? p.model;
  // Presentation-only shortening; capabilities and wire IDs always come from metadata.
  return label.replace(/^GPT[- ]?[0-9]+(?:\.[0-9]+)?[- ]/i, "");
}
export function effortLabel(p: RuntimePrefs, catalog: Catalog): string {
  if (!p.reasoning_effort) return "Auto";
  return optionsFor(p, catalog, false).find((o) => o.id === p.reasoning_effort)?.label ?? p.reasoning_effort.replace(/^./, (c) => c.toUpperCase());
}
export function speedLabel(p: RuntimePrefs, catalog: Catalog): string {
  if (!p.service_tier) return "Auto";
  if (p.service_tier === "default") return "Normal";
  return optionsFor(p, catalog, true).find((o) => o.id === p.service_tier)?.label ?? p.service_tier;
}
export function compactLabel(p: RuntimePrefs, catalog: Catalog): string {
  const parts = [modelLabel(p, catalog)];
  if (p.reasoning_effort) parts.push(effortLabel(p, catalog));
  if (p.service_tier) parts.push(speedLabel(p, catalog));
  return parts.join(" · ");
}

/** A model change drops only incompatible pins, with an explanation for each change. */
export function selectModel(prefs: RuntimePrefs, provider: string, model: CatalogModel): { next: RuntimePrefs; notes: string[] } {
  const next = { ...prefs, provider, model: model.id };
  const notes: string[] = [];
  if (next.reasoning_effort && !(model.effortOptions ?? []).some((o) => o.id === next.reasoning_effort)) {
    notes.push(`Effort reset to Auto: ${model.label} does not advertise ${next.reasoning_effort}.`);
    next.reasoning_effort = null;
  }
  if (next.service_tier && next.service_tier !== "default" && !(model.speedOptions ?? []).some((o) => o.id === next.service_tier)) {
    notes.push(`Speed reset to Auto: ${model.label} does not advertise ${next.service_tier}.`);
    next.service_tier = null;
  }
  return { next, notes };
}

function validationMessage(draft: RuntimePrefs, catalog: Catalog, loaded: boolean): string | null {
  if (!loaded || isAutomatic(draft)) return null;
  const candidates = catalog.sources
    .filter((s) => s.ready && (!draft.provider || s.provider === draft.provider))
    .flatMap((s) => s.models)
    .filter((m) => m.usable && (!draft.model || m.id === draft.model));
  const compatible = candidates.some(
    (m) =>
      (!draft.reasoning_effort || (m.effortOptions ?? []).some((o) => o.id === draft.reasoning_effort)) &&
      (!draft.service_tier || draft.service_tier === "default" || (m.speedOptions ?? []).some((o) => o.id === draft.service_tier)),
  );
  return compatible ? null : "No connected model advertises these pins. Choose a model, reset an axis to Auto, or refresh models.";
}

// ---------------------------------------------------------------------------
// State for one conversation (RuntimeSelectionState.swift)
// ---------------------------------------------------------------------------

export interface RuntimeSelection {
  conversationId: string;
  catalog: Catalog;
  loaded: boolean;
  loading: boolean;
  catalogError: string | null;
  saved: RuntimePrefs;
  draft: RuntimePrefs;
  setDraft(next: RuntimePrefs): void;
  dirty: boolean;
  saving: boolean;
  applyError: string | null;
  status: string | null;
  reconciliation: string[];
  validation: string | null;
  canApply: boolean;
  load(refresh?: boolean): Promise<void>;
  choose(provider: string, model: CatalogModel): void;
  automaticModel(): void;
  discard(): void;
  reset(): void;
  apply(): Promise<void>;
}

export function useRuntimeSelection(conversationId: string): RuntimeSelection {
  const { snapshot } = useWorkbench();
  const saved = useMemo(() => parsePrefs(snapshot?.conversation_runtime?.[conversationId]), [snapshot?.conversation_runtime, conversationId]);
  const revision: number = snapshot?.conversation_runtime_revisions?.[conversationId] ?? 0;
  const [draft, setDraftState] = useState<RuntimePrefs>(saved);
  const [catalog, setCatalog] = useState<Catalog>({ sources: [] });
  const [loaded, setLoaded] = useState(false);
  const [loading, setLoading] = useState(false);
  const [catalogError, setCatalogError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [applyError, setApplyError] = useState<string | null>(null);
  const [status, setStatus] = useState<string | null>(null);
  const [reconciliation, setReconciliation] = useState<string[]>([]);
  const lastRevision = useRef(revision);
  const draftRef = useRef(draft);
  draftRef.current = draft;
  const dirty = !prefsEqual(draft, saved);

  // A new confirmed value replaces an untouched draft; an edited draft is kept.
  const prevSaved = useRef(saved);
  useEffect(() => {
    const before = prevSaved.current;
    prevSaved.current = saved;
    if (revision <= lastRevision.current) return;
    lastRevision.current = revision;
    if (prefsEqual(draftRef.current, before)) setDraftState(saved);
    else setStatus("Saved settings changed elsewhere. Your unapplied draft is kept here.");
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [revision, saved]);

  const inFlight = useRef(false);
  const load = useCallback(async (refresh = false) => {
    if (inFlight.current) return;
    inFlight.current = true;
    setLoading(true);
    setCatalogError(null);
    try {
      const reply = await request({ AgentModels: { refresh } });
      if (!Array.isArray(reply?.AgentModels?.sources)) throw new Error("The daemon returned an unreadable model catalog. Refresh models to try again.");
      setCatalog(parseCatalog(reply.AgentModels));
      setLoaded(true);
    } catch (e) {
      setCatalogError(e instanceof Error ? e.message : String(e));
    } finally {
      inFlight.current = false;
      setLoading(false);
    }
  }, []);

  const setDraft = useCallback((next: RuntimePrefs) => {
    setDraftState(next);
    setApplyError(null);
    setStatus(null);
  }, []);

  const validation = validationMessage(draft, catalog, loaded);
  const canApply = dirty && !saving && !loading && validation === null && (loaded || isAutomatic(draft));

  return {
    conversationId, catalog, loaded, loading, catalogError, saved, draft, setDraft, dirty, saving, applyError, status, reconciliation, validation, canApply, load,
    choose(provider, model) {
      const { next, notes } = selectModel(draft, provider, model);
      setReconciliation(notes);
      setDraft(next);
    },
    automaticModel() { setReconciliation([]); setDraft({ ...draft, provider: null, model: null }); },
    discard() { setReconciliation([]); setDraft(saved); },
    reset() { setReconciliation([]); setDraft(AUTO_RUNTIME); },
    async apply() {
      if (!canApply) return;
      const submitted = draft;
      setSaving(true);
      setApplyError(null);
      setStatus(null);
      try {
        const reply = await command("SetConversationRuntime", { conversation_id: conversationId, preferences: submitted });
        // Resetting to automatic removes the entry, so absence confirms it; the revision always advances.
        const workbench = reply?.Workbench;
        const newRevision = workbench?.conversation_runtime_revisions?.[conversationId];
        if (typeof workbench?.conversation_runtime !== "object" || typeof newRevision !== "number" || !prefsEqual(parsePrefs(workbench.conversation_runtime[conversationId]), submitted)) {
          throw new Error("The daemon did not confirm these conversation settings. Refresh before trying again.");
        }
        if (newRevision <= lastRevision.current) throw new Error("The daemon returned an outdated settings revision. Refresh before trying again.");
        lastRevision.current = newRevision;
        accept(reply);
        setDraftState(submitted);
        setStatus("Settings applied.");
        setReconciliation([]);
      } catch (e) {
        setApplyError(e instanceof Error ? e.message : String(e));
      } finally {
        setSaving(false);
      }
    },
  };
}

// ---------------------------------------------------------------------------
// The picker
// ---------------------------------------------------------------------------

function Segmented({
  label, value, options, onChange, disabled,
}: {
  label: string;
  value: string;
  options: { id: string; label: string; title?: string }[];
  onChange(id: string): void;
  disabled?: boolean;
}) {
  return (
    <div role="radiogroup" aria-label={label} className="flex flex-wrap gap-1 rounded-lg bg-foreground/[0.05] p-0.5">
      {options.map((option) => {
        const on = option.id === value;
        return (
          <button
            key={option.id}
            type="button"
            role="radio"
            aria-checked={on}
            title={option.title}
            disabled={disabled}
            onClick={() => onChange(option.id)}
            className={cn(
              "min-w-0 rounded-md px-2.5 py-1 text-[12.5px] font-medium outline-none transition-colors focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-50",
              on ? "bg-card text-strong shadow-xs ring-1 ring-border" : "text-muted-foreground hover:text-foreground",
            )}
          >
            {option.label}
          </button>
        );
      })}
    </div>
  );
}

function ModelRow({ title, detail, selected, disabled, onClick, badge }: { title: string; detail?: string; selected: boolean; disabled?: boolean; onClick(): void; badge?: string }) {
  return (
    <button
      type="button"
      data-slot="model-row"
      aria-pressed={selected}
      disabled={disabled}
      onClick={onClick}
      className="flex w-full items-start gap-2 rounded-lg px-2.5 py-1.5 text-start outline-none transition-colors hover:bg-accent focus-visible:bg-accent focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-50 disabled:hover:bg-transparent"
    >
      <span className="min-w-0 flex-1">
        <span className="flex items-center gap-2 text-[13.5px] font-medium text-strong">
          <span className="truncate">{title}</span>
          {badge ? <Badge variant="outline" className="h-4 rounded-full px-1.5 text-[10.5px] font-normal text-muted-foreground">{badge}</Badge> : null}
        </span>
        {detail ? <span className="mt-0.5 block text-[12px] leading-snug text-muted-foreground">{detail}</span> : null}
      </span>
      {selected ? <Check weight="bold" className="mt-1 size-3.5 shrink-0 text-foreground" aria-label="Selected" /> : null}
    </button>
  );
}

function ModelList({ selection }: { selection: RuntimeSelection }) {
  const [query, setQuery] = useState("");
  const q = query.trim().toLowerCase();
  const matches = (m: CatalogModel, s: ModelSource) => !q || [m.id, m.label, m.description ?? "", s.label].some((t) => t.toLowerCase().includes(q));
  const any = selection.catalog.sources.some((s) => s.models.some((m) => matches(m, s)));
  return (
    <div className="flex flex-col gap-1">
      <div className="relative">
        <MagnifyingGlass className="pointer-events-none absolute start-2.5 top-1/2 size-3.5 -translate-y-1/2 text-muted-foreground" />
        <input
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          aria-label="Search available models"
          placeholder="Search models"
          className="h-8 w-full rounded-lg border border-border bg-foreground/[0.03] ps-8 pe-2 text-[13px] text-strong outline-none placeholder:text-muted-foreground focus-visible:ring-2 focus-visible:ring-ring"
        />
      </div>
      <div data-slot="model-list" className="mt-1 max-h-[216px] overflow-y-auto pe-0.5">
        <ModelRow
          title="Neko decides"
          detail="Chooses a compatible model and keeps your effort and speed pins."
          selected={!selection.draft.model}
          onClick={selection.automaticModel}
        />
        {selection.catalog.sources.map((source) => {
          const models = source.models.filter((m) => matches(m, source));
          if (!models.length && (source.ready || q)) return null;
          return (
            <div key={source.provider} className="mt-1.5">
              <MonoLabel className="px-2.5 text-[10px]">{source.label}{source.connection ? ` · ${source.connection}` : ""}</MonoLabel>
              {!source.ready ? <p className="px-2.5 py-1 text-[12px] text-muted-foreground">{source.note ?? "This source is not ready."}</p> : null}
              {models.map((model) => (
                <ModelRow
                  key={`${source.provider}:${model.id}`}
                  title={model.label}
                  detail={[model.reason ?? model.description, model.usable ? null : "Unavailable"].filter(Boolean).join(" · ") || undefined}
                  badge={model.recommended ? "Recommended" : undefined}
                  selected={selection.draft.provider === source.provider && selection.draft.model === model.id}
                  disabled={!source.ready || !model.usable}
                  onClick={() => selection.choose(source.provider, model)}
                />
              ))}
            </div>
          );
        })}
        {!any && !selection.loading ? (
          <p className="px-2.5 py-3 text-[12.5px] text-muted-foreground">
            {q ? "No models match this search." : "No models available. Refresh models or connect a provider in Settings → AI."}
          </p>
        ) : null}
      </div>
    </div>
  );
}

function RuntimePanel({ selection }: { selection: RuntimeSelection }) {
  const { draft, catalog } = selection;
  const effort = optionsFor(draft, catalog, false);
  const speed = optionsFor(draft, catalog, true);
  const model = modelOf(catalog, draft.provider, draft.model);
  const effortNote = effort.find((o) => o.id === draft.reasoning_effort)?.description
    ?? (model?.defaultEffort ? `Model default: ${effort.find((o) => o.id === model.defaultEffort)?.label ?? model.defaultEffort}. Auto lets Neko choose effort for the task.` : "Auto lets Neko choose effort for the task.");
  return (
    <div className="flex flex-col gap-3" data-slot="runtime-panel">
      <div className="flex items-center gap-2">
        <h3 className="flex-1 text-[15px] font-semibold text-strong">Model &amp; effort</h3>
        {selection.loading ? <CircleNotch className="size-4 animate-spin text-muted-foreground" aria-label="Loading models" /> : null}
        <Button type="button" variant="ghost" size="icon-xs" aria-label="Refresh models" disabled={selection.loading || selection.saving} onClick={() => void selection.load(true)}>
          <ArrowClockwise />
        </Button>
      </div>
      {selection.catalogError ? <p role="alert" className="select-text text-[12.5px] text-warning-foreground">Models could not be loaded: {selection.catalogError}</p> : null}
      <fieldset disabled={selection.saving} className="flex min-w-0 flex-col gap-3 border-0 p-0">
        <ModelList selection={selection} />
        <div className="flex flex-col gap-1.5">
          <div className="flex items-baseline justify-between"><MonoLabel className="text-[10px]">Effort</MonoLabel><span className="text-[12px] text-muted-foreground">{effortLabel(draft, catalog)}</span></div>
          {effort.length ? (
            <>
              <Segmented
                label="Reasoning effort"
                value={draft.reasoning_effort ?? "auto"}
                options={[{ id: "auto", label: "Auto" }, ...effort.map((o) => ({ id: o.id, label: o.label, title: o.description }))]}
                onChange={(id) => selection.setDraft({ ...draft, reasoning_effort: id === "auto" ? null : id })}
              />
              <p className="text-[12px] text-muted-foreground">{effortNote}</p>
            </>
          ) : draft.reasoning_effort ? (
            <div className="flex items-center gap-2 text-[12.5px] text-warning-foreground">Effort pin unavailable
              <Button type="button" variant="ghost" size="xs" onClick={() => selection.setDraft({ ...draft, reasoning_effort: null })}>Reset effort to Auto</Button>
            </div>
          ) : (
            <p className="text-[12px] text-muted-foreground">{selection.loaded ? "This model does not advertise effort levels." : "Load models to see effort levels."}</p>
          )}
        </div>
        <div className="flex flex-col gap-1.5">
          <div className="flex items-baseline justify-between"><MonoLabel className="text-[10px]">Speed</MonoLabel><span className="text-[12px] text-muted-foreground">{speedLabel(draft, catalog)}</span></div>
          {speed.length ? (
            <>
              <Segmented
                label="Speed"
                value={draft.service_tier ?? "auto"}
                options={[{ id: "auto", label: "Auto" }, { id: "default", label: "Normal" }, ...speed.map((o) => ({ id: o.id, label: o.label, title: o.description }))]}
                onChange={(id) => selection.setDraft({ ...draft, service_tier: id === "auto" ? null : id })}
              />
              <p className="text-[12px] text-muted-foreground">Extra speed may use more allowance. Auto respects the paid speed preference.</p>
            </>
          ) : draft.service_tier ? (
            <div className="flex items-center gap-2 text-[12.5px]">{draft.service_tier === "default" ? "Speed: Normal" : "Speed pin unavailable"}
              <Button type="button" variant="ghost" size="xs" onClick={() => selection.setDraft({ ...draft, service_tier: null })}>Reset speed to Auto</Button>
            </div>
          ) : (
            <p className="text-[12px] text-muted-foreground">{selection.loaded ? "No extra speed is offered for this model." : "Load models to see speed options."}</p>
          )}
        </div>
        <label className="flex items-center gap-3 text-[13px] text-foreground">
          <Switch size="sm" checked={draft.allow_paid_speed} onCheckedChange={(on) => selection.setDraft({ ...draft, allow_paid_speed: !!on })} aria-label="Allow automatic paid speed" />
          <span className="flex-1">Allow automatic paid speed<span className="block text-[12px] text-muted-foreground">Off by default. When speed is Auto, Neko may pick a tier that uses more allowance.</span></span>
        </label>
      </fieldset>
      {selection.reconciliation.map((note) => <p key={note} className="text-[12.5px] text-warning-foreground">{note}</p>)}
      {selection.validation ? <p role="alert" className="text-[12.5px] text-warning-foreground">{selection.validation}</p> : null}
      {selection.applyError ? (
        <p role="alert" data-slot="runtime-error" className="select-text text-[12.5px] text-destructive">Settings were not applied. {selection.applyError}</p>
      ) : selection.status && !selection.dirty ? (
        <p role="status" className="text-[12.5px] text-muted-foreground">{selection.status}</p>
      ) : null}
      {selection.dirty && !selection.saving && !selection.applyError ? <p className="text-[12px] text-muted-foreground">Not applied yet.</p> : null}
      <div className="flex items-center gap-1.5 border-t border-border/70 pt-3">
        <Button type="button" variant="ghost" size="sm" onClick={selection.reset} disabled={selection.saving} aria-label="Reset to Neko decides" title="Reset to Neko decides: clears every pin and automatic paid speed">Reset</Button>
        {selection.dirty && !selection.saving ? <Button type="button" variant="ghost" size="sm" onClick={selection.discard}>Discard</Button> : null}
        <span className="flex-1" />
        {selection.saving ? <span className="flex items-center gap-1.5 text-[12px] text-muted-foreground"><CircleNotch className="animate-spin" /> Checking…</span> : null}
        <Button type="button" size="sm" disabled={!selection.canApply} onClick={() => void selection.apply()}>Check &amp; apply</Button>
      </div>
      <p className="text-[11.5px] leading-snug text-muted-foreground">One short check may use allowance. Applies to the next turn or worker.</p>
    </div>
  );
}

export function RuntimePicker({ conversationId, openRequest }: { conversationId: string; openRequest?: number }) {
  const selection = useRuntimeSelection(conversationId);
  const [open, setOpen] = useState(false);
  const { load, loaded } = selection;
  useEffect(() => {
    if (open && !loaded) void load();
  }, [open, loaded, load]);
  // `/models` asks the picker to open.
  useEffect(() => {
    if (openRequest) setOpen(true);
  }, [openRequest]);
  const label = compactLabel(selection.saved, selection.catalog);
  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger
        data-slot="runtime-trigger"
        aria-label={`Conversation settings: ${label}`}
        title="Model, effort and speed for this conversation. Changes apply to the next turn or worker."
        className="flex h-8 max-w-[260px] items-center gap-2 rounded-full border border-border bg-card px-3 text-[12.5px] font-medium text-muted-foreground outline-none transition-colors hover:bg-accent hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring data-popup-open:bg-accent"
      >
        <span aria-hidden className="size-2.5 shrink-0 rounded-full bg-gradient-to-br from-primary/70 to-primary/20 ring-1 ring-primary/30" />
        {selection.saving ? <CircleNotch className="size-3.5 animate-spin" aria-label="Checking settings" /> : null}
        {requestsExtraSpeed(selection.saved) ? <Lightning weight="fill" className="size-3.5" aria-hidden /> : null}
        <span className="truncate">{label}</span>
        <CaretDown className="size-3 shrink-0 opacity-70" />
      </PopoverTrigger>
      <PopoverContent side="top" align="start" sideOffset={10} className="w-[392px] max-w-[calc(100vw-2rem)] gap-0 bg-popover p-4">
        <RuntimePanel selection={selection} />
      </PopoverContent>
    </Popover>
  );
}
