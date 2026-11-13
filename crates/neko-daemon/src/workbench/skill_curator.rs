//! The once-a-day skill curator pass (`neko_core::skills::curator`): runs only
//! when the Mac is idle (the machine steward is not holding work and the shared
//! `machine-ok` check agrees), no goal is active and no ticket has a worker. One
//! cheap-model call per skill a lesson touched, at most `PER_PASS`, never while
//! the database is held. A pass that cannot start is retried on the next wake,
//! not skipped for a day; one that ran (even with nothing to do) waits a day.
use super::*;
use neko_core::skills::curator::{self, Outcome};

/// How often the thread looks at whether a pass is due and the Mac is idle.
const WAKE: Duration = Duration::from_secs(10 * 60);
const ASK_TIMEOUT: Duration = Duration::from_secs(120);

impl Controller {
    pub(super) fn start_skill_curator(self: &Arc<Self>) {
        let controller = self.clone();
        std::thread::Builder::new()
            .name("neko-skill-curator".into())
            .spawn(move || loop {
                std::thread::sleep(WAKE);
                match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| controller.curator_tick(store::now_ms()))) {
                    Ok(Err(error)) => eprintln!("neko skills: curator: {error}"),
                    Err(_) => eprintln!("neko skills: curator panicked; continuing"),
                    Ok(Ok(_)) => {}
                }
            })
            .map(|_| ())
            .unwrap_or_else(|error| eprintln!("neko skills: cannot start the curator: {error}"));
    }

    /// Whether the Mac is idle enough: nothing held, no goal active, no worker running.
    fn idle_for_curator(&self) -> Result<bool, String> {
        let machine_ok = neko_core::machine::latest().is_some_and(|h| h.holding.is_none() && h.comfort_wait.is_none());
        if !machine_ok || !self.active.lock().unwrap_or_else(std::sync::PoisonError::into_inner).is_empty() {
            return Ok(false);
        }
        let db = self.db.lock().map_err(|_| "Skill storage unavailable")?;
        Ok(!neko_core::goals::load_all(&db)?.iter().any(|g| g.status.is_active()))
    }

    /// One look: returns how many skills were consolidated or proposed (0 when it did nothing).
    pub(super) fn curator_tick(&self, now_ms: i64) -> Result<usize, String> {
        {
            let db = self.db.lock().map_err(|_| "Skill storage unavailable")?;
            if !curator::due(&db, now_ms) {
                return Ok(0);
            }
        }
        if !self.idle_for_curator()? {
            return Ok(0);
        }
        self.curate(now_ms, &|prompt| ask(&self.db, prompt))
    }

    /// The pass itself, with the model behind `ask` so tests need none.
    pub(super) fn curate(&self, now_ms: i64, ask: &dyn Fn(&str) -> Result<String, String>) -> Result<usize, String> {
        let candidates = {
            let db = self.db.lock().map_err(|_| "Skill storage unavailable")?;
            let snapshot = store::load(&db)?;
            neko_core::skills::refresh(&db, &snapshot.workspaces)?;
            curator::candidates(&db, &neko_core::skills::load(&db)?, curator::PER_PASS)
        };
        let date = chrono::DateTime::from_timestamp_millis(now_ms).map_or_else(String::new, |t| t.format("%Y-%m-%d").to_string());
        let mut done = 0;
        for candidate in &candidates {
            let prepared = match curator::prepare(candidate) {
                Ok(prepared) => prepared,
                Err(error) => {
                    eprintln!("neko skills: curator: {}: {error}", candidate.skill.name);
                    continue;
                }
            };
            // The model is asked with no lock held.
            let answer = match ask(&prepared.prompt) {
                Ok(answer) => answer,
                Err(error) => {
                    // No model right now: try again on a later wake, not tomorrow.
                    eprintln!("neko skills: curator: no answer for {}: {error}", prepared.skill.name);
                    return Ok(done);
                }
            };
            let db = self.db.lock().map_err(|_| "Skill storage unavailable")?;
            match curator::finish(&db, &neko_protocol::support_dir(), &prepared, &answer, now_ms, &date) {
                Ok(Outcome::Unchanged) => {}
                Ok(_) => done += 1,
                Err(error) => eprintln!("neko skills: curator: {}: {error}", prepared.skill.name),
            }
        }
        curator::note_run(&*self.db.lock().map_err(|_| "Skill storage unavailable")?, now_ms)?;
        Ok(done)
    }
}

/// One tool-free cheap-model call (Haiku, else a Codex mini model), as the goal judge makes it.
fn ask(db: &Mutex<Db>, prompt: &str) -> Result<String, String> {
    let claude_ok = !neko_core::runtime_health::load(&db.lock().unwrap_or_else(std::sync::PoisonError::into_inner)).is_bad();
    let mut last = String::from("No model is available");
    for runtime in super::goals::judge_runtimes(claude_ok) {
        let scratch = std::env::temp_dir().join(format!("neko-curator-{}", store::new_id()));
        let result = (|| {
            std::fs::create_dir(&scratch).map_err(|e| format!("Cannot create the curator's scratch folder: {e}"))?;
            native_runner::extract(&native_runner::RunSpec { directory: scratch.clone(), prompt: prompt.to_owned(), writable: false, timeout: ASK_TIMEOUT, runtime }, &AtomicBool::new(false))
        })();
        let _ = std::fs::remove_dir(&scratch);
        match result {
            Ok(text) => return Ok(text),
            Err(error) => last = error,
        }
    }
    Err(last)
}
