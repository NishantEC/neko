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

/// A list line: its marker taken off, and whether it sits at the left edge (an indented line explains the step above it).
/// A bullet is `-`, `*` or `+` and a space (so `**bold**` is not one); a number is digits and `.` or `)`.
fn list_item(line: &str) -> Option<(&str, bool, bool)> {
    let top = !line.starts_with([' ', '\t']);
    let line = line.trim_start();
    if let Some(rest) = line.strip_prefix(['-', '*', '+']).filter(|r| r.starts_with(' ')) {
        return Some((rest.trim_start(), top, false));
    }
    let digits = line.chars().take_while(char::is_ascii_digit).count();
    if digits > 0 && line[digits..].starts_with(['.', ')']) && line[digits + 1..].starts_with(' ') {
        return Some((line[digits + 1..].trim_start(), top, true));
    }
    None
}

/// A step's words without the Markdown around them (`**bold**`, `` `code` ``).
fn plain(text: &str) -> String {
    text.replace("**", "").replace('`', "").trim().to_owned()
}

/// The plan's steps, in order, with the first unticked one active while the task builds. A plan with boxes is its
/// boxes; else its numbered lines at the left edge; else its bullets at the left edge. Indented lines and prose explain
/// a step and are not one.
pub fn steps(plan: &str, status: TaskStatus) -> Vec<Step> {
    let mut boxed: Vec<(String, bool)> = Vec::new();
    let mut numbered: Vec<String> = Vec::new();
    let mut bullets: Vec<String> = Vec::new();
    for line in plan.lines() {
        let Some((item, top, is_number)) = list_item(line) else { continue };
        if let Some(rest) = item.strip_prefix("[x]").or_else(|| item.strip_prefix("[X]")) {
            boxed.push((plain(rest), true));
        } else if let Some(rest) = item.strip_prefix("[ ]") {
            boxed.push((plain(rest), false));
        } else if top && !item.is_empty() {
            if is_number { numbered.push(plain(item)) } else { bullets.push(plain(item)) }
        }
    }
    let mut out: Vec<Step> = if !boxed.is_empty() {
        boxed.into_iter().map(|(text, done)| Step { text, state: if done { StepState::Done } else { StepState::Todo } }).collect()
    } else if !numbered.is_empty() {
        numbered.into_iter().map(|text| Step { text, state: StepState::Todo }).collect()
    } else {
        bullets.into_iter().map(|text| Step { text, state: StepState::Todo }).collect()
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
    fn a_plan_without_boxes_is_its_numbered_lines_else_its_bullets_and_markdown_is_not_a_step() {
        let s = steps("Steps:\n1. Read the job\n2) Add a **retry**\n   - an explaining bullet\n- Test it\nand some prose", TaskStatus::AwaitingApproval);
        assert_eq!(s.iter().map(|s| s.text.as_str()).collect::<Vec<_>>(), ["Read the job", "Add a retry"], "numbered lines at the left edge");
        let s = steps("- Read the job\n- Add a retry\n  - detail", TaskStatus::AwaitingApproval);
        assert_eq!(s.iter().map(|s| s.text.as_str()).collect::<Vec<_>>(), ["Read the job", "Add a retry"], "else the bullets");
        assert!(steps("**Bold lead.** prose\n*emphasis* prose", TaskStatus::Building).is_empty(), "emphasis is not a bullet");
        assert!(steps("Only prose.", TaskStatus::Building).is_empty());
    }
}
