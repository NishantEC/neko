//! Question 2: Neko-scale chat on Rune. A two-pane window: 500 tasks on the left (virtualised, with
//! collapsible groups and a right-click menu), a streaming markdown chat and a composer on the right.
//!
//! Flags (after `--`-less argv): `bench` (idle, stream, stream while scrolling, scroll, idle),
//! `leak` (10 stream cycles, memory after each; `fast=N` multiplies the token rate, `keep` keeps
//! the history), `rate=N` (tokens per second, default 60), `stream` (start one stream at once).

mod fake;
mod stats;

use std::borrow::Cow;
use std::time::Duration;

use fake::{GROUPS, Status, Task};
use rune::{AppSpec, Chrome, ClipboardContent, Cx, El, GrowthSpec, PointerPress, ScrollSpec, Spring, TextEdit, Tx, col, el, memo, row, text};
use rune_kit::shell::{PaneMotion, PaneState, Sidebar, SidebarHeader, SidebarRow, SplitView, Toolbar};
use rune_kit::{Button, ButtonVariant, ControlSize, Kit, Markdown, MenuItem, TextArea, context_menu, root};

thread_local! {
    static KIT: Kit = Kit::neutral(true);
}

fn arg(name: &str) -> Option<String> {
    rune::extra_args().iter().find_map(|a| a.strip_prefix(&format!("{name}=")).map(str::to_owned)).or_else(|| rune::extra_args().iter().any(|a| a == name).then(String::new))
}

#[derive(Clone, PartialEq)]
enum Line {
    Header(usize),
    Task(usize),
}

struct Msg {
    id: u64,
    user: bool,
    text: String,
}

struct Stream {
    start: f64,
    ends: Vec<usize>,
    pos: usize,
    rate: f64,
    text: String,
}

struct Attachment {
    id: u32,
    size: (u32, u32),
    kb: usize,
}

#[derive(Default)]
struct Bench {
    mode: u8, // 0 off, 1 bench, 2 leak
    t0: f64,
    step: u32,
    t_mark: f64,
    cycles: u32,
    last_scroll: f64,
    stream_start_pending: bool,
}

struct App {
    sidebar: PaneState,
    tasks: Vec<Task>,
    collapsed: [bool; 5],
    lines: Vec<Line>,
    lines_ver: u64,
    selected: usize,
    point: Option<PointerPress>,
    menu_task: usize,
    messages: Vec<Msg>,
    next_id: u64,
    reply: String,
    ends: Vec<usize>,
    stream: Option<Stream>,
    composer: TextEdit,
    attachments: Vec<Attachment>,
    pasted: u32,
    rate: f64,
    bench: Bench,
    note: String,
}

fn lines_for(tasks: &[Task], collapsed: &[bool; 5]) -> Vec<Line> {
    let mut out = Vec::with_capacity(tasks.len() + GROUPS.len());
    for (g, (status, _)) in GROUPS.iter().enumerate() {
        out.push(Line::Header(g));
        if !collapsed[g] {
            out.extend(tasks.iter().enumerate().filter(|(_, t)| t.status == *status).map(|(i, _)| Line::Task(i)));
        }
    }
    out
}

fn initial_messages() -> (Vec<Msg>, u64) {
    let m = |id, user, t: &str| Msg { id, user, text: t.to_owned() };
    (
        vec![
            m(1, true, "Move the checkpoint undo into its own module and keep the tool names."),
            m(2, false, "Done. I split `checkpoints.rs` into **three files** and kept `checkpoint_diff` and `undo_run`:\n\n- `store.rs` owns the refs\n- `diff.rs` renders per-run diffs\n- `undo.rs` refuses when the tree is dirty\n\nTests pass (`cargo test -p neko-core`)."),
            m(3, true, "Good. Now draft the plan for moving the web UI to a thin shell."),
        ],
        4,
    )
}

fn init() -> App {
    let tasks = fake::tasks(500);
    let collapsed = [false; 5];
    let lines = lines_for(&tasks, &collapsed);
    let (messages, next_id) = initial_messages();
    let reply = fake::reply(4000);
    let ends = fake::token_ends(&reply);
    let mode = if arg("bench").is_some() { 1 } else if arg("leak").is_some() { 2 } else { 0 };
    let fast: f64 = arg("fast").and_then(|v| v.parse().ok()).unwrap_or(1.0);
    let rate: f64 = arg("rate").and_then(|v| v.parse().ok()).unwrap_or(60.0) * fast;
    let mut composer = TextEdit::new("");
    composer.focused = false;
    let mut app = App {
        sidebar: PaneState::new(320.0).range(220.0, 520.0),
        tasks,
        collapsed,
        lines,
        lines_ver: 0,
        selected: 0,
        point: None,
        menu_task: 0,
        messages,
        next_id,
        reply,
        ends,
        stream: None,
        composer,
        attachments: Vec::new(),
        pasted: 0,
        rate,
        bench: Bench { mode, ..Bench::default() },
        note: String::new(),
    };
    if mode == 0 && arg("stream").is_some() {
        app.bench.stream_start_pending = true;
    }
    app
}

fn glyph(status: Status) -> El {
    let c = match status {
        Status::NeedsYou => rune::oklch(0.78, 0.15, 75.0),
        Status::Running => rune::oklch(0.72, 0.14, 250.0),
        Status::Queued => rune::oklch(0.6, 0.02, 250.0),
        Status::Done => rune::oklch(0.74, 0.14, 150.0),
        Status::Failed => rune::oklch(0.66, 0.2, 25.0),
    };
    el().size(9.0, 9.0).radius(4.5).fill(c)
}

fn line_key(s: &App, i: usize) -> Cow<'_, str> {
    match &s.lines[i] {
        Line::Header(g) => Cow::Owned(format!("h{g}")),
        Line::Task(t) => Cow::Borrowed(s.tasks[*t].id.as_str()),
    }
}

fn line_row(s: &App, i: usize) -> El {
    KIT.with(|k| match &s.lines[i] {
        Line::Header(g) => {
            let g = *g;
            let n = s.tasks.iter().filter(|t| t.status == GROUPS[g].0).count();
            SidebarHeader::new(GROUPS[g].1)
                .count(n)
                .collapsed(s.collapsed[g])
                .on_toggle(move |s: &mut App, _| {
                    s.collapsed[g] = !s.collapsed[g];
                    s.lines = lines_for(&s.tasks, &s.collapsed);
                    s.lines_ver += 1;
                })
                .build(k)
                .key(format!("h{g}"))
        }
        Line::Task(t) => {
            let task = &s.tasks[*t];
            let idx = *t;
            SidebarRow::new(task.title.clone())
                .height(45.0)
                .avatar(glyph(task.status))
                .subtitle(task.id.clone())
                .detail(task.meta.clone())
                .selected(s.selected == idx)
                .on_select(move |s: &mut App, _| s.selected = idx)
                .build(k)
                .key(task.id.clone())
                .on_secondary_press(move |s: &mut App, p, _| {
                    s.menu_task = idx;
                    s.point = Some(p);
                })
        }
    })
}

fn message(m: &Msg, k: &Kit) -> El {
    if m.user {
        row()
            .w_full()
            .justify_end()
            .child(col().max_w(560.0).px(14.0).py(10.0).radius(12.0).fill(k.c.layer_01).child(text(m.text.clone()).font(k.ty.body).color(k.c.text_base).wrap()))
    } else {
        col().w_full().max_w(760.0).child(Markdown::new(m.text.clone()).build(k))
    }
}

fn chat_row(s: &App, i: usize) -> El {
    KIT.with(|k| {
        let wrap = |e: El| col().w_full().px(24.0).py(8.0).child(e);
        if i < s.messages.len() {
            let m = &s.messages[i];
            // A finished message never changes: build it once.
            memo(&format!("m{}", m.id), m.text.len(), || wrap(message(m, k)))
        } else if let Some(st) = &s.stream {
            wrap(col().w_full().max_w(760.0).child(Markdown::new(st.text.clone()).streaming(true).build(k)))
        } else {
            el()
        }
    })
}

fn chat_key(s: &App, i: usize) -> Cow<'_, str> {
    if i < s.messages.len() { Cow::Owned(format!("m{}", s.messages[i].id)) } else { Cow::Borrowed("streaming") }
}

fn send(s: &mut App, tx: &mut Tx) {
    let t = s.composer.text.trim().to_owned();
    if t.is_empty() && s.attachments.is_empty() {
        return;
    }
    let mut body = t;
    for a in s.attachments.drain(..) {
        body.push_str(&format!("\n[image {}×{}]", a.size.0, a.size.1));
    }
    s.messages.push(Msg { id: s.next_id, user: true, text: body });
    s.next_id += 1;
    s.composer.clear();
    start_stream(s, 0.0);
    tx.scroll_into_view("streaming");
}

fn start_stream(s: &mut App, now: f64) {
    s.stream = Some(Stream { start: now, ends: s.ends.clone(), pos: 0, rate: s.rate, text: String::new() });
}

fn view(s: &App, cx: &mut Cx<App>) -> El {
    KIT.with(|k| view_with(s, cx, k))
}

fn view_with(s: &App, cx: &mut Cx<App>, k: &Kit) -> El {
    let c = &k.c;
    let mut layer = Vec::new();
    // --- left pane
    let list = Sidebar::new()
        .label("Tasks")
        .inset(0.0)
        .header_gap(4.0)
        .header_padding(0.0, 12.0)
        .row_gap(0.0)
        .row_height(45.0)
        .virtual_rows(s.lines.len(), line_key, line_row)
        .build(k);
    let menu = context_menu("task-menu", list, s.point)
        .item(MenuItem::new("Open").on_select(|s: &mut App| s.point = None))
        .item(MenuItem::new("Copy id").shortcut("⌘C").on_select(|s: &mut App| s.point = None))
        .item(MenuItem::new("Rename…").on_select(|s: &mut App| s.point = None))
        .separator()
        .item(MenuItem::new("Cancel task").danger(true).on_select(|s: &mut App| s.point = None))
        .on_close(|s: &mut App| s.point = None)
        .build(k, &mut layer);

    // --- right pane: transcript + composer
    let transcript = col().key("transcript").grow(1.0).min_h(0.0).scroll(ScrollSpec { stick_bottom: true }).child(
        col().key("msgs").w_full().growth(GrowthSpec::default()).virtual_rows(
            s.messages.len() + s.stream.is_some() as usize,
            160.0,
            chat_key,
            chat_row,
        ),
    );
    let mut chips = row().gap(6.0).px(24.0);
    for a in &s.attachments {
        let id = a.id;
        chips = chips.child(
            row().gap(6.0).px(8.0).py(4.0).radius(8.0).fill(c.layer_01).center_y()
                .child(text(format!("Image {}×{} · {} KB", a.size.0, a.size.1, a.kb)).font(k.ty.small).color(c.text_muted))
                .child(Button::new("×").variant(ButtonVariant::Ghost).control_size(ControlSize::Small).on_press(move |s: &mut App| s.attachments.retain(|x| x.id != id)).build(k).key(format!("chip-x{id}"))),
        );
    }
    let composer = TextArea::new(&s.composer)
        .placeholder("Message Neko — ⌘↩ to send, paste an image")
        .auto_grow(2, 8)
        .on_edit(|s: &mut App| &mut s.composer)
        .on_submit(send)
        .build(k)
        .key("composer")
        .on_input_paste(|s: &mut App, content: &ClipboardContent, _| {
            let Some(image) = content.image() else { return false };
            s.pasted += 1;
            s.attachments.push(Attachment { id: s.pasted, size: image.size(), kb: image.png().len() / 1024 });
            true
        });
    let bottom = col().w_full().px(0.0).py(12.0).gap(8.0).child(chips).child(row().px(24.0).gap(8.0).items_end().child(col().grow(1.0).w_full().child(composer)).child(
        Button::new("Send").variant(ButtonVariant::Contrast).on_press_tx(send).build(k).key("send"),
    ));
    let main = col().key("chat").w_full().h_full().child(transcript).child(bottom);

    let toolbar = Toolbar::new().title(if s.stream.is_some() { "Streaming…".to_owned() } else { format!("{} · {}", s.tasks[s.selected].id, s.tasks[s.selected].title) });
    let split = SplitView::new(main)
        .sidebar(&s.sidebar, |s: &mut App| &mut s.sidebar, menu)
        .toolbar(toolbar)
        .titlebar_inset(cx.titlebar_inset())
        .frame_inset(8.0)
        .toolbar_in_panel()
        .sidebar_padding(18.0, 10.0)
        .pane_motion(PaneMotion::new().spring(Spring::new(0.3, 0.0)))
        .reduced_motion(cx.is_reduced_motion())
        .build(k);
    root(split, layer)
}

// ---------------------------------------------------------------------------------------------
// poll: the token stream and the bench driver
// ---------------------------------------------------------------------------------------------

fn sweep(s: &mut App, tx: &mut Tx, t: f64) {
    if t - s.bench.last_scroll < 0.05 {
        return;
    }
    s.bench.last_scroll = t;
    let n = s.lines.len().max(2) as f64;
    let pos = (t * 60.0) % (2.0 * n);
    let idx = if pos < n { pos } else { 2.0 * n - pos } as usize;
    let key = line_key(s, idx.min(s.lines.len() - 1)).into_owned();
    tx.scroll_into_view_in("sidebar-list", key);
}

fn poll(s: &mut App, t: f64, tx: &mut Tx) -> bool {
    let mut changed = false;
    if s.bench.stream_start_pending {
        s.bench.stream_start_pending = false;
        start_stream(s, t);
        changed = true;
    }
    // --- advance the stream
    let mut finished = false;
    if let Some(st) = &mut s.stream {
        if st.start == 0.0 {
            st.start = t;
        }
        let want = (((t - st.start) * st.rate) as usize).min(st.ends.len());
        if want > st.pos {
            let from = if st.pos == 0 { 0 } else { st.ends[st.pos - 1] };
            st.text.push_str(&s.reply[from..st.ends[want - 1]]);
            st.pos = want;
            changed = true;
        }
        finished = st.pos >= st.ends.len();
    }
    if finished {
        let st = s.stream.take().unwrap();
        s.messages.push(Msg { id: s.next_id, user: false, text: st.text });
        s.next_id += 1;
        changed = true;
    }
    changed |= drive_bench(s, t, tx, finished);
    if s.stream.is_some() || s.bench.mode != 0 {
        tx.wake_after(Duration::from_millis(16));
    }
    changed
}

fn write_results(label: &str) {
    let json = stats::json(label);
    let _ = std::fs::create_dir_all("results");
    let _ = std::fs::write(format!("results/{label}.json"), &json);
    println!("{json}");
}

fn drive_bench(s: &mut App, t: f64, tx: &mut Tx, stream_finished: bool) -> bool {
    match s.bench.mode {
        1 => bench_run(s, t, tx, stream_finished),
        2 => leak_run(s, t, tx, stream_finished),
        _ => false,
    }
}

fn bench_run(s: &mut App, t: f64, tx: &mut Tx, stream_finished: bool) -> bool {
    if s.bench.t0 == 0.0 {
        s.bench.t0 = t;
        stats::set_phase("warmup");
    }
    let dt = t - s.bench.t0;
    match s.bench.step {
        0 if dt > 3.0 => {
            stats::memory("after-warmup");
            stats::set_phase("idle");
            s.bench.step = 1;
            s.bench.t_mark = t;
        }
        1 if t - s.bench.t_mark > 4.0 => {
            stats::memory("idle");
            start_stream(s, t);
            stats::set_phase("stream");
            s.bench.step = 2;
            s.bench.t_mark = t;
            return true;
        }
        2 => {
            let el = t - s.bench.t_mark;
            if stream_finished {
                stats::memory("stream-end");
                s.bench.step = 3;
                s.bench.t_mark = t;
                stats::set_phase("after-stream-idle");
            } else if (20.0..35.0).contains(&el) {
                stats::set_phase("stream+scroll");
                sweep(s, tx, t);
            } else {
                stats::set_phase("stream");
            }
        }
        3 if t - s.bench.t_mark > 3.0 => {
            stats::memory("stream-end+3s");
            stats::set_phase("scroll");
            s.bench.step = 4;
            s.bench.t_mark = t;
        }
        4 => {
            sweep(s, tx, t);
            if t - s.bench.t_mark > 12.0 {
                stats::set_phase("idle-after");
                s.bench.step = 5;
                s.bench.t_mark = t;
            }
        }
        5 if t - s.bench.t_mark > 4.0 => {
            stats::memory("final-idle");
            write_results(&arg("name").unwrap_or_else(|| "bench".into()));
            tx.quit();
            s.bench.step = 6;
        }
        _ => {}
    }
    false
}

fn leak_run(s: &mut App, t: f64, tx: &mut Tx, stream_finished: bool) -> bool {
    if s.bench.t0 == 0.0 {
        s.bench.t0 = t;
        stats::set_phase("leak");
    }
    let dt = t - s.bench.t0;
    match s.bench.step {
        0 if dt > 3.0 => {
            stats::memory("baseline");
            start_stream(s, t);
            s.bench.step = 1;
            return true;
        }
        1 if stream_finished => {
            s.bench.cycles += 1;
            s.bench.step = 2;
            s.bench.t_mark = t;
        }
        2 if t - s.bench.t_mark > 1.5 => {
            stats::memory(&format!("cycle-{}", s.bench.cycles));
            if s.bench.cycles >= 10 {
                write_results(&arg("name").unwrap_or_else(|| "leak".into()));
                tx.quit();
                s.bench.step = 9;
                return false;
            }
            if arg("keep").is_none() {
                let (m, id) = initial_messages();
                s.messages = m;
                s.next_id = id;
            }
            start_stream(s, t);
            s.bench.step = 1;
            return true;
        }
        _ => {}
    }
    false
}

fn main() {
    let chrome = Chrome { full_size_content: true, traffic_lights: Some((16.0, 25.0)), drag_regions: true };
    let spec = AppSpec {
        poll: Some(poll),
        scenes: &[
            ("half-stream", |s| {
                let mid = s.ends[s.ends.len() / 4];
                s.stream = Some(Stream { start: -1.0e9, ends: vec![mid, mid + 1], pos: 1, rate: 0.0, text: s.reply[..mid].to_owned() });
            }),
            ("collapsed", |s| {
                s.collapsed = [false, true, true, true, true];
                s.lines = lines_for(&s.tasks, &s.collapsed);
            }),
            ("menu", |s| {
                s.point = Some(PointerPress { x: 200.0, y: 180.0 });
            }),
        ],
        min_size: Some((720.0, 480.0)),
        ..AppSpec::new("Neko on Rune", (1180.0, 760.0), init, view)
    };
    rune::App::new(spec).chrome(chrome).on_frame_encoded(stats::frame).run();
}
