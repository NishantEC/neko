//! Bounded judgment history over the existing settings store. The daemon holds
//! its database lock across observation/save. This module never executes work,
//! modifies grants, or interprets cancellation as a working-style preference.
use crate::{Db, workbench};
use neko_protocol::decision_context::*;
use neko_protocol::workbench::{AgentRuntime, Snapshot, SupervisorAction, TaskStatus};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;

const SETTING: &str = "neko_decision_context_v1";
pub const MAX_RECORDS: usize = 128;
pub const MAX_PREFERENCES: usize = 64;
pub const MAX_STORAGE_BYTES: usize = 1024 * 1024;
pub const MAX_RECORD_BYTES: usize = 32 * 1024;
pub const MAX_TEXT_BYTES: usize = 2048;
// Leave room for a maximally escaped user correction without rewriting the
// observation when a new immutable version is appended.
const HOST_RECORD_BUDGET: usize = MAX_RECORD_BYTES - MAX_TEXT_BYTES * 6 - 2048;
pub const PROMPT_BUDGET: usize = 4096;
const MAX_ITEMS: usize = 16;

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct State {
    decision_records: Vec<DecisionRecord>,
    working_preferences: Vec<WorkingPreference>,
}

fn load(db: &Db, snapshot: &Snapshot) -> Result<State, String> {
    let state = match db.get_setting(SETTING).map_err(|e| e.to_string())? {
        None => State::default(),
        Some(json) => {
            if json.len() > MAX_STORAGE_BYTES {
                return Err("Cannot read decision context: storage exceeds its byte limit".into());
            }
            serde_json::from_str(&json).map_err(|e| format!("Cannot read decision context: {e}"))?
        }
    };
    validate(&state, snapshot).map_err(|e| format!("Cannot read decision context: {e}"))?;
    Ok(state)
}

pub(crate) fn attach(db: &Db, snapshot: &mut Snapshot) -> Result<(), String> {
    let state = load(db, snapshot)?;
    snapshot.decision_records = state.decision_records;
    snapshot.working_preferences = state.working_preferences;
    Ok(())
}

fn save(db: &Db, state: &mut State, snapshot: &Snapshot) -> Result<(), String> {
    // History is FIFO and immutable while retained; preferences are never evicted.
    while state.decision_records.len() > MAX_RECORDS {
        state.decision_records.remove(0);
    }
    let mut json = serde_json::to_string(state).map_err(|e| e.to_string())?;
    while json.len() > MAX_STORAGE_BYTES && state.decision_records.len() > 1 {
        state.decision_records.remove(0);
        json = serde_json::to_string(state).map_err(|e| e.to_string())?;
    }
    if json.len() > MAX_STORAGE_BYTES {
        return Err("Decision context storage is full".into());
    }
    validate(state, snapshot)?;
    db.set_setting(SETTING, &json).map_err(|e| e.to_string())
}

fn text(value: &str, limit: usize, required: bool) -> Result<(), String> {
    if value.len() > limit || (required && value.trim().is_empty()) {
        return Err(format!(
            "Decision context text must be {}under {limit} bytes",
            if required { "nonempty and " } else { "" }
        ));
    }
    Ok(())
}

fn ids(values: &[String]) -> Result<(), String> {
    if values.len() > MAX_ITEMS || values.iter().collect::<HashSet<_>>().len() != values.len() {
        return Err("Too many or duplicate decision context references".into());
    }
    for value in values {
        text(value, 256, true)?;
    }
    Ok(())
}

fn workspace<'a>(
    snapshot: &'a Snapshot,
    id: &str,
) -> Result<&'a neko_protocol::workbench::Workspace, String> {
    snapshot
        .workspaces
        .iter()
        .find(|w| w.id == id)
        .ok_or_else(|| "Decision context workspace no longer exists".into())
}

fn task_scope(
    snapshot: &Snapshot,
    workspace_id: &str,
    task_id: &str,
    require_present: bool,
) -> Result<(), String> {
    text(task_id, 256, true)?;
    match snapshot.tasks.iter().find(|t| t.id == task_id) {
        Some(t) if t.workspace_id != workspace_id => {
            Err("Decision context task belongs to another workspace".into())
        }
        None if require_present => Err("Decision context task no longer exists".into()),
        _ => Ok(()), // Historical episodes survive ticket-history deletion.
    }
}

fn validate(state: &State, snapshot: &Snapshot) -> Result<(), String> {
    if state.decision_records.len() > MAX_RECORDS
        || state.working_preferences.len() > MAX_PREFERENCES
    {
        return Err("Decision context record limit exceeded".into());
    }
    let mut record_ids = HashSet::new();
    let mut versions = HashSet::new();
    let mut fingerprints = std::collections::HashMap::new();
    for r in &state.decision_records {
        workspace(snapshot, &r.workspace_id)?;
        for id in [
            &r.id,
            &r.episode_id,
            &r.workspace_id,
            &r.scope.agent_profile_id,
        ] {
            text(id, 256, true)?;
        }
        if r.version == 0
            || !record_ids.insert(&r.id)
            || !versions.insert((&r.episode_id, r.version))
        {
            return Err("Invalid or duplicate decision context version".into());
        }
        if r.source.fingerprint.len() != 64
            || !r.source.fingerprint.bytes().all(|c| c.is_ascii_hexdigit())
        {
            return Err("Invalid decision context source fingerprint".into());
        }
        if let Some(episode) = fingerprints.insert(&r.source.fingerprint, &r.episode_id) {
            if episode != &r.episode_id {
                return Err("Duplicate decision context source fingerprint".into());
            }
        }
        if let Some(id) = &r.task_id {
            task_scope(snapshot, &r.workspace_id, id, false)?;
        }
        text(&r.scope.repository, 4096, true)?;
        text(&r.runtime.provider, 120, false)?;
        text(&r.runtime.model, 120, false)?;
        ids(&r.scope.connection_ids)?;
        ids(&r.scope.responsibility_ids)?;
        // Historical scope is frozen. Live source ids, connection assignments,
        // responsibilities and receipts can roll over or disappear independently.
        text(&r.rationale, MAX_TEXT_BYTES, true)?;
        for value in [&r.expected_outcome, &r.observed_outcome, &r.source.revision]
            .into_iter()
            .flatten()
        {
            text(value, MAX_TEXT_BYTES, false)?;
        }
        if r.alternatives.len() > MAX_ITEMS
            || r.source.declared_evidence.len() > MAX_ITEMS
            || r.source.evidence.len() > 8
            || r.preference_versions.len() > MAX_PREFERENCES
        {
            return Err("Too many decision context evidence items".into());
        }
        for value in r.alternatives.iter().chain(&r.source.declared_evidence) {
            text(value, 512, true)?;
        }
        let mut sources = HashSet::new();
        for e in &r.source.evidence {
            if !sources.insert(&e.source_id) {
                return Err("Duplicate decision context evidence".into());
            }
            for id in [&e.source_id, &e.connection_id] {
                text(id, 256, true)?;
            }
            text(&e.revision, MAX_TEXT_BYTES, false)?;
            text(&e.title, 512, false)?;
            text(&e.description, MAX_TEXT_BYTES, false)?;
            ids(&e.receipt_ids)?;
            if !r.scope.connection_ids.contains(&e.connection_id) {
                return Err("Decision context evidence outside its scope".into());
            }
        }
        let mut prefs = HashSet::new();
        for p in &r.preference_versions {
            text(&p.id, 256, true)?;
            if p.version == 0 || !prefs.insert(&p.id) {
                return Err("Invalid preference version reference".into());
            }
            if state.working_preferences.iter().any(|current| {
                current.id == p.id
                    && (current.workspace_id != r.workspace_id
                        || (current.version == p.version
                            && current.agent_profile_id != r.scope.agent_profile_id)
                        || current.version < p.version)
            }) {
                return Err(
                    "Decision context preference reference outside its scope or version".into(),
                );
            }
        }
        match (&r.supersedes_record_id, &r.correction) {
            (None, None) if r.version == 1 && r.episode_id == r.id => {}
            (Some(prior), Some(c)) if r.version > 1 && prior == &c.record_id => {
                text(prior, 256, true)?;
                text(&c.text, MAX_TEXT_BYTES, true)?;
                if let Some(previous) = state.decision_records.iter().find(|p| p.id == *prior) {
                    let mut expected = previous.clone();
                    expected.id = r.id.clone();
                    expected.version = r.version;
                    expected.supersedes_record_id = r.supersedes_record_id.clone();
                    expected.correction = r.correction.clone();
                    expected.created_at_ms = r.created_at_ms;
                    if previous.version.checked_add(1) != Some(r.version) || expected != *r {
                        return Err("Decision correction changed the host observation".into());
                    }
                }
            }
            _ => return Err("Invalid decision context correction chain".into()),
        }
        if r.delivery_stage == DeliveryStage::LocalAccepted
            && (r.action != DecisionAction::AcceptLocalResult
                || r.provenance != DecisionProvenance::UserCommand)
        {
            return Err("Local acceptance requires a successful user command".into());
        }
        if serde_json::to_vec(r).map_err(|e| e.to_string())?.len() > MAX_RECORD_BYTES {
            return Err("Decision context episode is too large".into());
        }
    }
    let mut preference_ids = HashSet::new();
    for p in &state.working_preferences {
        workspace(snapshot, &p.workspace_id)?;
        text(&p.id, 256, true)?;
        text(&p.agent_profile_id, 256, true)?;
        if p.version == 0 || !preference_ids.insert(&p.id) {
            return Err("Invalid or duplicate working preference version".into());
        }
        validate_preference(snapshot, state, p, false, None)?;
    }
    Ok(())
}

fn validate_preference(
    snapshot: &Snapshot,
    state: &State,
    p: &WorkingPreference,
    require_refs: bool,
    previous: Option<&WorkingPreference>,
) -> Result<(), String> {
    text(&p.instruction, MAX_TEXT_BYTES, true)?;
    if crate::neko_memory::sensitive(&p.instruction).is_some() {
        return Err("Working preferences cannot contain secrets".into());
    }
    ids(&p.applicability.task_ids)?;
    ids(&p.supporting_record_ids)?;
    if p.applicability.terms.is_empty() && p.applicability.task_ids.is_empty() {
        return Err("Choose a context for this working preference".into());
    }
    if p.applicability.terms.len() > MAX_ITEMS || p.exceptions.len() > MAX_ITEMS {
        return Err("Too many working preference contexts".into());
    }
    for term in p.applicability.terms.iter().chain(&p.exceptions) {
        text(term, 128, true)?;
        if tokens(term).is_empty() || crate::neko_memory::sensitive(term).is_some() {
            return Err("Working preference context needs words without secrets".into());
        }
    }
    for task in &p.applicability.task_ids {
        // Existing rows retain references after ticket-history deletion. Only
        // references already validated on this row may be resubmitted missing.
        let retained = previous.is_some_and(|old| old.applicability.task_ids.contains(task));
        task_scope(snapshot, &p.workspace_id, task, require_refs && !retained)?;
    }
    for id in &p.supporting_record_ids {
        match state.decision_records.iter().find(|r| r.id == *id) {
            Some(r) if r.workspace_id != p.workspace_id => {
                return Err("Supporting record belongs to another workspace".into());
            }
            None if require_refs
                && !previous.is_some_and(|old| old.supporting_record_ids.contains(id)) =>
            {
                return Err("Supporting record no longer exists".into());
            }
            _ => {} // Evicted history leaves an honest reference, never fabricated evidence.
        }
    }
    Ok(())
}

fn next_version(version: u32) -> Result<u32, String> {
    version
        .checked_add(1)
        .ok_or_else(|| "Decision context version exhausted".into())
}

fn check_version(actual: u32, expected: u32) -> Result<(), String> {
    if actual != expected {
        return Err("Decision context changed; reload before editing".into());
    }
    Ok(())
}

pub(crate) fn apply(
    db: &Db,
    snapshot: &Snapshot,
    command: DecisionContextCommand,
) -> Result<(), String> {
    let mut state = load(db, snapshot)?;
    let now = workbench::now_ms();
    match command {
        DecisionContextCommand::SavePreference {
            workspace_id,
            id,
            expected_version,
            mut applicability,
            instruction,
            supporting_record_ids,
            mut exceptions,
        } => {
            workspace(snapshot, &workspace_id)?;
            for term in applicability.terms.iter_mut().chain(&mut exceptions) {
                *term = term.trim().to_lowercase();
            }
            let owner = snapshot.agent_profiles.owner(&workspace_id).to_owned();
            let mut p = WorkingPreference {
                id: id.clone(),
                workspace_id,
                agent_profile_id: owner,
                state: PreferenceState::Proposed,
                applicability,
                instruction: instruction.trim().to_owned(),
                supporting_record_ids,
                exceptions,
                version: 1,
                created_at_ms: now,
                updated_at_ms: now,
            };
            let previous = if id.is_empty() {
                if expected_version.is_some() {
                    return Err("New preferences have no expected version".into());
                }
                if state.working_preferences.len() >= MAX_PREFERENCES {
                    return Err("Working preference storage is full; forget one first".into());
                }
                p.id = workbench::new_id();
                None
            } else {
                let previous = state
                    .working_preferences
                    .iter()
                    .find(|p| p.id == id)
                    .ok_or("Working preference no longer exists")?;
                if previous.workspace_id != p.workspace_id {
                    return Err("Working preference belongs to another workspace".into());
                }
                check_version(
                    previous.version,
                    expected_version.ok_or("Editing needs the current version")?,
                )?;
                p.version = next_version(previous.version)?;
                p.created_at_ms = previous.created_at_ms;
                Some(previous)
            };
            validate_preference(snapshot, &state, &p, true, previous)?;
            state.working_preferences.retain(|old| old.id != p.id);
            state.working_preferences.push(p);
        }
        DecisionContextCommand::KeepPreference {
            workspace_id,
            id,
            expected_version,
        } => {
            let p = preference_mut(&mut state, snapshot, &workspace_id, &id, expected_version)?;
            if p.agent_profile_id != snapshot.agent_profiles.owner(&workspace_id) {
                return Err(
                    "Edit this preference for the workspace's current agent before keeping it"
                        .into(),
                );
            }
            p.state = PreferenceState::Confirmed;
            p.version = next_version(p.version)?;
            p.updated_at_ms = now;
        }
        DecisionContextCommand::DismissPreference {
            workspace_id,
            id,
            expected_version,
        } => {
            let p = preference_mut(&mut state, snapshot, &workspace_id, &id, expected_version)?;
            p.state = PreferenceState::Dismissed;
            p.version = next_version(p.version)?;
            p.updated_at_ms = now;
        }
        DecisionContextCommand::ForgetPreference {
            workspace_id,
            id,
            expected_version,
        } => {
            preference_mut(&mut state, snapshot, &workspace_id, &id, expected_version)?;
            state.working_preferences.retain(|p| p.id != id);
        }
        DecisionContextCommand::CorrectDecision {
            workspace_id,
            record_id,
            expected_version,
            correction,
        } => {
            workspace(snapshot, &workspace_id)?;
            text(correction.trim(), MAX_TEXT_BYTES, true)?;
            if crate::neko_memory::sensitive(&correction).is_some() {
                return Err("Decision corrections cannot contain secrets".into());
            }
            let prior = state
                .decision_records
                .iter()
                .find(|r| r.id == record_id)
                .ok_or("Decision record no longer exists")?;
            if prior.workspace_id != workspace_id {
                return Err("Decision record belongs to another workspace".into());
            }
            check_version(prior.version, expected_version)?;
            if state
                .decision_records
                .iter()
                .any(|r| r.episode_id == prior.episode_id && r.version > prior.version)
            {
                return Err("Decision already corrected; reload the latest version".into());
            }
            let mut corrected = prior.clone();
            corrected.id = workbench::new_id();
            corrected.version = next_version(prior.version)?;
            corrected.supersedes_record_id = Some(record_id.clone());
            corrected.correction = Some(DecisionCorrection {
                record_id,
                text: correction.trim().into(),
                at_ms: now,
            });
            corrected.created_at_ms = now;
            let corrected_id = corrected.id.clone();
            let task_id = corrected.task_id.clone();
            state.decision_records.push(corrected);
            // Only explicit user corrections enter learning here. Cancellation,
            // replies, model prose and observations never infer a preference.
            if snapshot.memory_options.learning {
                if let Some(task_id) = task_id.filter(|id| {
                    snapshot
                        .tasks
                        .iter()
                        .any(|t| t.id == *id && t.workspace_id == workspace_id)
                }) {
                    if state.working_preferences.len() >= MAX_PREFERENCES {
                        return Err("Working preference storage is full; forget one before learning from a correction".into());
                    }
                    state.working_preferences.push(WorkingPreference {
                        id: workbench::new_id(),
                        workspace_id: workspace_id.clone(),
                        agent_profile_id: snapshot.agent_profiles.owner(&workspace_id).into(),
                        state: PreferenceState::Proposed,
                        applicability: PreferenceApplicability {
                            terms: vec![],
                            task_ids: vec![task_id],
                        },
                        instruction: correction.trim().into(),
                        supporting_record_ids: vec![corrected_id],
                        exceptions: vec![],
                        version: 1,
                        created_at_ms: now,
                        updated_at_ms: now,
                    });
                }
            }
        }
    }
    save(db, &mut state, snapshot)
}

fn preference_mut<'a>(
    state: &'a mut State,
    snapshot: &Snapshot,
    workspace_id: &str,
    id: &str,
    version: u32,
) -> Result<&'a mut WorkingPreference, String> {
    workspace(snapshot, workspace_id)?;
    let p = state
        .working_preferences
        .iter_mut()
        .find(|p| p.id == id)
        .ok_or("Working preference no longer exists")?;
    if p.workspace_id != workspace_id {
        return Err("Working preference belongs to another workspace".into());
    }
    check_version(p.version, version)?;
    Ok(p)
}

#[derive(Debug, Clone, Copy)]
pub enum HostObservation {
    Supervisor,
    Plan,
    Review,
    Failure,
}

/// Trusted host API, absent from the public wire command. Call after saving the
/// corresponding task state, with the runtime actually used for that run.
pub fn record_task_observation(
    db: &Db,
    snapshot: &Snapshot,
    task_id: &str,
    observation: HostObservation,
    runtime: &AgentRuntime,
) -> Result<DecisionRecord, String> {
    observe(db, snapshot, task_id, observation, snapshot, runtime)
}

/// Preserve the runtime and guidance actually supplied to a run, even if the
/// user edits preferences while it is executing. Sources/receipts are checked
/// against the committed result snapshot, never against old grants.
pub fn record_task_observation_with_context(
    db: &Db,
    result_snapshot: &Snapshot,
    task_id: &str,
    observation: HostObservation,
    decision_snapshot: &Snapshot,
) -> Result<DecisionRecord, String> {
    observe(
        db,
        result_snapshot,
        task_id,
        observation,
        decision_snapshot,
        &decision_snapshot.agent_runtime,
    )
}

fn observe(
    db: &Db,
    snapshot: &Snapshot,
    task_id: &str,
    observation: HostObservation,
    decision_snapshot: &Snapshot,
    runtime: &AgentRuntime,
) -> Result<DecisionRecord, String> {
    let task = snapshot
        .tasks
        .iter()
        .find(|t| t.id == task_id)
        .ok_or("Decision context task no longer exists")?;
    let (action, rationale, provenance, alternatives, observed) = match observation {
        HostObservation::Supervisor => {
            let s = task
                .supervision
                .as_ref()
                .ok_or("Task has no supervisor decision")?;
            let action = match s.action {
                SupervisorAction::PrepareFix => DecisionAction::PrepareFix,
                SupervisorAction::AskUser => DecisionAction::AskUser,
                SupervisorAction::Skip => DecisionAction::Skip,
            };
            let alternatives = [
                DecisionAction::PrepareFix,
                DecisionAction::AskUser,
                DecisionAction::Skip,
            ]
            .into_iter()
            .filter(|a| *a != action)
            .map(|a| {
                serde_json::to_value(a)
                    .unwrap()
                    .as_str()
                    .unwrap()
                    .to_owned()
            })
            .collect();
            (
                action,
                s.reason.clone(),
                DecisionProvenance::Supervisor,
                alternatives,
                None,
            )
        }
        HostObservation::Plan
            if matches!(
                task.status,
                TaskStatus::AwaitingApproval | TaskStatus::Building
            ) && !task.plan.trim().is_empty() =>
        {
            let waiting = task
                .events
                .iter()
                .rev()
                .find(|e| e.role == "supervisor")
                .and_then(|e| {
                    if e.message.starts_with("Needs your input before building:") {
                        Some((DecisionAction::AskUser, e.message.clone()))
                    } else if e.message.starts_with("Nothing to build:") {
                        Some((DecisionAction::Skip, e.message.clone()))
                    } else {
                        None
                    }
                });
            let (action, rationale) =
                waiting.unwrap_or((DecisionAction::ProposePlan, task.plan.clone()));
            (
                action,
                rationale,
                DecisionProvenance::HostObservation,
                vec![],
                None,
            )
        }
        HostObservation::Review if task.status == TaskStatus::ReadyForReview => (
            DecisionAction::ReviewLocalResult,
            "Host observed a local result ready for user review.".into(),
            DecisionProvenance::HostObservation,
            vec![],
            Some(
                "Independent verification gate passed; useful/interaction outcome not observed"
                    .into(),
            ),
        ),
        HostObservation::Failure if task.status == TaskStatus::Failed => {
            let host_failure = task
                .events
                .iter()
                .rev()
                .find(|e| matches!(e.role.as_str(), "system" | "supervisor"));
            let observed = host_failure
                .map(|e| format!("Task failed: {}", e.message))
                .unwrap_or_else(|| "Task failed; host failure detail not recorded".into());
            (
                DecisionAction::ReportFailure,
                "Host observed the task entering Failed.".into(),
                DecisionProvenance::HostObservation,
                vec![],
                Some(observed),
            )
        }
        _ => return Err("Host observation does not match the task state".into()),
    };
    record(
        db,
        snapshot,
        task_id,
        action,
        rationale,
        provenance,
        alternatives,
        observed,
        decision_snapshot,
        runtime,
        None,
    )
}

pub(crate) fn record_user_command(
    db: &Db,
    snapshot: &Snapshot,
    task_id: &str,
    action: DecisionAction,
    note: Option<&str>,
) -> Result<DecisionRecord, String> {
    let task = snapshot
        .tasks
        .iter()
        .find(|t| t.id == task_id)
        .ok_or("Decision context task no longer exists")?;
    let (rationale, observed) = match action {
        DecisionAction::ApproveLocalBuild => {
            ("User approved the plan for a local build.".into(), None)
        }
        DecisionAction::CancelTask => ("User cancelled the task; reason unknown.".into(), None),
        DecisionAction::AddNote => (note.ok_or("User note missing")?.to_owned(), None),
        DecisionAction::AcceptLocalResult if task.status == TaskStatus::Completed => (
            "User accepted the local result; delivery beyond this workspace is unknown.".into(),
            Some("User accepted the local result; useful/interaction outcome not observed".into()),
        ),
        _ => return Err("Unsupported user decision observation".into()),
    };
    let user = AgentRuntime {
        provider: "user".into(),
        model: String::new(), ..Default::default()
    };
    record(
        db,
        snapshot,
        task_id,
        action,
        rationale,
        DecisionProvenance::UserCommand,
        vec![],
        observed,
        snapshot,
        &user,
        Some(workbench::new_id()),
    )
}

fn nonempty(text: &str) -> Option<String> {
    (!text.trim().is_empty()).then(|| text.to_owned())
}

#[allow(clippy::too_many_arguments)]
fn record(
    db: &Db,
    snapshot: &Snapshot,
    task_id: &str,
    action: DecisionAction,
    rationale: String,
    provenance: DecisionProvenance,
    alternatives: Vec<String>,
    observed: Option<String>,
    decision_snapshot: &Snapshot,
    runtime: &AgentRuntime,
    command_receipt: Option<String>,
) -> Result<DecisionRecord, String> {
    let mut state = load(db, snapshot)?;
    let task = snapshot
        .tasks
        .iter()
        .find(|t| t.id == task_id)
        .ok_or("Decision context task no longer exists")?;
    let ws = workspace(snapshot, &task.workspace_id)?;
    workspace(decision_snapshot, &task.workspace_id)?;
    task_scope(decision_snapshot, &task.workspace_id, task_id, true)?;
    let mut evidence = Vec::new();
    let mut responsibility_ids = Vec::new();
    let mut connection_ids = Vec::new();
    let mut unavailable_sources = Vec::new();
    for source in snapshot
        .mcp
        .sources
        .iter()
        .filter(|s| s.task_id.as_deref() == Some(task_id))
    {
        let connection = snapshot
            .mcp
            .connections
            .iter()
            .find(|c| c.id == source.connection_id);
        let responsibility = snapshot
            .mcp
            .responsibilities
            .iter()
            .find(|r| r.id == source.responsibility_id);
        let stopping = matches!(
            action,
            DecisionAction::CancelTask | DecisionAction::ReportFailure
        );
        let scoped_evidence_available = connection.zip(responsibility).is_some_and(|(c, r)| {
            (c.workspace_id.is_empty() || c.workspace_id == ws.id)
                && r.workspace_id == ws.id
                && r.connection_ids.contains(&c.id)
                && !source.receipt_ids.is_empty()
                && source.receipt_ids.iter().all(|id| {
                    snapshot.mcp.receipts.iter().any(|receipt| {
                        receipt.id == *id
                            && receipt.workspace_id == ws.id
                            && receipt.connection_id == c.id
                            && receipt.success
                    })
                })
        });
        if stopping && !scoped_evidence_available {
            // Stopping is always allowed. Keep an explicit absence marker;
            // never borrow current evidence from another workspace or invent
            // retained receipts after pruning/deletion.
            unavailable_sources.push(format!("Source evidence unavailable for {}: current scoped responsibility or successful receipts unavailable.", source.id));
            continue;
        }
        let connection = connection.ok_or("Decision source connection unavailable")?;
        let responsibility = responsibility.ok_or("Decision source responsibility unavailable")?;
        if (!connection.workspace_id.is_empty() && connection.workspace_id != ws.id)
            || responsibility.workspace_id != ws.id
            || !responsibility.connection_ids.contains(&connection.id)
        {
            return Err("Decision source belongs to another workspace".into());
        }
        let needs_current_grant = provenance != DecisionProvenance::UserCommand
            && action != DecisionAction::ReportFailure;
        if needs_current_grant && source.receipt_ids.is_empty() {
            return Err("Decision source needs successful scoped receipts".into());
        }
        let mut receipt_ids = Vec::new();
        let mut pruned_receipts = false;
        for id in &source.receipt_ids {
            let Some(receipt) = snapshot
                .mcp
                .receipts
                .iter()
                .find(|r| r.id == *id)
            else {
                pruned_receipts = true;
                continue;
            };
            if receipt.workspace_id != ws.id || receipt.connection_id != connection.id || !receipt.success {
                return Err("Decision source needs successful scoped receipts".into());
            }
            if needs_current_grant
                && (!connection
                    .tools
                    .iter()
                    .any(|t| t.name == receipt.tool_name && t.schema_hash == receipt.schema_hash)
                    || !snapshot.mcp.grants.iter().any(|g| {
                        g.workspace_id == ws.id
                            && g.connection_id == connection.id
                            && g.tool_name == receipt.tool_name
                            && g.schema_hash == receipt.schema_hash
                    }))
            {
                return Err("Decision source receipt has no current scoped schema grant".into());
            }
            // Repeated intake references are the same successful receipt, not
            // independent observations. Normalize after validating every input.
            if !receipt_ids.contains(id) {
                receipt_ids.push(id.clone());
            }
        }
        if pruned_receipts {
            // Tickets outlive bounded tool history. Record the local action,
            // but attach no source proof whose receipts cannot be checked.
            // Execution/standing-authority checks still require live receipts.
            unavailable_sources.push(format!("Source evidence unavailable for {}: historical tool receipts are no longer retained.", source.id));
            continue;
        }
        if !responsibility_ids.contains(&responsibility.id) {
            responsibility_ids.push(responsibility.id.clone());
        }
        if !connection_ids.contains(&connection.id) {
            connection_ids.push(connection.id.clone());
        }
        evidence.push(EvidenceSnapshot {
            source_id: source.id.clone(),
            connection_id: connection.id.clone(),
            revision: source.revision.clone(),
            retrieved_ms: source.retrieved_ms,
            receipt_ids,
            title: source.title.clone(),
            description: source.description.clone(),
        });
    }
    let rationale = if unavailable_sources.is_empty() {
        rationale
    } else {
        format!("{rationale}\n{}", unavailable_sources.join("\n"))
    };
    let decision_task = decision_snapshot
        .tasks
        .iter()
        .find(|t| t.id == task_id)
        .ok_or("Decision-time task unavailable")?;
    let about = format!("{} {}", decision_task.title, decision_task.goal);
    let preference_versions =
        if provenance == DecisionProvenance::UserCommand || runtime.provider == "host" {
            vec![] // User/host actions do not claim to have consulted model guidance.
        } else {
            prompt_preferences(decision_snapshot, &ws.id, Some(task_id), &about)
                .into_iter()
                .map(|p| PreferenceVersion {
                    id: p.id.clone(),
                    version: p.version,
                })
                .collect()
        };
    let mut r = DecisionRecord {
        id: String::new(),
        episode_id: String::new(),
        version: 1,
        supersedes_record_id: None,
        workspace_id: ws.id.clone(),
        task_id: Some(task.id.clone()),
        source: DecisionSource {
            fingerprint: String::new(),
            revision: task.source_revision.clone(),
            evidence,
            declared_evidence: task
                .supervision
                .as_ref()
                .map(|s| s.evidence.clone())
                .unwrap_or_default(),
        },
        action,
        alternatives,
        rationale,
        provenance,
        runtime: runtime.clone(),
        preference_versions,
        scope: DecisionScope {
            agent_profile_id: decision_snapshot.agent_profiles.owner(&ws.id).into(),
            profile_revision: decision_snapshot.agent_profiles.revision,
            repository: snapshot.root_for(task, ws),
            responsibility_ids,
            connection_ids,
        },
        expected_outcome: nonempty(&task.goal),
        observed_outcome: observed,
        delivery_stage: if action == DecisionAction::AcceptLocalResult {
            DeliveryStage::LocalAccepted
        } else {
            DeliveryStage::Unknown
        },
        correction: None,
        created_at_ms: 0,
    };
    // Hash full inputs BEFORE shortening prose. Different Unicode suffixes and
    // different source revisions remain different episodes after truncation.
    let fingerprint = format!(
        "{:x}",
        Sha256::digest(
            serde_json::to_vec(&(&r, &task.plan, &task.result, command_receipt))
                .map_err(|e| e.to_string())?
        )
    );
    if let Some(previous) = state
        .decision_records
        .iter()
        .rev()
        .find(|r| r.source.fingerprint == fingerprint)
    {
        return Ok(previous.clone());
    }
    r.id = workbench::new_id();
    r.episode_id = r.id.clone();
    r.source.fingerprint = fingerprint;
    r.created_at_ms = workbench::now_ms();
    r.rationale = truncate(&r.rationale, MAX_TEXT_BYTES);
    r.expected_outcome = r.expected_outcome.map(|s| truncate(&s, MAX_TEXT_BYTES));
    r.observed_outcome = r.observed_outcome.map(|s| truncate(&s, MAX_TEXT_BYTES));
    // Identifiers and revisions are never shortened; rejecting preserves identity.
    r.source.declared_evidence.truncate(MAX_ITEMS);
    for item in &mut r.source.declared_evidence {
        *item = truncate(item, 512);
    }
    r.source.evidence.truncate(8);
    for e in &mut r.source.evidence {
        e.title = truncate(&e.title, 512);
        e.description = truncate(&e.description, MAX_TEXT_BYTES);
    }
    compact_record(&mut r)?;
    state.decision_records.push(r.clone());
    save(db, &mut state, snapshot)?;
    Ok(r)
}

/// Truncate by UTF-8 bytes at a character boundary, including the ellipsis.
fn truncate(text: &str, limit: usize) -> String {
    if text.len() <= limit {
        return text.into();
    }
    if limit < 3 {
        return String::new();
    }
    let mut end = limit - 3;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &text[..end])
}

fn compact_record(record: &mut DecisionRecord) -> Result<(), String> {
    // JSON control-character escaping can cost six bytes per source byte.
    // Shorten only prose, never identifiers, revisions, scope or references.
    let mut limit = MAX_TEXT_BYTES;
    while serde_json::to_vec(&record)
        .map_err(|e| e.to_string())?
        .len()
        > HOST_RECORD_BUDGET
    {
        limit /= 2;
        if limit < 64 {
            return Err(
                "Decision context identity/evidence references exceed the record byte limit".into(),
            );
        }
        record.rationale = truncate(&record.rationale, limit);
        for s in [&mut record.expected_outcome, &mut record.observed_outcome]
            .into_iter()
            .flatten()
        {
            *s = truncate(s, limit);
        }
        for s in record
            .source
            .declared_evidence
            .iter_mut()
            .chain(&mut record.alternatives)
        {
            *s = truncate(s, limit.min(512));
        }
        for e in &mut record.source.evidence {
            e.title = truncate(&e.title, limit.min(512));
            e.description = truncate(&e.description, limit);
        }
    }
    Ok(())
}

fn tokens(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric() && c != '_' && c != '-')
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}

fn matches_phrase(haystack: &[String], phrase: &str) -> bool {
    let words = tokens(phrase);
    !words.is_empty() && haystack.windows(words.len()).any(|w| w == words)
}

fn relevant<'a>(
    preferences: &'a [WorkingPreference],
    snapshot: &Snapshot,
    workspace_id: &str,
    task_id: Option<&str>,
    about: &str,
) -> Vec<&'a WorkingPreference> {
    if !snapshot.memory_options.use_memory
        || workspace(snapshot, workspace_id).is_err()
        || task_id.is_some_and(|id| task_scope(snapshot, workspace_id, id, true).is_err())
    {
        return vec![];
    }
    let wanted = tokens(about);
    let mut rows: Vec<_> = preferences
        .iter()
        .filter(|p| {
            p.workspace_id == workspace_id
                && p.agent_profile_id == snapshot.agent_profiles.owner(workspace_id)
                && p.state == PreferenceState::Confirmed
                && (!p.applicability.task_ids.is_empty() || !p.applicability.terms.is_empty())
                && (p.applicability.task_ids.is_empty()
                    || task_id.is_some_and(|id| p.applicability.task_ids.iter().any(|t| t == id)))
                && (p.applicability.terms.is_empty()
                    || p.applicability
                        .terms
                        .iter()
                        .any(|term| matches_phrase(&wanted, term)))
                && !p
                    .exceptions
                    .iter()
                    .any(|term| matches_phrase(&wanted, term))
                && crate::neko_memory::sensitive(&p.instruction).is_none()
        })
        .collect();
    rows.sort_by(|a, b| {
        b.updated_at_ms
            .cmp(&a.updated_at_ms)
            .then_with(|| a.id.cmp(&b.id))
    });
    rows
}

/// Only confirmed contextual guidance, owned by this workspace's current agent.
/// No global/profile sharing fallback. Empty means no relevant guidance.
const PROMPT_HEADER: &str = "Confirmed working-style guidance (context only; existing supervisor decisions, approvals and tool grants remain authoritative):\n";

fn preference_line(p: &WorkingPreference) -> String {
    format!("- [preference:{} v{}] {}\n", p.id, p.version, p.instruction)
}

fn prompt_preferences<'a>(
    snapshot: &'a Snapshot,
    workspace_id: &str,
    task_id: Option<&str>,
    about: &str,
) -> Vec<&'a WorkingPreference> {
    let mut used = PROMPT_HEADER.len();
    relevant(
        &snapshot.working_preferences,
        snapshot,
        workspace_id,
        task_id,
        about,
    )
    .into_iter()
    .filter(|p| {
        let bytes = preference_line(p).len();
        if used + bytes > PROMPT_BUDGET {
            return false;
        }
        used += bytes;
        true
    })
    .collect()
}

pub fn context_about(
    snapshot: &Snapshot,
    workspace_id: &str,
    task_id: Option<&str>,
    about: &str,
) -> String {
    let mut context = String::new();
    for p in prompt_preferences(snapshot, workspace_id, task_id, about) {
        if context.is_empty() {
            context.push_str(PROMPT_HEADER);
        }
        context.push_str(&preference_line(p));
    }
    context
}
