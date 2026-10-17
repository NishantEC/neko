//! The thread that triages watch-filed tasks (`neko_core::triage`): one
//! tool-free model call each, off the request path and never holding the
//! database lock while the model runs.

use super::*;
use std::collections::HashMap;

const EVERY: Duration = Duration::from_secs(10);

impl Controller {
    pub(super) fn start_triage(self: &Arc<Self>) {
        let controller = self.clone();
        std::thread::Builder::new()
            .name("neko-triage".into())
            .spawn(move || {
                let mut attempts: HashMap<String, u32> = HashMap::new();
                loop {
                    std::thread::sleep(EVERY);
                    if admission::load_average().is_some_and(|load| load >= 20.0) {
                        continue;
                    }
                    let pass = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        controller.triage_one(&mut attempts, ask_model)
                    }));
                    match pass {
                        Ok(Err(error)) => eprintln!("neko triage: {error}"),
                        Err(_) => eprintln!("neko triage: pass panicked; continuing"),
                        Ok(Ok(())) => {}
                    }
                }
            })
            .expect("spawn triage thread");
    }

    /// Triages the oldest waiting task, or releases it as work when it has
    /// waited too long or the model keeps failing.
    pub(super) fn triage_one(
        &self,
        attempts: &mut HashMap<String, u32>,
        ask: impl Fn(&str, &AgentRuntime) -> Result<String, String>,
    ) -> Result<(), String> {
        let snapshot = store::load(&self.db.lock().unwrap_or_else(std::sync::PoisonError::into_inner))?;
        let Some(task_id) = snapshot.untriaged.first().cloned() else { return Ok(()) };
        let created = snapshot.tasks.iter().find(|t| t.id == task_id).map(|t| t.created_at_ms).unwrap_or(0);
        let tries = attempts.get(&task_id).copied().unwrap_or(0);
        let decision = if store::now_ms() - created > neko_core::triage::MAX_WAIT_MS || tries >= neko_core::triage::MAX_ATTEMPTS {
            None
        } else {
            let Some(prompt) = neko_core::triage::prompt(&snapshot, &task_id) else {
                return self.settle(&task_id, None, attempts);
            };
            match ask(&prompt, &snapshot.agent_runtime).map(|answer| neko_core::triage::parse(&answer)) {
                Ok(Some(decision)) => Some(decision),
                Ok(None) | Err(_) => {
                    *attempts.entry(task_id).or_default() += 1;
                    return Ok(());
                }
            }
        };
        self.settle(&task_id, decision, attempts)
    }

    fn settle(&self, task_id: &str, decision: Option<neko_core::triage::Decision>, attempts: &mut HashMap<String, u32>) -> Result<(), String> {
        attempts.remove(task_id);
        let db = self.db.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut snapshot = store::load(&db)?;
        if neko_core::triage::apply(&mut snapshot, task_id, decision) {
            store::save(&db, &snapshot)?;
        }
        Ok(())
    }
}

fn ask_model(prompt: &str, runtime: &AgentRuntime) -> Result<String, String> {
    let scratch = std::env::temp_dir().join(format!("neko-triage-{}", store::new_id()));
    let result = (|| {
        std::fs::create_dir(&scratch).map_err(|e| format!("Cannot create triage scratch: {e}"))?;
        native_runner::extract(
            &native_runner::RunSpec {
                directory: scratch.clone(),
                prompt: prompt.to_owned(),
                writable: false,
                timeout: Duration::from_secs(90),
                runtime: runtime.clone(),
            },
            &AtomicBool::new(false),
        )
    })();
    let _ = std::fs::remove_dir(&scratch);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_waiting_task_is_held_until_triaged_then_released() {
        let controller = super::super::tests::controller_with_task(TaskStatus::Queued);
        {
            let db = controller.db.lock().unwrap();
            let mut snapshot = store::load(&db).unwrap();
            snapshot.tasks[0].created_at_ms = store::now_ms();
            snapshot.untriaged = vec!["t".into()];
            store::save(&db, &snapshot).unwrap();
        }
        let mut attempts = HashMap::new();
        controller.triage_one(&mut attempts, |_, _| Ok("not json".into())).unwrap();
        assert_eq!(store::load(&controller.db.lock().unwrap()).unwrap().untriaged, vec!["t".to_owned()], "a bad answer is retried");
        controller.triage_one(&mut attempts, |_, _| Ok(r#"{"decision":"work","priority":"now","reason":"Checkout is down."}"#.into())).unwrap();
        let after = store::load(&controller.db.lock().unwrap()).unwrap();
        assert!(after.untriaged.is_empty());
        assert_eq!(after.run_next, vec!["t".to_owned()]);
    }

    #[test]
    fn a_task_that_keeps_failing_triage_goes_ahead_as_work() {
        let controller = super::super::tests::controller_with_task(TaskStatus::Queued);
        {
            let db = controller.db.lock().unwrap();
            let mut snapshot = store::load(&db).unwrap();
            snapshot.tasks[0].created_at_ms = store::now_ms();
            snapshot.untriaged = vec!["t".into()];
            store::save(&db, &snapshot).unwrap();
        }
        let mut attempts = HashMap::new();
        for _ in 0..=neko_core::triage::MAX_ATTEMPTS {
            controller.triage_one(&mut attempts, |_, _| Err("model unavailable".into())).unwrap();
        }
        let after = store::load(&controller.db.lock().unwrap()).unwrap();
        assert!(after.untriaged.is_empty());
        assert_eq!(after.tasks[0].status, TaskStatus::Queued);
    }
}
