//! Home: what the middle region shows when no session is chosen, and after "New session" (polish spec §2, §3).
//!
//! A centred column, at most 680 wide, with its middle 40% down from the top: the greeting (the Neko
//! mark at 28 and "Good evening, {first name}" at 34 semibold), a status line whose two parts filter the
//! sessions column, and the composer card 24 below. Behind it, full bleed, the art (`crate::art`, a GPU
//! shader drawn at an effect slot) and a radial scrim that keeps the words at AA contrast.

use rune::{Color, El, Key, KeyEvent, col, el, icon, row, text};
use rune_kit::{Menu, MenuItem, Placement};

use super::card_band;
use crate::account;
use crate::app::App;
use crate::art;
use crate::clock;
use crate::groups::Group;
use crate::icons;
use crate::new_session::{MAX_REQUEST, Mode, NewMenu, RUNTIMES};
use crate::theme::Neko;

/// The column's widest, in points.
pub const COLUMN_MAX: f32 = 680.0;
/// The scrim behind the words: the window's colour at this strength, fading over this radius.
pub const SCRIM_ALPHA: f32 = 0.70;
pub const SCRIM_RADIUS: f32 = 520.0;
/// Where the column's middle sits, as a share of the region's height from the top.
pub const CENTRE: f32 = 0.40;
/// The weight of the space above the column against the space below it, so the column's middle sits 40% down.
const ABOVE: f32 = 3.2;

fn glyph_for(provider: &str) -> &'static rune::Icon {
    match provider {
        "claude" => &icons::CLAUDE_GLYPH,
        "opencode" => &icons::OPENCODE_GLYPH,
        _ => &icons::CODEX_GLYPH,
    }
}

fn runtime_name(provider: &str) -> &'static str {
    RUNTIMES.iter().find(|(id, _)| *id == provider).map_or("Codex", |(_, name)| name)
}

/// What the model part of a choice's label reads ("Claude · Opus 4" is "Opus 4").
fn model_name(label: &str) -> String {
    label.split_once(" · ").map_or(label, |(_, m)| m).to_owned()
}

/// A select's closed face: an optional icon, its words, a muted detail, a chevron. 32 high.
#[allow(clippy::too_many_arguments)]
fn face(t: &Neko, key: &str, label: &str, glyph: Option<&'static rune::Icon>, value: &str, detail: Option<&str>, open: bool, on_press: impl Fn(&mut App) + 'static) -> El {
    let mut r = row()
        .key(key.to_owned())
        .role("button")
        .label(format!("{label}: {value}"))
        .focusable(true)
        .h(32.0)
        .px(8.0)
        .gap(6.0)
        .center_y()
        .radius(t.radius.control)
        .fill(if open { t.surface.active } else { Color::TRANSPARENT })
        .hover(t.surface.hover)
        .focus_ring(2.0, 2.0, t.surface.ring)
        .on_press(on_press);
    if let Some(g) = glyph {
        r = r.child(icon(g, 16.0).color(t.text.muted).a11y_hidden());
    }
    r = r.child(text(value.to_owned()).font(t.ty.row).color(t.text.base).truncate().min_w(0.0).a11y_hidden());
    if let Some(d) = detail {
        r = r.child(text(d.to_owned()).font(t.ty.row).color(t.text.muted).truncate().min_w(0.0).a11y_hidden());
    }
    r.child(icon(&icons::CHEVRON_DOWN, 16.0).color(t.text.muted).a11y_hidden())
}

fn toggle_menu(menu: NewMenu) -> impl Fn(&mut App) + 'static {
    move |s: &mut App| {
        if let Some(f) = s.new_form.as_mut() {
            f.menu = if f.menu == Some(menu) { None } else { Some(menu) };
        }
    }
}

fn close_menu(s: &mut App) {
    if let Some(f) = s.new_form.as_mut() {
        f.menu = None;
    }
}

/// The status line: "3 running" and "3 need you", each its own button (it filters the sessions column).
fn status_line(s: &App, t: &Neko) -> El {
    let (running, needs) = (s.group_count(Group::Running), s.group_count(Group::NeedsYou));
    let mut line = row().key("home-status").gap(16.0).center_y().h(24.0);
    let part = |key: &str, dot: Color, words: String, group: Group, active: bool| {
        row()
            .key(key.to_owned())
            .role("button")
            .label(format!("{words}, show only these"))
            .checked(active)
            .focusable(true)
            .h(24.0)
            .px(6.0)
            .gap(6.0)
            .center_y()
            .radius(t.radius.control)
            .fill(if active { t.surface.active } else { Color::TRANSPARENT })
            .hover(t.surface.hover)
            .focus_ring(2.0, 2.0, t.surface.ring)
            .on_press(move |s: &mut App| s.set_filter(group))
            .child(el().size(6.0, 6.0).fill(dot).a11y_hidden())
            .child(text(words).font(t.ty.row).color(t.text.muted).a11y_hidden())
    };
    if running == 0 && needs == 0 {
        return line.child(text("Nothing running").key("home-status-none").font(t.ty.row).color(t.text.muted));
    }
    if running > 0 {
        line = line.child(part("home-running", t.meaning.iris, format!("{running} running"), Group::Running, s.filter == Some(Group::Running)));
    }
    if needs > 0 {
        line = line.child(part("home-needs", t.meaning.amber, format!("{needs} need you"), Group::NeedsYou, s.filter == Some(Group::NeedsYou)));
    }
    line
}

fn composer(s: &App, t: &Neko, layer: &mut Vec<El>) -> El {
    let Some(f) = s.new_form.as_ref() else { return el() };
    let k = &t.kit;
    let workspaces = s.summary.as_ref().map(|m| m.workspaces.clone()).unwrap_or_default();
    let ws_name = workspaces.get(f.workspace).map_or("No workspace".to_owned(), |w| w.name.clone());
    let branch = workspaces.get(f.workspace).and_then(|w| f.branches.iter().find(|(id, _)| *id == w.id)).map(|(_, b)| b.clone());
    let provider = f.runtime.clone().unwrap_or_else(|| s.default_provider());
    let model_label = f.model.as_ref().and_then(|m| f.choices.iter().find(|c| c.provider == provider && &c.model == m)).map_or("Auto".to_owned(), |c| model_name(&c.label));

    let mut ws_menu = Menu::new("new-workspace-menu")
        .trigger(face(t, "new-workspace", "Workspace", Some(&icons::FOLDER), &ws_name, branch.as_deref(), f.menu == Some(NewMenu::Workspace), toggle_menu(NewMenu::Workspace)))
        .open(f.menu == Some(NewMenu::Workspace))
        .placement(Placement::TopStart)
        .min_width(240.0)
        .on_close(close_menu);
    for (i, w) in workspaces.iter().enumerate() {
        ws_menu = ws_menu.item(MenuItem::new(w.name.clone()).checked(f.workspace == i).on_select(move |s: &mut App| {
            if let Some(f) = s.new_form.as_mut() {
                f.workspace = i;
                f.menu = None;
            }
        }));
    }

    // The runtimes the daemon says are ready, else the three it knows of.
    let ready: Vec<&str> = s.pages.catalog.as_ref().map(|c| c.sources.iter().filter(|x| x.status == neko_protocol::agent_models::SourceStatus::Ready).map(|x| x.provider.as_str()).collect()).unwrap_or_default();
    let mut rt_menu = Menu::new("new-runtime-menu")
        .trigger(face(t, "new-runtime", "Runtime", Some(glyph_for(&provider)), runtime_name(&provider), None, f.menu == Some(NewMenu::Runtime), toggle_menu(NewMenu::Runtime)))
        .open(f.menu == Some(NewMenu::Runtime))
        .placement(Placement::TopStart)
        .min_width(200.0)
        .on_close(close_menu);
    for (id, name) in RUNTIMES.iter().filter(|(id, _)| ready.is_empty() || ready.contains(id) || *id == provider.as_str()) {
        let id = (*id).to_owned();
        rt_menu = rt_menu.item(MenuItem::new(*name).icon(glyph_for(&id)).checked(provider == id).on_select(move |s: &mut App| {
            if let Some(f) = s.new_form.as_mut() {
                // A model belongs to its runtime: changing the runtime goes back to Auto.
                if f.runtime.as_deref() != Some(id.as_str()) {
                    f.model = None;
                }
                f.runtime = Some(id.clone());
                f.menu = None;
            }
        }));
    }

    let mut model_menu = Menu::new("new-model-menu")
        .trigger(face(t, "new-model", "Model", None, &model_label, None, f.menu == Some(NewMenu::Model), toggle_menu(NewMenu::Model)))
        .open(f.menu == Some(NewMenu::Model))
        .placement(Placement::TopEnd)
        .min_width(220.0)
        .on_close(close_menu)
        .item(MenuItem::new("Auto").description("The runtime picks").checked(f.model.is_none()).on_select(|s: &mut App| {
            if let Some(f) = s.new_form.as_mut() {
                f.model = None;
                f.menu = None;
            }
        }));
    for c in f.models_for(&provider) {
        let id = c.model.clone();
        model_menu = model_menu.item(MenuItem::new(model_name(&c.label)).checked(f.model.as_deref() == Some(c.model.as_str())).on_select(move |s: &mut App| {
            if let Some(f) = s.new_form.as_mut() {
                f.model = Some(id.clone());
                f.menu = None;
            }
        }));
    }

    let request = super::composer::field(t, &s.new_request, "new-request-box", "new-request", "Request", &format!("Describe a task for {ws_name}"), 12, |s: &mut App| &mut s.new_request, |s: &mut App, ev: &KeyEvent| {
        if ev.key == Key::Enter && ev.mods.cmd {
            s.submit_new();
            return true;
        }
        false
    });

    let request = super::composer::with_chips(s, t, crate::compose::Dest::New, request);

    // Plan / Build.
    let seg = |label: &'static str, key: &'static str, mode: Mode| {
        super::composer::seg(t, key, label, &format!("{label}: {}", if mode == Mode::Plan { "plan first" } else { "start building" }), if mode == Mode::Plan { "Plan first, then wait for approval" } else { "Start building right away" }, f.mode == mode, move |s: &mut App| {
            if let Some(f) = s.new_form.as_mut() {
                f.mode = mode;
            }
        })
    };
    let segmented = super::composer::segmented(t, "new-mode", "Plan or build", seg("Plan", "new-mode-plan", Mode::Plan), seg("Build", "new-mode-build", Mode::Build));

    let blocker = s.new_blocker();
    let can_send = blocker.is_none() && !f.busy;
    let send = super::composer::send(t, "new-start", "Send", "Send (⌘↩)", can_send, true, |s: &mut App| s.submit_new());
    let attach = super::composer::attach(t, "new-attach", crate::compose::Dest::New);

    let mut card = super::composer::card(t, "home-composer", request, vec![attach, ws_menu.build(k, layer), rt_menu.build(k, layer)], vec![segmented, model_menu.build(k, layer)], send);
    let used = s.new_request.text.len();
    if let Some(e) = &f.error {
        card = card.child(text(e.clone()).key("new-error").font(t.ty.meta).color(t.meaning.amber).wrap().live_region(rune::LiveRegion::Polite));
    } else if let Some(b) = blocker.filter(|_| !s.new_request.text.is_empty() || workspaces.is_empty()) {
        card = card.child(text(b).font(t.ty.meta).color(t.text.muted));
    }
    if used > MAX_REQUEST * 9 / 10 {
        card = card.child(text(format!("{used}/{MAX_REQUEST}")).font(t.ty.id).color(t.text.faint));
    }
    card
}

/// Home, filling the middle region.
pub fn home(s: &App, t: &Neko, layer: &mut Vec<El>) -> El {
    let hour = clock::local(s.now_ms).hour;
    let name = s.account.first_name().to_owned();
    let words = if name.is_empty() { account::greeting(hour).to_owned() } else { format!("{}, {name}", account::greeting(hour)) };
    let greeting = row().key("home-greeting").gap(12.0).center_y().child(icon(&icons::NEKO_MARK, 28.0).color(t.text.base).a11y_hidden()).child(text(words).key("home-title").font(t.ty.greeting).color(t.text.strong).role("heading"));
    let content = col().w_full().max_w(COLUMN_MAX).items_start().gap(24.0).child(col().gap(10.0).child(greeting).child(status_line(s, t))).child(composer(s, t, layer));
    // The art, then the scrim over it, then the column: painted in that order.
    let scrim = el().key("home-scrim").absolute(0.0, 0.0).w_full().h_full().radial_gradient([0.5, CENTRE], SCRIM_RADIUS, [(0.0, t.surface.window.alpha(SCRIM_ALPHA)), (1.0, t.surface.window.alpha(0.0))]).a11y_hidden();
    col()
        .key("home")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .clip_rounded(t.radius.window)
        .child(art::slot(s, t).absolute(0.0, 0.0).w_full().h_full().key("home-art").a11y_hidden())
        .child(scrim)
        .child(card_band(t))
        .child(col().w_full().grow(1.0).min_h(0.0).px(24.0).items_center().child(el().grow(ABOVE)).child(content).child(el().grow(10.0 - ABOVE)))
}
