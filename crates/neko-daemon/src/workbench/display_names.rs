//! Background naming of tickets and watches (`neko_core::display_names`).
//! Its own thread, so a slow or broken model never delays admission, replies
//! or requests. Model work never holds the database lock, and each answer is
//! committed only if the subject still needs a name and its input is unchanged.
use super::*;
use neko_core::display_names as names;
use std::collections::HashMap;

/// At most this many names are generated per pass.
const PER_TICK: usize = 3;
/// A pass with nothing to do, or with work done, waits this long.
const TICK: Duration = Duration::from_secs(5);
/// A failed subject waits `FIRST_RETRY << failures`, up to `MAX_RETRY`.
const FIRST_RETRY_MS: i64 = 60_000;
const MAX_RETRY_MS: i64 = 60 * 60_000;

/// In-memory per-subject backoff. A restart retries everything once, which
/// is bounded by `PER_TICK` and the backoff that follows.
#[derive(Default)]
pub(super) struct Backoff {
    retry: HashMap<names::Subject, (u32, i64)>,
}

impl Backoff {
    fn waiting(&self, subject: &names::Subject, now: i64) -> bool {
        self.retry.get(subject).is_some_and(|(_, at)| *at > now)
    }

    fn failed(&mut self, subject: &names::Subject, now: i64) {
        let entry = self.retry.entry(subject.clone()).or_insert((0, now));
        entry.1 = now + (FIRST_RETRY_MS << entry.0.min(6)).min(MAX_RETRY_MS);
        entry.0 = entry.0.saturating_add(1);
    }
}

/// Outcome of one pass, for the thread's own pacing.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Pass {
    pub attempted: usize,
    pub failed: usize,
}

impl Controller {
    pub(super) fn start_naming(self: &Arc<Self>) {
        let controller = self.clone();
        std::thread::Builder::new()
            .name("neko-naming".into())
            .spawn(move || {
                let mut backoff = Backoff::default();
                let mut idle_failures = 0_u32;
                loop {
                    // Light work, but still held back on an overloaded machine.
                    if admission::load_average().is_none_or(|load| load < 20.0) {
                        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            controller.naming_tick(&mut backoff, generate_name)
                        })) {
                            Ok(Ok(pass)) if pass.attempted > 0 && pass.failed == pass.attempted => {
                                idle_failures = idle_failures.saturating_add(1);
                            }
                            Ok(Ok(_)) => idle_failures = 0,
                            Ok(Err(error)) => {
                                idle_failures = idle_failures.saturating_add(1);
                                eprintln!("neko naming: {error}");
                            }
                            Err(_) => {
                                idle_failures = idle_failures.saturating_add(1);
                                eprintln!("neko naming: pass panicked; continuing");
                            }
                        }
                    }
                    // A model that fails every call slows the whole pass too.
                    std::thread::sleep(TICK * (1_u32 << idle_failures.min(6)));
                }
            })
            .map(|_| ())
            .unwrap_or_else(|error| eprintln!("neko naming: cannot start: {error}"));
    }

    /// Generate up to `PER_TICK` missing names. Failures are swallowed into
    /// `backoff`; only storage errors are returned.
    pub(super) fn naming_tick(
        &self,
        backoff: &mut Backoff,
        generate: impl Fn(&str, &AgentRuntime) -> Result<String, String>,
    ) -> Result<Pass, String> {
        let now = store::now_ms();
        let (jobs, runtime) = {
            let db = self.db.lock().map_err(|_| "Task storage unavailable")?;
            let snapshot = store::load(&db)?;
            (names::pending(&snapshot, PER_TICK, |s| backoff.waiting(s, now)), snapshot.agent_runtime)
        };
        let mut pass = Pass::default();
        for job in jobs {
            pass.attempted += 1;
            let name = generate(&job.prompt, &runtime).ok().and_then(|answer| names::parse_generated(&answer));
            let Some(name) = name else {
                pass.failed += 1;
                backoff.failed(&job.subject, store::now_ms());
                continue;
            };
            backoff.retry.remove(&job.subject);
            let db = self.db.lock().map_err(|_| "Task storage unavailable")?;
            let mut snapshot = store::load(&db)?;
            if names::commit(&mut snapshot, &job, name) {
                store::save(&db, &snapshot)?;
            }
        }
        Ok(pass)
    }
}

/// One isolated, tool-free extraction in its own empty scratch directory,
/// exactly as memory learning runs its extractor.
fn generate_name(prompt: &str, runtime: &AgentRuntime) -> Result<String, String> {
    let scratch = std::env::temp_dir().join(format!("neko-naming-{}", store::new_id()));
    let result = (|| {
        std::fs::create_dir(&scratch).map_err(|e| format!("Cannot create naming scratch: {e}"))?;
        native_runner::extract(
            &native_runner::RunSpec {
                directory: scratch.clone(),
                prompt: prompt.to_owned(),
                writable: false,
                timeout: Duration::from_secs(60),
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
    use neko_protocol::mcp_host::McpCommand;
    use std::cell::RefCell;

    #[test]
    fn pass_fills_only_missing_names_and_never_overwrites_the_users() {
        let controller = super::super::tests::controller_with_task(TaskStatus::Queued);
        let task_id = {
            let db = controller.db.lock().unwrap();
            let mut snapshot = store::load(&db).unwrap();
            let mut named = snapshot.tasks[0].clone();
            named.id = "named".into();
            named.display_title = Some("My own name".into());
            snapshot.tasks.push(named);
            store::save(&db, &snapshot).unwrap();
            snapshot.tasks[0].id.clone()
        };
        let prompts = RefCell::new(Vec::new());
        let pass = controller
            .naming_tick(&mut Backoff::default(), |prompt, _| {
                prompts.borrow_mut().push(prompt.to_owned());
                Ok("Fix crash when saving a client's basic info\n".into())
            })
            .unwrap();
        assert_eq!(pass, Pass { attempted: 1, failed: 0 });
        assert!(prompts.borrow()[0].contains("untrusted data"));
        let snapshot = store::load(&controller.db.lock().unwrap()).unwrap();
        let title = |id: &str| snapshot.tasks.iter().find(|t| t.id == id).unwrap().display_title.clone();
        assert_eq!(title(&task_id).as_deref(), Some("Fix crash when saving a client's basic info"));
        assert_eq!(title("named").as_deref(), Some("My own name"));
        // Nothing left to name: no model call at all.
        let pass = controller.naming_tick(&mut Backoff::default(), |_, _| panic!("no call expected")).unwrap();
        assert_eq!(pass, Pass::default());
    }

    #[test]
    fn failures_and_invalid_answers_back_off_instead_of_looping() {
        let controller = super::super::tests::controller_with_task(TaskStatus::Queued);
        let mut backoff = Backoff::default();
        let calls = RefCell::new(0);
        let pass = controller
            .naming_tick(&mut backoff, |_, _| {
                *calls.borrow_mut() += 1;
                Ok("First line\nsecond line".into())
            })
            .unwrap();
        assert_eq!(pass, Pass { attempted: 1, failed: 1 });
        let pass = controller.naming_tick(&mut backoff, |_, _| Err("model down".into())).unwrap();
        assert_eq!(pass, Pass::default(), "a failed subject waits before its next attempt");
        assert_eq!(*calls.borrow(), 1);
        let snapshot = store::load(&controller.db.lock().unwrap()).unwrap();
        assert_eq!(snapshot.tasks[0].display_title, None);
    }

    #[test]
    fn rename_commands_set_names_that_the_pass_keeps() {
        let controller = super::super::tests::controller_with_task(TaskStatus::Queued);
        let (task_id, workspace_id) = {
            let snapshot = store::load(&controller.db.lock().unwrap()).unwrap();
            (snapshot.tasks[0].id.clone(), snapshot.tasks[0].workspace_id.clone())
        };
        controller.command(Command::RenameTask { task_id: task_id.clone(), title: "  Fix login  ".into() }).unwrap();
        assert!(controller.command(Command::RenameTask { task_id: task_id.clone(), title: "a\nb".into() }).is_err());
        assert!(controller.command(Command::RenameTask { task_id: "missing".into(), title: "x".into() }).is_err());
        let watch_id = "r".to_owned();
        {
            let db = controller.db.lock().unwrap();
            let mut snapshot = store::load(&db).unwrap();
            snapshot.mcp.connections.push(neko_protocol::mcp_host::McpConnection {
                oauth: false,
                id: "c".into(),
                workspace_id: workspace_id.clone(),
                label: "Sentry".into(),
                config: neko_protocol::mcp_host::ServerConfig::Http { url: "https://example.com/mcp".into() },
                enabled: true,
                trusted: true,
                has_credentials: false,
                tools: vec![],
                discovered_ms: None,
                error: None,
                source_link: None,
            });
            snapshot.mcp.responsibilities.push(neko_protocol::mcp_host::Responsibility {
                id: watch_id.clone(),
                workspace_id,
                instruction: "Check for new errors".into(),
                connection_ids: vec!["c".into()],
                enabled: false,
                prepare_low_risk: false,
                next_due_ms: 0,
                last_attempt_ms: None,
                last_result: String::new(),
                failures: 0,
                name: None,
                name_by_user: false,
            });
            store::save(&db, &snapshot).unwrap();
        }
        controller
            .command(Command::Mcp(McpCommand::RenameResponsibility { responsibility_id: watch_id.clone(), name: "Sentry: my errors".into() }))
            .unwrap();
        let pass = controller.naming_tick(&mut Backoff::default(), |_, _| panic!("user names are never regenerated")).unwrap();
        assert_eq!(pass, Pass::default());
        let snapshot = store::load(&controller.db.lock().unwrap()).unwrap();
        assert_eq!(snapshot.tasks[0].display_title.as_deref(), Some("Fix login"));
        assert_eq!(snapshot.mcp.responsibilities[0].name.as_deref(), Some("Sentry: my errors"));
        assert!(snapshot.mcp.responsibilities[0].name_by_user);
        // Clearing hands both back to Neko.
        controller.command(Command::RenameTask { task_id, title: " ".into() }).unwrap();
        controller
            .command(Command::Mcp(McpCommand::RenameResponsibility { responsibility_id: watch_id, name: String::new() }))
            .unwrap();
        let pass = controller
            .naming_tick(&mut Backoff::default(), |prompt, _| {
                assert!(!prompt.contains("<<<WATCH") || prompt.contains("Connections: Sentry"), "{prompt}");
                Ok("Generated name".into())
            })
            .unwrap();
        assert_eq!(pass, Pass { attempted: 2, failed: 0 });
        let snapshot = store::load(&controller.db.lock().unwrap()).unwrap();
        assert_eq!(snapshot.tasks[0].display_title.as_deref(), Some("Generated name"));
        assert_eq!(snapshot.mcp.responsibilities[0].name.as_deref(), Some("Generated name"));
        assert!(!snapshot.mcp.responsibilities[0].name_by_user);
    }
}
