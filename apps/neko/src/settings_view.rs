//! Settings (⌘,): two pages. "Agents": each agent CLI Neko knows, as found on this Mac, which one new tasks use, and how much a
//! task's agent may do without asking. Neko asks for no API key: it uses the CLIs already installed and signed in. "Workspaces":
//! the folders the person added, with Rename, Reveal in Finder and Remove.

use neko_protocol::{PermissionMode, RuntimeInfo};
use rune::{El, col, row, text};
use rune_ui_style::weight;

use crate::ui::base::Shape;

use crate::state::App;
use crate::task_view::command_line;
use crate::ui::badge::Badge;
use crate::ui::base::{Theme, Tone, glyph, paint};
use crate::ui::icons;
use crate::ui::radio_group::RadioGroup;
use crate::ui::button::Button;
use crate::ui::input::Input;
use crate::notify::Allowed;
use crate::ui::segmented_control::SegmentedControl;
use crate::ui::switch::Switch;
use crate::view::COLUMN;
use rune_ui_style::{ButtonVariant, Size};
use std::rc::Rc;

/// What Settings says about full access, plainly.
pub const FULL_ACCESS_WARNING: &str =
    "With full access the agent runs any command, changes any file and reaches the internet without asking you. Use it only for work you could lose.";

fn state_of(r: &RuntimeInfo) -> (&'static str, Tone) {
    match (r.found, r.supported, r.signed_in) {
        (false, _, _) => ("Not installed", Tone::Neutral),
        (true, false, _) => ("Not supported yet", Tone::Neutral),
        (true, true, Some(false)) => ("Not signed in", Tone::Warning),
        (true, true, _) => ("Ready", Tone::Success),
    }
}

fn agent_card(t: &Theme, r: &RuntimeInfo) -> El {
    let (word, tone) = state_of(r);
    let key = format!("agent-{}", r.id);
    let head = row()
        .w_full()
        .gap(t.space.at(3.0))
        .center_y()
        .child(text(r.name.clone()).role("heading").level(3).font(weight(t.ty.md, t.ty.medium)).color(t.ink()).grow(1.0).truncate().min_w(0.0).shrink())
        .child(Badge::new(format!("{key}-state"), word).tone(tone).dot().build(t));
    let mut facts: Vec<String> = Vec::new();
    if let Some(v) = &r.version {
        facts.push(format!("Version {v}"));
    }
    match (r.signed_in, &r.login) {
        (Some(true), Some(l)) => facts.push(format!("Signed in with {l}")),
        (Some(true), None) => facts.push("Signed in".into()),
        (Some(false), _) => facts.push("Not signed in".into()),
        (None, _) if r.found && r.supported => facts.push("Could not tell whether it is signed in".into()),
        _ => {}
    }
    if r.found && !r.supported {
        facts.push(format!("This Neko cannot run {} yet", r.name));
    }
    let frame = col().key(key.clone()).role("group").label(format!("{}: {word}", r.name)).w_full().padding(t.metrics.inset).gap(t.space.at(2.5)).radius(t.radius.xl).round_as(t);
    let frame = paint(frame, &t.mat.slab.base);
    let mut card = frame.child(head);
    if !facts.is_empty() {
        card = card.child(text(facts.join(" · ")).key(format!("{key}-facts")).font(t.ty.base).color(t.ink_muted()).wrap().min_w(0.0));
    }
    if !r.found {
        card = card.child(text("Install it in Terminal:").font(t.ty.sm).color(t.ink_faint())).child(command_line(t, &format!("{key}-install"), &r.install));
    } else if r.signed_in == Some(false) {
        card = card.child(text("Sign in in Terminal:").font(t.ty.sm).color(t.ink_faint())).child(command_line(t, &format!("{key}-sign-in"), &r.sign_in));
    }
    card
}

/// Settings' pages, in the order the switch shows them.
pub const PAGES: [&str; 6] = ["Agents", "Workspaces", "Notifications", "Cats", "Briefing", "Plugins"];

pub fn settings_view(s: &App, t: &Theme, layer: &mut Vec<rune::El>) -> El {
    let mut page = col().key("settings").w_full().max_w(COLUMN).gap(t.space.at(6.0));
    let switch = SegmentedControl::<App>::new("settings-page", "Settings page", &PAGES, s.settings_page.min(PAGES.len() - 1)).size(Size::Sm).on_change(|s: &mut App, i| {
        s.settings_page = i;
        if i == 2 {
            s.read_allowed_soon();
        }
    })
    .build(t);
    page = page.child(
        row()
            .w_full()
            .gap(t.space.at(4.0))
            .center_y()
            .child(text("Settings").key("settings-title").role("heading").level(1).font(weight(t.ty.xl, t.ty.medium)).color(t.ink()).grow(1.0))
            .child(switch),
    );
    if s.settings_page == 1 {
        return wrap(t, page.child(workspaces_page(s, t)));
    }
    if s.settings_page == 2 {
        return wrap(t, page.child(notifications_page(s, t)));
    }
    if s.settings_page == 3 {
        return wrap(t, page.child(cats_page(s, t)));
    }
    if s.settings_page == 4 {
        return wrap(t, page.child(briefing_page(s, t)));
    }
    if s.settings_page == crate::plugins::SETTINGS_PAGE {
        return wrap(t, page.child(crate::plugins_view::page(s, t, layer)));
    }
    let mut agents = col().key("settings-agents").w_full().gap(t.space.at(3.0)).child(text("Agents").role("heading").level(2).font(weight(t.ty.md, t.ty.semibold)).color(t.ink())).child(
        text("Neko uses the agent CLIs already installed and signed in on this Mac. It asks for no API key and never stores one.").font(t.ty.base).color(t.ink_muted()).wrap().min_w(0.0),
    );
    match &s.runtimes {
        None => agents = agents.child(text("Looking for agent CLIs…").key("settings-looking").role("status").font(t.ty.base).color(t.ink_faint())),
        Some((list, settings)) => {
            for r in list {
                agents = agents.child(agent_card(t, r));
            }
            let usable: Vec<&RuntimeInfo> = list.iter().filter(|r| r.supported).collect();
            let names: Vec<String> = usable.iter().map(|r| r.name.clone()).collect();
            let options: Vec<(&str, Option<&str>)> = names.iter().map(|n| (n.as_str(), None)).collect();
            let chosen = usable.iter().position(|r| r.id == settings.runtime);
            let ids: Vec<String> = usable.iter().map(|r| r.id.clone()).collect();
            agents = agents.child(text("New tasks use").role("heading").level(3).font(weight(t.ty.base, t.ty.medium)).color(t.ink()).pt(t.space.at(2.0))).child(
                RadioGroup::<App>::new("settings-runtime", "New tasks use", &options, chosen)
                    .on_change(move |s: &mut App, i| {
                        if let Some(id) = ids.get(i) {
                            s.set_runtime(id);
                        }
                    })
                    .build(t),
            );
            let mode = settings.permission_mode;
            let modes = [
                (
                    "Follow Claude Code's own rules",
                    Some("Neko adds nothing. What your Claude Code settings allow runs without asking, as in Terminal. Anything Claude Code would ask about is asked in the task."),
                ),
                (
                    "Ask before risky steps",
                    Some("Neko asks before every command, any change outside the task's folder and the internet, whatever your Claude Code settings allow. Reading, and editing in the task's own folder, go ahead."),
                ),
                ("Full access", Some("Everything goes ahead without asking.")),
            ];
            let chosen = match mode {
                PermissionMode::Follow => 0,
                PermissionMode::Ask => 1,
                PermissionMode::Full => 2,
            };
            // What the person's own Claude Code settings let it do, read from them (never written), when Neko follows them. It sits
            // right under the heading, so it is in sight without scrolling in the default window.
            let rules = list.iter().find(|r| r.id == settings.runtime).and_then(|r| r.rules.clone());
            let mut perms = col().key("settings-permissions").w_full().gap(t.space.at(3.0)).child(text("Permissions").role("heading").level(2).font(weight(t.ty.md, t.ty.semibold)).color(t.ink()));
            if let (PermissionMode::Follow, Some(line)) = (mode, rules) {
                perms = perms.child(
                    row()
                        .key("settings-rules")
                        .role("status")
                        .label(line.clone())
                        .w_full()
                        .gap(t.metrics.gap)
                        .items_start()
                        .child(row().size(t.control.icon, t.ty.base.line_height).center().child(glyph(&icons::INFO, t.control.icon, t.ink_faint())))
                        .child(text(line).font(t.ty.base).color(t.ink_muted()).wrap().min_w(0.0).shrink().a11y_hidden()),
                );
            }
            perms = perms.child(
                RadioGroup::<App>::new("settings-mode", "How much the agent may do without asking", &modes, Some(chosen))
                    .on_change(|s: &mut App, i| {
                        s.set_permission_mode(match i {
                            1 => PermissionMode::Ask,
                            2 => PermissionMode::Full,
                            _ => PermissionMode::Follow,
                        })
                    })
                    .build(t),
            );
            if mode == PermissionMode::Full {
                perms = perms.child(
                    row()
                        .key("settings-full-warning")
                        .role("status")
                        .label(FULL_ACCESS_WARNING)
                        .w_full()
                        .gap(t.metrics.gap)
                        .items_start()
                        .child(row().size(t.control.icon, t.ty.base.line_height).center().child(glyph(&icons::WARNING, t.control.icon, t.c.warning.fg)))
                        .child(text(FULL_ACCESS_WARNING).font(t.ty.base).color(t.ink()).wrap().min_w(0.0).shrink().a11y_hidden()),
                );
            }
            page = page.child(agents).child(perms);
            return wrap(t, page);
        }
    }
    wrap(t, page.child(agents))
}

/// The Workspaces page: each workspace with its folder and how many tasks it has, and what can be done to it.
/// Settings ▸ Cats: every cat, the ones at work with Open, the retired ones with Bring back (docs/knowledge/cats.md).
fn cats_page(s: &App, t: &Theme) -> El {
    let mut part = col().key("settings-cats").w_full().gap(t.space.at(3.0)).child(text("Cats").role("heading").level(2).font(weight(t.ty.md, t.ty.semibold)).color(t.ink())).child(
        text("The cats who do your tasks, each with its own standing rules. A retired cat keeps its tasks readable and takes no new ones until you bring it back. At most 16 are at work at once.")
            .font(t.ty.base)
            .color(t.ink_muted())
            .wrap()
            .min_w(0.0),
    );
    // Cats are made here (the sidebar lists only the cats at work): name it, and its page opens.
    part = part.child(row().child(Button::new("settings-new-cat", "New cat…").variant(ButtonVariant::Secondary).size(Size::Sm).icon(&crate::ui::icons::PLUS).on_press(|s: &mut App| s.start_new_cat()).build(t)));
    for c in &s.cats {
        let key = format!("settings-cat-{}", c.id);
        let n = s.tasks.iter().filter(|x| x.cat == c.id).count();
        let count = match n {
            0 => "No tasks yet".to_owned(),
            1 => "1 task".to_owned(),
            n => format!("{n} tasks"),
        };
        let state = s.cat_state(&c.id).words();
        let mut head = row().w_full().gap(t.space.at(2.0)).center_y().child(text(c.name.clone()).role("heading").level(3).font(weight(t.ty.md, t.ty.semibold)).color(t.ink()).truncate().min_w(0.0).shrink());
        if !c.is_active() {
            head = head.child(Badge::new(format!("{key}-retired"), "Retired").tone(Tone::Neutral).dot().build(t));
        }
        let facts = col().grow(1.0).min_w(0.0).gap(t.space.at(1.0)).child(head).child(text(if c.is_active() { format!("{} · {count}", capital(&state)) } else { count }).key(format!("{key}-facts")).font(t.ty.sm).color(t.ink_faint()));
        let id = c.id.clone();
        let action = if c.is_active() {
            Button::new(format!("{key}-open"), "Open").variant(ButtonVariant::Secondary).size(Size::Sm).on_press(move |s: &mut App| s.open_cat_page(&id)).build(t)
        } else {
            Button::new(format!("{key}-back"), "Bring back").variant(ButtonVariant::Secondary).size(Size::Sm).on_press(move |s: &mut App| s.bring_back_cat(&id)).build(t)
        };
        let st = match s.cat_state(&c.id) {
            crate::state::CatState::Working(_) if c.is_active() => Some(neko_protocol::TaskStatus::Working),
            crate::state::CatState::NeedsYou if c.is_active() => Some(neko_protocol::TaskStatus::NeedsYou),
            _ => None,
        };
        let face = crate::faces::portrait_in(t, &format!("{key}-face"), &c.face, 40.0, st, t.c.surface.bg);
        let face = if c.is_active() { face } else { face.opacity(0.55) };
        part = part.child(paint(row().key(key.clone()).role("group").label(c.name.clone()).w_full().padding(t.space.at(3.0)).gap(t.space.at(3.0)).center_y().radius(t.radius.xl).round_as(t), &t.mat.slab.base).child(face).child(facts).child(action));
    }
    part
}

fn capital(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

fn workspaces_page(s: &App, t: &Theme) -> El {
    let mut part = col().key("settings-workspaces").w_full().gap(t.space.at(3.0)).child(text("Workspaces").role("heading").level(2).font(weight(t.ty.md, t.ty.semibold)).color(t.ink())).child(
        text("Folders you added. A new task works in the one you choose; in General each task gets a scratch folder of its own. Removing a workspace takes it off the list only: nothing on disk is touched.")
            .font(t.ty.base)
            .color(t.ink_muted())
            .wrap()
            .min_w(0.0),
    );
    for w in s.workspaces.iter().filter(|w| w.is_active()) {
        let key = format!("settings-ws-{}", w.id);
        let n = s.tasks_in(&w.id).len();
        let count = match n {
            0 => "No tasks yet".to_owned(),
            1 => "1 task".to_owned(),
            n => format!("{n} tasks"),
        };
        let folder = match &w.folder {
            Some(f) => crate::task_view::show_path(f, "", s.home.as_deref()),
            None => "No folder: each task gets a scratch folder of its own".to_owned(),
        };
        let mut head = row().w_full().gap(t.space.at(2.0)).center_y().child(text(w.name.clone()).role("heading").level(3).font(weight(t.ty.md, t.ty.medium)).color(t.ink()).truncate().min_w(0.0).shrink());
        if w.missing {
            head = head.child(Badge::new(format!("{key}-missing"), "Folder not found").tone(Tone::Warning).dot().build(t));
        }
        let facts = col()
            .grow(1.0)
            .min_w(0.0)
            .gap(t.space.at(1.0))
            .child(head)
            .child(text(folder).key(format!("{key}-folder")).font(t.ty.sm).color(t.ink_muted()).truncate_middle().min_w(0.0).help(w.folder.clone().unwrap_or_default()))
            .child(text(count).key(format!("{key}-count")).font(t.ty.sm).color(t.ink_faint()));
        let mut card = paint(row().key(key.clone()).role("group").label(w.name.clone()).w_full().padding(t.metrics.inset).gap(t.space.at(3.0)).center_y().radius(t.radius.xl).round_as(t), &t.mat.slab.base).child(facts);
        if !w.is_general() {
            let mut actions = row().gap(t.space.at(1.5)).center_y();
            let id = w.id.clone();
            actions = actions.child(Button::new(format!("{key}-rename"), "Rename…").variant(ButtonVariant::Secondary).size(Size::Sm).on_press(move |s: &mut App| s.start_rename(&id)).build(t));
            if w.missing {
                let id = w.id.clone();
                let locate: Rc<dyn Fn(&mut App, &mut rune::Tx)> = Rc::new(move |s: &mut App, tx: &mut rune::Tx| crate::pick_folder(s, crate::state::Pick::Locate(id.clone()), tx));
                actions = actions.child(Button::<App>::new(format!("{key}-locate"), "Locate…").variant(ButtonVariant::Secondary).size(Size::Sm).build(t).on_press_tx(move |s: &mut App, tx: &mut rune::Tx| locate(s, tx)));
            } else if let Some(f) = w.folder.clone() {
                actions = actions.child(Button::<App>::new(format!("{key}-reveal"), "Reveal in Finder").variant(ButtonVariant::Secondary).size(Size::Sm).build(t).on_press_tx(move |_: &mut App, tx: &mut rune::Tx| {
                    let _ = tx.reveal_path(&f);
                }));
            }
            let id = w.id.clone();
            actions = actions.child(Button::new(format!("{key}-remove"), "Remove…").variant(ButtonVariant::Secondary).size(Size::Sm).on_press(move |s: &mut App| s.ask_remove_workspace(&id)).build(t));
            card = card.child(actions);
        }
        part = part.child(card);
    }
    part = part.child(undo_storage(s, t));
    let add = Button::<App>::new("settings-add-workspace", "Add workspace…").variant(ButtonVariant::Secondary).size(Size::Sm).build(t).on_press_tx(|s: &mut App, tx: &mut rune::Tx| crate::pick_folder(s, crate::state::Pick::Add { select: false }, tx));
    part = part.child(row().child(add));
    let removed: Vec<&str> = s.workspaces.iter().filter(|w| !w.is_active()).map(|w| w.name.as_str()).collect();
    if !removed.is_empty() {
        part = part.child(
            text(format!("Removed: {}. Their tasks stay readable in the sidebar. Add a folder again to bring its workspace back.", removed.join(", ")))
                .key("settings-ws-removed")
                .font(t.ty.sm)
                .color(t.ink_faint())
                .wrap()
                .min_w(0.0),
        );
    }
    part
}

/// "Storage used by undo": the shadow repositories' bytes and the checkpoints kept, with "Clear undo history…" (it asks first).
fn undo_storage(s: &App, t: &Theme) -> El {
    let words = match s.undo_storage {
        None => "Storage used by undo: reading…".to_owned(),
        Some(u) if u.checkpoints == 0 => "Storage used by undo: none. No run has a checkpoint now.".to_owned(),
        Some(u) => format!(
            "Storage used by undo: {} in Neko's folder, {} checkpoint{} in {} task{}. Checkpoints of a git repository are kept in the repository itself.",
            crate::review_view::size_words(u.bytes),
            u.checkpoints,
            if u.checkpoints == 1 { "" } else { "s" },
            u.tasks,
            if u.tasks == 1 { "" } else { "s" }
        ),
    };
    let none = s.undo_storage.is_none_or(|u| u.checkpoints == 0 && u.bytes == 0);
    let clear = Button::new("settings-clear-undo", "Clear undo history…").variant(ButtonVariant::Secondary).size(Size::Sm).disabled(none).on_press(|s: &mut App| s.ask_clear_undo()).build(t);
    col()
        .key("settings-undo")
        .w_full()
        .gap(t.space.at(2.0))
        .pt(t.space.at(2.0))
        .child(text("Undo").role("heading").level(3).font(weight(t.ty.base, t.ty.medium)).color(t.ink()))
        .child(text(words).key("settings-undo-storage").font(t.ty.sm).color(t.ink_muted()).wrap().min_w(0.0))
        .child(row().child(clear))
}

/// What the page says about the system's permission, and the one step to fix it.
pub fn permission_words(a: Allowed) -> (&'static str, Option<&'static str>) {
    match a {
        Allowed::Granted => ("Notifications are on for Neko.", None),
        Allowed::NotAsked | Allowed::Unknown => ("Neko asks the Mac for permission the first time it has something to tell you.", None),
        Allowed::Denied => ("Notifications are off for Neko in System Settings.", Some("Open System Settings, choose Notifications, then Neko, and turn on Allow notifications.")),
        Allowed::Unavailable => ("This copy of Neko cannot send notifications: it is not running as an app.", Some("Open Neko from the Applications folder to get notifications.")),
    }
}

/// The Notifications page: three switches, the system's permission in words, and a test.
fn notifications_page(s: &App, t: &Theme) -> El {
    let sw = s.notifier.switches;
    let mut part = col().key("settings-notifications").w_full().gap(t.space.at(3.0)).child(text("Notifications").role("heading").level(2).font(weight(t.ty.md, t.ty.semibold)).color(t.ink()));
    let allowed = s.allowed();
    let (words, step) = permission_words(allowed);
    let (icon, ink) = match allowed {
        Allowed::Denied | Allowed::Unavailable => (&icons::WARNING, t.c.warning.fg),
        _ => (&icons::INFO, t.ink_faint()),
    };
    let mut said = col().gap(t.space.at(1.0)).min_w(0.0).shrink().child(text(words).font(t.ty.base).color(t.ink_muted()).wrap().min_w(0.0).a11y_hidden());
    if let Some(step) = step {
        said = said.child(text(step).key("settings-notify-step").font(t.ty.base).color(t.ink()).wrap().min_w(0.0).a11y_hidden());
    }
    let label = match step {
        Some(st) => format!("{words} {st}"),
        None => words.to_owned(),
    };
    part = part.child(
        row()
            .key("settings-notify-permission")
            .role("status")
            .label(label)
            .w_full()
            .gap(t.metrics.gap)
            .items_start()
            .child(row().size(t.control.icon, t.ty.base.line_height).center().child(glyph(icon, t.control.icon, ink)))
            .child(said),
    );
    if allowed == Allowed::Denied {
        let open = Button::<App>::new("settings-notify-open", "Open Notification Settings").variant(ButtonVariant::Secondary).size(Size::Sm).build(t).on_press_tx(|_: &mut App, tx: &mut rune::Tx| {
            let _ = tx.open_url_then(SYSTEM_NOTIFICATIONS, |_: &mut App, _, _| {});
        });
        part = part.child(row().child(open));
    }
    let switches = col()
        .key("settings-notify-switches")
        .role("group")
        .label("Notify me")
        .w_full()
        .gap(t.space.at(1.0))
        .child(Switch::<App>::new("notify-needs", "When a task needs you", sw.needs).help("A request to answer, a question, or a failure you can fix.").on_toggle(|s: &mut App| {
            let mut w = s.notifier.switches;
            w.needs = !w.needs;
            s.set_switches(w);
        }).build(t))
        .child(Switch::<App>::new("notify-finished", "When a task finishes", sw.finished).help("“Done”, with the first line of the answer.").on_toggle(|s: &mut App| {
            let mut w = s.notifier.switches;
            w.finished = !w.finished;
            s.set_switches(w);
        }).build(t))
        .child(Switch::<App>::new("notify-badge", "Show the count on the Dock icon", sw.badge).help("How many things need you.").on_toggle(|s: &mut App| {
            let mut w = s.notifier.switches;
            w.badge = !w.badge;
            s.set_switches(w);
        }).build(t));
    part = part.child(switches);
    part = part.child(
        text("Neko tells you only when it is not in front, or you are in another task. It never plays a sound of its own, and Focus decides what shows. The morning briefing has its own page, Briefing.")
            .key("settings-notify-rules")
            .font(t.ty.sm)
            .color(t.ink_faint())
            .wrap()
            .min_w(0.0),
    );
    let test = Button::new("settings-notify-test", "Send a test notification").variant(ButtonVariant::Secondary).size(Size::Sm).disabled(allowed == Allowed::Unavailable).on_press(|s: &mut App| s.send_test()).build(t);
    part.child(row().child(test))
}

/// Settings ▸ Briefing: what the briefing shows, and the morning notification: its switch and its time (docs/knowledge/briefing.md).
fn briefing_page(s: &App, t: &Theme) -> El {
    let mut part = col().key("settings-briefing").w_full().gap(t.space.at(3.0)).child(text("Briefing").role("heading").level(2).font(weight(t.ty.md, t.ty.semibold)).color(t.ink())).child(
        text("What happened while you were away, made from what Neko already keeps. No model writes it, it asks for no key, and it costs nothing.").font(t.ty.base).color(t.ink_muted()).wrap().min_w(0.0),
    );
    let shows = ["What still waits for you, first", "What finished, with the first line of each answer", "What your rounds did", "What is still working", "The totals, with the cost as Claude Code reported it"];
    let mut list = col().key("settings-briefing-shows").role("list").label("What the briefing shows").w_full().gap(t.space.at(1.5)).pl(t.space.at(1.0));
    for line in shows {
        list = list.child(row().role("listitem").label(line).gap(t.space.at(2.5)).center_y().child(glyph(&icons::CHECK, t.control.icon_sm, t.ink_faint())).child(text(line).font(t.ty.base).color(t.ink()).wrap().min_w(0.0).shrink().a11y_hidden()));
    }
    part = part.child(text("What it shows").role("heading").level(3).font(weight(t.ty.base, t.ty.medium)).color(t.ink()).pt(t.space.at(2.0))).child(list);
    let Some(cfg) = &s.briefing_settings else {
        return part.child(text("Waiting for Neko…").key("settings-briefing-waiting").role("status").font(t.ty.base).color(t.ink_faint()));
    };
    let on = cfg.morning_on;
    part = part.child(text("Each morning").role("heading").level(3).font(weight(t.ty.base, t.ty.medium)).color(t.ink()).pt(t.space.at(3.0))).child(
        Switch::<App>::new("briefing-morning-on", "Tell me when my briefing is ready", on).help("One quiet notification, only when something happened. Focus decides what shows.").on_toggle(move |s: &mut App| s.save_briefing_settings(Some(!on))).build(t),
    );
    let typed = s.typed_morning();
    let changed = typed.as_deref().is_some_and(|a| a != cfg.morning_at);
    let invalid = typed.is_none();
    let input = Input::new("briefing-morning-time", &s.briefing_time, |s: &mut App| &mut s.briefing_time)
        .name("Morning time")
        .placeholder("08:00")
        .description("Hours and minutes, 24-hour, like 08:00")
        .width(120.0)
        .invalid(invalid && on)
        .on_submit(|s: &mut App| s.save_briefing_settings(None))
        .build(t);
    let save = Button::<App>::new("briefing-morning-save", "Save").variant(ButtonVariant::Secondary).size(Size::Sm).disabled(!on || !changed).on_press(|s: &mut App| s.save_briefing_settings(None)).build(t);
    part = part.child(row().key("briefing-morning-row").gap(t.space.at(3.0)).center_y().child(text("At").font(t.ty.base).color(if on { t.ink() } else { t.ink_faint() })).child(input).child(save));
    if invalid && on {
        part = part.child(text("Write the time as hours and minutes, like 08:00.").key("briefing-morning-invalid").role("status").font(t.ty.sm).color(t.c.danger.fg).wrap().min_w(0.0));
    }
    part.child(
        text("Neko posts it when it is running and this Mac is awake. If the Mac slept through the time, it posts once when it wakes, if it is still that day. It posts nothing when nothing happened.")
            .key("settings-briefing-rules")
            .font(t.ty.sm)
            .color(t.ink_faint())
            .wrap()
            .min_w(0.0),
    )
}

/// System Settings ▸ Notifications, at Neko's own entry.
pub const SYSTEM_NOTIFICATIONS: &str = "x-apple.systempreferences:com.apple.Notifications-Settings.extension?id=dev.neko.app";

fn wrap(t: &Theme, page: El) -> El {
    col().key("settings-scroll").w_full().grow(1.0).min_h(0.0).items_center().px(t.space.at(8.0)).pt(t.space.at(6.0)).pb(t.space.at(10.0)).scroll(rune::ScrollSpec { stick_bottom: false }).child(page)
}
