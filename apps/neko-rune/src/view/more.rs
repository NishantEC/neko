//! The pages under "More": Usage (today's spend, the last week as a line, each vendor's quota as a
//! thin bar), Workspaces (one line per scope that opens to its folders and servers) and Plugins (the
//! installed list with how each last loaded). Charts are drawn with Rune's shapes, no chart crate.
//! Ported from `pages/setup/Usage.tsx`, `pages/setup/Workspaces.tsx` and the plugin status the web
//! reports to the daemon; the same requests, no writes.

use rune::{Color, El, Path, Shape, canvas, col, el, icon, row, text};

use neko_protocol::workbench::MachineHealth;

use super::ui::page;
use super::webkit;
use crate::app::App;
use crate::clock;
use crate::icons_phosphor;
use crate::theme::Neko;

const DAY_MS: i64 = 86_400_000;
/// The chart's height (`h-44`): 8 above the plot, the plot 138 high, then the axis labels.
const CHART_H: f32 = 176.0;

/// JavaScript's `toFixed`: the same digits, except that an exact tie (0.125 to two places) rounds up, where Rust rounds to even.
fn fixed(n: f64, places: i32) -> String {
    let scale = 10f64.powi(places);
    let scaled = n * scale;
    if scaled.fract() == 0.5 { format!("{:.*}", places as usize, (scaled.floor() + 1.0) / scale) } else { format!("{:.*}", places as usize, n) }
}

fn usd(n: f64) -> String {
    if n >= 100.0 { format!("${}", fixed(n, 0)) } else { format!("${}", fixed(n, 2)) }
}

/// A quota's value colour (`Usage.tsx`): quiet, amber from 70% used, coral from 90%; colour only beside words that say why.
pub fn quota_ink(t: &Neko, used: f32) -> Option<Color> {
    if used >= 0.9 {
        Some(t.meaning.coral)
    } else if used >= 0.7 {
        Some(t.meaning.amber)
    } else {
        None
    }
}

/// The Usage page (`Usage.tsx`, "Usage & machine"): This Mac, Spend, Models and Quota, each a `Section` of the web's figures.
pub fn usage(s: &App, t: &Neko, region_w: f32) -> El {
    webkit::set_page_width(region_w);
    // The column's inside: the window up to the page's 880, less the 28 of padding at either side.
    let inner = region_w.min(super::COLUMN) - 56.0;
    let body = col().key("usage-body").w_full().gap(28.0).child(this_mac(s, t)).child(spend(s, t, inner)).child(models(s, t)).child(quota(s, t));
    page(t, "usage", "Usage & machine", None, Vec::new(), body)
}

fn this_mac(s: &App, t: &Neko) -> El {
    let fetched = s.extra.machine_fetched;
    let updated = if fetched > 0 { format!("Updated {}s ago", ((s.now_ms - fetched) as f64 / 1000.0).round().max(0.0) as i64) } else { "Reading…".to_owned() };
    let big = webkit::page_width() < 640.0;
    let refresh = row().key("machine-refresh").role("button").label("Refresh the machine reading").help("Refresh").focusable(true).size(36.0, 36.0).center().radius(18.0).hover(t.text.base.alpha(0.07)).focus_ring(2.0, 2.0, t.kit.c.border_focus).on_press(|s: &mut App| s.extra_ask_machine()).child(icon(&icons_phosphor::ARROWS_CLOCKWISE, if big { 18.0 } else { 16.0 }).color(t.text.base).a11y_hidden());
    let actions = row().gap(6.0).center_y().child(text(updated).key("machine-updated").font(webkit::tnum(webkit::font(t, 12.0, 400.0, 18.0))).color(t.text.muted.alpha(0.7))).child(refresh);
    let body = match s.extra.machine.as_ref() {
        None if s.extra.machine_empty => webkit::empty(t, "machine-empty", &icons_phosphor::GAUGE_REGULAR, "No reading yet", "The daemon hasn’t sampled the machine. It does so every 15 seconds while it runs.", None),
        None => loading_rows(t, "machine-loading", 4),
        Some((h, _)) => machine_body(s, t, h),
    };
    webkit::section(t, "machine", "This Mac", None, Some(actions), body)
}

/// `LoadingRows`: a skeleton line per row while the daemon has not answered.
fn loading_rows(t: &Neko, key: &str, rows: usize) -> El {
    let list: Vec<El> = (0..rows).map(|i| row().w_full().px(12.0).py(10.0).gap(12.0).center_y().child(webkit::skeleton(t, &format!("{key}-dot-{i}"), 14.0, 14.0).radius(7.0)).child(el().h(16.0).w_frac((32 + (i * 17) % 36) as f32 / 100.0).radius(6.0).fill(t.text.base.alpha(0.06)))).collect();
    col().key(key.to_owned()).mx(-12.0).children(list)
}

fn tone_ink(t: &Neko, level: u8) -> Option<Color> {
    match level {
        2 => Some(t.meaning.coral),
        1 => Some(t.meaning.amber),
        _ => None,
    }
}

/// The machine reading: the state line, the four figures, and what each kind of agent holds.
fn machine_body(s: &App, t: &Neko, h: &MachineHealth) -> El {
    let state = machine_state(h);
    let slowed = if h.slowed > 0 { format!(" Slowed {} build and test {}; nothing was stopped.", h.slowed, if h.slowed == 1 { "process" } else { "processes" }) } else { String::new() };
    let line = row()
        .key("machine-state")
        .w_full()
        .gaps(4.0, 12.0)
        .flex_wrap()
        .center_y()
        .child(text(state.title.clone()).key("machine-state-title").font(webkit::font(t, 13.5, 400.0, 20.25)).color(tone_ink(t, state.level).unwrap_or(t.text.base)).a11y_hidden())
        .child(text(format!("{}{slowed}", state.detail)).key("machine-state-detail").font(webkit::font(t, 12.5, 400.0, 18.75)).color(t.text.muted).wrap().min_w(0.0).shrink().a11y_hidden())
        .child(el().grow(1.0).min_w(0.0))
        .child(text(format!("read {}", webkit::relative_time(s.now_ms, h.at_ms))).font(webkit::tnum(webkit::font(t, 12.0, 400.0, 18.0))).color(t.text.muted.alpha(0.7)).a11y_hidden());
    let figures: Vec<El> = machine_figures(h).into_iter().enumerate().map(|(i, (label, value, unit, detail, level))| webkit::figure(t, &format!("machine-{i}"), label, &value, unit.as_deref(), Some(&detail), tone_ink(t, level), None)).collect();
    let mut c = col().w_full().gap(6.0).child(line).child(webkit::panel(t, figures)).child(webkit::group_heading(t, "machine-agents-head", "Memory held by agents", None));
    if h.agents.is_empty() {
        c = c.child(text("No agent processes are running right now.".to_owned()).key("machine-agents-none").font(webkit::font(t, 13.0, 400.0, 19.5)).color(t.text.muted).wrap().min_w(0.0));
    } else {
        let rows: Vec<El> = h
            .agents
            .iter()
            .take(6)
            .enumerate()
            .map(|(i, a)| {
                let size = if a.mb >= 1024 { format!("{:.1} GB", a.mb as f64 / 1024.0) } else { format!("{} MB", a.mb) };
                webkit::figure(t, &format!("machine-agent-{i}"), &a.name, &size, None, Some(&format!("{} {}", a.processes, if a.processes == 1 { "process" } else { "processes" })), None, None)
            })
            .collect();
        c = c.child(webkit::panel(t, rows));
    }
    c
}

/// `usd` (`Usage.tsx`): two decimals, none from a hundred.
fn usd_text(n: f64) -> String {
    usd(n)
}

/// `compact` (`Usage.tsx`): 1.2k, 3.4M.
fn compact(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{}M", fixed(n as f64 / 1e6, 1))
    } else if n >= 1_000 {
        format!("{}k", fixed(n as f64 / 1e3, 1))
    } else {
        n.to_string()
    }
}

const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/// The last fourteen days, oldest first: the day as "Nov 5" and what was spent on it, by the wall clock's days (`spendOf`).
pub fn spend_by_day_14(usage: &[(i64, u64)], now_ms: i64) -> Vec<(String, f64)> {
    let key = |ms: i64| {
        let l = clock::local(ms);
        (l.year, l.month, l.day)
    };
    let mut days: Vec<((i32, u32, u32), String, f64)> = (0..14).rev().map(|i| (key(now_ms - i * DAY_MS), String::new(), 0.0)).collect();
    for d in &mut days {
        d.1 = format!("{} {}", MONTHS[d.0.1 as usize - 1], d.0.2);
    }
    for &(at, micro) in usage {
        if let Some(d) = days.iter_mut().find(|d| d.0 == key(at)) {
            d.2 += micro as f64 / 1_000_000.0;
        }
    }
    days.into_iter().map(|(_, label, cost)| (label, (cost * 100.0).round() / 100.0)).collect()
}

/// Recharts' five ticks for a value axis from 0 to the largest bar: `getNiceTickValues` with a step that is a multiple of 5% of a
/// power of ten.
pub fn chart_ticks(max: f64) -> [f64; 5] {
    let rough = if max > 0.0 { max / 4.0 } else { 0.25 };
    let digits = rough.log10().floor() + 1.0;
    let scale_base = 10f64.powf(digits);
    let ratio = rough / scale_base;
    let scale = if digits != 1.0 { 0.05 } else { 0.1 };
    let step = (ratio / scale - 1e-9).ceil() * scale * scale_base;
    let mut out = [0.0; 5];
    for (i, v) in out.iter_mut().enumerate() {
        *v = (i as f64 * step * 1e6).round() / 1e6;
    }
    out
}

/// A bar with its top corners rounded (`radius={[4, 4, 0, 0]}`).
fn bar_path(x: f32, y: f32, w: f32, h: f32) -> Path {
    let r = 4.0f32.min(h).min(w / 2.0);
    let k = 0.5523 * r;
    Path::new().move_to([x, y + h]).line_to([x, y + r]).cubic_to([x, y + r - k], [x + r - k, y], [x + r, y]).line_to([x + w - r, y]).cubic_to([x + w - r + k, y], [x + w, y + r - k], [x + w, y + r]).line_to([x + w, y + h]).close()
}

/// `Per day`: the bar chart (`recharts`, 176 high): a 44 wide axis of dollars, five faint gridlines over a plot 138 high, fourteen bars
/// in 80% of their band and a date under every other bar.
fn chart(t: &Neko, w: f32, days: &[(String, f64)]) -> El {
    let (left, right, top, plot_h) = (44.0f32, 4.0f32, 8.0f32, 138.0f32);
    let plot_w = w - left - right;
    let max = days.iter().map(|d| d.1).fold(0.0f64, f64::max);
    let ticks = chart_ticks(max);
    let ceiling = ticks[4].max(1e-9);
    let y_of = |v: f64| top + plot_h * (1.0 - (v / ceiling) as f32);
    let band = plot_w / days.len() as f32;
    let mut shape = Shape::new().view_box(w, CHART_H);
    for v in ticks {
        let y = y_of(v);
        shape = shape.line([left, y], [left + plot_w, y], 1.0, rune::LineCap::Butt, Color::WHITE.alpha(0.0353));
    }
    for (i, (_, cost)) in days.iter().enumerate() {
        if *cost <= 0.0 {
            continue;
        }
        let y = y_of(*cost);
        let bw = (band * 0.8).floor();
        shape = shape.fill_path(&bar_path(left + i as f32 * band + band * 0.1, y, bw, top + plot_h - y), t.meaning.iris);
    }
    let style = webkit::font(t, 12.0, 400.0, 15.0);
    let mut c = el().key("usage-chart").w(w).h(CHART_H).child(canvas(shape).key("usage-line").pin(Some(0.0), Some(0.0), None, None).w(w).h(CHART_H));
    for v in ticks {
        let words = format!("${}", format_tick(v));
        let width = rune::text::measure(&words, &[], style, None).0;
        c = c.child(text(words).font(style).color(t.text.muted).a11y_hidden().pin(Some(36.0 - width), Some(y_of(v) - 7.75), None, None));
    }
    for (i, (label, _)) in days.iter().enumerate().step_by(2) {
        let width = rune::text::measure(label, &[], style, None).0;
        c = c.child(text(label.clone()).font(style).color(t.text.muted).a11y_hidden().pin(Some(left + (i as f32 + 0.5) * band - width / 2.0), Some(156.52), None, None));
    }
    c
}

/// A number the way JavaScript prints it in `$${v}`: no trailing zeros.
fn format_tick(v: f64) -> String {
    let s = format!("{v:.4}");
    s.trim_end_matches('0').trim_end_matches('.').to_owned()
}

fn spend(s: &App, t: &Neko, inner: f32) -> El {
    let settings = s.summary.as_ref().map(|m| &m.settings);
    let report = settings.map(|m| m.spend.clone()).unwrap_or_default();
    let usage = settings.map(|m| m.usage.clone()).unwrap_or_default();
    // (`+ 0.0`: an empty float sum is negative zero, which prints as "$-0.00".)
    let week: f64 = usage.iter().filter(|(at, _)| (0..=7 * DAY_MS).contains(&(s.now_ms - at))).map(|(_, m)| *m as f64 / 1e6).sum::<f64>() + 0.0;
    let has_data = report.runs > 0;
    let budget = settings.and_then(|m| m.task_budget_cents).map(|c| c as f64 / 100.0);
    let top = report.tasks.first();
    let share = match (budget, top) {
        (Some(b), Some(top)) if b > 0.0 => top.cost.0 / b,
        _ => 0.0,
    };
    let mut figures: Vec<El> = Vec::new();
    if let Some(today) = s.pages.routines.as_ref().map(|d| d.spend_today) {
        let total = today.tasks_usd + today.routines_usd;
        let ceiling = today.ceiling_cents.map(|c| c as f64 / 100.0);
        if total > 0.0 || ceiling.is_some() {
            let ink = if today.held { Some(t.meaning.coral) } else if ceiling.is_some_and(|c| total >= c * 0.8) { Some(t.meaning.amber) } else { None };
            let lead = match ceiling {
                None => "No daily ceiling. Set one in Settings → AI & agents.".to_owned(),
                Some(c) if today.held => format!("Ceiling of {} reached: new goal turns and routine runs wait until tomorrow.", usd_text(c)),
                Some(c) => format!("of a {} daily ceiling", usd_text(c)),
            };
            let detail = format!("{lead}{}", if today.routines_usd > 0.0 { format!(" · {} from routines", usd_text(today.routines_usd)) } else { String::new() });
            figures.push(webkit::figure(t, "usage-today", "Today", &usd_text(total), None, Some(&detail), ink, None));
        }
    }
    if has_data {
        figures.push(webkit::figure(t, "usage-recorded", "Recorded spend", &usd_text(report.total.0), None, Some(&format!("{} priced {} · {} in · {} out", report.runs, if report.runs == 1 { "run" } else { "runs" }, compact(report.tokens_in), compact(report.tokens_out))), None, None));
        figures.push(webkit::figure(t, "usage-week", "Last 7 days", &usd_text(week), None, Some(if week > 0.0 { "Across every task" } else { "Nothing priced this week" }), None, None));
    }
    let ink = if share >= 1.0 { Some(t.meaning.coral) } else if share >= 0.8 { Some(t.meaning.amber) } else { None };
    let budget_detail = match (budget, top) {
        (None, _) => "No limit set. Change it in Settings → AI & agents.".to_owned(),
        (Some(_), Some(top)) => format!("Costliest task so far: {}{}", usd_text(top.cost.0), if share >= 0.8 { format!(" ({}% of the budget)", (share * 100.0).round() as i64) } else { String::new() }),
        (Some(_), None) => "No priced task yet".to_owned(),
    };
    figures.push(webkit::figure(t, "usage-budget", "Per-task budget", &budget.map_or("None".to_owned(), usd_text), None, Some(&budget_detail), ink, None));
    if has_data {
        figures.push(webkit::figure(t, "usage-priced", "Tasks with a price", &report.priced_tasks.to_string(), None, Some(&format!("of {} in history", report.tasks_total)), None, None));
    }
    let mut c = col().w_full().gap(14.0).child(webkit::panel(t, figures).key("spend-figures"));
    if has_data {
        let days = spend_by_day_14(&usage, s.now_ms);
        c = c.child(webkit::group_heading(t, "usage-days-head", "Per day", Some("last 14 days, USD"))).child(chart(t, inner, &days)).child(webkit::group_heading(t, "usage-tasks-head", "Costliest tasks", None));
        let rows: Vec<El> = report
            .tasks
            .iter()
            .enumerate()
            .map(|(i, task)| {
                let title = text(task.title.clone()).key(format!("usage-task-{i}")).font(webkit::font(t, 13.5, 400.0, 20.25)).color(t.text.base).truncate().a11y_hidden();
                let runs = text(format!("{} {}", task.runs, if task.runs == 1 { "run" } else { "runs" })).font(webkit::font(t, 12.5, 400.0, 18.75)).color(t.text.muted).truncate().grow(1.0).min_w(0.0).a11y_hidden();
                let cost = text(usd_text(task.cost.0)).key(format!("usage-task-{i}-cost")).font(webkit::tnum(webkit::font(t, 12.5, 400.0, 18.75))).color(t.text.base).a11y_hidden();
                webkit::info_row(t, &format!("usage-task-row-{i}"), title, Some(runs), None, vec![cost], None)
            })
            .collect();
        c = c.child(webkit::panel(t, rows).key("spend-tasks"));
    } else {
        c = c.child(text("No priced runs yet. When an agent that reports its cost finishes a run, its spend shows up here.".to_owned()).key("usage-none").font(webkit::font(t, 13.0, 400.0, 19.5)).color(t.text.muted).wrap().min_w(0.0));
    }
    let title = "Spend";
    webkit::section(t, "spend", title, None, None, c).help("Read from the cost each Claude Code and OpenCode run reports. Codex subscriptions don’t report a price, so their runs aren’t counted here.")
}

/// What the machine reading says to the person, in the web's words (`machineState` in Usage.tsx): a tone and a line, with a detail.
#[derive(Clone, Debug, PartialEq)]
pub struct MachineState {
    /// 0 fine, 1 warning (amber), 2 on hold (coral).
    pub level: u8,
    pub title: String,
    pub detail: String,
}

pub fn machine_state(h: &MachineHealth) -> MachineState {
    if let Some(why) = h.holding.as_ref().filter(|w| !w.is_empty()) {
        return MachineState { level: 2, title: "New work is on hold".to_owned(), detail: why.clone() };
    }
    if h.cap.is_some() {
        let (title, detail) = if h.night {
            ("Night: one task at a time", "01:00–06:00: Neko runs one task at a time, and new work waits while the load is above 12.")
        } else {
            ("Memory tight: one at a time", "Memory pressure is high, so agents start one at a time until it eases.")
        };
        return MachineState { level: 1, title: title.to_owned(), detail: detail.to_owned() };
    }
    let detail = if h.night { "Night hours: one heavy job at a time." } else { "New agent work starts normally." };
    MachineState { level: 0, title: format!("Mac OK · load {}", h.load.round() as i64), detail: detail.to_owned() }
}

/// One figure of the reading: its words, and how loudly it speaks (0 quiet, 1 amber, 2 coral): colour only when it needs the person.
/// (label, value, unit, detail, level).
pub fn machine_figures(h: &MachineHealth) -> Vec<(&'static str, String, Option<String>, String, u8)> {
    let cores = h.cores.max(1) as f64;
    let per_core = h.load / cores;
    let pressure = if h.pressure.is_empty() { "normal".to_owned() } else { h.pressure.clone() };
    let pressure_cap = pressure.chars().next().map(|c| c.to_uppercase().collect::<String>() + &pressure[c.len_utf8()..]).unwrap_or_default();
    vec![
        (
            "Load",
            format!("{:.1}", h.load),
            Some(format!("of {} cores", h.cores.max(1))),
            if h.load >= 20.0 { "No new agents start above a load of 20.".to_owned() } else if per_core >= 0.85 { format!("Busy: {per_core:.2} per core") } else { format!("{per_core:.2} per core") },
            if h.load >= 20.0 { 2 } else if per_core >= 0.85 { 1 } else { 0 },
        ),
        (
            "Memory pressure",
            pressure_cap,
            None,
            format!("{:.1} GB compressed · swap {:.1} of {:.1} GB", h.compressed_gb, h.swap_used_gb, h.swap_total_gb),
            match pressure.as_str() {
                "critical" => 2,
                "warning" => 1,
                _ => 0,
            },
        ),
        (
            "Free disk",
            format!("{:.0}", h.free_disk_gb),
            Some("GB".to_owned()),
            if h.free_disk_gb < 15.0 { "Under 15 GB: nothing new starts.".to_owned() } else { "Neko holds new work under 15 GB.".to_owned() },
            if h.free_disk_gb < 15.0 { 2 } else if h.free_disk_gb < 30.0 { 1 } else { 0 },
        ),
        (
            "Cooling",
            if h.kernel_task_cpu >= 50.0 { "Throttling".to_owned() } else { "Normal".to_owned() },
            None,
            if h.kernel_task_cpu >= 50.0 { format!("macOS is throttling (kernel_task {:.0}%)", h.kernel_task_cpu) } else { format!("kernel_task {:.0}%", h.kernel_task_cpu) },
            if h.kernel_task_cpu >= 50.0 { 1 } else { 0 },
        ),
    ]
}

/// Runs by model, from the last runs the workbench kept (the web's `ModelSection`): at most six, most used first.
pub fn model_counts(runs: &[(String, Option<String>)]) -> Vec<(String, usize)> {
    let mut counts: Vec<(String, usize)> = Vec::new();
    for (model, effort) in runs {
        let name = format!("{}{}", if model.is_empty() { "default model" } else { model.as_str() }, effort.as_ref().filter(|e| !e.is_empty()).map(|e| format!(" · {e}")).unwrap_or_default());
        match counts.iter_mut().find(|(n, _)| *n == name) {
            Some((_, n)) => *n += 1,
            None => counts.push((name, 1)),
        }
    }
    counts.sort_by_key(|c| std::cmp::Reverse(c.1));
    counts.truncate(6);
    counts
}

fn models(s: &App, t: &Neko) -> El {
    let runs = s.summary.as_ref().map(|m| m.settings.model_runs.clone()).unwrap_or_default();
    let counts = model_counts(&runs);
    if counts.is_empty() {
        return el().h(0.0);
    }
    let items: Vec<(String, usize, String)> = counts.iter().map(|(name, n)| (name.clone(), *n, format!("{n} {}", if *n == 1 { "run" } else { "runs" }))).collect();
    webkit::section(t, "models", "Models", Some(&format!("last {} runs", runs.len())), None, webkit::panel(t, vec![webkit::bar_list(t, "model", &items)]))
}

/// "Quota": what each vendor has left, one `Figure` each; or one line that says why there is none, with "Check again".
fn quota(s: &App, t: &Neko) -> El {
    let body = match &s.pages.quota {
        None if s.pages.notes.contains_key("page:quota") => quota_none(t, "This daemon doesn’t report vendor quota."),
        None => loading_rows(t, "quota-loading", 2),
        Some(rows) if rows.is_empty() => quota_none(t, "No vendor is signed in, or none has a metered window."),
        Some(rows) => {
            let figures: Vec<El> = rows
                .iter()
                .enumerate()
                .map(|(i, r)| {
                    let ink = quota_ink(t, r.used);
                    webkit::figure(t, &format!("quota-{i}"), &r.label, &r.value, Some(&r.unit), Some(&r.detail), ink, Some(&r.detail))
                })
                .collect();
            webkit::panel(t, figures).key("quota-figures")
        }
    };
    webkit::section(t, "quota", "Quota", None, None, body).help("What each signed-in vendor has left, read from its own API.")
}

fn quota_none(t: &Neko, words: &str) -> El {
    let again = row().key("quota-again").role("button").label("Check again").focusable(true).h(18.75).gap(4.0).center_y().radius(10.0).focus_ring(2.0, 2.0, t.kit.c.border_focus).on_press(|s: &mut App| {
        s.pages.quota = None;
        s.pages.notes.remove("page:quota");
        s.call(crate::link::Call::Quota);
    }).child(icon(&icons_phosphor::ARROWS_CLOCKWISE, 12.0).color(t.text.muted).a11y_hidden()).child(text("Check again".to_owned()).font(webkit::font(t, 12.5, 400.0, 18.75)).color(t.text.muted).a11y_hidden());
    row().key("quota-none").w_full().gaps(0.0, 12.0).flex_wrap().center_y().child(text(words.to_owned()).font(webkit::font(t, 13.0, 400.0, 19.5)).color(t.text.muted).wrap().min_w(0.0).shrink().a11y_hidden()).child(again)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_per_day_axis_has_rechartss_five_ticks() {
        assert_eq!(chart_ticks(3.9), [0.0, 1.0, 2.0, 3.0, 4.0]);
        assert_eq!(chart_ticks(0.0), [0.0, 0.25, 0.5, 0.75, 1.0]);
        assert_eq!(chart_ticks(2.2), [0.0, 0.55, 1.1, 1.65, 2.2]);
        assert_eq!(format_tick(1.65), "1.65");
        assert_eq!(format_tick(2.0), "2");
        assert_eq!(compact(2_900), "2.9k");
        assert_eq!(compact(648_400), "648.4k");
        assert_eq!(compact(1_250_000), "1.3M");
        assert_eq!(compact(12), "12");
        assert_eq!(usd(0.125), "$0.13");
        assert_eq!(usd(13.3), "$13.30");
        assert_eq!(usd(154.4), "$154");
    }

    #[test]
    fn the_fortnight_runs_oldest_first_and_a_run_lands_on_its_own_day() {
        let now = 40 * DAY_MS + 12 * 3_600_000;
        let days = spend_by_day_14(&[(now - 500, 3_000_000), (now - 13 * DAY_MS, 1_000_000), (now - 14 * DAY_MS - 1000, 9_000_000)], now);
        assert_eq!(days.len(), 14);
        assert_eq!(days[13].1, 3.0);
        assert_eq!(days[0].1, 1.0);
        assert_eq!(days.iter().map(|d| d.1).sum::<f64>(), 4.0, "a run older than fourteen days is not drawn");
    }

    #[test]
    fn the_quota_value_turns_amber_at_70_and_coral_at_90() {
        let t = crate::theme::theme(true);
        assert_eq!(quota_ink(&t, 0.5), None);
        assert_eq!(quota_ink(&t, 0.7), Some(t.meaning.amber));
        assert_eq!(quota_ink(&t, 0.899), Some(t.meaning.amber));
        assert_eq!(quota_ink(&t, 0.9), Some(t.meaning.coral));
    }
}
