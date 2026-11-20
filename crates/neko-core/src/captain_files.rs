//! A captain's four files: `RULES.md`, `CREW.md`, `PLAN.md`, `DIGEST.md` in
//! `<data>/captains/<id>/`. The person edits all four; every write keeps the
//! text it replaced as `<name>.prev`. Nothing here takes a path from a caller:
//! the folder comes from a validated id, the names from `CaptainFileKind`, and
//! a symlink anywhere on the way is refused, so a write cannot leave the
//! captain's own folder. Archiving a captain never touches these files.
//! See `docs/knowledge/captains.md`.
use neko_protocol::captains::{CaptainFile, CaptainFileKind, CaptainFiles, MAX_CAPTAIN_FILE};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// One writer at a time, so the `.prev` copy and the new text never interleave.
static WRITING: Mutex<()> = Mutex::new(());

const TOO_BIG: &str = "Keep it under 16 KB";
const BAD_TEXT: &str = "That text has a character Neko cannot keep";

fn prev_name(kind: CaptainFileKind) -> String {
    format!("{}.prev", kind.file_name())
}

/// What a new captain's file starts with. `RULES.md` carries the purpose; the others are a heading.
pub fn seed(kind: CaptainFileKind, purpose: &str) -> String {
    match kind {
        CaptainFileKind::Rules if !purpose.trim().is_empty() => format!("# Rules\n\n{}\n", purpose.trim()),
        CaptainFileKind::Rules => "# Rules\n".into(),
        CaptainFileKind::Crew => "# Crew rules\n".into(),
        CaptainFileKind::Plan => "# Plan\n".into(),
        CaptainFileKind::Digest => "# Digest\n".into(),
    }
}

/// A file's text without its first `# heading` line: what is left is what the person or captain wrote.
/// A file that is only its seed heading has an empty body (prompts treat it as empty).
pub fn body(text: &str) -> &str {
    match text.trim_start().strip_prefix("# ") {
        Some(rest) => rest.split_once('\n').map_or("", |(_, tail)| tail).trim(),
        None => text.trim(),
    }
}

fn is_link(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink())
}

/// `<root>/captains/<id>`, for a valid id only, and only when neither it nor `captains/` is a link that
/// leads elsewhere. With `create` the folder is made (0700); without, it may not exist yet.
fn confined_dir(root: &Path, id: &str, create: bool) -> Result<PathBuf, String> {
    if !crate::artifacts::valid_id(id) {
        return Err("That captain does not exist".into());
    }
    let parent = root.join("captains");
    if create {
        std::fs::create_dir_all(&parent).map_err(|e| format!("Cannot keep the captain's files: {e}"))?;
    }
    let dir = parent.join(id);
    if is_link(&dir) {
        return Err("The captain's folder is a link; Neko will not use it".into());
    }
    if !dir.exists() {
        if !create {
            return Ok(dir);
        }
        make_private_dir(&dir).map_err(|e| format!("Cannot keep the captain's files: {e}"))?;
    }
    let inside = match (dir.canonicalize(), parent.canonicalize()) {
        (Ok(dir), Ok(parent)) => dir.starts_with(parent),
        _ => false,
    };
    if inside && dir.is_dir() { Ok(dir) } else { Err("The captain's folder is not where Neko keeps it".into()) }
}

#[cfg(unix)]
fn make_private_dir(dir: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    std::fs::DirBuilder::new().mode(0o700).create(dir)
}

#[cfg(not(unix))]
fn make_private_dir(dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir(dir)
}

fn file_in(dir: &Path, name: &str) -> Result<PathBuf, String> {
    let path = dir.join(name);
    if is_link(&path) {
        return Err(format!("{name} is a link; Neko will not use it"));
    }
    Ok(path)
}

/// Writes `bytes` to `path` through a temporary file in the same folder and a rename.
fn replace(dir: &Path, path: &Path, bytes: &[u8]) -> Result<(), String> {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("file");
    let temp = dir.join(format!(".{name}.tmp"));
    let _ = std::fs::remove_file(&temp);
    let result = (|| {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        std::fs::rename(&temp, path)
    })();
    if let Err(e) = result {
        let _ = std::fs::remove_file(&temp);
        return Err(format!("Cannot save {name}: {e}"));
    }
    Ok(())
}

/// Creates a captain's folder with the four seeded files. A file that already exists is left alone.
pub fn create_all(root: &Path, id: &str, purpose: &str) -> Result<(), String> {
    let _guard = WRITING.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let dir = confined_dir(root, id, true)?;
    for kind in CaptainFileKind::ALL {
        let path = file_in(&dir, kind.file_name())?;
        if std::fs::symlink_metadata(&path).is_err() {
            replace(&dir, &path, seed(kind, purpose).as_bytes())?;
        }
    }
    Ok(())
}

/// Replaces one file with `text`, keeping what it held as `<name>.prev`. Empty or whitespace-only text
/// removes the file (after the copy). At most 16 KB, no NUL.
pub fn write(root: &Path, id: &str, kind: CaptainFileKind, text: &str) -> Result<(), String> {
    if text.len() > MAX_CAPTAIN_FILE {
        return Err(TOO_BIG.into());
    }
    if text.contains('\0') {
        return Err(BAD_TEXT.into());
    }
    let _guard = WRITING.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let dir = confined_dir(root, id, true)?;
    let path = file_in(&dir, kind.file_name())?;
    let prev = file_in(&dir, &prev_name(kind))?;
    if std::fs::symlink_metadata(&path).is_ok() {
        let old = std::fs::read(&path).map_err(|e| format!("Cannot read {}: {e}", kind.file_name()))?;
        replace(&dir, &prev, &old)?;
    }
    if text.trim().is_empty() {
        return match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(format!("Cannot clear {}: {e}", kind.file_name())),
        };
    }
    replace(&dir, &path, text.as_bytes())
}

/// The text of a file (cut at 16 KB on a character boundary, and whether it was cut) and its modified time.
fn read_one(path: &Path) -> Result<Option<(String, bool, i64)>, String> {
    use std::io::Read;
    if is_link(path) {
        return Err(format!("{} is a link; Neko will not use it", path.display()));
    }
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("Cannot read {}: {e}", path.display())),
    };
    let modified = file.metadata().ok().and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(0));
    let mut bytes = Vec::new();
    file.take(MAX_CAPTAIN_FILE as u64 + 1).read_to_end(&mut bytes).map_err(|e| format!("Cannot read {}: {e}", path.display()))?;
    let truncated = bytes.len() > MAX_CAPTAIN_FILE;
    bytes.truncate(MAX_CAPTAIN_FILE);
    let mut text = String::from_utf8_lossy(&bytes).into_owned();
    if truncated {
        // A cut inside a character became U+FFFD at the end; drop it.
        while text.ends_with('\u{fffd}') { text.pop(); }
    }
    Ok(Some((text, truncated, modified)))
}

/// All four files; a missing file reads as empty.
pub fn read_all(root: &Path, id: &str) -> Result<CaptainFiles, String> {
    let dir = confined_dir(root, id, false)?;
    let mut files = Vec::new();
    for kind in CaptainFileKind::ALL {
        let current = read_one(&dir.join(kind.file_name()))?;
        let prev = read_one(&dir.join(prev_name(kind)))?.map(|(text, ..)| text);
        let (text, truncated, updated_ms) = current.unwrap_or_default();
        files.push(CaptainFile { kind, text, updated_ms, prev, truncated });
    }
    Ok(CaptainFiles { id: id.to_owned(), files })
}

/// What the person or captain wrote in one file (its heading dropped, so a file that is only its heading is
/// empty), as prompts read it. A missing, unreadable or linked file reads as empty: a prompt is never refused
/// because a note is missing.
pub fn prompt_body(root: &Path, id: &str, kind: CaptainFileKind) -> String {
    let Ok(dir) = confined_dir(root, id, false) else { return String::new() };
    match read_one(&dir.join(kind.file_name())) {
        Ok(Some((text, ..))) => body(&text).to_owned(),
        _ => String::new(),
    }
}

/// The text appended to the rules of every session a captain starts: `CREW.md` under "Your captain's rules".
/// Empty (nothing injected) for a session with no captain and for an empty `CREW.md`.
pub fn crew_section(root: &Path, captain_id: Option<&str>) -> String {
    let Some(id) = captain_id else { return String::new() };
    let crew = prompt_body(root, id, CaptainFileKind::Crew);
    if crew.is_empty() { String::new() } else { format!("\n\nYour captain's rules\n{}\n", crew.trim_end()) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use CaptainFileKind::*;

    fn root() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    fn text_of(files: &CaptainFiles, kind: CaptainFileKind) -> &CaptainFile {
        files.files.iter().find(|f| f.kind == kind).unwrap()
    }

    #[test]
    fn an_invalid_id_never_creates_a_directory() {
        let root = root();
        for id in ["../x", "", &"a".repeat(65), "a/b", "/abs", "..", "a.b"] {
            assert!(create_all(root.path(), id, "p").is_err(), "{id:?}");
            assert!(write(root.path(), id, Rules, "x").is_err(), "{id:?}");
            assert!(read_all(root.path(), id).is_err(), "{id:?}");
        }
        assert!(!root.path().join("captains").exists());
        assert!(!root.path().parent().unwrap().join("x").exists());
    }

    #[test]
    fn create_seeds_rules_from_the_purpose_and_the_others_with_one_heading() {
        let root = root();
        create_all(root.path(), "c1", "Keep the web app fast.").unwrap();
        let files = read_all(root.path(), "c1").unwrap();
        assert_eq!(text_of(&files, Rules).text, "# Rules\n\nKeep the web app fast.\n");
        assert_eq!(text_of(&files, Crew).text, "# Crew rules\n");
        assert_eq!(text_of(&files, Plan).text, "# Plan\n");
        assert_eq!(text_of(&files, Digest).text, "# Digest\n");
        assert!(files.files.iter().all(|f| f.prev.is_none() && f.updated_ms > 0 && !f.truncated));
        create_all(root.path(), "c2", "  ").unwrap();
        assert_eq!(text_of(&read_all(root.path(), "c2").unwrap(), Rules).text, "# Rules\n");
    }

    #[test]
    fn creating_again_leaves_files_that_exist_alone() {
        let root = root();
        create_all(root.path(), "c1", "one").unwrap();
        write(root.path(), "c1", Plan, "my plan").unwrap();
        create_all(root.path(), "c1", "two").unwrap();
        let files = read_all(root.path(), "c1").unwrap();
        assert_eq!(text_of(&files, Plan).text, "my plan");
        assert!(text_of(&files, Rules).text.contains("one"));
    }

    #[test]
    fn a_captain_with_no_files_reads_as_four_empty_files() {
        let root = root();
        let files = read_all(root.path(), "never-made").unwrap();
        assert_eq!(files.files.len(), 4);
        assert!(files.files.iter().all(|f| f.text.is_empty() && f.prev.is_none() && f.updated_ms == 0));
    }

    #[test]
    fn sixteen_kb_writes_and_one_byte_more_fails_and_leaves_the_old_file() {
        let root = root();
        create_all(root.path(), "c1", "").unwrap();
        write(root.path(), "c1", Plan, &"a".repeat(16_384)).unwrap();
        let err = write(root.path(), "c1", Plan, &"b".repeat(16_385)).unwrap_err();
        assert_eq!(err, "Keep it under 16 KB");
        let files = read_all(root.path(), "c1").unwrap();
        assert_eq!(text_of(&files, Plan).text.len(), 16_384);
        assert!(text_of(&files, Plan).text.starts_with('a'));
    }

    #[test]
    fn every_write_keeps_the_text_it_replaced_as_prev() {
        let root = root();
        create_all(root.path(), "c1", "").unwrap();
        write(root.path(), "c1", Plan, "first").unwrap();
        assert_eq!(text_of(&read_all(root.path(), "c1").unwrap(), Plan).prev.as_deref(), Some("# Plan\n"));
        write(root.path(), "c1", Plan, "second").unwrap();
        write(root.path(), "c1", Plan, "third").unwrap();
        let files = read_all(root.path(), "c1").unwrap();
        assert_eq!((text_of(&files, Plan).text.as_str(), text_of(&files, Plan).prev.as_deref()), ("third", Some("second")));
        assert!(root.path().join("captains/c1/PLAN.md.prev").exists());
    }

    #[test]
    fn a_nul_byte_is_refused() {
        let root = root();
        create_all(root.path(), "c1", "").unwrap();
        assert!(write(root.path(), "c1", Rules, "a\0b").unwrap_err().contains("cannot keep"));
        assert!(text_of(&read_all(root.path(), "c1").unwrap(), Rules).text.starts_with("# Rules"));
    }

    #[test]
    fn empty_text_removes_the_file_and_keeps_prev() {
        let root = root();
        create_all(root.path(), "c1", "").unwrap();
        write(root.path(), "c1", Plan, "keep me").unwrap();
        write(root.path(), "c1", Plan, " \n").unwrap();
        let files = read_all(root.path(), "c1").unwrap();
        assert!(text_of(&files, Plan).text.is_empty());
        assert_eq!(text_of(&files, Plan).prev.as_deref(), Some("keep me"));
        write(root.path(), "c1", Plan, "").unwrap(); // clearing nothing is fine
    }

    #[test]
    fn a_hand_edited_file_over_16_kb_is_cut_on_a_character_boundary_and_flagged() {
        let root = root();
        create_all(root.path(), "c1", "").unwrap();
        let big = format!("{}{}", "a".repeat(16_383), "é".repeat(10)); // the first é straddles the limit
        std::fs::write(root.path().join("captains/c1/PLAN.md"), &big).unwrap();
        let files = read_all(root.path(), "c1").unwrap();
        let plan = text_of(&files, Plan);
        assert!(plan.truncated && plan.text.len() <= 16_384 && !plan.text.contains('\u{fffd}'));
    }

    #[cfg(unix)]
    #[test]
    fn a_folder_or_file_that_is_a_link_is_refused_and_nothing_outside_is_touched() {
        let root = root();
        let outside = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("captains")).unwrap();
        std::os::unix::fs::symlink(outside.path(), root.path().join("captains/c1")).unwrap();
        assert!(write(root.path(), "c1", Rules, "x").is_err());
        assert!(read_all(root.path(), "c1").is_err());
        assert!(std::fs::read_dir(outside.path()).unwrap().next().is_none());

        create_all(root.path(), "c2", "").unwrap();
        let target = outside.path().join("secret.md");
        std::fs::write(&target, "secret").unwrap();
        std::fs::remove_file(root.path().join("captains/c2/RULES.md")).unwrap();
        std::os::unix::fs::symlink(&target, root.path().join("captains/c2/RULES.md")).unwrap();
        assert!(write(root.path(), "c2", Rules, "overwritten").is_err());
        assert!(read_all(root.path(), "c2").is_err());
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "secret");
    }

    #[test]
    fn body_drops_the_seed_heading_only() {
        assert_eq!(body("# Crew rules\n"), "");
        assert_eq!(body("# Rules\n\nBe kind.\n"), "Be kind.");
        assert_eq!(body("Just text\n"), "Just text");
        assert_eq!(body(""), "");
    }
    #[test]
    fn crew_section_is_empty_without_a_captain_a_file_or_a_body_and_has_the_exact_heading_otherwise() {
        let root = root();
        assert_eq!(crew_section(root.path(), None), "");
        assert_eq!(crew_section(root.path(), Some("nobody")), "", "no folder");
        create_all(root.path(), "c1", "Look after the web app.").unwrap();
        assert_eq!(crew_section(root.path(), Some("c1")), "", "a heading-only CREW.md is empty");
        write(root.path(), "c1", Crew, "# Crew rules\n\nAlways run the linter.\nNo force pushes.\n\n").unwrap();
        assert_eq!(crew_section(root.path(), Some("c1")), "\n\nYour captain's rules\nAlways run the linter.\nNo force pushes.\n");
        write(root.path(), "c1", Crew, "   \n").unwrap();
        assert_eq!(crew_section(root.path(), Some("c1")), "", "clearing the file injects nothing");
        assert_eq!(crew_section(root.path(), Some("../x")), "", "a bad id reads nothing");
    }

    #[test]
    fn prompt_body_reads_the_text_after_the_heading_and_empty_when_missing() {
        let root = root();
        assert_eq!(prompt_body(root.path(), "c1", Plan), "");
        create_all(root.path(), "c1", "Look after the web app.").unwrap();
        assert_eq!(prompt_body(root.path(), "c1", Rules), "Look after the web app.");
        assert_eq!(prompt_body(root.path(), "c1", Plan), "");
        write(root.path(), "c1", Plan, "Ship the export fix first.").unwrap();
        assert_eq!(prompt_body(root.path(), "c1", Plan), "Ship the export fix first.");
    }
}
