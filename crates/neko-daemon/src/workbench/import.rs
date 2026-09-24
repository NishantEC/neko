//! Read-only previews hold source secrets only in memory. Applying a reviewed
//! selection uses the normal Keychain/controller boundary, never starts tools.
use super::*;
use neko_core::setup_import::{self, Discovery};
use neko_protocol::{setup_import::ImportCommand, mcp_host::ServerConfig};
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
        if self.session.as_ref().is_some_and(|s| now >= s.expires_ms) { self.session = None; }
    }
}

impl Session {
    fn validate(&self, id: &str, connections: &[String], repositories: &[String], trust: bool) -> Result<(), String> {
        if self.id != id || store::now_ms() >= self.expires_ms {
            return Err("Import preview expired. Scan again before applying.".into());
        }
        if connections.len() > 100 || repositories.len() > 100 { return Err("Too many import selections".into()); }
        for repository in repositories {
            if !self.discovery.repositories.contains(repository) { return Err("Repository was not part of the reviewed preview".into()); }
        }
        for id in connections {
            let candidate = self.discovery.candidates.iter().find(|c| c.preview.id == *id).ok_or("Connection was not part of the reviewed preview")?;
            let config = candidate.config.as_ref().ok_or("Resolve unsupported configuration before importing")?;
            if matches!(config, ServerConfig::Stdio { .. }) && !trust { return Err("Explicitly trust selected local processes before importing".into()); }
        }
        Ok(())
    }
}

impl Controller {
    pub(super) fn import_command(&self, command: ImportCommand) -> Result<Snapshot, String> {
        self.import_command_with_schedule_resolver(command, |repository| std::fs::canonicalize(repository).ok())
    }

    fn import_command_with_schedule_resolver(&self, command: ImportCommand, resolve: impl Fn(&str) -> Option<PathBuf>) -> Result<Snapshot, String> {
        let mut importer = self.importer.try_lock().map_err(|_| "Another import is in progress")?;
        importer.evict_expired(store::now_ms());
        match command {
            ImportCommand::Discover { repositories } => {
                if repositories.len() > 100 || repositories.iter().any(|p| p.len() > 4096 || !std::path::Path::new(p).is_absolute()) {
                    return Err("Choose at most 100 absolute repository paths".into());
                }
                let home = std::env::var_os("HOME").map(PathBuf::from).ok_or("Home directory unavailable")?;
                let paths = std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect::<Vec<_>>()).unwrap_or_default();
                let environment = std::env::vars_os().filter_map(|(key, value)| Some((key.into_string().ok()?, value.into_string().ok()?))).collect();
                let discovery = setup_import::discover(&home, &repositories.into_iter().map(PathBuf::from).collect::<Vec<_>>(), &paths, &environment);
                let id = store::new_id();
                let mut preview = discovery.preview();
                preview.preview_id = id.clone();
                let db = self.db.lock().map_err(|_| "Import storage unavailable")?;
                setup_import::save_preview(&db, &preview)?;
                importer.session = Some(Session { id, expires_ms: store::now_ms() + 600_000, discovery, applied: HashSet::new() });
                store::load(&db)
            }
            ImportCommand::Apply { preview_id, connection_ids, schedule_ids, repositories, include_credentials, trust_local_processes } => {
                let session = importer.session.as_mut().ok_or("Scan again; source credentials are not retained across restart")?;
                session.validate(&preview_id, &connection_ids, &repositories, trust_local_processes)?;
                if schedule_ids.len() > 100 || schedule_ids.iter().any(|id| !session.discovery.schedules.iter().any(|s| &s.id == id)) { return Err("Schedule was not part of the reviewed preview".into()); }
                for repository in &repositories {
                    let canonical = std::fs::canonicalize(repository).map_err(|_| "Selected repository is unavailable")?.to_string_lossy().into_owned();
                    let db = self.db.lock().map_err(|_| "Import storage unavailable")?;
                    let state = store::load(&db)?;
                    if !state.workspaces.iter().any(|w| w.repository == canonical) {
                        store::apply(&db, Command::SaveWorkspace { workspace: Workspace {
                            id: String::new(), name: std::path::Path::new(repository).file_name().and_then(|s| s.to_str()).unwrap_or("Imported workspace").into(),
                            repository: repository.clone(), instructions: String::new(), away_enabled: false,
                        }})?;
                    }
                }
                for id in connection_ids {
                    if session.applied.contains(&id) { continue; }
                    let candidate = session.discovery.candidates.iter().find(|c| c.preview.id == id).ok_or("Import candidate missing")?;
                    let config = candidate.config.clone().ok_or("Import candidate unsupported")?;
                    let state = store::load(&*self.db.lock().map_err(|_| "Import storage unavailable")?)?;
                    let workspace_id = if let Some(repository) = &candidate.preview.repository {
                        let canonical = std::fs::canonicalize(repository).map_err(|_| "Import workspace unavailable")?.to_string_lossy().into_owned();
                        state.workspaces.iter().find(|w| w.repository == canonical).ok_or("Select the connection's workspace for import first")?.id.clone()
                    } else { String::new() };
                    // Preserve an existing definition, including its credentials and
                    // permissions, rather than silently overwriting it on reimport.
                    if state.mcp.connections.iter().any(|c| c.workspace_id == workspace_id && c.label == candidate.preview.name && c.config == config) {
                        session.applied.insert(id); continue;
                    }
                    self.mcp.add_connection(workspace_id, candidate.preview.name.clone(), config,
                        trust_local_processes,
                        if include_credentials { candidate.credentials.clone() } else { None },
                        candidate.preview.enabled_at_source)?;
                    session.applied.insert(id);
                }
                // Resolve only the bounded reviewed selection before acquiring
                // the shared DB mutex: a filesystem/mount can stall here.
                let resolved_schedules = schedule_ids.iter().map(|id| {
                    let candidate = session.discovery.schedules.iter().find(|s| &s.id == id).ok_or("Schedule candidate missing")?;
                    let repository = candidate.repository.as_deref().and_then(&resolve);
                    Ok((candidate, repository))
                }).collect::<Result<Vec<_>, String>>()?;
                {
                    let db = self.db.lock().map_err(|_| "Import storage unavailable")?;
                    let mut state = store::load(&db)?;
                    for (candidate, repository) in resolved_schedules {
                        if state.schedules.iter().any(|s| s.source_id.as_deref() == Some(&candidate.id)) { continue; }
                        let workspace_id = repository
                            .and_then(|path| state.workspaces.iter().find(|w| std::path::Path::new(&w.repository) == path).map(|w| w.id.clone()));
                        neko_core::scheduled_plans::apply(&mut state, neko_protocol::scheduled_plans::ScheduleCommand::Save { schedule: neko_protocol::scheduled_plans::Schedule {
                            name:candidate.name.clone(),prompt:candidate.prompt.clone(),workspace_id,rule:candidate.rule.clone(),timezone:candidate.timezone.clone(),anchor_ms:candidate.anchor_ms,..Default::default()
                        }})?;
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
    fn schedule_paths_resolve_without_db_lock_and_match_fresh_workspace_state() {
        let controller = super::super::tests::controller_with_task(TaskStatus::Completed);
        let discovery = Discovery { schedules: vec![neko_protocol::setup_import::ImportSchedule {
            id:"source".into(),name:"Daily review".into(),prompt:"Review changes".into(),source:"fixture".into(),repository:Some("/source-alias".into()),rule:"FREQ=DAILY".into(),timezone:String::new(),anchor_ms:0,warnings:vec![],
        }],..Default::default() };
        controller.importer.lock().unwrap().session=Some(Session{id:"preview".into(),expires_ms:store::now_ms()+60_000,discovery,applied:HashSet::new()});
        let command=ImportCommand::Apply{preview_id:"preview".into(),connection_ids:vec![],schedule_ids:vec!["source".into()],repositories:vec![],include_credentials:false,trust_local_processes:false};
        let state=controller.import_command_with_schedule_resolver(command, |repository| {
            assert_eq!(repository,"/source-alias");
            let db=controller.db.try_lock().expect("Filesystem resolution must not hold the shared database mutex");
            // Simulate workspace state changing during slow filesystem I/O.
            let mut state=store::load(&db).unwrap();state.workspaces[0].repository="/resolved-repository".into();store::save(&db,&state).unwrap();
            Some(PathBuf::from("/resolved-repository"))
        }).unwrap();
        assert_eq!(state.schedules[0].workspace_id.as_deref(),Some("w"));
        assert!(!state.schedules[0].enabled);
    }
    #[test]
    fn schedule_import_is_paused_scope_missing_and_preserves_user_edits_on_reimport() {
        let controller = super::super::tests::controller_with_task(TaskStatus::Completed);
        let discovery = Discovery { schedules: vec![neko_protocol::setup_import::ImportSchedule {
            id:"source".into(),name:"Daily review".into(),prompt:"Review changes".into(),source:"fixture".into(),repository:Some("/not-a-real-repository".into()),rule:"FREQ=DAILY".into(),timezone:String::new(),anchor_ms:0,warnings:vec!["Needs workspace".into()],
        }],..Default::default() };
        controller.importer.lock().unwrap().session=Some(Session{id:"preview".into(),expires_ms:store::now_ms()+60_000,discovery,applied:HashSet::new()});
        let command=||ImportCommand::Apply{preview_id:"preview".into(),connection_ids:vec![],schedule_ids:vec!["source".into()],repositories:vec![],include_credentials:false,trust_local_processes:false};
        let state=controller.import_command(command()).unwrap();
        assert_eq!(state.schedules.len(),1);assert!(!state.schedules[0].enabled);assert!(state.schedules[0].workspace_id.is_none());assert!(state.mcp.grants.is_empty());assert_eq!(state.tasks.len(),1);
        let mut edited=state.schedules[0].clone();edited.prompt="My own updated instruction".into();
        controller.schedule_command(neko_protocol::scheduled_plans::ScheduleCommand::Save{schedule:edited}).unwrap();
        let state=controller.import_command(command()).unwrap();
        assert_eq!(state.schedules.len(),1);assert_eq!(state.schedules[0].prompt,"My own updated instruction");
        assert!(controller.import_command(ImportCommand::Apply{preview_id:"preview".into(),connection_ids:vec![],schedule_ids:vec!["not-reviewed".into()],repositories:vec![],include_credentials:false,trust_local_processes:false}).is_err());
    }
    #[test]
    fn expired_session_releases_cached_discovery() {
        let mut importer = Importer { session: Some(Session { id:"x".into(), expires_ms:10, discovery:Discovery::default(), applied:HashSet::new() }) };
        importer.evict_expired(9); assert!(importer.session.is_some());
        importer.evict_expired(10); assert!(importer.session.is_none());
    }
    #[test]
    fn preview_selection_is_bound_to_session_and_requires_local_trust() {
        let mut discovery = Discovery::default();
        discovery.candidates.push(neko_core::setup_import::Candidate {
            preview: neko_protocol::setup_import::ImportConnection { id: "c".into(), name:"Server".into(),source:"fixture".into(),repository:None,has_credentials:false,enabled_at_source:true,problem:None },
            config:Some(ServerConfig::Stdio {command:"/bin/echo".into(),args:vec![]}), credentials:None,
        });
        let session = Session {id:"review".into(),expires_ms:store::now_ms()+60_000,discovery,applied:HashSet::new()};
        assert!(session.validate("wrong", &["c".into()], &[], true).is_err());
        assert!(session.validate("review", &["unknown".into()], &[], true).is_err());
        assert!(session.validate("review", &["c".into()], &[], false).is_err());
        assert!(session.validate("review", &["c".into()], &[], true).is_ok());
        assert!(session.validate("review", &[], &["/not-reviewed".into()], true).is_err());
    }
}
