//! Setup, Skills: instruction files found in a workspace's folders. A skill gives an agent instructions and
//! never a tool; enabling pins the exact text you reviewed, so a changed file has to be enabled again. Search,
//! enable and lock, the curator's switch, a repository preview, and proposals (a new skill, or a diff to one
//! that exists) with Apply and Reject. Ported from `pages/setup/tools/Skills.tsx`.

use neko_protocol::skills::SkillProposal;
use rune::{El, ScrollSpec, col, icon, row, text};

use super::Page;
use super::kit::{self, action, details_block, empty, glyph, list_row};
use super::words::{self, DiffLine};
use crate::app::App;
use crate::icons;
use crate::link::SetupCall;
use crate::status::StatusKind;
use crate::theme::Neko;
use crate::view::ui::{MenuEntry, note, pick, switch, with_menu};
use crate::view::{accent_button, primary_button, quiet_button};

pub fn view(s: &App, t: &Neko, layer: &mut Vec<El>) -> Page {
    let Some(data) = s.setup_data() else { return Page::quiet(t, "skills-none", "Looking for your skills…") };
    let workspace = s.skills_workspace();
    if workspace.is_empty() {
        let body = empty(t, "skills-no-workspace", "Skills are read from a workspace’s folders. Add a workspace to see its skills.", Some(("Open Workspaces", Box::new(|s: &mut App| s.open_setup(super::state::SetupSection::Workspaces)))));
        return Page { count: None, trailing: Vec::new(), body, title: None, back: None };
    }
    let skills = &data.skills;
    let query = s.pages.text("skills:search").trim().to_lowercase();
    let visible: Vec<_> = skills.available.iter().filter(|k| (k.workspace_id.as_deref().unwrap_or("").is_empty() || k.workspace_id.as_deref() == Some(workspace.as_str())) && (query.is_empty() || format!("{} {}", k.name, k.description).to_lowercase().contains(&query))).collect();
    let unavailable: Vec<_> = skills.enabled.iter().filter(|r| r.workspace_id == workspace && !skills.available.iter().any(|a| a.path == r.path && (a.workspace_id.as_deref().unwrap_or("").is_empty() || a.workspace_id.as_deref() == Some(workspace.as_str())))).collect();
    let proposals: Vec<&SkillProposal> = skills.proposals.iter().filter(|p| p.workspace_id == workspace).collect();

    let mut trailing = Vec::new();
    if data.workspaces.len() > 1 {
        let names: Vec<String> = data.workspaces.iter().map(|w| w.name.clone()).collect();
        let by: Vec<(String, String)> = data.workspaces.iter().map(|w| (w.name.clone(), w.id.clone())).collect();
        trailing.push(pick(s, t, layer, "skills-workspace", "Workspace for skills", &names, &data.workspace_name(&workspace), 180.0, move |s: &mut App, chosen: &str| {
            if let Some((_, id)) = by.iter().find(|(n, _)| n == chosen) {
                s.setup.skills_workspace = Some(id.clone());
            }
        }));
    }
    trailing.push(quiet_button("Refresh", "skills-refresh", t, |s: &mut App| s.setup_call(SetupCall::SkillsRefresh)));

    let mut body = col().w_full().gap(2.0).child(kit::text_field(s, t, "skills:search", "Search skills", "Search skills", false, false, |_| {}));
    if let Some(n) = note(s, t, "setup:page") {
        body = body.child(n);
    }
    for r in unavailable {
        body = body.child(unavailable_row(t, r));
    }
    if visible.is_empty() {
        let line = if query.is_empty() { "No local skills available yet. Refresh local skills, or fetch an instruction file below.".to_owned() } else { format!("No skills match “{}”", s.pages.text("skills:search").trim()) };
        body = body.child(empty(t, "skills-empty", &line, query.is_empty().then(|| ("Refresh", Box::new(|s: &mut App| s.setup_call(SetupCall::SkillsRefresh)) as Box<dyn Fn(&mut App)>))));
    }
    for k in visible {
        body = body.child(skill_row(s, t, layer, &workspace, k));
    }
    body = body.child(find_a_skill(s, t));
    body = body.child(curator(s, t));
    if !proposals.is_empty() {
        body = body.child(kit::heading(t, "skills-proposals-head", "Proposals", Some(proposals.len()), false));
        for p in proposals {
            body = body.child(proposal(s, t, layer, p));
        }
    }
    Page { count: (!skills.available.is_empty()).then_some(skills.available.len()), trailing, body, title: None, back: None }
}

fn unavailable_row(t: &Neko, r: &neko_protocol::skills::EnabledSkill) -> El {
    let (ws, path, hash) = (r.workspace_id.clone(), r.path.clone(), r.content_hash.clone());
    list_row(t, format!("skill-gone-{}", r.path), format!("Unavailable enabled skill, {}", r.path), None)
        .child(glyph(t, StatusKind::Failed, "Unavailable"))
        .child(text("Unavailable enabled skill".to_owned()).font(t.ty.row).color(t.text.base).a11y_hidden())
        .child(text(r.path.clone()).font(t.ty.mono_meta).color(t.text.muted).truncate().grow(1.0).min_w(0.0).a11y_hidden())
        .child(quiet_button("Disable", &format!("skill-gone-disable-{}", r.path), t, move |s: &mut App| s.setup_call(SetupCall::SkillEnable { workspace_id: ws.clone(), path: path.clone(), content_hash: hash.clone(), enabled: false })))
}

fn skill_row(s: &App, t: &Neko, layer: &mut Vec<El>, workspace: &str, k: &neko_protocol::skills::Skill) -> El {
    let data = s.setup_data();
    let skills = data.map(|d| &d.skills);
    let record = skills.and_then(|sk| sk.enabled.iter().find(|e| e.workspace_id == workspace && e.path == k.path));
    let changed = record.is_some_and(|r| r.content_hash != k.content_hash);
    let state = skills.and_then(|sk| sk.curator.skills.iter().find(|c| c.path == k.path)).map(|c| c.state.clone());
    let locked = matches!(state.as_deref(), Some("locked" | "edited"));
    let (kind, word) = if record.is_none() { (StatusKind::Backlog, "Available") } else if changed { (StatusKind::InProgress, "Changed") } else { (StatusKind::Done, "Enabled") };
    let key = format!("skill:{}", k.path);
    let open = s.row_open(&key);
    let toggle = key.clone();
    let (sw_ws, sw_path, sw_hash, sw_rec) = (workspace.to_owned(), k.path.clone(), k.content_hash.clone(), record.cloned());
    let mut line = list_row(t, format!("skill-{}", k.name), format!("{}, {word}", k.name), Some(open))
        .on_press(move |s: &mut App| s.toggle_row(&toggle))
        .child(glyph(t, kind, word))
        .child(text(k.name.clone()).font(t.ty.row).color(t.text.base).truncate().min_w(0.0).max_w(240.0).a11y_hidden())
        .child(text(k.description.clone()).font(t.ty.meta).color(t.text.muted).truncate().grow(1.0).min_w(0.0).a11y_hidden())
        .child(text(word.to_owned()).key(format!("skill-state-{}", k.name)).font(t.ty.meta).color(if changed { t.meaning.amber } else { t.text.faint }).a11y_hidden());
    if state.is_some() {
        let p = k.path.clone();
        line = line.child(action(t, format!("skill-lock-{}", k.name), if locked { "Unlock for the curator" } else { "Lock against the curator" }, &icons::LOCK, move |s: &mut App| s.setup_call(SetupCall::SkillLock { path: p.clone(), locked: !locked })));
    }
    line = line.child(switch(t, &format!("skill-switch-{}", k.name), &format!("{} {}", if record.is_some() { "Disable" } else { "Enable" }, k.name), record.is_some(), move |s: &mut App| match &sw_rec {
        Some(r) => s.setup_call(SetupCall::SkillEnable { workspace_id: r.workspace_id.clone(), path: r.path.clone(), content_hash: r.content_hash.clone(), enabled: false }),
        None => s.setup_call(SetupCall::SkillEnable { workspace_id: sw_ws.clone(), path: sw_path.clone(), content_hash: sw_hash.clone(), enabled: true }),
    }));
    let mut entries: Vec<MenuEntry> = Vec::new();
    {
        let (w, p, h) = (workspace.to_owned(), k.path.clone(), k.content_hash.clone());
        let rec = record.cloned();
        entries.push((if rec.is_some() { "Disable in this workspace" } else { "Enable current instructions" }.to_owned(), std::rc::Rc::new(move |s: &mut App| match &rec {
            Some(r) => s.setup_call(SetupCall::SkillEnable { workspace_id: r.workspace_id.clone(), path: r.path.clone(), content_hash: r.content_hash.clone(), enabled: false }),
            None => s.setup_call(SetupCall::SkillEnable { workspace_id: w.clone(), path: p.clone(), content_hash: h.clone(), enabled: true }),
        })));
    }
    if state.is_some() {
        let p = k.path.clone();
        entries.push((if locked { "Unlock for the curator" } else { "Lock against the curator" }.to_owned(), std::rc::Rc::new(move |s: &mut App| s.setup_call(SetupCall::SkillLock { path: p.clone(), locked: !locked }))));
    }
    let mut out = col().key(format!("skill-block-{}", k.name)).w_full().child(with_menu(s, t, layer, &format!("skill:{}", k.name), line, entries));
    if let Some(n) = note(s, t, &format!("setup:{}", k.path)) {
        out = out.child(n);
    }
    if open {
        let mut d = details_block(format!("skill-details-{}", k.name));
        d = d.child(text(k.path.clone()).font(t.ty.mono_meta).color(t.text.muted).wrap().px(8.0).min_w(0.0));
        if changed {
            d = d.child(kit::amber(t, &format!("skill-changed-{}", k.name), "Instructions changed. Review them before enabling the updated version."));
        }
        if record.is_none() || changed {
            let (w, p, h) = (workspace.to_owned(), k.path.clone(), k.content_hash.clone());
            d = d.child(row().px(8.0).child(primary_button(if changed { "Enable reviewed changes" } else { "Enable current instructions" }, &format!("skill-enable-{}", k.name), t, move |s: &mut App| s.setup_call(SetupCall::SkillEnable { workspace_id: w.clone(), path: p.clone(), content_hash: h.clone(), enabled: true }))));
        }
        if let Some(c) = state.as_deref() {
            d = d.child(kit::words(t, &format!("skill-curated-{}", k.name), words::curator_words(c)));
        }
        d = d.child(kit::words(t, &format!("skill-open-{}", k.name), "Open the file in your editor to read it before enabling: this page cannot open local files."));
        out = out.child(d);
    }
    out
}

/// "Find a skill": a GitHub SKILL.md URL, fetched for review (only the instruction file; scripts are not installed).
fn find_a_skill(s: &App, t: &Neko) -> El {
    let open = s.row_open("skills:find");
    let mut c = col().key("skills-find").w_full().gap(2.0).pt(8.0);
    c = c.child(list_row(t, "skills-find-toggle".to_owned(), "Find a skill".to_owned(), Some(open)).on_press(|s: &mut App| s.toggle_row("skills:find")).child(icon(&icons::PLUS, 16.0).color(t.text.muted).a11y_hidden()).child(text("Find a skill".to_owned()).font(t.ty.row).color(t.text.muted).a11y_hidden()));
    if open {
        let mut d = details_block("skills-find-body".to_owned()).pl(8.0);
        d = d.child(quiet_button("Browse skills.sh", "skills-browse", t, |s: &mut App| s.open_link("https://skills.sh")));
        d = d.child(kit::field(t, "skill-url", "GitHub SKILL.md URL", Some("Only the instruction file is previewed. Scripts and references are not installed."), kit::text_field(s, t, "skill:url", "GitHub SKILL.md URL", "https://github.com/…/SKILL.md", true, false, |s: &mut App| s.preview_skill())));
        let ready = !s.pages.text("skill:url").trim().is_empty();
        d = d.child(row().child(if ready { quiet_button("Fetch for review", "skills-fetch", t, |s: &mut App| s.preview_skill()) } else { quiet_button("Fetch for review", "skills-fetch", t, |_| {}).disabled(true) }));
        c = c.child(d);
    }
    c
}

/// The skill curator: a daily pass that tidies what skills gathered; its switch, when it last ran, what it applied.
fn curator(s: &App, t: &Neko) -> El {
    let Some(data) = s.setup_data() else { return col() };
    let cur = &data.skills.curator;
    let on = !cur.off;
    let last = if cur.off { "Off.".to_owned() } else if cur.last_run_ms > 0 { format!("Last pass {}.", words::relative_time(Some(cur.last_run_ms), s.now_ms)) } else { "Has not run yet.".to_owned() };
    let help = "Once a day, when the Mac is idle, tidies the lessons a skill has gathered: merges duplicates and drops what a newer lesson contradicts. It applies by itself only to skills Neko wrote and you have not edited; everything else is a proposal. Nothing is deleted: removed text moves to an Archive section.";
    let mut c = col().key("skill-curator").w_full().gap(2.0).child(kit::heading(t, "skill-curator-head", "Skill curator", None, false));
    c = c.child(
        row()
            .key("skill-curator-row")
            .w_full()
            .min_h(kit::ROW_H)
            .px(8.0)
            .gap(10.0)
            .center_y()
            .radius(t.radius.row)
            .hover(t.surface.hover)
            .help(help)
            .child(text("Tidy skills once a day".to_owned()).font(t.ty.row).color(t.text.base).a11y_hidden())
            .child(text(last).key("skill-curator-last").font(t.ty.meta).color(t.text.muted).grow(1.0).min_w(0.0).a11y_hidden())
            .child(switch(t, "skill-curator-switch", "Skill curator", on, move |s: &mut App| s.setup_call(SetupCall::SkillCurator { on: !on }))),
    );
    for r in cur.recent.iter().take(3) {
        let key = format!("applied:{}:{}", r.path, r.at_ms);
        let open = s.row_open(&key);
        let toggle = key.clone();
        c = c.child(list_row(t, format!("skill-applied-{}", r.at_ms), format!("Tidied {}", r.name), Some(open)).on_press(move |s: &mut App| s.toggle_row(&toggle)).child(text(format!("Tidied {}", r.name)).font(t.ty.row).color(t.text.base).truncate().grow(1.0).min_w(0.0).a11y_hidden()).child(text(words::relative_time(Some(r.at_ms), s.now_ms)).font(t.ty.meta).color(t.text.faint).a11y_hidden()));
        if open {
            c = c.child(code(t, &format!("skill-applied-diff-{}", r.at_ms), &r.diff, true));
        }
    }
    c
}

/// A block of mono text; a diff colours its added and removed lines. Scrolls inside 300 points.
fn code(t: &Neko, key: &str, body: &str, diff: bool) -> El {
    let mut lines = col().w_full().gap(0.0).px(10.0).py(8.0);
    for (i, line) in body.lines().take(400).enumerate() {
        let ink = if diff {
            match words::diff_line(line) {
                DiffLine::Added => t.meaning.mint,
                DiffLine::Removed => t.meaning.coral,
                DiffLine::Hunk => t.meaning.sky,
                DiffLine::Context => t.text.muted,
            }
        } else {
            t.text.base
        };
        lines = lines.child(text(if line.is_empty() { " ".to_owned() } else { line.to_owned() }).key(format!("{key}-{i}")).font(t.ty.mono_meta).color(ink).wrap().min_w(0.0));
    }
    col().key(key.to_owned()).w_full().max_h(300.0).radius(t.radius.row).fill(t.surface.hover).scroll(ScrollSpec { stick_bottom: false }).child(lines)
}

fn proposal(s: &App, t: &Neko, layer: &mut Vec<El>, p: &SkillProposal) -> El {
    let key = format!("proposal:{}", p.id);
    let open = s.row_open(&key);
    let toggle = key.clone();
    let audit = p.audit_url.clone().unwrap_or_default();
    let reviewed = audit.is_empty() || p.audit_reviewed_hash.as_deref() == Some(p.content_hash.as_str());
    let (apply_id, apply_hash, reject_id, reject_hash) = (p.id.clone(), p.content_hash.clone(), p.id.clone(), p.content_hash.clone());
    let improves = p.patch.is_some();
    let line = list_row(t, format!("proposal-{}", p.id), format!("{}, {}", p.name, if improves { "suggested change" } else { "suggested skill" }), Some(open))
        .on_press(move |s: &mut App| s.toggle_row(&toggle))
        .child(glyph(t, StatusKind::InProgress, "Needs you"))
        .child(text(p.name.clone()).font(t.ty.row).color(t.text.base).truncate().min_w(0.0).max_w(260.0).a11y_hidden())
        .child(text(if improves { "Suggested change · review the diff" } else { "Suggested skill · review instructions" }.to_owned()).font(t.ty.meta).color(t.text.muted).truncate().grow(1.0).min_w(0.0).a11y_hidden())
        .child(action(t, format!("proposal-apply-{}", p.id), "Apply", &icons::CHECK_LINE, {
            let (i, h) = (apply_id.clone(), apply_hash.clone());
            move |s: &mut App| s.setup_call(SetupCall::SkillDecide { id: i.clone(), content_hash: h.clone(), accept: true })
        }))
        .child(action(t, format!("proposal-reject-{}", p.id), "Reject", &icons::CLOSE_SMALL, {
            let (i, h) = (reject_id.clone(), reject_hash.clone());
            move |s: &mut App| s.setup_call(SetupCall::SkillDecide { id: i.clone(), content_hash: h.clone(), accept: false })
        }));
    let entries: Vec<MenuEntry> = vec![
        ("Apply".to_owned(), std::rc::Rc::new({
            let (i, h) = (apply_id.clone(), apply_hash.clone());
            move |s: &mut App| s.setup_call(SetupCall::SkillDecide { id: i.clone(), content_hash: h.clone(), accept: true })
        })),
        ("Reject".to_owned(), std::rc::Rc::new({
            let (i, h) = (reject_id.clone(), reject_hash.clone());
            move |s: &mut App| s.setup_call(SetupCall::SkillDecide { id: i.clone(), content_hash: h.clone(), accept: false })
        })),
    ];
    let mut out = col().key(format!("proposal-block-{}", p.id)).w_full().child(with_menu(s, t, layer, &format!("proposal:{}", p.id), line, entries));
    if let Some(n) = note(s, t, &format!("setup:{}", p.id)) {
        out = out.child(n);
    }
    if !open {
        return out;
    }
    let mut d = details_block(format!("proposal-details-{}", p.id));
    d = d.child(kit::words(t, &format!("proposal-source-{}", p.id), &p.source));
    if !p.audit_status.is_empty() {
        d = d.child(kit::words(t, &format!("proposal-audit-status-{}", p.id), &p.audit_status));
    }
    if !audit.is_empty() {
        let opened = s.setup.audit_opened.contains(&p.id);
        let (open_id, open_url) = (p.id.clone(), audit.clone());
        d = d.child(row().px(8.0).child(quiet_button("View published source and audit links", &format!("proposal-view-audit-{}", p.id), t, move |s: &mut App| s.open_audit(&open_id, &open_url))));
        d = d.child(kit::words(t, &format!("proposal-audit-note-{}", p.id), "Read the published audit results. If none are available, do not confirm or install. Confirmation records your review, not a passing audit."));
        let (cid, chash, curl) = (p.id.clone(), p.content_hash.clone(), audit.clone());
        let confirm = if opened && !reviewed {
            accent_button("I reviewed the linked published audit results", &format!("proposal-confirm-{}", p.id), t, move |s: &mut App| s.setup_call(SetupCall::SkillAudit { id: cid.clone(), content_hash: chash.clone(), audit_url: curl.clone() }))
        } else {
            accent_button(if reviewed { "Audit review recorded" } else { "I reviewed the linked published audit results" }, &format!("proposal-confirm-{}", p.id), t, |_| {}).disabled(true)
        };
        d = d.child(row().px(8.0).child(confirm));
    }
    d = d.child(code(t, &format!("proposal-body-{}", p.id), &p.body, improves));
    d = d.child(kit::words(t, &format!("proposal-note-{}", p.id), if improves { "Apply rewrites that skill with these changes." } else { "Apply saves these exact instructions. Enable separately after installation." }));
    let (a_id, a_hash, r_id, r_hash) = (p.id.clone(), p.content_hash.clone(), p.id.clone(), p.content_hash.clone());
    let apply = if reviewed { primary_button("Apply", &format!("proposal-apply-btn-{}", p.id), t, move |s: &mut App| s.setup_call(SetupCall::SkillDecide { id: a_id.clone(), content_hash: a_hash.clone(), accept: true })) } else { primary_button("Apply", &format!("proposal-apply-btn-{}", p.id), t, |_| {}).disabled(true).fill(t.meaning.iris.alpha(0.4)) };
    d = d.child(row().px(8.0).gap(8.0).child(apply).child(quiet_button("Reject", &format!("proposal-reject-btn-{}", p.id), t, move |s: &mut App| s.setup_call(SetupCall::SkillDecide { id: r_id.clone(), content_hash: r_hash.clone(), accept: false }))));
    out.child(d)
}
