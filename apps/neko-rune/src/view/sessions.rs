//! The sessions column (polish spec §1), top to bottom on one 4/8 rhythm: the header (the Neko mark,
//! "Neko", the fold button), a search field that opens the ⌘K palette, "New session", the nav rows
//! (Routines, Calendar, Workspaces, each with its count), then the list that scrolls inside the free
//! space: Needs you (a reason icon, the title, the age), Running (a progress ring, the title, "4/7"),
//! Recent (a done icon, the title, the age, "View all"), and Waiting a while (folded). The account
//! footer is pinned at the bottom with the menu that holds Settings, Usage, Plugins, the web UI and Quit.
//!
//! Every row is 32 high with 8 of padding, a 16 point icon box, an 8 gap, radius 8; hover is the
//! foreground at 4%, the open row 7%. Colour appears only in the Needs you and Running icons.

use neko_protocol::workbench::TaskStatus;
use rune::{Color, El, LoopSpec, ScrollSpec, TextStyle, col, el, icon, row, text};
use rune_kit::{Menu, MenuItem, Placement, context_menu};

use crate::app::App;
use crate::groups::{Group, age, grouped_in, shown, status_word};
use crate::icons;
use crate::link::{TaskRow, Wait};
use crate::theme::Neko;
use crate::views::FullView;

/// The outer padding of the column, and the gap between its blocks.
pub const PAD: f32 = 12.0;
pub const GAP: f32 = 8.0;
/// Every row, header rows included.
pub const ROW_H: f32 = 32.0;
/// The footer.
pub const FOOTER_H: f32 = 40.0;
/// Where the header's words start, from the window's left edge: after the traffic lights.
pub const HEADER_INSET: f32 = 80.0;

pub fn column(s: &App, t: &Neko, layer: &mut Vec<El>) -> El {
    let mut c = col().w_full().h_full().min_w(0.0).padding(PAD).gap(GAP);
    c = c.child(header(s, t));
    // The search field and, flush under it, the workspace switcher (a 32 point row): one block, then the 8 point gap.
    c = c.child(col().key("sessions-find").w_full().child(search(t)).child(scope_row(s, t, layer)));
    // The fixed block: New session and the nav rows stay pinned above the scrolling list.
    let mut fixed = col().w_full().gap(0.0).child(new_row(s, t));
    for view in FullView::NAV {
        fixed = fixed.child(nav_row(s, view, t));
    }
    c = c.child(fixed);
    c = c.child(list(s, t, layer));
    c.child(footer(s, t, layer))
}

/// The header: the mark, "Neko", the fold button. Its words start after the traffic lights, and
/// dragging it moves the window.
fn header(_s: &App, t: &Neko) -> El {
    row()
        .key("sessions-header")
        .w_full()
        .h(ROW_H)
        .pl(HEADER_INSET - PAD)
        .gap(8.0)
        .center_y()
        .window_drag()
        .child(icon(&icons::NEKO_MARK, 16.0).color(t.text.base).a11y_hidden())
        .child(text("Neko").key("sessions-brand").font(t.ty.brand).color(t.text.base).role("heading").grow(1.0))
        .child(side_button(&icons::SIDEBAR, "Hide the sessions column", "sidebar-fold", t, |s: &mut App| s.layout.sessions.set_open(false)))
}

/// A 28 point icon button for the headers: the ink muted, the soft fill under the pointer.
pub fn side_button(glyph: &'static rune::Icon, label: &str, key: &str, t: &Neko, on_press: impl Fn(&mut App) + 'static) -> El {
    row().key(key.to_owned()).role("button").label(label).help(label).focusable(true).size(28.0, 28.0).center().radius(t.radius.control).hover(t.surface.hover).focus_ring(2.0, 2.0, t.surface.ring).on_press(on_press).child(icon(glyph, 16.0).color(t.text.muted).a11y_hidden())
}

/// The search field: it looks like a field and opens the ⌘K palette when pressed.
fn search(t: &Neko) -> El {
    row()
        .key("sessions-search")
        .role("button")
        .label("Search sessions")
        .help("Search sessions (⌘K)")
        .focusable(true)
        .w_full()
        .h(ROW_H)
        .px(8.0)
        .gap(8.0)
        .center_y()
        .radius(t.radius.row)
        .border(1.0, t.surface.field_border)
        .hover(t.surface.hover)
        .focus_ring(2.0, 2.0, t.surface.ring)
        .on_press(|s: &mut App| s.palette_toggle())
        .child(icon(&icons::SEARCH, 16.0).color(t.text.muted).a11y_hidden())
        .child(text("Search sessions").font(t.ty.row).color(t.text.faint).truncate().grow(1.0).min_w(0.0).a11y_hidden())
        .child(text("⌘K").font(t.ty.hint).color(t.text.faint).a11y_hidden())
}

/// The workspace switcher: a 32 point row under the search field, a folder, "All workspaces" or one workspace's name, and a chevron.
/// It scopes the list below, Routines, the Calendar's rows of Neko's work and Home's composer; the choice is kept.
fn scope_row(s: &App, t: &Neko, layer: &mut Vec<El>) -> El {
    let name = s.scope_name();
    let trigger = row()
        .key("scope")
        .role("button")
        .label(format!("Workspace: {name}"))
        .help("Show one workspace or all of them")
        .expanded(s.scope_open)
        .focusable(true)
        .w_full()
        .h(ROW_H)
        .px(8.0)
        .gap(8.0)
        .center_y()
        .radius(t.radius.row)
        .fill(if s.scope_open { t.surface.selected } else { Color::TRANSPARENT })
        .hover(t.surface.hover)
        .focus_ring(2.0, 2.0, t.surface.ring)
        .on_press(|s: &mut App| s.scope_open = !s.scope_open)
        .child(icon(&icons::FOLDER, 16.0).color(t.text.muted).a11y_hidden())
        .child(text(name.clone()).key("scope-name").font(t.ty.row).color(t.text.muted).truncate().grow(1.0).min_w(0.0).a11y_hidden())
        .child(icon(&icons::CHEVRON_DOWN, 16.0).color(t.text.muted).a11y_hidden());
    let mut menu = Menu::new("scope-menu").trigger(trigger).open(s.scope_open).placement(Placement::BottomStart).min_width(220.0).on_close(|s: &mut App| s.scope_open = false);
    menu = menu.item(MenuItem::new("All workspaces").icon(&icons::FOLDER).checked(s.scope().is_none()).on_select(|s: &mut App| s.set_scope(None)));
    if let Some(summary) = s.summary.as_ref() {
        for w in &summary.workspaces {
            let id = w.id.clone();
            menu = menu.item(MenuItem::new(w.name.clone()).icon(&icons::FOLDER).checked(s.scope() == Some(w.id.as_str())).on_select(move |s: &mut App| s.set_scope(Some(id.clone()))));
        }
    }
    col().key("scope-row").w_full().child(menu.build(&t.kit, layer))
}

/// The shape every row shares: 32 high, 8 of padding, radius 8, the hover and open fills.
fn base_row(key: String, label: String, open: bool, t: &Neko) -> El {
    row()
        .key(key)
        .role("button")
        .label(label)
        .focusable(true)
        .w_full()
        .h(ROW_H)
        .px(8.0)
        .gap(8.0)
        .center_y()
        .radius(t.radius.row)
        .fill(if open { t.surface.selected } else { Color::TRANSPARENT })
        .hover(t.surface.hover)
        .focus_ring(2.0, 2.0, t.surface.ring)
}

/// "New session": the primary row, a constant 7% fill and an Iris "+".
fn new_row(s: &App, t: &Neko) -> El {
    row()
        .key("new-session")
        .role("button")
        .label("New session")
        .help("Start a session (⌘N)")
        .checked(s.home_shown())
        .focusable(true)
        .w_full()
        .h(ROW_H)
        .px(8.0)
        .gap(8.0)
        .center_y()
        .radius(t.radius.row)
        .fill(t.text.base.alpha(0.07))
        .hover(t.surface.hover)
        .focus_ring(2.0, 2.0, t.surface.ring)
        .on_press(|s: &mut App| s.open_new())
        .child(icon(&icons::PLUS, 16.0).color(t.meaning.iris).a11y_hidden())
        .child(text("New session").font(t.ty.row).color(t.text.base).truncate().grow(1.0).a11y_hidden())
}

/// The count a nav row shows at its right.
fn nav_count(s: &App, view: FullView) -> Option<usize> {
    match view {
        FullView::Routines => s.pages.routines.as_ref().map(|r| r.views.iter().filter(|v| v.routine.enabled && s.scope().is_none_or(|w| v.routine.workspace_id == w)).count()),
        FullView::Calendar => s.pages.calendar.as_ref().filter(|c| c.status.enabled).map(|c| s.events_today(c)),
        // How many setup items need you (broken connections, pending proposals); no count when none do.
        FullView::Setup => Some(super::setup::attention(s)).filter(|n| *n > 0),
        // Pull requests that wait on you: ready to merge, checks failing, a review requested. No count at 0.
        FullView::PullRequests => s.pr_attention(),
        _ => None,
    }
}

fn nav_row(s: &App, view: FullView, t: &Neko) -> El {
    let open = s.view == Some(view);
    let count = nav_count(s, view);
    let mut r = base_row(format!("view-{}", view.web_id()), view.title().to_owned(), open, t)
        .on_press(move |s: &mut App| s.open_view(view))
        .child(icon(view.icon(), 16.0).color(if open { t.text.base } else { t.text.muted }).a11y_hidden())
        .child(text(view.title()).font(t.ty.row).color(if open { t.text.base } else { t.text.muted }).truncate().grow(1.0).a11y_hidden());
    if let Some(n) = count {
        r = r.child(text(n.to_string()).key(format!("count-{}", view.web_id())).font(t.ty.meta).color(t.text.muted).a11y_hidden());
    }
    r
}

/// A section's header row: the title, and at the right the count (or "View all").
fn section_header(group: Group, count: usize, collapsed: bool, t: &Neko) -> El {
    let mut r = row()
        .key(format!("group-{}", group.index()))
        .role("button")
        .label(format!("{}, {count}", group.title()))
        .expanded(!collapsed)
        .focusable(true)
        .w_full()
        .h(ROW_H)
        .px(8.0)
        .gap(8.0)
        .center_y()
        .radius(t.radius.row)
        .hover(t.surface.hover)
        .focus_ring(2.0, 2.0, t.surface.ring)
        .on_press(move |s: &mut App| s.toggle_group(group));
    if group.collapsed_by_default() {
        r = r.child(icon(if collapsed { &icons::CHEVRON_RIGHT } else { &icons::CHEVRON_DOWN }, 16.0).color(t.text.muted).a11y_hidden());
    }
    r = r.child(text(group.title()).font(TextStyle { weight: 500.0, ..t.ty.meta }).color(t.text.muted).truncate().grow(1.0).min_w(0.0).a11y_hidden());
    if group != Group::Recent {
        r = r.child(text(count.to_string()).key(format!("group-count-{}", group.index())).font(t.ty.meta).color(t.text.muted).a11y_hidden());
    }
    r
}

/// "View all", the quiet link at the right of Recent's header (its own button, so it does not fold the group).
fn view_all(t: &Neko) -> El {
    row()
        .key("view-all")
        .role("button")
        .label("View all sessions")
        .focusable(true)
        .h(24.0)
        .px(6.0)
        .center()
        .radius(t.radius.control)
        .hover(t.surface.hover)
        .focus_ring(2.0, 2.0, t.surface.ring)
        .on_press(|s: &mut App| s.open_all_sessions())
        .child(text("View all").font(t.ty.meta).color(t.text.faint).a11y_hidden())
}

/// A quiet row of words that is a button: "Show 4 more".
fn more_row(key: &str, label: String, t: &Neko, on_press: impl Fn(&mut App) + 'static) -> El {
    base_row(key.to_owned(), label.clone(), false, t).on_press(on_press).child(el().size(16.0, 16.0).a11y_hidden()).child(text(label).font(t.ty.meta).color(t.text.muted).truncate().a11y_hidden())
}

fn list(s: &App, t: &Neko, layer: &mut Vec<El>) -> El {
    // The rows are one focus group: Up and Down rove between them, Return opens the one in focus.
    let mut c = col().key("sessions-list").role("list").label("Sessions").focus_group().w_full().gap(GAP);
    match &s.summary {
        None => {
            // Before the first snapshot, and while Neko is not running: say which, calmly.
            c = c.child(text(crate::app::status_line(&s.link, None)).key("sessions-status").font(t.ty.meta).color(t.text.muted).px(8.0).py(6.0).live_region(rune::LiveRegion::Polite));
        }
        Some(summary) => {
            let groups = grouped_in(summary, s.now_ms, s.scope());
            if groups.iter().all(|(_, rows)| rows.is_empty()) {
                c = c.child(text("No sessions yet").key("sessions-empty").font(t.ty.meta).color(t.text.muted).px(8.0).py(6.0));
            }
            if let Some(g) = s.filter {
                c = c.child(
                    row()
                        .key("sessions-filter")
                        .role("button")
                        .label(format!("Showing {} only. Show all.", g.title()))
                        .focusable(true)
                        .w_full()
                        .h(ROW_H)
                        .px(8.0)
                        .gap(8.0)
                        .center_y()
                        .radius(t.radius.row)
                        .fill(t.surface.active)
                        .hover(t.surface.hover)
                        .focus_ring(2.0, 2.0, t.surface.ring)
                        .on_press(move |s: &mut App| s.set_filter(g))
                        .child(text(format!("Only {}", g.title().to_lowercase())).font(t.ty.meta).color(t.text.base).grow(1.0).a11y_hidden())
                        .child(text("Show all").font(t.ty.meta).color(t.text.muted).a11y_hidden()),
                );
            }
            for (group, rows) in groups {
                // A group with nothing in it is not a heading with a zero (DESIGN.md §8 rule 11).
                if rows.is_empty() || s.filter.is_some_and(|f| f != group) {
                    continue;
                }
                let collapsed = s.collapsed[group.index()];
                let mut section = col().key(format!("section-{}", group.index())).w_full();
                let mut head = section_header(group, rows.len(), collapsed, t);
                if group == Group::Recent {
                    head = head.child(view_all(t));
                }
                section = section.child(head);
                if !collapsed {
                    let expanded = group == Group::NeedsYou && s.needs_expanded;
                    let (visible, over) = shown(group, rows.len(), expanded);
                    for task in rows.iter().take(visible) {
                        section = section.child(session_row(s, task, group, t, layer));
                    }
                    if group == Group::NeedsYou && over > 0 {
                        section = section.child(more_row("needs-more", if expanded { "Show fewer".to_owned() } else { format!("Show {over} more") }, t, |s: &mut App| s.needs_expanded = !s.needs_expanded));
                    }
                }
                c = c.child(section);
            }
        }
    }
    // The list scrolls inside its own region: it ends where the footer begins and its last rows fade out
    // under that edge. The header, search, New session and the nav rows stay where they are.
    col().key("sessions-scroll").w_full().grow(1.0).min_h(0.0).clip_rounded(0.0).scroll(ScrollSpec { stick_bottom: false }).scroll_edge_fade(18.0).child(c.pb(8.0))
}

/// The reason icon for a Needs-you row (polish spec §1) and whether it is coral (only a failure is).
fn reason_icon(task: &TaskRow) -> (&'static rune::Icon, bool) {
    if task.status == TaskStatus::Failed {
        return (&icons::CROSS, true);
    }
    let glyph = match task.wait {
        Wait::Plan => &icons::REASON_LIST,
        Wait::Question => &icons::REASON_QUESTION,
        Wait::Review => &icons::REASON_BRANCH,
        Wait::Cap => &icons::GAUGE,
        Wait::Failed => &icons::CROSS,
        Wait::Other | Wait::None => match task.status {
            // A row that says nothing it waits for is read from its state.
            TaskStatus::AwaitingApproval => &icons::REASON_LIST,
            TaskStatus::ReadyForReview => &icons::REASON_BRANCH,
            _ => &icons::DOT,
        },
    };
    (glyph, task.wait == Wait::Failed)
}

/// The 16 point ring of a Running row: a 2 px track at the foreground's 12% and an Iris arc, `done` of
/// `of`; with no fraction a quarter-turn arc that rotates once every 1.2 s (still under Reduce Motion).
pub fn progress_ring(progress: Option<(u32, u32)>, t: &Neko) -> El {
    let track = icon(&icons::RING_TRACK, 16.0).absolute(0.0, 0.0).color(t.text.base.alpha(0.12));
    let arc = match progress {
        Some((done, of)) if of > 0 => icon(icons::ring_arc(done as f32 / of as f32), 16.0).absolute(0.0, 0.0).color(t.meaning.iris),
        _ => icon(&icons::RING_QUARTER, 16.0).absolute(0.0, 0.0).color(t.meaning.iris).looping(LoopSpec::rotation(1.2)),
    };
    el().size(16.0, 16.0).role("progressbar").label(match progress {
        Some((done, of)) => format!("{done} of {of}"),
        None => "Working".to_owned(),
    }).child(track).child(arc)
}

/// One session: icon, title, quiet meta. A press selects it; a right-press opens its menu.
pub(super) fn session_row(s: &App, task: &TaskRow, group: Group, t: &Neko, layer: &mut Vec<El>) -> El {
    let selected = s.selected.as_deref() == Some(task.id.as_str()) && s.view.is_none();
    let unseen = s.unseen.contains(&task.id);
    let id = task.id.clone();
    let press_id = id.clone();
    let when = age(s.now_ms, task.updated_at_ms);
    let meta = match group {
        Group::Running => task.progress.map(|(done, of)| format!("{done}/{of}")),
        _ => Some(when.clone()),
    };
    let mut r = base_row(format!("session-{}", task.id), format!("{}, {}, {when}{}", task.title, status_word(task.status), if unseen { ", not yet seen" } else { "" }), selected, t)
        .help(format!("{} · {when}", crate::status::status_label(task.status)))
        .checked(selected)
        .hover_group()
        .on_press(move |s: &mut App| s.select(&press_id))
        .on_secondary_press(move |s: &mut App, p, _| s.row_menu = Some((id.clone(), p)));
    r = r.child(match group {
        Group::NeedsYou | Group::WaitingAWhile => {
            let (glyph, coral) = reason_icon(task);
            let mut stack = el().size(16.0, 16.0).child(icon(glyph, 16.0).absolute(0.0, 0.0).color(if coral { t.meaning.coral } else { t.text.muted }).a11y_hidden());
            if !coral {
                // Amber only for the icon itself, and only while the row is hovered.
                stack = stack.child(icon(glyph, 16.0).absolute(0.0, 0.0).color(t.meaning.amber).group_opacity(0.0, 1.0).a11y_hidden());
            }
            stack
        }
        Group::Running => progress_ring(task.progress, t),
        Group::Recent => icon(if task.status == TaskStatus::Completed { &icons::CHECK_LINE } else { &icons::CROSS }, 16.0).color(t.text.muted).a11y_hidden(),
    });
    r = r.child(text(task.title.clone()).font(t.ty.row).color(t.text.base).truncate().grow(1.0).min_w(0.0).a11y_hidden());
    if unseen {
        // Iris, 6 points: what changed since you looked (DESIGN.md §8 rule 13).
        r = r.child(el().key(format!("unseen-{}", task.id)).size(6.0, 6.0).radius(3.0).fill(t.meaning.iris).a11y_hidden());
    }
    if let Some(meta) = meta {
        r = r.child(text(meta).key(format!("meta-{}", task.id)).font(t.ty.id).color(t.text.muted).a11y_hidden());
    }

    match s.row_menu.as_ref().filter(|(open, _)| *open == task.id) {
        Some((_, at)) => {
            // The same items as the session's "..." menu (`task_actions.rs`): one list, decided once.
            let menu = context_menu(format!("row-menu-{}", task.id), r, Some(*at)).on_close(|s: &mut App| s.row_menu = None);
            crate::task_actions::fill_menu(menu, s, &task.id).build(&t.kit, layer)
        }
        None => r,
    }
}

/// The account footer: avatar, name and subtitle, and the menu it opens.
fn footer(s: &App, t: &Neko, layer: &mut Vec<El>) -> El {
    let name = if s.account.name.is_empty() { "You".to_owned() } else { s.account.name.clone() };
    let agents = s.summary.as_ref().map_or(0, |m| m.working());
    let runtimes: Vec<String> = s.pages.catalog.as_ref().map(|c| c.sources.iter().filter(|x| x.status == neko_protocol::agent_models::SourceStatus::Ready).map(|x| x.label.clone()).collect()).unwrap_or_default();
    let subtitle = format!("{agents} agent{}{}", if agents == 1 { "" } else { "s" }, if runtimes.is_empty() { String::new() } else { format!(" · {}", runtimes.join(", ")) });
    let avatar = if s.account.has_picture() {
        el().key("account-avatar").size(24.0, 24.0).radius(12.0).pixels(&s.account.picture).a11y_hidden()
    } else {
        row().key("account-avatar").size(24.0, 24.0).radius(12.0).center().fill(t.meaning.iris).a11y_hidden().child(text(s.account.initials()).font(TextStyle { weight: 600.0, ..t.ty.id }).color(Color::WHITE))
    };
    let trigger = row()
        .key("account")
        .role("button")
        .label(format!("{name}, {subtitle}. Menu"))
        .focusable(true)
        .w_full()
        .h(FOOTER_H)
        .px(8.0)
        .gap(8.0)
        .center_y()
        .radius(t.radius.row)
        .fill(if s.more_open { t.surface.selected } else { Color::TRANSPARENT })
        .hover(t.surface.hover)
        .focus_ring(2.0, 2.0, t.surface.ring)
        .on_press(|s: &mut App| s.more_open = !s.more_open)
        .child(avatar)
        .child(col().grow(1.0).min_w(0.0).child(text(name).font(t.ty.row).color(t.text.base).truncate().a11y_hidden()).child(text(subtitle).font(TextStyle { size: 12.0, ..t.ty.meta }).color(t.text.muted).truncate().a11y_hidden()))
        .child(icon(&icons::CHEVRON_UP_DOWN, 16.0).color(t.text.muted).a11y_hidden());
    let mut menu = Menu::new("account-menu").trigger(trigger).open(s.more_open).placement(Placement::TopStart).min_width(220.0).on_close(|s: &mut App| s.more_open = false);
    for view in FullView::MENU {
        menu = menu.item(MenuItem::new(view.title()).icon(view.icon()).checked(s.view == Some(view)).on_select(move |s: &mut App| {
            s.more_open = false;
            s.open_view(view);
        }));
    }
    // The pages plugins registered (plugin slot b), under their own heading.
    // The web's pages first, then the native views (Luau) that installed packs ship.
    let mut pages: Vec<neko_protocol::PluginPage> = s.plugin_pages.clone().unwrap_or_default();
    pages.extend(s.luau.views().iter().filter_map(|p| p.page()));
    if !pages.is_empty() {
        menu = menu.separator().group_label("Plugin pages");
        for page in &pages {
            let open = page.clone();
            menu = menu.item(MenuItem::new(page.title.clone()).icon(&icons::WRENCH).checked(s.plugin_view.as_ref().is_some_and(|p| p.id == page.id)).on_select(move |s: &mut App| {
                s.more_open = false;
                s.open_plugin_page(open.clone());
            }));
        }
    }
    menu = menu.separator().item(MenuItem::new("Stop all work").icon(&icons::CROSS).shortcut("⌘⇧Esc").on_select(|s: &mut App| {
        s.more_open = false;
        s.ask_stop_all();
    }));
    menu = menu.item(MenuItem::new("Open the web UI").icon(&icons::EXTERNAL).on_select(|s: &mut App| {
        s.more_open = false;
        s.open_in_web(None);
    }));
    menu = menu.item(MenuItem::new("Quit").icon(&icons::POWER).on_select_tx(|s: &mut App, tx: &mut rune::Tx| {
        s.more_open = false;
        tx.quit();
    }));
    col().key("sessions-footer").w_full().child(menu.build(&t.kit, layer))
}
