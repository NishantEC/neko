//! Captain turns: the tick that turns pending wakes into one agent run per captain.
//!
//! A captain's thread and its wake queue are `neko_core::captain_thread`; this is the part that runs. Every
//! few seconds one pass (1) notices what each captain's sessions did and queues the wakes, (2) takes a captain's
//! pending wakes in order and coalesces them into ONE turn, (3) starts that turn on its own thread when the rules
//! allow: at most one turn per captain, the machine steward not holding, the daily spend ceiling not reached,
//! and for an unattended turn the debounce, the gap and the day cap. A captain turn is not a ticket worker (it
//! takes no admission slot) and it is not the Home chat (it never touches `neko_chat_v2`). The agent directs
//! sessions through the crew tools on the MCP bridge, under a lease whose role is `CrewRole::Captain`.
//! See `docs/knowledge/captains.md`.
use super::*;
use neko_core::captain_prompt::{self, PromptInput, QUIET};
use neko_core::captain_thread as thread;
use neko_core::{captain_files, captains};
use neko_protocol::captains::{Captain, CaptainFileKind, CaptainMessageRole};
use neko_protocol::crew::CrewRole;
use neko_protocol::workbench::MachineHealth;

const TICK: Duration = Duration::from_secs(3);
/// A captain turn ends after this long, like a routine's long run.
const TURN_TIMEOUT: Duration = Duration::from_secs(20 * 60);

/// One turn the plan decided to start. The captain's slot in `captain_active` is already taken.
pub(crate) struct Launch {
    pub captain: Captain,
    pub batch: thread::Batch,
    pub cancel: Arc<AtomicBool>,
}

/// What the runner is asked to do.
pub(crate) struct TurnRequest<'a> {
    pub captain: &'a Captain,
    pub prompt: String,
    pub directory: std::path::PathBuf,
    pub runtime: AgentRuntime,
    /// The lease's run id: `captain:<id>:<turn>`.
    pub run_id: String,
    pub person_turn: bool,
    pub cancel: &'a AtomicBool,
}

pub(crate) struct TurnResult {
    pub text: String,
    /// What the agent reported, in micro-dollars (0 when nothing).
    pub cost_micro_usd: u64,
}

/// Runs one captain turn's agent. The real one is `run_agent`; tests pass a fake.
pub(crate) type Runner<'a> = &'a (dyn Fn(&TurnRequest) -> Result<TurnResult, String> + Sync);

/// The machine steward's say: nothing starts while it holds, and under a cap only while fewer agents run than it allows.
pub(crate) fn steward_allows(health: Option<&MachineHealth>, running: usize) -> bool {
    health.is_none_or(|h| h.holding.is_none() && h.cap.is_none_or(|cap| running < cap as usize))
}

/// Frees a captain's slot and tells readers its turn is over, however the turn ended.
struct Slot<'a> {
    controller: &'a Controller,
    id: String,
}

impl Drop for Slot<'_> {
    fn drop(&mut self) {
        self.controller.captain_active.lock().unwrap_or_else(std::sync::PoisonError::into_inner).remove(&self.id);
        let db = self.controller.db.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let _ = thread::bump_rev(&db);
    }
}

impl Controller {
    /// Where captain files live: the data folder unless a test says otherwise.
    pub(crate) fn captain_root(&self) -> std::path::PathBuf {
        self.captain_root.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone().unwrap_or_else(neko_protocol::support_dir)
    }

    #[cfg(test)]
    pub(crate) fn set_captain_root(&self, root: &std::path::Path) {
        *self.captain_root.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(root.to_path_buf());
    }

    /// A turn of this captain is running now.
    pub fn captain_running(&self, id: &str) -> bool {
        self.captain_active.lock().unwrap_or_else(std::sync::PoisonError::into_inner).contains_key(id)
    }

    /// Agents running now: ticket workers, the Home chat and captain turns. What a steward cap counts.
    fn running_agents(&self) -> usize {
        self.active.lock().unwrap_or_else(std::sync::PoisonError::into_inner).len()
            + usize::from(self.chat_active.lock().unwrap_or_else(std::sync::PoisonError::into_inner).is_some())
            + self.captain_active.lock().unwrap_or_else(std::sync::PoisonError::into_inner).len()
    }

    /// The captains' thread: a pass every few seconds. One bad pass never ends it.
    pub(super) fn start_captains(self: &Arc<Self>) {
        let controller = self.clone();
        std::thread::Builder::new()
            .name("neko-captains".into())
            .spawn(move || loop {
                match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| controller.captains_pass())) {
                    Ok(Err(error)) => eprintln!("neko captains: {error}"),
                    Err(_) => eprintln!("neko captains: pass panicked; continuing"),
                    Ok(Ok(())) => {}
                }
                std::thread::sleep(TICK);
            })
            .map(|_| ())
            .unwrap_or_else(|error| eprintln!("neko captains: cannot start: {error}"));
    }

    /// One pass for real: plan, then run each turn on its own thread.
    fn captains_pass(self: &Arc<Self>) -> Result<(), String> {
        let health = neko_core::machine::latest();
        for launch in self.captain_plan(store::now_ms(), health.as_ref())? {
            let controller = self.clone();
            let name = format!("neko-captain-{}", launch.captain.name);
            let spawned = std::thread::Builder::new().name(name).spawn(move || {
                let runner = |request: &TurnRequest| run_agent(&controller.db, &controller.mcp, request);
                controller.captain_turn(launch, &runner);
            });
            if let Err(error) = spawned {
                eprintln!("neko captains: cannot start a turn: {error}");
            }
        }
        Ok(())
    }

    /// Notices what sessions did, then decides which captains start a turn now and takes their slots.
    /// Wakes stay queued for a captain that may not launch yet; an archived captain's are dropped.
    pub(crate) fn captain_plan(&self, now_ms: i64, health: Option<&MachineHealth>) -> Result<Vec<Launch>, String> {
        let db = self.db.lock().map_err(|_| "Captain storage unavailable")?;
        let all = captains::list(&db, true)?;
        if all.is_empty() {
            return Ok(Vec::new());
        }
        let snapshot = store::load(&db)?;
        // Read only when a turn is about to start: it walks the routines' runs, too much for an idle pass.
        let spend_held = |db: &Db| neko_core::spend::today(db, &snapshot, now_ms).reached();
        let mut launches = Vec::new();
        for captain in all {
            if captain.archived {
                thread::drop_all(&db, &captain.id)?;
                continue;
            }
            thread::notice_sessions(&db, &captain.id, &snapshot.tasks, &snapshot.goals, now_ms)?;
            let Some(batch) = thread::coalesce(&thread::pending(&db, &captain.id)?) else { continue };
            // Never two turns at once for one captain.
            if self.captain_running(&captain.id) {
                continue;
            }
            let (last_turn, turns_today) = thread::turn_stats(&db, &captain.id, now_ms)?;
            if thread::launch_gate(&batch, now_ms, last_turn, turns_today).is_err() || !steward_allows(health, self.running_agents()) || spend_held(&db) {
                continue;
            }
            let cancel = Arc::new(AtomicBool::new(false));
            self.captain_active.lock().unwrap_or_else(std::sync::PoisonError::into_inner).insert(captain.id.clone(), cancel.clone());
            launches.push(Launch { captain, batch, cancel });
        }
        Ok(launches)
    }

    /// One captain turn: take the wakes, build the prompt, run the agent, store the reply, record the usage.
    /// Always frees the captain's slot.
    pub(crate) fn captain_turn(&self, launch: Launch, runner: Runner) {
        let Launch { captain, batch, cancel } = launch;
        let _slot = Slot { controller: self, id: captain.id.clone() };
        if let Err(error) = self.captain_turn_inner(&captain, &batch, &cancel, runner) {
            eprintln!("neko captains: {}: {error}", captain.name);
            // The person should not be left wondering why nothing answered.
            let db = self.db.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            let short: String = error.chars().take(300).collect();
            let _ = thread::append(&db, &captain.id, CaptainMessageRole::Event, &format!("I could not take this turn: {short}"), store::now_ms());
        }
    }

    fn captain_turn_inner(&self, captain: &Captain, batch: &thread::Batch, cancel: &AtomicBool, runner: Runner) -> Result<(), String> {
        let root = self.captain_root();
        let started = store::now_ms();
        let turn_id = store::new_id();
        let (request, runtime) = {
            let db = self.db.lock().map_err(|_| "Captain storage unavailable")?;
            let snapshot = store::load(&db)?;
            let workspace = snapshot.workspaces.iter().find(|w| w.id == captain.workspace_id).ok_or("This captain's workspace no longer exists")?;
            let history = thread::recent(&db, &captain.id, thread::PROMPT_HISTORY)?;
            let sessions = captain_prompt::sessions_of(&captain.id, &snapshot.tasks);
            let events = captain_prompt::wake_text(&batch.events, &snapshot.tasks);
            let body = |kind| captain_files::prompt_body(&root, &captain.id, kind);
            let (rules, plan, crew, digest) = (body(CaptainFileKind::Rules), body(CaptainFileKind::Plan), body(CaptainFileKind::Crew), body(CaptainFileKind::Digest));
            let prompt = captain_prompt::prompt(&PromptInput {
                captain,
                workspace_name: &workspace.name,
                owner: &neko_core::routines::owner_name(),
                now_iso: &neko_core::routines::now_iso(),
                rules: &rules,
                plan: &plan,
                crew: &crew,
                digest: &digest,
                history: &history,
                sessions: &sessions,
                events: &events,
                person_turn: batch.person,
            });
            // The captain's own model choice, else the stored default runtime.
            let runtime = neko_core::runtime_selection::preview(&neko_core::agent_catalog::catalog(false), &captain.model_policy.preferences, &snapshot.agent_runtime).unwrap_or_else(|_| snapshot.agent_runtime.clone());
            let request = (prompt, std::path::PathBuf::from(&workspace.repository));
            // The wakes this turn took leave the queue now; wakes written while it runs wait for the next turn.
            thread::take(&db, &captain.id, batch)?;
            thread::bump_rev(&db)?;
            (request, runtime)
        };
        let outcome = runner(&TurnRequest {
            captain,
            prompt: request.0,
            directory: request.1,
            runtime: runtime.clone(),
            run_id: format!("captain:{}:{turn_id}", captain.id),
            person_turn: batch.person,
            cancel,
        });
        let finished = store::now_ms();
        let db = self.db.lock().map_err(|_| "Captain storage unavailable")?;
        // One ledger line per turn that ran an agent (R3): `task_id = "captain:<id>"`, whether it went well or not.
        let cost = outcome.as_ref().map_or(0, |r| r.cost_micro_usd);
        let model = if runtime.provider == "claude" { runtime.model.as_str() } else { "" };
        neko_core::usage_ledger::record_captain_turn(&db, &captain.id, started, finished, &runtime.provider, model, cost);
        if cancel.load(Ordering::Acquire) {
            return Ok(());
        }
        let result = outcome?;
        let reply = result.text.trim();
        // An unattended turn with nothing to say stores nothing; the person is never left without an answer.
        if reply == QUIET || (reply.is_empty() && !batch.person) {
            return Ok(());
        }
        let reply = if reply.is_empty() { "I looked, and there is nothing to add." } else { reply };
        thread::append(&db, &captain.id, CaptainMessageRole::Captain, reply, finished)?;
        Ok(())
    }
}

/// The real runner: the agent runs in the captain's repository, read-only, with the crew tools of its lease and
/// no connection tools (it directs sessions; its sessions use tools).
fn run_agent(db: &Arc<Mutex<Db>>, mcp: &Arc<crate::mcp_host::Host>, request: &TurnRequest) -> Result<TurnResult, String> {
    let revision = store::load(&*db.lock().map_err(|_| "Captain storage unavailable")?)?.agent_profiles.revision;
    let role = CrewRole::Captain { captain_id: request.captain.id.clone(), person_turn: request.person_turn };
    let lease = mcp.lease(&request.run_id, &request.captain.workspace_id, Vec::new(), revision)?;
    mcp.grant_crew(&lease, role);
    let bridge = native_runner::BridgeConfig {
        executable: std::env::current_exe().map_err(|e| e.to_string())?,
        socket: neko_protocol::socket_path(),
        token: lease.token().into(),
        sources_through_bridge: false,
    };
    let spec = native_runner::RunSpec { directory: request.directory.clone(), prompt: request.prompt.clone(), writable: false, timeout: TURN_TIMEOUT, runtime: request.runtime.clone() };
    let mut cost_usd = 0.0;
    let text = native_runner::run_with_bridge_streaming(&spec, &bridge, request.cancel, |event| {
        if let Some(cost) = neko_core::spend::event_cost_usd(&event) {
            cost_usd += cost;
        }
    })?;
    Ok(TurnResult { text, cost_micro_usd: neko_core::spend::micro_usd(cost_usd) })
}

#[cfg(test)]
#[path = "captains_tests.rs"]
mod tests;
