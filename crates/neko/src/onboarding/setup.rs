//! Six visible setup steps; permission asks remain substates, not extra screens.
use super::*;
use crate::text_field::TextField;
use neko_protocol::{
    mcp_host::{McpCommand, Responsibility},
    setup_import::ImportCommand,
    workbench::{ChatRole, Command, Snapshot, Workspace},
};
use std::collections::HashSet;

pub(super) fn evidence() -> bool {
    std::env::var_os("NEKO_SHOW_SETUP").is_some()
}
fn evidence_step(value: Option<&str>) -> SetupStep {
    value
        .and_then(|s| s.parse::<usize>().ok())
        .filter(|i| (1..=6).contains(i))
        .map(|i| SetupStep::ALL[i - 1])
        .unwrap_or(SetupStep::Welcome)
}

pub(super) struct Setup {
    pub step: SetupStep,
    pub busy: bool,
    pub error: Option<String>,
    pub notice: Option<String>,
    clipboard_ready: bool,
    snapshot: Snapshot,
    workspace: Option<String>,
    name: Entity<TextField>,
    repository: Entity<TextField>,
    responsibility: Entity<TextField>,
    server_name: Entity<TextField>,
    server_target: Entity<TextField>,
    server_args: Entity<TextField>,
    server_local: bool,
    server_global: bool,
    server_trust: bool,
    selected_connections: HashSet<String>,
    selected_repositories: HashSet<String>,
    selected_schedules: HashSet<String>,
    credentials: bool,
    trust: bool,
    brief_turn: Option<String>,
}
impl Setup {
    fn reset_import_consent(&mut self) {
        self.credentials = false;
        self.trust = false;
    }
    pub fn new(cx: &mut Context<OnboardingRoot>) -> Self {
        fn input(placeholder: &str, cx: &mut App) -> Entity<TextField> {
            let field = TextField::new(cx);
            field.update(cx, |f, cx| f.set_placeholder(placeholder.to_owned(), cx));
            field
        }
        let server_target = input("https://server.example/mcp or /absolute/executable", cx);
        let server_args = input("[\"/path/to/server.js\"]", cx);
        for field in [&server_target, &server_args] {
            cx.subscribe(
                field,
                |root, _, _: &crate::text_field::ContentChanged, cx| {
                    root.setup.server_trust = false;
                    cx.notify();
                },
            )
            .detach();
        }
        Self {
            step: if evidence() {
                evidence_step(std::env::var("NEKO_SETUP_STEP").ok().as_deref())
            } else {
                SetupStep::Welcome
            },
            busy: false,
            clipboard_ready: false,
            error: None,
            notice: None,
            snapshot: Snapshot::default(),
            workspace: None,
            name: input("Workspace name", cx),
            repository: input("/Users/you/Projects/repository", cx),
            responsibility: input("What should Neko keep an eye on?", cx),
            server_name: input("Connection name", cx),
            server_target,
            server_args,
            server_local: false,
            server_global: false,
            server_trust: false,
            selected_connections: HashSet::new(),
            selected_repositories: HashSet::new(),
            selected_schedules: HashSet::new(),
            credentials: false,
            trust: false,
            brief_turn: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SetupStep {
    Welcome,
    Mac,
    Import,
    Tools,
    Responsibility,
    Sweep,
}
impl SetupStep {
    pub(super) const ALL: [Self; 6] = [
        Self::Welcome,
        Self::Mac,
        Self::Import,
        Self::Tools,
        Self::Responsibility,
        Self::Sweep,
    ];
    pub(super) fn index(self) -> usize {
        Self::ALL.iter().position(|s| *s == self).unwrap()
    }
    pub(super) fn next(self) -> Self {
        Self::ALL[(self.index() + 1).min(5)]
    }
    pub(super) fn back(self) -> Self {
        Self::ALL[self.index().saturating_sub(1)]
    }
}

fn brief_in_scope(actual: Option<&str>, selected: Option<&str>) -> bool {
    selected.is_some() && actual == selected
}

fn begin_clipboard_change(busy:&mut bool,ready:bool)->bool {
    if *busy || !ready { return false; }
    *busy=true;
    true
}

fn recording_after_navigation(step: SetupStep, recording: RecordingState) -> RecordingState {
    if step == SetupStep::Mac {
        recording
    } else {
        RecordingState::Idle
    }
}

impl OnboardingRoot {
    fn setup_read_clipboard(&mut self, cx:&mut Context<Self>) {
        if self.setup.busy { return; }
        self.setup.busy=true;
        let client = self.client.clone();
        cx.spawn(async move |this, cx| {
            let result = client.request(Request::GetClipboardHistoryEnabled).await;
            let _ = this.update(cx, |root, cx| {
                root.setup.busy=false;
                if let Ok(Response::ClipboardHistoryEnabled { enabled }) = result {
                    root.flow.clipboard_enabled = enabled;
                    root.setup.clipboard_ready=true;
                    root.setup_request(Command::Snapshot,cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn setup_start(&mut self, cx: &mut Context<Self>) {
        self.flow.accessibility_granted = self.accessibility.is_trusted();
        self.setup_read_clipboard(cx);
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(2)).await;
                if this
                    .update(cx, |root, cx| {
                        let was_trusted = root.flow.accessibility_granted;
                        root.flow.accessibility_granted = root.accessibility.is_trusted();
                        if !was_trusted && root.flow.accessibility_granted && !evidence() {
                            root.ensure_hotkey_registered();
                        }
                        if !root.setup.busy {
                            if !root.setup.clipboard_ready {root.setup_read_clipboard(cx);} else {root.setup_request(Command::Snapshot, cx);}
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }

    fn setup_request(&mut self, command: Command, cx: &mut Context<Self>) {
        if self.setup.busy {
            return;
        }
        self.setup.busy = true;
        let poll = matches!(command, Command::Snapshot);
        let discover = matches!(
            command,
            Command::SetupImport(ImportCommand::Discover { .. })
        );
        let brief = matches!(command, Command::SendMessage { .. });
        let connection_io = matches!(
            command,
            Command::Mcp(
                McpCommand::AddConnection { .. }
                    | McpCommand::Authenticate { .. }
                    | McpCommand::Discover { .. }
            )
        );
        if !poll {
            self.setup.error = None;
        }
        let client = self.client.clone();
        cx.spawn(async move |this, cx| {
            let result = client.request(Request::Workbench(command)).await;
            let _ = this.update(cx, |root, cx| {
                root.setup.busy = false;
                match result {
                    Ok(Response::Workbench(snapshot)) => {
                        if root.setup.workspace.is_none() { root.setup.workspace = snapshot.workspaces.first().map(|w| w.id.clone()); }
                        if discover {
                            root.setup.reset_import_consent();
                            root.setup.selected_connections.clear();
                            root.setup.selected_repositories.clear();
                            root.setup.selected_schedules.clear();
                            root.setup.notice = Some("Select what to bring over. Nothing runs or gains permission during import.".into());
                        }
                        if brief { root.setup.brief_turn = snapshot.conversation.iter().rev().find(|m| m.role == ChatRole::Neko && m.pending).map(|m| m.id.clone()); }
                        root.setup.snapshot = snapshot;
                    }
                    Ok(Response::Error { message }) => root.setup.error = Some(if connection_io {"Connection failed. Review the server setup or sign in, then retry.".into()} else {message}),
                    Err(error) => root.setup.error = Some(error.to_string()),
                    _ => root.setup.error = Some("Unexpected setup response; try again.".into()),
                }
                cx.notify();
            });
        }).detach();
        cx.notify();
    }

    pub(super) fn setup_next(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.setup.busy || matches!(self.flow.hotkey_recording, RecordingState::Recording) {
            return;
        }
        if self.setup.step == SetupStep::Sweep {
            self.close_and_persist_completion(window, cx);
            return;
        }
        self.setup.step = self.setup.step.next();
        self.flow.hotkey_recording =
            recording_after_navigation(self.setup.step, self.flow.hotkey_recording.clone());
        self.setup.error = None;
        self.setup.notice = None;
        cx.notify();
    }

    fn setup_workspace_picker(&self, cx: &mut Context<Self>) -> gpui::Div {
        let mut choices = div().flex().flex_wrap().gap(px(8.));
        for workspace in &self.setup.snapshot.workspaces {
            let id = workspace.id.clone();
            choices = choices.child(button(
                format!("setup-workspace-{id}"),
                format!(
                    "{}{}",
                    if self.setup.workspace.as_ref() == Some(&id) {
                        "✓ "
                    } else {
                        ""
                    },
                    workspace.name
                ),
                !self.setup.busy,
                cx,
                move |r, _, cx| {
                    if r.setup.workspace.as_ref() != Some(&id) {
                        r.setup.brief_turn = None;
                    }
                    r.setup.workspace = Some(id.clone());
                    cx.notify();
                },
            ));
        }
        if self.setup.snapshot.workspaces.is_empty() {
            choices = choices.child(note("Add or import a workspace on the Import step first."));
        }
        choices
    }

    fn setup_import_view(&self, cx: &mut Context<Self>) -> gpui::Div {
        let mut content = div().flex().flex_col().gap(px(12.)).child(button(
            "setup-scan",
            "Scan Codex and Claude settings",
            !self.setup.busy,
            cx,
            |r, _, cx| {
                let repository = r.setup.repository.read(cx).content().trim().to_owned();
                r.setup_request(
                    Command::SetupImport(ImportCommand::Discover {
                        repositories: if repository.is_empty() {
                            vec![]
                        } else {
                            vec![repository]
                        },
                    }),
                    cx,
                );
            },
        ));
        let preview = &self.setup.snapshot.import_preview;
        for repository in &preview.repositories {
            let path = repository.clone();
            content = content.child(button(
                format!("setup-import-repo-{path}"),
                format!(
                    "{} Workspace · {path}",
                    if self.setup.selected_repositories.contains(&path) {
                        "✓"
                    } else {
                        "+"
                    }
                ),
                !self.setup.busy,
                cx,
                move |r, _, cx| {
                    if !r.setup.selected_repositories.remove(&path) {
                        r.setup.selected_repositories.insert(path.clone());
                    }
                    r.setup.reset_import_consent();
                    cx.notify();
                },
            ));
        }
        for connection in &preview.connections {
            let id = connection.id.clone();
            content = content.child(button(
                format!("setup-import-connection-{id}"),
                format!(
                    "{} {} · {}{}",
                    if self.setup.selected_connections.contains(&id) {
                        "✓"
                    } else {
                        "+"
                    },
                    connection.name,
                    connection.repository.as_deref().unwrap_or("Global"),
                    if connection.has_credentials {
                        " · credentials found"
                    } else {
                        ""
                    }
                ),
                !self.setup.busy && connection.problem.is_none(),
                cx,
                move |r, _, cx| {
                    if !r.setup.selected_connections.remove(&id) {
                        r.setup.selected_connections.insert(id.clone());
                    }
                    r.setup.reset_import_consent();
                    cx.notify();
                },
            ));
            if let Some(problem) = &connection.problem {
                content = content.child(note(problem.clone()));
            }
        }
        for warning in &preview.warnings {
            content = content.child(note(warning.clone()));
        }
        for schedule in &preview.schedules {
            let id = schedule.id.clone();
            content = content
                .child(button(
                    format!("setup-import-schedule-{id}"),
                    format!(
                        "{} Schedule · {} · import paused",
                        if self.setup.selected_schedules.contains(&id) {
                            "✓"
                        } else {
                            "+"
                        },
                        schedule.name
                    ),
                    !self.setup.busy,
                    cx,
                    move |r, _, cx| {
                        if !r.setup.selected_schedules.remove(&id) {
                            r.setup.selected_schedules.insert(id.clone());
                        }
                        r.setup.reset_import_consent();
                        cx.notify();
                    },
                ))
                .child(note(format!(
                    "{} · {}",
                    schedule.source,
                    schedule.warnings.join(" · ")
                )));
        }
        content=content.child(note(format!("{} local skills discovered. They stay in their source folders; enable reviewed instructions per workspace in Tools & skills.",self.setup.snapshot.skills.available.len())));
        if !preview.preview_id.is_empty() {
            content = content
                .child(button(
                    "setup-import-credentials",
                    format!(
                        "{} Copy selected credentials into Keychain",
                        if self.setup.credentials { "✓" } else { "○" }
                    ),
                    !self.setup.busy,
                    cx,
                    |r, _, cx| {
                        r.setup.credentials = !r.setup.credentials;
                        cx.notify();
                    },
                ))
                .child(button(
                    "setup-import-trust",
                    format!(
                        "{} Trust selected local MCP processes",
                        if self.setup.trust { "✓" } else { "○" }
                    ),
                    !self.setup.busy,
                    cx,
                    |r, _, cx| {
                        r.setup.trust = !r.setup.trust;
                        cx.notify();
                    },
                ))
                .child(button(
                    "setup-import-apply",
                    "Import selected definitions",
                    !self.setup.busy,
                    cx,
                    |r, _, cx| {
                        r.setup_apply_import(cx);
                    },
                ));
        }
        content.child(note("Or create a workspace. Skills in its local folders become discoverable; nothing is enabled automatically."))
            .child(field("Name",&self.setup.name)).child(field("Repository folder",&self.setup.repository))
            .child(button("setup-create-workspace","Create workspace",!self.setup.busy,cx,|r,_,cx| {
                let name=r.setup.name.read(cx).content().trim().to_owned(); let repository=r.setup.repository.read(cx).content().trim().to_owned();
                r.setup_request(Command::SaveWorkspace{workspace:Workspace{id:String::new(),name,repository,instructions:String::new(),away_enabled:false}},cx);
            })).child(self.setup_workspace_picker(cx))
    }

    fn setup_apply_import(&mut self, cx: &mut Context<Self>) {
        self.setup_request(
            Command::SetupImport(ImportCommand::Apply {
                schedule_ids: self.setup.selected_schedules.iter().cloned().collect(),
                preview_id: self.setup.snapshot.import_preview.preview_id.clone(),
                connection_ids: self.setup.selected_connections.iter().cloned().collect(),
                repositories: self.setup.selected_repositories.iter().cloned().collect(),
                include_credentials: self.setup.credentials,
                trust_local_processes: self.setup.trust,
            }),
            cx,
        );
    }

    fn setup_tools_view(&self, cx: &mut Context<Self>) -> gpui::Div {
        let mut body=div().flex().flex_col().gap(px(12.)).child(self.setup_workspace_picker(cx))
            .child(div().flex().gap(px(8.)).child(button("setup-registry","Browse MCP Registry",true,cx,|_,_,cx|cx.open_url("https://registry.modelcontextprotocol.io"))).child(button("setup-skills-catalog","Browse skills.sh",true,cx,|_,_,cx|cx.open_url("https://skills.sh"))))
            .child(note("Catalogs are external. Review publishers and instructions before installing; listings are not an approval or safety guarantee."));
        let Some(workspace) = &self.setup.workspace else {
            return body;
        };
        body = body.child(button(
            "setup-refresh-skills",
            "Discover local skills",
            !self.setup.busy,
            cx,
            |r, _, cx| {
                r.setup_request(
                    Command::Skills(neko_protocol::skills::SkillCommand::Refresh),
                    cx,
                )
            },
        ));
        for skill in self
            .setup
            .snapshot
            .skills
            .available
            .iter()
            .filter(|s| s.workspace_id.as_ref().is_none_or(|w| w == workspace))
            .take(12)
        {
            let path = skill.path.clone();
            let review_path = path.clone();
            let workspace_id = workspace.clone();
            let hash = skill.content_hash.clone();
            let enabled =
                self.setup.snapshot.skills.enabled.iter().any(|s| {
                    s.workspace_id == *workspace && s.path == path && s.content_hash == hash
                });
            body = body.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .child(note(format!(
                        "Skill · {} · {}",
                        skill.name, skill.description
                    )))
                    .child(
                        div()
                            .flex()
                            .gap(px(8.))
                            .child(button(
                                format!("setup-review-skill-{path}"),
                                "Open instructions",
                                true,
                                cx,
                                move |_, _, cx| {
                                    cx.open_with_system(std::path::Path::new(&review_path))
                                },
                            ))
                            .child(button(
                                format!("setup-enable-skill-{path}"),
                                if enabled {
                                    "Disable here"
                                } else {
                                    "Enable reviewed instructions here"
                                },
                                !self.setup.busy,
                                cx,
                                move |r, _, cx| {
                                    r.setup_request(
                                        Command::Skills(
                                            neko_protocol::skills::SkillCommand::SetEnabled {
                                                workspace_id: workspace_id.clone(),
                                                path: path.clone(),
                                                content_hash: hash.clone(),
                                                enabled: !enabled,
                                            },
                                        ),
                                        cx,
                                    )
                                },
                            )),
                    ),
            );
        }
        if self.setup.snapshot.skills.available.len() > 12 {
            body=body.child(note("Showing the first 12 scoped skills. Search the complete library in Tools & skills after setup."));
        }
        for connection in self
            .setup
            .snapshot
            .mcp
            .connections
            .iter()
            .filter(|c| c.available_in(workspace))
        {
            let id = connection.id.clone();
            let auth_id = id.clone();
            body = body
                .child(div().text_size(px(16.)).child(format!(
                    "{} · {}",
                    connection.label,
                    if connection.workspace_id.is_empty() {
                        "Global definition"
                    } else {
                        "Workspace"
                    }
                )))
                .child(button(
                    format!("setup-discover-{id}"),
                    "Connect and discover tools",
                    !self.setup.busy && connection.enabled,
                    cx,
                    move |r, _, cx| {
                        r.setup_request(
                            Command::Mcp(McpCommand::Discover {
                                connection_id: id.clone(),
                            }),
                            cx,
                        )
                    },
                ));
            if matches!(
                connection.config,
                neko_protocol::mcp_host::ServerConfig::Http { .. }
            ) {
                body = body.child(button(
                    format!("setup-auth-{auth_id}"),
                    "Sign in through browser",
                    !self.setup.busy && connection.enabled,
                    cx,
                    move |r, _, cx| {
                        r.setup_request(
                            Command::Mcp(McpCommand::Authenticate {
                                connection_id: auth_id.clone(),
                                client_id: None,
                            }),
                            cx,
                        )
                    },
                ));
            }
            if let Some(error) = &connection.error {
                body = body.child(note(error.clone()));
            }
            for tool in &connection.tools {
                let connection_id = connection.id.clone();
                let tool_name = tool.name.clone();
                let schema_hash = tool.schema_hash.clone();
                let workspace_id = workspace.clone();
                let allowed = self.setup.snapshot.mcp.grants.iter().any(|g| {
                    g.workspace_id == *workspace
                        && g.connection_id == connection.id
                        && g.tool_name == tool.name
                        && g.schema_hash == tool.schema_hash
                });
                body = body.child(button(
                    format!("setup-grant-{connection_id}-{tool_name}"),
                    format!(
                        "{} {} · {}",
                        if allowed { "✓" } else { "+" },
                        tool.name,
                        if tool.read_only {
                            "declared read-only"
                        } else {
                            "actions require approval in chat"
                        }
                    ),
                    !self.setup.busy && connection.enabled,
                    cx,
                    move |r, _, cx| {
                        r.setup_request(
                            Command::Mcp(McpCommand::SetWorkspaceToolGrant {
                                workspace_id: workspace_id.clone(),
                                connection_id: connection_id.clone(),
                                tool_name: tool_name.clone(),
                                schema_hash: schema_hash.clone(),
                                allowed: !allowed,
                            }),
                            cx,
                        )
                    },
                ));
            }
        }
        body = body
            .child(div().text_size(px(16.)).child("Add your own MCP"))
            .child(
                div()
                    .flex()
                    .gap(px(8.))
                    .child(button(
                        "setup-transport",
                        if self.setup.server_local {
                            "Local process · change"
                        } else {
                            "HTTP server · change"
                        },
                        !self.setup.busy,
                        cx,
                        |r, _, cx| {
                            r.setup.server_local = !r.setup.server_local;
                            r.setup.server_trust = false;
                            cx.notify();
                        },
                    ))
                    .child(button(
                        "setup-scope",
                        if self.setup.server_global {
                            "Global definition · change"
                        } else {
                            "This workspace · change"
                        },
                        !self.setup.busy,
                        cx,
                        |r, _, cx| {
                            r.setup.server_global = !r.setup.server_global;
                            r.setup.server_trust = false;
                            cx.notify();
                        },
                    )),
            )
            .child(field("Name", &self.setup.server_name))
            .child(field("Server", &self.setup.server_target));
        if self.setup.server_local {
            body = body
                .child(field("Arguments as a JSON list", &self.setup.server_args))
                .child(button(
                    "setup-local-trust",
                    if self.setup.server_trust {
                        "✓ Local process trusted"
                    } else {
                        "Trust this local process to run"
                    },
                    !self.setup.busy,
                    cx,
                    |r, _, cx| {
                        r.setup.server_trust = !r.setup.server_trust;
                        cx.notify();
                    },
                ));
        }
        body.child(button("setup-add-server","Add definition",!self.setup.busy,cx,|r,_,cx| {
            let target=r.setup.server_target.read(cx).content().trim().to_owned();
            let config=if r.setup.server_local {
                let raw=r.setup.server_args.read(cx).content().trim();
                let args=if raw.is_empty(){Ok(vec![])}else{serde_json::from_str::<Vec<String>>(raw)};
                let args=match args{Ok(args)=>args,Err(_)=>{r.setup.error=Some("Arguments must be a JSON list of strings.".into());cx.notify();return;}};
                neko_protocol::mcp_host::ServerConfig::Stdio{command:target,args}
            }else{neko_protocol::mcp_host::ServerConfig::Http{url:target}};
            let workspace_id=if r.setup.server_global{String::new()}else{r.setup.workspace.clone().unwrap_or_default()};
            r.setup_request(Command::Mcp(McpCommand::AddConnection{workspace_id,label:r.setup.server_name.read(cx).content().trim().to_owned(),config,trust_local_process:r.setup.server_trust,credentials:None}),cx);
        })).child(note("For credential JSON or a registered OAuth client ID, use Tools & skills after setup. No named service is required."))
    }

    fn setup_responsibility_view(&self, cx: &mut Context<Self>) -> gpui::Div {
        let mut body=div().flex().flex_col().gap(px(12.)).child(self.setup_workspace_picker(cx)).child(field("A standing responsibility",&self.setup.responsibility))
            .child(note("Neko checks the granted tools below. Local fixes still require approval; publishing and away messaging are not enabled here."));
        if let Some(workspace) = &self.setup.workspace {
            let connections: Vec<_> = self
                .setup
                .snapshot
                .mcp
                .connections
                .iter()
                .filter(|c| {
                    c.enabled
                        && c.available_in(workspace)
                        && self
                            .setup
                            .snapshot
                            .mcp
                            .grants
                            .iter()
                            .any(|g| g.workspace_id == *workspace && g.connection_id == c.id)
                })
                .map(|c| c.label.clone())
                .collect();
            body = body.child(note(format!(
                "Available: {}",
                if connections.is_empty() {
                    "none — grant a tool on the previous step".into()
                } else {
                    connections.join(", ")
                }
            )));
        }
        body.child(button(
            "setup-save-responsibility",
            "Start watching with these tools",
            !self.setup.busy && self.setup.workspace.is_some(),
            cx,
            |r, _, cx| {
                let Some(workspace_id) = r.setup.workspace.clone() else {
                    return;
                };
                let connection_ids =
                    r.setup
                        .snapshot
                        .mcp
                        .connections
                        .iter()
                        .filter(|c| {
                            c.enabled
                                && c.available_in(&workspace_id)
                                && r.setup.snapshot.mcp.grants.iter().any(|g| {
                                    g.workspace_id == workspace_id && g.connection_id == c.id
                                })
                        })
                        .map(|c| c.id.clone())
                        .collect();
                let instruction = r.setup.responsibility.read(cx).content().trim().to_owned();
                r.setup_request(
                    Command::Mcp(McpCommand::SaveResponsibility {
                        responsibility: Responsibility {
                            id: String::new(),
                            workspace_id,
                            instruction,
                            connection_ids,
                            enabled: true,
                            prepare_low_risk: false,
                            next_due_ms: 0,
                            last_attempt_ms: None,
                            last_result: String::new(),
                            failures: 0,
                        },
                    }),
                    cx,
                );
            },
        ))
        .children(
            self.setup
                .snapshot
                .mcp
                .responsibilities
                .iter()
                .filter(|r| Some(&r.workspace_id) == self.setup.workspace.as_ref())
                .map(|r| note(format!("{} · {}", r.instruction, r.last_result))),
        )
    }

    pub(super) fn render_setup(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let stage = self.setup.step;
        let labels = [
            "Welcome",
            "Your Mac",
            "Import",
            "Tools",
            "Responsibility",
            "First look",
        ];
        let titles = [
            "While you’re away,\nsomeone should be you.",
            "Let Neko work on your Mac",
            "Bring your setup with you",
            "Your tools, your boundaries",
            "What should I look after?",
            "Your first look, grounded in real work",
        ];
        let subtitles = [
            "One Neko to talk to. A quick panel when you need it. Your work stays yours.",
            "Each permission is optional. You can always open the full app without a shortcut.",
            "Review local Codex and Claude settings. Source files stay untouched.",
            "Global definitions are available everywhere; permissions are granted per workspace.",
            "Start with one clear responsibility. Tool access stays limited to what you grant.",
            "Neko reads the selected workspace and granted tools, then prepares a useful brief.",
        ];
        let mut progress = div().flex().gap(px(6.)).items_center();
        for (index, label) in labels.iter().enumerate() {
            let _ = label;
            progress = progress.child(div().w(px(20.)).h(px(3.)).rounded(px(2.)).bg(
                if index == stage.index() {
                    theme::active().text_primary
                } else {
                    theme::active().border_hairline_strong
                },
            ));
        }
        let mut body = div()
            .id("setup-body")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .px(px(90.))
            .py(px(28.))
            .flex()
            .flex_col()
            .gap(px(18.))
            .when(stage == SetupStep::Welcome, |b| {
                b.items_center().justify_center().text_center()
            });
        if stage == SetupStep::Welcome {
            body = body.child(
                gpui::svg()
                    .path(crate::assets::icon::MARK)
                    .size(px(88.))
                    .text_color(theme::active().text_primary),
            );
        }
        body = body
            .child(
                div()
                    .text_size(px(if stage == SetupStep::Welcome {
                        48.
                    } else {
                        30.
                    }))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .line_height(px(if stage == SetupStep::Welcome {
                        54.
                    } else {
                        38.
                    }))
                    .child(titles[stage.index()]),
            )
            .child(note(subtitles[stage.index()]));
        match stage {
            SetupStep::Welcome => {
                body=body.child(div().mt(px(18.)).child(button("setup-get-started","Get started",!self.setup.busy,cx,|r,w,cx|r.setup_next(w,cx))))
                    .child(note("Start with one workspace. Add tools and responsibilities at your own pace."));
            }
            SetupStep::Mac => {
                body = body.child(button("setup-accessibility", if self.flow.accessibility_granted { "Accessibility enabled" } else { "Allow Accessibility…" }, !self.setup.busy && !self.flow.accessibility_granted && !evidence(), cx, |r,_,cx| { r.flow.step=Step::AccessibilityAsk; r.request_accessibility(cx); }))
                    .child(note("Used for the global shortcut and paste actions. macOS controls this permission."))
                    .child(button("setup-clipboard", if !self.setup.clipboard_ready { "Reading clipboard setting…" } else if self.flow.clipboard_enabled { "Turn clipboard history off" } else { "Enable clipboard history" }, self.setup.clipboard_ready && !self.setup.busy && !evidence(), cx, |r,_,cx| {
                        if !begin_clipboard_change(&mut r.setup.busy,r.setup.clipboard_ready) { return; }
                        let enabled = !r.flow.clipboard_enabled;
                        let client=r.client.clone(); cx.notify();
                        cx.spawn(async move |this,cx| { let result=client.request(Request::SetClipboardHistoryEnabled{enabled}).await; let _=this.update(cx,|r,cx| { r.setup.busy=false; match result { Ok(Response::ClipboardHistoryEnabled{enabled:actual}) if actual==enabled=>r.flow.clipboard_enabled=actual,Ok(Response::Error{message})=>r.setup.error=Some(message),Err(e)=>r.setup.error=Some(e.to_string()),Ok(_)=>r.setup.error=Some("Clipboard setting was not acknowledged. Try again.".into()) }; cx.notify(); }); }).detach();
                    }))
                    .child(note("Saved locally. Off by default; enabling starts clipboard capture."))
                    .child(button("setup-shortcut", format!("Shortcut: {} · Change", self.flow.current_combo.display().replace('⌥',"Option + ").replace('⌘',"Command + ").replace('⌃',"Control + ").replace('⇧',"Shift + ")), self.flow.accessibility_granted && !self.setup.busy && !evidence(), cx, |r,_,cx| { r.flow.step=Step::LearnHotkey; r.toggle_recording(cx); }));
                if matches!(self.flow.hotkey_recording, RecordingState::Recording) {
                    body = body.child(note(
                        "Press the new key combination. Escape cancels recording.",
                    ));
                }
                if let RecordingState::Rejected(reason) = &self.flow.hotkey_recording {
                    body = body.child(note(reason.clone()));
                }
            }
            SetupStep::Import => {
                body = body.child(self.setup_import_view(cx));
            }
            SetupStep::Tools => {
                body = body.child(self.setup_tools_view(cx));
            }
            SetupStep::Responsibility => {
                body = body.child(self.setup_responsibility_view(cx));
            }
            SetupStep::Sweep => {
                body=body.child(self.setup_workspace_picker(cx)).child(button("setup-brief", "Read my workspace and prepare a brief", !self.setup.busy && self.setup.workspace.is_some() && !self.setup.snapshot.conversation.iter().any(|m|m.pending),cx,|r,_,cx| {
                    r.setup_request(Command::SendMessage { workspace_id:r.setup.workspace.clone(), text:"Prepare my first Neko brief. Read only the selected workspace and explicitly granted read-only tools. Do not edit, create tickets, publish, or call tools with side effects. Summarize what needs attention, cite the evidence you actually read, recommend one next step, and say what you could not inspect. Do not invent activity.".into() },cx);
                }));
                if let Some(turn) = self.setup.brief_turn.as_ref().and_then(|id| {
                    self.setup.snapshot.conversation.iter().find(|m| {
                        &m.id == id
                            && brief_in_scope(
                                m.workspace_id.as_deref(),
                                self.setup.workspace.as_deref(),
                            )
                    })
                }) {
                    body = body.child(note(if turn.pending {
                        "Reading your workspace…".to_string()
                    } else {
                        turn.text.clone()
                    }));
                    if turn.failed {
                        body=body.child(note("The brief could not finish. Retry or open Today to inspect the failure."));
                    }
                } else {
                    body=body.child(note("No sweep has run yet. You can prepare a brief now or open Today and start there."));
                }
            }
        }
        if let Some(error) = &self.setup.error {
            body = body.child(note(format!("Could not complete: {error}")));
        }
        if let Some(notice) = &self.setup.notice {
            body = body.child(note(notice.clone()));
        }
        let footer = div()
            .flex()
            .justify_between()
            .items_center()
            .px(px(40.))
            .py(px(18.))
            .border_t_1()
            .border_color(theme::active().border_hairline)
            .child(button(
                "setup-back",
                "Back",
                !self.setup.busy && stage != SetupStep::Welcome,
                cx,
                |r, _, cx| {
                    r.setup.step = r.setup.step.back();
                    r.flow.hotkey_recording =
                        recording_after_navigation(r.setup.step, r.flow.hotkey_recording.clone());
                    cx.notify();
                },
            ))
            .child(
                div()
                    .flex()
                    .gap(px(12.))
                    .child(button(
                        "setup-skip",
                        "Set up later",
                        !self.setup.busy,
                        cx,
                        |r, w, cx| r.close_and_persist_completion(w, cx),
                    ))
                    .child(button(
                        "setup-next",
                        if stage == SetupStep::Sweep {
                            "Open Neko"
                        } else {
                            "Continue"
                        },
                        !self.setup.busy,
                        cx,
                        |r, w, cx| r.setup_next(w, cx),
                    )),
            );
        div()
            .id("neko-setup")
            .tab_group()
            .key_context("Onboarding")
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::on_key_down))
            .flex()
            .flex_col()
            .size_full()
            .bg(theme::active().surface_panel)
            .text_color(theme::active().text_primary)
            .child(
                div()
                    .h(px(52.))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .window_control_area(gpui::WindowControlArea::Drag)
                    .child(progress),
            )
            .child(body)
            .when(stage != SetupStep::Welcome, |view| view.child(footer))
            .into_any_element()
    }
}

fn note(text: impl Into<SharedString>) -> gpui::Div {
    div()
        .text_size(px(13.))
        .line_height(px(20.))
        .text_color(theme::active().text_secondary)
        .child(text.into())
}
fn field(label: &str, input: &Entity<TextField>) -> gpui::Div {
    div()
        .flex()
        .flex_col()
        .gap(px(7.))
        .child(note(label.to_owned()))
        .child(
            div()
                .h(px(40.))
                .px(px(12.))
                .flex()
                .items_center()
                .rounded(px(8.))
                .border_1()
                .border_color(theme::active().border_hairline_strong)
                .bg(theme::active().surface_input)
                .child(input.clone()),
        )
}
fn button(
    id: impl Into<SharedString>,
    label: impl Into<SharedString>,
    enabled: bool,
    cx: &mut Context<OnboardingRoot>,
    action: impl Fn(&mut OnboardingRoot, &mut Window, &mut Context<OnboardingRoot>) + 'static,
) -> impl IntoElement {
    let action = Rc::new(action);
    let keyboard = action.clone();
    div()
        .id(id.into())
        .tab_index(0)
        .tab_stop(enabled)
        .px(px(14.))
        .py(px(10.))
        .rounded(px(8.))
        .border_1()
        .border_color(theme::active().border_hairline)
        .bg(theme::active().surface_input)
        .text_size(px(13.))
        .text_color(if enabled {
            theme::active().text_primary
        } else {
            theme::active().text_secondary
        })
        .focus_visible(|s| s.border_color(theme::active().text_primary))
        .when(enabled, |b| {
            b.cursor_pointer()
                .hover(|s| s.bg(theme::active().surface_selected))
                .on_click(cx.listener(move |r, _, w, cx| action(r, w, cx)))
                .on_key_down(cx.listener(move |r, e: &KeyDownEvent, w, cx| {
                    if matches!(r.flow.hotkey_recording, RecordingState::Recording) {
                        r.on_key_down(e, w, cx);
                        cx.stop_propagation();
                        return;
                    }
                    if e.keystroke.modifiers == gpui::Modifiers::default()
                        && matches!(e.keystroke.key.as_str(), "enter" | "space")
                    {
                        keyboard(r, w, cx);
                        cx.stop_propagation();
                    }
                }))
        })
        .child(label.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn delayed_clipboard_ack_rejects_duplicate_activation() {
        let mut busy=false;
        assert!(!begin_clipboard_change(&mut busy,false));
        assert!(!busy);
        assert!(begin_clipboard_change(&mut busy,true));
        assert!(!begin_clipboard_change(&mut busy,true));
        assert!(busy);
    }
    #[test]
    fn another_workspace_never_inherits_the_displayed_brief() {
        assert!(brief_in_scope(Some("a"), Some("a")));
        assert!(!brief_in_scope(Some("a"), Some("b")));
        assert!(!brief_in_scope(None, Some("b")));
    }
    #[test]
    fn leaving_mac_cancels_recording() {
        assert_eq!(
            recording_after_navigation(SetupStep::Welcome, RecordingState::Recording),
            RecordingState::Idle
        );
        assert_eq!(
            recording_after_navigation(SetupStep::Mac, RecordingState::Recording),
            RecordingState::Recording
        );
    }
    #[test]
    fn evidence_steps_are_one_based_and_invalid_values_start_at_welcome() {
        assert_eq!(evidence_step(Some("1")), SetupStep::Welcome);
        assert_eq!(evidence_step(Some("6")), SetupStep::Sweep);
        for value in [None, Some("0"), Some("7"), Some("oops")] {
            assert_eq!(evidence_step(value), SetupStep::Welcome);
        }
    }
    #[test]
    fn six_steps_have_bounded_back_and_next() {
        let mut step = SetupStep::Welcome;
        assert_eq!(step.back(), SetupStep::Welcome);
        for expected in [
            SetupStep::Mac,
            SetupStep::Import,
            SetupStep::Tools,
            SetupStep::Responsibility,
            SetupStep::Sweep,
        ] {
            step = step.next();
            assert_eq!(step, expected);
        }
        assert_eq!(step.next(), SetupStep::Sweep);
        assert_eq!(step.back(), SetupStep::Responsibility);
    }
}
