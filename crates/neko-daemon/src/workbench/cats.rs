//! Cat turns: the tick that turns pending wakes into one agent run per cat.
//!
//! A cat's thread and its wake queue are `neko_core::cat_thread`; this is the part that runs. Every
//! few seconds one pass (1) notices what each cat's sessions did and queues the wakes, (2) takes a cat's
//! pending wakes in order and coalesces them into ONE turn, (3) starts that turn on its own thread when the rules
//! allow: at most one turn per cat, the machine steward not holding, the daily spend ceiling not reached,
//! and for an unattended turn the debounce, the gap and the day cap. A cat turn is not a ticket worker (it
//! takes no admission slot) and it is not the Home chat (it never touches `neko_chat_v2`). The agent directs
//! sessions through the crew tools on the MCP bridge, under a lease whose role is `CrewRole::Cat`.
//! See `docs/knowledge/cats.md`.
use super::*;
use neko_core::cat_prompt::{self, PromptInput, QUIET};
use neko_core::cat_thread as thread;
use neko_core::{cat_files, cats};
use neko_protocol::cats::{Cat, CatFileKind, CatMessageRole};
use neko_protocol::crew::CrewRole;
use neko_protocol::workbench::MachineHealth;
use super::admission::{may_start, Start, StartKind, HELD};

const TICK: Duration = Duration::from_secs(3);
/// A cat turn ends after this long, like a routine's long run.
const TURN_TIMEOUT: Duration = Duration::from_secs(20 * 60);

/// One turn the plan decided to start. The cat's slot in `cat_active` is already taken.
pub(crate) struct Launch {
    pub cat: Cat,
    pub batch: thread::Batch,
    pub cancel: Arc<AtomicBool>,
}

/// What the runner is asked to do.
pub(crate) struct TurnRequest<'a> {
    pub cat: &'a Cat,
    pub prompt: String,
    pub directory: std::path::PathBuf,
    pub runtime: AgentRuntime,
    /// The lease's run id: `cat:<id>:<turn>`.
    pub run_id: String,
    pub person_turn: bool,
    pub cancel: &'a AtomicBool,
}

pub(crate) struct TurnResult {
    pub text: String,
    /// What the agent reported, in micro-dollars (0 when nothing).
    pub cost_micro_usd: u64,
}

/// Runs one cat turn's agent. The real one is `run_agent`; tests pass a fake.
pub(crate) type Runner<'a> = &'a (dyn Fn(&TurnRequest) -> Result<TurnResult, String> + Sync);

/// Frees a cat's slot and tells readers its turn is over, however the turn ended.
struct Slot<'a> {
    controller: &'a Controller,
    id: String,
}

impl Drop for Slot<'_> {
    fn drop(&mut self) {
        self.controller.cat_active.lock().unwrap_or_else(std::sync::PoisonError::into_inner).remove(&self.id);
        let db = self.controller.db.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let _ = thread::bump_rev(&db);
    }
}

impl Controller {
    /// Where cat files live: the data folder unless a test says otherwise.
    pub(crate) fn cat_root(&self) -> std::path::PathBuf {
        self.cat_root.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone().unwrap_or_else(neko_protocol::support_dir)
    }

    #[cfg(test)]
    pub(crate) fn set_cat_root(&self, root: &std::path::Path) {
        *self.cat_root.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(root.to_path_buf());
    }

    /// A turn of this cat is running now.
    pub fn cat_running(&self, id: &str) -> bool {
        self.cat_active.lock().unwrap_or_else(std::sync::PoisonError::into_inner).contains_key(id)
    }

    /// Agents running now: ticket workers, the Home chat and cat turns. What a steward cap counts.
    // TODO(H3): routine, watch, curator, judge and small-model runs are not counted yet.
    pub(crate) fn running_agents(&self) -> usize {
        self.active.lock().unwrap_or_else(std::sync::PoisonError::into_inner).len() + self.others_running()
    }

    /// The Home chat and cat turns: `running_agents` without the ticket workers, for the
    /// scheduler, which already holds that lock.
    pub(crate) fn others_running(&self) -> usize {
        usize::from(self.chat_active.lock().unwrap_or_else(std::sync::PoisonError::into_inner).is_some())
            + self.cat_active.lock().unwrap_or_else(std::sync::PoisonError::into_inner).len()
    }

    /// The cats' thread: a pass every few seconds. One bad pass never ends it.
    pub(super) fn start_cats(self: &Arc<Self>) {
        let controller = self.clone();
        std::thread::Builder::new()
            .name("neko-cats".into())
            .spawn(move || loop {
                match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| controller.cats_pass())) {
                    Ok(Err(error)) => eprintln!("neko cats: {error}"),
                    Err(_) => eprintln!("neko cats: pass panicked; continuing"),
                    Ok(Ok(())) => {}
                }
                std::thread::sleep(TICK);
            })
            .map(|_| ())
            .unwrap_or_else(|error| eprintln!("neko cats: cannot start: {error}"));
    }

    /// One pass for real: plan, then run each turn on its own thread.
    fn cats_pass(self: &Arc<Self>) -> Result<(), String> {
        let health = neko_core::machine::latest();
        for launch in self.cat_plan(store::now_ms(), health.as_ref())? {
            let controller = self.clone();
            let name = format!("neko-cat-{}", launch.cat.name);
            let spawned = std::thread::Builder::new().name(name).spawn(move || {
                let runner = |request: &TurnRequest| run_agent(&controller.db, &controller.mcp, request);
                controller.cat_turn(launch, &runner);
            });
            if let Err(error) = spawned {
                eprintln!("neko cats: cannot start a turn: {error}");
            }
        }
        Ok(())
    }

    /// Notices what sessions did, then decides which cats start a turn now and takes their slots.
    /// Wakes stay queued for a cat that may not launch yet; an archived cat's are dropped.
    pub(crate) fn cat_plan(&self, now_ms: i64, health: Option<&MachineHealth>) -> Result<Vec<Launch>, String> {
        // Counted before the database lock, which the scheduler takes after its `active` lock; each turn launched below adds one.
        let running_before = self.running_agents();
        let db = self.db.lock().map_err(|_| "Cat storage unavailable")?;
        let all = cats::list(&db, true)?;
        if all.is_empty() {
            return Ok(Vec::new());
        }
        let snapshot = store::load(&db)?;
        let mut launches = Vec::new();
        for cat in all {
            if cat.archived {
                thread::drop_all(&db, &cat.id)?;
                continue;
            }
            thread::notice_sessions(&db, &cat.id, &snapshot.tasks, &snapshot.goals, now_ms)?;
            let Some(batch) = thread::coalesce(&thread::pending(&db, &cat.id)?) else { continue };
            // Never two turns at once for one cat.
            if self.cat_running(&cat.id) {
                continue;
            }
            let (last_turn, turns_today) = thread::turn_stats(&db, &cat.id, now_ms)?;
            if thread::launch_gate(&batch, now_ms, last_turn, turns_today).is_err() {
                continue;
            }
            // The one gate. Read only when a turn is about to start: the spend check walks the routines' runs.
            let start = Start::new(StartKind::CatTurn, Some(&cat.workspace_id)).by_person(batch.person).running(running_before + launches.len()).at(now_ms).with_health(health);
            if let Some(line) = may_start(&db, start).line() {
                // The wakes stay queued; one "Held:" line per hold period tells the person why nothing answered.
                let noted = thread::recent(&db, &cat.id, 1)?.last().is_some_and(|m| m.role == CatMessageRole::Event && m.text.starts_with(HELD));
                if !noted {
                    thread::append(&db, &cat.id, CatMessageRole::Event, &line, now_ms)?;
                }
                continue;
            }
            let cancel = Arc::new(AtomicBool::new(false));
            self.cat_active.lock().unwrap_or_else(std::sync::PoisonError::into_inner).insert(cat.id.clone(), cancel.clone());
            launches.push(Launch { cat, batch, cancel });
        }
        Ok(launches)
    }

    /// One cat turn: take the wakes, build the prompt, run the agent, store the reply, record the usage.
    /// Always frees the cat's slot.
    pub(crate) fn cat_turn(&self, launch: Launch, runner: Runner) {
        let Launch { cat, batch, cancel } = launch;
        let _slot = Slot { controller: self, id: cat.id.clone() };
        if let Err(error) = self.cat_turn_inner(&cat, &batch, &cancel, runner) {
            eprintln!("neko cats: {}: {error}", cat.name);
            // The person should not be left wondering why nothing answered.
            let db = self.db.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            let short: String = error.chars().take(300).collect();
            let _ = thread::append(&db, &cat.id, CatMessageRole::Event, &format!("I could not take this turn: {short}"), store::now_ms());
        }
    }

    fn cat_turn_inner(&self, cat: &Cat, batch: &thread::Batch, cancel: &AtomicBool, runner: Runner) -> Result<(), String> {
        let root = self.cat_root();
        let started = store::now_ms();
        let turn_id = store::new_id();
        let (request, runtime) = {
            let db = self.db.lock().map_err(|_| "Cat storage unavailable")?;
            let snapshot = store::load(&db)?;
            let workspace = snapshot.workspaces.iter().find(|w| w.id == cat.workspace_id).ok_or("This cat's workspace no longer exists")?;
            let history = thread::recent(&db, &cat.id, thread::PROMPT_HISTORY)?;
            let sessions = cat_prompt::sessions_of(&cat.id, &snapshot.tasks);
            let events = cat_prompt::wake_text(&batch.events, &snapshot.tasks);
            let body = |kind| cat_files::prompt_body(&root, &cat.id, kind);
            let (rules, plan, task_rules, briefing) = (body(CatFileKind::Rules), body(CatFileKind::Plan), body(CatFileKind::TaskRules), body(CatFileKind::Briefing));
            let prompt = cat_prompt::prompt(&PromptInput {
                cat,
                workspace_name: &workspace.name,
                owner: &neko_core::routines::owner_name(),
                now_iso: &neko_core::routines::now_iso(),
                rules: &rules,
                plan: &plan,
                task_rules: &task_rules,
                briefing: &briefing,
                history: &history,
                sessions: &sessions,
                events: &events,
                person_turn: batch.person,
            });
            // The cat's own model choice, else the stored default runtime.
            let runtime = neko_core::runtime_selection::preview(&neko_core::agent_catalog::catalog(false), &cat.model_policy.preferences, &snapshot.agent_runtime).unwrap_or_else(|_| snapshot.agent_runtime.clone());
            let request = (prompt, std::path::PathBuf::from(&workspace.repository));
            // The wakes this turn took leave the queue now; wakes written while it runs wait for the next turn.
            thread::take(&db, &cat.id, batch)?;
            thread::bump_rev(&db)?;
            (request, runtime)
        };
        let outcome = runner(&TurnRequest {
            cat,
            prompt: request.0,
            directory: request.1,
            runtime: runtime.clone(),
            run_id: format!("cat:{}:{turn_id}", cat.id),
            person_turn: batch.person,
            cancel,
        });
        let finished = store::now_ms();
        let db = self.db.lock().map_err(|_| "Cat storage unavailable")?;
        // One ledger line per turn that ran an agent (R3): `task_id = "cat:<id>"`, whether it went well or not.
        let cost = outcome.as_ref().map_or(0, |r| r.cost_micro_usd);
        let model = if runtime.provider == "claude" { runtime.model.as_str() } else { "" };
        neko_core::usage_ledger::record_cat_turn(&db, &cat.id, started, finished, &runtime.provider, model, cost);
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
        thread::append(&db, &cat.id, CatMessageRole::Cat, reply, finished)?;
        Ok(())
    }
}

/// The real runner: the agent runs in the cat's repository, read-only, with the crew tools of its lease and
/// no connection tools (it directs sessions; its sessions use tools).
fn run_agent(db: &Arc<Mutex<Db>>, mcp: &Arc<crate::mcp_host::Host>, request: &TurnRequest) -> Result<TurnResult, String> {
    let revision = store::load(&*db.lock().map_err(|_| "Cat storage unavailable")?)?.agent_profiles.revision;
    let role = CrewRole::Cat { cat_id: request.cat.id.clone(), person_turn: request.person_turn };
    let lease = mcp.lease(&request.run_id, &request.cat.workspace_id, Vec::new(), revision)?;
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
#[path = "cats_tests.rs"]
mod tests;
