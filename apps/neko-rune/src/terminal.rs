//! The Terminal tab's data: a session's shell commands and their output as a terminal would have
//! drawn it (P3a). The commands are the receipts the session view already shows as tool cards
//! ([`crate::turns::command_receipt`]); each one's output goes through the `vt100` crate (MIT), so
//! colour codes and cursor moves come out as they would in a terminal, and is kept as lines of
//! styled runs that name colours by their terminal meaning (default, an index, true colour). The
//! theme turns those into its own colours when a line is drawn ([`spans`]).
//!
//! Bounds: the last [`MAX_COMMANDS`] commands, and per command at most [`MAX_LINES`] lines of
//! output: the first half and the last half, with "…N lines hidden" between.

use neko_protocol::workbench::Task;
use rune::Span;

use crate::theme::Neko;
use crate::turns::command_receipt;

/// Commands kept, newest last.
pub const MAX_COMMANDS: usize = 200;
/// Lines of output kept per command.
pub const MAX_LINES: usize = 2000;
/// The terminal's width in columns; longer lines wrap as a terminal wraps them.
pub const COLS: u16 = 120;
/// Rows of the live screen; lines that scroll off it are read from the scrollback.
const ROWS: u16 = 24;

/// A terminal colour by its meaning, before any theme.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Ink {
    #[default]
    Default,
    Index(u8),
    Rgb(u8, u8, u8),
}

impl From<vt100::Color> for Ink {
    fn from(c: vt100::Color) -> Ink {
        match c {
            vt100::Color::Default => Ink::Default,
            vt100::Color::Idx(i) => Ink::Index(i),
            vt100::Color::Rgb(r, g, b) => Ink::Rgb(r, g, b),
        }
    }
}

/// A stretch of one line drawn the same way.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Run {
    pub text: String,
    pub fg: Ink,
    pub bg: Ink,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub inverse: bool,
}

impl Run {
    fn same_style(&self, other: &Run) -> bool {
        (self.fg, self.bg, self.bold, self.italic, self.underline, self.inverse) == (other.fg, other.bg, other.bold, other.italic, other.underline, other.inverse)
    }
}

pub type Line = Vec<Run>;

/// The text of a line without its styles.
pub fn plain(line: &Line) -> String {
    line.iter().map(|r| r.text.as_str()).collect()
}

/// One command: what was run, how it ended and what it printed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Entry {
    /// Stable while the task's events stay the same: the event's position.
    pub id: String,
    pub command: String,
    pub exit: Option<i64>,
    /// The kept lines: all of them, or the first half then the last half around the gap.
    pub lines: Vec<Line>,
    /// Lines left out between `lines[..at]` and `lines[at..]`.
    pub hidden: usize,
    /// Where the "…N lines hidden" row sits in `lines`.
    pub at: usize,
}

/// What the output of a command looks like once a terminal has drawn it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Rendered {
    pub lines: Vec<Line>,
    pub hidden: usize,
    pub at: usize,
}

/// The cell styles of one screen row as runs, trailing blanks dropped.
fn row_runs(screen: &vt100::Screen, row: u16, cols: u16) -> Line {
    let mut runs: Line = Vec::new();
    for col in 0..cols {
        let Some(cell) = screen.cell(row, col) else { break };
        if cell.is_wide_continuation() {
            continue;
        }
        let text = if cell.has_contents() { cell.contents() } else { " ".to_owned() };
        let run = Run { text, fg: cell.fgcolor().into(), bg: cell.bgcolor().into(), bold: cell.bold(), italic: cell.italic(), underline: cell.underline(), inverse: cell.inverse() };
        match runs.last_mut() {
            Some(last) if last.same_style(&run) => last.text.push_str(&run.text),
            _ => runs.push(run),
        }
    }
    // Trailing blanks that carry no background are not part of the line.
    while let Some(last) = runs.last_mut() {
        if last.bg == Ink::Default && !last.inverse {
            let trimmed = last.text.trim_end_matches(' ').len();
            last.text.truncate(trimmed);
        }
        if last.text.is_empty() { runs.pop(); } else { break }
    }
    runs
}

/// Most rows a terminal is given: the cap, and room for a few lines that wrapped.
const MAX_ROWS: usize = MAX_LINES + 100;

/// Runs `text` through a terminal big enough to hold it (so nothing scrolls away) and reads its rows.
/// A bare `\n` is a new line, as it is on a terminal that translates it (the saved output is the
/// program's stdout, not a raw tty stream).
fn lines_of(text: &str) -> Vec<Line> {
    let wrapped: usize = text.split('\n').map(|l| l.chars().count().div_ceil(usize::from(COLS)).max(1)).sum();
    let rows = (wrapped + 2).clamp(usize::from(ROWS), MAX_ROWS) as u16;
    let mut parser = vt100::Parser::new(rows, COLS, 0);
    parser.process(text.replace("\r\n", "\n").replace('\n', "\r\n").as_bytes());
    let mut lines: Vec<Line> = (0..rows).map(|r| row_runs(parser.screen(), r, COLS)).collect();
    while lines.last().is_some_and(Vec::is_empty) {
        lines.pop();
    }
    lines
}

/// What the output of a command looks like once a terminal has drawn it: at most [`MAX_LINES`] lines,
/// the first half and the last half around the gap when there are more.
pub fn render(output: &str) -> Rendered {
    let total: Vec<&str> = output.split('\n').collect();
    // A trailing newline is the end of the last line, not a blank line.
    let count = if output.ends_with('\n') { total.len() - 1 } else { total.len() };
    let half = MAX_LINES / 2;
    if count > MAX_LINES {
        let mut lines = lines_of(&total[..half].join("\n"));
        let at = lines.len();
        lines.extend(lines_of(&total[count - half..count].join("\n")));
        return Rendered { lines, hidden: count - MAX_LINES, at };
    }
    let lines = lines_of(output);
    // Long lines wrap into more rows than there are lines: keep the same bound on rows.
    if lines.len() > MAX_LINES {
        let hidden = lines.len() - MAX_LINES;
        let mut kept: Vec<Line> = lines[..half].to_vec();
        kept.extend_from_slice(&lines[lines.len() - half..]);
        return Rendered { lines: kept, hidden, at: half };
    }
    Rendered { lines, hidden: 0, at: 0 }
}

/// The task's commands, oldest first, at most [`MAX_COMMANDS`] (the newest), and how many older ones
/// were left out.
pub fn entries(task: &Task) -> (Vec<Entry>, usize) {
    let receipts: Vec<(usize, _)> = task.events.iter().enumerate().filter_map(|(i, e)| command_receipt(e).map(|r| (i, r))).collect();
    let dropped = receipts.len().saturating_sub(MAX_COMMANDS);
    let entries = receipts
        .into_iter()
        .skip(dropped)
        .map(|(i, r)| {
            let out = render(&r.output);
            Entry { id: format!("receipt-{i}"), command: r.command, exit: r.exit, lines: out.lines, hidden: out.hidden, at: out.at }
        })
        .collect();
    (entries, dropped)
}

/// How many commands the task has run (cheap: no output is drawn). The tab shows when it is not 0.
pub fn command_count(task: &Task) -> usize {
    task.events.iter().filter(|e| command_receipt(e).is_some()).count()
}

/// A line as the text and the spans that colour it, in the theme's palette.
pub fn spans(line: &Line, t: &Neko) -> (String, Vec<Span>) {
    let mut text = String::new();
    let mut out = Vec::new();
    for run in line {
        let start = text.len();
        text.push_str(&run.text);
        let end = text.len();
        let colour = |ink: Ink| match ink {
            Ink::Default => None,
            Ink::Index(i) => Some(t.ansi(i)),
            Ink::Rgb(r, g, b) => Some(t.rgb(r, g, b)),
        };
        let (mut fg, mut bg) = (colour(run.fg), colour(run.bg));
        if run.inverse {
            // Inverse swaps the ink and the ground; a default one is the theme's own.
            let (f, b) = (fg.unwrap_or(t.terminal_fg()), bg.unwrap_or(t.terminal_bg()));
            (fg, bg) = (Some(b), Some(f));
        }
        if fg.is_none() && bg.is_none() && !run.bold && !run.italic && !run.underline {
            continue;
        }
        out.push(Span {
            start: start as u32,
            end: end as u32,
            weight: run.bold.then_some(700.0),
            color: fg,
            bg,
            italic: run.italic,
            underline: run.underline.then(|| fg.unwrap_or(t.terminal_fg())),
            ..Span::default()
        });
    }
    (text, out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::theme;

    fn text(r: &Rendered) -> Vec<String> {
        r.lines.iter().map(plain).collect()
    }

    #[test]
    fn plain_output_is_its_lines_and_a_trailing_newline_adds_no_blank_one() {
        assert_eq!(text(&render("one\ntwo\nthree\n")), ["one", "two", "three"]);
        assert_eq!(text(&render("one\n\ntwo")), ["one", "", "two"], "blank lines inside stay");
        assert_eq!(render("").lines.len(), 0);
        assert_eq!(text(&render("a\r\nb\r\n")), ["a", "b"], "a terminal's own line ends too");
    }

    #[test]
    fn colours_come_out_as_their_terminal_meaning() {
        let r = render("\x1b[31mred\x1b[0m plain \x1b[1;32mbold green\x1b[0m \x1b[38;2;10;20;30mtrue\x1b[0m \x1b[44mon blue\x1b[0m\n");
        let line = &r.lines[0];
        assert_eq!(plain(line), "red plain bold green true on blue");
        assert_eq!((line[0].text.as_str(), line[0].fg), ("red", Ink::Index(1)));
        assert_eq!(line[1].fg, Ink::Default);
        let bold = line.iter().find(|r| r.text == "bold green").unwrap();
        assert!(bold.bold && bold.fg == Ink::Index(2));
        assert!(line.iter().any(|r| r.text == "true" && r.fg == Ink::Rgb(10, 20, 30)));
        assert!(line.iter().any(|r| r.text == "on blue" && r.bg == Ink::Index(4)));
    }

    #[test]
    fn cursor_moves_and_carriage_returns_are_applied() {
        // A progress line rewritten in place keeps only its last state.
        assert_eq!(text(&render("10%\r50%\r100%\n")), ["100%"]);
        // Erase and move: "abc", back two, overwrite the middle.
        assert_eq!(text(&render("abc\x1b[2Dx\n")), ["axc"]);
        // Clear line.
        assert_eq!(text(&render("gone\x1b[2Kleft\n")).concat().trim(), "left");
    }

    #[test]
    fn long_lines_wrap_at_the_terminal_width() {
        let long = "x".repeat(COLS as usize * 2 + 5);
        let r = render(&format!("{long}\n"));
        assert_eq!(r.lines.len(), 3);
        assert_eq!(plain(&r.lines[2]).len(), 5);
    }

    #[test]
    fn output_over_the_cap_keeps_the_first_and_last_halves_around_a_gap() {
        let output: String = (0..5000).map(|i| format!("line {i}\n")).collect();
        let r = render(&output);
        assert_eq!(r.lines.len(), MAX_LINES);
        assert_eq!(r.hidden, 3000);
        assert_eq!(r.at, MAX_LINES / 2);
        let t = text(&r);
        assert_eq!(t[0], "line 0");
        assert_eq!(t[MAX_LINES / 2 - 1], "line 999");
        assert_eq!(t[MAX_LINES / 2], "line 4000", "the gap holds lines 1000 to 3999");
        assert_eq!(t[MAX_LINES - 1], "line 4999");
        // At the cap exactly, nothing is hidden.
        let exact: String = (0..MAX_LINES).map(|i| format!("l{i}\n")).collect();
        let r = render(&exact);
        assert_eq!((r.lines.len(), r.hidden), (MAX_LINES, 0));
    }

    fn task(events: &[(&str, String)]) -> Task {
        serde_json::from_value(serde_json::json!({
            "id": "t", "workspace_id": "w", "issue_id": null, "title": "t", "goal": "g", "status": "Building", "plan": "", "result": "", "worktree": null,
            "events": events.iter().enumerate().map(|(i, (role, message))| serde_json::json!({"at_ms": i as i64, "role": role, "message": message})).collect::<Vec<_>>(),
            "created_at_ms": 0, "updated_at_ms": 0,
        }))
        .unwrap()
    }

    fn receipt(command: &str, output: &str, exit: i64) -> String {
        format!("VERIFICATION_COMMAND {}", serde_json::json!({"command": command, "output": output, "exit_code": exit}))
    }

    #[test]
    fn a_tasks_commands_are_its_saved_receipts_and_only_the_last_200_are_kept() {
        let mut events = vec![("builder", "Working in the folder.".to_owned()), ("builder", receipt("cargo test", "ok\n", 0)), ("user", receipt("a user cannot make a receipt", "", 0)), ("reviewer", receipt("rg retry", "src/a.rs:1\n", 1))];
        let t = task(&events);
        let (list, dropped) = entries(&t);
        assert_eq!(dropped, 0);
        assert_eq!(list.iter().map(|e| (e.command.as_str(), e.exit)).collect::<Vec<_>>(), [("cargo test", Some(0)), ("rg retry", Some(1))]);
        assert_eq!(command_count(&t), 2);
        assert_eq!(list[1].id, "receipt-3", "the id is the event's place, as the session view's tool card has it");
        // 250 commands: the newest 200 stay, the oldest are counted.
        events.clear();
        for i in 0..250 {
            events.push(("builder", receipt(&format!("cmd {i}"), "x\n", 0)));
        }
        let (list, dropped) = entries(&task(&events));
        assert_eq!((list.len(), dropped), (200, 50));
        assert_eq!(list[0].command, "cmd 50");
        assert_eq!(list[199].command, "cmd 249");
    }

    #[test]
    fn colours_map_to_the_themes_palette_and_inverse_swaps_the_ground() {
        let t = theme(true);
        let r = render("\x1b[31mred\x1b[0m \x1b[7minv\x1b[0m\n");
        let (text, spans) = spans(&r.lines[0], &t);
        assert_eq!(text, "red inv");
        assert_eq!(spans[0].color.map(|c| c.hex()), Some(t.meaning.coral.hex()));
        let inv = &spans[1];
        assert_eq!(inv.color.map(|c| c.hex()), Some(t.terminal_bg().hex()));
        assert_eq!(inv.bg.map(|c| c.hex()), Some(t.terminal_fg().hex()));
    }
}
