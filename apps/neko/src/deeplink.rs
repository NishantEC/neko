//! Neko's links (docs/knowledge/needs-you.md): `neko://task/<id>` opens the app at that task. The bundle declares the scheme
//! (`scripts/build-app.sh`); Rune hands each link to `on_open_url`, the one that launched the app too. A link comes from anyone who
//! can run `open`, so it is read strictly: one scheme, one kind of path, an id of the shape the daemon makes, nothing else.

use neko_protocol::TaskId;

/// The scheme, as `Info.plist` declares it.
pub const SCHEME: &str = "neko";

/// The link that opens `task`.
pub fn url(task: &TaskId) -> String {
    format!("{SCHEME}://task/{}", task.0)
}

/// Whether `id` is a task id as the daemon makes them: `task_` and 26 capital letters and digits.
pub fn is_task_id(id: &str) -> bool {
    id.strip_prefix("task_").is_some_and(|rest| rest.len() == 26 && rest.bytes().all(|b| b.is_ascii_digit() || b.is_ascii_uppercase()))
}

/// The task a link opens, or None for anything that is not exactly `neko://task/<id>` (a trailing slash is allowed; the scheme is
/// read without regard to case, as URL schemes are).
pub fn parse(link: &str) -> Option<TaskId> {
    let link = link.trim();
    let (scheme, rest) = link.split_once("://")?;
    if !scheme.eq_ignore_ascii_case(SCHEME) {
        return None;
    }
    let id = rest.strip_prefix("task/")?.trim_end_matches('/');
    is_task_id(id).then(|| TaskId(id.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "task_01HZX3J0000000000000000000";

    #[test]
    fn a_task_link_is_read_strictly() {
        assert_eq!(parse(&format!("neko://task/{ID}")), Some(TaskId(ID.into())));
        assert_eq!(parse(&format!("NEKO://task/{ID}/")), Some(TaskId(ID.into())), "the scheme's case and a trailing slash");
        assert_eq!(url(&TaskId(ID.into())), format!("neko://task/{ID}"));
        assert_eq!(parse(&url(&TaskId(ID.into()))), Some(TaskId(ID.into())), "a link Neko makes is one it opens");
        for bad in [
            "neko://task/".to_owned(),
            "neko://task/task_1".to_owned(),
            format!("neko://task/{ID}/more"),
            format!("neko://task/{ID}?x=1"),
            format!("neko://settings/{ID}"),
            format!("https://task/{ID}"),
            format!("neko:task/{ID}"),
            format!("neko://task/{}", ID.to_lowercase()),
            format!("neko://task/../{ID}"),
            String::new(),
        ] {
            assert_eq!(parse(&bad), None, "{bad:?} is not a task link");
        }
    }
}
