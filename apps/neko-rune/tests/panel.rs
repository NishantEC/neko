//! The context panel through Rune's headless `Runtime`: real layout and hit-testing, made-up data and
//! made-up files (`samples.rs`), real folders under the temp directory for the Files tab. Its rules
//! (when it opens, which tab, what a close does), each tab, each viewer, the expand state, what it
//! releases and what it refuses to read.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use neko_protocol::{Artifact, ArtifactVersion};
use neko_rune::app::{App, DEMO_NOW};
use neko_rune::link::{Call, LinkEvent};
use neko_rune::panel::{Content, Loaded, Opened, Shown, Tab, Target};
use neko_rune::view::view;
use neko_rune::{samples, scenes, scenes_panel};
use rune::{Env, Key, KeyEvent, Mods, Runtime};

const WINDOW: (f32, f32) = (1700.0, 900.0);
const MIN: i64 = 60_000;

fn runtime(setup: impl FnOnce(&mut App)) -> Runtime<App> {
    runtime_at(WINDOW, setup)
}

fn runtime_at(size: (f32, f32), setup: impl FnOnce(&mut App)) -> Runtime<App> {
    let mut app = App::headless();
    setup(&mut app);
    let mut rt = Runtime::new(app, view, size, Env { system_dark: true, scale: 2.0 });
    rt.frame(0.0);
    frames(&mut rt, 20);
    rt
}

/// Frames as the window runs them: the panel's tick (loads, scrolls, animations), then the frame.
fn frames(rt: &mut Runtime<App>, n: usize) {
    for i in 0..n {
        rt.update(|s, tx| {
            s.panel_tick(i as f64 / 60.0, tx);
        });
        rt.frame(1.0 / 60.0);
    }
}

fn click(rt: &mut Runtime<App>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("no element keyed {key}"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    frames(rt, 20);
}

fn texts(rt: &Runtime<App>) -> Vec<String> {
    rt.elements().into_iter().filter_map(|e| e.text).collect()
}

fn keyed(rt: &Runtime<App>, prefix: &str) -> usize {
    rt.elements().iter().filter(|e| e.key.as_deref().is_some_and(|k| k.starts_with(prefix))).count()
}

fn rect(rt: &Runtime<App>, key: &str) -> [f32; 4] {
    rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key)).unwrap_or_else(|| panic!("no element keyed {key}")).rect
}

fn segments(rt: &Runtime<App>) -> Vec<String> {
    let mut found: Vec<(String, String)> = rt.elements().into_iter().filter_map(|e| Some((e.key.clone().filter(|k| k.starts_with("segment"))?, e.label.clone().unwrap_or_default()))).collect();
    found.sort();
    found.into_iter().map(|(_, label)| label).collect()
}

fn artifact(id: &str, kind: &str, versions: u32) -> Artifact {
    Artifact {
        id: id.into(),
        title: format!("{id} title"),
        kind: kind.into(),
        workspace_id: None,
        message_id: None,
        versions: (1..=versions).map(|n| ArtifactVersion { n, file: format!("v{n}.{kind}"), at_ms: DEMO_NOW - 2 * MIN, note: String::new(), message_id: None }).collect(),
    }
}

/// A folder under the temp directory, removed when it goes out of scope.
struct Temp(PathBuf);

impl Temp {
    fn new(name: &str) -> Temp {
        let dir = std::env::temp_dir().join(format!("nrp-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Temp(dir)
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn tab_titles(app: &App) -> Vec<&'static str> {
    app.panel_tabs().into_iter().map(Tab::title).collect()
}

// ---------------------------------------------------------------- the rules

#[test]
fn the_tabs_that_have_content_show_in_order_and_a_recent_artifact_wins_the_first_one() {
    let rt = runtime(scenes_panel::auto_canvas);
    assert_eq!(tab_titles(&rt.state), ["Canvas", "Diff", "Files", "Terminal"]);
    assert_eq!(rt.state.panel_tab(), Some(Tab::Canvas), "an artifact shown two minutes ago");
    assert!(rt.state.layout.context.is_open());
    assert_eq!(segments(&rt), ["Canvas", "Diff", "Files", "Terminal"], "the control shows each, in this order");
    assert!(rt.find("context-picker").is_some() && rt.find("context-expand").is_some() && rt.find("context-close").is_some());
    assert_eq!(rect(&rt, "context-head")[3], 36.0, "the header is 36 points");
    assert_eq!(rect(&rt, "context")[2], 420.0, "the default width");
}

#[test]
fn without_a_recent_artifact_the_first_tab_is_diff_then_terminal_then_files() {
    let mut app = App::headless();
    scenes_panel::auto_canvas(&mut app);
    // The artifact was shown eleven minutes ago: the Canvas is still there, but not first.
    app.panel.sessions.get_mut("t6").unwrap().shown[0].at_ms = app.now_ms - 11 * MIN;
    app.panel.sessions.get_mut("t6").unwrap().tab = None;
    assert_eq!(app.panel_tab(), Some(Tab::Diff));
    // No run changed a file: Terminal.
    app.work.checkpoints.remove("t6");
    assert_eq!(tab_titles(&app), ["Canvas", "Files", "Terminal"]);
    assert_eq!(app.panel_tab(), Some(Tab::Terminal));
    // No commands either (the events are gone): Files.
    if let Some(d) = app.summary.as_mut().and_then(|m| m.detail.as_mut()) {
        d.task.events.clear();
    }
    assert_eq!(app.panel_tab(), Some(Tab::Files));
    // Nothing but the old artifact: it is what there is.
    app.panel.sessions.get_mut("t6").unwrap().folder = Some(None);
    assert_eq!(tab_titles(&app), ["Canvas"]);
    assert_eq!(app.panel_tab(), Some(Tab::Canvas));
}

#[test]
fn the_person_s_tab_choice_stands_while_it_has_content() {
    let mut rt = runtime(scenes_panel::auto_diff);
    assert_eq!(rt.state.panel_tab(), Some(Tab::Diff));
    click(&mut rt, "segment2");
    assert_eq!(rt.state.panel_tab(), Some(Tab::Terminal), "the third of Diff, Files, Terminal");
    assert!(texts(&rt).iter().any(|t| t.starts_with("$ cargo test")), "{:?}", texts(&rt));
}

#[test]
fn a_session_with_nothing_to_show_has_no_panel_and_one_with_something_opens_it() {
    let mut rt = runtime(|s| {
        scenes::SCENES.iter().find(|(n, _)| *n == "live").map(|(_, f)| f(s)).unwrap();
        // The scene leaves the panel as a person who had closed it: this session starts fresh.
        s.panel.sessions.get_mut("t0").unwrap().person_closed = false;
    });
    assert!(!rt.state.layout.context.is_open());
    // A task with no events and no folder has no tab: choosing it closes the panel.
    rt.update(|s, _| s.select("t10"));
    frames(&mut rt, 10);
    assert!(rt.state.panel_tabs().is_empty() && !rt.state.layout.context.is_open() && rt.find("context").is_none());
    // The one with commands opens it by itself, on Terminal.
    rt.update(|s, _| s.select("t0"));
    frames(&mut rt, 10);
    assert!(rt.state.layout.context.is_open(), "t0 has a command in its events");
    assert_eq!(rt.state.panel_tab(), Some(Tab::Terminal));
}

#[test]
fn closing_the_panel_sticks_for_that_session_until_an_artifact_is_shown() {
    let mut rt = runtime(scenes_panel::auto_diff);
    assert!(rt.state.layout.context.is_open());
    click(&mut rt, "context-close");
    assert!(!rt.state.layout.context.is_open());
    // New data, the same session again, another session and back: still closed.
    rt.update(|s, _| {
        let summary = s.summary.clone().unwrap();
        s.apply_link(LinkEvent::Live(summary));
    });
    frames(&mut rt, 5);
    assert!(!rt.state.layout.context.is_open(), "a new snapshot does not reopen it");
    rt.update(|s, _| s.select("t10"));
    rt.update(|s, _| s.select("t6"));
    frames(&mut rt, 5);
    assert!(!rt.state.layout.context.is_open(), "coming back to the session does not reopen it");
    // An artifact is shown: it opens, on the Canvas.
    rt.update(|s, _| {
        s.apply_link(LinkEvent::ShowArtifact { id: "abc".into(), version: Some(2) });
    });
    frames(&mut rt, 5);
    assert!(rt.state.layout.context.is_open());
    assert_eq!(rt.state.panel_tab(), Some(Tab::Canvas));
    // The daemon was asked about it.
    assert!(rt.state.outbox.contains(&Call::ArtifactInfo { id: "abc".into() }), "{:?}", rt.state.outbox);
}

#[test]
fn a_shown_artifact_goes_to_the_session_that_showed_it_before() {
    let mut app = App::headless();
    scenes_panel::auto_diff(&mut app);
    app.apply_link(LinkEvent::ShowArtifact { id: "abc".into(), version: Some(1) });
    assert_eq!(app.selected.as_deref(), Some("t6"));
    app.select("t10");
    assert_eq!(app.selected.as_deref(), Some("t10"));
    assert!(!app.layout.context.is_open(), "the other session has nothing");
    // The artifact has a new version: its session is brought back, not the one open now.
    app.apply_link(LinkEvent::ShowArtifact { id: "abc".into(), version: Some(2) });
    assert_eq!(app.selected.as_deref(), Some("t6"), "switched to its session");
    assert!(app.layout.context.is_open() && app.panel_tab() == Some(Tab::Canvas));
    assert_eq!(app.panel.session("t6").unwrap().shown.iter().filter(|s| s.id == "abc").count(), 1, "one row per artifact");
    // A different artifact is new: it belongs to the session open now.
    app.select("t10");
    app.apply_link(LinkEvent::ShowArtifact { id: "xyz".into(), version: None });
    assert!(app.panel.session("t10").unwrap().shown.iter().any(|s| s.id == "xyz"));
}

#[test]
fn expand_fills_the_session_and_panel_regions_and_escape_restores_it() {
    let mut rt = runtime(scenes_panel::diff);
    let before = rect(&rt, "context");
    assert!(rt.find("session-region").is_some());
    click(&mut rt, "context-expand");
    assert!(rt.state.panel.expanded);
    assert!(rt.find("session-region").is_none(), "the session region gives its room");
    let after = rect(&rt, "context");
    assert!(after[2] > before[2] + 300.0, "{before:?} {after:?}");
    assert!(rt.find("sessions").is_some() && rt.find("neko").is_some(), "the sessions and Neko columns stay");
    assert!(texts(&rt).iter().any(|t| t == "Whole task"), "and the panel still works");
    // Esc restores it.
    let esc = KeyEvent { key: Key::Escape, mods: Mods::default() };
    rt.update(|s, _| {
        assert!(s.panel_on_key(&esc));
    });
    frames(&mut rt, 5);
    assert!(!rt.state.panel.expanded && rt.find("session-region").is_some());
    frames(&mut rt, 60);
    assert_eq!(rect(&rt, "context")[2], before[2]);
    // The button restores it too, and closing the panel ends it.
    click(&mut rt, "context-expand");
    click(&mut rt, "context-expand");
    assert!(!rt.state.panel.expanded);
    click(&mut rt, "context-expand");
    click(&mut rt, "context-close");
    assert!(!rt.state.panel.expanded && !rt.state.layout.context.is_open());
}

#[test]
fn layout_pressure_collapses_neko_and_icon_tabs_never_truncate() {
    let mut rt = runtime_at((1360.0, 840.0), scenes_panel::auto_canvas);
    // The panel is open at its 420 default: the session would have 279, so Neko is its strip.
    assert!(rt.state.layout.context.is_open() && rt.state.layout.neko.is_open(), "the state is untouched");
    assert!((rect(&rt, "neko")[2] - 44.0).abs() < 1.0, "Neko collapsed itself: {:?}", rect(&rt, "neko"));
    assert!(rect(&rt, "session-region")[2] >= 480.0, "the session keeps 480: {:?}", rect(&rt, "session-region"));
    // 420 is under 440: icons only, each named (tooltip and accessible name), never a cut label.
    assert!(rt.find("context-icon-tabs").is_some());
    assert_eq!(segments(&rt), ["Canvas", "Diff", "Files", "Terminal"]);
    assert!(texts(&rt).iter().all(|t| !t.ends_with('…') || !["Canvas", "Diff", "Files", "Termi"].iter().any(|w| t.starts_with(w))), "{:?}", texts(&rt));
    // Closing the panel ends the pressure: Neko is back at its width.
    click(&mut rt, "context-close");
    frames(&mut rt, 60);
    assert!((rect(&rt, "neko")[2] - 340.0).abs() < 1.0, "{:?}", rect(&rt, "neko"));
    // The person collapses it by hand, reopens the panel: it stays collapsed; expands it: it stays open.
    click(&mut rt, "neko-collapse");
    frames(&mut rt, 60);
    rt.update(|s, _| s.panel_set_open(true));
    frames(&mut rt, 60);
    assert!((rect(&rt, "neko")[2] - 44.0).abs() < 1.0);
    click(&mut rt, "neko-expand");
    frames(&mut rt, 60);
    assert!((rect(&rt, "neko")[2] - 340.0).abs() < 1.0, "the person's expand wins over the pressure: {:?}", rect(&rt, "neko"));
    let ctx = rect(&rt, "context")[2];
    assert!(ctx < 420.0 && ctx >= 320.0, "the panel narrowed to leave the session its minimum: {ctx}");
}

#[test]
fn a_wide_panel_shows_the_words() {
    let mut rt = runtime_at((1700.0, 900.0), scenes_panel::auto_canvas);
    rt.update(|s, _| s.layout.drag(neko_rune::layout::Region::Context, 420.0, -100.0));
    frames(&mut rt, 5);
    assert!(rect(&rt, "context")[2] >= 440.0);
    assert!(rt.find("context-icon-tabs").is_none() && rt.find("context-picker").is_some());
    let mut names = segments(&rt);
    names.dedup();
    assert_eq!(names, ["Canvas", "Diff", "Files", "Terminal"]);
}

// ---------------------------------------------------------------- the tabs

#[test]
fn the_diff_tab_lists_files_with_their_counts_and_a_click_scrolls_to_one() {
    let mut rt = runtime(scenes_panel::diff);
    assert_eq!(rt.state.panel_tab(), Some(Tab::Diff));
    assert_eq!(keyed(&rt, "diff-file-"), 3, "one row per file");
    let words = texts(&rt);
    for expect in ["web/src/board.tsx", "web/src/board.test.tsx", "web/src/empty.css", "+3", "+5", "Whole task"] {
        assert!(words.iter().any(|w| w == expect), "{expect} in {words:?}");
    }
    assert!(rt.find("panel-diff-t6|whole").is_some(), "the kit's diff view");
    click(&mut rt, "diff-file-2");
    assert_eq!(rt.state.work.diffs.get("t6|whole").unwrap().visible_file(), 2, "the diff scrolled to that file");
}

#[test]
fn the_run_select_asks_for_that_runs_diff() {
    let mut rt = runtime(scenes_panel::diff);
    rt.state.outbox.clear();
    click(&mut rt, "pick-context-run");
    assert_eq!(rt.state.pages.open_select.as_deref(), Some("context-run"));
    rt.update(|s, _| s.panel_pick_run(Some(2)));
    frames(&mut rt, 20);
    assert_eq!(rt.state.panel.session("t6").unwrap().run, Some(2));
    assert!(rt.state.outbox.contains(&Call::CheckpointDiff { task_id: "t6".into(), run: Some(2) }), "{:?}", rt.state.outbox);
}

#[test]
fn the_terminal_tab_draws_commands_and_output_through_a_terminal() {
    let rt = runtime(scenes_panel::terminal);
    let words = texts(&rt);
    assert!(words.iter().any(|w| w == "$ cargo test -p board --color always"), "{words:?}");
    assert!(words.iter().any(|w| w == "100%"), "a rewritten progress line is its last state: {words:?}");
    assert!(!words.iter().any(|w| w.contains('\u{1b}') || w.contains("10%")), "no escape codes reach the screen");
    assert!(words.iter().any(|w| w == "exit 101"));
    // Commands in order, newest at the bottom.
    let ys: Vec<f32> = ["$ rg -n EmptyBoard web/src", "$ cargo test -p board", "$ cargo test -p board --color always"].iter().map(|c| rt.elements().into_iter().find(|e| e.text.as_deref() == Some(*c)).unwrap_or_else(|| panic!("{c}")).rect[1]).collect();
    assert!(ys.windows(2).all(|w| w[0] < w[1]), "{ys:?}");
}

#[test]
fn the_files_tab_marks_changed_files_and_opens_one_in_a_viewer_with_a_back_arrow() {
    let folder = Temp::new("files");
    std::fs::create_dir_all(folder.0.join("src")).unwrap();
    std::fs::write(folder.0.join("src/lib.rs"), samples::code()).unwrap();
    std::fs::write(folder.0.join("README.md"), samples::markdown()).unwrap();
    let path = folder.0.to_string_lossy().into_owned();
    let mut rt = runtime(|s| {
        scenes_panel::diff(s);
        s.folder_arrived("t6", Some(path.clone()));
        s.panel.sessions.get_mut("t6").unwrap().tab = Some(Tab::Files);
    });
    assert_eq!(rt.state.panel_tab(), Some(Tab::Files));
    assert!(rt.find("src").is_some() || rt.find("src/lib.rs").is_some(), "the folder's tree: {:?}", texts(&rt));
    // Open the tree's folder and then its file.
    rt.update(|s, _| s.panel.files_tree.expand_all());
    frames(&mut rt, 10);
    click(&mut rt, "src/lib.rs");
    assert_eq!(rt.state.panel.session("t6").unwrap().file.as_deref(), Some("src/lib.rs"));
    assert!(rt.find("viewer-code").is_some(), "the code viewer: {:?}", texts(&rt));
    assert!(texts(&rt).iter().any(|t| t.contains("pub fn count")));
    // Back to the tree; the Markdown file opens in the Markdown viewer.
    click(&mut rt, "files-back");
    assert!(rt.state.panel.session("t6").unwrap().file.is_none() && rt.find("viewer-code").is_none());
    click(&mut rt, "README.md");
    assert!(rt.find("viewer-markdown").is_some());
    assert!(rt.find("files-back").is_some());
}

#[test]
fn coming_back_to_a_session_lists_its_folder_again_when_the_tree_was_another_sessions() {
    let folder = Temp::new("relist");
    std::fs::write(folder.0.join("a.txt"), "a").unwrap();
    let path = folder.0.to_string_lossy().into_owned();
    let mut app = App::headless();
    scenes_panel::diff(&mut app);
    app.folder_arrived("t6", Some(path));
    assert!(app.panel.files_tree.contains("a.txt"));
    // Another session's tree took the place (as choosing it would): this one is listed again on return.
    app.panel.files_meta.as_mut().unwrap().task_id = "other".into();
    app.panel.files_tree = rune_kit::FileTree::new().build(vec![("zzz.txt", false)]);
    app.panel_sync();
    assert_eq!(app.panel.files_meta.as_ref().map(|m| m.task_id.as_str()), Some("t6"));
    assert!(app.panel.files_tree.contains("a.txt") && !app.panel.files_tree.contains("zzz.txt"));
    // A folder that is not there is no Files tab, and nothing loops.
    app.folder_arrived("t6", Some("/definitely/not/here".into()));
    assert_eq!(app.panel.session("t6").unwrap().folder, Some(None));
    assert!(!app.panel_tabs().contains(&Tab::Files));
}

#[test]
fn the_files_tab_refuses_a_link_that_leaves_the_folder_and_a_file_over_five_megabytes() {
    let base = Temp::new("refuse");
    let folder = base.0.join("work");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(base.0.join("secret.txt"), "outside").unwrap();
    std::os::unix::fs::symlink(base.0.join("secret.txt"), folder.join("leak.txt")).unwrap();
    std::fs::write(folder.join("big.bin"), vec![1u8; 5 * 1024 * 1024 + 1]).unwrap();
    let path = folder.to_string_lossy().into_owned();
    thread_local! { static REVEALED: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) }; }
    let mut rt = runtime(|s| {
        scenes_panel::diff(s);
        s.revealer = |p| {
            REVEALED.with(|r| r.borrow_mut().push(p.to_owned()));
            Ok(())
        };
        s.folder_arrived("t6", Some(path.clone()));
        s.panel.sessions.get_mut("t6").unwrap().tab = Some(Tab::Files);
    });
    rt.update(|s, _| s.panel_open_file("leak.txt"));
    frames(&mut rt, 10);
    let words = texts(&rt);
    assert!(words.iter().any(|w| w == "That file is outside this folder, so it isn’t shown."), "{words:?}");
    assert!(!words.iter().any(|w| w.contains("outside") && w.len() < 10), "the secret is not read");
    rt.update(|s, _| s.panel_open_file("big.bin"));
    frames(&mut rt, 10);
    assert!(texts(&rt).iter().any(|w| w == "Too large to preview"), "{:?}", texts(&rt));
    click(&mut rt, "too-large-reveal");
    let revealed = REVEALED.with(|r| r.borrow().clone());
    assert_eq!(revealed.len(), 1);
    assert!(revealed[0].ends_with("work/big.bin"), "{revealed:?}");
}

// ---------------------------------------------------------------- artifacts and the Canvas

#[test]
fn an_artifact_is_read_only_from_under_the_artifact_store() {
    let data = Temp::new("store");
    let dir = data.0.join("artifacts/abc");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("v1.csv"), samples::csv(5)).unwrap();
    std::fs::write(data.0.join("outside.csv"), "id\n1\n").unwrap();
    let mut app = App::headless();
    scenes_panel::auto_closed(&mut app);
    app.panel.data_dir = data.0.clone();
    app.panel.artifacts.insert("abc".into(), artifact("abc", "csv", 1));
    app.panel.sessions.entry("t0".into()).or_default().shown.push(Shown { id: "abc".into(), version: None, at_ms: app.now_ms });
    let target = Target::Artifact { id: "abc".into(), version: 1 };
    let mut try_path = |path: &str| {
        app.panel.open = Some(Opened { target: target.clone(), content: Content::Loading });
        app.artifact_file("abc", Some(1), path, "csv");
        match &app.panel.open.as_ref().unwrap().content {
            Content::Ready(l) => Ok(l.size),
            Content::Failed(m) => Err(m.clone()),
            _ => Err("loading".into()),
        }
    };
    let inside = dir.join("v1.csv").to_string_lossy().into_owned();
    assert!(try_path(&inside).is_ok(), "the stored file");
    let outside = data.0.join("outside.csv").to_string_lossy().into_owned();
    assert!(try_path(&outside).unwrap_err().contains("outside this folder"), "a file in the data folder but not in the store");
    assert!(try_path("/etc/hosts").unwrap_err().contains("outside this folder"));
    assert!(try_path(&format!("{}/../outside.csv", dir.display())).unwrap_err().contains("outside this folder"), "a ..");
    std::os::unix::fs::symlink(data.0.join("outside.csv"), dir.join("v2.csv")).unwrap();
    assert!(try_path(&dir.join("v2.csv").to_string_lossy()).unwrap_err().contains("outside this folder"), "a link out of the store");
}

#[test]
fn the_canvas_lists_artifacts_with_a_kind_icon_title_version_and_time_and_picks_a_version() {
    let mut rt = runtime(scenes_panel::canvas_image);
    assert_eq!(keyed(&rt, "artifact-"), 3);
    let words = texts(&rt);
    for expect in ["Board mockup", "Release notes", "Budget", "v3", "v2", "v1", "2m", "40m", "1h"] {
        assert!(words.iter().any(|w| w == expect), "{expect} in {words:?}");
    }
    assert!(rt.find("viewer-image").is_some());
    // The version select lists newest first; choosing one asks for that version's file.
    rt.state.outbox.clear();
    click(&mut rt, "pick-context-version");
    assert_eq!(rt.state.pages.open_select.as_deref(), Some("context-version"), "pressing the select opens its list");
    rt.update(|s, _| s.panel_pick_version("a-board", 1));
    frames(&mut rt, 20);
    assert_eq!(rt.state.panel.session("t6").unwrap().versions.get("a-board"), Some(&1));
    assert!(rt.state.outbox.contains(&Call::ArtifactFile { id: "a-board".into(), version: Some(1) }), "{:?}", rt.state.outbox);
    // Another artifact in the list is chosen by pressing its row.
    rt.update(|s, _| s.pages.open_select = None);
    frames(&mut rt, 5);
    click(&mut rt, "artifact-a-budget");
    assert_eq!(rt.state.panel.session("t6").unwrap().canvas.as_deref(), Some("a-budget"));
}

// ---------------------------------------------------------------- the viewers

#[test]
fn every_built_in_viewer_draws_its_kind() {
    type Case = (&'static str, fn(&mut App), &'static str);
    let cases: [Case; 11] = [
        ("png", scenes_panel::canvas_image, "viewer-image"),
        ("gif", scenes_panel::image_gif, "viewer-image"),
        ("svg", scenes_panel::image_svg, "viewer-svg"),
        ("md", scenes_panel::markdown, "viewer-markdown"),
        ("rs", scenes_panel::code, "viewer-code"),
        ("csv", scenes_panel::table, "viewer-table"),
        ("pdf", scenes_panel::pdf, "viewer-pdf"),
        ("glb", scenes_panel::model, "viewer-model3d"),
        ("html", scenes_panel::web, "viewer-web-pending"),
        ("bin", scenes_panel::fallback, "viewer-fallback"),
        ("big", scenes_panel::too_large, "viewer-too-large"),
    ];
    for (kind, scene, key) in cases {
        let rt = runtime(scene);
        assert!(rt.find(key).is_some(), "{kind}: no {key} in {:?}", rt.elements().into_iter().filter_map(|e| e.key).filter(|k| k.starts_with("viewer")).collect::<Vec<_>>());
        let inside = rect(&rt, "context");
        let viewer = rect(&rt, key);
        assert!(viewer[0] >= inside[0] - 0.5 && viewer[0] + viewer[2] <= inside[0] + inside[2] + 0.5, "{kind} stays inside the panel: {viewer:?} in {inside:?}");
    }
}

#[test]
fn an_unknown_kind_shows_its_name_size_and_two_ways_out_and_a_page_says_it_is_coming() {
    let rt = runtime(scenes_panel::fallback);
    let words = texts(&rt);
    for expect in ["dump.bin", "10 bytes", "Reveal in Finder", "Open in web UI"] {
        assert!(words.iter().any(|w| w == expect), "{expect} in {words:?}");
    }
    let rt = runtime(scenes_panel::web);
    let words = texts(&rt);
    assert!(words.iter().any(|w| w == "Opens here in the next update") && words.iter().any(|w| w == "Open in web UI"), "{words:?}");
}

#[test]
fn a_hundred_thousand_row_csv_builds_only_the_rows_on_screen_and_keeps_its_header_and_footer() {
    let rt = runtime(scenes_panel::table);
    let rows = keyed(&rt, "r");
    assert!(rows > 5 && rows < 200, "{rows} rows are built, not 100,000");
    assert!(rt.find("table-header").is_some());
    assert!(texts(&rt).iter().any(|t| t == "100,000 rows · 4 columns"));
    assert!(rt.find("r50000").is_some(), "scrolled to the middle");
    assert!(rt.find("r0").is_none(), "the first rows are far above");
}

#[test]
fn a_pdf_draws_the_pages_on_screen_counts_them_and_keeps_at_most_six() {
    let mut rt = runtime(scenes_panel::pdf);
    let words = texts(&rt);
    assert!(words.iter().any(|w| w == "9 / 16 · 100%"), "page 9 of 16: {words:?}");
    let key = ("a-report".to_owned(), 1);
    let drawn = rt.state.panel.states.with(&key, |st| st.peek::<neko_rune::viewers::pdf::SharedDoc>().map(|d| d.borrow().held()));
    assert!(drawn.is_some_and(|n| (1..=6).contains(&n)), "{drawn:?}");
    // ⌘+ zooms, ⌘0 is back to 100%.
    let command = |c: char| KeyEvent { key: Key::Command(c), mods: Mods { cmd: true, ..Mods::default() } };
    rt.update(|s, _| assert!(s.panel_on_key(&command('='))));
    frames(&mut rt, 20);
    assert!(texts(&rt).iter().any(|w| w.ends_with("· 125%")), "{:?}", texts(&rt));
    rt.update(|s, _| assert!(s.panel_on_key(&command('-'))));
    rt.update(|s, _| assert!(s.panel_on_key(&command('-'))));
    frames(&mut rt, 20);
    assert!(texts(&rt).iter().any(|w| w.ends_with("· 80%")), "{:?}", texts(&rt));
    rt.update(|s, _| assert!(s.panel_on_key(&command('0'))));
    frames(&mut rt, 20);
    assert!(texts(&rt).iter().any(|w| w.ends_with("· 100%")));
}

#[test]
fn the_model_viewer_turns_zooms_fits_and_toggles_the_wireframe_with_w() {
    let mut rt = runtime(scenes_panel::model_solid);
    assert!(rt.find("model3d-view").is_some());
    let key = ("a-knot".to_owned(), 1);
    let cam = |rt: &Runtime<App>| rt.state.panel.states.with(&key, |st| st.camera);
    let start = cam(&rt);
    let (x, y) = rt.find("model3d-view").unwrap();
    rt.pointer_down(x, y);
    rt.pointer_move(x + 60.0, y + 20.0);
    rt.pointer_up(x + 60.0, y + 20.0);
    frames(&mut rt, 5);
    let turned = cam(&rt);
    assert!(turned.yaw != start.yaw && turned.pitch != start.pitch, "dragging orbits: {start:?} {turned:?}");
    let w = KeyEvent { key: Key::Text("w".into()), mods: Mods::default() };
    rt.update(|s, _| assert!(s.panel_on_key(&w)));
    assert!(cam(&rt).wire, "W turns the wireframe on");
    rt.update(|s, _| assert!(s.panel_on_key(&w)));
    assert!(!cam(&rt).wire, "and off");
    // A double press fits the model again.
    rt.update(|s, _| s.panel.states.with(&key, |st| st.camera.zoom = 4.0));
    let (x, y) = rt.find("model3d-view").unwrap();
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    frames(&mut rt, 5);
    assert_eq!(cam(&rt).zoom, 1.0, "double press fits");
}

#[test]
fn a_viewer_holds_nothing_once_it_is_off_screen() {
    let mut rt = runtime(scenes_panel::pdf);
    let states = |rt: &Runtime<App>| rt.state.panel.states.resident();
    assert_eq!(states(&rt), 1, "the PDF's pages are held while it shows");
    assert!(rt.state.panel.open.is_some());
    // Another tab: nothing is on screen, so nothing is held, but the page and zoom stay.
    rt.update(|s, _| s.panel_choose_tab(Tab::Diff));
    frames(&mut rt, 5);
    assert_eq!(states(&rt), 0);
    assert!(rt.state.panel.open.is_none(), "the loaded bytes went too");
    assert_eq!(rt.state.panel.states.with(&("a-report".to_owned(), 1), |st| st.page), 8, "where the reader was is remembered");
    // Back: it is read and drawn again, on the same page.
    rt.update(|s, _| s.panel_choose_tab(Tab::Canvas));
    frames(&mut rt, 5);
    assert!(rt.state.outbox.iter().any(|c| matches!(c, Call::ArtifactFile { id, .. } if id == "a-report")), "the file is asked for again");
    // Closing the panel releases it too.
    rt.update(|s, _| {
        s.panel.open = Some(Opened { target: Target::Artifact { id: "a-report".into(), version: 1 }, content: Content::Ready(Rc::new(Loaded::from_bytes("/x/v1.pdf".into(), "pdf".into(), "r".into(), samples::pdf(4)))) });
    });
    frames(&mut rt, 10);
    assert_eq!(states(&rt), 1);
    click(&mut rt, "context-close");
    frames(&mut rt, 5);
    assert_eq!((states(&rt), rt.state.panel.open.is_some()), (0, false), "a closed panel holds no pictures");
}

#[test]
fn an_animated_gif_advances_on_the_frame_clock_and_stops_when_it_leaves() {
    let mut rt = runtime(scenes_panel::image_gif);
    let key = ("a-loop".to_owned(), 1);
    let shown = |rt: &Runtime<App>| rt.state.panel.states.with(&key, |st| st.peek::<neko_rune::viewers::image_view::ImageData>().and_then(|d| d.gif.as_ref().map(|g| (g.shown, g.frames.len()))));
    let (first, n) = shown(&rt).expect("an animation");
    assert_eq!(n, 8);
    // 0.35 s later the clock is on frame 3 or 4 (a tenth of a second a frame).
    // The clock started at the first frame (0.0): 0.55 s on it is frame 5 (a tenth of a second a frame).
    rt.update(|s, tx| {
        s.panel_tick(0.55, tx);
    });
    let (later, _) = shown(&rt).unwrap();
    assert_ne!(later, first, "the frame moved");
    assert_eq!(later, 5, "frame {later}");
    // Off screen, its frames are let go.
    rt.update(|s, _| s.panel_choose_tab(Tab::Diff));
    frames(&mut rt, 5);
    assert_eq!(rt.state.panel.states.resident(), 0);
}
