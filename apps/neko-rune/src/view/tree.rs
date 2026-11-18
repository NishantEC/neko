//! The Tree layout of All sessions, as the web draws it (`work/TreeView.tsx`, the layout inside `pages/work/Tasks.tsx`): who started
//! whom. Neko on top, the watches and tasks it answers for below it, and the agent runs under each task; one 36 point row per node
//! (a 28 wide caret, a 24 point icon, the name with the model or origin after it, the tools, the state, then cost and duration in two
//! 64 wide columns), 29 points further in per level with the web's connector lines. The tree itself is `link::tree_model` (a port of
//! `buildTree`), made once per snapshot; this file only draws it. The Rune-only things: the right-click menu on a task's row and J / K.

use rune::{Color, El, Family, Icon, TextStyle, col, el, icon, row, text};
use rune_kit::context_menu;

use super::{ui, webkit};
use crate::app::App;
use crate::icons;
use crate::icons_phosphor as ph;
use crate::link::tree_model::{AgentNode, AgentTree, NodeKind, NodeState, format_cost, format_duration, tree_summary};
use crate::theme::Neko;

/// The web's `STATE_TONE`, as the colour of the state label.
#[derive(Clone, Copy, PartialEq)]
enum Tone {
    Attention,
    Review,
    Running,
    Done,
    Failed,
    Neutral,
}

fn tone_of(state: NodeState) -> Tone {
    match state {
        NodeState::Running => Tone::Running,
        NodeState::NeedsYou | NodeState::ChangesAsked | NodeState::Failing => Tone::Attention,
        NodeState::Review => Tone::Review,
        NodeState::Done | NodeState::Passed | NodeState::Watching => Tone::Done,
        NodeState::Failed => Tone::Failed,
        NodeState::UpNext | NodeState::Stopped | NodeState::Paused => Tone::Neutral,
    }
}

/// `STATE_ICON`.
fn state_icon(state: NodeState) -> &'static Icon {
    match state {
        NodeState::Running | NodeState::Done | NodeState::Passed => &ph::CHECK_BOLD,
        NodeState::UpNext => &ph::CLOCK_BOLD,
        NodeState::NeedsYou => &ph::HAND_PALM_BOLD,
        NodeState::Review | NodeState::Watching => &ph::EYE_BOLD,
        NodeState::ChangesAsked => &ph::ARROW_BEND_UP_LEFT_BOLD,
        NodeState::Stopped | NodeState::Paused => &ph::STOP_BOLD,
        NodeState::Failed | NodeState::Failing => &ph::WARNING_CIRCLE_BOLD,
    }
}

/// `nodeIcon`.
fn node_icon(node: &AgentNode) -> &'static Icon {
    match node.kind {
        NodeKind::Neko => &ph::SPARKLE,
        NodeKind::Watch => &ph::BINOCULARS,
        NodeKind::Task => &ph::LIST_CHECKS,
        NodeKind::Run => match node.name.as_str() {
            "Scout" => &ph::MAP_TRIFOLD,
            "Builder" => &ph::HAMMER,
            "Reviewer" => &ph::SHIELD_CHECK,
            "Recovery" => &ph::LIFEBUOY,
            _ => &ph::CHECK_BOLD,
        },
    }
}

fn mono(t: &Neko, size: f32, lh: f32) -> TextStyle {
    webkit::tnum(TextStyle { family: Family::Mono, size, weight: 400.0, line_height: lh, ..t.ty.row })
}

fn measure(words: &str, style: TextStyle) -> f32 {
    rune::text::measure(words, &[], style, None).0
}

/// The state's colour (`TONE_TEXT`) and its wash (`TONE_BG`).
fn tone_colors(t: &Neko, tone: Tone) -> (Color, Color) {
    let ink = match tone {
        Tone::Attention => t.meaning.amber,
        Tone::Review => t.meaning.sky,
        Tone::Running => t.meaning.iris,
        Tone::Done => t.meaning.mint,
        Tone::Failed => t.meaning.coral,
        Tone::Neutral => t.text.muted,
    };
    (ink, if tone == Tone::Neutral { t.text.base.alpha(0.06) } else { ink.alpha(0.12) })
}

const LABEL: fn(&Neko) -> TextStyle = |t| webkit::font(t, 11.5, 500.0, 11.5);
const TAG: fn(&Neko) -> TextStyle = |t| webkit::font(t, 11.5, 500.0, 18.6875);

/// `StatusLabel`: 20 high, 6 at the sides, the symbol (a spinner while it runs) 4 before the words.
fn status_label(t: &Neko, key: &str, node: &AgentNode) -> El {
    let (ink, wash) = tone_colors(t, tone_of(node.state));
    let glyph = if node.is_live() { icon(icons::ring_arc(0.75), 10.0).color(ink).a11y_hidden() } else { icon(state_icon(node.state), 12.0).color(ink).a11y_hidden() };
    row().key(key.to_owned()).h(20.0).px(6.0).gap(4.0).center_y().radius(10.0).fill(wash).child(glyph).child(text(node.state.title().to_owned()).font(LABEL(t)).color(ink).a11y_hidden())
}

/// `Tag`: 20 high, a neutral wash, at most 160 wide.
fn tag(t: &Neko, words: &str) -> El {
    row().h(20.0).max_w(160.0).px(6.0).center_y().radius(10.0).fill(t.text.base.alpha(0.06)).child(text(words.to_owned()).font(TAG(t)).color(t.text.base.alpha(0.7)).truncate().a11y_hidden())
}

/// What the name column has left on a row: the row's width less what the others take (all measured on the web: the caret 28, the
/// icon 24, 10 between, the tools from 768, the state, and from 640 the two 64 wide columns).
fn name_room(t: &Neko, node: &AgentNode, depth: usize, page_w: f32) -> f32 {
    let mut room = page_w - 56.0 - 29.0 * depth as f32 - 28.0 - 12.0 - 24.0 - 10.0;
    let mut gaps = 0;
    if page_w >= 768.0 {
        let tags: f32 = node.tools.iter().take(2).map(|x| measure(x, TAG(t)).min(148.0) + 12.0).sum::<f32>() + if node.tools.len() > 2 { measure(&format!("+{}", node.tools.len() - 2), TAG(t)) + 12.0 } else { 0.0 };
        let n = node.tools.len().min(3) as f32;
        room -= tags + (n - 1.0).max(0.0) * 6.0;
        gaps += 1;
    }
    room -= measure(node.state.title(), LABEL(t)) + 28.0;
    gaps += 1;
    if page_w >= 640.0 {
        room -= 128.0;
        gaps += 2;
    }
    room - 10.0 * gaps as f32
}

/// A row's width at `depth`: the page less its 28 at each side, and 29 (the rail's 13 and the step's 16) for each level.
fn row_width(depth: usize) -> f32 {
    webkit::page_width() - 56.0 - 29.0 * depth as f32
}

/// The node's line: the name in medium weight with the model, origin or what it does after it, cut together at the end.
fn name_cell(s: &App, t: &Neko, node: &AgentNode, depth: usize) -> El {
    let live = node.is_live();
    let sub = node.origin.as_deref().filter(|_| node.subtitle.is_empty()).unwrap_or(&node.subtitle).to_owned();
    let name_style = webkit::font(t, 13.5, 500.0, 21.9375);
    let small = webkit::font(t, 12.5, 400.0, 20.3125);
    let room = name_room(t, node, depth, webkit::page_width()).max(0.0);
    let name_w = measure(&node.name, name_style);
    let shown_name = name_w.min(room);
    let mut left = room - shown_name;
    let mut cell = row().key(format!("tree-name-{}", node.id)).grow(1.0).min_w(0.0).center_y().child(text(node.name.clone()).key(format!("tree-title-{}", node.id)).font(name_style).color(t.text.strong).truncate().w(shown_name).shrink().a11y_hidden());
    let _ = s;
    let mut push = |cell: El, words: &str, alpha: f32| {
        let w = measure(words, small);
        if left > 8.0 && !words.is_empty() {
            let shown = w.min(left - 8.0);
            left -= shown + 8.0;
            cell.child(text(words.to_owned()).font(small).color(t.text.muted.alpha(alpha)).truncate().w(shown).shrink().ml(8.0).a11y_hidden())
        } else {
            cell
        }
    };
    cell = push(cell, &sub, 1.0);
    if live && !node.work.is_empty() {
        cell = push(cell, &node.work, 0.8);
    }
    cell
}

/// One node's row: the caret (or its 28 point place), then the row itself.
fn node_row(s: &App, t: &Neko, layer: &mut Vec<El>, node: &AgentNode, parent: Option<&AgentNode>, depth: usize, open: bool) -> El {
    let _ = parent;
    let id = node.id.clone();
    let key_id = node.task_id.clone().filter(|_| node.kind == NodeKind::Task).unwrap_or_else(|| node.id.clone());
    let live = node.is_live();
    let dimmed = node.kind == NodeKind::Run && !live;
    let n = node.children.len();
    let lead = if n > 0 {
        let fold_id = id.clone();
        row()
            .key(format!("tree-fold-{key_id}"))
            .role("button")
            .label(if open { format!("Hide {n} below {}", node.name) } else { format!("Show {n} below {}", node.name) })
            .help(if open { "Hide what it started" } else { "Show what it started" })
            .focusable(true)
            .size(28.0, 36.0)
            .center()
            .radius(10.0)
            .hover(t.text.base.alpha(0.06))
            .focus_ring(2.0, 2.0, t.kit.c.border_focus)
            .on_press(move |s: &mut App| s.tree_toggle(&fold_id))
            .child(icon(if open { &ph::CARET_DOWN_BOLD } else { &ph::CARET_RIGHT_BOLD }, 12.0).color(t.text.muted).a11y_hidden())
    } else {
        el().key(format!("tree-lead-{key_id}")).w(28.0).h(36.0)
    };
    let mut words = vec![node.name.clone()];
    if let Some(sub) = Some(node.subtitle.clone()).filter(|x| !x.is_empty()) {
        words.push(sub);
    }
    words.push(node.state.title().to_owned());
    if !node.tools.is_empty() {
        words.push(node.tools.join(", "));
    }
    if let Some(c) = node.cost {
        words.push(format_cost(c.0));
    }
    if let Some(d) = node.duration_ms {
        words.push(format_duration(d));
    }
    if live && !node.work.is_empty() {
        words.push(node.work.clone());
    }
    if n > 0 {
        words.push(format!("{n} below, {}", if open { "shown" } else { "hidden" }));
    }
    let selected = node.task_id.as_deref().is_some_and(|task| node.kind == NodeKind::Task && s.selected.as_deref() == Some(task) && s.view.is_none());
    let cursor_on = node.kind == NodeKind::Task && s.all.cursor_shown && s.nav_ids().get(s.all.cursor).is_some_and(|c| Some(c.as_str()) == node.task_id.as_deref());
    let on = selected || cursor_on;
    let w = webkit::page_width();
    let mut line = row()
        .key(format!("tree-row-{key_id}"))
        .role("treeitem")
        .label(words.join(", "))
        .checked(on)
        .focusable(true)
        .grow(1.0)
        .min_w(0.0)
        .h(36.0)
        .pl(4.0)
        .pr(8.0)
        .gap(10.0)
        .center_y()
        .radius(14.0)
        .fill(if on { t.text.base.alpha(0.07) } else { Color::TRANSPARENT })
        .hover(t.text.base.alpha(0.05))
        .focus_ring(2.0, 2.0, t.kit.c.border_focus)
        .child(row().key(format!("tree-icon-{key_id}")).size(24.0, 24.0).center().radius(10.0).fill(t.text.base.alpha(0.07)).child(icon(node_icon(node), 13.0).color(t.text.base.alpha(0.75)).a11y_hidden()))
        .child(name_cell(s, t, node, depth));
    if w >= 768.0 {
        let mut tags = row().gap(6.0).center_y();
        for x in node.tools.iter().take(2) {
            tags = tags.child(tag(t, x));
        }
        if node.tools.len() > 2 {
            tags = tags.child(tag(t, &format!("+{}", node.tools.len() - 2)));
        }
        line = line.child(tags);
    }
    line = line.child(status_label(t, &format!("tree-state-{key_id}"), node));
    if w >= 640.0 {
        let cost = node.cost.map(|c| format_cost(c.0)).unwrap_or_default();
        let dur = node.duration_ms.map(format_duration).unwrap_or_default();
        line = line
            .child(row().key(format!("tree-cost-{key_id}")).w(64.0).justify_end().child(text(cost).font(mono(t, 11.5, 18.6875)).color(t.text.muted).a11y_hidden()))
            .child(row().key(format!("tree-time-{key_id}")).w(64.0).justify_end().child(text(dur).font(mono(t, 11.5, 18.6875)).color(t.text.muted).a11y_hidden()));
    }
    if dimmed {
        line = line.opacity(0.7);
    }
    let press = node.clone();
    line = line.help(match &node.origin {
        Some(o) => format!("{o}. Enter or click to open."),
        None => "Enter or click to open.".to_owned(),
    });
    let menu_task = node.task_id.clone();
    let line = line
        .on_press(move |s: &mut App| match press.kind {
            NodeKind::Neko => s.open_new(),
            NodeKind::Watch => s.open_setup(crate::view::setup::SetupSection::Watching),
            _ => {
                if let Some(task) = press.task_id.as_deref() {
                    s.select(task);
                }
            }
        })
        .on_secondary_press(move |s: &mut App, p, _| {
            if let Some(task) = menu_task.as_ref() {
                s.row_menu = Some((task.clone(), p));
            }
        });
    let line = match (node.kind, s.row_menu.as_ref().filter(|(open, _)| Some(open.as_str()) == node.task_id.as_deref())) {
        (NodeKind::Task, Some((task, at))) => {
            let mut menu = context_menu(format!("row-menu-{task}"), line, Some(*at)).on_close(|s: &mut App| s.row_menu = None);
            if n > 0 {
                let owner = id.clone();
                menu = menu.item(rune_kit::MenuItem::new(if open { "Hide what it started" } else { "Show what it started" }).on_select(move |s: &mut App| {
                    s.row_menu = None;
                    s.tree_toggle(&owner);
                }));
                menu = menu.separator();
            }
            crate::task_actions::fill_menu(menu, s, task).build(&t.kit, layer)
        }
        _ => line,
    };
    row().key(format!("tree-node-{}", node.id)).w(row_width(depth)).h(36.0).child(lead).child(line)
}

/// A node and what it started: the children hang off a line 13 points in, each on a 16 point step with its elbow 18 down.
fn branch(s: &App, t: &Neko, layer: &mut Vec<El>, node: &AgentNode, parent: Option<&AgentNode>, depth: usize) -> El {
    let open = s.tree_open(node) && !node.children.is_empty();
    let mut c = col().w(row_width(depth)).child(node_row(s, t, layer, node, parent, depth, open));
    if open {
        let line = t.surface.border;
        let mut group = col().w(row_width(depth) - 13.0).ml(13.0);
        let last = node.children.len() - 1;
        for (i, child) in node.children.iter().enumerate() {
            let rail = if i == last { el().pin(Some(0.0), Some(0.0), None, None).size(1.0, 18.0) } else { el().pin(Some(0.0), Some(0.0), None, Some(0.0)).w(1.0) };
            group = group.child(
                col()
                    .w(row_width(depth) - 13.0)
                    .pl(16.0)
                    .child(rail.fill(line))
                    .child(el().pin(Some(0.0), Some(18.0), None, None).size(12.0, 1.0).fill(line))
                    .child(branch(s, t, layer, child, Some(node), depth + 1)),
            );
        }
        c = c.child(group);
    }
    c
}

/// The summary line: "68 tasks · 0 running · $153 reported", and Expand all / Collapse all (the web's ghost `xs` button).
fn summary_line(s: &App, t: &Neko, tree: &AgentTree) -> El {
    let foldable = tree.root.descendants().into_iter().filter(|n| !n.children.is_empty()).count() > 0;
    let all_open = tree.root.descendants().into_iter().filter(|n| !n.children.is_empty()).all(|n| s.tree_open(n));
    let mut line = row().key("tree-summary-line").w_full().h(32.0).pl(28.0).gap(8.0).center_y().mb(8.0).child(
        text(tree_summary(tree)).key("tree-summary").font(mono(t, 12.0, 19.5)).color(t.text.muted).help("What runs reported spending. Codex runs report no cost, so they are not counted.").a11y_hidden(),
    );
    line = line.child(el().grow(1.0).min_w(0.0));
    if foldable {
        let label = if all_open { "Collapse all" } else { "Expand all" };
        line = line.child(
            row()
                .key("tree-fold-all")
                .role("button")
                .label(label)
                .focusable(true)
                .h(32.0)
                .px(10.0)
                .center()
                .radius(16.0)
                .hover(t.text.base.alpha(0.06))
                .focus_ring(2.0, 2.0, t.kit.c.border_focus)
                .on_press(move |s: &mut App| s.tree_set_all(!all_open))
                .child(text(label).font(webkit::font(t, 12.0, 500.0, 16.0)).color(t.text.muted).a11y_hidden()),
        );
    }
    line
}

/// The tree: its summary line, then Neko and everything under it; an empty one says why, as the Tasks page does.
pub fn body(s: &App, t: &Neko, layer: &mut Vec<El>) -> El {
    let Some(summary) = s.summary.as_ref() else { return ui::quiet_line(t, "tree-none", "Neko isn't running yet.") };
    let tree = s.agent_tree();
    if summary.tasks.is_empty() {
        return webkit::empty(t, "tree-empty", &ph::LIST_CHECKS, "No tasks yet", "Describe what you want done in Home and Neko files it here.", Some(("tree-new-task", "New task", Box::new(|s: &mut App| s.open_new()))));
    }
    if tree.root.children.is_empty() {
        let search = s.all.search.text.trim();
        let filtered = !search.is_empty() || s.all.stage.is_some() || s.all.workspace.is_some() || !s.all.include_stopped;
        let title = if search.is_empty() { "No tasks here".to_owned() } else { format!("No tasks match “{}”", s.all.search.text) };
        let button: Option<webkit::EmptyButton> = filtered.then(|| ("tree-clear-filters", "Clear filters", Box::new(|s: &mut App| s.clear_all_filters()) as Box<dyn Fn(&mut App)>));
        return webkit::empty(t, "tree-empty", &ph::MAGNIFYING_GLASS, &title, if filtered { "Nothing fits the filters you have on." } else { "Nothing to show." }, button);
    }
    let list = col().key("tree").role("tree").label("Who started whom").focus_group().w_full().px(28.0).pt(12.0).pb(48.0).child(summary_line(s, t, &tree)).child(branch(s, t, layer, &tree.root, None, 0));
    col().key("tree-scroll").w_full().grow(1.0).min_h(0.0).scroll(rune::ScrollSpec { stick_bottom: false }).child(list)
}
