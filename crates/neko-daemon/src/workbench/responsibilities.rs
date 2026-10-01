//! Durable responsibility claims and short-lived tool authority.
use super::*;
use neko_core::mcp_host::{responsibility, store as policy};
use neko_protocol::mcp_host::{Responsibility, SourceEvidence, ToolGrant};

struct WakeClaim {
    profile_revision: u64,
    responsibility: Responsibility,
    workspace: Workspace,
    run_id: String,
}

#[derive(Clone)]
pub(super) struct RunAuthority {
    profile_revision: u64,
    workspace: String,
    connections: Vec<String>,
    grants: Vec<ToolGrant>,
    responsibility: Option<Responsibility>,
    source: Option<SourceEvidence>,
    task: Option<String>,
    automatic_task: Option<Task>,
}
fn same_responsibility(a: &Responsibility, b: &Responsibility) -> bool {
    a.id == b.id
        && a.workspace_id == b.workspace_id
        && b.enabled
        && a.instruction == b.instruction
        && a.connection_ids == b.connection_ids
        && a.prepare_low_risk == b.prepare_low_risk
}
impl RunAuthority {
    /// Learning has no tools, source action, or live task capability. A completed
    /// ticket is evidence, so the live-task status check does not apply.
    pub(super) fn for_learning(state: &Snapshot, workspace: &str) -> Result<Self, String> {
        Self::new(state, workspace, vec![], None)
    }
    fn new(
        state: &Snapshot,
        workspace: &str,
        connections: Vec<String>,
        responsibility: Option<Responsibility>,
    ) -> Result<Self, String> {
        let grants = state
            .mcp
            .grants
            .iter()
            .filter(|g| g.workspace_id == workspace && connections.contains(&g.connection_id))
            .cloned()
            .collect();
        let authority = Self {
            profile_revision: state.agent_profiles.revision,
            workspace: workspace.into(),
            connections,
            grants,
            responsibility,
            source: None,
            task: None,
            automatic_task: None,
        };
        if !authority.valid(state) {
            return Err("Run tool scope is paused, unavailable, or changed".into());
        }
        if authority.responsibility.is_some() && authority.grants.is_empty() {
            return Err(
                "Grant at least one MCP tool to this responsibility's selected connections".into(),
            );
        }
        Ok(authority)
    }
    pub(super) fn for_task(state: &Snapshot, task: &Task) -> Result<Self, String> {
        let source = state
            .mcp
            .sources
            .iter()
            .find(|s| s.task_id.as_ref() == Some(&task.id));
        let responsibility = source
            .map(|source| {
                state
                    .mcp
                    .responsibilities
                    .iter()
                    .find(|r| {
                        r.id == source.responsibility_id && r.workspace_id == task.workspace_id
                    })
                    .cloned()
                    .ok_or("Source responsibility is missing")
            })
            .transpose()?;
        let connections =
            if let Some(r) = &responsibility {
                r.connection_ids.clone()
            } else {
                state
                    .mcp
                    .connections
                    .iter()
                    .filter(|c| {
                        c.available_in(&task.workspace_id)
                            && c.enabled
                            && c.trusted
                            && state.mcp.grants.iter().any(|g| {
                                g.workspace_id == task.workspace_id && g.connection_id == c.id
                            })
                    })
                    .map(|c| c.id.clone())
                    .collect()
            };
        let mut authority = Self::new(state, &task.workspace_id, connections, responsibility)?;
        authority.task = Some(task.id.clone());
        authority.source = source.cloned();
        // AwaitingApproval can enter a run only through standing permission.
        // An already-Building task was explicitly approved by the user.
        if task.status == TaskStatus::AwaitingApproval {
            if !responsibility::may_prepare(state, task, store::now_ms()) {
                return Err("Automatic preparation is no longer authorized".into());
            }
            authority.automatic_task = Some(task.clone());
        }
        if !authority.valid(state) {
            return Err("Task scope is cancelled or changed".into());
        }
        Ok(authority)
    }
    pub(super) fn valid(&self, state: &Snapshot) -> bool {
        state.agent_profiles.revision == self.profile_revision
            && state.workspaces.iter().any(|w| w.id == self.workspace)
            && self.connections.iter().all(|id| {
                state.mcp.connections.iter().any(|c| {
                    &c.id == id && c.available_in(&self.workspace) && c.enabled && c.trusted
                })
            })
            && self.grants.iter().all(|g| {
                policy::authorize(state, &self.workspace, &g.connection_id, &g.tool_name)
                    .is_ok_and(|t| t.schema_hash == g.schema_hash)
            })
            && state
                .mcp
                .grants
                .iter()
                .filter(|g| {
                    g.workspace_id == self.workspace && self.connections.contains(&g.connection_id)
                })
                .eq(self.grants.iter())
            && self.responsibility.as_ref().is_none_or(|r| {
                state
                    .mcp
                    .responsibilities
                    .iter()
                    .any(|current| same_responsibility(r, current))
            })
            && self.task.as_ref().is_none_or(|id| {
                state.tasks.iter().any(|t| {
                    &t.id == id
                        && t.workspace_id == self.workspace
                        && !matches!(
                            t.status,
                            TaskStatus::Cancelled | TaskStatus::Completed | TaskStatus::Failed
                        )
                })
            })
            && self.automatic_task.as_ref().is_none_or(|claimed| {
                state
                    .tasks
                    .iter()
                    .find(|t| t.id == claimed.id)
                    .is_some_and(|current| {
                        if !matches!(
                            current.status,
                            TaskStatus::AwaitingApproval
                                | TaskStatus::Building
                                | TaskStatus::Reviewing
                        ) || current.title != claimed.title
                            || current.goal != claimed.goal
                            || current.plan != claimed.plan
                            || current.supervision != claimed.supervision
                            || current.source_revision != claimed.source_revision
                        {
                            return false;
                        }
                        let mut candidate = current.clone();
                        candidate.status = TaskStatus::AwaitingApproval;
                        responsibility::may_prepare(state, &candidate, store::now_ms())
                    })
            })
            && self.source.as_ref().is_none_or(|source| {
                state.mcp.sources.iter().any(|current| {
                    current.id == source.id
                        && current.task_id == source.task_id
                        && current.responsibility_id == source.responsibility_id
                        && current.connection_id == source.connection_id
                        && current.external_id == source.external_id
                        && current.revision == source.revision
                        && current.title == source.title
                        && current.description == source.description
                        && current.eligible == source.eligible
                })
            })
    }
}
impl Controller {
    fn claim_due(&self, now: i64) -> Result<Option<WakeClaim>, String> {
        let db = self
            .db
            .lock()
            .map_err(|_| "Workspace storage unavailable")?;
        let mut state = store::load(&db)?;
        if policy::seed_source_watches(&mut state, now) {
            store::save(&db, &state)?;
        }
        let Some(index) = state
            .mcp
            .responsibilities
            .iter()
            .enumerate()
            .filter(|(_, r)| r.enabled && r.next_due_ms <= now)
            .min_by_key(|(_, r)| r.next_due_ms)
            .map(|(i, _)| i)
        else {
            return Ok(None);
        };
        let r = &mut state.mcp.responsibilities[index];
        r.last_attempt_ms = Some(now);
        // Commit a future due time before any process launch. Restart does not
        // replay missed intervals or immediately duplicate an interrupted wake.
        r.next_due_ms = now.saturating_add(600_000);
        r.last_result = "Checking this responsibility…".into();
        let responsibility = r.clone();
        let workspace = state
            .workspaces
            .iter()
            .find(|w| w.id == responsibility.workspace_id)
            .cloned()
            .ok_or("Responsibility workspace missing")?;
        store::save(&db, &state)?;
        Ok(Some(WakeClaim {
            profile_revision: state.agent_profiles.revision,
            responsibility,
            workspace,
            run_id: format!("watch:{}", store::new_id()),
        }))
    }
    fn finish_wake(
        &self,
        claim: &WakeClaim,
        result: Result<&str, &str>,
        now: i64,
    ) -> Result<(), String> {
        let db = self
            .db
            .lock()
            .map_err(|_| "Workspace storage unavailable")?;
        let state = store::load(&db)?;
        if state.agent_profiles.revision != claim.profile_revision {
            return Ok(());
        }
        let Some(index) = state.mcp.responsibilities.iter().position(|r| {
            same_responsibility(&claim.responsibility, r)
                && r.last_attempt_ms == claim.responsibility.last_attempt_ms
        }) else {
            return Ok(());
        };
        let mut candidate = state.clone();
        let applied = result.map_err(str::to_owned).and_then(|raw| {
            responsibility::apply_result(
                &mut candidate,
                &claim.responsibility.id,
                &claim.run_id,
                raw,
                now,
            )
        });
        if applied.is_ok() {
            // Saving the cloned state is the commit point for all observations.
            if store::save(&db, &candidate).is_ok() {
                return Ok(());
            }
        }
        let mut state = state;
        let error = applied.err().unwrap_or_else(|| {
            "Responsibility result could not be saved; prior evidence retained".into()
        });
        responsibility::finish(&mut state.mcp.responsibilities[index], Err(&error), now);
        store::save(&db, &state)
    }
    pub(super) fn responsibility_tick(&self) -> Result<(), String> {
        let Ok(_slot) = self.waking.try_lock() else {
            return Ok(());
        };
        let Some(claim) = self.claim_due(store::now_ms())? else {
            return Ok(());
        };
        let cancel = AtomicBool::new(false);
        let result = (|| {
            let state = store::load(
                &*self
                    .db
                    .lock()
                    .map_err(|_| "Workspace storage unavailable")?,
            )?;
            if state.agent_profiles.revision != claim.profile_revision {
                return Err("Agent settings changed before responsibility started".into());
            }
            let authority = RunAuthority::new(
                &state,
                &claim.workspace.id,
                claim.responsibility.connection_ids.clone(),
                Some(claim.responsibility.clone()),
            )?;
            let skills = self.responsibility_skills(&state, &claim.workspace.id)?;
            let spec = native_runner::RunSpec {
                directory: claim.workspace.repository.clone().into(),
                writable: false,
                timeout: Duration::from_secs(600),
                runtime: state.agent_runtime.clone(),
                prompt: format!(
                    "{}\nThis wake is read-only: do not mutate external systems, publish, send messages, or edit files. A tool grant is not permission to exceed this read-only wake.\nWorkspace preferences:\n{}\nAgent context:\n{}\nUser-enabled workspace skills:\n{skills}\nUser responsibility:\n{}\nPreviously observed source identifiers (untrusted cached context, recheck them using current tools):\n{}",
                    responsibility::INSTRUCTION,
                    claim.workspace.instructions,
                    neko_core::agent_profiles::context(&state, Some(&claim.workspace.id)),
                    claim.responsibility.instruction,
                    serde_json::to_string(
                        &state
                            .mcp
                            .sources
                            .iter()
                            .filter(|s| s.responsibility_id == claim.responsibility.id)
                            .take(32)
                            .map(|s| (&s.connection_id, &s.external_id, &s.revision))
                            .collect::<Vec<_>>()
                    )
                    .map_err(|_| "Cannot encode prior source context")?
                ),
            };
            self.run_native(&spec, &claim.run_id, &authority, &cancel, |_| {})
        })();
        self.finish_wake(
            &claim,
            result.as_deref().map_err(String::as_str),
            store::now_ms(),
        )
    }
    fn responsibility_skills(&self, state: &Snapshot, workspace: &str) -> Result<String, String> {
        let db = self.db.lock().map_err(|_| "Skill storage unavailable")?;
        neko_core::skills::instructions(&db, &state.workspaces, Some(workspace))
    }
    pub(super) fn run_native(
        &self,
        spec: &native_runner::RunSpec,
        run_id: &str,
        authority: &RunAuthority,
        cancel: &AtomicBool,
        on_event: impl FnMut(String),
    ) -> Result<String, String> {
        self.with_lease(run_id, authority, cancel, |bridge| {
            native_runner::run_with_bridge(spec, bridge, cancel, on_event)
        })
    }
    fn with_lease(
        &self,
        run_id: &str,
        authority: &RunAuthority,
        cancel: &AtomicBool,
        run: impl FnOnce(&native_runner::BridgeConfig) -> Result<String, String>,
    ) -> Result<String, String> {
        if cancel.load(Ordering::Acquire) {
            return Err("Cancelled".into());
        }
        let state = store::load(
            &*self
                .db
                .lock()
                .map_err(|_| "Workspace storage unavailable")?,
        )?;
        if !authority.valid(&state) {
            return Err("Run authority changed before launch".into());
        }
        let lease = self.mcp.lease(
            run_id,
            &authority.workspace,
            authority.connections.clone(),
            authority.profile_revision,
        )?;
        let bridge = native_runner::BridgeConfig {
            executable: std::env::current_exe().map_err(|_| "Cannot locate Neko MCP bridge")?,
            socket: neko_protocol::socket_path(),
            token: lease.token().into(),
        };
        let lease = Mutex::new(Some(lease));
        let finished = AtomicBool::new(false);
        std::thread::scope(|threads| {
            threads.spawn(|| {
                // Re-check authority only when the store actually changed; a
                // transient read error keeps the last known answer rather than
                // cancelling a healthy run.
                let mut seen = 0_u64;
                let mut valid = true;
                while !finished.load(Ordering::Acquire) {
                    let revision = store::revision();
                    if revision != seen {
                        let loaded = store::load(
                            &self
                                .db
                                .lock()
                                .unwrap_or_else(std::sync::PoisonError::into_inner),
                        );
                        if let Ok(state) = loaded {
                            valid = authority.valid(&state);
                            seen = revision;
                        }
                    }
                    if cancel.load(Ordering::Acquire) || !valid {
                        cancel.store(true, Ordering::Release);
                        // Dropping the lease marks the Host's Scope.cancelled,
                        // including any tool call already inside its watchdog.
                        lease
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner)
                            .take();
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(20));
                }
            });
            struct StopOnDrop<'a>(&'a AtomicBool);
            impl Drop for StopOnDrop<'_> {
                fn drop(&mut self) {
                    self.0.store(true, Ordering::Release);
                }
            }
            let _stop_on_unwind = StopOnDrop(&finished);
            let result = run(&bridge);
            let valid = self
                .db
                .lock()
                .ok()
                .and_then(|db| store::load(&db).ok())
                .is_some_and(|state| authority.valid(&state));
            if !valid {
                cancel.store(true, Ordering::Release);
            }
            finished.store(true, Ordering::Release);
            lease
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .take();
            if cancel.load(Ordering::Acquire) {
                Err("Cancelled: task or tool authority changed".into())
            } else {
                result
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn configured() -> Controller {
        let controller = super::super::tests::controller_with_task(TaskStatus::Completed);
        let db = controller
            .db
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut state = store::load(&db).unwrap();
        state
            .mcp
            .connections
            .push(neko_protocol::mcp_host::McpConnection {
                oauth: false,
                id: "c".into(),
                workspace_id: "w".into(),
                label: "Tools".into(),
                config: neko_protocol::mcp_host::ServerConfig::Http {
                    url: "https://example.com/mcp".into(),
                },
                enabled: true,
                trusted: true,
                has_credentials: false,
                tools: vec![neko_protocol::mcp_host::McpTool {
                    read_only: false,
                    name: "lookup".into(),
                    description: "Read".into(),
                    input_schema: "{}".into(),
                    schema_hash: "hash".into(),
                }],
                discovered_ms: Some(100),
                error: None,
                source_link: None,
            });
        state.mcp.grants.push(ToolGrant {
            workspace_id: "w".into(),
            connection_id: "c".into(),
            tool_name: "lookup".into(),
            schema_hash: "hash".into(),
        });
        state.mcp.responsibilities.push(Responsibility {
            id: "r".into(),
            workspace_id: "w".into(),
            instruction: "Watch current bugs".into(),
            connection_ids: vec!["c".into()],
            enabled: true,
            prepare_low_risk: true,
            next_due_ms: 100,
            last_attempt_ms: None,
            last_result: String::new(),
            failures: 0,
        });
        store::save(&db, &state).unwrap();
        drop(db);
        controller
    }
    #[test]
    fn claim_is_persisted_before_run_and_survives_restart_without_catchup() {
        let controller = configured();
        let claim = controller
            .claim_due(100)
            .unwrap()
            .expect("due responsibility");
        assert_eq!(claim.responsibility.last_attempt_ms, Some(100));
        let restarted = Controller::new(controller.db.clone());
        assert!(restarted.claim_due(101).unwrap().is_none());
        assert!(
            controller
                .command(Command::Snapshot)
                .unwrap()
                .mcp
                .responsibilities[0]
                .next_due_ms
                >= 600_100
        );
    }
    #[test]
    fn background_run_claims_new_readable_source_without_turn_on() {
        let controller = configured();
        {
            let db = controller.db.lock().unwrap();
            let mut state = store::load(&db).unwrap();
            state.mcp.responsibilities.clear();
            state.mcp.connections[0].tools[0].read_only = true;
            state.mcp.connections[0].tools[0].name = "list_issues".into();
            state.mcp.grants[0].tool_name = "list_issues".into();
            store::save(&db, &state).unwrap();
        }
        let claim = controller.claim_due(100).unwrap().expect("automatic watch");
        assert!(claim.run_id.starts_with("watch:"));
        assert!(claim.responsibility.enabled);
        assert!(!claim.responsibility.prepare_low_risk);
        assert_eq!(claim.responsibility.connection_ids, ["c"]);
        assert!(controller.claim_due(101).unwrap().is_none());
    }
    #[test]
    fn responsibility_skills_are_scoped_and_changed_files_fail_before_launch() {
        let controller = configured();
        let temp = std::env::temp_dir().join(format!("neko-wake-skill-test-{}", store::new_id()));
        let folder = temp.join(".neko/skills/wake");
        std::fs::create_dir_all(&folder).unwrap();
        let path = folder.join("SKILL.md");
        std::fs::write(&path, "RESPONSIBILITY_SKILL_SENTINEL").unwrap();
        let state = {
            let db = controller.db.lock().unwrap();
            let mut state = store::load(&db).unwrap();
            state.workspaces[0].repository = temp.to_string_lossy().into_owned();
            store::save(&db, &state).unwrap();
            let available = neko_core::skills::discover(&[neko_core::skills::Root {
                path: folder,
                source: "Test".into(),
                workspace_id: Some("w".into()),
            }]);
            let skill = &available[0];
            neko_core::skills::set_enabled(
                &db,
                &available,
                &["w".into()],
                "w",
                &skill.path,
                &skill.content_hash,
                true,
            )
            .unwrap();
            state
        };
        assert!(
            controller
                .responsibility_skills(&state, "w")
                .unwrap()
                .contains("RESPONSIBILITY_SKILL_SENTINEL")
        );
        // Global local skills may be advertised in every workspace, but the
        // enabled body from this workspace must not leak into another one.
        assert!(
            !controller
                .responsibility_skills(&state, "other")
                .unwrap()
                .contains("RESPONSIBILITY_SKILL_SENTINEL")
        );
        std::fs::write(&path, "Changed instructions").unwrap();
        controller.responsibility_tick().unwrap();
        let failed = controller.command(Command::Snapshot).unwrap();
        assert!(
            failed.mcp.responsibilities[0]
                .last_result
                .contains("changed")
        );
        assert_eq!(failed.mcp.responsibilities[0].failures, 1);
        std::fs::remove_file(path).unwrap();
        assert!(
            controller
                .responsibility_skills(&state, "w")
                .unwrap_err()
                .contains("unavailable")
        );
        std::fs::remove_dir_all(temp).unwrap();
    }
    #[test]
    fn invalid_result_records_failure_and_backoff_without_creating_tasks() {
        let controller = configured();
        let claim = controller.claim_due(100).unwrap().unwrap();
        controller.finish_wake(&claim, Ok("not JSON"), 101).unwrap();
        let state = controller.command(Command::Snapshot).unwrap();
        assert_eq!(state.tasks.len(), 1);
        assert_eq!(state.mcp.responsibilities[0].failures, 1);
        assert!(state.mcp.responsibilities[0].next_due_ms >= 1_200_101);
    }
    #[test]
    fn paused_responsibility_cannot_accept_late_result() {
        let controller = configured();
        let claim = controller.claim_due(100).unwrap().unwrap();
        {
            let db = controller
                .db
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let mut state = store::load(&db).unwrap();
            state.mcp.responsibilities[0].enabled = false;
            store::save(&db, &state).unwrap();
        }
        controller
            .finish_wake(&claim, Ok(r#"{"summary":"late","observations":[]}"#), 101)
            .unwrap();
        assert!(
            !controller
                .command(Command::Snapshot)
                .unwrap()
                .mcp
                .responsibilities[0]
                .last_result
                .contains("late")
        );
    }
    #[test]
    fn scoped_lease_is_revoked_during_pause_and_after_return() {
        let controller = configured();
        let state = controller.command(Command::Snapshot).unwrap();
        let authority = RunAuthority::new(
            &state,
            "w",
            vec!["c".into()],
            Some(state.mcp.responsibilities[0].clone()),
        )
        .unwrap();
        let cancel = AtomicBool::new(false);
        let mut token = String::new();
        let result = controller.with_lease("run", &authority, &cancel, |bridge| {
            token = bridge.token.clone();
            let request = || neko_protocol::mcp_host::BridgeRequest {
                token: Secret(bridge.token.clone()),
                action: neko_protocol::mcp_host::BridgeAction::List,
            };
            assert!(controller.mcp.bridge(request()).unwrap().contains("lookup"));
            let mut r = state.mcp.responsibilities[0].clone();
            r.enabled = false;
            controller
                .command(Command::Mcp(
                    neko_protocol::mcp_host::McpCommand::SaveResponsibility { responsibility: r },
                ))
                .unwrap();
            let deadline = std::time::Instant::now() + Duration::from_secs(1);
            while !cancel.load(Ordering::Acquire) && std::time::Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(5));
            }
            assert!(cancel.load(Ordering::Acquire));
            assert!(controller.mcp.bridge(request()).is_err());
            Ok("late".into())
        });
        assert!(result.is_err());
        assert!(
            controller
                .mcp
                .bridge(neko_protocol::mcp_host::BridgeRequest {
                    token: Secret(token),
                    action: neko_protocol::mcp_host::BridgeAction::List
                })
                .is_err()
        );
    }
    #[test]
    fn cancelling_task_revokes_the_same_active_bridge() {
        let controller = configured();
        {
            let db = controller
                .db
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let mut state = store::load(&db).unwrap();
            state.tasks[0].status = TaskStatus::Planning;
            store::save(&db, &state).unwrap();
        }
        let state = controller.command(Command::Snapshot).unwrap();
        let authority = RunAuthority::for_task(&state, &state.tasks[0]).unwrap();
        let cancel = Arc::new(AtomicBool::new(false));
        controller
            .active
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(Active {
                task_id: "t".into(),
                cancelled: cancel.clone(),
            });
        let result = controller.with_lease("task-run", &authority, &cancel, |bridge| {
            controller
                .command(Command::CancelTask {
                    task_id: "t".into(),
                })
                .unwrap();
            let request = || neko_protocol::mcp_host::BridgeRequest {
                token: Secret(bridge.token.clone()),
                action: neko_protocol::mcp_host::BridgeAction::List,
            };
            let deadline = std::time::Instant::now() + Duration::from_secs(1);
            while controller.mcp.bridge(request()).is_ok() && std::time::Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(5));
            }
            assert!(controller.mcp.bridge(request()).is_err());
            Ok("late".into())
        });
        assert!(result.is_err());
    }
    #[test]
    fn scoped_run_rejects_revoked_grants_before_model_launch() {
        let controller = configured();
        let state = controller.command(Command::Snapshot).unwrap();
        let authority = RunAuthority::new(
            &state,
            "w",
            vec!["c".into()],
            Some(state.mcp.responsibilities[0].clone()),
        )
        .unwrap();
        {
            let db = controller
                .db
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let mut state = store::load(&db).unwrap();
            state.mcp.grants.clear();
            store::save(&db, &state).unwrap();
        }
        assert!(
            controller
                .with_lease("run", &authority, &AtomicBool::new(false), |_| panic!(
                    "revoked authority must not launch a model"
                ))
                .is_err()
        );
    }
    #[test]
    fn automatic_permission_must_survive_until_builder_launch() {
        for revoke in 0..4 {
            let controller = configured();
            let now = store::now_ms();
            let claim = controller.claim_due(now).unwrap().unwrap();
            {
                let db = controller
                    .db
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                let mut state = store::load(&db).unwrap();
                state
                    .mcp
                    .receipts
                    .push(neko_protocol::mcp_host::ToolReceipt {
                        id: "proof".into(),
                        run_id: claim.run_id.clone(),
                        workspace_id: "w".into(),
                        connection_id: "c".into(),
                        tool_name: "lookup".into(),
                        schema_hash: "hash".into(),
                        at_ms: now,
                        success: true,
                    });
                store::save(&db, &state).unwrap();
            }
            controller.finish_wake(&claim, Ok(r#"{"summary":"Checked","observations":[{"connection_id":"c","external_id":"item","revision":"v1","title":"Bug","description":"Current bug","receipt_ids":["proof"],"eligible":true}]}"#), now).unwrap();
            let mut state = controller.command(Command::Snapshot).unwrap();
            let task = &mut state.tasks[1];
            task.status = TaskStatus::AwaitingApproval;
            task.plan = "Fix label".into();
            task.supervision = Some(serde_json::from_str(r#"{"action":"prepare_fix","risk":"low","is_bug":true,"reason":"Bounded","evidence":["src/label.rs:1"],"files":["src/label.rs"],"tests":["cargo test label"],"sensitive_areas":[],"uncertainties":[],"plan":"Fix label"}"#).unwrap());
            assert!(responsibility::may_prepare(&state, &state.tasks[1], now));
            let authority = RunAuthority::for_task(&state, &state.tasks[1]).unwrap();
            state.tasks[1].status = TaskStatus::Building;
            let original = state.clone();
            let task_claim = TaskClaim {
                task: state.tasks[1].clone(),
                authority: authority.clone(),
            };
            match revoke {
                0 => state.mcp.responsibilities[0].prepare_low_risk = false,
                1 => state.mcp.responsibilities[0].instruction = "Different scope".into(),
                2 => state.mcp.sources[0].eligible = false,
                _ => {
                    state.mcp.sources[0].retrieved_ms = 1;
                    state.mcp.receipts[0].at_ms = 1;
                }
            }
            store::save(
                &controller
                    .db
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner),
                &state,
            )
            .unwrap();
            assert!(
                controller
                    .with_lease("builder", &authority, &AtomicBool::new(false), |_| panic!(
                        "revoked automatic authority must never launch builder"
                    ))
                    .is_err(),
                "case {revoke}"
            );
            let error = controller
                .execute_with_worktree(&task_claim, &AtomicBool::new(false), |_, _, _| {
                    panic!("revoked claim must not start worktree creation")
                })
                .unwrap_err();
            assert!(error.contains("before execution"));

            // Reproduce revocation during a slow checkout. The returned path
            // is synthetic: the post-checkout guard must stop before any model.
            store::save(
                &controller
                    .db
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner),
                &original,
            )
            .unwrap();
            let error = controller
                .execute_with_worktree(&task_claim, &AtomicBool::new(false), |_, _, _| {
                    store::save(
                        &controller
                            .db
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner),
                        &state,
                    )
                    .unwrap();
                    Ok(std::path::PathBuf::from(
                        "/synthetic/worktree-never-executed",
                    ))
                })
                .unwrap_err();
            assert!(
                error.contains("during worktree setup"),
                "case {revoke}: {error}"
            );
            if revoke == 0 {
                // An explicit user-approved Building task does not depend on
                // standing prepare_low_risk permission at its own claim time.
                let explicit = RunAuthority::for_task(&state, &state.tasks[1]).unwrap();
                assert!(explicit.automatic_task.is_none());
                assert!(explicit.valid(&state));
            }
        }
    }
    #[test]
    fn edited_instruction_rejects_old_run_even_with_valid_receipts() {
        let controller = configured();
        let claim = controller.claim_due(100).unwrap().unwrap();
        {
            let db = controller
                .db
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let mut state = store::load(&db).unwrap();
            state
                .mcp
                .receipts
                .push(neko_protocol::mcp_host::ToolReceipt {
                    id: "proof".into(),
                    run_id: claim.run_id.clone(),
                    workspace_id: "w".into(),
                    connection_id: "c".into(),
                    tool_name: "lookup".into(),
                    schema_hash: "hash".into(),
                    at_ms: 100,
                    success: true,
                });
            state.mcp.responsibilities[0].instruction =
                "Only investigate a different project now".into();
            store::save(&db, &state).unwrap();
        }
        let raw = r#"{"summary":"Old instruction completed","observations":[{"connection_id":"c","external_id":"item","revision":"v1","title":"Bug","description":"Current bug","receipt_ids":["proof"],"eligible":true}]}"#;
        controller.finish_wake(&claim, Ok(raw), 101).unwrap();
        let state = controller.command(Command::Snapshot).unwrap();
        assert_eq!(state.tasks.len(), 1);
        assert!(state.mcp.sources.is_empty());
        assert!(
            !state.mcp.responsibilities[0]
                .last_result
                .contains("Old instruction")
        );
    }
    #[test]
    fn current_receipts_commit_observations_once() {
        let controller = configured();
        let claim = controller.claim_due(100).unwrap().unwrap();
        {
            let db = controller
                .db
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let mut state = store::load(&db).unwrap();
            state
                .mcp
                .receipts
                .push(neko_protocol::mcp_host::ToolReceipt {
                    id: "proof".into(),
                    run_id: claim.run_id.clone(),
                    workspace_id: "w".into(),
                    connection_id: "c".into(),
                    tool_name: "lookup".into(),
                    schema_hash: "hash".into(),
                    at_ms: 100,
                    success: true,
                });
            store::save(&db, &state).unwrap();
        }
        let raw = r#"{"summary":"Checked","observations":[{"connection_id":"c","external_id":"item","revision":"v1","title":"Bug","description":"Current bug","receipt_ids":["proof"],"eligible":true}]}"#;
        controller.finish_wake(&claim, Ok(raw), 101).unwrap();
        let prior = controller.command(Command::Snapshot).unwrap();
        let authority = RunAuthority::for_task(&prior, &prior.tasks[1]).unwrap();
        controller.finish_wake(&claim, Ok(raw), 102).unwrap();
        let state = controller.command(Command::Snapshot).unwrap();
        assert!(
            authority.valid(&state),
            "refreshing identical source content must not cancel ongoing work"
        );
        assert_eq!(state.tasks.len(), 2);
        assert_eq!(state.mcp.sources.len(), 1);
        assert_eq!(state.mcp.responsibilities[0].failures, 0);
    }
}
