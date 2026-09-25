//! Explicitly enabled local schedules; each run creates a normal approval-gated ticket.
use super::*;
use neko_protocol::scheduled_plans::{Schedule, ScheduleCommand};

fn workspace_for_save(existing: &Schedule, selected: Option<&str>) -> Option<String> {
    existing
        .workspace_id
        .clone()
        .or_else(|| selected.map(str::to_owned))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn editing_a_then_selecting_b_cannot_move_the_schedule() {
        let existing = Schedule {
            id: "s".into(),
            workspace_id: Some("a".into()),
            ..Default::default()
        };
        assert_eq!(workspace_for_save(&existing, Some("b")), Some("a".into()));
        assert_eq!(
            workspace_for_save(&Schedule::default(), Some("b")),
            Some("b".into())
        );
    }
}

pub(super) struct Form {
    editing: Option<String>,
    removing: Option<String>,
    name: Entity<TextField>,
    prompt: Entity<TextField>,
    rule: Entity<TextField>,
    timezone: Entity<TextField>,
}
impl Form {
    pub(super) fn saved(
        &mut self,
        submitted: &Schedule,
        snapshot: &Snapshot,
        previous: &[String],
        cx: &App,
    ) {
        if value(&self.name, cx) != submitted.name || value(&self.prompt, cx) != submitted.prompt {
            return;
        }
        self.editing = snapshot
            .schedules
            .iter()
            .find(|s| {
                if submitted.id.is_empty() {
                    !previous.contains(&s.id)
                        && s.name == submitted.name
                        && s.prompt == submitted.prompt
                        && s.workspace_id == submitted.workspace_id
                } else {
                    s.id == submitted.id
                }
            })
            .map(|s| s.id.clone());
    }
    pub(super) fn new(cx: &mut App) -> Self {
        Self {
            editing: None,
            removing: None,
            name: input("Schedule name", cx),
            prompt: input("What should Neko inspect and plan?", cx),
            rule: input("FREQ=DAILY;BYHOUR=9;BYMINUTE=0", cx),
            timezone: input("IANA timezone, for example Asia/Kolkata or UTC", cx),
        }
    }
}

pub(super) fn view(root: &WorkspaceRoot, cx: &mut Context<WorkspaceRoot>) -> gpui::Div {
    let mut body=div().flex().flex_col().gap(px(12.)).mt(px(20.)).child(heading("Scheduled plans","Saved and imported schedules stay paused until you explicitly enable them. The Mac and Neko daemon must be running. Missed intervals do not trigger a burst."));
    for schedule in root.snapshot.schedules.iter().filter(|s| {
        s.workspace_id.is_none()
            || s.workspace_id
                .as_ref()
                .is_some_and(|id| root.selection.includes(id))
    }) {
        let id = schedule.id.clone();
        let enable = id.clone();
        let run = id.clone();
        let edit = schedule.clone();
        let remove = id.clone();
        let workspace = schedule
            .workspace_id
            .as_ref()
            .and_then(|id| root.snapshot.workspaces.iter().find(|w| &w.id == id))
            .map(|w| w.name.as_str())
            .unwrap_or("Choose a workspace");
        body = body.child(
            div()
                .flex()
                .flex_col()
                .gap(px(8.))
                .p(px(14.))
                .rounded(px(10.))
                .border_1()
                .border_color(theme::active().border_hairline)
                .child(div().text_size(px(15.)).child(format!(
                    "{} · {}",
                    schedule.name,
                    if schedule.enabled { "Active" } else { "Paused" }
                )))
                .child(note(format!(
                    "{workspace} · {} · {}",
                    if schedule.rule.is_empty() {
                        "Choose a recurrence"
                    } else {
                        &schedule.rule
                    },
                    if schedule.timezone.is_empty() {
                        "Choose a timezone"
                    } else {
                        &schedule.timezone
                    }
                )))
                .child(note(schedule.prompt.clone()))
                .child(note(schedule.last_result.clone()))
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap(px(8.))
                        .child(button(
                            format!("schedule-enable-{id}"),
                            if schedule.enabled { "Pause" } else { "Enable" },
                            !root.busy,
                            false,
                            cx,
                            {
                                let enabled = !schedule.enabled;
                                move |r, _, cx| {
                                    r.request(
                                        Command::Schedules(ScheduleCommand::SetEnabled {
                                            id: enable.clone(),
                                            enabled,
                                        }),
                                        cx,
                                    )
                                }
                            },
                        ))
                        .child(button(
                            format!("schedule-run-{id}"),
                            "Run now",
                            !root.busy,
                            false,
                            cx,
                            move |r, _, cx| {
                                r.request(
                                    Command::Schedules(ScheduleCommand::RunNow { id: run.clone() }),
                                    cx,
                                )
                            },
                        ))
                        .child(button(
                            format!("schedule-edit-{id}"),
                            "Edit",
                            !root.busy,
                            false,
                            cx,
                            move |r, _, cx| {
                                r.schedule_form.editing = Some(edit.id.clone());
                                if let Some(workspace) = &edit.workspace_id {
                                    r.selection.workspace = Some(workspace.clone());
                                }
                                for (field, value) in [
                                    (&r.schedule_form.name, &edit.name),
                                    (&r.schedule_form.prompt, &edit.prompt),
                                    (&r.schedule_form.rule, &edit.rule),
                                    (&r.schedule_form.timezone, &edit.timezone),
                                ] {
                                    field.update(cx, |f, cx| f.set_content(value, cx));
                                }
                                cx.notify();
                            },
                        ))
                        .child(button(
                            format!("schedule-remove-{id}"),
                            if root.schedule_form.removing.as_ref() == Some(&id) {
                                "Confirm removal"
                            } else {
                                "Remove"
                            },
                            !root.busy,
                            false,
                            cx,
                            move |r, _, cx| {
                                if r.schedule_form.removing.as_ref() == Some(&remove) {
                                    r.schedule_form.removing = None;
                                    r.request(
                                        Command::Schedules(ScheduleCommand::Remove {
                                            id: remove.clone(),
                                        }),
                                        cx,
                                    );
                                } else {
                                    r.schedule_form.removing = Some(remove.clone());
                                    cx.notify();
                                }
                            },
                        )),
                ),
        );
    }
    let bound = root
        .schedule_form
        .editing
        .as_ref()
        .and_then(|id| root.snapshot.schedules.iter().find(|s| &s.id == id))
        .and_then(|s| s.workspace_id.as_ref())
        .and_then(|id| root.snapshot.workspaces.iter().find(|w| &w.id == id));
    if let Some(workspace) = bound {
        body = body.child(note(format!(
            "Editing schedule for {}. Changing the sidebar selection does not move this schedule.",
            workspace.name
        )));
    }
    body.child(heading(if root.schedule_form.editing.is_some(){"Edit schedule"}else{"New schedule"},"Save a paused draft first. Select a workspace in the sidebar for a new draft; enabling validates its rule and timezone. Existing task results are retained when a schedule is removed."))
        .child(field("Name",&root.schedule_form.name)).child(field("Instructions",&root.schedule_form.prompt))
        .child(field("Recurrence rule (hourly or slower)",&root.schedule_form.rule)).child(field("Timezone",&root.schedule_form.timezone))
        .child(button("schedule-save","Save paused draft",!root.busy,false,cx,|r,_,cx|{
            let existing=r.schedule_form.editing.as_ref().and_then(|id|r.snapshot.schedules.iter().find(|s|&s.id==id)).cloned().unwrap_or_default();
            let schedule=Schedule{name:value(&r.schedule_form.name,cx),prompt:value(&r.schedule_form.prompt,cx),rule:value(&r.schedule_form.rule,cx),timezone:value(&r.schedule_form.timezone,cx),workspace_id:workspace_for_save(&existing,r.selection.workspace.as_deref()),anchor_ms:if existing.anchor_ms==0{SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis().min(i64::MAX as u128) as i64}else{existing.anchor_ms},..existing};
            r.request(Command::Schedules(ScheduleCommand::Save{schedule}),cx);
        }))
        .child(button("schedule-new","Clear form for a new schedule",!root.busy,false,cx,|r,_,cx|{r.schedule_form=Form::new(cx);cx.notify();}))
}
