//! Exactly one resident follow-up worker, including completed-ticket skills.
use super::*;
use neko_core::memory_learning as learning;

impl Controller {
    pub(super) fn start_learning(self: &Arc<Self>) {
        let controller = self.clone();
        std::thread::spawn(move || {
            let mut owned = None;
            let mut startup = true;
            let mut failures = 0_u32;
            loop {
                match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    controller.recover_learning_startup(&mut startup)?;
                    controller.learning_tick(&mut owned)
                })) {
                    Ok(Err(error)) => {
                        failures = failures.saturating_add(1);
                        eprintln!("neko learning: {error}");
                    }
                    Err(_) => {
                        failures = failures.saturating_add(1);
                        eprintln!("neko learning worker panicked; interrupted job will retry");
                    }
                    Ok(Ok(())) => {
                        failures = 0;
                    }
                }
                std::thread::sleep(Duration::from_secs((2_u64 << failures.min(4)).min(30)));
            }
        });
    }

    fn recover_learning_startup(&self, pending: &mut bool) -> Result<(), String> {
        if *pending {
            let db = self
                .db
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            learning::recover(&db)?;
            self.db.clear_poison();
            *pending = false;
        }
        Ok(())
    }

    fn recover_owned_attempt(&self, owned: &mut Option<learning::Job>) -> Result<(), String> {
        if let Some(job) = owned.as_ref() {
            let db = self
                .db
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            learning::retry_owned(&db, job, store::now_ms())?;
            self.db.clear_poison();
            *owned = None;
        }
        Ok(())
    }

    fn learning_tick(&self, owned: &mut Option<learning::Job>) -> Result<(), String> {
        // Retain ownership in memory if even the retry write fails. Subsequent
        // ticks back off and retry that write before attempting another claim.
        self.recover_owned_attempt(owned)?;
        let (job, snapshot, authority) = {
            let db = self.db.lock().map_err(|_| "Learning storage unavailable")?;
            let snapshot = store::load(&db)?;
            let Some(job) = learning::claim(&db, &snapshot)? else {
                return Ok(());
            };
            *owned = Some(job.clone());
            let authority = job
                .workspace_id
                .as_deref()
                .map(|w| responsibilities::RunAuthority::for_learning(&snapshot, w))
                .transpose()?;
            (job, snapshot, authority)
        };
        if !job.needs_memory() {
            let result = self.finish_followup(
                &job,
                authority.as_ref(),
                Ok(String::new()),
                propose_ticket_skill,
            );
            if result.is_ok() {
                *owned = None;
            }
            return result;
        }
        // Every extractor gets its own empty directory, including scoped work.
        // It never inherits chat's already-deleted scratch or the Neko data dir.
        let scratch = std::env::temp_dir().join(format!("neko-learning-{}", store::new_id()));
        let result = (|| {
            std::fs::create_dir(&scratch)
                .map_err(|e| format!("Cannot create learning scratch: {e}"))?;
            native_runner::extract(
                &native_runner::RunSpec {
                    directory: scratch.clone(),
                    prompt: learning::prompt(&job, &snapshot),
                    writable: false,
                    timeout: Duration::from_secs(90),
                    runtime: snapshot.agent_runtime.clone(),
                },
                &AtomicBool::new(false),
            )
        })();
        let _ = std::fs::remove_dir(&scratch);
        let result = self.finish_followup(&job, authority.as_ref(), result, propose_ticket_skill);
        if result.is_ok() {
            *owned = None;
        }
        result
    }

    fn finish_followup(
        &self,
        job: &learning::Job,
        authority: Option<&responsibilities::RunAuthority>,
        result: Result<String, String>,
        propose_skill: impl FnOnce(&Arc<Mutex<Db>>, &Task, &learning::Job) -> Result<(), String>,
    ) -> Result<(), String> {
        let committed = if job.needs_memory() {
            self.commit_learning(
                job,
                authority,
                result.as_deref().unwrap_or("{\"memories\":[]}"),
            )?
        } else {
            job.memory_outcome()
        }; // Storage failures stop dependent work.
        // Preserve existing skill learning, now behind the same one-worker cap.
        if committed != learning::FinishOutcome::Stale
            && let learning::Source::Ticket(id) = &job.source
        {
            let fresh = store::load(&*self.db.lock().map_err(|_| "Learning storage unavailable")?)?;
            if learning::valid(&job, &fresh)
                && let Some(task) = fresh.tasks.iter().find(|t| t.id == *id)
            {
                propose_skill(&self.db, task, job)?;
                let db = self.db.lock().map_err(|_| "Learning storage unavailable")?;
                let mut after = store::load(&db)?;
                if learning::valid(&job, &after)
                    && let Some(task) = after.tasks.iter_mut().find(|t| t.id == *id)
                {
                    store::append_event(
                        task,
                        "learning",
                        if result.is_err()
                            || matches!(committed, learning::FinishOutcome::Rejected(_))
                        {
                            "Memory extraction could not finish. This ticket remains complete; no inferred memory was saved. Any skill proposal still requires review."
                        } else {
                            "Checked for reusable learning. Review suggested memories on Memory and skills in Tools & skills; nothing inferred was activated automatically."
                        },
                    );
                    db.atomic(|| {
                        store::save(&db, &after)?;
                        learning::finish_skill(&db, job)
                    })?;
                }
            }
        }
        if let learning::FinishOutcome::Rejected(reason) = committed {
            return Err(reason);
        }
        result.map(|_| ())
    }

    fn commit_learning(
        &self,
        job: &learning::Job,
        authority: Option<&responsibilities::RunAuthority>,
        answer: &str,
    ) -> Result<learning::FinishOutcome, String> {
        let db = self.db.lock().map_err(|_| "Learning storage unavailable")?;
        let snapshot = store::load(&db)?;
        let answer = if authority.is_none_or(|a| a.valid(&snapshot)) {
            answer
        } else {
            "{\"memories\":[]}"
        };
        learning::finish(&db, &snapshot, job, answer)
    }
}

/// Proposal creation and source completion are one durable operation. Accepting
/// or rejecting the proposal after a crash cannot make its source run again.
pub(super) fn commit_ticket_skill(
    db: &Arc<Mutex<Db>>,
    task: &Task,
    job: &learning::Job,
    body: Option<&str>,
) -> Result<(), String> {
    let db = db.lock().map_err(|_| "Learning storage unavailable")?;
    let snapshot = store::load(&db)?;
    if !learning::valid(job, &snapshot) || !learning::owns_skill(&db, job)? {
        return Err("Skill learning source or attempt changed; no proposal saved".into());
    }
    db.atomic(|| {
        if let Some(body) = body {
            neko_core::skills::propose(
                &db,
                &task.workspace_id,
                &format!("Learning from {}", task.title),
                body,
                &format!("Completed ticket {}", task.id),
                None,
            )?;
        }
        learning::finish_skill(&db, job)
    })
}

/// Model/format failures are terminal once this receipt commits. Storage errors
/// remain retryable; a failed write cannot masquerade as successful completion.
pub(super) fn fail_ticket_skill(
    db: &Arc<Mutex<Db>>,
    task: &Task,
    job: &learning::Job,
    reason: &str,
) -> Result<(), String> {
    let db = db.lock().map_err(|_| "Learning storage unavailable")?;
    let mut snapshot = store::load(&db)?;
    if !learning::valid(job, &snapshot) || !learning::owns_skill(&db, job)? {
        return Err("Skill learning source or attempt changed".into());
    }
    let current = snapshot
        .tasks
        .iter_mut()
        .find(|t| t.id == task.id)
        .ok_or("Completed ticket no longer exists")?;
    store::append_event(
        current,
        "learning",
        "Skill extraction could not finish. The ticket is complete; no skill was installed. Any memory proposal still requires review.",
    );
    db.atomic(|| {
        store::save(&db, &snapshot)?;
        learning::finish_skill(&db, job)
    })?;
    Err(reason.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn startup_recovery_remains_pending_until_storage_recovers() {
        let controller = super::super::tests::controller_with_task(TaskStatus::Completed);
        let (job, _) = completed_job(&controller);
        let json = {
            let db = controller.db.lock().unwrap();
            let json = db.get_setting("memory_learning_v1").unwrap().unwrap();
            db.set_setting("memory_learning_v1", "corrupt").unwrap();
            json
        };
        let mut pending = true;
        assert!(controller.recover_learning_startup(&mut pending).is_err());
        assert!(pending);
        controller
            .db
            .lock()
            .unwrap()
            .set_setting("memory_learning_v1", &json)
            .unwrap();
        controller.recover_learning_startup(&mut pending).unwrap();
        assert!(!pending);
        let db = controller.db.lock().unwrap();
        let recovered = learning::claim(&db, &store::load(&db).unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(recovered.source, job.source);
        assert!(recovered.needs_memory());
    }

    #[test]
    fn skill_model_failure_is_terminal_and_does_not_repeat_extraction() {
        let controller = super::super::tests::controller_with_task(TaskStatus::Completed);
        let (job, authority) = completed_job(&controller);
        let result = controller.finish_followup(
            &job,
            Some(&authority),
            Ok("{\"memories\":[]}".into()),
            |db, task, job| fail_ticket_skill(db, task, job, "Malformed skill output"),
        );
        assert_eq!(result.unwrap_err(), "Malformed skill output");
        let db = controller.db.lock().unwrap();
        learning::retry_owned(&db, &job, store::now_ms()).unwrap();
        learning::recover(&db).unwrap();
        let state = store::load(&db).unwrap();
        assert!(learning::claim(&db, &state).unwrap().is_none());
        assert!(
            state.tasks[0]
                .events
                .iter()
                .any(|event| event.message.contains("Skill extraction could not finish"))
        );
        assert!(neko_core::skills::load(&db).unwrap().proposals.is_empty());
    }

    #[test]
    fn failed_retry_write_retains_owned_attempt_until_storage_recovers() {
        let controller = super::super::tests::controller_with_task(TaskStatus::Completed);
        let (job, _) = completed_job(&controller);
        let mut owned = Some(job);
        let json = {
            let db = controller.db.lock().unwrap();
            let json = db.get_setting("memory_learning_v1").unwrap().unwrap();
            db.set_setting("memory_learning_v1", "corrupt").unwrap();
            json
        };
        assert!(controller.recover_owned_attempt(&mut owned).is_err());
        assert!(owned.is_some());
        controller
            .db
            .lock()
            .unwrap()
            .set_setting("memory_learning_v1", &json)
            .unwrap();
        controller.recover_owned_attempt(&mut owned).unwrap();
        assert!(owned.is_none());
        let db = controller.db.lock().unwrap();
        let state = store::load(&db).unwrap();
        assert!(
            learning::claim(&db, &state).unwrap().is_none(),
            "Recovered attempt honors backoff"
        );
        let id = neko_chat::begin_turn(&db, "another source").unwrap();
        neko_chat::finish_turn(&db, &id, "reply", vec![], false).unwrap();
        let state = store::load(&db).unwrap();
        learning::enqueue(&db, &state, learning::Source::Chat(id.clone())).unwrap();
        assert_eq!(
            learning::claim(&db, &state).unwrap().unwrap().source,
            learning::Source::Chat(id)
        );
    }

    #[test]
    fn interrupted_skill_phase_recovers_without_repeating_memory_and_publishes_once() {
        let controller = super::super::tests::controller_with_task(TaskStatus::Completed);
        let (job, authority) = completed_job(&controller);
        let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            controller.finish_followup(
                &job,
                Some(&authority),
                Ok("invalid JSON".into()),
                |_, _, _| panic!("interrupted skill phase"),
            )
        }));
        assert!(panicked.is_err());
        let recovered = {
            let db = controller.db.lock().unwrap();
            learning::recover(&db).unwrap();
            learning::claim(&db, &store::load(&db).unwrap())
                .unwrap()
                .unwrap()
        };
        assert!(!recovered.needs_memory());
        let result = controller.finish_followup(&recovered, Some(&authority), Ok("must never be parsed again".into()), |db, task, current| {
            commit_ticket_skill(db, task, current, Some("---\nname: recovered\ndescription: Useful bounded procedure\n---\nInspect the evidence."))
        });
        assert!(result.unwrap_err().contains("Invalid memory extraction"));
        let state = controller.command(Command::Snapshot).unwrap();
        assert_eq!(state.skills.proposals.len(), 1);
        let proposal = &state.skills.proposals[0];
        // The user rejects the source's proposal before a subsequent restart.
        let db = controller.db.lock().unwrap();
        neko_core::skills::decide(
            &db,
            std::path::Path::new("/unused"),
            &proposal.id,
            &proposal.content_hash,
            false,
        )
        .unwrap();
        learning::recover(&db).unwrap();
        learning::retry_owned(&db, &recovered, store::now_ms()).unwrap();
        assert!(
            learning::claim(&db, &store::load(&db).unwrap())
                .unwrap()
                .is_none()
        );
        assert!(neko_core::skills::load(&db).unwrap().proposals.is_empty());
    }
    fn completed_job(controller: &Controller) -> (learning::Job, responsibilities::RunAuthority) {
        let db = controller.db.lock().unwrap();
        let state = store::load(&db).unwrap();
        learning::enqueue(&db, &state, learning::Source::Ticket("t".into())).unwrap();
        let job = learning::claim(&db, &state).unwrap().unwrap();
        (
            job,
            responsibilities::RunAuthority::for_learning(&state, "w").unwrap(),
        )
    }

    #[test]
    fn rejected_memory_output_still_runs_independent_skill_followup() {
        let controller = super::super::tests::controller_with_task(TaskStatus::Completed);
        let (job, authority) = completed_job(&controller);
        let mut called = false;
        let result = controller.finish_followup(
            &job,
            Some(&authority),
            Ok("not JSON".into()),
            |_, task, captured| {
                assert_eq!(task.id, "t");
                assert_eq!(captured.id, job.id);
                called = true;
                Ok(())
            },
        );
        assert!(
            called,
            "Invalid memory output must not suppress independent skills"
        );
        assert!(result.unwrap_err().contains("Invalid memory extraction"));
        let state = controller.command(Command::Snapshot).unwrap();
        assert!(state.memory_proposals.is_empty());
        assert!(
            state.tasks[0].events.iter().any(|e| e.role == "learning"
                && e.message.contains("Memory extraction could not finish"))
        );
    }

    #[test]
    fn storage_failure_or_revocation_never_launches_skill_followup() {
        for corrupt in [false, true] {
            let controller = super::super::tests::controller_with_task(TaskStatus::Completed);
            let (job, authority) = completed_job(&controller);
            {
                let db = controller.db.lock().unwrap();
                if corrupt {
                    db.set_setting("memory_learning_v1", "not valid JSON")
                        .unwrap();
                } else {
                    let mut state = store::load(&db).unwrap();
                    state.agent_profiles.revision += 1;
                    store::save(&db, &state).unwrap();
                }
            }
            let mut called = false;
            let result = controller.finish_followup(
                &job,
                Some(&authority),
                Ok("not JSON".into()),
                |_, _, _| {
                    called = true;
                    Ok(())
                },
            );
            assert!(!called);
            assert_eq!(result.is_err(), corrupt);
        }
    }

    #[test]
    fn saturated_memory_proposals_do_not_block_ticket_skill_learning() {
        let controller = super::super::tests::controller_with_task(TaskStatus::Completed);
        {
            let db = controller.db.lock().unwrap();
            for i in 0..22 {
                let id = neko_chat::begin_turn(&db, "Useful evidence").unwrap();
                neko_chat::finish_turn(&db, &id, "Answer", vec![], false).unwrap();
                let state = store::load(&db).unwrap();
                learning::enqueue(&db, &state, learning::Source::Chat(id)).unwrap();
                let job = learning::claim(&db, &state).unwrap().unwrap();
                let memories: Vec<_> = (0..if i == 21 { 1 } else { 3 })
                    .map(|n| serde_json::json!({"text":format!("Fact {i}-{n}"),"kind":"profile"}))
                    .collect();
                assert_eq!(
                    learning::finish(
                        &db,
                        &state,
                        &job,
                        &serde_json::json!({"memories":memories}).to_string()
                    )
                    .unwrap(),
                    learning::FinishOutcome::Committed
                );
            }
            assert_eq!(store::load(&db).unwrap().memory_proposals.len(), 64);
        }
        let (job, authority) = completed_job(&controller);
        let mut called = false;
        let result = controller.finish_followup(
            &job,
            Some(&authority),
            Ok(r#"{"memories":[{"text":"Ticket fact","kind":"workspace"}]}"#.into()),
            |_, _, _| {
                called = true;
                Ok(())
            },
        );
        assert!(called);
        assert!(
            result
                .unwrap_err()
                .contains("Review existing memory proposals")
        );
        assert_eq!(
            controller
                .command(Command::Snapshot)
                .unwrap()
                .memory_proposals
                .len(),
            64
        );
    }
    #[test]
    fn full_learning_queue_never_discards_a_successful_chat_reply() {
        let db = Arc::new(Mutex::new(Db::open_in_memory().unwrap()));
        for _ in 0..32 {
            let guard = db.lock().unwrap();
            let id = neko_chat::begin_turn(&guard, "Evidence").unwrap();
            neko_chat::finish_turn(&guard, &id, "Answered", vec![], false).unwrap();
            learning::enqueue(
                &guard,
                &store::load(&guard).unwrap(),
                learning::Source::Chat(id),
            )
            .unwrap();
        }
        let id = neko_chat::begin_turn(&db.lock().unwrap(), "One more").unwrap();
        complete_chat_reply(
            &db,
            &AtomicBool::new(false),
            &id,
            "One more",
            None,
            neko_chat::Reply { used_memory: vec![],
                text: "Your answer".into(),
                tickets: vec![],
                memories: vec![],
                responsibilities: vec![],
            },
        )
        .unwrap();
        let state = store::load(&db.lock().unwrap()).unwrap();
        let reply = state.conversation.last().unwrap();
        assert!(!reply.pending && !reply.failed);
        assert!(reply.text.starts_with("Your answer"));
        assert!(reply.text.contains("Memory learning is busy"));
    }
    #[test]
    fn chat_followup_is_queued_and_late_profile_output_is_discarded() {
        let db = Arc::new(Mutex::new(Db::open_in_memory().unwrap()));
        let controller = Controller::new(db.clone());
        let id = neko_chat::begin_turn(&db.lock().unwrap(), "Small batches worked").unwrap();
        complete_chat_reply(
            &db,
            &AtomicBool::new(false),
            &id,
            "Small batches worked",
            None,
            neko_chat::Reply { used_memory: vec![],
                text: "Acknowledged".into(),
                tickets: vec![],
                memories: vec![],
                responsibilities: vec![],
            },
        )
        .unwrap();
        let job = {
            let db = db.lock().unwrap();
            let mut state = store::load(&db).unwrap();
            let job = learning::claim(&db, &state)
                .unwrap()
                .expect("chat queues separate extraction");
            state.agent_profiles.revision += 1;
            store::save(&db, &state).unwrap();
            job
        };
        controller
            .commit_learning(
                &job,
                None,
                r#"{"memories":[{"text":"stale","kind":"profile"}]}"#,
            )
            .unwrap();
        let state = store::load(&db.lock().unwrap()).unwrap();
        assert!(state.memory_proposals.is_empty());
        assert!(state.memory.is_empty());
    }
}
