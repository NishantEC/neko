//! Pacing for new agents. There is deliberately no fixed limit on how many
//! tickets work at once, but every agent is a full CLI session with its own
//! build and test processes, and many starting together has overloaded this
//! machine before (a load average above 240 restarted it). So new agents are
//! released from a small bucket that refills slowly, and none start while the
//! machine is already busy. Agents that are running are never affected.
//! Separately, at most `Snapshot::writable_per_repository` tickets (default
//! 4) per repository hold a writable run at once; read-only phases are not
//! counted.
use neko_protocol::workbench::{Snapshot, Task, TaskStatus};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

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
        let mut snapshot = Snapshot { workspaces: vec![workspace("a", "/athena"), workspace("b", "/navi")], max_writable_per_repository: Some(4), ..Default::default() };
        // Two already writing in athena, one planning (read-only) there.
        snapshot.tasks.push(task("running-1", "a", TaskStatus::Building));
        snapshot.tasks.push(task("running-2", "a", TaskStatus::Reviewing));
        snapshot.tasks.push(task("scouting", "a", TaskStatus::Planning));
        for n in 0..5 { snapshot.tasks.push(task(&format!("athena-{n}"), "a", TaskStatus::Queued)); }
        snapshot.tasks.push(task("navi-0", "b", TaskStatus::Queued));
        snapshot.task_read_only.insert("athena-4".into());
        let active = ["running-1", "running-2", "scouting"];
        let candidates: Vec<usize> = (3..snapshot.tasks.len()).collect();
        let admitted: Vec<&str> = cap_writable_per_repository(&snapshot, &active, candidates.clone())
            .into_iter().map(|i| snapshot.tasks[i].id.as_str()).collect();
        assert_eq!(admitted, ["athena-0", "athena-1", "athena-4", "navi-0"], "two more writers fit in athena; a read-only ask and another repository are not held back");
        snapshot.max_writable_per_repository = Some(1);
        let admitted: Vec<&str> = cap_writable_per_repository(&snapshot, &active, candidates)
            .into_iter().map(|i| snapshot.tasks[i].id.as_str()).collect();
        assert_eq!(admitted, ["athena-4", "navi-0"]);
    }

    #[test]
    fn reads_a_real_load_average() {
        assert!(load_average().is_some_and(|load| load >= 0.0));
    }
}
