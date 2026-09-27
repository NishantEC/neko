use neko_protocol::mcp_host::ServerConfig;
use neko_protocol::setup_import::{ImportCandidate, ImportCandidateKind};

fn candidate_in_workspace(candidate: &ImportCandidate, repository: &str) -> bool {
    if candidate.kind != ImportCandidateKind::Connection {
        return false;
    }
    let Some(path) = candidate.workspace.as_deref() else {
        return true;
    };
    if path == repository {
        return true;
    }
    let (Ok(left), Ok(right)) = (
        std::path::Path::new(path).canonicalize(),
        std::path::Path::new(repository).canonicalize(),
    ) else {
        return false;
    };
    left == right
}

fn candidate_is_local(candidate: &ImportCandidate) -> bool {
    // Old previews omit transport. Unknown means local until proven otherwise,
    // so a discovery format change never silently trusts executable code.
    candidate.metadata.get("transport").map(String::as_str) != Some("http")
}

pub(super) fn needs_initial_discovery(workspace: Option<&str>, scanned: Option<&str>) -> bool {
    workspace.is_some() && workspace != scanned
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Tab {
    Connections,
    Skills,
    Browse,
}

fn responsibility_from_form(
    workspace: &str,
    instruction: &str,
    connections: &[String],
    prepare: bool,
) -> Result<neko_protocol::mcp_host::Responsibility, String> {
    if instruction.trim().is_empty() || connections.is_empty() {
        return Err("Write an instruction and select at least one connection".into());
    }
    Ok(neko_protocol::mcp_host::Responsibility {
        id: String::new(),
        workspace_id: workspace.into(),
        instruction: instruction.trim().into(),
        connection_ids: connections.to_vec(),
        enabled: true,
        prepare_low_risk: prepare,
        next_due_ms: 0,
        last_attempt_ms: None,
        last_result: String::new(),
        failures: 0,
    })
}

pub(super) fn config_from_form(
    local: bool,
    target: &str,
    arguments: &str,
) -> Result<ServerConfig, String> {
    let target = target.trim();
    if target.is_empty() {
        return Err("Enter a server URL or executable path".into());
    }
    if local {
        if !std::path::Path::new(target).is_absolute() {
            return Err("Use an absolute executable path".into());
        }
        let args = serde_json::from_str::<Vec<String>>(if arguments.trim().is_empty() {
            "[]"
        } else {
            arguments
        })
        .map_err(|_| "Arguments must be a JSON array of strings, not a shell command")?;
        Ok(ServerConfig::Stdio {
            command: target.into(),
            args,
            cwd: None,
        })
    } else {
        Ok(ServerConfig::Http { url: target.into() })
    }
}

pub(super) fn view(
    root: &super::WorkspaceRoot,
    cx: &mut gpui::Context<super::WorkspaceRoot>,
) -> gpui::AnyElement {
    use super::*;
    use neko_protocol::mcp_host::McpCommand;
    if root.selection.workspace.is_none() {
        return root
            .choose_workspace("Choose where these tools may be used.", cx)
            .into_any_element();
    }
    let mut body = div()
        .id("mcp-tools")
        .size_full()
        .overflow_y_scroll()
        .flex()
        .flex_col()
        .gap(px(18.))
        .child(heading(
            "Tools & skills",
            "Add your own servers. No integration is required or granted automatically.",
        ));
    let mut tabs = div().flex().gap(px(8.));
    for (tab, label) in [
        (Tab::Connections, "Connections"),
        (Tab::Skills, "Skills"),
        (Tab::Browse, "Browse"),
    ] {
        tabs = tabs.child(button(
            format!("tools-tab-{label}"),
            label,
            true,
            root.tools_tab == tab,
            cx,
            move |r, _, cx| {
                r.tools_tab = tab;
                if tab == Tab::Connections {
                    refresh_found_connections(r, cx);
                }
                cx.notify();
            },
        ));
    }
    body = body.child(tabs);
    match root.tools_tab {
        Tab::Skills => return body.child(super::skills::view(root,cx)).into_any_element(),
        Tab::Browse => return body
            .child(heading("Find tools and skills","Catalogs are discovery sources, not permission grants. Review the source before adding anything."))
            .child(button("browse-mcp-registry","Open MCP Registry",true,false,cx,|_,_,cx|cx.open_url("https://registry.modelcontextprotocol.io")))
            .child(note("Copy a server's configuration, then add it in Connections. Local servers require explicit process trust; every tool requires a workspace grant."))
            .child(super::skills::catalog(root,cx)).into_any_element(),
        Tab::Connections => {}
    }
    body = body.child(found_connections(root, cx));
    body = body.child(heading(
        "Neko connections",
        "Only tools you explicitly allow can be used in this workspace.",
    ));
    for c in root.snapshot.mcp.connections.iter().filter(|c| {
        root.selection
            .workspace
            .as_ref()
            .is_some_and(|w| c.available_in(w))
    }) {
        let id = c.id.clone();
        let toggle = id.clone();
        let enabled = c.enabled;
        let mut card = div()
            .flex()
            .flex_col()
            .gap(px(10.))
            .p(px(16.))
            .rounded(px(12.))
            .bg(theme::active().surface_raised)
            .child(
                div()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child(c.label.clone()),
            )
            .child(note(format!(
                "{} · {} · {} discovered tools",
                if c.workspace_id.is_empty() {
                    "Global definition · grants are workspace-specific"
                } else {
                    "Workspace connection"
                },
                if enabled { "Enabled" } else { "Paused" },
                c.tools.len()
            )))
            .child(
                div()
                    .flex()
                    .gap(px(8.))
                    .child(button(
                        format!("discover-{id}"),
                        "Discover tools",
                        !root.busy && enabled,
                        true,
                        cx,
                        move |root, _, cx| {
                            root.request(
                                Command::Mcp(McpCommand::Discover {
                                    connection_id: id.clone(),
                                }),
                                cx,
                            )
                        },
                    ))
                    .child(button(
                        format!("pause-{toggle}"),
                        if enabled {
                            "Pause & revoke grants"
                        } else {
                            "Enable"
                        },
                        !root.busy,
                        false,
                        cx,
                        move |root, _, cx| {
                            root.request(
                                Command::Mcp(McpCommand::SetEnabled {
                                    connection_id: toggle.clone(),
                                    enabled: !enabled,
                                }),
                                cx,
                            )
                        },
                    )),
            );
        if let Some(error) = &c.error {
            card = card.child(
                div()
                    .text_color(theme::active().state_danger)
                    .child(error.clone()),
            );
        }
        if matches!(c.config, ServerConfig::Http { .. }) {
            let id = c.id.clone();
            card = card.child(button(
                format!("auth-{id}"),
                "Sign in through browser",
                !root.busy && enabled,
                false,
                cx,
                move |root, _, cx| {
                    let client_id = value(&root.oauth_client_id, cx);
                    root.request(
                        Command::Mcp(McpCommand::Authenticate {
                            connection_id: id.clone(),
                            client_id: (!client_id.trim().is_empty())
                                .then(|| client_id.trim().to_owned()),
                        }),
                        cx,
                    );
                },
            ));
        }
        for tool in &c.tools {
            let allowed = root.snapshot.mcp.grants.iter().any(|g| {
                g.connection_id == c.id
                    && Some(&g.workspace_id) == root.selection.workspace.as_ref()
                    && g.tool_name == tool.name
                    && g.schema_hash == tool.schema_hash
            });
            let id = c.id.clone();
            let name = tool.name.clone();
            let hash = tool.schema_hash.clone();
            card = card.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .child(div().child(tool.name.clone()))
                    .child(note(tool.description.clone()))
                    .child(note(if tool.read_only { "Server declares this tool read-only. Granting access permits chat lookups without another prompt." } else { "Action or unspecified effect. Chat asks you to approve each call; unattended responsibilities use the grant directly." }))
                    .child(note(format!("Input schema: {}", tool.input_schema)))
                    .child(button(
                        format!("grant-{id}-{name}"),
                        if allowed {
                            "Revoke unattended access"
                        } else {
                            "Allow unattended use"
                        },
                        !root.busy && enabled && c.error.is_none(),
                        allowed,
                        cx,
                        move |root, _, cx| {
                            root.request(
                                Command::Mcp(McpCommand::SetWorkspaceToolGrant {
                                    workspace_id: root.selection.workspace.clone().unwrap_or_default(),
                                    connection_id: id.clone(),
                                    tool_name: name.clone(),
                                    schema_hash: hash.clone(),
                                    allowed: !allowed,
                                }),
                                cx,
                            )
                        },
                    )),
            );
        }
        body = body.child(card);
    }
    body = body.child(field("OAuth client ID (optional, for the next sign-in)", &root.oauth_client_id))
        .child(note("Browser sign-in uses server-provided OAuth metadata. If registration is unavailable, supply your registered public client ID. Sign-in never grants tool access."))
        .child(responsibilities(root, cx));
    body.child(heading("Add an MCP server", "Credentials and permissions are independent for every connection."))
        .child(field("Connection name", &root.server_label))
        .child(button("mcp-scope", if root.global_server { "Scope: Global · grant separately in each workspace" } else { "Scope: This workspace" }, !root.busy, root.global_server, cx, |root, _, cx| { root.global_server = !root.global_server; cx.notify(); }))
        .child(button("mcp-transport", if root.local_server { "Transport: local process" } else { "Transport: remote HTTP" }, !root.busy, root.local_server, cx, |root, _, cx| { root.local_server = !root.local_server; root.trust_server = false; cx.notify(); }))
        .child(field(if root.local_server { "Absolute executable path" } else { "Server URL" }, &root.server_target))
        .when(root.local_server, |body| body.child(field("Arguments (JSON array)", &root.server_args))
            .child(note("Local servers execute code on your computer, outside worker sandboxes. Only trust software you know. Nothing is downloaded automatically."))
            .child(button("mcp-trust", if root.trust_server { "Local process trusted" } else { "I trust this local executable and its arguments" }, !root.busy, root.trust_server, cx, |root, _, cx| { root.trust_server = !root.trust_server; cx.notify(); })))
        .child(field("Optional credentials", &root.api_key))
        .child(note("Secret JSON: {\"bearer\":\"…\"} for HTTP, or {\"environment\":{\"TOKEN\":\"…\"}} for local servers. Stored in Keychain, never in snapshots. Do not put secrets in URLs or arguments."))
        .child(note("Grant only tools you trust to run unattended. A server's read-only label is not a security guarantee. Tool schema changes require renewed consent."))
        .when_some(root.connection_error.clone(), |body, e| body.child(div().text_color(theme::active().state_danger).child(e)))
        .child(button("add-mcp", "Add server", !root.busy && (!root.local_server || root.trust_server), true, cx, |root, _, cx| root.connect_mcp(cx)))
        .into_any_element()
}

pub(super) fn refresh_found_connections(
    root: &mut super::WorkspaceRoot,
    cx: &mut gpui::Context<super::WorkspaceRoot>,
) {
    use neko_protocol::setup_import::ImportCommand;
    let Some((workspace_id, repository)) = root
        .snapshot
        .workspaces
        .iter()
        .find(|workspace| Some(&workspace.id) == root.selection.workspace.as_ref())
        .map(|workspace| (workspace.id.clone(), workspace.repository.clone()))
    else {
        return;
    };
    root.found_connections_scanned_workspace = Some(workspace_id);
    root.request(
        super::Command::SetupImport(ImportCommand::Discover {
            repositories: vec![repository],
            source_id: None,
        }),
        cx,
    );
}

fn found_connections(
    root: &super::WorkspaceRoot,
    cx: &mut gpui::Context<super::WorkspaceRoot>,
) -> gpui::AnyElement {
    use super::*;
    use neko_protocol::mcp_host::McpCommand;
    let Some(workspace) = root
        .snapshot
        .workspaces
        .iter()
        .find(|workspace| Some(&workspace.id) == root.selection.workspace.as_ref())
    else {
        return div().into_any_element();
    };
    let candidates = root
        .snapshot
        .import_preview
        .candidates
        .iter()
        .filter(|candidate| candidate_in_workspace(candidate, &workspace.repository))
        .collect::<Vec<_>>();
    let mut section = div()
        .flex()
        .flex_col()
        .gap(px(10.))
        .child(heading(
            "Found in your setup",
            "Connections from your local agent configuration. Neko keeps a link; the source stays in place. Tool access still needs your approval.",
        ))
        .child(button(
            "refresh-found-connections",
            "Refresh local setup",
            !root.busy,
            false,
            cx,
            |root, _, cx| refresh_found_connections(root, cx),
        ));
    if candidates.is_empty() {
        return section
            .child(note("No connections found yet. Refresh to check Codex, Claude and other local setup for this workspace."))
            .into_any_element();
    }
    for candidate in candidates {
        let linked = root.snapshot.mcp.connections.iter().find(|connection| {
            connection.available_in(&workspace.id)
                && connection
                    .source_link
                    .as_ref()
                    .is_some_and(|source| source.candidate_id == candidate.id)
        });
        let source_disabled = candidate
            .metadata
            .get("enabled_at_source")
            .map(String::as_str)
            == Some("false");
        let problem = candidate
            .problem
            .as_deref()
            .or(source_disabled.then_some("Disabled in source setup"));
        let existing_copy = candidate.workspace.is_some()
            && candidate
                .metadata
                .get("already_in_neko")
                .map(String::as_str)
                == Some("true")
            && linked.is_none();
        let local = candidate_is_local(candidate);
        let available = problem.is_none() && linked.is_none() && !root.busy;
        let workspace_id = workspace.id.clone();
        let candidate_id = candidate.id.clone();
        let scope = candidate
            .workspace
            .as_deref()
            .map_or("Global", |_| "This workspace");
        let status = if let Some(connection) = linked {
            connection.error.as_ref().map_or_else(
                || {
                    format!(
                        "Linked as {} · tools still need your approval",
                        connection.label
                    )
                },
                |error| format!("Needs review: {error}"),
            )
        } else if let Some(problem) = problem {
            format!("Unavailable: {problem}")
        } else if existing_copy {
            "Already saved in Neko · linking will use the source instead and reset tool grants"
                .into()
        } else if local {
            "Local process · review before trusting".into()
        } else {
            "Ready to link · tools still need your approval".into()
        };
        let mut row = div()
            .flex()
            .flex_col()
            .gap(px(6.))
            .p(px(14.))
            .rounded(px(10.))
            .bg(theme::active().surface_raised)
            .child(
                div()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child(candidate.name.clone()),
            )
            .child(note(format!("{} · {}", candidate.source, scope)))
            .child(note(
                candidate
                    .metadata
                    .get("config_summary")
                    .cloned()
                    .unwrap_or_else(|| "Review this server in its source file".into()),
            ))
            .child(note(format!(
                "Configuration {}",
                candidate
                    .metadata
                    .get("config_fingerprint")
                    .map(String::as_str)
                    .unwrap_or("unavailable")
            )))
            .child(note(status));
        if linked.is_none() && problem.is_none() {
            if local {
                row = row.child(note("This executable can run on your Mac outside the agent sandbox. Linking trusts this local process, but never grants any of its tools."));
            }
            if existing_copy {
                row = row.child(note("This replaces the matching saved definition. Its current tool approvals will be cleared; source credentials stay in their original setup."));
            }
            row = row.child(button(
                format!("link-found-{candidate_id}"),
                if local {
                    "Trust local process & link"
                } else if existing_copy {
                    "Use source instead"
                } else {
                    "Link connection"
                },
                available,
                false,
                cx,
                move |root, _, cx| {
                    root.request(
                        Command::Mcp(McpCommand::LinkSource {
                            workspace_id: workspace_id.clone(),
                            candidate_id: candidate_id.clone(),
                            trust_local_process: local,
                        }),
                        cx,
                    )
                },
            ));
        } else if linked.is_some() && problem.is_none() {
            row = row.child(button(
                format!("relink-found-{candidate_id}"),
                if local {
                    "Trust current process & relink"
                } else {
                    "Relink current source"
                },
                !root.busy,
                false,
                cx,
                move |root, _, cx| {
                    root.request(
                        Command::Mcp(McpCommand::LinkSource {
                            workspace_id: workspace_id.clone(),
                            candidate_id: candidate_id.clone(),
                            trust_local_process: local,
                        }),
                        cx,
                    )
                },
            ));
        }
        section = section.child(row);
    }
    section.into_any_element()
}

fn responsibilities(
    root: &super::WorkspaceRoot,
    cx: &mut gpui::Context<super::WorkspaceRoot>,
) -> gpui::AnyElement {
    use super::*;
    use neko_protocol::mcp_host::McpCommand;
    let mut body = div().flex().flex_col().gap(px(10.)).child(heading("Responsibilities", "Check every 10 minutes while Neko is running. Failures back off; missed checks are not replayed."));
    for r in root
        .snapshot
        .mcp
        .responsibilities
        .iter()
        .filter(|r| root.selection.includes(&r.workspace_id))
    {
        let mut toggled = r.clone();
        toggled.enabled = !r.enabled;
        let mut permission = r.clone();
        permission.prepare_low_risk = !r.prepare_low_risk;
        let id = r.id.clone();
        let edit = r.clone();
        body = body.child(
            div()
                .flex()
                .flex_col()
                .gap(px(8.))
                .p(px(14.))
                .rounded(px(10.))
                .bg(theme::active().surface_raised)
                .child(div().child(r.instruction.clone()))
                .child(note(format!(
                    "{} · {} · {} selected connections · {} consecutive failures",
                    if r.enabled { "Active" } else { "Paused" },
                    if r.prepare_low_risk {
                        "Low-risk local fixes allowed"
                    } else {
                        "Plan only"
                    },
                    r.connection_ids.len(),
                    r.failures
                )))
                .child(note(if r.last_result.is_empty() {
                    "No completed check yet.".into()
                } else {
                    r.last_result.clone()
                }))
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap(px(8.))
                        .child(button(
                            format!("responsibility-pause-{id}"),
                            if r.enabled { "Pause" } else { "Resume" },
                            !root.busy,
                            false,
                            cx,
                            move |root, _, cx| {
                                root.request(
                                    Command::Mcp(McpCommand::SaveResponsibility {
                                        responsibility: toggled.clone(),
                                    }),
                                    cx,
                                )
                            },
                        ))
                        .child(button(
                            format!("responsibility-policy-{id}"),
                            if r.prepare_low_risk {
                                "Require approval for fixes"
                            } else {
                                "Allow low-risk local fixes"
                            },
                            !root.busy,
                            false,
                            cx,
                            move |root, _, cx| {
                                root.request(
                                    Command::Mcp(McpCommand::SaveResponsibility {
                                        responsibility: permission.clone(),
                                    }),
                                    cx,
                                )
                            },
                        ))
                        .child(button(
                            format!("responsibility-wake-{id}"),
                            "Check now",
                            !root.busy && r.enabled,
                            true,
                            cx,
                            move |root, _, cx| {
                                root.request(
                                    Command::Mcp(McpCommand::Wake {
                                        responsibility_id: id.clone(),
                                    }),
                                    cx,
                                )
                            },
                        ))
                        .child(button(
                            format!("responsibility-edit-{}", edit.id),
                            "Edit",
                            !root.busy,
                            false,
                            cx,
                            move |root, _, cx| {
                                root.responsibility_editing = Some(edit.id.clone());
                                root.responsibility_instruction.update(cx, |field, cx| {
                                    field.set_content(&edit.instruction, cx)
                                });
                                root.responsibility_connections = edit.connection_ids.clone();
                                root.responsibility_prepare = edit.prepare_low_risk;
                                cx.notify();
                            },
                        )),
                ),
        );
    }
    body = body.child(field(if root.responsibility_editing.is_some() { "Edit responsibility" } else { "New responsibility" }, &root.responsibility_instruction))
        .when(root.responsibility_editing.is_some(), |body| body.child(button("cancel-responsibility-edit", "Cancel editing", !root.busy, false, cx, |root, _, cx| {
            root.responsibility_editing = None;
            root.responsibility_connections.clear();
            root.responsibility_prepare = false;
            root.responsibility_instruction.update(cx, |field, cx| field.clear(cx));
            cx.notify();
        }))).child(note("Select only the connections this responsibility needs. Only explicitly granted tools are available."));
    for c in root.snapshot.mcp.connections.iter().filter(|c| {
        root.selection
            .workspace
            .as_ref()
            .is_some_and(|w| c.available_in(w))
    }) {
        let id = c.id.clone();
        let selected = root.responsibility_connections.contains(&id);
        body = body.child(button(
            format!("responsibility-connection-{id}"),
            format!("{} {}", if selected { "✓" } else { "+" }, c.label),
            !root.busy,
            selected,
            cx,
            move |root, _, cx| {
                if root.responsibility_connections.contains(&id) {
                    root.responsibility_connections.retain(|c| c != &id);
                } else {
                    root.responsibility_connections.push(id.clone());
                }
                cx.notify();
            },
        ));
    }
    body.child(button("responsibility-prepare", if root.responsibility_prepare { "Low-risk local fixes allowed" } else { "Plan only — fixes need approval" }, !root.busy, root.responsibility_prepare, cx, |root, _, cx| { root.responsibility_prepare = !root.responsibility_prepare; cx.notify(); }))
        .child(note("Local fixes require fresh source evidence and a bounded low-risk assessment. No push, PR, message or deployment permission is implied. Tool grants themselves may allow remote changes: review them carefully."))
        .child(button("responsibility-add", if root.responsibility_editing.is_some() { "Save responsibility" } else { "Start responsibility" }, !root.busy, true, cx, |root, _, cx| {
            let Some(workspace) = root.selection.workspace.as_deref() else { return; };
            match responsibility_from_form(workspace, &value(&root.responsibility_instruction, cx), &root.responsibility_connections, root.responsibility_prepare) {
                Ok(mut responsibility) => {
                    if let Some(id) = &root.responsibility_editing {
                        responsibility.id = id.clone();
                        responsibility.enabled = root.snapshot.mcp.responsibilities.iter().find(|r| &r.id == id).is_some_and(|r| r.enabled);
                    }
                    root.request(Command::Mcp(McpCommand::SaveResponsibility { responsibility }), cx);
                },
                Err(error) => { root.connection_error = Some(error); cx.notify(); }
            }
        })).into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn candidate(workspace: Option<&str>, transport: &str) -> ImportCandidate {
        ImportCandidate {
            id: "source-id".into(),
            kind: ImportCandidateKind::Connection,
            source: "Codex".into(),
            scope: workspace.map_or_else(|| "global".into(), |path| format!("workspace:{path}")),
            workspace: workspace.map(str::to_owned),
            name: "example".into(),
            metadata: BTreeMap::from([("transport".into(), transport.into())]),
            problem: None,
        }
    }

    #[test]
    fn found_connections_are_limited_to_global_and_selected_workspace() {
        assert!(candidate_in_workspace(&candidate(None, "http"), "/repo/a"));
        assert!(candidate_in_workspace(
            &candidate(Some("/repo/a"), "http"),
            "/repo/a"
        ));
        assert!(!candidate_in_workspace(
            &candidate(Some("/repo/b"), "http"),
            "/repo/a"
        ));
        let mut skill = candidate(None, "http");
        skill.kind = ImportCandidateKind::Skill;
        assert!(!candidate_in_workspace(&skill, "/repo/a"));
    }

    #[test]
    fn unknown_transport_requires_local_process_trust() {
        assert!(!candidate_is_local(&candidate(None, "http")));
        assert!(candidate_is_local(&candidate(None, "stdio")));
        assert!(candidate_is_local(&candidate(None, "unsupported")));
    }

    #[test]
    fn initial_discovery_runs_once_per_workspace() {
        assert!(!needs_initial_discovery(None, None));
        assert!(needs_initial_discovery(Some("one"), None));
        assert!(!needs_initial_discovery(Some("one"), Some("one")));
        assert!(needs_initial_discovery(Some("two"), Some("one")));
    }
    #[test]
    fn responsibility_form_requires_explicit_scope_and_defaults_to_planning() {
        assert!(responsibility_from_form("w", "Watch bugs", &[], false).is_err());
        assert!(responsibility_from_form("w", "  ", &["c".into()], false).is_err());
        let r = responsibility_from_form("w", " Watch my assigned bugs ", &["c".into()], false)
            .unwrap();
        assert_eq!(r.instruction, "Watch my assigned bugs");
        assert!(!r.prepare_low_risk);
        assert!(r.enabled);
        assert!(r.id.is_empty());
    }
    #[test]
    fn local_arguments_are_an_array_not_a_shell_command() {
        assert_eq!(
            config_from_form(
                true,
                "/usr/bin/node",
                r#"["/path with spaces/server.js","--read-only"]"#
            )
            .unwrap(),
            ServerConfig::Stdio {
                command: "/usr/bin/node".into(),
                args: vec!["/path with spaces/server.js".into(), "--read-only".into()],
                cwd: None,
            }
        );
        assert!(config_from_form(true, "node", "[]").is_err());
        assert!(config_from_form(true, "/usr/bin/node", "$(touch /tmp/no)").is_err());
    }
    #[test]
    fn remote_configuration_does_not_require_local_arguments() {
        assert_eq!(
            config_from_form(false, " https://example.com/mcp ", "ignored").unwrap(),
            ServerConfig::Http {
                url: "https://example.com/mcp".into()
            }
        );
    }
}
