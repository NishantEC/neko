//! Read only documented portable scheduling files. Never infer source authority.
use neko_protocol::setup_import::ImportSchedule;
use sha2::{Digest, Sha256};
use std::{
    ffi::{CStr, CString, OsString},
    fs::File,
    io::Read,
    os::fd::{AsRawFd, FromRawFd, IntoRawFd},
    path::{Component, Path},
};

fn names(directory: File) -> Result<Vec<OsString>, String> {
    use std::os::unix::ffi::OsStrExt;
    let fd = directory.into_raw_fd();
    // SAFETY: fd is an owned open directory. fdopendir takes ownership only
    // on success, and Directory below closes that stream exactly once.
    let stream = unsafe { libc::fdopendir(fd) };
    if stream.is_null() {
        // SAFETY: failed fdopendir leaves fd owned by us.
        drop(unsafe { File::from_raw_fd(fd) });
        return Err("Cannot list schedule directory".into());
    }
    struct Directory(*mut libc::DIR);
    impl Drop for Directory {
        fn drop(&mut self) {
            // SAFETY: this guard exclusively owns the successful fdopendir.
            unsafe {
                libc::closedir(self.0);
            }
        }
    }
    let stream = Directory(stream);
    let mut names = Vec::new();
    for _ in 0..102 {
        // SAFETY: live exclusively owned DIR. Copy the filename before the next
        // readdir invalidates its pointer; POSIX guarantees NUL-terminated d_name.
        let entry = unsafe { libc::readdir(stream.0) };
        if entry.is_null() {
            break;
        }
        let name = unsafe { CStr::from_ptr((*entry).d_name.as_ptr()) }.to_bytes();
        if name == b"." || name == b".." {
            continue;
        }
        names.push(std::ffi::OsStr::from_bytes(name).to_owned());
        if names.len() == 100 {
            break;
        }
    }
    Ok(names)
}

/// Each path component is opened relative to its already-open parent and with
/// O_NOFOLLOW. This includes directories, preventing symlink replacement races.
fn open_beneath(home: &Path, relative: &Path, directory: bool) -> Result<File, String> {
    use std::os::unix::ffi::OsStrExt;
    let mut parent = File::open(home).map_err(|_| "Home directory unavailable")?;
    let components: Vec<_> = relative.components().collect();
    for (i, component) in components.iter().enumerate() {
        let Component::Normal(name) = component else {
            return Err("Invalid import path".into());
        };
        let name = CString::new(name.as_bytes()).map_err(|_| "Invalid import filename")?;
        let flags = libc::O_RDONLY
            | libc::O_NOFOLLOW
            | libc::O_NONBLOCK
            | libc::O_CLOEXEC
            | if i + 1 < components.len() || directory {
                libc::O_DIRECTORY
            } else {
                0
            };
        // SAFETY: parent owns a live FD; name is NUL-terminated for this call;
        // openat is read-only and the returned descriptor is checked below.
        let fd = unsafe { libc::openat(parent.as_raw_fd(), name.as_ptr(), flags) };
        if fd < 0 {
            return Err("Import file unavailable, unsupported, or symlinked".into());
        }
        // SAFETY: successful openat returns a new owned descriptor exactly once.
        parent = unsafe { File::from_raw_fd(fd) };
    }
    Ok(parent)
}

fn text(home: &Path, relative: &Path) -> Result<String, String> {
    let file = open_beneath(home, relative, false)?;
    if !file
        .metadata()
        .map_err(|_| "Import metadata unavailable")?
        .is_file()
    {
        return Err("Import is not a regular file".into());
    }
    let mut text = String::new();
    file.take(512 * 1024 + 1)
        .read_to_string(&mut text)
        .map_err(|_| "Import file is not UTF-8")?;
    if text.len() > 512 * 1024 {
        return Err("Import exceeds 512 KB".into());
    }
    Ok(text)
}

pub fn discover(home: &Path) -> (Vec<ImportSchedule>, Vec<String>) {
    let mut found = Vec::new();
    let mut warnings = Vec::new();
    let mut total_bytes = 0;
    for (root, filename, codex) in [
        (".codex/automations", "automation.toml", true),
        (".claude/scheduled-tasks", "SKILL.md", false),
    ] {
        // Enumerate the opened directory, never a path that can be swapped.
        let Ok(directory) = open_beneath(home, Path::new(root), true) else {
            continue;
        };
        let Ok(entries) = names(directory) else {
            warnings.push(format!("{root}: cannot list schedules"));
            continue;
        };
        for entry in entries {
            let relative = Path::new(root).join(&entry).join(filename);
            let source = home.join(&relative).to_string_lossy().into_owned();
            let parsed = text(home, &relative).and_then(|body| {
                if codex {
                    parse_codex(&body, &source)
                } else {
                    parse_claude(&body, &source, &entry.to_string_lossy())
                }
            });
            match parsed {
                Ok(schedule) => {
                    let size = serde_json::to_vec(&schedule).map_or(usize::MAX, |v| v.len());
                    if found.len() >= 100 || total_bytes + size > 512 * 1024 {
                        warnings
                            .push("Schedule preview limit reached; select fewer sources".into());
                        break;
                    }
                    total_bytes += size;
                    found.push(schedule);
                }
                Err(error) => warnings.push(format!("{source}: {error}")),
            }
        }
    }
    if home
        .join(".claude/scheduled_tasks.json")
        .symlink_metadata()
        .is_ok()
    {
        warnings.push("Claude CLI scheduled_tasks.json is unsupported: its storage schema is undocumented; no content was imported.".into());
    }
    (found, warnings)
}

fn identity(source: &str) -> String {
    format!("schedule:{:x}", Sha256::digest(source.as_bytes()))
}

fn parse_codex(body: &str, source: &str) -> Result<ImportSchedule, String> {
    let value: toml::Value =
        toml::from_str(body).map_err(|_| "Invalid automation TOML (contents withheld)")?;
    let get = |key| {
        value
            .get(key)
            .and_then(toml::Value::as_str)
            .unwrap_or("")
            .to_owned()
    };
    let mut warnings = vec!["Imported paused. Choose an IANA timezone and review the rule before enabling; source permissions are not imported.".into()];
    let heartbeat = value.get("target_thread_id").is_some() || get("kind") == "heartbeat";
    let repository = if heartbeat {
        warnings.push("Needs workspace: attached conversation context is not portable; the prompt is preserved without that conversation.".into());
        None
    } else {
        let paths = value.get("cwds").and_then(toml::Value::as_array);
        if paths.is_some_and(|p| p.len() > 1) {
            warnings.push("Multiple source workspaces: choose one workspace explicitly.".into());
            None
        } else {
            paths
                .and_then(|p| p.first())
                .and_then(toml::Value::as_str)
                .filter(|p| Path::new(p).is_absolute())
                .map(str::to_owned)
        }
    };
    if repository.is_none() && !heartbeat {
        warnings.push("Needs workspace: no portable repository path was available; project IDs are not repository paths.".into());
    }
    let schedule = ImportSchedule {
        id: identity(source),
        name: get("name"),
        prompt: get("prompt"),
        source: source.into(),
        repository,
        rule: get("rrule"),
        timezone: String::new(),
        anchor_ms: value
            .get("created_at")
            .and_then(toml::Value::as_integer)
            .unwrap_or(0),
        warnings,
    };
    validate(schedule)
}

fn parse_claude(body: &str, source: &str, fallback: &str) -> Result<ImportSchedule, String> {
    let normalized = body.replace("\r\n", "\n");
    let front = normalized
        .strip_prefix("---\n")
        .ok_or("Scheduled SKILL.md requires YAML frontmatter")?;
    let (metadata, prompt) = front
        .split_once("\n---\n")
        .ok_or("Scheduled SKILL.md frontmatter is incomplete")?;
    // Display metadata only. Preserve the actual prompt body without executing,
    // resolving YAML tags, aliases, or any undocumented schedule metadata.
    let name = metadata
        .lines()
        .find_map(|line| line.strip_prefix("name:"))
        .map(|s| s.trim().trim_matches(['\'', '"']))
        .filter(|s| !s.is_empty())
        .unwrap_or(fallback);
    validate(ImportSchedule { id:identity(source), name:name.into(), prompt:prompt.trim().into(), source:source.into(), repository:None,rule:String::new(),timezone:String::new(),anchor_ms:0,warnings:vec!["Imported paused. Schedule, folder, model and enabled state are unavailable in documented SKILL.md storage. Choose a workspace, rule and timezone; source permissions are not imported.".into()] })
}

fn validate(schedule: ImportSchedule) -> Result<ImportSchedule, String> {
    if schedule.name.trim().is_empty()
        || schedule.name.len() > 256
        || schedule.prompt.trim().is_empty()
        || schedule.prompt.len() > 32_768
        || schedule.rule.len() > 2048
        || schedule.repository.as_ref().is_some_and(|p| p.len() > 4096)
    {
        return Err("Schedule name, prompt or metadata exceeds supported limits".into());
    }
    Ok(schedule)
}
