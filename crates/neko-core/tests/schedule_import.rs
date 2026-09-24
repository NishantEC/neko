use neko_core::setup_import;
use std::{collections::BTreeMap, fs};

#[test]
fn codex_and_claude_discovery_preserves_portable_content_and_warns_missing_context() {
    let home = tempfile::tempdir().unwrap();
    for dir in [
        ".codex/automations/review",
        ".codex/automations/heartbeat",
        ".claude/scheduled-tasks/morning",
    ] {
        fs::create_dir_all(home.path().join(dir)).unwrap();
    }
    fs::write(home.path().join(".codex/automations/review/automation.toml"),"id = 'review'\nname = 'Review'\nprompt = 'Review recent changes'\nrrule = 'FREQ=DAILY;BYHOUR=9'\nstatus = 'ACTIVE'\ncreated_at = 123\ncwds = ['/tmp/project']\n").unwrap();
    fs::write(home.path().join(".codex/automations/heartbeat/automation.toml"),"id = 'heartbeat'\nname = 'Watch'\nprompt = 'Check previous result'\nrrule = 'FREQ=HOURLY'\ntarget_thread_id = 'private-thread'\n").unwrap();
    fs::write(
        home.path().join(".claude/scheduled-tasks/morning/SKILL.md"),
        "---\nname: Morning\ndescription: Daily review\n---\nReview repository changes.\n",
    )
    .unwrap();
    fs::write(
        home.path().join(".claude/scheduled_tasks.json"),
        "not a portable documented schema",
    )
    .unwrap();
    let preview = setup_import::discover(home.path(), &[], &[], &BTreeMap::new()).preview();
    let value = serde_json::to_value(&preview).unwrap();
    assert_eq!(
        value["schedules"].as_array().map(Vec::len),
        Some(3),
        "discover both documented sources"
    );
    let schedules = value["schedules"].as_array().unwrap();
    let codex = schedules.iter().find(|s| s["name"] == "Review").unwrap();
    assert_eq!(codex["rule"], "FREQ=DAILY;BYHOUR=9");
    assert_eq!(codex["repository"], "/tmp/project");
    let heartbeat = schedules.iter().find(|s| s["name"] == "Watch").unwrap();
    assert!(heartbeat["repository"].is_null());
    assert!(heartbeat["warnings"].to_string().contains("conversation"));
    let claude = schedules.iter().find(|s| s["name"] == "Morning").unwrap();
    assert_eq!(claude["prompt"], "Review repository changes.");
    assert_eq!(claude["rule"], "");
    assert!(claude["warnings"].to_string().contains("unavailable"));
    assert!(preview.warnings.join(" ").contains("unsupported"));
}

#[test]
fn symlinked_files_and_directories_are_never_imported() {
    use std::os::unix::fs::symlink;
    let home = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    fs::create_dir_all(home.path().join(".codex/automations/file")).unwrap();
    fs::write(
        outside.path().join("automation.toml"),
        "name='Secret'\nprompt='outside private content'\n",
    )
    .unwrap();
    symlink(
        outside.path().join("automation.toml"),
        home.path().join(".codex/automations/file/automation.toml"),
    )
    .unwrap();
    symlink(
        outside.path(),
        home.path().join(".codex/automations/directory"),
    )
    .unwrap();
    let value = serde_json::to_value(
        setup_import::discover(home.path(), &[], &[], &BTreeMap::new()).preview(),
    )
    .unwrap();
    assert_eq!(value["schedules"].as_array().map(Vec::len), Some(0));
    assert!(!value.to_string().contains("outside private content"));
}

#[test]
fn fifo_and_oversized_source_are_rejected_without_reading_their_content() {
    use std::os::unix::ffi::OsStrExt;
    let home = tempfile::tempdir().unwrap();
    for dir in [".codex/automations/fifo", ".codex/automations/big"] {
        fs::create_dir_all(home.path().join(dir)).unwrap();
    }
    let fifo = std::ffi::CString::new(
        home.path()
            .join(".codex/automations/fifo/automation.toml")
            .as_os_str()
            .as_bytes(),
    )
    .unwrap();
    // SAFETY: live NUL-terminated fixture path; mkfifo copies it during this call.
    assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
    fs::write(
        home.path().join(".codex/automations/big/automation.toml"),
        "x".repeat(512 * 1024 + 1),
    )
    .unwrap();
    let before = std::time::Instant::now();
    let preview = setup_import::discover(home.path(), &[], &[], &BTreeMap::new()).preview();
    assert!(before.elapsed() < std::time::Duration::from_secs(2));
    assert!(preview.schedules.is_empty());
    assert!(preview.warnings.iter().any(|w| w.contains("regular file")));
    assert!(preview.warnings.iter().any(|w| w.contains("512 KB")));
}
