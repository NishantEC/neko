//! The composer while the agent works (docs/knowledge/composer.md): Return steers, ⌥Return queues and ⌘Return interrupts (the two
//! swap when Settings says so); what waits above the composer, one quiet row each; the activity bar, one thin line that opens on
//! demand; and ↑ to bring a message back. At rest every key just sends.
//!
//! The composer is rune-ui's PromptInput, which knows one way to send. The keys that choose a mode are read by the wrapper round it
//! (`key_preview`), before the text field sees them; a typed message goes to the daemon with its mode, and the daemon decides when
//! and how it reaches the agent.

use neko_protocol::{Queued, QueuedId, QueuedKind, Request, SendMode, Task, TaskId, TaskStatus};
use rune::{El, Key, KeyEvent, Tx, col, row, text};
use rune_ui_style::{ButtonVariant as Variant, Size, weight};

use crate::state::{App, Asked};
use crate::ui::agent::{PressTx, icon_button};
use crate::ui::base::{Shape, Theme, glyph};
use crate::ui::button::Button;
use crate::ui::dropdown_menu::{DropdownMenu, DropdownMenuItem};
use crate::view::menu_named;

/// A Bash step still running after this long is a command still running (a quick one is just a step).
pub const COMMAND_AFTER_MS: i64 = 5_000;
/// Queued messages shown one by one; more fold behind "N queued".
pub const SHOWN_QUEUED: usize = 2;

/// What the composer remembers beyond its words.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Composer {
    /// The activity bar is open.
    pub activity_open: bool,
    /// The folded queue is open.
    pub queue_open: bool,
    /// The mode menu beside Send is open.
    pub mode_open: bool,
    /// ↑ brought a message back: the draft it replaced, and what it brought (↓ gives the draft back while the words are unchanged).
    pub recall: Option<Recall>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Recall {
    pub task: TaskId,
    pub draft: String,
    pub words: String,
}

/// What a command in the activity bar is.
#[derive(Clone, Debug, PartialEq)]
pub struct Running {
    pub words: String,
    pub since_ms: i64,
}

/// "2m", "1h 5m", "just started": how long something has run, in as few characters as read.
pub fn running_for(now_ms: i64, since_ms: i64) -> String {
    let minutes = (now_ms - since_ms).max(0) / 60_000;
    match minutes {
        0 => "just started".to_owned(),
        1..60 => format!("{minutes}m"),
        _ => format!("{}h {}m", minutes / 60, minutes % 60).replace(" 0m", ""),
    }
}

/// The first line of a message, for a row.
pub fn first_line(words: &str) -> String {
    words.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("").to_owned()
}

impl App {
    /// Whether a run is going in the open thread now (a thread that only waits on its kittens has none: every key just sends).
    pub fn open_is_running(&self) -> bool {
        self.open_task().is_some_and(|t| t.status.is_active()) || self.book().is_some_and(|b| b.running().is_some())
    }

    fn queue_first(&self) -> bool {
        self.runtimes.as_ref().is_some_and(|(_, s)| s.queue_first)
    }

    /// What Return does while the agent works, and what ⌥Return does (Settings can swap them).
    pub fn return_mode(&self) -> SendMode {
        if self.queue_first() { SendMode::Queue } else { SendMode::Steer }
    }

    pub fn option_return_mode(&self) -> SendMode {
        if self.queue_first() { SendMode::Steer } else { SendMode::Queue }
    }

    /// The words in the open thread's composer, when there are any.
    pub fn has_words(&self) -> bool {
        self.reply_box().is_some_and(|r| !r.text.trim().is_empty())
    }

    /// Sends the open thread's composer. `mode` is how it reaches a working agent (None: Return's); at rest every mode just sends.
    pub fn send_words(&mut self, mode: Option<SendMode>) {
        let Some(task) = self.open.clone() else { return };
        let words = self.reply_box().map(|r| r.text.clone()).unwrap_or_default();
        // A request waits for its answer: words are not sent past it.
        if words.trim().is_empty() || self.pending_request().is_some() {
            return;
        }
        let mode = self.open_is_running().then(|| mode.unwrap_or_else(|| self.return_mode()));
        if let Some(id) = self.send(Request::FollowUp { task: task.clone(), words: words.clone(), answering: None, mode }) {
            self.asked.insert(id, Asked::FollowUp(task.clone(), words));
            // The keyboard stays in the box: the next message can follow at once.
            let mut empty = rune::TextEdit::default();
            empty.focused = true;
            self.replies.insert(task, empty);
            self.composer.recall = None;
            self.composer.mode_open = false;
            self.at_bottom = true;
        }
    }

    pub fn send_queued_now(&mut self, queued: &QueuedId) {
        if let Some(task) = self.open.clone()
            && let Some(id) = self.send(Request::SendQueuedNow { task, queued: queued.clone() })
        {
            self.asked.insert(id, Asked::Queued);
        }
    }

    pub fn discard_queued(&mut self, queued: &QueuedId) {
        if let Some(task) = self.open.clone()
            && let Some(id) = self.send(Request::DiscardQueued { task, queued: queued.clone() })
        {
            self.asked.insert(id, Asked::Queued);
        }
    }

    /// Edit: the message leaves the list and its words come back into the composer, in front of what is there.
    pub fn edit_queued(&mut self, q: &Queued) {
        if let Some(id) = self.send(Request::DiscardQueued { task: q.task.clone(), queued: q.id.clone() }) {
            self.asked.insert(id, Asked::EditQueued(q.task.clone(), q.words.clone()));
        }
    }

    /// The words of an edited message are in the composer.
    pub fn queued_edited(&mut self, task: TaskId, words: String) {
        let r = self.replies.entry(task).or_default();
        let joined = if r.text.trim().is_empty() { words } else { format!("{words}\n\n{}", r.text) };
        *r = rune::TextEdit::new(&joined);
        r.focused = true;
    }

    /// Stops one kitten (and any it started): the × on its chip.
    pub fn stop_kitten(&mut self, id: &TaskId) {
        self.send(Request::StopTask { task: id.clone() });
    }

    /// The commands still running in the open thread: Bash steps of its running run that have gone on a while.
    pub fn running_commands(&self) -> Vec<Running> {
        let Some(book) = self.book() else { return Vec::new() };
        let Some(run) = book.running() else { return Vec::new() };
        book.parts
            .iter()
            .filter(|p| p.run.as_ref() == Some(&run.id))
            .filter_map(|p| match &p.body {
                neko_protocol::PartBody::Tool { name, summary, state: neko_protocol::ToolState::Running, .. } if name == "Bash" && self.now_ms - p.at_ms >= COMMAND_AFTER_MS => Some(Running { words: summary.clone(), since_ms: p.at_ms }),
                _ => None,
            })
            .collect()
    }

    /// ↑ in an empty composer: the newest queued message to edit, else the last message the person sent. ↓ gives the draft back.
    fn recall_up(&mut self) -> bool {
        let Some(task) = self.open.clone() else { return false };
        if self.reply_box().is_some_and(|r| !r.text.is_empty()) {
            return false;
        }
        let queued = self.book().and_then(|b| b.queued.iter().rev().find(|q| q.kind == QueuedKind::Queued)).cloned();
        if let Some(q) = queued {
            self.edit_queued(&q);
            return true;
        }
        let last = self.book().and_then(|b| b.parts.iter().rev().find_map(|p| if let neko_protocol::PartBody::Person { text } = &p.body { Some(text.clone()) } else { None }));
        let Some(words) = last.or_else(|| self.open_task().map(|t| t.words.clone())).filter(|w| !w.trim().is_empty()) else { return false };
        self.composer.recall = Some(Recall { task: task.clone(), draft: String::new(), words: words.clone() });
        let r = self.replies.entry(task).or_default();
        *r = rune::TextEdit::new(&words);
        r.focused = true;
        true
    }

    fn recall_down(&mut self) -> bool {
        let Some(r) = self.composer.recall.clone() else { return false };
        if self.open.as_ref() != Some(&r.task) || self.reply_box().map(|b| b.text.as_str()) != Some(r.words.as_str()) {
            self.composer.recall = None;
            return false;
        }
        let e = self.replies.entry(r.task).or_default();
        *e = rune::TextEdit::new(&r.draft);
        e.focused = true;
        self.composer.recall = None;
        true
    }
}

/// What Return with these modifiers sends as. `None`: the key is not ours (the text field takes it). `Some(None)`: send, with no mode
/// (nothing runs, so every mode just sends). While the agent works: Return steers, ⌥Return queues, ⌘Return interrupts (Settings can
/// swap the first two). At rest ⌘Return sends, as Return does; ⌥Return stays a new line, as it is in every text field.
pub fn mode_of(cmd: bool, alt: bool, running: bool, queue_first: bool) -> Option<Option<SendMode>> {
    let (first, second) = if queue_first { (SendMode::Queue, SendMode::Steer) } else { (SendMode::Steer, SendMode::Queue) };
    match (cmd, alt, running) {
        (false, false, true) => Some(Some(first)),
        (true, false, true) => Some(Some(SendMode::Interrupt)),
        (false, true, true) => Some(Some(second)),
        (true, false, false) => Some(None),
        _ => None,
    }
}

/// The keys that choose how words reach the agent, read before the text field sees them (the wrapper round the composer).
pub fn key_preview(s: &mut App, k: &KeyEvent, tx: &mut Tx) -> bool {
    let m = k.mods;
    match k.key {
        Key::Enter if !m.shift && !m.ctrl => {
            if !s.has_words() || s.pending_request().is_some() {
                return false;
            }
            let Some(mode) = mode_of(m.cmd, m.alt, s.open_is_running(), s.queue_first()) else { return false };
            if !tx.key_repeat() {
                s.send_words(mode);
            }
            true
        }
        // Escape stops the agent whatever is typed; the field alone only does it while the box is empty.
        Key::Escape if !(m.cmd || m.shift || m.alt || m.ctrl) && s.open_is_working() && s.pending_request().is_none() => {
            s.stop();
            true
        }
        Key::Up if !(m.cmd || m.shift || m.alt || m.ctrl) => s.recall_up(),
        Key::Down if !(m.cmd || m.shift || m.alt || m.ctrl) => s.recall_down(),
        _ => false,
    }
}

/// The placeholder of the composer, which says what Return does while the agent works.
pub fn placeholder(s: &App, task: &Task, asking: bool) -> String {
    let (ret, alt) = match s.return_mode() {
        SendMode::Queue => ("Queue a message", "⌥↩ to steer"),
        _ => ("Steer the agent", "⌥↩ to queue"),
    };
    if asking {
        "Waiting for your answer above. Escape denies it.".to_owned()
    } else if task.status == TaskStatus::Waiting {
        "The agent is waiting on its kittens. Say something to wake it, or press Escape to stop.".to_owned()
    } else if s.open_is_running() {
        format!("{ret} ({alt})")
    } else if task.status == TaskStatus::Asked {
        "Answer the question".to_owned()
    } else {
        "Message the agent".to_owned()
    }
}

/// The quiet line under the box while words are typed and the agent works: what each Return does.
pub fn key_hints(s: &App, t: &Theme) -> Option<El> {
    if !s.open_is_running() || !s.has_words() || s.pending_request().is_some() {
        return None;
    }
    let (a, b) = match s.return_mode() {
        SendMode::Queue => ("Return queues", "⌥Return steers"),
        _ => ("Return steers", "⌥Return queues"),
    };
    let said = format!("{a} · {b} · ⌘Return interrupts");
    Some(text(said.clone()).key("reply-hints").role("note").label(said).font(t.ty.sm).color(t.ink_faint()).wrap().min_w(0.0).pl(t.space.at(1.0)))
}

/// The mode beside Send while words wait to go to a working agent: what Return does, and the other ways, each of which sends.
pub fn mode_chip(s: &App, t: &Theme, layer: &mut Vec<El>) -> Option<El> {
    if !s.open_is_running() || !s.has_words() || s.pending_request().is_some() {
        return None;
    }
    let (word, ret, alt) = match s.return_mode() {
        SendMode::Queue => ("Queue", SendMode::Queue, SendMode::Steer),
        _ => ("Steer", SendMode::Steer, SendMode::Queue),
    };
    let name = |m: SendMode| match m {
        SendMode::Steer => "Steer",
        SendMode::Queue => "Queue",
        SendMode::Interrupt => "Interrupt",
    };
    let face = crate::ui::agent::chip(t, "send-mode", None)
        .role("button")
        .label(format!("Send as: {word}"))
        .help("How this message reaches the agent while it works")
        .expanded(s.composer.mode_open)
        .focusable(true)
        .child(text(word).font(t.ty.sm).color(t.ink()).a11y_hidden())
        .child(glyph(&crate::ui::icons::CHEVRON_UP_DOWN, t.control.icon_sm, t.ink_faint()))
        .on_press(|s: &mut App| s.composer.mode_open = !s.composer.mode_open);
    let item = |m: SendMode, keys: &'static str| DropdownMenuItem::new(name(m)).shortcut(keys).checked(m == ret).on_select(move |s: &mut App| s.send_words(Some(m)));
    let (ret_keys, alt_keys) = ("↩", "⌥↩");
    let menu = DropdownMenu::new("send-mode-menu")
        .trigger(face)
        .open(s.composer.mode_open)
        .min_width(220.0)
        .on_close(|s: &mut App| s.composer.mode_open = false)
        .label("Send while it works")
        .item(item(ret, ret_keys))
        .item(item(alt, alt_keys))
        .item(item(SendMode::Interrupt, "⌘↩"));
    Some(menu_named(menu, t, layer, "Send while it works", s.composer.mode_open))
}

fn ghost(t: &Theme, key: String, words: &str, press: impl Fn(&mut App) + 'static) -> El {
    Button::<App>::new(key, words).variant(Variant::Ghost).size(Size::Sm).on_press(press).build(t)
}

/// One message waiting, one line: what it is, its words, and what can be done with it.
fn waiting_row(t: &Theme, q: &Queued, n: usize, rule: bool) -> El {
    let (kind, tail) = match q.kind {
        QueuedKind::Queued => ("Queued", ""),
        QueuedKind::Steered => ("Steered", "waiting to be read"),
        QueuedKind::Interrupting => ("Interrupting", ""),
    };
    let line = first_line(&q.words);
    let said = if tail.is_empty() { format!("{kind}: {line}") } else { format!("{kind}, {tail}: {line}") };
    let mut r = row()
        .key(format!("waiting-{n}"))
        .role("group")
        .label(said)
        .w_full()
        .min_h(t.control.sm + 4.0)
        .pl(t.space.at(3.0))
        .pr(t.space.at(1.0))
        .gap(t.space.at(2.0))
        .center_y()
        .child(text(kind).font(weight(t.ty.sm, t.ty.medium)).color(t.ink_muted()).a11y_hidden())
        .child(text(line).font(t.ty.sm).color(t.ink_faint()).truncate().grow(1.0).shrink().min_w(0.0).a11y_hidden());
    if rule {
        r = r.border_sides([0.0, 0.0, 1.0, 0.0], t.c.line);
    }
    if !tail.is_empty() {
        r = r.child(text(tail).font(t.ty.sm).color(t.ink_faint()).a11y_hidden().pr(t.space.at(2.0)));
    }
    if q.kind == QueuedKind::Queued {
        let (send, edit, drop) = (q.clone(), q.clone(), q.id.clone());
        r = r
            .child(ghost(t, format!("waiting-{n}-send"), "Send now", move |s: &mut App| s.send_queued_now(&send.id)))
            .child(ghost(t, format!("waiting-{n}-edit"), "Edit", move |s: &mut App| s.edit_queued(&edit)))
            .child(icon_button(t, &format!("waiting-{n}-discard"), "Discard", &crate::ui::icons::CLOSE, None, false, Some(discard_press(drop))));
    }
    r
}

fn discard_press(id: QueuedId) -> PressTx<App> {
    std::rc::Rc::new(move |s: &mut App, _: &mut Tx| s.discard_queued(&id))
}

/// What waits above the composer, in one quiet block with a hairline between rows: the steered and interrupting messages as they
/// are, the queued ones one by one (two at most), more folded behind "3 queued".
pub fn waiting(s: &App, t: &Theme) -> Option<El> {
    let queued = &s.book()?.queued;
    if queued.is_empty() {
        return None;
    }
    let held: Vec<&Queued> = queued.iter().filter(|q| q.kind == QueuedKind::Queued).collect();
    let mut shown: Vec<&Queued> = queued.iter().filter(|q| q.kind != QueuedKind::Queued).collect();
    let on_way = shown.len();
    let folded = held.len() > SHOWN_QUEUED;
    let open = s.composer.queue_open;
    if !folded || open {
        shown.extend(held.iter().copied());
    }
    let rows = shown.len() + usize::from(folded);
    let mut stack = col().key("waiting").role("list").label("Messages waiting").w_full().gap(0.0).radius(t.radius.lg).round_as(t).fill(t.c.hover);
    let mut n = 0;
    for (i, q) in shown.iter().enumerate() {
        // The fold stands after the messages on their way and before the queued ones it holds.
        if folded && i == on_way {
            stack = stack.child(fold_row(t, held.len(), open, n + 1 < rows));
            n += 1;
        }
        stack = stack.child(waiting_row(t, q, n, n + 1 < rows));
        n += 1;
    }
    if folded && !open {
        stack = stack.child(fold_row(t, held.len(), open, false));
    }
    Some(stack)
}

fn fold_row(t: &Theme, count: usize, open: bool, rule: bool) -> El {
    let said = format!("{count} queued");
    let mut r = row()
        .key("waiting-fold")
        .role("button")
        .label(said.clone())
        .expanded(open)
        .focusable(true)
        .w_full()
        .min_h(t.control.sm + 4.0)
        .pl(t.space.at(3.0))
        .pr(t.space.at(3.0))
        .gap(t.space.at(2.0))
        .center_y()
        .child(text(said).font(weight(t.ty.sm, t.ty.medium)).color(t.ink_muted()).grow(1.0).a11y_hidden())
        .child(glyph(if open { &crate::ui::icons::CHEVRON_UP } else { &crate::ui::icons::CHEVRON_DOWN }, t.control.icon_sm, t.ink_faint()))
        .on_press(|s: &mut App| s.composer.queue_open = !s.composer.queue_open);
    if rule {
        r = r.border_sides([0.0, 0.0, 1.0, 0.0], t.c.line);
    }
    r
}

/// What is running for the thread, besides the words: its kittens and its long commands, in a sentence.
fn summary(kittens: usize, commands: usize) -> String {
    let k = match kittens {
        0 => None,
        1 => Some("1 kitten working".to_owned()),
        n => Some(format!("{n} kittens working")),
    };
    let c = match commands {
        0 => None,
        1 => Some("1 command running".to_owned()),
        n => Some(format!("{n} commands running")),
    };
    [k, c].into_iter().flatten().collect::<Vec<_>>().join(" · ")
}

/// The activity bar: one thin line above the composer while a kitten works or a command runs, opening on demand to what it is. The
/// pane's Overview holds the detail (titles, states, the tree); the bar is the quick way to see and to stop. None when nothing but
/// the run itself goes (the conversation shows that).
pub fn activity_bar(s: &App, t: &Theme) -> Option<El> {
    let task = s.open_task().filter(|t| t.is_thread())?;
    let kittens = s.live_subagents(&task.id);
    let commands = s.running_commands();
    if kittens.is_empty() && commands.is_empty() {
        return None;
    }
    let said = summary(kittens.len(), commands.len());
    let open = s.composer.activity_open;
    let face = row()
        .key("activity-bar")
        .role("button")
        .label(format!("Running: {said}"))
        .expanded(open)
        .focusable(true)
        .w_full()
        .h(t.control.md)
        .px(t.space.at(3.0))
        .gap(t.space.at(2.0))
        .center_y()
        .radius(t.radius.lg)
        .round_as(t)
        .hover(t.c.hover)
        .child(crate::subagents_view::still_ring(t, t.control.icon - 2.0).a11y_hidden())
        .child(text(said).font(t.ty.sm).color(t.ink_muted()).truncate().grow(1.0).shrink().min_w(0.0).a11y_hidden())
        .child(glyph(if open { &crate::ui::icons::CHEVRON_DOWN } else { &crate::ui::icons::CHEVRON_UP }, t.control.icon_sm, t.ink_faint()))
        .on_press(|s: &mut App| s.composer.activity_open = !s.composer.activity_open);
    let mut bar = col().key("activity").role("group").label("What is running").w_full().gap(0.0);
    if open {
        let mut list = col().key("activity-list").role("list").w_full().pb(t.space.at(1.0));
        if s.open_is_running() {
            let stop: PressTx<App> = std::rc::Rc::new(|s: &mut App, _: &mut Tx| s.stop());
            list = list.child(item(t, "activity-run", "The agent", "", "working", Some(("Stop the agent", stop))));
        }
        for k in kittens {
            let (id, id2) = (k.id.clone(), k.id.clone());
            let name = k.subagent_name().unwrap_or_default();
            let since = s.books.get(&k.id).and_then(|b| b.runs.last().map(|r| r.started_ms)).unwrap_or(k.created_ms);
            let age = if k.status == TaskStatus::New { "waiting its turn".to_owned() } else { running_for(s.now_ms, since) };
            let stop: PressTx<App> = std::rc::Rc::new(move |s: &mut App, _: &mut Tx| s.stop_kitten(&id));
            let num = k.subagent.as_ref().map(|o| o.number.clone()).unwrap_or_default();
            let mut line = item(t, &format!("activity-kitten-{num}"), &name, &k.title, &age, Some((&format!("Stop {name}"), stop)));
            line = line.on_press(move |s: &mut App| s.open_kitten_tab(&id2));
            list = list.child(line);
        }
        for (i, c) in commands.iter().enumerate() {
            list = list.child(item(t, &format!("activity-command-{i}"), &c.words, "", &running_for(s.now_ms, c.since_ms), None));
        }
        bar = bar.child(face).child(list);
    } else {
        bar = bar.child(face);
    }
    Some(bar)
}

/// One line of the open bar: a name, how long, and the × that stops it (a command has none: stopping the agent stops it).
fn item(t: &Theme, key: &str, name: &str, detail: &str, age: &str, stop: Option<(&str, PressTx<App>)>) -> El {
    let mut r = row()
        .key(key.to_owned())
        .role("listitem")
        .label(if detail.is_empty() { format!("{name}, {age}") } else { format!("{name}, {detail}, {age}") })
        .w_full()
        .min_h(t.control.sm + 4.0)
        .pl(t.space.at(3.0))
        .pr(t.space.at(1.0))
        .gap(t.space.at(2.0))
        .center_y()
        .radius(t.radius.lg)
        .round_as(t)
        .hover(t.c.hover)
        .child(text(name.to_owned()).font(t.ty.sm).color(t.ink()).truncate().shrink().min_w(0.0).a11y_hidden())
        .child(text(detail.to_owned()).font(t.ty.sm).color(t.ink_faint()).truncate().grow(1.0).shrink().min_w(0.0).a11y_hidden())
        .child(text(age.to_owned()).font(t.ty.sm).color(t.ink_faint()).a11y_hidden());
    if let Some((words, press)) = stop {
        r = r.child(icon_button(t, &format!("{key}-stop"), words, &crate::ui::icons::CLOSE, None, false, Some(press)));
    }
    r
}

/// Settings ▸ Agents: what Return does while the agent works.
pub fn settings_choice(s: &App, t: &Theme) -> Option<El> {
    let (_, settings) = s.runtimes.as_ref()?;
    let options = [("Return steers, ⌥Return queues", Some("The words reach the agent at its next step.")), ("Return queues, ⌥Return steers", Some("The words wait and start the next run when this one ends."))];
    let chosen = usize::from(settings.queue_first);
    Some(
        col()
            .key("settings-sending")
            .w_full()
            .gap(t.space.at(3.0))
            .child(text("While the agent works").role("heading").level(2).font(weight(t.ty.md, t.ty.semibold)).color(t.ink()))
            .child(crate::ui::radio_group::RadioGroup::<App>::new("settings-send", "What Return does while the agent works", &options, Some(chosen)).on_change(|s: &mut App, i| s.set_queue_first(i == 1)).build(t)),
    )
}

impl App {
    pub fn set_queue_first(&mut self, on: bool) {
        if let Some((_, s)) = &self.runtimes {
            let settings = neko_protocol::Settings { queue_first: on, ..s.clone() };
            self.send(Request::SetSettings { settings });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn how_long_reads_short() {
        assert_eq!(running_for(30_000, 0), "just started");
        assert_eq!(running_for(2 * 60_000 + 5, 0), "2m");
        assert_eq!(running_for(65 * 60_000, 0), "1h 5m");
        assert_eq!(running_for(60 * 60_000, 0), "1h");
    }

    #[test]
    fn return_steers_option_return_queues_and_command_return_interrupts_while_it_works() {
        assert_eq!(mode_of(false, false, true, false), Some(Some(SendMode::Steer)));
        assert_eq!(mode_of(false, true, true, false), Some(Some(SendMode::Queue)));
        assert_eq!(mode_of(true, false, true, false), Some(Some(SendMode::Interrupt)));
        // Settings swap the first two; interrupt is always ⌘Return.
        assert_eq!(mode_of(false, false, true, true), Some(Some(SendMode::Queue)));
        assert_eq!(mode_of(false, true, true, true), Some(Some(SendMode::Steer)));
        assert_eq!(mode_of(true, false, true, true), Some(Some(SendMode::Interrupt)));
        // Both modifiers are no mode of ours.
        assert_eq!(mode_of(true, true, true, false), None);
    }

    #[test]
    fn at_rest_command_return_sends_and_option_return_stays_a_new_line() {
        assert_eq!(mode_of(true, false, false, false), Some(None));
        assert_eq!(mode_of(false, true, false, false), None);
        assert_eq!(mode_of(false, false, false, false), None, "plain Return at rest is the composer's own submit");
    }

    #[test]
    fn a_row_shows_the_first_line_of_the_message() {
        assert_eq!(first_line("\n  Check the docs\nthen the tests"), "Check the docs");
        assert_eq!(first_line("   "), "");
    }

    #[test]
    fn the_summary_counts_what_runs() {
        assert_eq!(summary(2, 1), "2 kittens working · 1 command running");
        assert_eq!(summary(1, 0), "1 kitten working");
        assert_eq!(summary(0, 3), "3 commands running");
    }
}
