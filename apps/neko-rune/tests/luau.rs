//! Luau plugins (`src/luau/`), driven headless through Rune's `Runtime` over made-up data: where the code comes from, what a plugin may
//! do (reads that are reads, a draft that is never sent, storage in its own namespace, no raw colours), how it is held (the watchdog
//! and the heap), what the person sees when one stops, hot reload with state kept, and the two panels the example pack ships.
//! Nothing here talks to a daemon: a fake wire answers (and counts) every request.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use neko_protocol::packs::{PackListing, PackStatus, PackUiFile};
use neko_protocol::{Request, Response};
use neko_rune::app::App;
use neko_rune::link::http::Http;
use neko_rune::link::luau::{PackSync, Wire, sync};
use neko_rune::luau::{FileStatus, install_for_tests};
use neko_rune::scenes::SCENES;
use neko_rune::scenes_luau::{BROKEN, NEEDS_YOU, ROUTINES, demo_wire, static_card};
use neko_rune::view::view;
use rune::{Env, Runtime};

const WINDOW: (f32, f32) = (1360.0, 840.0);
const TOKEN: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

fn settle(rt: &mut Runtime<App>) {
    for _ in 0..20 {
        rt.frame(1.0 / 60.0);
    }
}

fn runtime(app: App) -> Runtime<App> {
    let mut rt = Runtime::new(app, view, WINDOW, Env { system_dark: true, scale: 1.0 });
    rt.on_key = Some(neko_rune::on_key);
    rt.frame(0.0);
    settle(&mut rt);
    rt
}

fn scene_rt(name: &str, dark: bool) -> Runtime<App> {
    let mut app = App::headless_neko();
    let (_, f) = SCENES.iter().find(|(n, _)| *n == name).unwrap_or_else(|| panic!("no scene {name}"));
    f(&mut app);
    app.outbox.clear();
    let mut rt = Runtime::new(app, view, WINDOW, Env { system_dark: dark, scale: 1.0 });
    rt.frame(0.0);
    settle(&mut rt);
    rt
}

/// Lets the scripts run what is due: finished reads come back on other threads, so this waits a little in real time.
fn pump(rt: &mut Runtime<App>, ms: u64) {
    // Carry on from where the plugins' clock got to: a `task.wait` begun earlier is still waiting.
    let mut now = rt.state.luau.clock.max(1.0);
    for _ in 0..(ms / 5).max(1) {
        std::thread::sleep(std::time::Duration::from_millis(5));
        now += 0.005;
        rt.update(move |s, tx| {
            s.luau_tick(now, tx);
        });
        rt.frame(0.005);
    }
    settle(rt);
}

fn texts(rt: &Runtime<App>) -> Vec<String> {
    rt.elements().into_iter().filter_map(|e| e.text).collect()
}

fn has_text(rt: &Runtime<App>, needle: &str) -> bool {
    texts(rt).iter().any(|t| t.contains(needle))
}

fn click(rt: &mut Runtime<App>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("no element keyed {key}: {:?}", rt.elements().into_iter().filter_map(|e| e.key).filter(|k| k.contains("luau") || k.contains("card")).collect::<Vec<_>>()));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    settle(rt);
}

fn right_click(rt: &mut Runtime<App>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("no element keyed {key}"));
    rt.secondary_down(x, y);
    rt.secondary_up(x, y);
    settle(rt);
}

fn rect(rt: &Runtime<App>, key: &str) -> [f32; 4] {
    rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key)).unwrap_or_else(|| panic!("no element keyed {key}")).rect
}

/// A wire that counts every request it is asked and answers like the demo daemon.
fn counting() -> (Wire, Arc<AtomicUsize>, Arc<Mutex<Vec<String>>>) {
    let (count, seen) = (Arc::new(AtomicUsize::new(0)), Arc::new(Mutex::new(Vec::new())));
    let inner = demo_wire();
    let (c, s) = (count.clone(), seen.clone());
    let wire = Wire::fake(
        move |request| {
            c.fetch_add(1, Ordering::SeqCst);
            s.lock().unwrap().push(format!("{request:?}").chars().take(60).collect());
            inner.ask(request)
        },
        None,
    )
    .at(neko_rune::app::DEMO_NOW);
    (wire, count, seen)
}

fn card(id: &str, title: &str, init: &str, render: &str) -> String {
    format!("local ui = require(\"@rune/ui\")\nreturn {{ slot = \"card\", id = \"{id}\", title = \"{title}\", init = function(host)\n{init}\nend, render = function(state, host)\n{render}\nend }}")
}

fn app_with(files: &[(&str, &str)], wire: Wire) -> App {
    let mut app = App::headless_neko();
    install_for_tests(&mut app, "demo", files, wire);
    app
}

fn status_of(app: &App, path: &str) -> FileStatus {
    app.luau.rows().into_iter().find(|r| r.path == path).unwrap_or_else(|| panic!("no row for {path}")).status
}

// ---- where Luau comes from ----------------------------------------------------------------------------------------------

fn listing_with(statuses: &[(&str, PackStatus, bool)]) -> PackListing {
    let mut listing = neko_rune::scenes_setup::demo_packs();
    let template = listing.installed[0].clone();
    listing.installed = statuses
        .iter()
        .map(|(name, status, luau)| {
            let mut p = template.clone();
            p.info.name = (*name).to_owned();
            p.info.title = (*name).to_owned();
            p.status = *status;
            p.info.contents.luau = if *luau { vec!["ui/a.luau".to_owned()] } else { Vec::new() };
            p
        })
        .collect();
    listing
}

#[test]
fn only_healthy_packs_with_luau_files_are_asked_for_their_panels_and_a_refusal_is_shown() {
    let asked = Arc::new(Mutex::new(Vec::<String>::new()));
    let log = asked.clone();
    let listing = listing_with(&[("good", PackStatus::Healthy, true), ("changed", PackStatus::Changed, true), ("failing", PackStatus::Failing, true), ("plain", PackStatus::Healthy, false), ("late", PackStatus::Healthy, true)]);
    let wire = Wire::fake(
        move |request| match request {
            Request::Packs(_) => Ok(Response::Packs(listing.clone())),
            Request::PackUi { name } => {
                log.lock().unwrap().push(name.clone());
                if name == "late" {
                    // The daemon refuses a pack whose files changed after the list was made.
                    Ok(Response::Error { message: "The installed files changed on disk since you approved them. Nothing runs until you review them.".into() })
                } else {
                    Ok(Response::PackUi(vec![PackUiFile { path: "ui/a.luau".into(), source: "return {}".into() }]))
                }
            }
            other => Err(format!("unexpected {other:?}")),
        },
        None,
    );
    let packs = sync(&wire).unwrap();
    assert_eq!(*asked.lock().unwrap(), ["good", "late"], "changed, failing and plain packs are never asked for files");
    let by = |n: &str| packs.iter().find(|p| p.name == n).unwrap();
    assert!(by("good").outcome.is_ok());
    assert!(by("changed").outcome.as_ref().unwrap_err().contains("changed on disk"));
    assert!(by("failing").outcome.as_ref().unwrap_err().contains("health check failed"));
    assert!(by("late").outcome.as_ref().unwrap_err().contains("changed on disk"));
    assert!(packs.iter().all(|p| p.name != "plain"));
}

#[test]
fn a_pack_that_stops_being_healthy_or_unchanged_loses_its_panels_at_once_and_says_why() {
    let (wire, _, _) = counting();
    let mut app = app_with(&[("ui/a.luau", &static_card("a", "Alpha", "clock", "one"))], wire);
    assert_eq!(app.luau.cards().len(), 1);
    assert_eq!(status_of(&app, "ui/a.luau"), FileStatus::Loaded);
    app.luau_synced(Ok(vec![PackSync { name: "demo".into(), title: "Demo".into(), declared: vec!["ui/a.luau".into()], outcome: Err("The installed files changed on disk since you approved them.".into()) }]), None);
    assert!(app.luau.plugins.is_empty() && app.luau.cards().is_empty(), "nothing of an unapproved pack runs");
    match status_of(&app, "ui/a.luau") {
        FileStatus::Stopped(why) => assert!(why.contains("changed on disk"), "{why}"),
        other => panic!("{other:?}"),
    }
    // Gone from the list altogether (removed): its rows go too, and a daemon that cannot be reached leaves what runs alone.
    let (wire, _, _) = counting();
    let mut app = app_with(&[("ui/a.luau", &static_card("a", "Alpha", "clock", "one"))], wire);
    app.luau_synced(Err("Neko isn't running.".into()), None);
    assert_eq!(app.luau.cards().len(), 1, "a failed sync changes nothing");
    app.luau_synced(Ok(Vec::new()), None);
    assert!(app.luau.plugins.is_empty() && app.luau.packs.is_empty());
}

#[test]
fn rune_never_reads_a_pack_folder_itself() {
    // The sources come from `PackUi` only: no file reading in the plugin code, and nothing that names the packs folder.
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    for file in ["luau/mod.rs", "luau/host.rs", "luau/nodes.rs", "view/luau.rs"] {
        let text = std::fs::read_to_string(root.join(file)).unwrap();
        let code = text.split("#[cfg(test)]").next().unwrap();
        for forbidden in ["std::fs", "File::open", "read_to_string", "read_dir", "\"packs\"", "/packs/", "from_dir", "Command::new"] {
            assert!(!code.contains(forbidden), "{file} uses `{forbidden}`: Luau sources come from the daemon's PackUi, never from a folder");
        }
    }
    // The one file read the nodes do is an artifact picture, confined to the artifact store.
    let nodes = std::fs::read_to_string(root.join("luau/nodes.rs")).unwrap();
    assert!(nodes.contains("Confined::new") && nodes.contains("\"artifacts\""));
}

// ---- what a plugin may do -------------------------------------------------------------------------------------------------

#[test]
fn host_read_refuses_every_name_that_is_not_a_listed_read_and_sends_nothing() {
    let (wire, count, _) = counting();
    let init = r#"
    local state = {}
    task.spawn(function()
        local out = {}
        for _, name in { "DeleteTask", "SendMessage", "CancelTask", "ApproveTask", "Packs", "McpBridge", "SaveRoutine", "" } do
            local value, err = host.read(name, { id = "t1", text = "hi" })
            table.insert(out, tostring(value) .. ":" .. tostring(err))
        end
        state.out = out
    end)
    return state"#;
    let render = r#"return ui.text(state.out and table.concat(state.out, "|") or "waiting")"#;
    let app = app_with(&[("ui/p.luau", &card("probe", "Probe", init, render))], wire);
    let mut rt = runtime(app);
    pump(&mut rt, 100);
    let said = texts(&rt).into_iter().find(|t| t.contains("is not a read")).unwrap_or_else(|| panic!("{:?}", texts(&rt)));
    assert_eq!(said.matches("nil:").count(), 8, "every one is nil plus a reason: {said}");
    assert!(said.contains("DeleteTask is not a read Neko offers plugins"), "{said}");
    assert_eq!(count.load(Ordering::SeqCst), 0, "not one request left the window");
}

#[test]
fn host_read_makes_the_listed_reads_four_a_second_and_the_fifth_is_refused() {
    let (wire, count, seen) = counting();
    let init = r#"
    local state = { ok = 0, limited = 0 }
    task.spawn(function()
        for i = 1, 6 do
            local value, err = host.read("Workbench")
            if value then state.ok += 1 elseif string.find(err, "rate limited") then state.limited += 1 end
        end
    end)
    return state"#;
    let app = app_with(&[("ui/p.luau", &card("probe", "Probe", init, r#"return ui.text(`ok={state.ok} limited={state.limited}`)"#))], wire);
    let mut rt = runtime(app);
    pump(&mut rt, 150);
    assert!(has_text(&rt, "ok=4 limited=2"), "{:?}", texts(&rt));
    assert_eq!(count.load(Ordering::SeqCst), 4, "{:?}", seen.lock().unwrap());
}

#[test]
fn host_read_answers_a_table_of_the_tasks_and_the_counts_not_the_snapshot() {
    let (wire, _, _) = counting();
    let init = r#"
    local state = {}
    task.spawn(function()
        local wb = host.read("Workbench")
        state.line = `{#wb.tasks} tasks, {wb.needs_you} need you, first {wb.tasks[1].title} ({wb.tasks[1].glyph}, {tostring(wb.tasks[1].needs_you)})`
    end)
    return state"#;
    let app = app_with(&[("ui/p.luau", &card("probe", "Probe", init, r#"return ui.text(state.line or "waiting")"#))], wire);
    let mut rt = runtime(app);
    pump(&mut rt, 100);
    // Newest first: the building task, then the login test that needs approval.
    assert!(has_text(&rt, "6 tasks, 4 need you, first Tidy the changelog (in-progress, false)"), "{:?}", texts(&rt));
}

#[test]
fn host_ask_drafts_into_the_ask_box_and_never_sends() {
    let (wire, count, _) = counting();
    let render = r#"return ui.button({ label = "Draft", key = "draft-btn", on_press = function() host.ask("What changed in the last hour?") end })"#;
    let mut app = app_with(&[("ui/p.luau", &card("probe", "Probe", "return {}", render))], wire);
    app.ask.replace_text("My own words");
    let mut rt = runtime(app);
    click(&mut rt, "draft-btn");
    assert!(rt.state.layout.neko.is_open(), "Neko's column is open to show the draft");
    assert!(rt.state.ask.text.starts_with("My own words") && rt.state.ask.text.contains("What changed in the last hour?"), "after what the person wrote: {:?}", rt.state.ask.text);
    assert!(rt.state.ask.focused, "the ask box has the keyboard");
    assert!(rt.state.outbox.is_empty(), "nothing was sent: {:?}", rt.state.outbox);
    assert_eq!(count.load(Ordering::SeqCst), 0, "and nothing reached the daemon");
    assert!(!rt.state.work.busy.contains("ask"), "no ask is in flight");
}

#[test]
fn host_ask_and_host_open_refuse_what_they_cannot_do() {
    let (wire, _, _) = counting();
    let init = r#"
    local state = {}
    local a, ae = host.ask("")
    local b, be = host.ask(string.rep("x", 5000))
    local c, ce = host.open("https://example.com/x")
    local d, de = host.open("../etc/passwd")
    state.line = `{tostring(a)}|{tostring(b)}|{tostring(c)}|{tostring(d)}`
    return state"#;
    let app = app_with(&[("ui/p.luau", &card("probe", "Probe", init, "return ui.text(state.line)"))], wire);
    let rt = runtime(app);
    assert!(has_text(&rt, "nil|nil|nil|nil"), "{:?}", texts(&rt));
    assert!(rt.state.ask.text.is_empty() && rt.state.view.is_none());
    // A route that is well formed but not one Rune has is a quiet toast, not a browser.
    let render = r#"return ui.button({ label = "Go", key = "go-btn", on_press = function() host.open("some-page-that-is-not-here") end })"#;
    let (wire, _, _) = counting();
    let mut rt = runtime(app_with(&[("ui/p.luau", &card("probe", "Probe", "return {}", render))], wire));
    click(&mut rt, "go-btn");
    assert!(rt.state.view.is_none() && rt.state.plugin_view.is_none());
    assert!(has_text(&rt, "A plugin asked for a place Neko doesn’t have."), "{:?}", texts(&rt));
}

#[test]
fn host_open_goes_to_the_routes_rune_has() {
    let (wire, _, _) = counting();
    let render = r#"return ui.button({ label = "Go", key = "go-btn", on_press = function() host.open("settings?section=general") end })"#;
    let mut rt = runtime(app_with(&[("ui/p.luau", &card("probe", "Probe", "return {}", render))], wire));
    click(&mut rt, "go-btn");
    assert_eq!(rt.state.view, Some(neko_rune::views::FullView::Settings));
}

#[test]
fn host_ask_and_host_open_do_nothing_in_the_background() {
    let (wire, count, _) = counting();
    let init = r#"
    local state = {}
    task.spawn(function()
        local a, ae = host.ask("drafted from a task")
        local b, be = host.open("settings")
        state.line = `{tostring(a)}|{tostring(b)}|{tostring(ae)}`
    end)
    return state"#;
    let mut rt = runtime(app_with(&[("ui/p.luau", &card("probe", "Probe", init, r#"return ui.text(state.line or "waiting")"#))], wire));
    pump(&mut rt, 80);
    assert!(has_text(&rt, "nil|nil|host.ask only works when a person presses something"), "{:?}", texts(&rt));
    assert!(rt.state.ask.text.is_empty() && rt.state.view.is_none(), "a task cannot draft or navigate");
    assert_eq!(count.load(Ordering::SeqCst), 0);
}

#[test]
fn a_handler_that_loops_cannot_hammer_the_window_and_hover_and_scroll_handlers_do_not_exist() {
    let (wire, _, _) = counting();
    let render = r#"return ui.button({ label = "Loop", key = "loop-btn", on_press = function() for i = 1, 5 do host.ask("draft " .. i) end end })"#;
    let mut rt = runtime(app_with(&[("ui/p.luau", &card("probe", "Probe", "return {}", render))], wire));
    click(&mut rt, "loop-btn");
    assert!(rt.state.ask.text.contains("draft 1") && !rt.state.ask.text.contains("draft 2"), "{:?}", rt.state.ask.text);
    for prop in ["onHover = function() host.ask('x') end", "onVisibleRows = function() end"] {
        let (wire, _, _) = counting();
        let render = format!("return ui.row({{ {prop} }}, {{ 'x' }})");
        let rt = runtime(app_with(&[("ui/p.luau", &card("probe", "Probe", "return {}", &render))], wire));
        assert!(has_text(&rt, "Probe stopped:") && has_text(&rt, "not available to plugins"), "{prop}: {:?}", texts(&rt));
    }
}

#[test]
fn a_table_that_is_too_big_or_too_deep_is_refused_quickly_and_the_window_stays_smooth() {
    let (wire, count, _) = counting();
    let init = r#"
    local state = {}
    task.spawn(function()
        local huge = {}
        for i = 1, 2000000 do huge[i] = i end
        local deep = {}
        local cur = deep
        for i = 1, 100 do cur.x = {} cur = cur.x end
        local cyclic = {}
        cyclic.me = cyclic
        local _, e1 = host.read("Workbench", huge)
        local _, e2 = host.read("Workbench", deep)
        local _, e3 = host.read("Workbench", cyclic)
        local _, e4 = host.storage.set("k", huge)
        state.line = tostring(e1) .. "|" .. tostring(e2) .. "|" .. tostring(e3) .. "|" .. tostring(e4)
    end)
    return state"#;
    let started = std::time::Instant::now();
    let mut rt = runtime(app_with(&[("ui/p.luau", &card("probe", "Probe", init, r#"return ui.text(state.line or "waiting")"#))], wire));
    pump(&mut rt, 200);
    assert!(started.elapsed().as_secs() < 8, "{:?}", started.elapsed());
    let line = texts(&rt).into_iter().find(|t| t.contains("too large")).unwrap_or_else(|| panic!("{:?}", texts(&rt)));
    assert_eq!(line.matches("too").count(), 4, "{line}");
    assert_eq!(count.load(Ordering::SeqCst), 0);
}

/// Runs ticks on the real clock until the text shows or `secs` pass.
fn wait_for(rt: &mut Runtime<App>, needle: &str, secs: f64) -> bool {
    let (start, from) = (std::time::Instant::now(), rt.state.luau.clock.max(1.0));
    loop {
        let now = from + start.elapsed().as_secs_f64();
        rt.update(move |s, tx| {
            s.luau_tick(now, tx);
        });
        rt.frame(0.01);
        if has_text(rt, needle) {
            return true;
        }
        if start.elapsed().as_secs_f64() > secs {
            return false;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

#[test]
fn a_plugin_that_keeps_starting_long_tasks_is_stopped_by_its_cpu_budget_and_not_run_again() {
    let (wire, _, _) = counting();
    // Each step is under the 250 ms watchdog, so only the budget over a second can see it.
    let init = r#"
    local state = { steps = 0 }
    task.spawn(function()
        while true do
            local began = os.clock()
            while os.clock() - began < 0.12 do end
            state.steps += 1
            task.wait(0)
        end
    end)
    return state"#;
    let fine = static_card("fine", "Fine", "clock", "still here");
    let mut rt = runtime(app_with(&[("ui/busy.luau", &card("busy", "Busy", init, "return ui.text('working')")), ("ui/fine.luau", &fine)], wire));
    assert!(wait_for(&mut rt, "Busy stopped: it uses too much time", 20.0), "{:?}", texts(&rt));
    assert!(has_text(&rt, "still here"));
    // Stopped means not run: the steps stop counting.
    let calls = rt.state.luau.plugins[0].script.metrics().vm_calls;
    pump(&mut rt, 60);
    assert_eq!(rt.state.luau.plugins[0].script.metrics().vm_calls, calls, "a stopped plugin's tasks wait for Reload");
}

#[test]
fn print_warn_and_error_cannot_make_the_library_hold_megabytes() {
    let (wire, _, _) = counting();
    let init = r#"
    local state = {}
    task.spawn(function()
        local big = string.rep("x", 8 * 1024 * 1024)
        for i = 1, 30 do print(big) warn(big, big) end
        state.done = true
        error(big)
    end)
    return state"#;
    let rt = {
        let mut rt = runtime(app_with(&[("ui/p.luau", &card("probe", "Probe", init, "return ui.text('x')"))], wire));
        pump(&mut rt, 400);
        rt
    };
    let log = rt.state.luau.plugins[0].script.log();
    assert!(log.len() >= 30, "the lines were logged");
    let longest = log.iter().map(|l| l.message.len()).max().unwrap_or(0);
    assert!(longest <= 1500, "every logged line is cut short, the longest is {longest} bytes");
    assert!(has_text(&rt, "Probe stopped:"));
}

#[test]
fn a_table_that_holds_itself_cannot_blow_up_a_node_or_a_reload() {
    // A table handed to a host node is copied into Rust whole: a DAG that holds itself twice at every level would be 2^64 copies.
    let (wire, _, _) = counting();
    let render = r#"local t = {} t[1] = t t[2] = t return ui.button({ label = "x", junk = t })"#;
    let started = std::time::Instant::now();
    let rt = runtime(app_with(&[("ui/p.luau", &card("probe", "Probe", "return {}", render))], wire));
    assert!(has_text(&rt, "Probe stopped:") && has_text(&rt, "must be a plain value"), "{:?}", texts(&rt));
    assert!(started.elapsed().as_secs() < 5);
    // The same table as state: saving it for a reload is a bounded copy.
    let (wire, _, _) = counting();
    let v1 = card("probe", "One", "local s = { n = 7 } s.a = s s.b = s s.list = { s, s } return s", r#"return ui.text(`n={state.n}`)"#);
    let v2 = card("probe", "Two", "return { n = 0 }", r#"return ui.text(`n={state.n}`)"#);
    let mut rt = runtime(app_with(&[("ui/p.luau", &v1)], wire));
    let started = std::time::Instant::now();
    rt.update(|s, _| {
        s.luau_synced(Ok(vec![PackSync { name: "demo".into(), title: "demo".into(), declared: vec!["ui/p.luau".into()], outcome: Ok(vec![PackUiFile { path: "ui/p.luau".into(), source: v2.clone() }]) }]), None);
    });
    settle(&mut rt);
    assert!(started.elapsed().as_secs() < 5, "{:?}", started.elapsed());
    assert!(has_text(&rt, "n=7") && has_text(&rt, "Two"), "the plain part of the state came across: {:?}", texts(&rt));
}

#[test]
fn a_plugin_has_no_way_to_write_send_or_leave_its_sandbox() {
    let (wire, count, _) = counting();
    let init = r#"
    local state = {}
    local function nope(name, value) return name .. "=" .. (value == nil and "nil" or "present") end
    local frozen = pcall(function() host.read = function() end end)
    local frozen_ui = pcall(function() ui.row = nil end)
    local req = pcall(require, "../outside")
    local req2 = pcall(require, "@self/../../etc/passwd")
    local out = {
        nope("io", io), nope("os.execute", os and os.execute), nope("os.getenv", os and os.getenv), nope("fetch", fetch),
        nope("dofile", dofile), nope("package", package), nope("host.send", host.send), nope("host.request", host.request), nope("host.write", host.write),
        "host_frozen=" .. tostring(not frozen), "ui_frozen=" .. tostring(not frozen_ui), "require_out=" .. tostring(req or req2),
    }
    state.line = table.concat(out, " ")
    return state"#;
    let app = app_with(&[("ui/p.luau", &card("probe", "Probe", init, "return ui.text(state.line)"))], wire);
    let rt = runtime(app);
    let line = texts(&rt).into_iter().find(|t| t.contains("io=")).unwrap_or_else(|| panic!("{:?}", texts(&rt)));
    for must in ["io=nil", "os.execute=nil", "os.getenv=nil", "fetch=nil", "dofile=nil", "package=nil", "host.send=nil", "host.request=nil", "host.write=nil", "host_frozen=true", "ui_frozen=true", "require_out=false"] {
        assert!(line.contains(must), "{must} in {line}");
    }
    assert_eq!(count.load(Ordering::SeqCst), 0);
}

#[test]
fn raw_colours_are_refused_however_they_are_spelled_and_names_are_neko_roles() {
    for render in [
        r##"return ui.text("x", { color = "#ff0000" })"##,
        r##"return ui.row({ fill = "#" .. "00ff00" }, { "x" })"##,
        r##"return ui.col({ border = "#abc" }, { "x" })"##,
        r##"return ui.text("x", { color = ui.hex("#ffffff") })"##,
        r##"return ui.button({ label = "x", fill = "#ff0000" })"##,
        r##"local style = table.freeze({ hover = "#123456" }) return ui.row(style, { "x" })"##,
    ] {
        let (wire, _, _) = counting();
        let app = app_with(&[("ui/p.luau", &card("probe", "Probe", "return {}", render))], wire);
        let rt = runtime(app);
        assert!(has_text(&rt, "Probe stopped:") && has_text(&rt, "raw colours are not allowed"), "{render}: {:?}", texts(&rt));
    }
    let (wire, _, _) = counting();
    let render = r#"return ui.col({ gap = 2 }, { ui.text("muted", { muted = true }), ui.text{ "braced", muted = true, size = 12 }, ui.text("mint", { color = "mint" }), ui.row({ fill = "hover", radius = 4 }, { "role colours" }) })"#;
    let rt = runtime(app_with(&[("ui/p.luau", &card("probe", "Probe", "return {}", render))], wire));
    assert!(has_text(&rt, "braced") && has_text(&rt, "role colours") && !has_text(&rt, "stopped"), "{:?}", texts(&rt));
}

// ---- how a plugin is held -------------------------------------------------------------------------------------------------

#[test]
fn a_runaway_render_is_stopped_by_the_watchdog_and_the_rest_of_the_window_carries_on() {
    let (wire, _, _) = counting();
    let spin = card("spin", "Spinner", "return {}", "while true do end");
    let fine = static_card("fine", "Fine", "clock", "still here");
    let app = app_with(&[("ui/spin.luau", &spin), ("ui/fine.luau", &fine)], wire);
    let started = std::time::Instant::now();
    let rt = runtime(app);
    assert!(started.elapsed().as_secs() < 5, "the window did not freeze: {:?}", started.elapsed());
    assert!(has_text(&rt, "Spinner stopped: it ran longer than 250 ms"), "{:?}", texts(&rt));
    assert!(has_text(&rt, "still here"), "another card still draws");
    assert!(rt.find("luau-reload-demo/ui/spin.luau").is_some(), "Reload is offered");
    assert!(rt.find("session-region").is_some(), "the rest of the app is there");
}

#[test]
fn a_runaway_handler_is_stopped_too() {
    let (wire, _, _) = counting();
    let render = r#"return ui.button({ label = "Spin", key = "spin-btn", on_press = function() while true do end end })"#;
    let mut rt = runtime(app_with(&[("ui/p.luau", &card("probe", "Probe", "return {}", render))], wire));
    click(&mut rt, "spin-btn");
    assert!(has_text(&rt, "Probe stopped: it ran longer than 250 ms"), "{:?}", texts(&rt));
}

#[test]
fn a_plugin_that_eats_memory_is_stopped_at_64_mb() {
    let (wire, _, _) = counting();
    let hog = card("hog", "Hog", "return {}", "local chunk = string.rep('x', 1048576) local held = {} for i = 1, 400 do held[i] = chunk .. i end return ui.text('no')");
    let fine = static_card("fine", "Fine", "clock", "still here");
    let rt = runtime(app_with(&[("ui/hog.luau", &hog), ("ui/fine.luau", &fine)], wire));
    assert!(has_text(&rt, "Hog stopped: it used more than 64 MB"), "{:?}", texts(&rt));
    assert!(has_text(&rt, "still here"));
}

#[test]
fn a_stopped_plugin_shows_one_quiet_line_and_reload_gives_it_a_clean_start() {
    let (wire, _, _) = counting();
    // Fails when pressed (a handler error), runs fine again after Reload.
    let render = r#"return ui.col({}, { ui.text(`presses: {state.n or 0}`), ui.button({ label = "Boom", key = "boom-btn", on_press = function() state.n = (state.n or 0) + 1 if state.n == 1 then error("boom") end end }) })"#;
    let mut rt = runtime(app_with(&[("ui/p.luau", &card("probe", "Probe", "return {}", render))], wire));
    assert!(has_text(&rt, "presses: 0"));
    click(&mut rt, "boom-btn");
    let stopped: Vec<String> = texts(&rt).into_iter().filter(|t| t.contains("stopped")).collect();
    assert_eq!(stopped.len(), 1, "one line: {stopped:?}");
    assert!(stopped[0].starts_with("Probe stopped: ") && stopped[0].contains("boom"), "{stopped:?}");
    assert!(!has_text(&rt, "presses:"), "the card does not draw while it is stopped");
    click(&mut rt, "luau-reload-demo/ui/p.luau");
    assert!(has_text(&rt, "presses: 0") && !has_text(&rt, "stopped"), "{:?}", texts(&rt));
}

#[test]
fn a_file_that_does_not_say_what_it_is_does_not_start_and_says_why() {
    let (wire, _, _) = counting();
    let bad = [
        ("ui/no-slot.luau", "return { id = 'a', title = 'A', render = function() end }"),
        ("ui/bad-id.luau", "return { slot = 'card', id = 'Bad Id', title = 'A', render = function() end }"),
        ("ui/no-title.luau", "return { slot = 'card', id = 'a', render = function() end }"),
        ("ui/no-render.luau", "return { slot = 'card', id = 'a', title = 'A' }"),
        ("ui/syntax.luau", "return {"),
        ("ui/not-a-table.luau", "return 5"),
    ];
    let app = app_with(&bad, wire);
    assert!(app.luau.cards().is_empty());
    for (path, _) in bad {
        assert!(matches!(status_of(&app, path), FileStatus::Stopped(_)), "{path}");
    }
    let FileStatus::Stopped(why) = status_of(&app, "ui/no-slot.luau") else { unreachable!() };
    assert!(why.contains("slot"), "{why}");
    let FileStatus::Stopped(why) = status_of(&app, "ui/bad-id.luau") else { unreachable!() };
    assert!(why.contains("id"), "{why}");
}

// ---- hot reload -----------------------------------------------------------------------------------------------------------

#[test]
fn a_changed_file_reloads_in_place_and_keeps_its_state() {
    let (wire, _, _) = counting();
    let v1 = card("probe", "Probe one", "return { n = 0 }", r#"return ui.button({ label = `count {state.n}`, key = "count-btn", on_press = function() state.n += 1 end })"#);
    let v2 = card("probe", "Probe two", "return { n = 0 }", r#"return ui.button({ label = `now {state.n}`, key = "count-btn", on_press = function() state.n += 1 end })"#);
    let mut rt = runtime(app_with(&[("ui/p.luau", &v1)], wire));
    for _ in 0..3 {
        click(&mut rt, "count-btn");
    }
    assert!(has_text(&rt, "count 3") && has_text(&rt, "Probe one"), "{:?}", texts(&rt));
    // The pack updated: the same pack answers with a changed file.
    rt.update(|s, _| {
        s.luau_synced(Ok(vec![PackSync { name: "demo".into(), title: "demo".into(), declared: vec!["ui/p.luau".into()], outcome: Ok(vec![PackUiFile { path: "ui/p.luau".into(), source: v2.clone() }]) }]), None);
    });
    settle(&mut rt);
    assert!(has_text(&rt, "now 3") && has_text(&rt, "Probe two"), "the new code runs with the old state: {:?}", texts(&rt));
    assert_eq!(rt.state.luau.plugins[0].script.metrics().reloads, 1, "reloaded in place, not started again");
    click(&mut rt, "count-btn");
    assert!(has_text(&rt, "now 4"));
    // An unchanged answer touches nothing; a file that no longer compiles shows the error line and keeps what ran.
    rt.update(|s, _| {
        s.luau_synced(Ok(vec![PackSync { name: "demo".into(), title: "demo".into(), declared: vec!["ui/p.luau".into()], outcome: Ok(vec![PackUiFile { path: "ui/p.luau".into(), source: v2.clone() }]) }]), None);
    });
    assert_eq!(rt.state.luau.plugins[0].script.metrics().reloads, 1);
    rt.update(|s, _| {
        s.luau_synced(Ok(vec![PackSync { name: "demo".into(), title: "demo".into(), declared: vec!["ui/p.luau".into()], outcome: Ok(vec![PackUiFile { path: "ui/p.luau".into(), source: "return {".into() }]) }]), None);
    });
    settle(&mut rt);
    assert!(has_text(&rt, "Probe two stopped:"), "{:?}", texts(&rt));
}

#[test]
fn a_pack_answer_asks_for_its_panels_again_and_the_clock_does_too_every_thirty_seconds() {
    let mut app = App::headless_neko();
    app.luau.next_sync = 500.0;
    app.luau.resync();
    assert_eq!(app.luau.next_sync, 0.0);
    assert_eq!(neko_rune::luau::SYNC_EVERY, 30.0);
    // Any answer about a pack (installed, updated, approved, removed) is an update event.
    app.luau.next_sync = 500.0;
    app.apply_reply(&neko_rune::link::Call::Setup(Box::new(neko_rune::link::SetupAction::PackList)), neko_rune::link::Reply::Packs(PackListing::default()));
    assert_eq!(app.luau.next_sync, 0.0);
}

// ---- the example pack ---------------------------------------------------------------------------------------------------

#[test]
fn the_example_cards_render_headless_in_the_neko_column() {
    let rt = scene_rt("luau-card", true);
    assert!(has_text(&rt, "Cards") && has_text(&rt, "Needs you"));
    assert!(has_text(&rt, "4 sessions need you"), "{:?}", texts(&rt));
    for title in ["Fix the flaky login test", "Review the release notes", "Approve the schema migration plan"] {
        assert!(has_text(&rt, title), "{title}");
    }
    assert!(!has_text(&rt, "Add a retry to the export job"), "only the first three");
    // Each card: a 32 point header and a body that is at most 160 points tall.
    let head = rect(&rt, "luau-card-head-hello-luau/ui/needs-you.luau");
    let body = rect(&rt, "luau-card-body-hello-luau/ui/needs-you.luau");
    assert_eq!(head[3], 32.0);
    assert!(body[3] <= 160.5, "{body:?}");
    // The section sits below the Digest.
    assert!(rect(&rt, "cards")[1] > rect(&rt, "digest")[1]);
}

#[test]
fn the_example_view_renders_headless_with_its_filter_working() {
    let mut rt = scene_rt("luau-view", true);
    assert!(has_text(&rt, "Routines at a glance"));
    for name in ["Messages", "Morning digest", "Pull requests", "Mac care", "Weekly review"] {
        assert!(has_text(&rt, name), "{name}: {:?}", texts(&rt));
    }
    // Typing in the filter reaches the plugin, and the text survives every redraw (the canvas spike's memo bug).
    click(&mut rt, &neko_rune::luau::nodes::input_element_key("hello-luau/ui/routines.luau", "filter"));
    rt.paste("mess");
    settle(&mut rt);
    pump(&mut rt, 60);
    assert!(has_text(&rt, "Messages") && !has_text(&rt, "Morning digest") && !has_text(&rt, "Mac care"), "{:?}", texts(&rt));
    pump(&mut rt, 40);
    assert_eq!(rt.state.luau.inputs.values().next().map(|e| e.text.clone()), Some("mess".to_owned()), "the field kept what was typed");
    rt.paste("xyz");
    settle(&mut rt);
    pump(&mut rt, 60);
    assert!(has_text(&rt, "No routine matches"), "{:?}", texts(&rt));
}

#[test]
fn views_are_listed_next_to_the_web_plugin_pages_and_open_by_route() {
    let mut app = App::headless_neko();
    install_for_tests(&mut app, "hello-luau", &[("ui/needs-you.luau", NEEDS_YOU), ("ui/routines.luau", ROUTINES)], demo_wire());
    let pages: Vec<_> = app.luau.views().iter().filter_map(|p| p.page()).collect();
    assert_eq!(pages.len(), 1);
    assert_eq!((pages[0].id.as_str(), pages[0].title.as_str()), ("luau_hello-luau:routines", "Routines at a glance"));
    assert!(neko_rune::deeplink::clean_route(&pages[0].id).is_some(), "the route is one `--open` accepts");
    app.open_route("luau_hello-luau:routines");
    assert_eq!(app.plugin_view.as_ref().map(|p| p.id.as_str()), Some("luau_hello-luau:routines"));
    let rt = runtime(app);
    assert!(has_text(&rt, "Routines at a glance"));
}

#[test]
fn the_status_rows_say_loaded_with_a_check_and_stopped_with_a_cross_and_the_reason() {
    let rt = scene_rt("luau-status", true);
    for path in ["needs-you.luau", "routines.luau", "weather.luau"] {
        assert!(rt.find(&format!("luau-file-hello-luau-ui-{path}")).is_some(), "{path}");
    }
    let said = |key: &str| rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key)).and_then(|e| e.text).unwrap_or_default();
    assert_eq!(said("luau-file-hello-luau-ui-needs-you.luau-status"), "Loaded");
    assert!(said("luau-file-hello-luau-ui-weather.luau-status").starts_with("Stopped: ui/weather.luau:1: attempt to index nil"));
    let rt = scene_rt("luau-pack", true);
    assert!(rt.find("luau-file-demo-board-layout.luau").is_some() && rt.find("luau-file-demo-board-broken.luau").is_some());
}

#[test]
fn every_luau_scene_renders_in_both_themes() {
    for scene in ["luau-card", "luau-cards-more", "luau-stopped", "luau-view", "luau-status", "luau-pack"] {
        for dark in [true, false] {
            let rt = scene_rt(scene, dark);
            assert!(!rt.elements().is_empty(), "{scene} dark={dark}");
        }
    }
}

// ---- the cards section ----------------------------------------------------------------------------------------------------

#[test]
fn the_cards_section_shows_three_and_and_n_more_and_folds() {
    let mut rt = scene_rt("luau-cards-more", true);
    let heads = |rt: &Runtime<App>| rt.elements().iter().filter(|e| e.key.as_deref().is_some_and(|k| k.starts_with("luau-card-head-"))).count();
    assert_eq!(heads(&rt), 3);
    assert!(has_text(&rt, "and 2 more"), "{:?}", texts(&rt));
    // (The column's top block scrolls inside a share of the window, so the row's own press is reached by scrolling: the press does what this does.)
    assert!(rt.find("cards-more").is_some());
    rt.update(|s, _| s.toggle("cards:all"));
    settle(&mut rt);
    assert_eq!(heads(&rt), 5);
    assert!(has_text(&rt, "Show fewer"));
    rt.update(|s, _| s.toggle("cards:all"));
    settle(&mut rt);
    assert_eq!(heads(&rt), 3);
    // Collapsible, open by default.
    click(&mut rt, "cards-toggle");
    assert_eq!(heads(&rt), 0);
    click(&mut rt, "cards-toggle");
    assert_eq!(heads(&rt), 3);
}

#[test]
fn a_card_has_no_fill_an_icon_or_the_dot_and_a_menu_with_reload_hide_and_open_pack() {
    let mut rt = scene_rt("luau-cards-more", true);
    click(&mut rt, "cards-more");
    right_click(&mut rt, "luau-card-head-hello-luau/ui/today.luau");
    for item in ["Reload", "Hide", "Open pack"] {
        assert!(has_text(&rt, item), "{item}: {:?}", texts(&rt));
    }
    let click_text = |rt: &mut Runtime<App>, label: &str| {
        let r = rt.elements().into_iter().find(|e| e.text.as_deref() == Some(label)).unwrap_or_else(|| panic!("no {label}")).rect;
        rt.pointer_down(r[0] + r[2] / 2.0, r[1] + r[3] / 2.0);
        rt.pointer_up(r[0] + r[2] / 2.0, r[1] + r[3] / 2.0);
        settle(rt);
    };
    click_text(&mut rt, "Hide");
    assert!(rt.state.luau.is_hidden("hello-luau", "today"), "hidden by the person");
    assert!(rt.find("luau-card-head-hello-luau/ui/today.luau").is_none());
    rt.update(|s, _| s.luau_show_all());
    settle(&mut rt);
    assert!(rt.find("luau-card-head-hello-luau/ui/today.luau").is_some() || rt.find("cards-more").is_some());
    right_click(&mut rt, "luau-card-head-hello-luau/ui/focus.luau");
    click_text(&mut rt, "Open pack");
    assert_eq!(rt.state.setup.pack.as_deref(), Some("hello-luau"));
    assert_eq!(rt.state.view, Some(neko_rune::views::FullView::Setup));
}

// ---- storage --------------------------------------------------------------------------------------------------------------

/// A loopback stand-in for the daemon's `/api/storage`: keeps values in memory and records each request line.
type Lines = Arc<Mutex<Vec<String>>>;
type Store = Arc<Mutex<std::collections::BTreeMap<String, String>>>;

fn storage_server() -> (Http, Lines, Store) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let (lines, store) = (Arc::new(Mutex::new(Vec::new())), Arc::new(Mutex::new(std::collections::BTreeMap::<String, String>::new())));
    let (l, s) = (lines.clone(), store.clone());
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut request = String::new();
            reader.read_line(&mut request).unwrap();
            let mut length = 0usize;
            let mut auth = false;
            loop {
                let mut header = String::new();
                reader.read_line(&mut header).unwrap();
                if header == "\r\n" || header.is_empty() {
                    break;
                }
                let lower = header.to_ascii_lowercase();
                if let Some(v) = lower.strip_prefix("content-length:") {
                    length = v.trim().parse().unwrap();
                }
                auth |= header.contains(&format!("Bearer {TOKEN}"));
            }
            let mut body = vec![0u8; length];
            reader.read_exact(&mut body).unwrap();
            let body = String::from_utf8_lossy(&body).into_owned();
            let mut parts = request.split_whitespace();
            let (method, path) = (parts.next().unwrap_or("").to_owned(), parts.next().unwrap_or("").to_owned());
            l.lock().unwrap().push(format!("{method} {path}"));
            let rest = path.strip_prefix("/api/storage/").unwrap_or("");
            let (status, reply) = if !auth {
                (401, String::new())
            } else {
                match (method.as_str(), rest.split_once('/')) {
                    ("GET", None) => {
                        let prefix = format!("{rest}/");
                        let keys: Vec<String> = s.lock().unwrap().iter().filter_map(|(k, v)| k.strip_prefix(&prefix).map(|key| format!("{{\"key\":\"{key}\",\"size\":{}}}", v.len()))).collect();
                        (200, format!("{{\"keys\":[{}]}}", keys.join(",")))
                    }
                    ("GET", Some(_)) => (200, s.lock().unwrap().get(rest).cloned().unwrap_or_else(|| "null".to_owned())),
                    ("PUT", Some(_)) => {
                        s.lock().unwrap().insert(rest.to_owned(), body);
                        (200, "{\"ok\":true}".to_owned())
                    }
                    _ => (405, String::new()),
                }
            };
            let mut stream = stream;
            let _ = write!(stream, "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}", reply.len());
        }
    });
    (Http::new(port, TOKEN), lines, store)
}

#[test]
fn host_storage_keeps_json_in_the_packs_own_namespace_up_to_256_kb() {
    let (http, lines, store) = storage_server();
    let wire = Wire::fake(|_| Err("no daemon".into()), Some(http));
    let init = r#"
    local state = {}
    task.spawn(function()
        local out = {}
        local function note(label, a, b) table.insert(out, label .. "=" .. tostring(a) .. (b and ("/" .. tostring(b)) or "")) end
        note("unset", host.storage.get("never"))
        note("set", host.storage.set("settings", { filter = "x", n = 3, list = { 1, 2, 3 } }))
        local back = host.storage.get("settings")
        note("back", back and back.filter, back and back.list and back.list[3])
        note("badkey", host.storage.set("../escape", 1))
        task.wait(1.05)
        note("dotkey", host.storage.set(".hidden", 1))
        note("big", host.storage.set("big", string.rep("x", 300 * 1024)))
        task.wait(1.05)
        note("fill", host.storage.set("fill", string.rep("y", 200 * 1024)))
        task.wait(1.05)
        note("over", host.storage.set("more", string.rep("z", 100 * 1024)))
        task.wait(1.05)
        note("replace", host.storage.set("fill", "small"))
        state.line = table.concat(out, " ")
    end)
    return state"#;
    let app = app_with(&[("ui/p.luau", &card("probe", "Probe", init, r#"return ui.text(state.line or "waiting")"#))], wire);
    let mut rt = runtime(app);
    pump(&mut rt, 5600);
    let line = texts(&rt).into_iter().find(|t| t.starts_with("unset=")).unwrap_or_else(|| panic!("{:?} {:?}", texts(&rt), lines.lock().unwrap()));
    assert!(line.contains("unset=nil"), "{line}");
    assert!(line.contains("set=true"), "{line}");
    assert!(line.contains("back=x/3"), "{line}");
    assert!(line.contains("badkey=nil/") && line.contains("dotkey=nil/"), "{line}");
    assert!(line.contains("big=nil/That value is too big to store"), "{line}");
    assert!(line.contains("fill=true"), "{line}");
    assert!(line.contains("over=nil/A pack keeps at most 256 KB"), "{line}");
    assert!(line.contains("replace=true"), "overwriting a key counts its new size, not both: {line}");
    // Everything went to the pack's own namespace and nowhere else.
    for l in lines.lock().unwrap().iter() {
        assert!(l.contains("/api/storage/luau_demo"), "{l}");
    }
    assert!(store.lock().unwrap().keys().all(|k| k.starts_with("luau_demo/")));
}

// ---- unit-level odds -------------------------------------------------------------------------------------------------------

#[test]
fn the_catalog_example_installs_nothing_by_itself_and_its_files_are_the_ones_drawn() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packs/hello-luau");
    let manifest = std::fs::read_to_string(dir.join("pack.toml")).unwrap();
    assert!(manifest.contains("name = \"hello-luau\"") && manifest.contains("luau = [\"ui/needs-you.luau\", \"ui/routines.luau\"]"));
    for forbidden in ["[[mcp]]", "[[skills]]", "[[routines]]", "[health]", "web_plugin = \"w"] {
        assert!(!manifest.contains(forbidden), "the example has no {forbidden}");
    }
    assert_eq!(std::fs::read_to_string(dir.join("ui/needs-you.luau")).unwrap(), NEEDS_YOU);
    assert_eq!(std::fs::read_to_string(dir.join("ui/routines.luau")).unwrap(), ROUTINES);
    assert!(BROKEN.contains("forecast.today"));
}

#[test]
fn a_file_that_does_not_compile_stops_only_the_plugins_that_use_it() {
    let (wire, _, _) = counting();
    let user = "local helper = require(\"./broken\")\nreturn { slot = 'card', id = 'user', title = 'User', render = function() end }";
    let files = [("ui/broken.luau", "return {"), ("ui/fine.luau", &static_card("fine", "Fine", "clock", "still here")), ("ui/user.luau", user)];
    let app = app_with(&files, wire);
    assert!(matches!(status_of(&app, "ui/broken.luau"), FileStatus::Stopped(_)));
    assert!(matches!(status_of(&app, "ui/user.luau"), FileStatus::Stopped(_)), "it requires the broken one");
    assert_eq!(status_of(&app, "ui/fine.luau"), FileStatus::Loaded, "an unrelated file is not touched");
}
