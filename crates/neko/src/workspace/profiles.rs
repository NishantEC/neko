//! User-owned identities and explicit directional memory sharing.
use super::*;
use neko_protocol::agent_profiles::{AgentProfile, ProfileCommand};

pub(super) struct Form {
    editing: Option<String>,
    name: Entity<TextField>,
    instructions: Entity<TextField>,
}
impl Form {
    pub(super) fn saved(
        &mut self,
        submitted: &AgentProfile,
        snapshot: &Snapshot,
        previous: &[String],
    ) {
        self.editing = snapshot
            .agent_profiles
            .profiles
            .iter()
            .find(|p| {
                if submitted.id.is_empty() {
                    !previous.contains(&p.id)
                        && p.name == submitted.name.trim()
                        && p.instructions == submitted.instructions
                } else {
                    p.id == submitted.id
                }
            })
            .map(|p| p.id.clone());
    }
    pub(super) fn new(cx: &mut App) -> Self {
        Self {
            editing: None,
            name: input("Profile name, for example Work", cx),
            instructions: input("How this Neko should work", cx),
        }
    }
}

pub(super) fn view(root: &WorkspaceRoot, cx: &mut Context<WorkspaceRoot>) -> gpui::AnyElement {
    let state = &root.snapshot.agent_profiles;
    let mut body=div().id("agent-profiles").size_full().overflow_y_scroll().flex().flex_col().gap(px(16.))
        .child(heading("Agent profiles","Separate instructions and memories. Workspace tools remain explicitly granted; a profile is not an operating-system privacy boundary."));
    for profile in &state.profiles {
        let id = profile.id.clone();
        let edit = profile.clone();
        let assign = profile.id.clone();
        let owned = root
            .snapshot
            .workspaces
            .iter()
            .filter(|w| state.owner(&w.id) == profile.id)
            .map(|w| w.name.clone())
            .collect::<Vec<_>>();
        let mut card = div()
            .flex()
            .flex_col()
            .gap(px(10.))
            .p(px(16.))
            .rounded(px(12.))
            .bg(theme::active().surface_raised)
            .child(div().text_size(px(16.)).child(profile.name.clone()))
            .child(note(profile.instructions.clone()))
            .child(note(format!(
                "Workspaces: {}",
                if owned.is_empty() {
                    "None".into()
                } else {
                    owned.join(", ")
                }
            )))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(8.))
                    .child(button(
                        format!("profile-active-{id}"),
                        if state.active_profile_id == id {
                            "Default for unscoped chat"
                        } else {
                            "Use for unscoped chat"
                        },
                        !root.busy,
                        state.active_profile_id == id,
                        cx,
                        move |r, _, cx| {
                            r.request(
                                Command::AgentProfiles(ProfileCommand::SetActive {
                                    profile_id: id.clone(),
                                }),
                                cx,
                            )
                        },
                    ))
                    .child(button(
                        format!("profile-edit-{}", profile.id),
                        "Edit instructions",
                        !root.busy,
                        false,
                        cx,
                        move |r, _, cx| {
                            r.profile_form.editing = Some(edit.id.clone());
                            r.profile_form
                                .name
                                .update(cx, |f, cx| f.set_content(&edit.name, cx));
                            r.profile_form
                                .instructions
                                .update(cx, |f, cx| f.set_content(&edit.instructions, cx));
                            cx.notify();
                        },
                    )),
            )
            .child(button(
                format!("profile-assign-{}", profile.id),
                "Assign selected workspace",
                !root.busy && root.selection.workspace.is_some(),
                false,
                cx,
                move |r, _, cx| {
                    if let Some(workspace_id) = r.selection.workspace.clone() {
                        r.request(
                            Command::AgentProfiles(ProfileCommand::AssignWorkspace {
                                workspace_id,
                                profile_id: assign.clone(),
                            }),
                            cx,
                        );
                    }
                },
            ));
        for source in state.profiles.iter().filter(|s| s.id != profile.id) {
            let reader_id = profile.id.clone();
            let source_id = source.id.clone();
            let allowed = state
                .read_grants
                .iter()
                .any(|g| g.reader_id == reader_id && g.source_id == source_id);
            card = card.child(button(
                format!("profile-share-{reader_id}-{source_id}"),
                format!(
                    "{} {} to read {}’s profile memories",
                    if allowed {
                        "Revoke permission for"
                    } else {
                        "Allow"
                    },
                    profile.name,
                    source.name
                ),
                !root.busy,
                false,
                cx,
                move |r, _, cx| {
                    r.request(
                        Command::AgentProfiles(ProfileCommand::SetReadGrant {
                            reader_id: reader_id.clone(),
                            source_id: source_id.clone(),
                            allowed: !allowed,
                        }),
                        cx,
                    )
                },
            ));
        }
        body = body.child(card);
    }
    body.child(note("Sharing is one-way and covers profile memories only—not workspace notes, conversations, credentials or tools. Workspace reassignment is checked against active work."))
        .child(heading(if root.profile_form.editing.is_some(){"Edit profile"}else{"New profile"},"Instructions shape behavior; they do not expand permissions."))
        .child(field("Name",&root.profile_form.name)).child(field("Instructions",&root.profile_form.instructions))
        .child(button("profile-save","Save profile",!root.busy,false,cx,|r,_,cx|r.request(Command::AgentProfiles(ProfileCommand::Save{profile:AgentProfile{id:r.profile_form.editing.clone().unwrap_or_default(),name:value(&r.profile_form.name,cx),instructions:value(&r.profile_form.instructions,cx)}}),cx)))
        .child(button("profile-new","Clear form for a new profile",!root.busy,false,cx,|r,_,cx|{r.profile_form=Form::new(cx);cx.notify();})).into_any_element()
}
