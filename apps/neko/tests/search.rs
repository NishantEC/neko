//! Search (⌘K) in the window (docs/knowledge/search.md): the palette opens and closes with its keys, its sections come in order, a
//! message hit shows its words, Return, ⌘Return and Tab do what the footer says, and the daemon's answers are used only for the text
//! now in the field. The window here has no daemon: the request and the answer are in the daemon's tests and the QA scenarios.

mod support;
use neko_app::state::App;
use neko_protocol::{Found, FoundBy, Reply, TaskId};
use rune::{Key, Mods, Runtime};
use support::*;

fn scene(name: &str) -> fn(&mut App) {
    scenes().iter().find(|s| s.0 == name).unwrap().1
}

fn cmd() -> Mods {
    Mods { cmd: true, ..Mods::default() }
}

fn headings(rt: &Runtime<App>) -> Vec<String> {
    nodes(&rt.accessibility_json()).into_iter().filter(|(_, n)| str_of(n, "role") == "heading" && ["Recent", "Threads", "Kittens", "Cattains", "Projects", "Actions"].contains(&str_of(n, "name"))).map(|(_, n)| str_of(&n, "name").to_owned()).collect()
}

fn options(rt: &Runtime<App>) -> Vec<String> {
    nodes(&rt.accessibility_json()).into_iter().filter(|(_, n)| str_of(n, "role") == "list-box-option").map(|(_, n)| str_of(&n, "name").to_owned()).collect()
}

fn id_of(rt: &Runtime<App>, starts: &str) -> TaskId {
    rt.state.tasks.iter().find(|t| t.title.starts_with(starts)).map(|t| t.id.clone()).unwrap_or_else(|| panic!("no thread {starts}"))
}

fn has_key(rt: &Runtime<App>, key: &str) -> bool {
    rt.elements().into_iter().any(|e| e.key.as_deref() == Some(key))
}

#[test]
fn command_k_opens_the_palette_with_the_keyboard_in_its_field_and_closes_it_again() {
    for size in SIZES {
        let mut rt = runtime(size, false, scene("new-thread"));
        assert!(!has_key(&rt, "palette"));
        press(&mut rt, Key::Command('k'), cmd());
        assert!(rt.state.palette.open && has_key(&rt, "palette") && has_key(&rt, "palette-input"), "⌘K opens it at {size:?}");
        assert!(rt.state.palette.query.focused, "the keyboard is in its field");
        press(&mut rt, Key::Command('k'), cmd());
        assert!(!rt.state.palette.open && !has_key(&rt, "palette"), "⌘K again closes it");
        press(&mut rt, Key::Command('k'), cmd());
        press(&mut rt, Key::Escape, Mods::default());
        assert!(!rt.state.palette.open, "Escape closes it");
        // The sidebar's Search button, when the sidebar shows, is the same thing.
        if has_key(&rt, "search-open") {
            click(&mut rt, "search-open");
            assert!(rt.state.palette.open);
        }
    }
}

#[test]
fn with_nothing_typed_it_lists_the_recent_threads_then_the_actions() {
    for dark in [false, true] {
        let rt = runtime((1200.0, 800.0), dark, scene("search-empty"));
        assert_eq!(headings(&rt), ["Recent", "Actions"]);
        let o = options(&rt);
        assert!(o[0].starts_with("Answer the question about the retry limit"), "the newest thread first: {o:?}");
        assert!(o.iter().any(|n| n == "New thread") && o.iter().any(|n| n == "Needs you"), "{o:?}");
    }
}

#[test]
fn typed_text_finds_threads_kittens_cattains_projects_and_actions_in_that_order() {
    for dark in [false, true] {
        let rt = runtime((1200.0, 800.0), dark, scene("search-typing"));
        assert_eq!(headings(&rt), ["Threads", "Kittens", "Cattains", "Projects", "Actions"]);
        let o = options(&rt);
        assert_eq!(o[..2], ["Move the blog to the new layout", "Fix the contact form on Safari"]);
        assert!(o.contains(&"mochi".to_owned()) && o.contains(&"mochi-notes".to_owned()));
        let tree = nodes(&rt.accessibility_json());
        let said = tree.iter().find(|(_, n)| str_of(n, "name") == "Fix the contact form on Safari").map(|(_, n)| str_of(n, "description").to_owned()).unwrap();
        assert!(said.contains("moves focus"), "a message hit says the words it was found by: {said}");
    }
}

#[test]
fn an_action_word_finds_actions_and_no_threads_and_a_wrong_filter_or_no_match_says_so() {
    let rt = runtime((1200.0, 800.0), false, scene("search-actions"));
    // Command's fuzzy matching is loose (the letters in order, anywhere in a name or its hidden words), so a Cattain or a project may
    // come along; no thread does, since the daemon found none, and the actions are best matches first.
    let h = headings(&rt);
    assert!(h.contains(&"Actions".to_owned()) && !h.contains(&"Threads".to_owned()) && !h.contains(&"Kittens".to_owned()), "{h:?}");
    let o = options(&rt);
    let at = o.iter().position(|n| n == "Settings").expect("Settings");
    assert!(o[at..].iter().any(|n| n.starts_with("Settings: ")), "{o:?}");
    let rt = runtime((1200.0, 800.0), false, scene("search-none"));
    assert!(options(&rt).is_empty() && nodes(&rt.accessibility_json()).iter().any(|(_, n)| str_of(n, "name") == "No results" || str_of(n, "text") == "No results"));
    let rt = runtime((1200.0, 800.0), false, scene("search-problem"));
    assert!(nodes(&rt.accessibility_json()).iter().any(|(_, n)| str_of(n, "text").contains("is not a status") || str_of(n, "name").contains("is not a status")));
}

#[test]
fn put_away_threads_say_so_and_a_waiting_answer_keeps_the_rows_and_shows_the_spinner() {
    let rt = runtime((1200.0, 800.0), false, scene("search-put-away"));
    let texts: Vec<String> = rt.elements().into_iter().filter_map(|e| e.text).collect();
    assert!(texts.iter().any(|t| t.starts_with("settled · ")) && texts.iter().any(|t| t.starts_with("archived · ")), "{texts:?}");
    let rt = runtime((1200.0, 800.0), false, scene("search-loading"));
    assert!(rt.state.palette_loading());
    assert!(options(&rt).iter().any(|n| n == "Move the blog to the new layout"), "the rows stay while the answer is awaited");
    let rt = runtime((1200.0, 800.0), false, scene("search-first-moment"));
    assert!(options(&rt).is_empty() || options(&rt).iter().all(|n| !n.contains("login")));
    assert!(nodes(&rt.accessibility_json()).iter().any(|(_, n)| str_of(n, "text") == "Searching…" || str_of(n, "name") == "Searching…"));
}

#[test]
fn return_opens_the_row_and_a_found_message_is_scrolled_to_once_its_thread_is_open() {
    let mut rt = runtime((1200.0, 800.0), false, scene("search-message"));
    // Down to the second row (a message hit), then Return.
    press(&mut rt, Key::Down, Mods::default());
    press(&mut rt, Key::Enter, Mods::default());
    let id = id_of(&rt, "Find out why the nightly build");
    assert_eq!(rt.state.open, Some(id.clone()), "Return opened the thread");
    assert!(!rt.state.palette.open, "the palette closed");
    let r = rt.state.palette.reveal.clone().expect("a message to scroll to");
    assert_eq!((r.task, r.seq), (id, 3));
    // With its transcript not here yet nothing happens; once it is, the scroll is asked for.
    for _ in 0..3 {
        rt.frame(0.0);
    }
    assert!(rt.state.palette.reveal.is_some(), "waits for the transcript");
}

#[test]
fn command_return_opens_a_thread_with_its_pane_and_a_kitten_in_the_main_view() {
    let mut rt = runtime((1280.0, 800.0), false, scene("search-typing"));
    // Highlight the kitten (the third row), ⌘Return.
    press(&mut rt, Key::Down, Mods::default());
    press(&mut rt, Key::Down, Mods::default());
    press(&mut rt, Key::Enter, cmd());
    let kitten = rt.state.tasks.iter().find(|t| t.title.starts_with("Move the retry settings")).unwrap().id.clone();
    assert_eq!(rt.state.open, Some(kitten), "⌘Return opens a kitten in the main view");
    // Return alone opens it as a tab beside its thread.
    let mut rt = runtime((1280.0, 800.0), false, scene("search-typing"));
    press(&mut rt, Key::Down, Mods::default());
    press(&mut rt, Key::Down, Mods::default());
    press(&mut rt, Key::Enter, Mods::default());
    let thread = id_of(&rt, "Find out why the nightly build");
    assert_eq!(rt.state.open, Some(thread), "Return opens its thread");
    assert!(rt.state.pane_visible(), "with the pane showing the kitten's tab");
}

#[test]
fn tab_turns_the_text_into_a_new_thread_in_the_project_a_filter_names() {
    let mut rt = runtime((1200.0, 800.0), false, scene("tasks-home"));
    rt.update(|s, _| {
        s.workspaces = neko_app::scenes::sample_workspaces();
        s.open_palette();
        s.palette.query = rune::TextEdit::new("fix the login redirect project:acme status:needs");
        s.palette.query.focused = true;
    });
    settle(&mut rt);
    press(&mut rt, Key::Tab, Mods::default());
    assert!(!rt.state.palette.open && rt.state.open.is_none(), "the palette closed on New thread");
    assert_eq!(rt.state.draft.text, "fix the login redirect", "the words are in the composer, the filters are not");
    assert_eq!(rt.state.workspace, "ws_acme", "the project the filter named is chosen");
    assert!(rt.state.draft.focused);
}

#[test]
fn an_answer_is_used_only_for_the_text_it_was_asked_for() {
    let mut s = App::default();
    s.open_palette();
    s.palette.in_flight = Some(5);
    s.palette.query = rune::TextEdit::new("login");
    let found = |task: &str| Found { task: TaskId(task.into()), kitten: false, by: FoundBy::Title, snippet: String::new(), part: None, seq: 0 };
    let reply = |q: &str, t: &str| Reply::Found { query: q.into(), hits: vec![found(t)], problem: None, took_ms: 2 };
    // An older request's answer is taken as a search answer (so nothing else reads it) and dropped.
    assert!(s.take_found(4, &reply("log", "task_old")));
    assert!(s.palette.hits.is_empty() && s.palette.in_flight == Some(5));
    assert!(s.take_found(5, &reply("login", "task_new")));
    assert_eq!((s.palette.hits.len(), s.palette.answered.as_deref(), s.palette.in_flight), (1, Some("login"), None));
    assert!(!s.take_found(6, &Reply::Ok), "another reply is not a search answer");
}

#[test]
fn search_names_what_it_finds_in_the_menu_bar_and_its_key_is_free() {
    let m = neko_app::menu::model(true, false, false);
    let search = m.iter().find(|i| i.id == "neko-search").expect("a Search item");
    assert_eq!((search.title, search.key, search.menu), ("Search…", Some("cmd+k"), Some("File")));
}
