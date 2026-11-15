//! Question 3: read the real task list from a Neko daemon through `neko-protocol`'s own types and
//! framing (`Frame`, `write_frame`, `read_frame`: a 4-byte length and a JSON body over the Unix
//! socket). The HTTP route the web UI uses answers 401 without its session, so a native client
//! takes the socket, as `neko-client` does. A real client would keep one connection and hold
//! `WorkbenchIfChanged` for pushes; this one asks once.

use std::os::unix::net::UnixStream;
use std::time::{Duration, Instant};

use neko_protocol::workbench::{Command, TaskStatus};
use neko_protocol::{Frame, Request, Response, read_frame, write_frame};

use crate::fake::{Status, Task};

pub fn ask(socket: &str, request: Request) -> Result<Response, String> {
    let mut s = UnixStream::connect(socket).map_err(|e| format!("connect {socket}: {e}"))?;
    s.set_read_timeout(Some(Duration::from_secs(10))).ok();
    write_frame(&mut s, &Frame::Request { id: 1, request }).map_err(|e| e.to_string())?;
    loop {
        match read_frame(&mut s).map_err(|e| e.to_string())? {
            Some(Frame::Response { id: 1, response }) => return Ok(response),
            Some(_) => continue, // an event pushed in between
            None => return Err("daemon closed the connection".into()),
        }
    }
}

fn map(status: &TaskStatus) -> Status {
    match status {
        TaskStatus::AwaitingApproval | TaskStatus::ReadyForReview => Status::NeedsYou,
        TaskStatus::Planning | TaskStatus::Building | TaskStatus::Reviewing => Status::Running,
        TaskStatus::Queued => Status::Queued,
        TaskStatus::Completed | TaskStatus::Cancelled => Status::Done,
        TaskStatus::Failed => Status::Failed,
    }
}

/// The daemon's tasks as rows, how long ask + decode + mapping took, and the task count.
pub fn fetch_tasks(socket: &str) -> Result<(Vec<Task>, Duration, usize), String> {
    let t = Instant::now();
    match ask(socket, Request::Workbench(Command::Snapshot))? {
        Response::Workbench(snapshot) => {
            let n = snapshot.tasks.len();
            let tasks = snapshot
                .tasks
                .iter()
                .map(|t| Task {
                    id: t.issue_id.clone().unwrap_or_else(|| t.id.chars().take(8).collect()),
                    title: t.display_title.clone().unwrap_or_else(|| t.title.clone()),
                    status: map(&t.status),
                    meta: format!("{} events", t.events.len()),
                })
                .collect();
            Ok((tasks, t.elapsed(), n))
        }
        other => Err(format!("unexpected response: {other:?}").chars().take(200).collect()),
    }
}
