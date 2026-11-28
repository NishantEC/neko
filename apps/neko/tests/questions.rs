//! Questions go up (docs/knowledge/subagents.md, "Questions go up"; app-shell.md, "Kittens in the app"), on scenes: a kitten's card while
//! it asks the agent (quiet, never amber), the wake note and the note that the agent answered, the question a thread waits on with the
//! person, Needs you led by "Kitten 1 asks (via the agent)", and a kitten's tab with its question and the agent's answer.

mod support;
use rune::Runtime;
use support::*;

use neko_app::state::App;

fn scene(name: &str) -> fn(&mut App) {
    scenes().iter().find(|s| s.0 == name).unwrap_or_else(|| panic!("no scene {name}")).1
}

fn has_key(rt: &Runtime<App>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key))
}

fn texts(rt: &Runtime<App>) -> Vec<String> {
    rt.elements().into_iter().filter(|e| e.opacity > 0.0).filter_map(|e| e.text).collect()
}

fn node(rt: &Runtime<App>, key: &str) -> serde_json::Value {
    nodes(&rt.accessibility_json()).into_iter().map(|(_, n)| n).find(|n| str_of(n, "key") == key).unwrap_or_else(|| panic!("{key} is not in the tree"))
}

const Q: &str = "Which database should the migration target: SQLite or Postgres? The project has drivers for both.";

#[test]
fn a_kittens_card_says_it_asks_the_agent() {
    let rt = runtime((1200.0, 800.0), false, scene("question-card"));
    let t = texts(&rt);
    assert!(t.iter().any(|x| x == &format!("Asks the agent: {Q}")), "{t:?}");
    assert!(str_of(&node(&rt, "subagent-card-1"), "name").contains(&format!("Has a question. Asks the agent: {Q}")), "{}", str_of(&node(&rt, "subagent-card-1"), "name"));
    // Nothing waits for the person: the thread's row is not amber for it.
    assert!(!t.iter().any(|x| x.starts_with("Needs you: a question")), "{t:?}");
}

#[test]
fn the_conversation_says_the_kitten_asked_and_the_agent_answered() {
    let rt = runtime((1200.0, 800.0), false, scene("question-answered-open"));
    let t = texts(&rt);
    for want in ["Kitten 1 has a question", "The agent answered Kitten 1", "The question", "The answer", Q] {
        assert!(t.iter().any(|x| x == want), "{want} in {t:?}");
    }
    // The step reads as a person would say it, never as a tool name.
    assert!(!t.iter().any(|x| x.contains("message_subagent")), "{t:?}");
}

#[test]
fn a_thread_that_asked_the_person_ends_on_the_question_and_says_whose_it_is() {
    let rt = runtime((1200.0, 800.0), false, scene("question-to-person"));
    let t = texts(&rt);
    assert!(t.iter().any(|x| x == "Kitten 1 asks (via the agent)"), "{t:?}");
    assert!(has_key(&rt, "question-block"));
    assert!(t.iter().any(|x| x.starts_with("Asks you (via the agent): ")), "the card says the person has it: {t:?}");
    assert!(t.iter().any(|x| x == "Needs you: a question from Kitten 1"), "the row says whose: {t:?}");
}

#[test]
fn needs_you_leads_a_passed_up_question_with_the_kitten_and_whom_it_came_through() {
    let rt = runtime((1200.0, 800.0), false, scene("question-needs"));
    let t = texts(&rt);
    for want in ["Kitten 1 asks (via the agent)", "Kitten 1.2 asks (via Kitten 1 and the agent)", "The agent did not answer it, so Neko passed it to you.", "Kitten 2 asks"] {
        assert!(t.iter().any(|x| x == want), "{want} in {t:?}");
    }
}

#[test]
fn a_kittens_tab_shows_its_question_and_then_the_agents_answer_as_the_agents() {
    let rt = runtime((1200.0, 800.0), false, scene("question-kitten-tab"));
    let t = texts(&rt);
    assert!(t.iter().any(|x| x == "Kitten 1 asks the agent") && t.iter().any(|x| x == "The agent passed it to you in Needs you."), "{t:?}");
    assert!(!has_key(&rt, "pane-kitten-report"), "a kitten waiting on its answer has no report yet");
    let rt = runtime((1200.0, 800.0), false, scene("question-kitten-answered"));
    let t = texts(&rt);
    assert!(t.iter().any(|x| x == "From the agent"), "the answer is the agent's, not the person's: {t:?}");
}
