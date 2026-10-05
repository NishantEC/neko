use super::*;

impl Controller {
    pub(super) fn set_conversation_runtime(
        &self,
        conversation_id: String,
        preferences: RuntimePreferences,
        check: impl FnOnce(&Snapshot, &RuntimePreferences) -> Result<(), String>,
    ) -> Result<Snapshot, String> {
        // Discovery and the explicit connection check never hold the DB lock.
        let before = store::load(&*self.db.lock().map_err(|_| "Task storage unavailable")?)?;
        neko_core::runtime_selection::validate_scope(&before, &conversation_id)?;
        neko_core::runtime_selection::validate_preferences(&preferences)?;
        if preferences.provider.is_some()
            || preferences.reasoning_effort.is_some()
            || preferences.service_tier.is_some()
        {
            check(&before, &preferences)?;
        }
        let db = self.db.lock().map_err(|_| "Task storage unavailable")?;
        let current = store::load(&db)?;
        if current.conversation_runtime_revisions.get(&conversation_id)
            != before.conversation_runtime_revisions.get(&conversation_id)
            || current.agent_runtime != before.agent_runtime
        {
            return Err("Conversation settings changed during the check. Reopen the selector and try again.".into());
        }
        store::apply(
            &db,
            Command::SetConversationRuntime {
                conversation_id,
                preferences,
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn controller() -> Controller {
        Controller::new(Arc::new(Mutex::new(Db::open_in_memory().unwrap())))
    }
    fn pins() -> RuntimePreferences {
        RuntimePreferences {
            provider: Some("codex".into()),
            model: Some("fixture".into()),
            reasoning_effort: Some("high".into()),
            service_tier: Some("default".into()),
            allow_paid_speed: false,
        }
    }
    #[test]
    fn failed_check_keeps_saved_settings_and_active_worker() {
        let controller = controller();
        let id = neko_core::runtime_selection::home_id("default", None);
        let cancelled = Arc::new(AtomicBool::new(false));
        *controller.chat_active.lock().unwrap() = Some(Active {
            task_id: "running".into(),
            cancelled: cancelled.clone(),
        });
        assert!(
            controller
                .set_conversation_runtime(id.clone(), pins(), |_, _| Err("No access".into()))
                .is_err()
        );
        assert!(
            controller
                .command(Command::Snapshot)
                .unwrap()
                .conversation_runtime
                .is_empty()
        );
        let after = controller
            .set_conversation_runtime(id.clone(), pins(), |_, _| Ok(()))
            .unwrap();
        assert_eq!(after.conversation_runtime[&id], pins());
        assert_eq!(after.agent_runtime, AgentRuntime::default());
        assert!(!cancelled.load(Ordering::Acquire));
    }
    #[test]
    fn concurrent_change_wins_over_slow_check() {
        let controller = controller();
        let id = neko_core::runtime_selection::home_id("default", None);
        let updated = RuntimePreferences {
            allow_paid_speed: true,
            ..Default::default()
        };
        let error = controller
            .set_conversation_runtime(id.clone(), pins(), |_, _| {
                // The check can re-enter storage: no lock held around model work.
                controller.set_conversation_runtime(id.clone(), updated.clone(), |_, _| {
                    panic!("No inference for Auto")
                })?;
                Ok(())
            })
            .unwrap_err();
        assert!(error.contains("changed during the check"));
        assert_eq!(
            controller
                .command(Command::Snapshot)
                .unwrap()
                .conversation_runtime[&id],
            updated
        );
    }
    #[test]
    fn reset_to_same_defaults_invalidates_an_older_manual_check() {
        let controller = controller();
        let id = neko_core::runtime_selection::home_id("default", None);
        let error = controller
            .set_conversation_runtime(id.clone(), pins(), |_, _| {
                controller.set_conversation_runtime(
                    id.clone(),
                    RuntimePreferences::default(),
                    |_, _| panic!("Reset needs no inference"),
                )?;
                Ok(())
            })
            .unwrap_err();
        assert!(error.contains("changed during the check"));
        let after = controller.command(Command::Snapshot).unwrap();
        assert!(!after.conversation_runtime.contains_key(&id));
        assert_eq!(after.conversation_runtime_revisions[&id], 1);
    }
    #[test]
    fn missing_scope_is_rejected_before_inference() {
        let controller = controller();
        let id = "task:missing".to_owned();
        assert!(
            controller
                .set_conversation_runtime(id, pins(), |_, _| panic!(
                    "Unknown scope must fail before inference"
                ))
                .is_err()
        );
    }
}
