//! The self-improvement loop: Neko notices trouble that repeats (a tool it could not find, a
//! correction given three times, the same question asked again), asks the cheapest model for one
//! suggestion, delivers it as one Inbox item with a default, and applies it only when the person
//! says so. After an apply it watches the same signal for 7 days and offers Undo when it did not
//! help. See docs/knowledge/self-improvement.md.
//!
//! - `signals`: what counts as a signal, observations in their own row, the limits.
//! - `prompt`: the one prompt (golden file) and the validation of the model's answer.
//! - `apply`: applying, rejecting, undoing and verifying, with every side effect behind `Effects`.
//!
//! Rows (never the workbench snapshot): `improve_state_v1` (the suggestions), `improve_signals_v1`
//! (observations, quiet), `improve_undo_v1:<id>` (what an undo needs), `improve_off_v1`.
use crate::Db;
use neko_protocol::improve::{ImproveEvent, ImproveKind, ImproveReply, ImproveState, Improvement};
use neko_protocol::routines::{Cadence, NeedsYou, Routine, RoutineKind, RoutineRun};
use serde::{Deserialize, Serialize};

#[path = "improve/apply.rs"]
pub mod apply;
#[path = "improve/prompt.rs"]
pub mod prompt;
#[path = "improve/signals.rs"]
pub mod signals;

pub use apply::{apply, reject, undo, verify, Effects};

pub const DAY_MS: i64 = 24 * 60 * 60 * 1000;
/// Proposals per rolling day.
pub const PER_DAY: usize = 3;
/// How long a rejected key stays quiet.
pub const REJECT_MEMORY_DAYS: i64 = 30;
/// How long an applied suggestion's signal is watched.
pub const VERIFY_DAYS: i64 = 7;
/// Evidence a proposal carries.
pub const MAX_EVIDENCE: usize = 4 * 1024;
const MAX_KEPT: usize = 200;

const STATE_ROW: &str = "improve_state_v1";
const OFF_ROW: &str = "improve_off_v1";
const UNDO_ROW: &str = "improve_undo_v1:";
/// The routine that holds suggestion items. It never runs; it only gives them a place in the
/// Inbox, where the phone channels already read them (as goals do).
pub const ROUTINE_PREFIX: &str = "improve:";
/// Item links: `improve:<id>` is the suggestion, `improve-undo:<id>` its "That didn't help" item.
pub const LINK: &str = "improve:";
pub const UNDO_LINK: &str = "improve-undo:";

/// "Suggestions" in Settings, AI. On by default.
pub fn enabled(db: &Db) -> bool {
    db.get_setting(OFF_ROW).ok().flatten().is_none_or(|v| v != "1")
}

pub fn set_enabled(db: &Db, on: bool) -> Result<(), String> {
    db.set_setting(OFF_ROW, if on { "0" } else { "1" }).map_err(|e| e.to_string())
}

pub fn load(db: &Db) -> Vec<Improvement> {
    db.get_setting(STATE_ROW).ok().flatten().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

pub fn save(db: &Db, all: &[Improvement]) -> Result<(), String> {
    let mut all = all.to_vec();
    // Past the cap the oldest finished ones go first.
    while all.len() > MAX_KEPT {
        let victim = all.iter().position(|p| matches!(p.state, ImproveState::Helped | ImproveState::Rejected | ImproveState::DidntHelp)).unwrap_or(0);
        all.remove(victim);
    }
    db.set_setting(STATE_ROW, &serde_json::to_string(&all).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

/// What an undo needs, kept per suggestion (a skill's old file can be 64 KB).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Undo {
    pub skill_path: String,
    pub skill_base: String,
    pub skill_revised: String,
    /// The `source` of the skill proposal made for a patch or a new skill.
    pub skill_source: String,
    pub new_skill_proposal: String,
    pub order_workspace: String,
    pub order_line: String,
    pub order_before: String,
    pub order_had_entry: bool,
    pub pack: String,
    pub task_id: String,
}

pub fn undo_record(db: &Db, id: &str) -> Undo {
    db.get_setting(&format!("{UNDO_ROW}{id}")).ok().flatten().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

pub fn save_undo(db: &Db, id: &str, undo: &Undo) -> Result<(), String> {
    db.set_setting(&format!("{UNDO_ROW}{id}"), &serde_json::to_string(undo).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

pub(crate) fn note(p: &mut Improvement, at_ms: i64, text: impl Into<String>) {
    p.history.push(ImproveEvent { at_ms, text: text.into() });
    let extra = p.history.len().saturating_sub(40);
    p.history.drain(..extra);
}

/// What a pass knows about the world besides the database.
#[derive(Debug, Clone, Default)]
pub struct World {
    /// The catalog's packs: name and description.
    pub packs: Vec<(String, String)>,
    /// A Development checkout is set, so `code_task` may be suggested.
    pub dev_checkout: bool,
    /// Where a signal with no workspace of its own appears.
    pub default_workspace: String,
    /// Standing orders by workspace (missing means the default).
    pub orders: std::collections::BTreeMap<String, String>,
}

/// Makes a suggestion from a due signal's draft. Refused when the signal is no longer due (another
/// pass made one, the day's three are used, the key is open or was rejected lately).
pub fn add(db: &Db, signal: &signals::Signal, draft: prompt::Draft, workspace_id: &str, now: i64) -> Result<Improvement, String> {
    if !signals::due(db, now).iter().any(|s| s.key == signal.key) {
        return Err("That signal is not due any more".into());
    }
    let mut p = Improvement {
        id: crate::workbench::new_id(),
        signal_key: signal.key.clone(),
        kind: draft.kind,
        title: draft.title,
        why: draft.why,
        change: draft.change,
        default_label: draft.default_label,
        target: draft.target,
        workspace_id: workspace_id.to_owned(),
        evidence: signal.evidence.chars().take(MAX_EVIDENCE + 400).collect(),
        diff: draft.diff,
        state: ImproveState::Open,
        created_ms: now,
        ..Default::default()
    };
    note(&mut p, now, format!("Suggested after: {}", signal.label));
    let mut all = load(db);
    all.push(p.clone());
    save(db, &all)?;
    Ok(p)
}

/// One pass: records what was seen, takes the signals that are due, asks the model (no lock held)
/// for one suggestion each and delivers the valid ones as Inbox items. A model that is not
/// available stops the pass (it will run again); an answer that does not validate is dropped,
/// logged once, and its key waits a day. Returns the ids of the new suggestions.
pub fn run_pass(dbm: &std::sync::Mutex<Db>, inputs: &signals::Inputs, world: &World, ask: &dyn Fn(&str) -> Result<String, String>, now: i64) -> Result<Vec<String>, String> {
    let lock = || dbm.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if !enabled(&lock()) {
        return Ok(Vec::new());
    }
    signals::record(&lock(), inputs, now)?;
    let due = signals::due(&lock(), now);
    let skills = crate::skills::load(&lock()).map(|s| s.available).unwrap_or_default();
    let date = chrono::DateTime::from_timestamp_millis(now).map_or_else(String::new, |t| t.format("%Y-%m-%d").to_string());
    let pack_names: Vec<String> = world.packs.iter().map(|(n, _)| n.clone()).collect();
    let mut made = Vec::new();
    for signal in due {
        let workspace = if signal.workspace_id.is_empty() { world.default_workspace.clone() } else { signal.workspace_id.clone() };
        let related = prompt::related(&skills, &signal, 8);
        let orders = world.orders.get(&workspace).map(String::as_str).unwrap_or(crate::routines::DEFAULT_STANDING_ORDERS);
        let text = prompt::prompt(&prompt::Context { signal: &signal, packs: &world.packs, skills: &related, standing_orders: orders, dev_checkout: world.dev_checkout });
        let answer = match ask(&text) {
            Ok(answer) => answer,
            Err(error) => {
                eprintln!("neko improve: no model answer for {}: {error}", signal.key);
                break;
            }
        };
        let db = lock();
        match prompt::parse(&db, &answer, &prompt::Rules { packs: &pack_names, dev_checkout: world.dev_checkout, date: &date }) {
            Ok(draft) => match add(&db, &signal, draft, &workspace, now).and_then(|p| deliver(&db, &p.id, now).map(|_| p.id)) {
                Ok(id) => made.push(id),
                Err(error) => eprintln!("neko improve: {}: {error}", signal.key),
            },
            Err(error) => {
                eprintln!("neko improve: dropped a suggestion for {}: {error}", signal.key);
                signals::mark_dropped(&db, &signal.key, now)?;
            }
        }
    }
    Ok(made)
}

/// The suggestions, newest first, with what a client needs to show them.
pub fn view(db: &Db, now_ms: i64, dev_checkout: bool) -> ImproveReply {
    let mut proposals = load(db);
    for p in &mut proposals {
        if let Some(applied) = p.applied_ms {
            p.after = signals::count_since(db, &p.signal_key, applied, now_ms.max(applied));
        }
        let has_undo = matches!(p.state, ImproveState::Applied | ImproveState::Helped | ImproveState::DidntHelp) && applied_something(db, p);
        p.can_undo = has_undo;
        p.undo_confirms = has_undo && matches!(p.kind, ImproveKind::InstallPack | ImproveKind::CodeTask);
    }
    proposals.sort_by_key(|p| std::cmp::Reverse(p.created_ms));
    ImproveReply { enabled: enabled(db), dev_checkout, proposals, open_pack: None, note: None }
}

/// Whether an undo record exists, so there is something to reverse.
fn applied_something(db: &Db, p: &Improvement) -> bool {
    let undo = undo_record(db, &p.id);
    match p.kind {
        ImproveKind::SkillPatch => !undo.skill_path.is_empty(),
        ImproveKind::NewSkill => !undo.new_skill_proposal.is_empty(),
        ImproveKind::StandingOrder => !undo.order_line.is_empty(),
        ImproveKind::InstallPack => !undo.pack.is_empty(),
        ImproveKind::CodeTask => !undo.task_id.is_empty(),
    }
}

// ---------------------------------------------------------------------------
// Inbox
// ---------------------------------------------------------------------------

fn raise(db: &Db, workspace_id: &str, title: &str, detail: &str, link: &str, default: &str, now: i64) -> Result<u64, String> {
    use crate::routines as core;
    if workspace_id.is_empty() {
        return Err("A suggestion needs a workspace to appear under".into());
    }
    let id = format!("{ROUTINE_PREFIX}{workspace_id}");
    let mut store = core::load(db)?;
    if !store.routines.iter().any(|r| r.id == id) {
        store.routines.push(Routine {
            id: id.clone(),
            workspace_id: workspace_id.to_owned(),
            kind: RoutineKind::Custom,
            name: "Suggestions".into(),
            instruction: "Suggestions Neko makes about itself report here.".into(),
            cadence: Cadence::Daily { hour: 0, minute: 0 },
            enabled: false,
            model: String::new(),
            paseo_schedule: None,
            browser: false,
            computer: false,
            customized: true,
            catch_up: None,
        });
        core::save_definitions(db, &store)?;
    }
    let seq = core::reserve_seq(db, 1)?;
    let item = NeedsYou {
        seq,
        title: crate::inbox::one_question(title),
        detail: detail.chars().take(1500).collect(),
        link: Some(link.to_owned()),
        created_ms: now,
        touched_ms: now,
        default: crate::inbox::clean_default(default),
        ..Default::default()
    };
    let run = RoutineRun { id: crate::workbench::new_id(), routine_id: id.clone(), started_ms: now, finished_ms: Some(now), ok: true, quiet: false, did: Vec::new(), needs_you: vec![item], error: None, text: String::new(), ..Default::default() };
    core::push_run(db, &id, run)?;
    Ok(seq)
}

/// Closes an open item of ours, with the reason Closed recently shows.
pub fn close_item(db: &Db, seq: u64, reason: &str, now: i64) -> Result<(), String> {
    use crate::routines as core;
    for routine in core::load(db)?.routines.iter().filter(|r| r.id.starts_with(ROUTINE_PREFIX)) {
        let mut runs = core::load_runs(db, &routine.id)?;
        let mut changed = false;
        for item in runs.iter_mut().flat_map(|r| r.needs_you.iter_mut()) {
            if item.seq == seq && !item.done {
                crate::inbox::close(item, reason, now);
                changed = true;
            }
        }
        if changed {
            core::save_runs(db, &routine.id, &runs)?;
        }
    }
    Ok(())
}

/// The suggestion as one Inbox item: "Neko suggests: <title>", why plus the change summary, and
/// the apply action as the default.
pub fn deliver(db: &Db, id: &str, now: i64) -> Result<u64, String> {
    let mut all = load(db);
    let p = all.iter_mut().find(|p| p.id == id).ok_or("That suggestion is gone")?;
    if let Some(seq) = p.inbox_seq {
        return Ok(seq);
    }
    let detail = format!("{}\n\n{}", p.why.trim(), change_summary(p));
    let seq = raise(db, &p.workspace_id, &format!("Neko suggests: {}", p.title), &detail, &format!("{LINK}{}", p.id), &p.default_label, now)?;
    p.inbox_seq = Some(seq);
    note(p, now, "Raised in the Inbox");
    save(db, &all)?;
    Ok(seq)
}

/// "That didn't help: <title>", whose default is Undo.
pub fn deliver_undo(db: &Db, p: &mut Improvement, now: i64) -> Result<u64, String> {
    let detail = format!("It fired {} time{} in the 7 days before and {} since.\n\n{}", p.before, if p.before == 1 { "" } else { "s" }, p.after, change_summary(p));
    let seq = raise(db, &p.workspace_id, &format!("That didn't help: {}", p.title), &detail, &format!("{UNDO_LINK}{}", p.id), "Undo it", now)?;
    p.undo_seq = Some(seq);
    Ok(seq)
}

/// A line of what the change is, for the Inbox detail.
pub fn change_summary(p: &Improvement) -> String {
    let words = match p.kind {
        ImproveKind::InstallPack => format!("Opens the preview of the {} pack. You install it there; nothing installs by itself.", p.target),
        ImproveKind::SkillPatch => format!("Revises the skill {}. Neko applies it itself only when its own rules allow; otherwise you approve it in Skills.", p.target),
        ImproveKind::NewSkill => format!("Adds a new skill, {}, for you to review in Skills.", p.target),
        ImproveKind::StandingOrder => format!("Adds this line to the standing orders: {}", crate::goals::head(&p.change, 300)),
        ImproveKind::CodeTask => format!("Starts a task in the Development checkout: {}", crate::goals::head(&p.change, 300)),
    };
    format!("Change: {words}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use signals::{Inputs, Seen};
    use std::sync::Mutex;

    fn failing(n: &str, now: i64) -> Seen {
        Seen { id: n.into(), at_ms: now - 1000, text: format!("zsh: command not found: tool-{n}"), workspace_id: "w1".into() }
    }

    const ORDER: &str = r#"{"kind":"standing_order","title":"Look in the usual places","why":"A tool was not found.","change":"When a command is not found, look in /opt/homebrew/bin first.","default_label":"Add this standing order"}"#;

    #[test]
    fn a_pass_with_a_stub_model_makes_at_most_three_a_day_each_one_inbox_item() {
        let db = Mutex::new(Db::open_in_memory().unwrap());
        let now = 100 * DAY_MS;
        let inputs = Inputs { failures: ["a", "b", "c", "d"].iter().map(|n| failing(n, now)).collect(), ..Default::default() };
        let calls = std::cell::Cell::new(0);
        let made = run_pass(&db, &inputs, &World::default(), &|_| {
            calls.set(calls.get() + 1);
            Ok(ORDER.into())
        }, now)
        .unwrap();
        assert_eq!((made.len(), calls.get()), (3, 3));
        assert_eq!(load(&db.lock().unwrap()).len(), 3);
        // The fourth waits for tomorrow; nothing new is asked today.
        assert!(run_pass(&db, &Inputs::default(), &World::default(), &|_| panic!("asked"), now + 1000).unwrap().is_empty());
        let next_day = run_pass(&db, &Inputs::default(), &World::default(), &|_| Ok(ORDER.into()), now + DAY_MS + 1000).unwrap();
        assert_eq!(next_day.len(), 1);
    }

    #[test]
    fn a_pass_drops_a_bad_answer_once_and_stops_without_a_model() {
        let db = Mutex::new(Db::open_in_memory().unwrap());
        let now = 100 * DAY_MS;
        let inputs = Inputs { failures: vec![failing("a", now)], ..Default::default() };
        assert!(run_pass(&db, &inputs, &World::default(), &|_| Ok("not json".into()), now).unwrap().is_empty());
        assert!(load(&db.lock().unwrap()).is_empty());
        // Dropped: not asked again for a day.
        assert!(run_pass(&db, &Inputs::default(), &World::default(), &|_| panic!("asked"), now + 1000).unwrap().is_empty());
        // No model: nothing is dropped or made, and it is asked again later.
        let db2 = Mutex::new(Db::open_in_memory().unwrap());
        assert!(run_pass(&db2, &inputs, &World::default(), &|_| Err("no model".into()), now).unwrap().is_empty());
        assert_eq!(run_pass(&db2, &Inputs::default(), &World::default(), &|_| Ok(ORDER.into()), now + 1000).unwrap().len(), 1);
    }

    #[test]
    fn with_suggestions_off_a_pass_reads_nothing_and_asks_nothing() {
        let db = Mutex::new(Db::open_in_memory().unwrap());
        set_enabled(&db.lock().unwrap(), false).unwrap();
        let now = 100 * DAY_MS;
        let inputs = Inputs { failures: vec![failing("a", now)], ..Default::default() };
        assert!(run_pass(&db, &inputs, &World::default(), &|_| panic!("asked"), now).unwrap().is_empty());
        assert!(signals::due(&db.lock().unwrap(), now).is_empty());
        set_enabled(&db.lock().unwrap(), true).unwrap();
        assert!(signals::due(&db.lock().unwrap(), now).is_empty(), "what was seen while off was never kept");
    }

    #[test]
    fn the_view_marks_what_can_be_undone_and_which_undos_ask_first() {
        let db = Db::open_in_memory().unwrap();
        let mk = |id: &str, kind, state| Improvement { id: id.into(), kind, state, applied_ms: Some(1), ..Default::default() };
        save(&db, &[mk("a", ImproveKind::InstallPack, ImproveState::Applied), mk("b", ImproveKind::StandingOrder, ImproveState::Applied), mk("c", ImproveKind::CodeTask, ImproveState::Open)]).unwrap();
        save_undo(&db, "a", &Undo { pack: "linear".into(), ..Undo::default() }).unwrap();
        save_undo(&db, "b", &Undo { order_line: "x".into(), ..Undo::default() }).unwrap();
        let reply = view(&db, 10, true);
        let by = |id: &str| reply.proposals.iter().find(|p| p.id == id).unwrap();
        assert_eq!((by("a").can_undo, by("a").undo_confirms), (true, true));
        assert_eq!((by("b").can_undo, by("b").undo_confirms), (true, false));
        assert!(!by("c").can_undo);
        assert!(reply.enabled && reply.dev_checkout);
    }
}
