//! A task's plan as the checklist the context panel's Plan tab shows (the web's checklist view): its steps, each
//! done, active or still to do.
//!
//! The steps are the plan's `- [x]` / `- [ ]` lines when it has any. A plan without checkboxes is read by its numbered
//! and bulleted lines, all still to do. Headings and prose are not steps.

use neko_protocol::workbench::TaskStatus;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StepState {
    Done,
    /// The step being worked on: the first unticked one, while the task is building.
    Active,
    Todo,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Step {
    pub text: String,
    pub state: StepState,
}

fn strip_marker(line: &str) -> Option<&str> {
    let line = line.trim_start();
    if let Some(rest) = line.strip_prefix(['-', '*', '+']) {
        return Some(rest.trim_start());
    }
    let digits = line.chars().take_while(char::is_ascii_digit).count();
    if digits > 0 && line[digits..].starts_with(['.', ')']) {
        return Some(line[digits + 1..].trim_start());
    }
    None
}

/// The plan's steps, in order, with the first unticked one active while the task builds.
pub fn steps(plan: &str, status: TaskStatus) -> Vec<Step> {
    let mut boxed: Vec<(String, bool)> = Vec::new();
    let mut listed: Vec<String> = Vec::new();
    for line in plan.lines() {
        let Some(item) = strip_marker(line) else { continue };
        if let Some(rest) = item.strip_prefix("[x]").or_else(|| item.strip_prefix("[X]")) {
            boxed.push((rest.trim().to_owned(), true));
        } else if let Some(rest) = item.strip_prefix("[ ]") {
            boxed.push((rest.trim().to_owned(), false));
        } else if !item.is_empty() {
            listed.push(item.to_owned());
        }
    }
    let mut out: Vec<Step> = if boxed.is_empty() {
        listed.into_iter().map(|text| Step { text, state: StepState::Todo }).collect()
    } else {
        boxed.into_iter().map(|(text, done)| Step { text, state: if done { StepState::Done } else { StepState::Todo } }).collect()
    };
    if status == TaskStatus::Building
        && let Some(first) = out.iter_mut().find(|s| s.state == StepState::Todo)
    {
        first.state = StepState::Active;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ticked_lines_are_the_steps_and_the_first_open_one_is_active_while_building() {
        let plan = "# Plan\n\nWhy: the board is empty.\n\n- [x] Read the board\n- [ ] Add the empty state\n- [ ] Test it\n";
        let s = steps(plan, TaskStatus::Building);
        assert_eq!(s.iter().map(|s| s.text.as_str()).collect::<Vec<_>>(), ["Read the board", "Add the empty state", "Test it"]);
        assert_eq!(s.iter().map(|s| s.state.clone()).collect::<Vec<_>>(), [StepState::Done, StepState::Active, StepState::Todo]);
        assert!(steps(plan, TaskStatus::AwaitingApproval).iter().all(|s| s.state != StepState::Active), "nothing is active before it builds");
    }

    #[test]
    fn a_plan_without_boxes_is_its_numbered_and_bulleted_lines() {
        let s = steps("Steps:\n1. Read the job\n2) Add a retry\n- Test it\nand some prose", TaskStatus::AwaitingApproval);
        assert_eq!(s.iter().map(|s| s.text.as_str()).collect::<Vec<_>>(), ["Read the job", "Add a retry", "Test it"]);
        assert!(steps("Only prose.", TaskStatus::Building).is_empty());
    }
}
