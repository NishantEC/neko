//! The one gate every agent start passes (`may_start`), and the pacing for
//! new tickets.
//!
//! **The gate (H1, docs/knowledge/admission-gate.md).** Whatever starts an
//! agent (a task worker, a goal turn, a routine round, a watch wake, a cat's
//! turn, the skill curator, the goal judge, triage, naming and the other small
//! model calls) asks `may_start` first. It checks, in order: the machine
//! steward holding; the steward's cap on agents running at once; the daily
//! spend ceiling; quiet hours, for starts nobody asked for. A start the
//! person asked for directly skips quiet hours only. A held start is not a
//! failure: it stays pending for its own tick and the site records one
//! `Held: <reason>` line where the person can see it.
//!
//! **Pacing.** There is deliberately no fixed limit on how many
//! tickets work at once, but every agent is a full CLI session with its own
//! build and test processes, and many starting together has overloaded this
//! machine before (a load average above 240 restarted it). So new agents are
//! released from a small bucket that refills slowly, and none start while the
//! machine is already busy. Agents that are running are never affected.
//! Separately, at most `Snapshot::writable_per_repository` tickets (default
//! 2) per repository hold a writable run at once; read-only phases are not
//! counted.
use neko_protocol::workbench::{Snapshot, Task, TaskStatus};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use neko_core::Db;
use neko_protocol::workbench::MachineHealth;
use std::collections::HashSet;

/// Every kind of agent start. A new way to start an agent adds a variant here
/// and calls `may_start`; `no_agent_start_bypasses_the_gate` fails until it does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StartKind {
    /// A task's worker (planning, building, reviewing), claimed by the scheduler.
    Task,
    /// The next turn of a goal.
    GoalTurn,
    /// One round of a routine.
    Routine,
    /// A watch waking an agent to read its sources.
    WatchWake,
    /// A cat's turn (a reply to the person, or a look at what its sessions did).
    CatTurn,
    /// The daily skill curator pass and the self-improvement gatherer.
    SkillCurator,
    /// The goal judge's model call.
    Judge,
    /// Triage of a task a watch filed.
    Triage,
    /// Naming a task or a watch.
    Naming,
    /// Any other small tool-free model call (memory learning, chat summaries, a model check).
    SmallModel,
}

impl StartKind {
    #[cfg(test)]
    pub(crate) const ALL: [StartKind; 10] = [
        Self::Task, Self::GoalTurn, Self::Routine, Self::WatchWake, Self::CatTurn,
        Self::SkillCurator, Self::Judge, Self::Triage, Self::Naming, Self::SmallModel,
    ];

    /// Starts nobody asked for, which wait out quiet hours. (A person's own
    /// Run now, message to a cat or typed task is never proactive.)
    pub(crate) fn proactive(self) -> bool {
        matches!(self, Self::Routine | Self::WatchWake | Self::CatTurn | Self::SkillCurator)
    }
}

/// Why a start waits, in the words the person reads after `Held: `.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum HoldReason {
    /// The machine steward's own reason (disk, memory, load, comfort).
    Machine(String),
    /// The steward allows only this many agents at once right now.
    Cap(u32),
    /// Today's spend has reached the daily ceiling.
    Spend,
    /// Quiet hours, and nobody asked for this start.
    Quiet,
}

impl std::fmt::Display for HoldReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Machine(why) => f.write_str(why),
            Self::Cap(cap) => write!(f, "the Mac allows {cap} agent{} at once right now, and that many are running", if *cap == 1 { "" } else { "s" }),
            Self::Spend => f.write_str("today's spend has reached the daily ceiling"),
            Self::Quiet => f.write_str("quiet hours; it starts when they end"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Verdict {
    Admit,
    Hold(HoldReason),
}

impl Verdict {
    pub(crate) fn admitted(&self) -> bool {
        matches!(self, Self::Admit)
    }

    /// The `Held: <reason>` line a site shows, when held.
    pub(crate) fn line(&self) -> Option<String> {
        match self {
            Self::Admit => None,
            Self::Hold(reason) => Some(format!("{HELD}{reason}")),
        }
    }
}

/// What every "held" line starts with. Sites coalesce on it.
pub(crate) const HELD: &str = "Held: ";

enum Health {
    Latest,
    Given(Option<MachineHealth>),
}

/// One start being asked about.
pub(crate) struct Start<'a> {
    kind: StartKind,
    workspace: Option<&'a str>,
    person: bool,
    running: usize,
    now_ms: i64,
    health: Health,
}

impl<'a> Start<'a> {
    pub(crate) fn new(kind: StartKind, workspace: Option<&'a str>) -> Self {
        Self { kind, workspace, person: false, running: 0, now_ms: neko_core::workbench::now_ms(), health: Health::Latest }
    }

    /// The person asked for this start directly: it skips quiet hours (only).
    pub(crate) fn by_person(mut self, person: bool) -> Self {
        self.person = person;
        self
    }

    /// Agents running now, for the steward's cap.
    // TODO(H3): the count is tickets + Home chat + cat turns. H3 makes it
    // complete (routine, watch, curator, judge and small-model runs).
    pub(crate) fn running(mut self, running: usize) -> Self {
        self.running = running;
        self
    }

    pub(crate) fn at(mut self, now_ms: i64) -> Self {
        self.now_ms = now_ms;
        self
    }

    /// Use this steward reading instead of the latest one (tests, and sites that already read it).
    pub(crate) fn with_health(mut self, health: Option<&MachineHealth>) -> Self {
        self.health = Health::Given(health.cloned());
        self
    }
}

/// The decision itself, from facts already gathered: steward holding, the
/// cap, the spend ceiling, quiet hours, in that order.
pub(crate) fn decide(kind: StartKind, person: bool, health: Option<&MachineHealth>, running: usize, spend_held: impl FnOnce() -> bool, quiet: impl FnOnce() -> bool) -> Verdict {
    if let Some(health) = health {
        if let Some(why) = &health.holding {
            return Verdict::Hold(HoldReason::Machine(why.clone()));
        }
        if let Some(cap) = health.cap {
            if running >= cap as usize {
                return Verdict::Hold(HoldReason::Cap(cap));
            }
        }
    }
    if spend_held() {
        return Verdict::Hold(HoldReason::Spend);
    }
    if kind.proactive() && !person && quiet() {
        return Verdict::Hold(HoldReason::Quiet);
    }
    Verdict::Admit
}

/// Whether the person's quiet hours are on at `now_ms` (the phone's setting, the one place they are kept).
fn quiet_now(db: &Db, now_ms: i64) -> bool {
    neko_core::phone::quiet_at(&neko_core::phone::load_settings(db), neko_core::machine::local_hour(now_ms))
}

/// The one gate. Every start of an agent asks here first.
pub(crate) fn may_start(db: &Db, start: Start) -> Verdict {
    let health = match &start.health {
        Health::Latest => neko_core::machine::latest(),
        Health::Given(health) => health.clone(),
    };
    let verdict = decide(
        start.kind,
        start.person,
        health.as_ref(),
        start.running,
        || crate::routine_care::spend_held(db, start.now_ms),
        || quiet_now(db, start.now_ms),
    );
    let _ = start.workspace; // the ceiling and quiet hours are global today; kept so a per-workspace rule needs no new call sites
    verdict
}

/// Puts `line` ("Held: ...") on a held task, once per hold period: not when its latest event already is one.
pub(crate) fn note_held_on_task(task: &mut Task, line: &str) {
    if task.events.last().is_some_and(|e| e.role == HELD_ROLE && e.message.starts_with(HELD)) {
        return;
    }
    neko_core::workbench::append_event(task, HELD_ROLE, line);
}

/// `text` without a leading `Held: <reason>` line (a watch's last result carries one while held).
pub(crate) fn without_held(text: &str) -> &str {
    match text.strip_prefix(HELD) {
        Some(rest) => rest.split_once('\n').map_or("", |(_, tail)| tail),
        None => text,
    }
}

/// The event role of a "Held:" line on a task.
pub(crate) const HELD_ROLE: &str = "admission";

type Counter = Box<dyn Fn() -> usize + Send + Sync>;

static RUNNING: std::sync::OnceLock<Counter> = std::sync::OnceLock::new();

/// The controller registers how it counts running agents, for the few start sites that
/// have no handle on it. (Only the first registration counts.)
// TODO(H3): this becomes a read of the one `RunningRuns` registry.
pub(crate) fn register_running(count: impl Fn() -> usize + Send + Sync + 'static) {
    let _ = RUNNING.set(Box::new(count));
}

/// Agents running now (0 before registration, as in tests). Takes the controller's own
/// locks: call it before taking the database lock, never while holding `Controller::active`.
pub(crate) fn running_now() -> usize {
    RUNNING.get().map_or(0, |count| count())
}

fn background_holds() -> &'static Mutex<HashSet<&'static str>> {
    static SET: std::sync::OnceLock<Mutex<HashSet<&'static str>>> = std::sync::OnceLock::new();
    SET.get_or_init(|| Mutex::new(HashSet::new()))
}

/// For a start with no item to put a line on (the curator, triage, naming,
/// small model calls): asks the gate and, when held, writes one
/// `Held: <reason>` line to the daemon log per hold period for `site`.
/// Returns whether to go ahead.
pub(crate) fn admit_background(db: &Db, site: &'static str, start: Start) -> bool {
    let kind = start.kind;
    let workspace = start.workspace.map(str::to_owned);
    let verdict = may_start(db, start);
    let mut holds = background_holds().lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    match verdict.line() {
        None => {
            holds.remove(site);
            true
        }
        Some(line) => {
            if holds.insert(site) {
                eprintln!("neko admission: {site} ({kind:?}{}): {line}", workspace.map(|w| format!(", workspace {w}")).unwrap_or_default());
            }
            false
        }
    }
}

/// Agents that may start together after a quiet period.
pub(super) const BURST: f64 = 6.0;
/// After the burst, one more agent may start this often.
pub(super) const REFILL_EVERY: Duration = Duration::from_secs(10);
/// No new agent starts while the one-minute load average is at or above this.
pub(super) const LOAD_LIMIT: f64 = 20.0;

struct Bucket {
    tokens: f64,
    updated: Instant,
}

fn bucket() -> &'static Mutex<Option<Bucket>> {
    static BUCKET: Mutex<Option<Bucket>> = Mutex::new(None);
    &BUCKET
}

/// How many of `waiting` eligible tickets may start a worker now.
pub(super) fn admit(waiting: usize, load: Option<f64>, now: Instant) -> usize {
    let mut guard = bucket().lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    admit_from(&mut guard, waiting, load, now)
}

fn admit_from(slot: &mut Option<Bucket>, waiting: usize, load: Option<f64>, now: Instant) -> usize {
    let bucket = slot.get_or_insert(Bucket { tokens: BURST, updated: now });
    let refill = now.saturating_duration_since(bucket.updated).as_secs_f64() / REFILL_EVERY.as_secs_f64();
    bucket.tokens = (bucket.tokens + refill).min(BURST);
    bucket.updated = now;
    if load.is_some_and(|load| load >= LOAD_LIMIT) {
        return 0;
    }
    let admitted = (bucket.tokens.floor() as usize).min(waiting);
    bucket.tokens -= admitted as f64;
    admitted
}

/// Whether `task`'s next (or current) run changes its worktree. Mirrors
/// `execute_with_worktree`: a Queued ticket builds directly unless planning
/// is on, the user asked read-only, or a split proposal is pending; Planning
/// is a read-only scout or supervisor.
pub(super) fn writes(snapshot: &Snapshot, task: &Task) -> bool {
    match task.status {
        TaskStatus::Building | TaskStatus::Reviewing | TaskStatus::AwaitingApproval => true,
        TaskStatus::Queued => {
            !snapshot.plan_first
                && !snapshot.task_read_only.contains(&task.id)
                && !snapshot.splits.iter().any(|s| s.parent_id == task.id && !s.approved)
        }
        _ => false,
    }
}

fn repository(snapshot: &Snapshot, task: &Task) -> String {
    snapshot.workspaces.iter().find(|w| w.id == task.workspace_id)
        .map_or_else(|| task.workspace_id.clone(), |w| snapshot.root_for(task, w))
}

/// Keep, in order, the candidates (indexes into `snapshot.tasks`) that fit
/// under the per-repository cap on writable runs, counting the active
/// workers that already hold one. Read-only candidates always pass; held
/// back ones simply stay queued for a later tick.
pub(super) fn cap_writable_per_repository(snapshot: &Snapshot, active: &[&str], candidates: Vec<usize>) -> Vec<usize> {
    let limit = snapshot.writable_per_repository();
    let mut holding: HashMap<String, usize> = HashMap::new();
    for task in snapshot.tasks.iter().filter(|t| active.contains(&t.id.as_str()) && writes(snapshot, t)) {
        *holding.entry(repository(snapshot, task)).or_default() += 1;
    }
    candidates.into_iter().filter(|&index| {
        let task = &snapshot.tasks[index];
        if !writes(snapshot, task) {
            return true;
        }
        let count = holding.entry(repository(snapshot, task)).or_default();
        if *count >= limit {
            return false;
        }
        *count += 1;
        true
    }).collect()
}

/// The one-minute load average, or `None` if the system will not say.
pub(super) fn load_average() -> Option<f64> {
    unsafe extern "C" {
        fn getloadavg(loadavg: *mut f64, nelem: i32) -> i32;
    }
    let mut loads = [0.0_f64; 1];
    // SAFETY: getloadavg writes at most `nelem` doubles into the buffer.
    (unsafe { getloadavg(loads.as_mut_ptr(), 1) } == 1).then_some(loads[0])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bursts_then_paces_and_holds_back_under_load() {
        let start = Instant::now();
        let mut slot = None;
        assert_eq!(admit_from(&mut slot, 27, Some(3.0), start), 6, "a burst starts together");
        assert_eq!(admit_from(&mut slot, 21, Some(3.0), start + Duration::from_secs(2)), 0);
        assert_eq!(admit_from(&mut slot, 21, Some(3.0), start + Duration::from_secs(10)), 1, "then one every ten seconds");
        assert_eq!(admit_from(&mut slot, 20, Some(25.0), start + Duration::from_secs(60)), 0, "nothing new while overloaded");
        assert_eq!(admit_from(&mut slot, 20, Some(4.0), start + Duration::from_secs(60)), 5, "credit earned while waiting is kept");
        assert_eq!(admit_from(&mut slot, 2, None, start + Duration::from_secs(600)), 2, "an unknown load does not block work");
    }

    #[test]
    fn at_most_the_limit_of_writing_tickets_per_repository_and_read_only_work_is_not_capped() {
        use neko_protocol::workbench::Workspace;
        let workspace = |id: &str, repository: &str| Workspace { id: id.into(), name: id.into(), repository: repository.into(), instructions: String::new(), away_enabled: false };
        let task = |id: &str, workspace: &str, status: TaskStatus| Task { id: id.into(), workspace_id: workspace.into(), status, ..crate::workbench::tests::controller_task() };
        let mut snapshot = Snapshot { workspaces: vec![workspace("a", "/acme-api"), workspace("b", "/navi")], max_writable_per_repository: Some(4), ..Default::default() };
        // Two already writing in acme-api, one planning (read-only) there.
        snapshot.tasks.push(task("running-1", "a", TaskStatus::Building));
        snapshot.tasks.push(task("running-2", "a", TaskStatus::Reviewing));
        snapshot.tasks.push(task("scouting", "a", TaskStatus::Planning));
        for n in 0..5 { snapshot.tasks.push(task(&format!("acme-api-{n}"), "a", TaskStatus::Queued)); }
        snapshot.tasks.push(task("navi-0", "b", TaskStatus::Queued));
        snapshot.task_read_only.insert("acme-api-4".into());
        let active = ["running-1", "running-2", "scouting"];
        let candidates: Vec<usize> = (3..snapshot.tasks.len()).collect();
        let admitted: Vec<&str> = cap_writable_per_repository(&snapshot, &active, candidates.clone())
            .into_iter().map(|i| snapshot.tasks[i].id.as_str()).collect();
        assert_eq!(admitted, ["acme-api-0", "acme-api-1", "acme-api-4", "navi-0"], "two more writers fit in acme-api; a read-only ask and another repository are not held back");
        snapshot.max_writable_per_repository = Some(1);
        let admitted: Vec<&str> = cap_writable_per_repository(&snapshot, &active, candidates)
            .into_iter().map(|i| snapshot.tasks[i].id.as_str()).collect();
        assert_eq!(admitted, ["acme-api-4", "navi-0"]);
    }

    // ---- the gate ----------------------------------------------------------

    fn health(holding: Option<&str>, cap: Option<u32>) -> MachineHealth {
        MachineHealth { holding: holding.map(str::to_owned), cap, ..Default::default() }
    }

    /// A database whose daily ceiling is one dollar and whose ledger already holds two dollars today.
    fn over_the_ceiling() -> Db {
        let db = Db::open_in_memory().unwrap();
        let mut snapshot = neko_core::workbench::load(&db).unwrap();
        snapshot.daily_budget_cents = Some(100);
        neko_core::workbench::save(&db, &snapshot).unwrap();
        let now = neko_core::workbench::now_ms();
        neko_core::usage_ledger::record_cat_turn(&db, "c", now - 5_000, now - 4_000, "claude", "sonnet", 2_000_000);
        db
    }

    fn quiet_settings(db: &Db, around: i64) {
        let hour = neko_core::machine::local_hour(around);
        let settings = neko_protocol::phone::PhoneSettings { quiet_enabled: true, quiet_from_hour: hour, quiet_to_hour: (hour + 1) % 24, ..Default::default() };
        neko_core::phone::save_settings(db, settings).unwrap();
    }

    fn no_quiet_hours(db: &Db) {
        neko_core::phone::save_settings(db, neko_protocol::phone::PhoneSettings { quiet_enabled: false, ..Default::default() }).unwrap();
    }

    #[test]
    fn every_kind_is_refused_while_the_steward_holds() {
        let db = Db::open_in_memory().unwrap();
        no_quiet_hours(&db);
        let holding = health(Some("free disk is under 15 GB"), None);
        for kind in StartKind::ALL {
            for person in [false, true] {
                let verdict = may_start(&db, Start::new(kind, Some("w")).by_person(person).with_health(Some(&holding)));
                assert_eq!(verdict, Verdict::Hold(HoldReason::Machine("free disk is under 15 GB".into())), "{kind:?} (person: {person})");
                assert_eq!(verdict.line().as_deref(), Some("Held: free disk is under 15 GB"));
            }
            assert!(may_start(&db, Start::new(kind, Some("w")).with_health(Some(&health(None, None)))).admitted(), "{kind:?} starts when nothing holds");
            assert!(may_start(&db, Start::new(kind, None).with_health(None)).admitted(), "{kind:?} starts with no steward reading");
        }
    }

    #[test]
    fn every_kind_is_refused_at_the_steward_cap_and_admitted_below_it() {
        let db = Db::open_in_memory().unwrap();
        no_quiet_hours(&db);
        let capped = health(None, Some(2));
        for kind in StartKind::ALL {
            assert!(may_start(&db, Start::new(kind, None).running(1).with_health(Some(&capped))).admitted(), "{kind:?} fits under a cap of two with one running");
            let verdict = may_start(&db, Start::new(kind, None).running(2).with_health(Some(&capped)));
            assert_eq!(verdict, Verdict::Hold(HoldReason::Cap(2)), "{kind:?}");
            assert_eq!(verdict.line().as_deref(), Some("Held: the Mac allows 2 agents at once right now, and that many are running"));
            assert!(!may_start(&db, Start::new(kind, None).by_person(true).running(3).with_health(Some(&capped))).admitted(), "{kind:?}: the person's own start obeys the cap too");
        }
    }

    #[test]
    fn every_kind_is_refused_over_the_daily_spend_ceiling_and_a_new_day_clears_it() {
        let db = over_the_ceiling();
        no_quiet_hours(&db);
        for kind in StartKind::ALL {
            for person in [false, true] {
                assert_eq!(may_start(&db, Start::new(kind, None).by_person(person).with_health(None)), Verdict::Hold(HoldReason::Spend), "{kind:?} (person: {person})");
            }
        }
        // The same ledger a day later is yesterday's spend.
        let tomorrow = neko_core::workbench::now_ms() + 2 * 86_400_000;
        assert!(may_start(&db, Start::new(StartKind::Routine, None).at(tomorrow).with_health(None)).admitted());
        // Lifting the ceiling admits every kind again.
        let mut snapshot = neko_core::workbench::load(&db).unwrap();
        snapshot.daily_budget_cents = None;
        neko_core::workbench::save(&db, &snapshot).unwrap();
        for kind in StartKind::ALL {
            assert!(may_start(&db, Start::new(kind, None).with_health(None)).admitted(), "{kind:?}");
        }
    }

    #[test]
    fn proactive_kinds_wait_out_quiet_hours_and_starts_the_person_asked_for_do_not() {
        let db = Db::open_in_memory().unwrap();
        let now = neko_core::workbench::now_ms();
        quiet_settings(&db, now);
        let proactive: Vec<_> = StartKind::ALL.into_iter().filter(|k| k.proactive()).collect();
        assert_eq!(proactive, [StartKind::Routine, StartKind::WatchWake, StartKind::CatTurn, StartKind::SkillCurator]);
        for kind in StartKind::ALL {
            let unattended = may_start(&db, Start::new(kind, None).at(now).with_health(None));
            let asked = may_start(&db, Start::new(kind, None).by_person(true).at(now).with_health(None));
            if kind.proactive() {
                assert_eq!(unattended, Verdict::Hold(HoldReason::Quiet), "{kind:?} waits for quiet hours to end");
                assert_eq!(unattended.line().as_deref(), Some("Held: quiet hours; it starts when they end"));
            } else {
                assert!(unattended.admitted(), "{kind:?} is not proactive");
            }
            assert!(asked.admitted(), "{kind:?} the person asked for skips quiet hours");
        }
        // Out of quiet hours, or with them off, everything starts.
        assert!(may_start(&db, Start::new(StartKind::Routine, None).at(now + 3 * 3_600_000).with_health(None)).admitted());
        no_quiet_hours(&db);
        assert!(may_start(&db, Start::new(StartKind::WatchWake, None).at(now).with_health(None)).admitted());
    }

    #[test]
    fn the_checks_run_in_order_steward_then_cap_then_spend_then_quiet() {
        let all = |holding: bool, cap: bool, spend: bool, quiet: bool| {
            let health = health(holding.then_some("hot"), cap.then_some(1));
            decide(StartKind::Routine, false, Some(&health), 1, || spend, || quiet)
        };
        assert_eq!(all(true, true, true, true), Verdict::Hold(HoldReason::Machine("hot".into())));
        assert_eq!(all(false, true, true, true), Verdict::Hold(HoldReason::Cap(1)));
        assert_eq!(all(false, false, true, true), Verdict::Hold(HoldReason::Spend));
        assert_eq!(all(false, false, false, true), Verdict::Hold(HoldReason::Quiet));
        assert_eq!(all(false, false, false, false), Verdict::Admit);
        // Later checks are not even evaluated once an earlier one holds (the spend check walks the ledger).
        let verdict = decide(StartKind::Task, false, Some(&health(Some("hot"), None)), 0, || panic!("spend was read"), || panic!("quiet was read"));
        assert_eq!(verdict, Verdict::Hold(HoldReason::Machine("hot".into())));
    }

    #[test]
    fn a_held_task_gets_one_held_line_per_hold_period() {
        let mut task = crate::workbench::tests::controller_task();
        let before = task.events.len();
        note_held_on_task(&mut task, "Held: today's spend has reached the daily ceiling");
        note_held_on_task(&mut task, "Held: today's spend has reached the daily ceiling");
        note_held_on_task(&mut task, "Held: the Mac allows 1 agent at once right now, and that many are running");
        assert_eq!(task.events.len(), before + 1, "later ticks of the same hold add nothing");
        assert_eq!((task.events.last().unwrap().role.as_str(), task.events.last().unwrap().message.as_str()), (HELD_ROLE, "Held: today's spend has reached the daily ceiling"));
        // After something else happened, a new hold is a new period.
        neko_core::workbench::append_event(&mut task, "builder", "Started");
        note_held_on_task(&mut task, "Held: quiet hours; it starts when they end");
        assert_eq!(task.events.len(), before + 3);
    }

    #[test]
    fn a_held_line_is_not_a_result() {
        assert_eq!(without_held("Held: disk is low\nFound two bugs"), "Found two bugs");
        assert_eq!(without_held("Held: disk is low"), "");
        assert_eq!(without_held("Found two bugs"), "Found two bugs");
    }

    #[test]
    fn a_background_start_with_no_item_logs_one_line_per_hold_period() {
        let db = Db::open_in_memory().unwrap();
        no_quiet_hours(&db);
        let site = "test site";
        let holds = || background_holds().lock().unwrap().contains(site);
        // `Start` reads the real steward by default, so the hold here is the ceiling, which lives in this database.
        assert!(admit_background(&db, site, Start::new(StartKind::Naming, None).with_health(None)));
        assert!(!holds());
        let over = over_the_ceiling();
        no_quiet_hours(&over);
        assert!(!admit_background(&over, site, Start::new(StartKind::Naming, None).with_health(None)));
        assert!(holds(), "the first held pass is logged and remembered");
        assert!(!admit_background(&over, site, Start::new(StartKind::Naming, None).with_health(None)));
        assert!(admit_background(&db, site, Start::new(StartKind::Naming, None).with_health(None)));
        assert!(!holds(), "admitted again: the next hold is a new period");
    }

    /// How each place that can start an agent reaches the gate, by file. A file with
    /// an agent spawn that is not listed here fails the test below until it is, which is
    /// the point: the way to add a new start is to route it through `may_start`.
    enum Gate {
        /// The file asks the gate for these kinds itself.
        Here(&'static [StartKind]),
        /// A run inside a task worker the scheduler already admitted (`StartKind::Task`).
        Within(&'static str),
        /// Reached only through a gated function in another file.
        Caller(&'static str),
        /// Not an agent Neko starts.
        Exempt(&'static str),
    }

    const SITES: &[(&str, Gate)] = &[
        ("neko-daemon/src/workbench.rs", Gate::Here(&[StartKind::Task, StartKind::SmallModel])),
        ("neko-daemon/src/workbench/goals.rs", Gate::Here(&[StartKind::GoalTurn, StartKind::Judge])),
        ("neko-daemon/src/workbench/cats.rs", Gate::Here(&[StartKind::CatTurn])),
        ("neko-daemon/src/workbench/responsibilities.rs", Gate::Here(&[StartKind::WatchWake])),
        ("neko-daemon/src/routines.rs", Gate::Here(&[StartKind::Routine])),
        ("neko-daemon/src/workbench/skill_curator.rs", Gate::Here(&[StartKind::SkillCurator])),
        ("neko-daemon/src/improve.rs", Gate::Here(&[StartKind::SkillCurator])),
        ("neko-daemon/src/workbench/triage.rs", Gate::Here(&[StartKind::Triage])),
        ("neko-daemon/src/workbench/display_names.rs", Gate::Here(&[StartKind::Naming])),
        ("neko-daemon/src/workbench/memory_learning.rs", Gate::Here(&[StartKind::SmallModel])),
        ("neko-daemon/src/workbench/checkpoints.rs", Gate::Here(&[StartKind::SmallModel])),
        ("neko-daemon/src/workbench/review_repair.rs", Gate::Within("repair runs of a task worker")),
        ("neko-daemon/src/workbench/publish.rs", Gate::Within("conflict repair of a task being published")),
        ("neko-daemon/src/codex.rs", Gate::Exempt("the Codex desktop app-server mirror (legacy, NEKO_LEGACY_AGENTS) reads Codex's own sessions; Neko starts no agent through it")),
        ("neko-core/src/apps.rs", Gate::Exempt("the Spotlight app index (mdfind -live) is a search helper, not an agent")),
        ("neko-core/src/agent_catalog.rs", Gate::Caller("Controller::check_model (SmallModel, by the person)")),
        ("neko-core/src/runtime_selection.rs", Gate::Within("the model pick of a task worker or of a Home chat turn")),
        ("neko-core/src/supervision.rs", Gate::Within("the reply classifier of a task worker")),
    ];

    /// Source files (relative to `crates/`) outside the runner itself.
    fn sources() -> Vec<(String, String)> {
        fn walk(root: &std::path::Path, dir: &std::path::Path, out: &mut Vec<(String, String)>) {
            for entry in std::fs::read_dir(dir).unwrap().flatten() {
                let path = entry.path();
                let relative = path.strip_prefix(root).unwrap().to_string_lossy().into_owned();
                if path.is_dir() {
                    if !relative.contains("native_runner") && !relative.ends_with("/bin") {
                        walk(root, &path, out);
                    }
                } else if relative.ends_with(".rs") && !relative.contains("native_runner") && !relative.ends_with("_tests.rs") {
                    out.push((relative, std::fs::read_to_string(&path).unwrap()));
                }
            }
        }
        let crates = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_owned();
        let mut out = Vec::new();
        for krate in ["neko-daemon", "neko-core"] {
            walk(&crates, &crates.join(krate).join("src"), &mut out);
        }
        out
    }

    /// The part of a file that is not its test module.
    fn production(text: &str) -> &str {
        text.split("#[cfg(test)]\nmod ").next().unwrap_or(text)
    }

    #[test]
    fn no_agent_start_bypasses_may_start() {
        let sources = sources();
        // 1. Every file that builds a run or spawns a guarded child is in the table.
        let spawners: Vec<&str> = sources
            .iter()
            .filter(|(path, text)| !path.ends_with("admission.rs") && { let code = production(text); code.contains("RunSpec {") || code.contains("spawn_guarded(") })
            .map(|(path, _)| path.as_str())
            .collect();
        for path in &spawners {
            assert!(SITES.iter().any(|(site, _)| site == path), "{path} starts an agent and is not routed through admission::may_start; add it to SITES with how it is gated");
        }
        // 2. A file that gates itself really asks the gate for each kind it claims.
        for (path, gate) in SITES {
            let (_, text) = sources.iter().find(|(p, _)| p == path).unwrap_or_else(|| panic!("{path} is listed but does not exist"));
            let code = production(text);
            match gate {
                Gate::Here(kinds) => {
                    assert!(code.contains("may_start(") || code.contains("admit_background("), "{path} lists a gate but never calls it");
                    for kind in *kinds {
                        assert!(code.contains(&format!("StartKind::{kind:?}")), "{path} never asks the gate about {kind:?}");
                    }
                }
                Gate::Within(why) | Gate::Caller(why) | Gate::Exempt(why) => assert!(!why.is_empty()),
            }
        }
        // 3. Every kind of start has a site that asks about it.
        for kind in StartKind::ALL {
            assert!(SITES.iter().any(|(_, gate)| matches!(gate, Gate::Here(kinds) if kinds.contains(&kind))), "nothing asks the gate about {kind:?}");
        }
        // 4. The old per-site checks are gone: the spend ceiling is read only by the gate, and the cat tick has
        //    no steward rule of its own.
        for (path, text) in &sources {
            if path.ends_with("admission.rs") || path.ends_with("routine_care.rs") || path.ends_with("morning_digest.rs") {
                continue;
            }
            assert!(!production(text).contains("spend_held("), "{path} reads the spend ceiling by itself instead of asking admission::may_start");
        }
        assert!(!production(&sources.iter().find(|(p, _)| p == "neko-daemon/src/workbench/cats.rs").unwrap().1).contains("steward_allows"));
    }

    #[test]
    fn reads_a_real_load_average() {
        assert!(load_average().is_some_and(|load| load >= 0.0));
    }
}
