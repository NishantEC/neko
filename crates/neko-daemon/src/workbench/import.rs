//! Read-only previews hold source secrets only in memory. Applying a reviewed
//! selection uses the normal Keychain/controller boundary, never starts tools.
use super::*;
use neko_core::setup_import::{self, Discovery};
use neko_protocol::setup_import::{ImportCandidateKind, ImportCommand, ImportPreview};
use std::{collections::HashSet, path::PathBuf};

#[derive(Default)]
pub(super) struct Importer {
    session: Option<Session>,
}
struct Session {
    id: String,
    expires_ms: i64,
    discovery: Discovery,
    applied: HashSet<String>,
}
impl Importer {
    pub(super) fn evict_expired(&mut self, now: i64) {
        if self.session.as_ref().is_some_and(|s| now >= s.expires_ms) {
            self.session = None;
        }
    }
}

impl Session {
    fn validate(
        &self,
        id: &str,
        connections: &[String],
        skills: &[String],
        workspaces: &[String],
        repositories: &[String],
        existing_repositories: &HashSet<String>,
        global_connection_ids: &[String],
        _trust: bool,
    ) -> Result<(), String> {
        if self.id != id || store::now_ms() >= self.expires_ms {
            return Err("Import preview expired. Scan again before applying.".into());
        }
        if connections.len() > 100
            || global_connection_ids.len() > 100
            || skills.len() > 100
            || workspaces.len() > 100
            || repositories.len() > 100
        {
            return Err("Too many import selections".into());
        }
        let preview = self.discovery.preview();
        for skill_id in skills {
            let candidate = preview
                .candidates
                .iter()
                .find(|candidate| candidate.id == *skill_id)
                .filter(|candidate| {
                    candidate.kind == neko_protocol::setup_import::ImportCandidateKind::Skill
                })
                .ok_or("Skill was not part of the reviewed preview")?;
            if !candidate.metadata.contains_key("path")
                || !candidate.metadata.contains_key("content_hash")
            {
                return Err("Skill preview is incomplete; scan again".into());
            }
            if let Some(repository) = candidate.workspace.as_deref() {
                let workspace_selected = workspaces.iter().any(|workspace_id| {
                    preview.candidates.iter().any(|workspace| {
                        workspace.id == *workspace_id
                            && workspace.kind
                                == neko_protocol::setup_import::ImportCandidateKind::Workspace
                            && workspace.workspace.as_deref() == Some(repository)
                    })
                });
                if !workspace_selected && !existing_repositories.contains(repository) {
                    return Err("Select the skill's workspace before importing it".into());
                }
            }
        }
        for workspace_id in workspaces {
            let candidate = preview
                .candidates
                .iter()
                .find(|candidate| candidate.id == *workspace_id)
                .filter(|candidate| {
                    candidate.kind == neko_protocol::setup_import::ImportCandidateKind::Workspace
                })
                .ok_or("Workspace was not part of the reviewed preview")?;
            if candidate.workspace.is_none() {
                return Err("Workspace preview is incomplete; scan again".into());
            }
        }
        for repository in repositories {
            if !self.discovery.repositories.contains(repository) {
                return Err("Repository was not part of the reviewed preview".into());
            }
        }
        for id in connections {
            let candidate = self
                .discovery
                .candidates
                .iter()
                .find(|c| c.preview.id == *id)
                .ok_or("Connection was not part of the reviewed preview")?;
            candidate
                .config
                .as_ref()
                .ok_or("Resolve unsupported configuration before importing")?;
            if let Some(repository) = candidate.preview.repository.as_deref() {
                let import_globally = global_connection_ids.contains(id);
                let selected_workspace = workspaces.iter().any(|workspace_id| {
                    preview.candidates.iter().any(|workspace| {
                        workspace.id == *workspace_id
                            && workspace.kind == ImportCandidateKind::Workspace
                            && workspace.workspace.as_deref() == Some(repository)
                    })
                }) || repositories.iter().any(|path| path == repository);
                let existing_workspace =
                    std::fs::canonicalize(repository).ok().is_some_and(|path| {
                        existing_repositories.contains(&path.to_string_lossy().into_owned())
                    });
                if !import_globally && !selected_workspace && !existing_workspace {
                    return Err(
                        "Select the connection's workspace or explicitly keep it global".into(),
                    );
                }
            }
        }
        for id in global_connection_ids {
            if !connections.contains(id)
                || !self.discovery.candidates.iter().any(|candidate| {
                    candidate.preview.id == *id && candidate.preview.repository.is_some()
                })
            {
                return Err("Global fallback was not selected for a scoped connection".into());
            }
        }
        Ok(())
    }
}

fn mark_existing_items(preview: &mut ImportPreview, discovery: &Discovery, state: &Snapshot) {
    for item in &mut preview.candidates {
        let workspace_id = item.workspace.as_deref().and_then(|path| {
            let canonical = std::fs::canonicalize(path).ok()?;
            state
                .workspaces
                .iter()
                .find(|workspace| std::path::Path::new(&workspace.repository) == canonical)
                .map(|workspace| workspace.id.as_str())
        });
        let existing = match item.kind {
            ImportCandidateKind::Workspace => workspace_id.is_some(),
            ImportCandidateKind::Skill => {
                let hash = item.metadata.get("content_hash");
                let expected_workspace = workspace_id.unwrap_or("");
                (item.workspace.is_none() || workspace_id.is_some())
                    && state.skills.proposals.iter().any(|skill| {
                        skill.name == item.name
                            && Some(&skill.content_hash) == hash
                            && skill.workspace_id == expected_workspace
                    })
            }
            ImportCandidateKind::Connection => discovery
                .candidates
                .iter()
                .find(|candidate| candidate.preview.id == item.id)
                .and_then(|candidate| candidate.config.as_ref())
                .is_some_and(|config| {
                    let expected_workspace = workspace_id.unwrap_or("");
                    (item.workspace.is_none() || workspace_id.is_some())
                        && state.mcp.connections.iter().any(|connection| {
                            connection.workspace_id == expected_workspace
                                && connection.label == item.name
                                && &connection.config == config
                        })
                }),
            ImportCandidateKind::Schedule => false,
        };
        if existing {
            item.metadata
                .insert("already_in_neko".into(), "true".into());
        } else {
            let same_name = match item.kind {
                ImportCandidateKind::Connection => state
                    .mcp
                    .connections
                    .iter()
                    .any(|connection| connection.label == item.name),
                ImportCandidateKind::Skill => state
                    .skills
                    .proposals
                    .iter()
                    .any(|skill| skill.name == item.name),
                ImportCandidateKind::Workspace => state
                    .workspaces
                    .iter()
                    .any(|workspace| workspace.name == item.name),
                ImportCandidateKind::Schedule => false,
            };
            if same_name {
                item.metadata
                    .insert("same_name_different_setup".into(), "true".into());
            }
        }
    }
}

impl Controller {
    pub(super) fn import_command(&self, command: ImportCommand) -> Result<Snapshot, String> {
        self.import_command_with_schedule_resolver(command, |repository| {
            std::fs::canonicalize(repository).ok()
        })
    }

    fn import_command_with_schedule_resolver(
        &self,
        command: ImportCommand,
        resolve: impl Fn(&str) -> Option<PathBuf>,
    ) -> Result<Snapshot, String> {
        let mut importer = self
            .importer
            .try_lock()
            .map_err(|_| "Another import is in progress")?;
        importer.evict_expired(store::now_ms());
        match command {
            ImportCommand::ListSources => {
                let home = std::env::var_os("HOME")
                    .map(PathBuf::from)
                    .ok_or("Home directory unavailable")?;
                let db = self.db.lock().map_err(|_| "Import storage unavailable")?;
                let mut preview = neko_protocol::setup_import::ImportPreview::default();
                preview.sources = setup_import::available_sources(&home);
                setup_import::save_preview(&db, &preview)?;
                importer.session = None;
                store::load(&db)
            }
            ImportCommand::Discover {
                repositories,
                source_id,
            } => {
                if repositories.len() > 100
                    || repositories
                        .iter()
                        .any(|p| p.len() > 4096 || !std::path::Path::new(p).is_absolute())
                {
                    return Err("Choose at most 100 absolute repository paths".into());
                }
                let home = std::env::var_os("HOME")
                    .map(PathBuf::from)
                    .ok_or("Home directory unavailable")?;
                let paths = std::env::var_os("PATH")
                    .map(|p| std::env::split_paths(&p).collect::<Vec<_>>())
                    .unwrap_or_default();
                let environment = std::env::vars_os()
                    .filter_map(|(key, value)| {
                        Some((key.into_string().ok()?, value.into_string().ok()?))
                    })
                    .collect();
                let repositories = repositories
                    .into_iter()
                    .map(PathBuf::from)
                    .collect::<Vec<_>>();
                let discovery = match source_id.as_deref() {
                    Some(source) => setup_import::discover_source(
                        &home,
                        &repositories,
                        &paths,
                        &environment,
                        source,
                    )?,
                    None => setup_import::discover(&home, &repositories, &paths, &environment),
                };
                let id = store::new_id();
                let mut preview = discovery.preview();
                preview.preview_id = id.clone();
                preview.sources = setup_import::available_sources(&home);
                preview.active_source = source_id;
                let db = self.db.lock().map_err(|_| "Import storage unavailable")?;
                mark_existing_items(&mut preview, &discovery, &store::load(&db)?);
                setup_import::save_preview(&db, &preview)?;
                importer.session = Some(Session {
                    id,
                    expires_ms: store::now_ms() + 600_000,
                    discovery,
                    applied: HashSet::new(),
                });
                store::load(&db)
            }
            ImportCommand::Apply {
                preview_id,
                connection_ids,
                global_connection_ids,
                schedule_ids,
                skill_ids,
                workspace_ids,
                repositories,
                include_credentials,
                trust_local_processes,
            } => {
                let existing_repositories = {
                    let db = self.db.lock().map_err(|_| "Import storage unavailable")?;
                    store::load(&db)?
                        .workspaces
                        .into_iter()
                        .map(|workspace| workspace.repository)
                        .collect::<HashSet<_>>()
                };
                let session = importer
                    .session
                    .as_mut()
                    .ok_or("Scan again; source credentials are not retained across restart")?;
                session.validate(
                    &preview_id,
                    &connection_ids,
                    &skill_ids,
                    &workspace_ids,
                    &repositories,
                    &existing_repositories,
                    &global_connection_ids,
                    trust_local_processes,
                )?;
                if schedule_ids.len() > 100
                    || schedule_ids
                        .iter()
                        .any(|id| !session.discovery.schedules.iter().any(|s| &s.id == id))
                {
                    return Err("Schedule was not part of the reviewed preview".into());
                }
                let preview = session.discovery.preview();
                let selected_repositories = workspace_ids
                    .iter()
                    .filter_map(|id| {
                        preview
                            .candidates
                            .iter()
                            .find(|candidate| candidate.id == *id)
                            .and_then(|candidate| candidate.workspace.clone())
                    })
                    .chain(repositories.iter().cloned())
                    .collect::<Vec<_>>();
                // Validate the complete selection before the first SaveWorkspace;
                // otherwise a later non-Git directory leaves earlier imports applied.
                let selected_repositories = selected_repositories
                    .into_iter()
                    .map(|repository| {
                        let canonical =
                            neko_core::workbench::canonical_workspace_directory(&repository)
                                .map_err(|error| {
                                    let name = std::path::Path::new(&repository)
                                        .file_name()
                                        .and_then(|name| name.to_str())
                                        .unwrap_or("selected folder");
                                    format!("Cannot import workspace {name}: {error}")
                                })?;
                        Ok((repository, canonical))
                    })
                    .collect::<Result<Vec<_>, String>>()?;
                for (repository, canonical) in selected_repositories {
                    let db = self.db.lock().map_err(|_| "Import storage unavailable")?;
                    let state = store::load(&db)?;
                    if !state.workspaces.iter().any(|w| w.repository == canonical) {
                        store::apply(
                            &db,
                            Command::SaveWorkspace {
                                workspace: Workspace {
                                    id: String::new(),
                                    name: std::path::Path::new(&repository)
                                        .file_name()
                                        .and_then(|s| s.to_str())
                                        .unwrap_or("Imported workspace")
                                        .into(),
                                    repository: canonical,
                                    instructions: String::new(),
                                    away_enabled: false,
                                },
                            },
                        )?;
                    }
                }
                for skill_id in skill_ids {
                    let candidate = preview
                        .candidates
                        .iter()
                        .find(|candidate| candidate.id == skill_id)
                        .ok_or("Skill candidate missing")?;
                    let path = candidate.metadata.get("path").ok_or("Skill path missing")?;
                    let content_hash = candidate
                        .metadata
                        .get("content_hash")
                        .ok_or("Skill hash missing")?;
                    let body = setup_import::read_skill_for_import(path, content_hash)?;
                    let state =
                        store::load(&*self.db.lock().map_err(|_| "Import storage unavailable")?)?;
                    let workspace_id = candidate
                        .workspace
                        .as_deref()
                        .and_then(|repository| {
                            let canonical = std::fs::canonicalize(repository)
                                .ok()?
                                .to_string_lossy()
                                .into_owned();
                            state
                                .workspaces
                                .iter()
                                .find(|workspace| workspace.repository == canonical)
                                .map(|workspace| workspace.id.clone())
                        })
                        .unwrap_or_default();
                    if state.skills.proposals.iter().any(|skill| {
                        skill.workspace_id == workspace_id
                            && skill.name == candidate.name
                            && skill.content_hash == *content_hash
                    }) {
                        continue;
                    }
                    let db = self.db.lock().map_err(|_| "Import storage unavailable")?;
                    neko_core::skills::propose(
                        &db,
                        &workspace_id,
                        &candidate.name,
                        &body,
                        path,
                        None,
                    )?;
                }
                for id in connection_ids {
                    if session.applied.contains(&id) {
                        continue;
                    }
                    let candidate = session
                        .discovery
                        .candidates
                        .iter()
                        .find(|c| c.preview.id == id)
                        .ok_or("Import candidate missing")?;
                    let config = candidate
                        .config
                        .clone()
                        .ok_or("Import candidate unsupported")?;
                    let state =
                        store::load(&*self.db.lock().map_err(|_| "Import storage unavailable")?)?;
                    let workspace_id = if global_connection_ids.contains(&id) {
                        String::new()
                    } else if let Some(repository) = &candidate.preview.repository {
                        let canonical = std::fs::canonicalize(repository)
                            .map_err(|_| "Import workspace unavailable")?
                            .to_string_lossy()
                            .into_owned();
                        state
                            .workspaces
                            .iter()
                            .find(|w| w.repository == canonical)
                            .ok_or("Select the connection's workspace for import first")?
                            .id
                            .clone()
                    } else {
                        String::new()
                    };
                    // Preserve an existing definition, including its credentials and
                    // permissions, rather than silently overwriting it on reimport.
                    if state.mcp.connections.iter().any(|c| {
                        c.workspace_id == workspace_id
                            && c.label == candidate.preview.name
                            && c.config == config
                    }) {
                        session.applied.insert(id);
                        continue;
                    }
                    self.mcp.add_connection(
                        workspace_id,
                        candidate.preview.name.clone(),
                        config,
                        trust_local_processes,
                        if include_credentials {
                            candidate.credentials.clone()
                        } else {
                            None
                        },
                        false,
                        true,
                    )?;
                    session.applied.insert(id);
                }
                // Resolve only the bounded reviewed selection before acquiring
                // the shared DB mutex: a filesystem/mount can stall here.
                let resolved_schedules = schedule_ids
                    .iter()
                    .map(|id| {
                        let candidate = session
                            .discovery
                            .schedules
                            .iter()
                            .find(|s| &s.id == id)
                            .ok_or("Schedule candidate missing")?;
                        let repository = candidate.repository.as_deref().and_then(&resolve);
                        Ok((candidate, repository))
                    })
                    .collect::<Result<Vec<_>, String>>()?;
                {
                    let db = self.db.lock().map_err(|_| "Import storage unavailable")?;
                    let mut state = store::load(&db)?;
                    for (candidate, repository) in resolved_schedules {
                        if state
                            .schedules
                            .iter()
                            .any(|s| s.source_id.as_deref() == Some(&candidate.id))
                        {
                            continue;
                        }
                        let workspace_id = repository.and_then(|path| {
                            state
                                .workspaces
                                .iter()
                                .find(|w| std::path::Path::new(&w.repository) == path)
                                .map(|w| w.id.clone())
                        });
                        neko_core::scheduled_plans::apply(
                            &mut state,
                            neko_protocol::scheduled_plans::ScheduleCommand::Save {
                                schedule: neko_protocol::scheduled_plans::Schedule {
                                    name: candidate.name.clone(),
                                    prompt: candidate.prompt.clone(),
                                    workspace_id,
                                    rule: candidate.rule.clone(),
                                    timezone: candidate.timezone.clone(),
                                    anchor_ms: candidate.anchor_ms,
                                    ..Default::default()
                                },
                            },
                        )?;
                        let saved = state.schedules.last_mut().ok_or("Schedule import failed")?;
                        saved.source_id = Some(candidate.id.clone());
                        saved.last_result = candidate.warnings.join(" ");
                    }
                    store::save(&db, &state)?;
                }
                let db = self.db.lock().map_err(|_| "Import storage unavailable")?;
                let state = store::load(&db)?;
                neko_core::skills::refresh(&db, &state.workspaces)?;
                let mut preview = setup_import::load_preview(&db)?;
                preview.warnings = vec!["Selected definitions imported. Existing matching connections were preserved. No tools were launched or granted; review credentials, skills and permissions before use.".into()];
                setup_import::save_preview(&db, &preview)?;
                store::load(&db)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_workspace_connection_requires_explicit_global_fallback() {
        let controller = super::super::tests::controller_with_task(TaskStatus::Completed);
        let discovery = Discovery {
            candidates: vec![setup_import::Candidate {
                preview: neko_protocol::setup_import::ImportConnection {
                    id: "scoped".into(),
                    name: "example".into(),
                    source: "Codex".into(),
                    repository: Some("/missing/workspace".into()),
                    has_credentials: false,
                    enabled_at_source: true,
                    problem: None,
                },
                config: Some(ServerConfig::Http {
                    url: "https://example.com/mcp".into(),
                }),
                credentials: None,
            }],
            ..Default::default()
        };
        controller.importer.lock().unwrap().session = Some(Session {
            id: "preview".into(),
            expires_ms: store::now_ms() + 60_000,
            discovery,
            applied: HashSet::new(),
        });
        let apply = |global_connection_ids: Vec<String>| ImportCommand::Apply {
            preview_id: "preview".into(),
            connection_ids: vec!["scoped".into()],
            global_connection_ids,
            schedule_ids: vec![],
            skill_ids: vec![],
            workspace_ids: vec![],
            repositories: vec![],
            include_credentials: false,
            trust_local_processes: false,
        };
        assert!(controller.import_command(apply(vec![])).is_err());
        let state = controller
            .import_command(apply(vec!["scoped".into()]))
            .unwrap();
        let imported = state
            .mcp
            .connections
            .iter()
            .find(|c| c.label == "example")
            .unwrap();
        assert!(imported.workspace_id.is_empty());
        assert!(!imported.enabled);
        assert!(state.mcp.grants.is_empty());
    }

    #[test]
    fn existing_connection_is_marked_on_its_own_row_only_for_same_definition() {
        let config = neko_protocol::mcp_host::ServerConfig::Http {
            url: "https://example.com/mcp".into(),
        };
        let discovery = Discovery {
            candidates: vec![setup_import::Candidate {
                preview: neko_protocol::setup_import::ImportConnection {
                    id: "one".into(),
                    name: "Example".into(),
                    source: "Codex".into(),
                    repository: None,
                    has_credentials: false,
                    enabled_at_source: true,
                    problem: None,
                },
                config: Some(config.clone()),
                credentials: None,
            }],
            ..Default::default()
        };
        let mut state = Snapshot::default();
        state
            .mcp
            .connections
            .push(neko_protocol::mcp_host::McpConnection {
                oauth: false,
                id: "existing".into(),
                workspace_id: String::new(),
                label: "Example".into(),
                config: config.clone(),
                enabled: false,
                trusted: false,
                has_credentials: false,
                tools: Vec::new(),
                discovered_ms: None,
                error: None,
            });
        let mut preview = discovery.preview();
        mark_existing_items(&mut preview, &discovery, &state);
        assert_eq!(
            preview.candidates[0]
                .metadata
                .get("already_in_neko")
                .map(String::as_str),
            Some("true")
        );

        state.mcp.connections[0].config = neko_protocol::mcp_host::ServerConfig::Http {
            url: "https://different.example/mcp".into(),
        };
        let mut preview = discovery.preview();
        mark_existing_items(&mut preview, &discovery, &state);
        assert!(
            !preview.candidates[0]
                .metadata
                .contains_key("already_in_neko")
        );
        assert_eq!(
            preview.candidates[0]
                .metadata
                .get("same_name_different_setup")
                .map(String::as_str),
            Some("true")
        );
    }
    use neko_protocol::mcp_host::ServerConfig;
    #[test]
    fn schedule_paths_resolve_without_db_lock_and_match_fresh_workspace_state() {
        let controller = super::super::tests::controller_with_task(TaskStatus::Completed);
        let discovery = Discovery {
            schedules: vec![neko_protocol::setup_import::ImportSchedule {
                id: "source".into(),
                name: "Daily review".into(),
                prompt: "Review changes".into(),
                source: "fixture".into(),
                repository: Some("/source-alias".into()),
                rule: "FREQ=DAILY".into(),
                timezone: String::new(),
                anchor_ms: 0,
                warnings: vec![],
            }],
            ..Default::default()
        };
        controller.importer.lock().unwrap().session = Some(Session {
            id: "preview".into(),
            expires_ms: store::now_ms() + 60_000,
            discovery,
            applied: HashSet::new(),
        });
        let command = ImportCommand::Apply {
            preview_id: "preview".into(),
            connection_ids: vec![],
            global_connection_ids: vec![],
            schedule_ids: vec!["source".into()],
            skill_ids: vec![],
            workspace_ids: vec![],
            repositories: vec![],
            include_credentials: false,
            trust_local_processes: false,
        };
        let state = controller
            .import_command_with_schedule_resolver(command, |repository| {
                assert_eq!(repository, "/source-alias");
                let db = controller
                    .db
                    .try_lock()
                    .expect("Filesystem resolution must not hold the shared database mutex");
                // Simulate workspace state changing during slow filesystem I/O.
                let mut state = store::load(&db).unwrap();
                state.workspaces[0].repository = "/resolved-repository".into();
                store::save(&db, &state).unwrap();
                Some(PathBuf::from("/resolved-repository"))
            })
            .unwrap();
        assert_eq!(state.schedules[0].workspace_id.as_deref(), Some("w"));
        assert!(!state.schedules[0].enabled);
    }
    #[test]
    fn schedule_import_is_paused_scope_missing_and_preserves_user_edits_on_reimport() {
        let controller = super::super::tests::controller_with_task(TaskStatus::Completed);
        let discovery = Discovery {
            schedules: vec![neko_protocol::setup_import::ImportSchedule {
                id: "source".into(),
                name: "Daily review".into(),
                prompt: "Review changes".into(),
                source: "fixture".into(),
                repository: Some("/not-a-real-repository".into()),
                rule: "FREQ=DAILY".into(),
                timezone: String::new(),
                anchor_ms: 0,
                warnings: vec!["Needs workspace".into()],
            }],
            ..Default::default()
        };
        controller.importer.lock().unwrap().session = Some(Session {
            id: "preview".into(),
            expires_ms: store::now_ms() + 60_000,
            discovery,
            applied: HashSet::new(),
        });
        let command = || ImportCommand::Apply {
            preview_id: "preview".into(),
            connection_ids: vec![],
            global_connection_ids: vec![],
            schedule_ids: vec!["source".into()],
            skill_ids: vec![],
            workspace_ids: vec![],
            repositories: vec![],
            include_credentials: false,
            trust_local_processes: false,
        };
        let state = controller.import_command(command()).unwrap();
        assert_eq!(state.schedules.len(), 1);
        assert!(!state.schedules[0].enabled);
        assert!(state.schedules[0].workspace_id.is_none());
        assert!(state.mcp.grants.is_empty());
        assert_eq!(state.tasks.len(), 1);
        let mut edited = state.schedules[0].clone();
        edited.prompt = "My own updated instruction".into();
        controller
            .schedule_command(neko_protocol::scheduled_plans::ScheduleCommand::Save {
                schedule: edited,
            })
            .unwrap();
        let state = controller.import_command(command()).unwrap();
        assert_eq!(state.schedules.len(), 1);
        assert_eq!(state.schedules[0].prompt, "My own updated instruction");
        assert!(
            controller
                .import_command(ImportCommand::Apply {
                    preview_id: "preview".into(),
                    connection_ids: vec![],
                    global_connection_ids: vec![],
                    schedule_ids: vec!["not-reviewed".into()],
                    skill_ids: vec![],
                    workspace_ids: vec![],
                    repositories: vec![],
                    include_credentials: false,
                    trust_local_processes: false
                })
                .is_err()
        );
    }
    #[test]
    fn expired_session_releases_cached_discovery() {
        let mut importer = Importer {
            session: Some(Session {
                id: "x".into(),
                expires_ms: 10,
                discovery: Discovery::default(),
                applied: HashSet::new(),
            }),
        };
        importer.evict_expired(9);
        assert!(importer.session.is_some());
        importer.evict_expired(10);
        assert!(importer.session.is_none());
    }
    #[test]
    fn preview_selection_is_bound_to_session_and_allows_untrusted_local_definitions() {
        let mut discovery = Discovery::default();
        discovery
            .candidates
            .push(neko_core::setup_import::Candidate {
                preview: neko_protocol::setup_import::ImportConnection {
                    id: "c".into(),
                    name: "Server".into(),
                    source: "fixture".into(),
                    repository: None,
                    has_credentials: false,
                    enabled_at_source: true,
                    problem: None,
                },
                config: Some(ServerConfig::Stdio {
                    command: "/bin/echo".into(),
                    args: vec![],
                    cwd: None,
                }),
                credentials: None,
            });
        let session = Session {
            id: "review".into(),
            expires_ms: store::now_ms() + 60_000,
            discovery,
            applied: HashSet::new(),
        };
        let no_workspaces = HashSet::new();
        assert!(
            session
                .validate(
                    "wrong",
                    &["c".into()],
                    &[],
                    &[],
                    &[],
                    &no_workspaces,
                    &[],
                    true
                )
                .is_err()
        );
        assert!(
            session
                .validate(
                    "review",
                    &["unknown".into()],
                    &[],
                    &[],
                    &[],
                    &no_workspaces,
                    &[],
                    true
                )
                .is_err()
        );
        assert!(
            session
                .validate(
                    "review",
                    &["c".into()],
                    &[],
                    &[],
                    &[],
                    &no_workspaces,
                    &[],
                    false
                )
                .is_ok()
        );
        assert!(
            session
                .validate(
                    "review",
                    &["c".into()],
                    &[],
                    &[],
                    &[],
                    &no_workspaces,
                    &[],
                    true
                )
                .is_ok()
        );
        assert!(
            session
                .validate(
                    "review",
                    &[],
                    &[],
                    &[],
                    &["/not-reviewed".into()],
                    &no_workspaces,
                    &[],
                    true
                )
                .is_err()
        );
    }

    #[test]
    fn invalid_selected_workspace_does_not_partially_import_earlier_workspaces() {
        let controller = super::super::tests::controller_with_task(TaskStatus::Completed);
        let valid = tempfile::tempdir().unwrap();
        let invalid = tempfile::tempdir().unwrap();
        assert!(
            std::process::Command::new("git")
                .args(["init", "--quiet"])
                .current_dir(valid.path())
                .status()
                .unwrap()
                .success()
        );
        let discovery = Discovery {
            repositories: vec![
                valid.path().to_string_lossy().into_owned(),
                invalid
                    .path()
                    .join("missing")
                    .to_string_lossy()
                    .into_owned(),
            ],
            ..Default::default()
        };
        let workspace_ids = discovery
            .preview()
            .candidates
            .iter()
            .filter(|candidate| candidate.kind == ImportCandidateKind::Workspace)
            .map(|candidate| candidate.id.clone())
            .collect();
        controller.importer.lock().unwrap().session = Some(Session {
            id: "preview".into(),
            expires_ms: store::now_ms() + 60_000,
            discovery,
            applied: HashSet::new(),
        });
        let before = store::load(&controller.db.lock().unwrap())
            .unwrap()
            .workspaces
            .len();
        let result = controller.import_command(ImportCommand::Apply {
            preview_id: "preview".into(),
            connection_ids: vec![],
            global_connection_ids: vec![],
            schedule_ids: vec![],
            skill_ids: vec![],
            workspace_ids,
            repositories: vec![],
            include_credentials: false,
            trust_local_processes: false,
        });
        assert!(result.is_err());
        let after = store::load(&controller.db.lock().unwrap())
            .unwrap()
            .workspaces
            .len();
        assert_eq!(after, before);
    }

    #[test]
    fn selected_skill_is_reviewed_disabled_and_workspace_is_idempotent() {
        let controller = super::super::tests::controller_with_task(TaskStatus::Completed);
        let repository = std::env::temp_dir().join(format!("neko-import-{}", store::new_id()));
        std::fs::create_dir_all(&repository).unwrap();
        std::process::Command::new("git")
            .args(["init", "--quiet"])
            .current_dir(&repository)
            .output()
            .unwrap();
        let skill_dir = repository.join(".agents/skills/example");
        std::fs::create_dir_all(&skill_dir).unwrap();
        let skill_path = skill_dir.join("SKILL.md");
        std::fs::write(&skill_path, "---\nname: Example\n---\nReview me").unwrap();
        let repository = repository.canonicalize().unwrap();
        let skill = neko_core::skills::discover(&[neko_core::skills::Root {
            path: repository.join(".agents/skills"),
            source: "Workspace".into(),
            workspace_id: Some(repository.to_string_lossy().into_owned()),
        }])
        .into_iter()
        .find(|skill| skill.path == skill_path.canonicalize().unwrap().to_string_lossy())
        .unwrap();
        let discovery = Discovery {
            repositories: vec![repository.to_string_lossy().into_owned()],
            skills: vec![skill],
            ..Default::default()
        };
        let preview = discovery.preview();
        let skill_id = preview
            .candidates
            .iter()
            .find(|candidate| {
                candidate.kind == neko_protocol::setup_import::ImportCandidateKind::Skill
            })
            .unwrap()
            .id
            .clone();
        let workspace_id = preview
            .candidates
            .iter()
            .find(|candidate| {
                candidate.kind == neko_protocol::setup_import::ImportCandidateKind::Workspace
            })
            .unwrap()
            .id
            .clone();
        controller.importer.lock().unwrap().session = Some(Session {
            id: "preview".into(),
            expires_ms: store::now_ms() + 60_000,
            discovery,
            applied: HashSet::new(),
        });
        let command = || ImportCommand::Apply {
            preview_id: "preview".into(),
            connection_ids: vec![],
            global_connection_ids: vec![],
            schedule_ids: vec![],
            skill_ids: vec![skill_id.clone()],
            workspace_ids: vec![workspace_id.clone()],
            repositories: vec![],
            include_credentials: false,
            trust_local_processes: false,
        };
        let mut without_workspace = command();
        if let ImportCommand::Apply { workspace_ids, .. } = &mut without_workspace {
            workspace_ids.clear();
        }
        assert!(controller.import_command(without_workspace).is_err());
        let before = store::load(&controller.db.lock().unwrap()).unwrap();
        let baseline_workspaces = before.workspaces.len();
        let state = controller.import_command(command()).unwrap();
        assert_eq!(state.workspaces.len(), baseline_workspaces + 1);
        assert!(state.skills.enabled.is_empty());
        assert_eq!(state.skills.proposals.len(), 1);
        let state = controller.import_command(command()).unwrap();
        assert_eq!(state.workspaces.len(), baseline_workspaces + 1);
        assert_eq!(state.skills.proposals.len(), 1);
        let mut import_into_existing_workspace = command();
        if let ImportCommand::Apply { workspace_ids, .. } = &mut import_into_existing_workspace {
            workspace_ids.clear();
        }
        let state = controller
            .import_command(import_into_existing_workspace)
            .unwrap();
        assert_eq!(state.workspaces.len(), baseline_workspaces + 1);
        assert_eq!(state.skills.proposals.len(), 1);
    }
}
