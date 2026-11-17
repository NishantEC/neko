//! The pages under "More": Usage (today's spend, the last week as a line, each vendor's quota as a
//! thin bar), Workspaces (one line per scope that opens to its folders and servers) and Plugins (the
//! installed list with how each last loaded). Charts are drawn with Rune's shapes, no chart crate.
//! Ported from `pages/setup/Usage.tsx`, `pages/setup/Workspaces.tsx` and the plugin status the web
//! reports to the daemon; the same requests, no writes.

use rune::{Color, El, Shape, canvas, col, el, row, text};

use neko_protocol::workbench::MachineHealth;

use super::ui::{group, page, quiet_line};
use crate::app::App;
use crate::theme::Neko;

const DAY_MS: i64 = 86_400_000;
const CHART_H: f32 = 160.0;

/// Spend per day for the last seven days, oldest first, in US dollars: day `6` is the last 24 hours.
pub fn spend_by_day(usage: &[(i64, u64)], now_ms: i64) -> [f64; 7] {
    let mut days = [0.0; 7];
    for &(at, micro) in usage {
        let age = now_ms - at;
        if (0..7 * DAY_MS).contains(&age) {
            days[6 - (age / DAY_MS) as usize] += micro as f64 / 1_000_000.0;
        }
    }
    days
}

fn usd(n: f64) -> String {
    if n >= 100.0 { format!("${n:.0}") } else { format!("${n:.2}") }
}

/// The bar's colour: Iris, amber past 80% used, coral past 95%.
pub fn bar_color(t: &Neko, used: f32) -> Color {
    if used >= 0.95 {
        t.meaning.coral
    } else if used >= 0.8 {
        t.meaning.amber
    } else {
        t.meaning.iris
    }
}

/// A figure as one line: label, value, muted detail (DESIGN.md §8 rule 12).
fn figure(t: &Neko, key: &str, label: &str, value: &str, detail: &str) -> El {
    row()
        .key(key.to_owned())
        .w_full()
        .min_h(32.0)
        .px(6.0)
        .gap(10.0)
        .center_y()
        .child(text(label.to_owned()).font(t.ty.row).color(t.text.muted))
        .child(text(value.to_owned()).key(format!("{key}-value")).font(rune::TextStyle { weight: 500.0, ..t.ty.row }).color(t.text.base))
        .child(text(detail.to_owned()).font(t.ty.meta).color(t.text.muted).truncate().grow(1.0).min_w(0.0))
}

/// The seven-day line: 1.5 point Iris on three faint gridlines, with the amounts at the left.
fn chart(t: &Neko, days: &[f64; 7]) -> El {
    let top = days.iter().cloned().fold(0.0f64, f64::max);
    // A round top for the axis so the gridlines read as 0, half and full.
    let ceiling = if top <= 0.0 { 1.0 } else { nice(top) };
    const W: f32 = 700.0;
    let y = |v: f64| CHART_H - 6.0 - (v / ceiling) as f32 * (CHART_H - 12.0);
    let x = |i: usize| 8.0 + i as f32 * (W - 16.0) / 6.0;
    let grid = t.text.faint.alpha(0.35);
    let mut shape = Shape::new().view_box(W, CHART_H);
    for tick in [0.0, 0.5, 1.0] {
        let gy = y(ceiling * tick);
        shape = shape.line([0.0, gy], [W, gy], 1.0, rune::LineCap::Butt, grid);
    }
    let points: Vec<[f32; 2]> = days.iter().enumerate().map(|(i, v)| [x(i), y(*v)]).collect();
    shape = shape.stroke(points, 1.5, t.meaning.iris);
    let axis = col().w(44.0).h(CHART_H).child(
        el().w_full().h(CHART_H).child(
            col()
                .w_full()
                .h(CHART_H)
                .justify_between()
                .child(text(usd(ceiling)).font(t.ty.id).color(t.text.faint))
                .child(text(usd(ceiling / 2.0)).font(t.ty.id).color(t.text.faint))
                .child(text(usd(0.0)).font(t.ty.id).color(t.text.faint)),
        ),
    );
    let mut labels = row().w_full().px(8.0).pl(52.0);
    for (i, _) in days.iter().enumerate() {
        let word = if i == 6 { "today".to_owned() } else { format!("{}d", 6 - i) };
        labels = labels.child(row().grow(1.0).min_w(0.0).justify_center().child(text(word).font(t.ty.id).color(t.text.faint)));
    }
    col().key("usage-chart").w_full().gap(4.0).child(row().w_full().gap(8.0).child(axis).child(canvas(shape).key("usage-line").grow(1.0).h(CHART_H))).child(labels)
}

/// A round number at or above `v`: 1, 2, 5 times a power of ten.
pub fn nice(v: f64) -> f64 {
    let p = 10f64.powf(v.log10().floor());
    [1.0, 2.0, 5.0, 10.0].iter().map(|m| m * p).find(|c| *c >= v).unwrap_or(10.0 * p)
}

pub fn usage(s: &App, t: &Neko) -> El {
    let usage = s.summary.as_ref().map(|m| m.settings.usage.clone()).unwrap_or_default();
    let ceiling = s.summary.as_ref().and_then(|m| m.settings.daily_budget_cents);
    let spend = s.pages.routines.as_ref().map(|d| d.spend_today);
    let today = match &spend {
        Some(sp) => sp.tasks_usd + sp.routines_usd,
        None => usage.iter().filter(|(at, _)| s.now_ms - at < DAY_MS).map(|(_, m)| *m as f64 / 1_000_000.0).sum(),
    };
    let ceiling = spend.and_then(|sp| sp.ceiling_cents).or(ceiling);
    let detail = match (ceiling, spend.is_some_and(|sp| sp.held)) {
        (Some(c), true) => format!("Ceiling of {} reached: new goal turns and routine runs wait until tomorrow.", usd(c as f64 / 100.0)),
        (Some(c), false) => format!("of a {} daily ceiling", usd(c as f64 / 100.0)),
        (None, _) => "No daily ceiling. Set one in Settings.".to_owned(),
    };
    let mut body = col().w_full().gap(18.0).child(machine(s, t)).child(figure(t, "usage-today", "Today", &usd(today), &detail));
    let days = spend_by_day(&usage, s.now_ms);
    body = if usage.is_empty() {
        body.child(quiet_line(t, "usage-none", "No priced runs yet"))
    } else {
        body.child(col().w_full().gap(6.0).child(group(t, "Last 7 days")).child(chart(t, &days)))
    };
    body = body.child(models(s, t));
    body = body.child(quota(s, t));
    page(t, "usage", "Usage", None, Vec::new(), body)
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
pub fn machine_figures(h: &MachineHealth) -> Vec<(&'static str, String, String, u8)> {
    let cores = h.cores.max(1) as f64;
    let per_core = h.load / cores;
    let pressure = if h.pressure.is_empty() { "normal".to_owned() } else { h.pressure.clone() };
    let pressure_cap = pressure.chars().next().map(|c| c.to_uppercase().collect::<String>() + &pressure[c.len_utf8()..]).unwrap_or_default();
    vec![
        (
            "Load",
            format!("{:.1} of {} cores", h.load, h.cores.max(1)),
            if h.load >= 20.0 { "No new agents start above a load of 20.".to_owned() } else if per_core >= 0.85 { format!("Busy: {per_core:.2} per core") } else { format!("{per_core:.2} per core") },
            if h.load >= 20.0 { 2 } else if per_core >= 0.85 { 1 } else { 0 },
        ),
        (
            "Memory pressure",
            pressure_cap,
            format!("{:.1} GB compressed · swap {:.1} of {:.1} GB", h.compressed_gb, h.swap_used_gb, h.swap_total_gb),
            match pressure.as_str() {
                "critical" => 2,
                "warning" => 1,
                _ => 0,
            },
        ),
        (
            "Free disk",
            format!("{:.0} GB", h.free_disk_gb),
            if h.free_disk_gb < 15.0 { "Under 15 GB: nothing new starts.".to_owned() } else { "Neko holds new work under 15 GB.".to_owned() },
            if h.free_disk_gb < 15.0 { 2 } else if h.free_disk_gb < 30.0 { 1 } else { 0 },
        ),
        (
            "Cooling",
            if h.kernel_task_cpu >= 50.0 { "Throttling".to_owned() } else { "Normal".to_owned() },
            if h.kernel_task_cpu >= 50.0 { format!("macOS is throttling (kernel_task {:.0}%)", h.kernel_task_cpu) } else { format!("kernel_task {:.0}%", h.kernel_task_cpu) },
            if h.kernel_task_cpu >= 50.0 { 1 } else { 0 },
        ),
    ]
}

fn tone(t: &Neko, level: u8) -> rune::Color {
    match level {
        2 => t.meaning.coral,
        1 => t.meaning.amber,
        _ => t.text.base,
    }
}

/// "Machine health": the hold or cap on new work, then load, memory, disk and cooling as one-line figures (DESIGN.md §8 rule 12).
fn machine(s: &App, t: &Neko) -> El {
    let mut c = col().key("machine").w_full().gap(2.0).child(group(t, "Machine health"));
    let Some((h, read_at)) = s.extra.machine.as_ref() else {
        let line = if s.extra.machine_empty { "No reading yet. The daemon samples the machine every 15 seconds while it runs." } else { s.pages.notes.get("page:machine").map_or("Reading the machine…", String::as_str) };
        return c.child(quiet_line(t, "machine-loading", line));
    };
    let state = machine_state(h);
    c = c.child(
        row()
            .key("machine-state")
            .w_full()
            .min_h(32.0)
            .px(6.0)
            .gap(10.0)
            .center_y()
            .child(text(state.title.clone()).key("machine-state-title").font(rune::TextStyle { weight: 500.0, ..t.ty.row }).color(tone(t, state.level)))
            .child(text(format!("{}{}", state.detail, if h.slowed > 0 { format!(" Slowed {} build and test {}; nothing was stopped.", h.slowed, if h.slowed == 1 { "process" } else { "processes" }) } else { String::new() })).font(t.ty.meta).color(t.text.muted).wrap().grow(1.0).min_w(0.0))
            .child(text(format!("read {}", crate::pages::ago(s.now_ms, *read_at))).font(t.ty.id).color(t.text.faint)),
    );
    for (i, (label, value, detail, level)) in machine_figures(h).into_iter().enumerate() {
        c = c.child(
            row()
                .key(format!("machine-{i}"))
                .w_full()
                .min_h(32.0)
                .px(6.0)
                .gap(10.0)
                .center_y()
                .child(text(label).font(t.ty.row).color(t.text.muted))
                .child(text(value).key(format!("machine-{i}-value")).font(rune::TextStyle { weight: 500.0, ..t.ty.row }).color(tone(t, level)))
                .child(text(detail).font(t.ty.meta).color(t.text.muted).truncate().grow(1.0).min_w(0.0)),
        );
    }
    if !h.agents.is_empty() {
        c = c.child(group(t, "Memory held by agents").pt(8.0));
        for (i, a) in h.agents.iter().take(6).enumerate() {
            let size = if a.mb >= 1024 { format!("{:.1} GB", a.mb as f64 / 1024.0) } else { format!("{} MB", a.mb) };
            c = c.child(figure(t, &format!("machine-agent-{i}"), &a.name, &size, &format!("{} {}", a.processes, if a.processes == 1 { "process" } else { "processes" })));
        }
    } else {
        c = c.child(quiet_line(t, "machine-agents-none", "No agent processes are running right now."));
    }
    c
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
    let top = counts.first().map_or(1, |c| c.1).max(1) as f32;
    let mut c = col().key("models").w_full().gap(2.0).child(row().w_full().gap(8.0).child(group(t, "Models")).child(text(format!("last {} runs", runs.len())).font(t.ty.id).color(t.text.faint)));
    for (i, (name, n)) in counts.iter().enumerate() {
        let bar = row().w(160.0).h(4.0).radius(2.0).fill(t.surface.active).child(el().key(format!("model-fill-{i}")).w(160.0 * *n as f32 / top).h(4.0).radius(2.0).fill(t.meaning.iris));
        c = c.child(row().key(format!("model-{i}")).w_full().min_h(32.0).px(6.0).gap(12.0).center_y().radius(t.radius.row).hover(t.surface.hover).child(text(name.clone()).key(format!("model-{i}-name")).font(t.ty.row).color(t.text.base).truncate().grow(1.0).min_w(0.0)).child(bar).child(text(format!("{n} {}", if *n == 1 { "run" } else { "runs" })).key(format!("model-{i}-runs")).font(t.ty.meta).color(t.text.muted)));
    }
    c
}

fn quota(s: &App, t: &Neko) -> El {
    let mut c = col().key("quota").w_full().gap(2.0).child(group(t, "Quota"));
    match &s.pages.quota {
        None => c = c.child(quiet_line(t, "quota-loading", s.pages.notes.get("page:quota").map_or("Reading what each vendor has left…", String::as_str))),
        Some(rows) if rows.is_empty() => c = c.child(quiet_line(t, "quota-none", "No vendor is signed in, or none has a metered window.")),
        Some(rows) => {
            for (i, r) in rows.iter().enumerate() {
                let color = bar_color(t, r.used);
                let words = if r.words.is_empty() { format!("{:.0}% used", r.used * 100.0) } else { r.words.join(" · ") };
                let bar = row()
                    .w(160.0)
                    .h(4.0)
                    .radius(2.0)
                    .fill(t.surface.active)
                    .child(el().key(format!("quota-fill-{i}")).w(160.0 * r.used.clamp(0.0, 1.0)).h(4.0).radius(2.0).fill(color));
                c = c.child(
                    row()
                        .key(format!("quota-{i}"))
                        .w_full()
                        .min_h(32.0)
                        .px(6.0)
                        .gap(12.0)
                        .center_y()
                        .radius(t.radius.row)
                        .hover(t.surface.hover)
                        .child(text(r.label.clone()).font(t.ty.row).color(t.text.base).truncate().grow(1.0).min_w(0.0))
                        .child(bar)
                        .child(text(words).key(format!("quota-words-{i}")).font(t.ty.meta).color(t.text.muted).truncate().min_w(0.0).max_w(200.0)),
                );
            }
        }
    }
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spend_lands_in_the_day_it_was_made_oldest_first() {
        let now = 10 * DAY_MS;
        let usage = [(now - 1000, 2_000_000u64), (now - DAY_MS - 5, 1_500_000), (now - 6 * DAY_MS - 1, 500_000), (now - 7 * DAY_MS - 1, 9_000_000), (now + 5, 1)];
        let d = spend_by_day(&usage, now);
        assert_eq!(d[6], 2.0);
        assert_eq!(d[5], 1.5);
        assert_eq!(d[0], 0.5);
        assert_eq!(d.iter().sum::<f64>(), 4.0, "a run older than a week or in the future is not drawn");
    }

    #[test]
    fn the_axis_tops_at_a_round_number() {
        assert_eq!(nice(0.83), 1.0);
        assert_eq!(nice(1.2), 2.0);
        assert_eq!(nice(3.7), 5.0);
        assert_eq!(nice(5.0), 5.0);
        assert_eq!(nice(12.0), 20.0);
    }

    #[test]
    fn the_quota_bar_turns_amber_at_80_and_coral_at_95() {
        let t = crate::theme::theme(true);
        assert_eq!(bar_color(&t, 0.5), t.meaning.iris);
        assert_eq!(bar_color(&t, 0.8), t.meaning.amber);
        assert_eq!(bar_color(&t, 0.949), t.meaning.amber);
        assert_eq!(bar_color(&t, 0.95), t.meaning.coral);
    }
}
