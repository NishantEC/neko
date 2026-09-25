//! Explicit workspace instruction activation and exact-content review.
use super::*;
use neko_protocol::skills::SkillCommand;

fn filtered_skills<'a>(
    state: &'a neko_protocol::skills::SkillState,
    workspace: &str,
    query: &str,
    browse: bool,
) -> Vec<&'a neko_protocol::skills::Skill> {
    let query = query.trim().to_lowercase();
    state
        .available
        .iter()
        .filter(|s| s.workspace_id.as_deref().is_none_or(|id| id == workspace))
        .filter(|s| {
            browse
                || !query.is_empty()
                || state
                    .enabled
                    .iter()
                    .any(|e| e.workspace_id == workspace && e.path == s.path)
        })
        .filter(|s| {
            query.is_empty()
                || format!("{} {} {}", s.name, s.description, s.source)
                    .to_lowercase()
                    .contains(&query)
        })
        .collect()
}

pub(super) fn view(root: &WorkspaceRoot, cx: &mut Context<WorkspaceRoot>) -> gpui::AnyElement {
    let Some(workspace) = root.selection.workspace.as_ref() else {
        return div().into_any_element();
    };
    let mut body = div().flex().flex_col().gap(px(10.))
        .child(heading("Skills", "Enable instructions separately for each workspace. Skills do not grant tools or filesystem privacy."))
        .child(button("discover-skills", "Refresh local skills", !root.busy, false, cx, |root, _, cx| root.request(Command::Skills(SkillCommand::Refresh), cx)))
        .child(note("Discovers Codex, Agents, Claude, Neko and workspace skill folders. Review source instructions before enabling."))
        .child(field("Search installed skills", &root.skill_filter))
        .child(button("skill-library", if root.browse_skills { "Show enabled only" } else { "Browse all installed skills" }, true, root.browse_skills, cx, |root, _, cx| { root.browse_skills = !root.browse_skills; root.skill_limit = 20; cx.notify(); }));
    let visible = filtered_skills(
        &root.snapshot.skills,
        workspace,
        &value(&root.skill_filter, cx),
        root.browse_skills,
    );
    if visible.is_empty() {
        body = body.child(note(
            "No matching enabled skills. Search or browse the library to add one.",
        ));
    }
    for (index, skill) in visible.iter().take(root.skill_limit).enumerate() {
        let enabled = root
            .snapshot
            .skills
            .enabled
            .iter()
            .find(|s| s.workspace_id == *workspace && s.path == skill.path);
        let changed = enabled.is_some_and(|e| e.content_hash != skill.content_hash);
        let active = enabled.is_some();
        let path = skill.path.clone();
        let review_path = path.clone();
        let hash = skill.content_hash.clone();
        let workspace_id = workspace.clone();
        let disable = SkillCommand::SetEnabled {
            workspace_id: workspace.clone(),
            path: skill.path.clone(),
            content_hash: skill.content_hash.clone(),
            enabled: false,
        };
        body = body.child(
            div()
                .flex()
                .flex_col()
                .gap(px(6.))
                .p(px(12.))
                .rounded(px(10.))
                .bg(theme::active().surface_raised)
                .child(div().child(skill.name.clone()))
                .child(note(format!(
                    "{} · {}{}",
                    skill.source,
                    if skill.workspace_id.is_some() {
                        "Workspace scope"
                    } else {
                        "Global source · activation is workspace-only"
                    },
                    if changed {
                        " · Changed: re-enable required"
                    } else {
                        ""
                    }
                )))
                .child(note(skill.description.clone()))
                .child(note(skill.path.clone()))
                .child(button(
                    format!("skill-review-{index}"),
                    "Open instructions to review",
                    !root.busy,
                    false,
                    cx,
                    move |_, _, cx| cx.open_url(&format!("file://{review_path}")),
                ))
                .child(button(
                    format!("skill-toggle-{index}"),
                    if changed {
                        "Enable reviewed changes"
                    } else if active {
                        "Disable in this workspace"
                    } else {
                        "Enable in this workspace"
                    },
                    !root.busy,
                    active,
                    cx,
                    move |root, _, cx| {
                        root.request(
                            Command::Skills(SkillCommand::SetEnabled {
                                workspace_id: workspace_id.clone(),
                                path: path.clone(),
                                content_hash: hash.clone(),
                                enabled: !active || changed,
                            }),
                            cx,
                        )
                    },
                ))
                .when(changed, |card| {
                    card.child(button(
                        format!("skill-disable-changed-{index}"),
                        "Disable in this workspace",
                        !root.busy,
                        false,
                        cx,
                        move |root, _, cx| root.request(Command::Skills(disable.clone()), cx),
                    ))
                }),
        );
    }
    if visible.len() > root.skill_limit {
        body = body
            .child(note(format!(
                "Showing {} of {} matches",
                root.skill_limit,
                visible.len()
            )))
            .child(button(
                "skill-more",
                "Show 20 more",
                true,
                false,
                cx,
                |root, _, cx| {
                    root.skill_limit += 20;
                    cx.notify();
                },
            ));
    }
    // Missing files must still have a recovery action.
    for (index, enabled) in root
        .snapshot
        .skills
        .enabled
        .iter()
        .filter(|e| {
            e.workspace_id == *workspace
                && !root
                    .snapshot
                    .skills
                    .available
                    .iter()
                    .any(|s| available_in_workspace(s, &e.path, workspace))
        })
        .enumerate()
    {
        let enabled = enabled.clone();
        body = body
            .child(note(format!(
                "Unavailable enabled skill: {}. Runs stop until restored or disabled.",
                enabled.path
            )))
            .child(button(
                format!("missing-skill-{index}"),
                "Disable unavailable skill",
                !root.busy,
                false,
                cx,
                move |root, _, cx| {
                    root.request(
                        Command::Skills(SkillCommand::SetEnabled {
                            workspace_id: enabled.workspace_id.clone(),
                            path: enabled.path.clone(),
                            content_hash: enabled.content_hash.clone(),
                            enabled: false,
                        }),
                        cx,
                    )
                },
            ));
    }
    body.child(proposals(root, cx)).into_any_element()
}

pub(super) fn catalog(root: &WorkspaceRoot, cx: &mut Context<WorkspaceRoot>) -> gpui::AnyElement {
    div().flex().flex_col().gap(px(10.)).child(heading("Find a skill", "Browse skills.sh, then paste the GitHub URL of a standalone SKILL.md."))
        .child(button("browse-skills", "Browse skills.sh", true, false, cx, |_, _, cx| cx.open_url("https://skills.sh")))
        .child(field("GitHub SKILL.md URL", &root.skill_source))
        .child(note("Preview downloads only this instruction file (64 KB maximum). Scripts, references and other assets are not installed; choose a self-contained skill. No remote installer is executed."))
        .child(button("preview-skill", "Fetch for review", !root.busy, true, cx, |root, _, cx| {
            if let Some(workspace_id) = root.selection.workspace.clone() {
                root.request(Command::Skills(SkillCommand::PreviewRepository { workspace_id, url: value(&root.skill_source, cx) }), cx);
            }
        })).child(proposals(root,cx)).into_any_element()
}

fn proposals(root: &WorkspaceRoot, cx: &mut Context<WorkspaceRoot>) -> gpui::AnyElement {
    let Some(workspace) = root.selection.workspace.as_ref() else {
        return div().into_any_element();
    };
    let mut body = div().flex().flex_col().gap(px(10.));
    for proposal in root
        .snapshot
        .skills
        .proposals
        .iter()
        .filter(|p| p.workspace_id == *workspace)
    {
        let accept = proposal.clone();
        let reject = proposal.clone();
        let mut card = div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .p(px(14.))
            .rounded(px(10.))
            .bg(theme::active().surface_raised)
            .child(div().child(format!("Review: {}", proposal.name)))
            .child(note(proposal.source.clone()))
            .child(note(proposal.audit_status.clone()));
        if let Some(url) = &proposal.audit_url {
            let url = url.clone();
            let key = (proposal.id.clone(), proposal.content_hash.clone());
            let opened = root.opened_skill_audits.contains(&key);
            let confirmation = SkillCommand::ConfirmAuditReview {
                id: proposal.id.clone(),
                content_hash: proposal.content_hash.clone(),
                audit_url: url.clone(),
            };
            card = card.child(button(
                format!("skill-audit-{}", proposal.id),
                "View skills.sh source and audit links",
                true,
                false,
                cx,
                move |root, _, cx| {
                    cx.open_url(&url);
                    if !root.opened_skill_audits.contains(&key) { root.opened_skill_audits.push(key.clone()); }
                    cx.notify();
                },
            ))
            .child(note("Read the published audit results at the opened link. If no results are available, do not confirm or install. Confirmation records your review, not a passing audit."))
            .child(button(format!("skill-audit-confirm-{}", proposal.id), "I reviewed the linked published audit results", !root.busy && opened, false, cx,
                move |root, _, cx| root.request(Command::Skills(confirmation.clone()), cx)));
        }
        card = card.child(div().id(SharedString::from(format!("skill-content-{}", proposal.id))).max_h(px(320.)).overflow_y_scroll().child(proposal.body.clone()))
            .child(note("Accept saves exactly these instructions into Neko’s own skill folder. Enable it separately after installation."))
            .child(button(format!("skill-accept-{}", proposal.id), "Accept & save reviewed content", !root.busy && audit_review_complete(proposal), true, cx, move |root, _, cx| root.request(Command::Skills(SkillCommand::DecideProposal { id: accept.id.clone(), content_hash: accept.content_hash.clone(), accept: true }), cx)))
            .child(button(format!("skill-reject-{}", proposal.id), "Reject", !root.busy, false, cx, move |root, _, cx| root.request(Command::Skills(SkillCommand::DecideProposal { id: reject.id.clone(), content_hash: reject.content_hash.clone(), accept: false }), cx)));
        body = body.child(card);
    }
    body.into_any_element()
}

fn audit_review_complete(proposal: &neko_protocol::skills::SkillProposal) -> bool {
    proposal.audit_url.is_none()
        || proposal.audit_reviewed_hash.as_ref() == Some(&proposal.content_hash)
}

fn available_in_workspace(
    skill: &neko_protocol::skills::Skill,
    path: &str,
    workspace: &str,
) -> bool {
    skill.path == path
        && skill
            .workspace_id
            .as_deref()
            .is_none_or(|id| id == workspace)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn collapsed_library_keeps_enabled_skills_and_search_respects_scope() {
        let mut state = neko_protocol::skills::SkillState::default();
        for (path, name, scope) in [
            ("/a", "Active", None),
            ("/b", "Browse", None),
            ("/c", "Browse private", Some("other")),
        ] {
            state.available.push(neko_protocol::skills::Skill {
                path: path.into(),
                name: name.into(),
                description: String::new(),
                source: "fixture".into(),
                workspace_id: scope.map(str::to_owned),
                content_hash: "h".into(),
            });
        }
        state.enabled.push(neko_protocol::skills::EnabledSkill {
            workspace_id: "w".into(),
            path: "/a".into(),
            content_hash: "h".into(),
        });
        assert_eq!(filtered_skills(&state, "w", "", false).len(), 1);
        assert_eq!(filtered_skills(&state, "w", "", true).len(), 2);
        assert_eq!(filtered_skills(&state, "w", "browse", false).len(), 1);
        assert_eq!(
            filtered_skills(&state, "w", "ACTIVE", false)[0].name,
            "Active"
        );
    }
    #[test]
    fn other_workspace_canonical_path_does_not_hide_unavailable_recovery() {
        let mut skill = neko_protocol::skills::Skill {
            path: "/shared/SKILL.md".into(),
            name: "Shared".into(),
            description: String::new(),
            source: "Workspace".into(),
            workspace_id: Some("other".into()),
            content_hash: "hash".into(),
        };
        assert!(!available_in_workspace(
            &skill,
            "/shared/SKILL.md",
            "current"
        ));
        skill.workspace_id = Some("current".into());
        assert!(available_in_workspace(
            &skill,
            "/shared/SKILL.md",
            "current"
        ));
        skill.workspace_id = None;
        assert!(available_in_workspace(
            &skill,
            "/shared/SKILL.md",
            "current"
        ));
        assert!(!available_in_workspace(
            &skill,
            "/different/SKILL.md",
            "current"
        ));
    }
}
