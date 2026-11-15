//! Question 3: read the real task list from a Neko daemon through `neko-protocol`'s own types.
//! The daemon answers the same JSON the web UI sends: `POST /api/request` with a
//! `neko_protocol::Request`, answered with a `neko_protocol::Response`. A plain `TcpStream` is
//! enough for the spike; a real client would keep one connection and hold `WorkbenchIfChanged`.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

use neko_protocol::workbench::TaskStatus;
use neko_protocol::{Request, Response};

use crate::fake::{Status, Task};

fn dechunk(mut body: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    loop {
        let Some(nl) = body.windows(2).position(|w| w == b"\r\n") else { break };
        let size = usize::from_str_radix(std::str::from_utf8(&body[..nl]).unwrap_or("0").trim(), 16).unwrap_or(0);
        if size == 0 {
            break;
        }
        let start = nl + 2;
        out.extend_from_slice(&body[start..start + size]);
        body = &body[start + size + 2..];
    }
    out
}

pub fn post(port: u16, request: &Request) -> Result<Response, String> {
    let payload = serde_json::to_string(request).map_err(|e| e.to_string())?;
    let mut s = TcpStream::connect(("127.0.0.1", port)).map_err(|e| e.to_string())?;
    s.set_read_timeout(Some(Duration::from_secs(10))).ok();
    write!(
        s,
        "POST /api/request HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
        payload.len()
    )
    .map_err(|e| e.to_string())?;
    let mut raw = Vec::new();
    s.read_to_end(&mut raw).map_err(|e| e.to_string())?;
    let split = raw.windows(4).position(|w| w == b"\r\n\r\n").ok_or("no header end")? + 4;
    let head = String::from_utf8_lossy(&raw[..split]).to_ascii_lowercase();
    if !head.starts_with("http/1.1 200") {
        return Err(head.lines().next().unwrap_or("").to_owned());
    }
    let body = if head.contains("transfer-encoding: chunked") { dechunk(&raw[split..]) } else { raw[split..].to_vec() };
    serde_json::from_slice(&body).map_err(|e| format!("decode: {e}"))
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

/// The daemon's tasks as rows, and how long the request, decode and mapping took.
pub fn fetch_tasks(port: u16) -> Result<(Vec<Task>, Duration, usize), String> {
    let t = Instant::now();
    match post(port, &Request::Workbench(neko_protocol::workbench::Command::Snapshot))? {
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
