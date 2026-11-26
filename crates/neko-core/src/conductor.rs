//! One run's book (docs/knowledge/transcript.md): the agent's events become the task's stored transcript and the changes pushed to
//! screens.
//!
//! Words stream to screens at once ([`Out::Delta`]) but reach the disk at most every [`FLUSH_EVERY_MS`] and when their part ends:
//! the earlier project wrote 43 GB in 12 hours by storing every tick. Tool calls, permission requests and notices are written as
//! they happen. Logic only: the clock is handed in, and the caller does the I/O of running the agent and of git.

use std::collections::HashMap;

use neko_protocol::{
    Cat,
    Change, Changes, Decision, FailureKind, Part, PartBody, PartId, Permission, PermissionId, PermissionMode, PermissionState, Pricing, Run, RunId, RunStatus, Task, TaskId, TaskStatus, Tone, ToolState, Usage,
};

use crate::policy::{self, Verdict};
use crate::records::Place;
use crate::runtime::{Action, AgentEvent, Answer, Failure, Outcome};
use crate::{Core, StoreError};

/// The longest a part's words wait in memory before they are written.
pub const FLUSH_EVERY_MS: i64 = 3_000;

/// What the caller does after an event.
#[derive(Clone, Debug, PartialEq)]
pub enum Out {
    /// Push this stored change (with its revision) to every subscriber.
    Change(u64, Change),
    /// Push these words to screens watching the task (not stored yet).
    Delta { part: PartId, text: String },
    /// Answer the runtime's request now.
    Answer { request: String, answer: Answer },
    /// Answer a waiting hook helper ("Ask before risky steps"): its request id (`hook:…`), the answer and why, in words for the
    /// agent.
    HookAnswer { request: String, answer: Answer, why: String },
    /// Tell screens watching the task whether the agent is thinking (not stored).
    Activity { thinking: bool },
}

/// The prefix of a request that came through the run's hook, not the CLI's own control channel.
pub const HOOK: &str = "hook:";

/// Why a hook's step was denied, in words the agent reads.
pub const DENIED: &str = "The person said no to this step in Neko. Do not try it another way; say what you would have done instead.";
pub const RUN_ENDED: &str = "This run ended before the person answered in Neko, so the step did not run.";
pub const EXPIRED: &str = "No one answered in Neko in time, so the step did not run.";

fn answer_out(request: String, answer: Answer, why: &str) -> Out {
    if request.starts_with(HOOK) { Out::HookAnswer { request, answer, why: why.to_owned() } } else { Out::Answer { request, answer } }
}

/// What the runtime is, for the words of a notice.
#[derive(Clone, Debug)]
pub struct Who {
    pub id: String,
    pub name: String,
    pub sign_in: String,
    pub install: String,
    pub pricing: Pricing,
}

/// Why an answer was refused.
#[derive(Debug)]
pub enum AnswerError {
    NotFound,
    /// Already answered, or expired.
    Settled,
    Store(StoreError),
}

impl From<StoreError> for AnswerError {
    fn from(e: StoreError) -> Self {
        AnswerError::Store(e)
    }
}

/// The book of one run.
pub struct RunBook {
    pub run: Run,
    task: TaskId,
    who: Who,
    place: Place,
    mode: PermissionMode,
    /// The part whose words are arriving, and whether they changed since they were last written.
    open: Option<(Part, bool)>,
    last_flush: i64,
    tools: HashMap<String, Part>,
    /// Requests put to the person: Neko's id → the runtime's (or the hook's).
    asked: HashMap<PermissionId, String>,
    /// Requests (the hook's and the CLI's) and the tool calls they are for.
    hook_calls: HashMap<String, String>,
    /// Tool calls the person said no to (or no one answered): they end as denied, not failed.
    denied: std::collections::HashSet<String>,
    /// Whether the agent is thinking now, as screens were last told.
    thinking: bool,
    /// Tool calls the hook let go ahead: Claude Code's own ask for one of them is answered yes at once.
    approved: std::collections::HashSet<String>,
    /// Bytes of part bodies written so far (for the write-rate check).
    pub written: u64,
    pub writes: u32,
}

fn part_kind_open(b: &PartBody) -> bool {
    matches!(b, PartBody::Agent { .. } | PartBody::Thinking { .. })
}

impl RunBook {
    /// Stores the new run and the task's new status. `words` (the person's) is stored first as their part, unless None. `cat` is
    /// the task's cat: the run records it and the version of its rules, and when those rules differ from the ones the task's last
    /// run used, one quiet part says so first.
    #[allow(clippy::too_many_arguments)]
    pub fn begin(core: &mut Core, task: &TaskId, who: Who, place: Place, mode: PermissionMode, resume: Option<String>, words: Option<&str>, cat: Option<&Cat>) -> Result<(RunBook, Vec<Out>), StoreError> {
        let now = core.now();
        let before = core.store.runs_of(task)?.into_iter().rev().find_map(|r| r.rules_hash);
        let changed = cat.filter(|c| before.as_ref().is_some_and(|h| *h != c.rules_hash));
        let run = Run {
            id: RunId(core.new_id("run")),
            task: task.clone(),
            runtime: who.id.clone(),
            model: None,
            status: RunStatus::Running,
            started_ms: now,
            ended_ms: None,
            session: resume,
            workdir: place.workdir.clone(),
            branch: place.branch.clone(),
            reason: None,
            usage: None,
            changes: None,
            undo: None,
            cat: cat.map(|c| c.id.clone()),
            rules_hash: cat.map(|c| c.rules_hash.clone()),
        };
        let mut out = Vec::new();
        let rev = core.store.insert_run(&run, now)?;
        out.push(Out::Change(rev, Change::RunChanged { run: run.clone() }));
        let mut book = RunBook { run, task: task.clone(), who, place, mode, open: None, last_flush: now, tools: HashMap::new(), asked: HashMap::new(), hook_calls: HashMap::new(), denied: Default::default(), thinking: false, approved: Default::default(), written: 0, writes: 0 };
        if let Some(c) = changed {
            let p = book.new_part(core, PartBody::RulesChanged { cat: c.id.clone(), name: c.name.clone() }, now, true)?;
            out.extend(p);
        }
        if let Some(w) = words {
            let p = book.new_part(core, PartBody::Person { text: w.to_owned() }, now, true)?;
            out.extend(p);
        }
        out.extend(book.task_status(core, TaskStatus::Working, now)?);
        Ok((book, out))
    }

    /// Writes the run's own fields and reads back where its undo stands (the daemon keeps that), so what is pushed is whole.
    fn write_run(&mut self, core: &mut Core, now: i64) -> Result<u64, StoreError> {
        let rev = core.store.update_run(&self.run, now)?;
        self.run.undo = core.store.get_run(&self.run.id)?.and_then(|r| r.undo);
        Ok(rev)
    }

    fn task_status(&self, core: &mut Core, s: TaskStatus, now: i64) -> Result<Vec<Out>, StoreError> {
        let current = core.store.get_task(&self.task)?.map(|t| t.status);
        if current == Some(s) {
            return Ok(Vec::new());
        }
        let (task, rev) = core.store.set_task_status(&self.task, s, now)?;
        Ok(vec![Out::Change(rev, Change::TaskUpdated { task })])
    }

    fn new_part(&mut self, core: &mut Core, body: PartBody, now: i64, ended: bool) -> Result<Vec<Out>, StoreError> {
        let part = Part {
            id: PartId(core.new_id("part")),
            task: self.task.clone(),
            run: Some(self.run.id.clone()),
            seq: core.store.next_part_seq(&self.task)?,
            at_ms: now,
            ended_ms: ended.then_some(now),
            body,
        };
        let rev = core.store.insert_part(&part, now)?;
        self.count(&part);
        let out = vec![Out::Change(rev, Change::PartChanged { part: part.clone() })];
        if !ended {
            if part_kind_open(&part.body) {
                self.open = Some((part, false));
            } else if let PartBody::Tool { call, .. } = &part.body {
                self.tools.insert(call.clone(), part);
            }
        }
        Ok(out)
    }

    fn count(&mut self, p: &Part) {
        self.writes += 1;
        self.written += serde_json::to_string(&p.body).map_or(0, |s| s.len() as u64);
    }

    /// The part words are streaming into, with every word so far (the store may lag it by the write rate).
    pub fn open_part(&self) -> Option<&Part> {
        self.open.as_ref().map(|(p, _)| p)
    }

    /// Writes every word the open part holds now, whatever the write rate (the daemon is stopping: what the person saw is kept).
    pub fn save_words(&mut self, core: &mut Core, now: i64) -> Result<Vec<Out>, StoreError> {
        self.flush(core, now, false)
    }

    /// Writes the open part's words if they changed; `end` closes it (and pushes it whole).
    fn flush(&mut self, core: &mut Core, now: i64, end: bool) -> Result<Vec<Out>, StoreError> {
        let mut out = Vec::new();
        let Some((mut part, dirty)) = self.open.take() else { return Ok(out) };
        if end {
            match &mut part.body {
                PartBody::Agent { done, .. } | PartBody::Thinking { done, .. } => *done = true,
                _ => {}
            }
            part.ended_ms = Some(now);
        }
        if dirty || end {
            let rev = core.store.update_part(&part, now)?;
            self.count(&part);
            self.last_flush = now;
            if end {
                out.push(Out::Change(rev, Change::PartChanged { part: part.clone() }));
            }
        }
        if !end {
            self.open = Some((part, false));
        }
        Ok(out)
    }

    /// Appends streamed words, opening a part of the right kind first.
    fn words(&mut self, core: &mut Core, text: String, thinking: bool, now: i64) -> Result<Vec<Out>, StoreError> {
        let mut out = Vec::new();
        let same = matches!((&self.open, thinking), (Some((Part { body: PartBody::Thinking { .. }, .. }, _)), true) | (Some((Part { body: PartBody::Agent { .. }, .. }, _)), false));
        if !same {
            out.extend(self.flush(core, now, true)?);
            let body = if thinking { PartBody::Thinking { text: String::new(), done: false } } else { PartBody::Agent { text: String::new(), done: false } };
            out.extend(self.new_part(core, body, now, false)?);
        }
        if let Some((part, dirty)) = &mut self.open {
            match &mut part.body {
                PartBody::Agent { text: t, .. } | PartBody::Thinking { text: t, .. } => t.push_str(&text),
                _ => {}
            }
            *dirty = true;
            out.push(Out::Delta { part: part.id.clone(), text });
        }
        if now - self.last_flush >= FLUSH_EVERY_MS {
            out.extend(self.flush(core, now, false)?);
        }
        Ok(out)
    }

    /// Writes words that waited long enough. Call about once a second.
    pub fn tick(&mut self, core: &mut Core, now: i64) -> Result<Vec<Out>, StoreError> {
        if now - self.last_flush >= FLUSH_EVERY_MS { self.flush(core, now, false) } else { Ok(Vec::new()) }
    }

    /// The folder the run works in.
    pub fn workdir(&self) -> &str {
        &self.place.workdir
    }

    /// The step a tool call is, in a few words ("Bash cargo test"), while it runs or after.
    pub fn step_words(&self, call: &str) -> Option<String> {
        self.tools.get(call).and_then(|p| if let PartBody::Tool { name, summary, .. } = &p.body { Some(format!("{name} {summary}").trim().to_owned()) } else { None })
    }

    /// Whether a request is waiting for the person.
    pub fn waiting(&self) -> bool {
        !self.asked.is_empty()
    }

    /// One event from the runtime. `Finished` is [`RunBook::finish`]'s.
    pub fn on(&mut self, core: &mut Core, ev: AgentEvent, now: i64) -> Result<Vec<Out>, StoreError> {
        let mut out = Vec::new();
        // Anything else that arrives ends the thinking screens show.
        let thinks = matches!(ev, AgentEvent::ThinkingStarted | AgentEvent::ThinkingDelta(_) | AgentEvent::Usage(_) | AgentEvent::Unchecked { .. } | AgentEvent::Started { .. });
        if self.thinking && !thinks {
            self.thinking = false;
            out.push(Out::Activity { thinking: false });
        }
        match ev {
            AgentEvent::ThinkingStarted => {
                if !self.thinking {
                    self.thinking = true;
                    out.push(Out::Activity { thinking: true });
                }
            }
            AgentEvent::Unchecked { .. } => {}
            AgentEvent::Started { session, model } => {
                if session.is_some() {
                    self.run.session = session;
                }
                self.run.model = model.or(self.run.model.take());
                let rev = self.write_run(core, now)?;
                out.push(Out::Change(rev, Change::RunChanged { run: self.run.clone() }));
            }
            AgentEvent::TextDelta(t) => out.extend(self.words(core, t, false, now)?),
            AgentEvent::ThinkingDelta(t) => out.extend(self.words(core, t, true, now)?),
            AgentEvent::TextDone | AgentEvent::ThinkingDone => out.extend(self.flush(core, now, true)?),
            AgentEvent::ToolStarted { id, name, summary, input } => {
                out.extend(self.flush(core, now, true)?);
                out.extend(self.new_part(core, PartBody::Tool { call: id, name, summary, input, output: None, state: ToolState::Running, exit: None }, now, false)?);
            }
            AgentEvent::ToolFinished { id, ok, output, exit } => {
                if let Some(mut part) = self.tools.remove(&id) {
                    if let PartBody::Tool { output: o, state, exit: e, .. } = &mut part.body {
                        let said_no = self.denied.contains(&id);
                        *o = Some(output);
                        *state = if ok { ToolState::Done } else if said_no { ToolState::Denied } else { ToolState::Failed };
                        *e = exit;
                    }
                    part.ended_ms = Some(now);
                    let rev = core.store.update_part(&part, now)?;
                    self.count(&part);
                    out.push(Out::Change(rev, Change::PartChanged { part }));
                }
            }
            AgentEvent::PermissionAsked { id, tool, action, tool_use } => out.extend(self.asked(core, id, tool, action, tool_use, now)?),
            AgentEvent::PermissionWithdrawn { id } => {
                // A hook's step no one answered is denied; a step the CLI took back is not.
                if let Some(t) = self.hook_calls.remove(&id)
                    && id.starts_with(HOOK)
                {
                    self.denied.insert(t);
                }
                if let Some((perm, _)) = self.asked.iter().find(|(_, r)| **r == id).map(|(p, r)| (p.clone(), r.clone())) {
                    self.asked.remove(&perm);
                    if let Some((p, rev)) = core.store.settle_permission(&perm, PermissionState::Expired, now)? {
                        out.push(Out::Change(rev, Change::PermissionChanged { permission: p }));
                    }
                    if self.asked.is_empty() {
                        out.extend(self.task_status(core, TaskStatus::Working, now)?);
                    }
                }
            }
            AgentEvent::Usage(r) => {
                self.run.usage = Some(Usage {
                    input_tokens: r.input_tokens,
                    output_tokens: r.output_tokens,
                    cached_tokens: r.cached_tokens,
                    cost_usd: r.cost_usd,
                    model: r.model.clone(),
                    pricing: self.who.pricing,
                    reported_by: self.who.name.clone(),
                    context_tokens: r.context_tokens,
                    context_window: r.context_window,
                });
                if self.run.model.is_none() {
                    self.run.model = r.model;
                }
                let rev = self.write_run(core, now)?;
                out.push(Out::Change(rev, Change::RunChanged { run: self.run.clone() }));
            }
            AgentEvent::Finished(o) => out.extend(self.finish(core, o, None, now)?),
        }
        Ok(out)
    }

    /// A permission request, from the CLI or from the run's hook: decided by the policy, or stored and put to the person.
    fn asked(&mut self, core: &mut Core, request: String, tool: String, action: Action, tool_use: Option<String>, now: i64) -> Result<Vec<Out>, StoreError> {
        let hook = request.starts_with(HOOK);
        // The hook already let this call go ahead (the person said yes, or Neko's policy did): Claude Code's own ask is answered.
        if !hook && tool_use.as_ref().is_some_and(|t| self.approved.contains(t)) {
            return Ok(vec![Out::Answer { request, answer: Answer::Allow }]);
        }
        let always = core.store.always_rules(&self.task)?;
        let cx = policy::Context { workdir: std::path::Path::new(&self.place.workdir), mode: self.mode, plain: self.place.plain, always: &always };
        let ask = match policy::decide(&action, &cx) {
            Verdict::Allow => {
                if hook && let Some(t) = &tool_use {
                    self.approved.insert(t.clone());
                }
                return Ok(vec![answer_out(request, Answer::Allow, "Neko's policy allows this step.")]);
            }
            Verdict::Ask(a) => a,
        };
        if let Some(t) = &tool_use {
            self.hook_calls.insert(request.clone(), t.clone());
        }
        let mut out = self.flush(core, now, true)?;
        let perm = Permission {
            id: PermissionId(core.new_id("perm")),
            task: self.task.clone(),
            run: self.run.id.clone(),
            tool,
            title: ask.title,
            why: ask.why,
            detail: ask.detail,
            destructive: ask.destructive,
            state: PermissionState::Pending,
            asked_ms: now,
            answered_ms: None,
        };
        let rev = core.store.insert_permission(&perm, &ask.rule, now)?;
        out.push(Out::Change(rev, Change::PermissionChanged { permission: perm.clone() }));
        out.extend(self.new_part(core, PartBody::Permission { permission: perm.id.clone() }, now, true)?);
        self.asked.insert(perm.id.clone(), request);
        out.extend(self.task_status(core, TaskStatus::NeedsYou, now)?);
        Ok(out)
    }

    /// The person's answer. Refused when it was answered already or has expired.
    pub fn answer(&mut self, core: &mut Core, perm: &PermissionId, d: Decision, now: i64) -> Result<Vec<Out>, AnswerError> {
        let Some(p) = core.store.get_permission(perm)? else { return Err(AnswerError::NotFound) };
        let state = match d {
            Decision::AllowOnce => PermissionState::Allowed,
            // A destructive step is never allowed always: it counts once.
            Decision::AlwaysAllow if p.destructive => PermissionState::Allowed,
            Decision::AlwaysAllow => PermissionState::AlwaysAllowed,
            Decision::Deny => PermissionState::Denied,
        };
        let Some((settled, rev)) = core.store.settle_permission(perm, state, now)? else { return Err(AnswerError::Settled) };
        let mut out = vec![Out::Change(rev, Change::PermissionChanged { permission: settled })];
        if let Some(request) = self.asked.remove(perm) {
            let answer = if d == Decision::Deny { Answer::Deny } else { Answer::Allow };
            if let Some(t) = self.hook_calls.remove(&request) {
                if answer == Answer::Allow {
                    if request.starts_with(HOOK) {
                        self.approved.insert(t);
                    }
                } else {
                    self.denied.insert(t);
                }
            }
            out.push(answer_out(request, answer, if answer == Answer::Allow { "The person allowed this step in Neko." } else { DENIED }));
        }
        if self.asked.is_empty() {
            out.extend(self.task_status(core, TaskStatus::Working, now)?);
        }
        Ok(out)
    }

    /// The end: open parts close, running tools fail, waiting requests expire, the run and the task take their final state, and a
    /// failure leaves a notice that says what to do next.
    pub fn finish(&mut self, core: &mut Core, outcome: Outcome, changes: Option<Changes>, now: i64) -> Result<Vec<Out>, StoreError> {
        let mut out = self.flush(core, now, true)?;
        if self.thinking {
            self.thinking = false;
            out.push(Out::Activity { thinking: false });
        }
        for (_, mut part) in std::mem::take(&mut self.tools) {
            if let PartBody::Tool { state, .. } = &mut part.body {
                *state = ToolState::Failed;
            }
            part.ended_ms = Some(now);
            let rev = core.store.update_part(&part, now)?;
            out.push(Out::Change(rev, Change::PartChanged { part }));
        }
        for p in core.store.pending_of_run(&self.run.id)? {
            if let Some((p, rev)) = core.store.settle_permission(&p.id, PermissionState::Expired, now)? {
                out.push(Out::Change(rev, Change::PermissionChanged { permission: p }));
            }
        }
        // A hook still waiting hears no: the step does not run.
        for (_, request) in self.asked.drain() {
            if request.starts_with(HOOK) {
                out.push(answer_out(request, Answer::Deny, RUN_ENDED));
            }
        }
        self.hook_calls.clear();
        let (status, task_status) = match &outcome {
            Outcome::Done if ends_with_question(&core.store.parts_of(&self.task)?, &self.run.id) => (RunStatus::Done, TaskStatus::Asked),
            Outcome::Done => (RunStatus::Done, TaskStatus::Done),
            Outcome::Stopped => (RunStatus::Stopped, TaskStatus::Stopped),
            Outcome::Failed(_) => (RunStatus::Failed, TaskStatus::Failed),
        };
        if let Outcome::Failed(f) = &outcome {
            let (text, next) = notice(&self.who, f);
            self.run.reason = Some(text.clone());
            out.extend(self.new_part(core, PartBody::Notice { tone: Tone::Error, text, next, failure: failure_kind(f) }, now, true)?);
        }
        if outcome == Outcome::Stopped {
            self.run.reason = Some("You stopped it.".to_owned());
        }
        self.run.status = status;
        self.run.ended_ms = Some(now);
        self.run.changes = changes;
        let rev = self.write_run(core, now)?;
        out.push(Out::Change(rev, Change::RunChanged { run: self.run.clone() }));
        out.extend(self.task_status(core, task_status, now)?);
        Ok(out)
    }
}

/// Whether a finished run ended with a question for the person: its last part is the agent's words and they end in a question mark
/// (closing quotes, brackets and emphasis aside). Its requests have all been settled by then.
pub fn ends_with_question(parts: &[Part], run: &RunId) -> bool {
    let last = parts.iter().rev().find(|p| p.run.as_ref() == Some(run) && !matches!(&p.body, PartBody::Agent { text, .. } if text.trim().is_empty()));
    match last.map(|p| &p.body) {
        Some(PartBody::Agent { text, .. }) => text.trim_end().trim_end_matches(['*', '_', '"', '\'', ')', ']', '`', '\u{201D}', '\u{2019}']).ends_with('?'),
        _ => false,
    }
}

/// Which failures the person can act on (docs/knowledge/needs-you.md): they are listed in "Needs you". Every other failure is a
/// notice in the task only.
pub fn failure_kind(f: &Failure) -> Option<FailureKind> {
    match f {
        Failure::NotInstalled => Some(FailureKind::NotInstalled),
        Failure::NotSignedIn => Some(FailureKind::NotSignedIn),
        Failure::UsageLimit { .. } => Some(FailureKind::UsageLimit),
        Failure::FolderGone => Some(FailureKind::FolderMissing),
        Failure::Crashed { .. } | Failure::Reported(_) | Failure::TimedOut | Failure::Stalled | Failure::HookMissing | Failure::Unchecked { .. } => None,
    }
}

/// A notice for a failure: what happened, and what to do next. Calm words, never a stack or JSON.
pub fn notice(who: &Who, f: &Failure) -> (String, Option<String>) {
    let n = &who.name;
    match f {
        Failure::NotInstalled => (format!("{n} is not installed on this Mac."), Some(format!("Install it with `{}`, sign in, then send a follow-up.", who.install))),
        Failure::NotSignedIn => (format!("{n} is not signed in."), Some(format!("Run `{}` in Terminal, then send a follow-up.", who.sign_in))),
        Failure::UsageLimit { resets: Some(r) } => (format!("{n} has reached its usage limit."), Some(format!("It resets {r}. Send a follow-up then to carry on."))),
        Failure::UsageLimit { resets: None } => (format!("{n} has reached its usage limit."), Some("Send a follow-up once the limit has reset to carry on.".to_owned())),
        Failure::FolderGone => ("The folder this task works in is gone.".to_owned(), Some("Put the folder back, or start a new task with another folder.".to_owned())),
        Failure::Crashed { code, words } => {
            let plain = !words.is_empty() && !words.trim_start().starts_with(['{', '[']) && !words.trim_start().starts_with("at ") && words.len() < 300;
            let mut t = format!("{n} stopped unexpectedly");
            if let Some(c) = code {
                t.push_str(&format!(" (exit code {c})"));
            }
            t.push('.');
            if plain {
                t.push_str(&format!(" It said: {}", words.trim()));
            }
            (t, Some("Send a follow-up to try again.".to_owned()))
        }
        Failure::Reported(words) => (format!("{n} could not finish: {words}"), Some("Send a follow-up to try again.".to_owned())),
        Failure::TimedOut => ("This ran for 30 minutes, the longest one run may take, so Neko stopped it.".to_owned(), Some("Send a follow-up to carry on where it stopped.".to_owned())),
        Failure::Stalled => (format!("{n} said nothing for 5 minutes, so Neko stopped it."), Some("Send a follow-up to try again.".to_owned())),
        Failure::HookMissing => (
            "Neko's permission check is missing, so it did not start this run.".to_owned(),
            Some("Reinstall Neko, or choose another permission mode in Settings (⌘,), then send a follow-up.".to_owned()),
        ),
        Failure::Unchecked { step } => (
            "Neko could not check a step, so it stopped the run.".to_owned(),
            Some(match step {
                Some(st) => format!("This step ran unchecked: {st}. Look at what it did, then send a follow-up."),
                None => "One step ran unchecked. Look at what it did, then send a follow-up.".to_owned(),
            }),
        ),
    }
}

/// A notice on its own, outside any run (the runtime could not start). Stored as a part; returns the changes. `failure`: the kind
/// of failure the person can act on, if it is one.
pub fn standalone_notice(core: &mut Core, task: &Task, tone: Tone, text: String, next: Option<String>, failure: Option<FailureKind>) -> Result<Vec<Out>, StoreError> {
    let now = core.now();
    let part = Part { id: PartId(core.new_id("part")), task: task.id.clone(), run: None, seq: core.store.next_part_seq(&task.id)?, at_ms: now, ended_ms: Some(now), body: PartBody::Notice { tone, text, next, failure } };
    let rev = core.store.insert_part(&part, now)?;
    Ok(vec![Out::Change(rev, Change::PartChanged { part })])
}

/// The person's words outside any run (a follow-up that could not start one). Stored as a part.
pub fn person_part(core: &mut Core, task: &TaskId, words: &str) -> Result<Vec<Out>, StoreError> {
    let now = core.now();
    let part = Part { id: PartId(core.new_id("part")), task: task.clone(), run: None, seq: core.store.next_part_seq(task)?, at_ms: now, ended_ms: Some(now), body: PartBody::Person { text: words.to_owned() } };
    let rev = core.store.insert_part(&part, now)?;
    Ok(vec![Out::Change(rev, Change::PartChanged { part })])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn part(seq: u64, run: &str, body: PartBody) -> Part {
        Part { id: PartId(format!("p{seq}")), task: TaskId("t".into()), run: Some(RunId(run.into())), seq, at_ms: 0, ended_ms: None, body }
    }

    fn words(seq: u64, run: &str, text: &str) -> Part {
        part(seq, run, PartBody::Agent { text: text.into(), done: true })
    }

    #[test]
    fn a_run_that_ends_with_a_question_is_the_persons_turn() {
        let run = RunId("r".into());
        assert!(ends_with_question(&[words(1, "r", "Which branch should I use?")], &run));
        assert!(ends_with_question(&[words(1, "r", "Should I push it?**\n\n")], &run), "emphasis and spaces aside");
        assert!(ends_with_question(&[words(1, "r", "Shall I go on?"), words(2, "r", "  ")], &run), "empty words are not the last part");
        assert!(!ends_with_question(&[words(1, "r", "Done. The tests pass.")], &run));
        assert!(!ends_with_question(&[words(1, "r", "Is this right? I checked it.")], &run), "a question in the middle is not one for the person");
        let tool = part(2, "r", PartBody::Tool { call: "c".into(), name: "Bash".into(), summary: "ls".into(), input: "ls".into(), output: None, state: ToolState::Done, exit: None });
        assert!(!ends_with_question(&[words(1, "r", "Shall I?"), tool], &run), "a step after the question");
        assert!(!ends_with_question(&[words(1, "other", "Shall I?")], &run), "another run's words");
    }
}
