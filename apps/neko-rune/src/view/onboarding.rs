//! The first-run setup, in the session region in place of Home (`web/src/onboarding/*`): a 560 point column, six steps, a
//! step indicator of six dots (Iris for the current one, muted for the others), "Continue" and a quiet "Skip setup".
//! The state and the requests are `onboarding.rs`; every switch saves what it changes, as the web's does.

use neko_protocol::agent_models::ModelSource;
use rune::{El, ScrollSpec, TextStyle, col, el, icon, row, text};

use super::ui::{edit_of, editing, switch, switch_when};
use super::{band, primary_button, quiet_button};
use crate::app::App;
use crate::icons;
use crate::onboarding::{AGENT_CLIS, AgentState, Step, agent_state};
use crate::theme::Neko;

const COLUMN: f32 = 560.0;

fn lines(t: &Neko, key: &str, title: &str, subtitle: &str) -> El {
    col().w_full().gap(8.0).child(text(title.to_owned()).key(format!("{key}-title")).font(TextStyle { size: 24.0, ..t.ty.title }.lh(32.0)).color(t.text.strong).role("heading").wrap().min_w(0.0)).child(text(subtitle.to_owned()).key(format!("{key}-subtitle")).font(t.ty.row).color(t.text.muted).wrap().min_w(0.0))
}

/// One line of a step: a title, a muted line under it, and what the person does at the right.
fn line(t: &Neko, key: &str, title: &str, detail: &str, trailing: El) -> El {
    let mut words = col().grow(1.0).min_w(0.0).gap(2.0).child(text(title.to_owned()).key(format!("{key}-title")).font(t.ty.row).color(t.text.base).wrap().min_w(0.0));
    if !detail.is_empty() {
        words = words.child(text(detail.to_owned()).key(format!("{key}-detail")).font(t.ty.meta).color(t.text.muted).wrap().min_w(0.0));
    }
    row().key(key.to_owned()).w_full().min_h(44.0).px(8.0).py(6.0).gap(12.0).center_y().radius(t.radius.row).hover(t.surface.hover).child(words).child(trailing)
}

fn ready(t: &Neko, key: &str, words: &str) -> El {
    row().key(key.to_owned()).gap(6.0).center_y().child(icon(&icons::CHECK_LINE, 16.0).color(t.meaning.mint).a11y_hidden()).child(text(words.to_owned()).font(t.ty.meta).color(t.meaning.mint))
}

fn copy_command(t: &Neko, key: &str, command: &str) -> El {
    let c = command.to_owned();
    row()
        .key(key.to_owned())
        .gap(6.0)
        .center_y()
        .child(text(command.to_owned()).font(t.ty.mono).color(t.text.base).px(6.0).py(2.0).radius(6.0).fill(t.surface.hover))
        .child(quiet_button("Copy", &format!("{key}-copy"), t, |_: &mut App| {}).label(format!("Copy {command}")).on_press_tx(move |s: &mut App, tx: &mut rune::Tx| {
            tx.copy(c.clone());
            s.show_toast("Copied");
        }))
}

fn welcome(s: &App, t: &Neko) -> El {
    let owner = s.pages.routines.as_ref().map(|r| r.owner_name.clone()).filter(|n| !n.is_empty());
    let title = owner.map_or("A little more off your mind.".to_owned(), |o| format!("Hi {o}. A little more off your mind."));
    col()
        .key("onb-welcome")
        .w_full()
        .gap(20.0)
        .items_center()
        .py(24.0)
        .child(icon(&icons::NEKO_MARK, 40.0).color(t.text.strong).a11y_hidden())
        .child(text(title).key("onb-welcome-title").font(TextStyle { size: 30.0, ..t.ty.greeting }.lh(38.0)).color(t.text.strong).role("heading").wrap().min_w(0.0))
        .child(text("Neko hands your work to the coding agents already on this Mac and tells you when something needs you. Nothing runs on its own until you say so, and every step here can be skipped.").key("onb-welcome-body").font(t.ty.row).color(t.text.muted).wrap().max_w(460.0).min_w(0.0))
}

fn agents(s: &App, t: &Neko) -> El {
    let sources: Option<&Vec<ModelSource>> = s.runtime.catalog.as_ref().or(s.pages.catalog.as_ref()).map(|c| &c.sources);
    let mut c = col().key("onb-agents").w_full().gap(20.0).child(lines(t, "onb-agents", "Which agents are on this Mac?", "Neko does not bring its own model. It runs the agent CLIs you already have, with your sign-in."));
    let mut list = col().key("onb-agent-list").w_full().gap(1.0);
    let mut ready_count = 0;
    match sources {
        None => list = list.child(text("Looking for agents…").key("onb-agents-loading").font(t.ty.meta).color(t.text.muted).px(8.0).py(6.0)),
        Some(sources) => {
            for (provider, name, install, sign_in) in AGENT_CLIS {
                let source = sources.iter().find(|x| x.provider == provider);
                let state = agent_state(source);
                let connection = source.map(|x| x.connection.clone()).unwrap_or_default();
                let key = format!("onb-agent-{provider}");
                let (detail, trailing) = match state {
                    AgentState::Ready => {
                        ready_count += 1;
                        (if connection.is_empty() { "Ready".to_owned() } else { connection }, ready(t, &format!("{key}-ready"), "Ready"))
                    }
                    AgentState::SignIn => ("Found, not signed in. Run this in Terminal, finish signing in, then check again.".to_owned(), copy_command(t, &format!("{key}-cmd"), sign_in)),
                    AgentState::Missing => ("Not installed. Optional. Install it in Terminal, then check again.".to_owned(), copy_command(t, &format!("{key}-cmd"), install)),
                    AgentState::Error => (if connection.is_empty() { "Could not be read".to_owned() } else { connection }, el()),
                };
                list = list.child(line(t, &key, name, &detail, trailing));
            }
        }
    }
    c = c.child(list);
    let mut foot = row().w_full().px(8.0).gap(12.0).center_y().child(if s.onboarding.checking { quiet_button("Checking…", "onb-agents-check", t, |_| {}).disabled(true) } else { quiet_button("Check again", "onb-agents-check", t, |s: &mut App| s.onboarding_check_agents()) });
    if sources.is_some() && ready_count == 0 {
        foot = foot.child(text("Neko needs one of these to do work. You can finish setup now and come back.").key("onb-agents-none").font(t.ty.meta).color(t.text.muted).wrap().min_w(0.0).grow(1.0));
    }
    c.child(foot)
}

fn home_relative(path: &str, home: Option<&str>) -> String {
    match home {
        Some(h) if path.starts_with(h) => format!("~{}", &path[h.len()..]),
        _ => path.to_owned(),
    }
}

fn workspaces(s: &App, t: &Neko) -> El {
    let mut c = col().key("onb-workspaces").w_full().gap(20.0).child(lines(t, "onb-workspaces", "Where do you work?", "A workspace is a folder and the tools that belong to it. Neko found these on this Mac; add the ones you want, or any other folder."));
    let known = s.summary.as_ref().map(|m| m.settings.workspaces.clone()).unwrap_or_default();
    let scopes = s.pages.routines.as_ref().map(|r| r.scopes.clone());
    let home = std::env::var("HOME").ok();
    let mut list = col().key("onb-workspace-list").w_full().gap(1.0);
    match &scopes {
        None => list = list.child(text("Looking for your folders…").key("onb-ws-loading").font(t.ty.meta).color(t.text.muted).px(8.0).py(6.0)),
        Some(scopes) => {
            for (i, scope) in scopes.iter().enumerate() {
                let folder = scope.folders.first().cloned().unwrap_or_default();
                let mut names: Vec<String> = scope.servers.iter().map(|x| x.name.clone()).collect();
                names.dedup();
                let tools = if names.is_empty() { "no tool servers".to_owned() } else { format!("{} tool server{}: {}{}", names.len(), if names.len() == 1 { "" } else { "s" }, names.iter().take(4).cloned().collect::<Vec<_>>().join(", "), if names.len() > 4 { format!(", +{}", names.len() - 4) } else { String::new() }) };
                let added = scope.workspace_id.is_some();
                let (f, n) = (folder.clone(), scope.name.clone());
                let trailing = if added { ready(t, &format!("onb-ws-{i}-added"), "Added") } else { quiet_button("Add", &format!("onb-ws-{i}-add"), t, move |s: &mut App| s.onboarding_add_workspace(&f, Some(&n))).label(format!("Add {}", scope.name)) };
                list = list.child(line(t, &format!("onb-ws-{i}"), &scope.name, &format!("{} · {tools}", home_relative(&folder, home.as_deref())), trailing));
            }
            let covered: std::collections::HashSet<&str> = scopes.iter().filter_map(|x| x.workspace_id.as_deref()).collect();
            for (i, w) in known.iter().filter(|w| !covered.contains(w.id.as_str())).enumerate() {
                list = list.child(line(t, &format!("onb-own-{i}"), &w.name, &home_relative(&w.repository, home.as_deref()), ready(t, &format!("onb-own-{i}-added"), "Added")));
            }
            if scopes.is_empty() && known.is_empty() {
                list = list.child(text("No project folders with tools set up were found. Add a folder below.").key("onb-ws-none").font(t.ty.meta).color(t.text.muted).px(8.0).py(6.0));
            }
        }
    }
    c = c.child(list);
    let typed = !s.pages.text("onb-path").trim().is_empty();
    let field = rune_kit::TextInput::new(edit_of(s, "onb-path")).label("Folder path").placeholder(format!("{}/Projects/app", home.as_deref().unwrap_or("/Users/you"))).width(None).on_edit(editing("onb-path".to_owned())).on_submit(|s: &mut App, _| {
        let path = s.pages.text("onb-path").to_owned();
        s.onboarding_add_workspace(&path, None);
    }).build(&t.kit).key("onb-path").grow(1.0).min_w(0.0);
    let add = if typed {
        quiet_button("Add workspace", "onb-path-add", t, |s: &mut App| {
            let path = s.pages.text("onb-path").to_owned();
            s.onboarding_add_workspace(&path, None);
        })
    } else {
        quiet_button("Add workspace", "onb-path-add", t, |_| {}).disabled(true)
    };
    c.child(col().w_full().gap(6.0).child(text("Add a folder").font(t.ty.group).color(t.text.muted).px(8.0)).child(row().w_full().gap(8.0).center_y().child(field).child(add)))
}

fn routines(s: &App, t: &Neko) -> El {
    let data = s.pages.routines.as_ref();
    let on = data.is_some_and(|d| d.routines_on);
    let scopes = data.map_or(0, |d| d.scopes.len());
    let detail = if on {
        "On. A workspace is added for every scope found on this Mac, and each gets its routines. Pause any of them from the Inbox.".to_owned()
    } else if scopes > 0 {
        format!("Off. Turning them on adds a workspace for each of the {scopes} scope{} found, and your home folder as Personal.", if scopes == 1 { "" } else { "s" })
    } else {
        "Off".to_owned()
    };
    let mut c = col().key("onb-routines").w_full().gap(20.0).child(lines(t, "onb-routines", "Let Neko look after your day?", "Routines are standing orders that run in the background: check messages, a morning brief, a sweep of pull requests, learning how you work. They use your agents and your tools, so they are off until you turn them on."));
    c = c.child(line(t, "onb-run-routines", "Run routines", &detail, switch_when(t, "onb-routines-switch", "Run routines", on, data.is_some(), move |s: &mut App| s.call(crate::link::Call::SetRoutinesOn { enabled: !on }))));
    let no_agent = !s.runtime.catalog.as_ref().or(s.pages.catalog.as_ref()).is_some_and(|c| c.sources.iter().any(|x| agent_state(Some(x)) == AgentState::Ready));
    if no_agent {
        c = c.child(text("No agent is signed in yet, so routines would wait until one is.").key("onb-routines-no-agent").font(t.ty.meta).color(t.text.muted).px(8.0));
    }
    c
}

fn mac(s: &App, t: &Neko) -> El {
    let calendar = s.pages.calendar.as_ref().map(|c| c.status.enabled);
    let phone = s.pages.phone.as_ref().map(|p| p.settings.reminders);
    let clipboard = s.extra.clipboard;
    let tog = |key: &str, label: &str, on: Option<bool>, f: fn(&mut App, bool)| match on {
        Some(on) => switch(t, key, label, on, move |s: &mut App| f(s, !on)),
        None => switch_when(t, key, label, false, false, |_| {}),
    };
    col()
        .key("onb-mac")
        .w_full()
        .gap(20.0)
        .child(lines(t, "onb-mac", "Reach you, and read your day", "Each of these is optional. macOS asks you once, when you switch one on."))
        .child(
            col()
                .w_full()
                .gap(1.0)
                .child(line(t, "onb-calendar", "Calendar", "Show your meetings beside Neko’s work", tog("onb-calendar-switch", "Calendar", calendar, |s, on| s.call(crate::link::Call::SetCalendarOn { enabled: on }))))
                .child(line(t, "onb-phone", "Phone reminders", "A reminder on your iPhone when something needs you. Uses Apple Reminders through iCloud, only when you are away from the Mac. Settings, Phone has quiet hours and an optional ntfy channel.", tog("onb-phone-switch", "Phone reminders", phone, |s, on| s.change_phone("reminders", |p| p.reminders = on))))
                .child(line(t, "onb-clipboard", "Clipboard history", "Saved only on this Mac", tog("onb-clipboard-switch", "Clipboard history", clipboard, |s, on| s.set_clipboard(on))))
                .child(line(t, "onb-accessibility", "Accessibility", "Lets the quick panel paste for you", quiet_button("Open Settings", "onb-accessibility-open", t, |s: &mut App| {
                    if (s.opener)("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility").is_err() {
                        s.onboarding.error = Some("Couldn’t open System Settings.".to_owned());
                    }
                }))),
        )
}

fn done(s: &App, t: &Neko) -> El {
    let agents = s.runtime.catalog.as_ref().or(s.pages.catalog.as_ref()).map_or(0, |c| AGENT_CLIS.iter().filter(|(p, ..)| agent_state(c.sources.iter().find(|x| x.provider == *p)) == AgentState::Ready).count());
    let workspaces = s.summary.as_ref().map_or(0, |m| m.settings.workspaces.len());
    let on = |v: Option<bool>| match v {
        None => "…",
        Some(true) => "On",
        Some(false) => "Off",
    };
    let summary: [(&str, String); 5] = [
        ("Agents ready", if agents > 0 { agents.to_string() } else { "None yet".to_owned() }),
        ("Workspaces", if workspaces > 0 { workspaces.to_string() } else { "None yet".to_owned() }),
        ("Routines", on(s.pages.routines.as_ref().map(|r| r.routines_on)).to_owned()),
        ("Calendar", on(s.pages.calendar.as_ref().map(|c| c.status.enabled)).to_owned()),
        ("Phone reminders", on(s.pages.phone.as_ref().map(|p| p.settings.reminders)).to_owned()),
    ];
    let mut list = col().key("onb-summary").w_full().gap(2.0);
    for (i, (label, value)) in summary.iter().enumerate() {
        list = list.child(row().key(format!("onb-sum-{i}")).w_full().px(8.0).gap(12.0).h(28.0).center_y().child(text((*label).to_owned()).font(t.ty.row).color(t.text.muted).w(160.0)).child(text(value.clone()).key(format!("onb-sum-{i}-value")).font(t.ty.row).color(t.text.strong)));
    }
    col()
        .key("onb-done")
        .w_full()
        .gap(20.0)
        .child(lines(t, "onb-done", "Neko is ready.", "Tell it what you want done. It plans, asks before anything risky, and tells you when something needs you. You can open this setup again from Settings, General."))
        .child(list)
        .child(text("⌘K finds any page or action. The quick panel opens from anywhere with ⌥ Space.").font(t.ty.meta).color(t.text.muted).px(8.0))
}

/// Six dots, the current one Iris (the web's progress list).
fn dots(s: &App, t: &Neko) -> El {
    let mut d = row().key("onb-dots").gap(8.0).center_y().role("list").label("Progress");
    for (i, step) in Step::ALL.iter().enumerate() {
        let current = i == s.onboarding.step;
        d = d.child(el().key(format!("onb-dot-{i}")).size(if current { 18.0 } else { 6.0 }, 6.0).radius(3.0).fill(if current { t.meaning.iris } else { t.text.muted.alpha(0.4) }).label(if current { format!("{} (current step)", step.label()) } else { step.label().to_owned() }).help(step.label()));
    }
    d
}

pub fn view(s: &App, t: &Neko) -> El {
    let step = s.onboarding.current();
    let body = match step {
        Step::Welcome => welcome(s, t),
        Step::Agents => agents(s, t),
        Step::Workspaces => workspaces(s, t),
        Step::Routines => routines(s, t),
        Step::Mac => mac(s, t),
        Step::Done => done(s, t),
    };
    let mut content = col().key("onb-content").w_full().max_w(COLUMN).px(16.0).py(32.0).gap(16.0).child(body);
    if let Some(error) = &s.onboarding.error {
        content = content.child(text(error.clone()).key("onb-error").font(t.ty.meta).color(t.meaning.coral).wrap().min_w(0.0).role("alert"));
    }
    let scroll = col().key("onb-scroll").w_full().grow(1.0).min_h(0.0).scroll(ScrollSpec { stick_bottom: false }).child(row().w_full().justify_center().child(content));
    let busy = s.onboarding.busy;
    let back = if s.onboarding.step == 0 { el().w(72.0).h(28.0) } else { quiet_button("Back", "onb-back", t, |s: &mut App| s.onboarding_back()).w(72.0) };
    let skip = quiet_button("Skip setup", "onb-skip", t, |s: &mut App| s.onboarding_complete());
    let skip = if busy { skip.disabled(true) } else { skip };
    let next_label = if s.onboarding.last() { "Done" } else { "Continue" };
    let next = if busy { primary_button(next_label, "onb-next", t, |_| {}).disabled(true) } else { primary_button(next_label, "onb-next", t, |s: &mut App| s.onboarding_next()) };
    let footer = row().key("onb-footer").w_full().px(20.0).py(12.0).gap(12.0).center_y().child(back).child(row().grow(1.0).justify_center().child(dots(s, t))).child(skip).child(next);
    col().key("onboarding").w_full().h_full().child(band(t)).child(scroll).child(footer)
}
