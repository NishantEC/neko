//! A workspace's folder that goes away, or comes back, while a screen watches is noticed within a few seconds and pushed.

mod support;
use std::time::{Duration, Instant};

use neko_protocol::*;
use support::*;

fn wait_missing(c: &mut neko_protocol::client::Client, id: &str, missing: bool) -> Duration {
    let t = Instant::now();
    loop {
        assert!(t.elapsed() < Duration::from_secs(10), "no workspace_updated with missing = {missing} within 10 s");
        if let Some(ServerMessage::Event { change: Change::WorkspaceUpdated { workspace }, .. }) = c.next_event(Duration::from_millis(100)).unwrap()
            && workspace.id == id
            && workspace.missing == missing
        {
            return t.elapsed();
        }
    }
}

#[test]
fn a_folder_removed_and_put_back_while_a_screen_watches_is_pushed_within_seconds() {
    let d = Daemon::start("folders");
    let mut c = d.client();
    assert!(matches!(c.request(Request::Subscribe).unwrap(), Reply::Subscribed { .. }));
    let folder = d.dir.join("tofu-notes");
    std::fs::create_dir_all(&folder).unwrap();
    let id = match c.request(Request::AddWorkspace { folder: folder.to_string_lossy().into_owned() }).unwrap() {
        Reply::Workspace { workspace, .. } => workspace.id,
        r => panic!("add: {r:?}"),
    };
    std::thread::sleep(Duration::from_secs(4));
    std::fs::remove_dir_all(&folder).unwrap();
    let gone = wait_missing(&mut c, &id, true);
    std::fs::create_dir_all(&folder).unwrap();
    let back = wait_missing(&mut c, &id, false);
    assert!(gone < Duration::from_secs(5) && back < Duration::from_secs(5), "noticed after {gone:?} and {back:?}");
}
