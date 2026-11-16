//! The session's model chip: what a conversation runs on, its reasoning effort and its speed. A port of the web's
//! `chat/ChatInputRuntime.tsx` (the logic; the popover is `view/runtime.rs`). The catalog comes from `AgentModels`
//! (listing never spends tokens) and a choice is saved with `SetConversationRuntime { conversation_id: "task:<id>",
//! preferences }`, the same request the web sends. Each axis is pinned on its own (None = Neko decides); a model and its
//! provider are one pin; changing the model drops only the pins it cannot honour and says so.

use neko_protocol::agent_models::{CatalogModel, ModelAccess, ModelCatalog, RuntimeOption, SourceStatus};
use neko_protocol::workbench::RuntimePreferences;
use rune::TextEdit;

/// What the popover keeps between frames.
pub struct RuntimeUi {
    pub open: bool,
    /// The conversation being edited (`task:<id>`, `home:<profile>:<workspace>`) and the chip the popover hangs from.
    pub conversation: String,
    pub anchor: &'static str,
    /// The pins being edited (a copy of what the conversation has until the person changes something).
    pub draft: RuntimePreferences,
    /// What the conversation had when the popover was opened, and its revision.
    pub saved: RuntimePreferences,
    pub revision: u64,
    pub catalog: Option<ModelCatalog>,
    pub loading: bool,
    pub catalog_error: Option<String>,
    pub saving: bool,
    pub apply_error: Option<String>,
    pub status: Option<String>,
    /// One line for each pin a model change dropped.
    pub notes: Vec<String>,
    /// The model search field.
    pub query: TextEdit,
}

impl Default for RuntimeUi {
    fn default() -> Self {
        RuntimeUi {
            open: false,
            conversation: String::new(),
            anchor: "runtime-chip",
            draft: RuntimePreferences::default(),
            saved: RuntimePreferences::default(),
            revision: 0,
            catalog: None,
            loading: false,
            catalog_error: None,
            saving: false,
            apply_error: None,
            status: None,
            notes: Vec::new(),
            query: TextEdit::new(""),
        }
    }
}

/// The pins as the web parses them: a provider and a model are one pin, so one alone is none.
pub fn clean(prefs: &RuntimePreferences) -> RuntimePreferences {
    let keep = |v: &Option<String>| v.clone().filter(|s| !s.is_empty());
    let (provider, model) = (keep(&prefs.provider), keep(&prefs.model));
    let both = provider.is_some() && model.is_some();
    RuntimePreferences { provider: provider.filter(|_| both), model: model.filter(|_| both), reasoning_effort: keep(&prefs.reasoning_effort), service_tier: keep(&prefs.service_tier), allow_paid_speed: prefs.allow_paid_speed }
}

fn is_automatic(p: &RuntimePreferences) -> bool {
    p.model.is_none() && p.reasoning_effort.is_none() && p.service_tier.is_none()
}

fn usable(model: &CatalogModel) -> bool {
    model.access != ModelAccess::Unavailable
}

fn model_of<'a>(catalog: &'a ModelCatalog, provider: &Option<String>, id: &Option<String>) -> Option<&'a CatalogModel> {
    let (provider, id) = (provider.as_deref()?, id.as_deref()?);
    catalog.sources.iter().find(|s| s.provider == if provider.is_empty() { "codex" } else { provider })?.models.iter().find(|m| m.id == id)
}

/// Effort or speed levels this selection can reach: its model's, or every usable model's. Speed never lists `default`
/// (Normal is offered by the control itself).
pub fn options_for(prefs: &RuntimePreferences, catalog: &ModelCatalog, speed: bool) -> Vec<RuntimeOption> {
    let models: Vec<&CatalogModel> = if prefs.provider.is_some() && prefs.model.is_some() {
        model_of(catalog, &prefs.provider, &prefs.model).into_iter().collect()
    } else {
        catalog.sources.iter().filter(|s| s.status == SourceStatus::Ready).flat_map(|s| s.models.iter()).filter(|m| usable(m)).collect()
    };
    let mut seen: Vec<String> = Vec::new();
    let mut out = Vec::new();
    for option in models.into_iter().flat_map(|m| if speed { m.speed_options.clone() } else { m.effort_options.clone() }.unwrap_or_default()) {
        if (speed && option.id == "default") || seen.contains(&option.id) {
            continue;
        }
        seen.push(option.id.clone());
        out.push(option);
    }
    out
}

/// "Neko decides", or the model's name without a leading "GPT-5 " style prefix.
pub fn model_label(p: &RuntimePreferences, catalog: &ModelCatalog) -> String {
    let (Some(_), Some(model)) = (&p.provider, &p.model) else { return "Neko decides".to_owned() };
    let label = model_of(catalog, &p.provider, &p.model).map_or(model.clone(), |m| m.label.clone());
    strip_gpt(&label)
}

/// The web's `label.replace(/^GPT[- ]?[0-9]+(?:\.[0-9]+)?[- ]/i, "")`.
fn strip_gpt(label: &str) -> String {
    let lower = label.to_ascii_lowercase();
    let Some(rest) = lower.strip_prefix("gpt") else { return label.to_owned() };
    let mut at = 3;
    let bytes = lower.as_bytes();
    if matches!(rest.chars().next(), Some('-' | ' ')) {
        at += 1;
    }
    let digits = |from: usize| bytes[from..].iter().take_while(|b| b.is_ascii_digit()).count();
    let d = digits(at);
    if d == 0 {
        return label.to_owned();
    }
    at += d;
    if bytes.get(at) == Some(&b'.') && digits(at + 1) > 0 {
        at += 1 + digits(at + 1);
    }
    if matches!(bytes.get(at), Some(b'-' | b' ')) { label[at + 1..].to_owned() } else { label.to_owned() }
}

pub fn effort_label(p: &RuntimePreferences, catalog: &ModelCatalog) -> String {
    let Some(effort) = &p.reasoning_effort else { return "Auto".to_owned() };
    options_for(p, catalog, false).into_iter().find(|o| &o.id == effort).map(|o| o.label).unwrap_or_else(|| {
        let mut c = effort.chars();
        c.next().map_or_else(String::new, |f| f.to_uppercase().collect::<String>() + c.as_str())
    })
}

pub fn speed_label(p: &RuntimePreferences, catalog: &ModelCatalog) -> String {
    match p.service_tier.as_deref() {
        None => "Auto".to_owned(),
        Some("default") => "Normal".to_owned(),
        Some(tier) => options_for(p, catalog, true).into_iter().find(|o| o.id == tier).map_or_else(|| tier.to_owned(), |o| o.label),
    }
}

/// "GPT label · High · Fast", the chip's words.
pub fn compact_label(p: &RuntimePreferences, catalog: &ModelCatalog) -> String {
    let mut parts = vec![model_label(p, catalog)];
    if p.reasoning_effort.is_some() {
        parts.push(effort_label(p, catalog));
    }
    if p.service_tier.is_some() {
        parts.push(speed_label(p, catalog));
    }
    parts.join(" · ")
}

/// A model change drops only the pins it cannot honour, with a sentence for each.
pub fn select_model(prefs: &RuntimePreferences, provider: &str, model: &CatalogModel) -> (RuntimePreferences, Vec<String>) {
    let mut next = RuntimePreferences { provider: Some(provider.to_owned()), model: Some(model.id.clone()), ..prefs.clone() };
    let mut notes = Vec::new();
    if let Some(effort) = next.reasoning_effort.clone()
        && !model.effort_options.clone().unwrap_or_default().iter().any(|o| o.id == effort)
    {
        notes.push(format!("Effort reset to Auto: {} does not advertise {effort}.", model.label));
        next.reasoning_effort = None;
    }
    if let Some(tier) = next.service_tier.clone()
        && tier != "default"
        && !model.speed_options.clone().unwrap_or_default().iter().any(|o| o.id == tier)
    {
        notes.push(format!("Speed reset to Auto: {} does not advertise {tier}.", model.label));
        next.service_tier = None;
    }
    (next, notes)
}

/// Why these pins cannot be applied, when no connected model honours them all.
pub fn validation(draft: &RuntimePreferences, catalog: &ModelCatalog, loaded: bool) -> Option<&'static str> {
    if !loaded || is_automatic(draft) {
        return None;
    }
    let compatible = catalog
        .sources
        .iter()
        .filter(|s| s.status == SourceStatus::Ready && draft.provider.as_ref().is_none_or(|p| &s.provider == p))
        .flat_map(|s| s.models.iter())
        .filter(|m| usable(m) && draft.model.as_ref().is_none_or(|id| &m.id == id))
        .any(|m| {
            draft.reasoning_effort.as_ref().is_none_or(|e| m.effort_options.clone().unwrap_or_default().iter().any(|o| &o.id == e))
                && draft.service_tier.as_ref().is_none_or(|t| t == "default" || m.speed_options.clone().unwrap_or_default().iter().any(|o| &o.id == t))
        });
    (!compatible).then_some("No connected model advertises these pins. Choose a model, reset an axis to Auto, or refresh models.")
}

impl RuntimeUi {
    pub fn dirty(&self) -> bool {
        clean(&self.draft) != clean(&self.saved)
    }

    pub fn loaded(&self) -> bool {
        self.catalog.is_some()
    }

    pub fn can_apply(&self) -> bool {
        let catalog = self.catalog.clone().unwrap_or_default();
        self.dirty() && !self.saving && !self.loading && validation(&self.draft, &catalog, self.loaded()).is_none() && (self.loaded() || is_automatic(&self.draft))
    }

    pub fn choose(&mut self, provider: &str, model: &CatalogModel) {
        let (next, notes) = select_model(&self.draft, provider, model);
        self.notes = notes;
        self.set_draft(next);
    }

    pub fn automatic_model(&mut self) {
        self.notes.clear();
        let next = RuntimePreferences { provider: None, model: None, ..self.draft.clone() };
        self.set_draft(next);
    }

    pub fn set_draft(&mut self, next: RuntimePreferences) {
        self.draft = next;
        self.apply_error = None;
        self.status = None;
    }

    pub fn discard(&mut self) {
        self.notes.clear();
        let saved = self.saved.clone();
        self.set_draft(saved);
    }

    pub fn reset(&mut self) {
        self.notes.clear();
        self.set_draft(RuntimePreferences::default());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog() -> ModelCatalog {
        serde_json::from_value(serde_json::json!({
            "sources": [
                {"provider": "claude", "label": "Claude", "connection": "Pro", "status": "ready", "models": [
                    {"id": "opus", "label": "Opus", "access": "listed", "effort_options": [{"id": "low", "label": "Low"}, {"id": "high", "label": "High"}], "speed_options": []},
                    {"id": "haiku", "label": "Haiku", "access": "listed", "effort_options": [], "speed_options": []},
                    {"id": "old", "label": "Old", "access": "unavailable", "effort_options": [{"id": "max", "label": "Max"}]}
                ]},
                {"provider": "codex", "label": "Codex", "connection": "Plus", "status": "ready", "models": [
                    {"id": "gpt-5.5", "label": "GPT-5.5 Codex", "access": "checked", "effort_options": [{"id": "high", "label": "High"}], "speed_options": [{"id": "default", "label": "Normal"}, {"id": "priority", "label": "Fast"}]}
                ]},
                {"provider": "opencode", "label": "OpenCode", "connection": "Not running", "status": "not_running", "models": [
                    {"id": "x", "label": "X", "access": "listed", "effort_options": [{"id": "ultra", "label": "Ultra"}]}
                ]}
            ]
        }))
        .unwrap()
    }

    fn pins(provider: &str, model: &str, effort: Option<&str>, speed: Option<&str>) -> RuntimePreferences {
        RuntimePreferences { provider: Some(provider.into()), model: Some(model.into()), reasoning_effort: effort.map(Into::into), service_tier: speed.map(Into::into), allow_paid_speed: false }
    }

    #[test]
    fn a_provider_without_a_model_is_no_pin() {
        let only_provider = RuntimePreferences { provider: Some("claude".into()), ..Default::default() };
        assert_eq!(clean(&only_provider), RuntimePreferences::default());
        assert_eq!(clean(&pins("claude", "opus", None, None)), pins("claude", "opus", None, None));
    }

    #[test]
    fn effort_levels_are_the_models_own_or_every_usable_ready_models() {
        let c = catalog();
        let ids = |v: Vec<RuntimeOption>| v.into_iter().map(|o| o.id).collect::<Vec<_>>();
        assert_eq!(ids(options_for(&pins("claude", "opus", None, None), &c, false)), ["low", "high"]);
        // Automatic: every usable model of every ready source; unavailable models and sources that are not ready add none.
        assert_eq!(ids(options_for(&RuntimePreferences::default(), &c, false)), ["low", "high"]);
        assert!(options_for(&pins("claude", "haiku", None, None), &c, false).is_empty(), "an empty list means unsupported");
    }

    #[test]
    fn speed_never_lists_normal_because_the_control_offers_it() {
        let c = catalog();
        let speeds: Vec<String> = options_for(&pins("codex", "gpt-5.5", None, None), &c, true).into_iter().map(|o| o.id).collect();
        assert_eq!(speeds, ["priority"]);
    }

    #[test]
    fn changing_the_model_drops_only_the_pins_it_cannot_honour_and_says_so() {
        let c = catalog();
        let haiku = c.sources[0].models[1].clone();
        let (next, notes) = select_model(&pins("claude", "opus", Some("high"), Some("priority")), "claude", &haiku);
        assert_eq!((next.model.as_deref(), next.reasoning_effort, next.service_tier), (Some("haiku"), None, None));
        assert_eq!(notes.len(), 2);
        assert!(notes[0].starts_with("Effort reset to Auto: Haiku does not advertise high."));
        // A pin the new model honours stays.
        let opus = c.sources[0].models[0].clone();
        let (kept, notes) = select_model(&pins("claude", "haiku", Some("low"), Some("default")), "claude", &opus);
        assert_eq!((kept.reasoning_effort.as_deref(), kept.service_tier.as_deref()), (Some("low"), Some("default")));
        assert!(notes.is_empty(), "Normal is always honoured");
    }

    #[test]
    fn labels_read_as_the_chip_does() {
        let c = catalog();
        assert_eq!(model_label(&RuntimePreferences::default(), &c), "Neko decides");
        assert_eq!(compact_label(&pins("codex", "gpt-5.5", Some("high"), Some("priority")), &c), "Codex · High · Fast");
        assert_eq!(compact_label(&pins("claude", "opus", None, Some("default")), &c), "Opus · Normal");
        assert_eq!(strip_gpt("GPT-5 Codex"), "Codex");
        assert_eq!(strip_gpt("gpt 4.1 mini"), "mini");
        assert_eq!(strip_gpt("GPT-image"), "GPT-image");
        assert_eq!(strip_gpt("Opus"), "Opus");
    }

    #[test]
    fn pins_no_model_honours_are_refused_with_the_webs_sentence() {
        let c = catalog();
        assert_eq!(validation(&pins("claude", "haiku", Some("high"), None), &c, true), Some("No connected model advertises these pins. Choose a model, reset an axis to Auto, or refresh models."));
        assert_eq!(validation(&pins("claude", "opus", Some("high"), None), &c, true), None);
        assert_eq!(validation(&pins("claude", "haiku", Some("high"), None), &c, false), None, "nothing is judged before the catalog loads");
        assert_eq!(validation(&RuntimePreferences::default(), &c, true), None);
    }

    #[test]
    fn apply_needs_a_change_and_a_loaded_catalog_unless_it_is_a_reset() {
        let mut ui = RuntimeUi { saved: pins("claude", "opus", None, None), draft: pins("claude", "opus", None, None), ..Default::default() };
        assert!(!ui.can_apply(), "nothing changed");
        ui.draft.reasoning_effort = Some("high".into());
        assert!(!ui.can_apply(), "an unloaded catalog cannot vouch for a pin");
        ui.catalog = Some(catalog());
        assert!(ui.can_apply());
        ui.saving = true;
        assert!(!ui.can_apply());
        ui.saving = false;
        ui.reset();
        assert!(ui.can_apply() && ui.draft == RuntimePreferences::default(), "reset to Neko decides is a change to apply");
        ui.discard();
        assert_eq!(ui.draft, ui.saved);
    }
}
