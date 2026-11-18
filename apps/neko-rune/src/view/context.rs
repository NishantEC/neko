//! The context panel (docs/decisions/0002, plugin slot (a)): what the selected session produces, one
//! tab at a time. The header is 36 points: a segmented control over only the tabs that have content
//! (Canvas, Diff, Files, Terminal, Plan, Details, in that order), then Expand and Close. The panel opens by the rules
//! in `panel.rs`; this file draws it. The tabs' bodies are in `context_diff.rs`, `context_files.rs`,
//! `context_terminal.rs` and, for the Canvas and every viewer, here.

use rune::{El, col, el, row, text};
use rune_kit::SegmentedControl;
use rune_kit::glyphs::{FILE, FILE_CODE, FILE_IMAGE, FILE_TEXT};

use super::{context_browser, context_diff, context_files, context_info, context_terminal, icon_button, placeholder, quiet_button, ui};
use crate::app::App;
use crate::groups::age;
use crate::icons;
use crate::panel::{Content, Loaded, Tab, Target};
use crate::readers::size_words;
use crate::theme::Neko;
use crate::viewers::{self, ViewerInput, family};
use crate::web_panel::Want;

/// The header's height.
pub const HEAD: f32 = 36.0;

/// Below this panel width the tabs are icons only (Canvas, Diff, Files, Terminal) with a tooltip
/// naming each: four labels in a 36 point header truncated ("Termi…") before this was a rule.
pub const ICONS_BELOW: f32 = 440.0;

/// `width` is the panel's own width (an expanded panel passes the window's).
pub fn panel(s: &App, t: &Neko, layer: &mut Vec<El>, width: f32) -> El {
    let tabs = s.panel_tabs();
    let tab = s.panel_tab();
    let expanded = s.panel.expanded;
    let mut head = row().key("context-head").w_full().h(HEAD).px(8.0).gap(8.0).center_y();
    if tabs.is_empty() {
        head = head.child(el().grow(1.0));
    } else {
        let labels: Vec<&str> = tabs.iter().map(|t| t.title()).collect();
        let selected = tab.and_then(|t| tabs.iter().position(|x| *x == t));
        let picks = tabs.clone();
        let control = if width < ICONS_BELOW {
            icon_tabs(&tabs, selected, t)
        } else {
            SegmentedControl::new(&labels, selected).label("Context").width(None).on_pick(move |s: &mut App, i: usize, _| s.panel_choose_tab(picks[i])).build(&t.kit)
        };
        head = head.child(row().key("context-picker").grow(1.0).min_w(0.0).child(control));
    }
    head = head
        .child(icon_button(if expanded { &icons::COLLAPSE } else { &icons::EXPAND }, if expanded { "Restore the panel" } else { "Expand the panel" }, "context-expand", t, move |s: &mut App| s.panel_expand(!expanded)))
        .child(icon_button(&icons::CLOSE_SMALL, "Close the panel", "context-close", t, |s: &mut App| s.panel_set_open(false)));
    let body = match tab {
        Some(Tab::Canvas) => canvas(s, t, layer),
        Some(Tab::Diff) => context_diff::tab(s, t, layer),
        Some(Tab::Files) => context_files::tab(s, t),
        Some(Tab::Terminal) => context_terminal::tab(s, t),
        Some(Tab::Browser) => context_browser::tab(s, t, layer),
        Some(Tab::Plan) => context_info::plan(s, t),
        Some(Tab::Details) => context_info::details(s, t),
        None => placeholder(&["Nothing to show yet."], t),
    };
    col().key("context").min_w(0.0).my(8.0).mx(2.0).radius(t.radius.window).clip_rounded(t.radius.window).fill(t.surface.card).child(head).child(viewers::web::region(col().key("context-body").w_full().grow(1.0).min_h(0.0).child(body)))
}

/// The icon of a tab.
pub fn tab_icon(tab: Tab) -> &'static rune::Icon {
    match tab {
        Tab::Canvas => &icons::CANVAS,
        Tab::Diff => &icons::DIFF,
        Tab::Files => &icons::FILES,
        Tab::Terminal => &icons::TERMINAL,
        Tab::Browser => &icons::GLOBE,
        Tab::Plan => &icons::CHECKLIST,
        Tab::Details => &icons::INFO,
    }
}

/// The tabs as icons in the kit's segmented look (raised selected segment on a quiet layer); same keys,
/// roles and names as the labelled control so a script and a screen reader find the same things.
fn icon_tabs(tabs: &[Tab], selected: Option<usize>, t: &Neko) -> El {
    let c = &t.kit.c;
    let n = tabs.len();
    let mut control = row().key("context-icon-tabs").role("radiogroup").label("Context").focus_group().h(28.0).center_y().radius(6.0).fill(c.layer_01).box_shadow(rune::Shadow { spread: 0.5, color: c.border_base, ..Default::default() });
    for (i, tab) in tabs.iter().enumerate() {
        let on = selected == Some(i);
        let pick = *tab;
        let mut seg = row()
            .key(format!("segment{i}"))
            .role("radio")
            .label(tab.title())
            .help(tab.title())
            .focusable(true)
            .focus_selected(on)
            .focus_ring(2.0, 2.0, c.border_focus)
            .position_in_set(i + 1, n)
            .w(34.0)
            .h(28.0)
            .center()
            .radius(6.0)
            .on_press(move |s: &mut App| s.panel_choose_tab(pick))
            .child(rune::icon(tab_icon(*tab), 16.0).color(if on { c.text_base } else { c.text_muted }).a11y_hidden());
        if on {
            seg = seg.fill(c.bg_base).box_shadow(rune::Shadow { spread: 0.5, color: c.border_strong, ..Default::default() });
        } else {
            seg = seg.hover(t.surface.hover);
        }
        control = control.child(seg);
    }
    control
}

/// The version select's options, newest first: `v3`, `v2`, `v1`.
pub fn version_options(mut versions: Vec<u32>) -> Vec<String> {
    versions.sort_unstable_by(|a, b| b.cmp(a));
    versions.into_iter().map(|v| format!("v{v}")).collect()
}

/// The icon of a kind: the kit's file glyphs by family.
pub fn kind_icon(kind: &str) -> &'static rune::Icon {
    match family(kind) {
        "image" | "model" => &FILE_IMAGE,
        "text" | "table" | "document" => &FILE_TEXT,
        "page" => &FILE_CODE,
        _ => &FILE,
    }
}

/// The Canvas: the session's artifacts as rows, the chosen one's version select, and its viewer.
fn canvas(s: &App, t: &Neko, layer: &mut Vec<El>) -> El {
    let Some(id) = s.selected.clone() else { return placeholder(&["Nothing to show yet."], t) };
    let Some(sp) = s.panel.session(&id) else { return placeholder(&["Nothing to show yet."], t) };
    let target = s.panel_target();
    let current = target.as_ref().and_then(|t| match t {
        Target::Artifact { id, .. } => Some(id.clone()),
        Target::File { .. } => None,
    });
    let mut list = col().key("canvas-list").w_full().px(8.0).pb(4.0);
    for shown in &sp.shown {
        let record = s.panel.artifacts.get(&shown.id);
        let selected = current.as_deref() == Some(shown.id.as_str());
        let version = sp.versions.get(&shown.id).copied().or_else(|| record.and_then(|a| a.versions.iter().map(|v| v.n).max()));
        let when = record.and_then(|a| a.versions.iter().map(|v| v.at_ms).max()).unwrap_or(shown.at_ms);
        let title = record.map_or_else(|| shown.id.clone(), |a| a.title.clone());
        let kind = record.map(|a| a.kind.clone()).unwrap_or_default();
        let pick = shown.id.clone();
        let mut line = row()
            .key(format!("artifact-{}", shown.id))
            .role("button")
            .label(format!("{title}, {}", version.map_or("loading".to_owned(), |v| format!("version {v}"))))
            .focusable(true)
            .w_full()
            .h(32.0)
            .px(8.0)
            .gap(8.0)
            .center_y()
            .radius(t.radius.row)
            .fill(if selected { t.surface.selected } else { rune::Color::TRANSPARENT })
            .hover(t.surface.hover)
            .focus_ring(2.0, 2.0, t.kit.c.border_focus)
            .on_press(move |s: &mut App| s.panel_pick_artifact(&pick))
            .child(rune::icon(kind_icon(&kind), 16.0).color(t.text.muted).a11y_hidden())
            .child(text(title).font(t.ty.row).color(t.text.base).truncate().grow(1.0).min_w(0.0).a11y_hidden());
        if let Some(v) = version {
            line = line.child(text(format!("v{v}")).font(t.ty.id).color(t.text.faint).a11y_hidden());
        }
        line = line.child(text(age(s.now_ms, when)).font(t.ty.meta).color(t.text.muted).a11y_hidden());
        // Download, Open in browser and Copy link at the right of the row on hover or focus (the web's `artifactMenu`), and in its right-click menu.
        let (dl, br, cp) = (shown.id.clone(), shown.id.clone(), shown.id.clone());
        let link = s.artifact_link(&shown.id);
        let act = |key: String, label: &str, glyph: &'static rune::Icon| row().key(key).role("button").label(label.to_owned()).help(label.to_owned()).focusable(true).size(24.0, 24.0).center().radius(t.radius.control).hover(t.surface.active).focus_ring(2.0, 2.0, t.kit.c.border_focus).child(rune::icon(glyph, 14.0).color(t.text.muted).a11y_hidden());
        let copy = {
            let link = link.clone();
            act(format!("canvas-copy-{}", shown.id), "Copy link", &crate::icons::COPY).on_press_tx(move |s: &mut App, tx: &mut rune::Tx| match &link {
                Some(l) => {
                    tx.copy(l.clone());
                    s.show_toast("Link copied");
                }
                None => s.show_toast("Neko’s web address isn’t known yet."),
            })
        };
        let actions = row()
            .key(format!("canvas-actions-{}", shown.id))
            .gap(2.0)
            .group_opacity(0.0, 1.0)
            .child(act(format!("canvas-download-{}", shown.id), "Download", &crate::icons::ARROW_UP).on_press(move |s: &mut App| s.artifact_download(&dl)))
            .child(act(format!("canvas-browser-{}", shown.id), "Open in browser", &crate::icons::EXTERNAL).on_press(move |s: &mut App| s.artifact_open_in_browser(&br)))
            .child(copy);
        let _ = cp;
        list = list.child(line.child(actions));
    }
    let mut c = col().w_full().grow(1.0).min_h(0.0).child(col().key("canvas-scroll").w_full().max_h(4.0 * 32.0 + 8.0).min_h(0.0).clip_rounded(0.0).scroll(rune::ScrollSpec { stick_bottom: false }).child(list));
    if let Some(Target::Artifact { id, version }) = &target {
        // The chosen artifact's own line: a version select at the right when it has more than one.
        let versions: Vec<u32> = s.panel.artifacts.get(id).map(|a| a.versions.iter().map(|v| v.n).collect()).unwrap_or_default();
        let title = s.panel.artifacts.get(id).map_or_else(|| id.clone(), |a| a.title.clone());
        let mut line = row().key("canvas-head").w_full().h(32.0).px(12.0).gap(8.0).center_y().child(text(title).font(rune::TextStyle { weight: 500.0, ..t.ty.meta }).color(t.text.base).truncate().grow(1.0).min_w(0.0));
        if !versions.is_empty() {
            let options = version_options(versions);
            let owner = id.clone();
            line = line.child(ui::pick(s, t, layer, "context-version", "Version", &options, &format!("v{version}"), 80.0, move |s: &mut App, chosen: &str| {
                if let Ok(n) = chosen.trim_start_matches('v').parse::<u32>() {
                    s.panel_pick_version(&owner, n);
                }
            }));
        }
        c = c.child(line);
    }
    match target {
        Some(target) => c.child(viewer(s, t, &target)),
        None => c.child(placeholder(&["Nothing to show yet."], t)),
    }
}

/// What the open target shows: its viewer, the fallback, or why there is nothing.
pub fn viewer(s: &App, t: &Neko, target: &Target) -> El {
    let content = s.panel.open.as_ref().filter(|o| &o.target == target).map(|o| &o.content);
    match content {
        None | Some(Content::Loading) => placeholder(&["Opening…"], t),
        Some(Content::Failed(message)) => placeholder(&[message.as_str()], t),
        Some(Content::TooLarge { size, path }) => too_large(t, *size, path),
        Some(Content::Ready(loaded)) => match viewers::resolve_for(&loaded.kind, Some(loaded.bytes())) {
            // The web UI draws these (HTML, Excalidraw, a plugin's viewer): its embed page, in the web view.
            // One web view serves the window: while a plugin's page has it, the panel's page waits.
            Some(spec) if viewers::web::is_web(spec.id) && s.plugin_view.is_some() => placeholder(&["Shown when the plugin page is closed."], t),
            Some(spec) if viewers::web::is_web(spec.id) && target.web_route().is_some() => {
                let Some(route) = target.web_route() else { return placeholder(&["Nothing to show yet."], t) };
                s.web_view.set_want(Some(Want { route, dark: t.dark }));
                viewers::web::slot(t, &viewers::web::line(s))
            }
            // A file of the working folder is not an artifact the web UI can show: its text, as code.
            Some(spec) if viewers::web::is_web(spec.id) => match viewers::resolve("txt") {
                Some(code) => {
                    let input = ViewerInput { kind: &loaded.kind, bytes: Some(loaded.shared()), path: Some(&loaded.path), text: loaded.text(), theme_dark: t.dark, title: &loaded.title };
                    let key = target.key();
                    s.panel.states.with(&key, |st| (code.build)(&input, st))
                }
                None => fallback(s, t, loaded, target),
            },
            Some(spec) => {
                let input = ViewerInput { kind: &loaded.kind, bytes: Some(loaded.shared()), path: Some(&loaded.path), text: loaded.text(), theme_dark: t.dark, title: &loaded.title };
                let key = target.key();
                let route = target.route();
                s.panel.states.with(&key, |st| {
                    st.route = route;
                    (spec.build)(&input, st)
                })
            }
            None => fallback(s, t, loaded, target),
        },
    }
}

/// An unknown kind: its name, its size, and ways out (the web UI has viewers this window lacks).
fn fallback(_s: &App, t: &Neko, loaded: &Loaded, target: &Target) -> El {
    let path = loaded.path.to_string_lossy().into_owned();
    let route = target.route();
    col()
        .key("viewer-fallback")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .center()
        .gap(6.0)
        .px(24.0)
        .child(text(loaded.title.clone()).font(rune::TextStyle { weight: 500.0, ..t.ty.row }).color(t.text.base).truncate())
        .child(text(size_words(loaded.size)).font(t.ty.meta).color(t.text.muted))
        .child(
            row()
                .gap(8.0)
                .pt(8.0)
                .child(quiet_button("Reveal in Finder", "fallback-reveal", t, move |s: &mut App| s.reveal(&path)))
                .child(quiet_button("Open in web UI", "fallback-web", t, move |s: &mut App| s.open_in_web(route.as_deref()))),
        )
}

fn too_large(t: &Neko, size: u64, path: &std::path::Path) -> El {
    let path = path.to_string_lossy().into_owned();
    col()
        .key("viewer-too-large")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .center()
        .gap(6.0)
        .px(24.0)
        .child(text("Too large to preview").font(rune::TextStyle { weight: 500.0, ..t.ty.row }).color(t.text.base))
        .child(text(size_words(size)).font(t.ty.meta).color(t.text.muted))
        .child(row().pt(8.0).child(quiet_button("Reveal in Finder", "too-large-reveal", t, move |s: &mut App| s.reveal(&path))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_are_listed_newest_first() {
        assert_eq!(version_options(vec![1, 3, 2]), ["v3", "v2", "v1"]);
        assert!(version_options(vec![]).is_empty());
    }
}
