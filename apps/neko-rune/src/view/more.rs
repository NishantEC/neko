//! The pages under "More": Usage (today's spend, the last week as a line, each vendor's quota as a
//! thin bar), Workspaces (one line per scope that opens to its folders and servers) and Plugins (the
//! installed list with how each last loaded). Charts are drawn with Rune's shapes, no chart crate.
//! Ported from `pages/setup/Usage.tsx`, `pages/setup/Workspaces.tsx` and the plugin status the web
//! reports to the daemon; the same requests, no writes.

use rune::{Color, El, Shape, canvas, col, el, row, text};

use super::ui::{MenuEntry, group, page, quiet_line, with_menu};
use crate::app::App;
use crate::pages::ago;
use crate::status::{StatusKind, status_glyph};
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
    let mut body = col().w_full().gap(18.0).child(figure(t, "usage-today", "Today", &usd(today), &detail));
    let days = spend_by_day(&usage, s.now_ms);
    body = if usage.is_empty() {
        body.child(quiet_line(t, "usage-none", "No priced runs yet"))
    } else {
        body.child(col().w_full().gap(6.0).child(group(t, "Last 7 days")).child(chart(t, &days)))
    };
    body = body.child(quota(s, t));
    page(t, "usage", "Usage", None, Vec::new(), body)
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

// ---- Plugins

pub fn plugins(s: &App, t: &Neko, layer: &mut Vec<El>) -> El {
    let mut body = col().w_full().gap(2.0);
    let list = s.pages.plugins.clone();
    match &list {
        None => body = body.child(quiet_line(t, "plugins-loading", s.pages.notes.get("page:plugins").map_or("Reading the plugins…", String::as_str))),
        Some(l) if l.is_empty() => body = body.child(quiet_line(t, "plugins-none", "No plugin has loaded yet")),
        Some(l) => {
            for p in l {
                let (kind, words) = if let Some(problem) = &p.problem {
                    (StatusKind::Failed, format!("Failed: {problem}"))
                } else if p.rolled_back {
                    (StatusKind::InProgress, "Rolled back to the previous version".to_owned())
                } else if p.ok {
                    (StatusKind::Done, format!("Loaded {}", ago(s.now_ms, p.at_ms)))
                } else {
                    (StatusKind::Backlog, "Not loaded".to_owned())
                };
                let line = row()
                        .key(format!("plugin-{}", p.id))
                        .w_full()
                        .min_h(36.0)
                        .px(6.0)
                        .gap(12.0)
                        .center_y()
                        .radius(t.radius.row)
                        .hover(t.surface.hover)
                        .help(words.clone())
                        .child(status_glyph(kind, t, 14.0, Some(&words)))
                        .child(text(p.id.clone()).font(t.ty.row).color(t.text.base).truncate().grow(1.0).min_w(0.0))
                        .child(text(format!("v{}", p.version)).font(t.ty.id).color(t.text.faint))
                        .child(text(words).key(format!("plugin-words-{}", p.id)).font(t.ty.meta).color(if kind == StatusKind::Failed { t.meaning.coral } else { t.text.muted }).truncate().min_w(0.0).max_w(300.0));
                let mut entries: Vec<MenuEntry> = vec![("Check again".to_owned(), std::rc::Rc::new(|s: &mut App| s.call(crate::link::Call::Plugins)))];
                if p.id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.') && !p.id.starts_with('.') {
                    let dir = neko_protocol::support_dir().join("plugins").join(&p.id);
                    entries.push(("Open folder".to_owned(), std::rc::Rc::new(move |s: &mut App| s.open_path(&dir.to_string_lossy()))));
                }
                body = body.child(with_menu(s, t, layer, &format!("plugin:{}", p.id), line, entries));
            }
        }
    }
    page(t, "plugins", "Plugins", list.as_ref().filter(|l| !l.is_empty()).map(Vec::len), Vec::new(), body)
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
