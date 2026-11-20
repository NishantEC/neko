//! A cat's four files: `RULES.md`, `TASK_RULES.md`, `PLAN.md`, `BRIEFING.md` in
//! `<data>/cats/<id>/`. The person edits all four; every write keeps the
//! text it replaced as `<name>.prev`. Nothing here takes a path from a caller:
//! the folder comes from a validated id, the names from `CatFileKind`, and
//! a symlink anywhere on the way is refused, so a write cannot leave the
//! cat's own folder. Archiving a cat never touches these files.
//! See `docs/knowledge/cats.md`.
use neko_protocol::cats::{CatFile, CatFileKind, CatFiles, MAX_CAT_FILE};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// One writer at a time, so the `.prev` copy and the new text never interleave.
static WRITING: Mutex<()> = Mutex::new(());

const TOO_BIG: &str = "Keep it under 16 KB";
const BAD_TEXT: &str = "That text has a character Neko cannot keep";

fn prev_name(kind: CatFileKind) -> String {
    format!("{}.prev", kind.file_name())
}

/// What a new cat's file starts with. `RULES.md` carries the purpose; the others are a heading.
pub fn seed(kind: CatFileKind, purpose: &str) -> String {
    match kind {
        CatFileKind::Rules if !purpose.trim().is_empty() => format!("# Rules\n\n{}\n", purpose.trim()),
        CatFileKind::Rules => "# Rules\n".into(),
        CatFileKind::TaskRules => "# Task rules\n".into(),
        CatFileKind::Plan => "# Plan\n".into(),
        CatFileKind::Briefing => "# Briefing\n".into(),
    }
}

/// A file's text without its first `# heading` line: what is left is what the person or cat wrote.
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

/// `<root>/cats/<id>`, for a valid id only, and only when neither it nor `cats/` is a link that
/// leads elsewhere. With `create` the folder is made (0700); without, it may not exist yet.
fn confined_dir(root: &Path, id: &str, create: bool) -> Result<PathBuf, String> {
    if !crate::artifacts::valid_id(id) {
        return Err("That cat does not exist".into());
    }
    let parent = root.join("cats");
    if create {
        std::fs::create_dir_all(&parent).map_err(|e| format!("Cannot keep the cat's files: {e}"))?;
    }
    let dir = parent.join(id);
    if is_link(&dir) {
        return Err("The cat's folder is a link; Neko will not use it".into());
    }
    if !dir.exists() {
        if !create {
            return Ok(dir);
        }
        make_private_dir(&dir).map_err(|e| format!("Cannot keep the cat's files: {e}"))?;
    }
    let inside = match (dir.canonicalize(), parent.canonicalize()) {
        (Ok(dir), Ok(parent)) => dir.starts_with(parent),
        _ => false,
    };
    if inside && dir.is_dir() { Ok(dir) } else { Err("The cat's folder is not where Neko keeps it".into()) }
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

/// Creates a cat's folder with the four seeded files. A file that already exists is left alone.
pub fn create_all(root: &Path, id: &str, purpose: &str) -> Result<(), String> {
    let _guard = WRITING.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let dir = confined_dir(root, id, true)?;
    for kind in CatFileKind::ALL {
        let path = file_in(&dir, kind.file_name())?;
        if std::fs::symlink_metadata(&path).is_err() {
            replace(&dir, &path, seed(kind, purpose).as_bytes())?;
        }
    }
    Ok(())
}

/// Replaces one file with `text`, keeping what it held as `<name>.prev`. Empty or whitespace-only text
/// removes the file (after the copy). At most 16 KB, no NUL.
pub fn write(root: &Path, id: &str, kind: CatFileKind, text: &str) -> Result<(), String> {
    if text.len() > MAX_CAT_FILE {
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
    file.take(MAX_CAT_FILE as u64 + 1).read_to_end(&mut bytes).map_err(|e| format!("Cannot read {}: {e}", path.display()))?;
    let truncated = bytes.len() > MAX_CAT_FILE;
    bytes.truncate(MAX_CAT_FILE);
    let mut text = String::from_utf8_lossy(&bytes).into_owned();
    if truncated {
        // A cut inside a character became U+FFFD at the end; drop it.
        while text.ends_with('\u{fffd}') { text.pop(); }
    }
    Ok(Some((text, truncated, modified)))
}

/// All four files; a missing file reads as empty.
pub fn read_all(root: &Path, id: &str) -> Result<CatFiles, String> {
    let dir = confined_dir(root, id, false)?;
    let mut files = Vec::new();
    for kind in CatFileKind::ALL {
        let current = read_one(&dir.join(kind.file_name()))?;
        let prev = read_one(&dir.join(prev_name(kind)))?.map(|(text, ..)| text);
        let (text, truncated, updated_ms) = current.unwrap_or_default();
        files.push(CatFile { kind, text, updated_ms, prev, truncated });
    }
    Ok(CatFiles { id: id.to_owned(), files })
}

/// What the person or cat wrote in one file (its heading dropped, so a file that is only its heading is
/// empty), as prompts read it. A missing, unreadable or linked file reads as empty: a prompt is never refused
/// because a note is missing.
pub fn prompt_body(root: &Path, id: &str, kind: CatFileKind) -> String {
    let Ok(dir) = confined_dir(root, id, false) else { return String::new() };
    match read_one(&dir.join(kind.file_name())) {
        Ok(Some((text, ..))) => body(&text).to_owned(),
        _ => String::new(),
    }
}

/// The text appended to the rules of every session a cat starts: `TASK_RULES.md` under "Rules from <cat name>".
/// Empty (nothing injected) for a session with no cat and for an empty `TASK_RULES.md`.
pub fn task_rules_section(root: &Path, cat_id: Option<&str>, cat_name: &str) -> String {
    let Some(id) = cat_id else { return String::new() };
    let task_rules = prompt_body(root, id, CatFileKind::TaskRules);
    if task_rules.is_empty() { String::new() } else { format!("\n\nRules from {}\n{}\n", cat_name.trim(), task_rules.trim_end()) }
}

/// What Phase 1 called things before a cat was a cat: the folder `<root>/captains/` and two file names.
const OLD_FOLDER: &str = "captains";
const OLD_FILES: [(&str, &str); 4] = [("CREW.md", "TASK_RULES.md"), ("CREW.md.prev", "TASK_RULES.md.prev"), ("DIGEST.md", "BRIEFING.md"), ("DIGEST.md.prev", "BRIEFING.md.prev")];
/// The seeds those files started with; one that is still exactly its seed takes the new heading.
const OLD_SEEDS: [(&str, &str, &str); 2] = [("TASK_RULES.md", "# Crew rules\n", "# Task rules\n"), ("BRIEFING.md", "# Digest\n", "# Briefing\n")];

fn exists(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok()
}

/// Moves `from` to `to` only when `to` is absent. Never overwrites, never deletes.
fn move_if_free(from: &Path, to: &Path, notes: &mut Vec<String>) {
    if exists(to) {
        notes.push(format!("left {} where it is: {} already exists", from.display(), to.display()));
    } else if let Err(e) = std::fs::rename(from, to) {
        notes.push(format!("could not move {}: {e}", from.display()));
    }
}

/// Once, at startup: `<root>/captains/<id>/` becomes `<root>/cats/<id>/`, `CREW.md` becomes `TASK_RULES.md` and
/// `DIGEST.md` becomes `BRIEFING.md` (their `.prev` copies too). Idempotent, and it never overwrites or deletes
/// content: a file whose new name is taken stays where it is, a link is skipped, and an old folder is removed
/// only when the moves left it empty. Answers what it did not do, one line each, for the log.
pub fn migrate_legacy(root: &Path) -> Vec<String> {
    let _guard = WRITING.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut notes = Vec::new();
    let (old, new) = (root.join(OLD_FOLDER), root.join("cats"));
    if is_link(&old) {
        notes.push(format!("{} is a link; left alone", old.display()));
    } else if old.is_dir() {
        if is_link(&new) {
            notes.push(format!("{} is a link; the cat folders were not moved", new.display()));
            return notes;
        }
        if let Err(e) = std::fs::create_dir_all(&new) {
            notes.push(format!("could not make {}: {e}", new.display()));
            return notes;
        }
        for entry in std::fs::read_dir(&old).into_iter().flatten().flatten() {
            let (from, to) = (entry.path(), new.join(entry.file_name()));
            if is_link(&from) {
                notes.push(format!("{} is a link; left alone", from.display()));
            } else if !exists(&to) {
                move_if_free(&from, &to, &mut notes);
            } else if from.is_dir() && to.is_dir() && !is_link(&to) {
                for file in std::fs::read_dir(&from).into_iter().flatten().flatten() {
                    if file.file_type().is_ok_and(|t| t.is_file()) {
                        move_if_free(&file.path(), &to.join(file.file_name()), &mut notes);
                    }
                }
                let _ = std::fs::remove_dir(&from);
            } else {
                notes.push(format!("left {} where it is: {} already exists", from.display(), to.display()));
            }
        }
        let _ = std::fs::remove_dir(&old);
    }
    if is_link(&new) || !new.is_dir() {
        return notes;
    }
    for entry in std::fs::read_dir(&new).into_iter().flatten().flatten() {
        let dir = entry.path();
        if is_link(&dir) || !dir.is_dir() || !entry.file_name().to_str().is_some_and(crate::artifacts::valid_id) {
            continue;
        }
        for (from, to) in OLD_FILES {
            let from = dir.join(from);
            if exists(&from) && !is_link(&from) {
                move_if_free(&from, &dir.join(to), &mut notes);
            }
        }
        for (name, old_seed, new_seed) in OLD_SEEDS {
            let path = dir.join(name);
            if !is_link(&path) && std::fs::read_to_string(&path).is_ok_and(|text| text == old_seed) {
                if let Err(e) = replace(&dir, &path, new_seed.as_bytes()) {
                    notes.push(e);
                }
            }
        }
    }
    notes
}

#[cfg(test)]
mod tests {
    use super::*;
    use CatFileKind::*;

    fn root() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    fn text_of(files: &CatFiles, kind: CatFileKind) -> &CatFile {
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
        assert!(!root.path().join("cats").exists());
        assert!(!root.path().parent().unwrap().join("x").exists());
    }

    #[test]
    fn create_seeds_rules_from_the_purpose_and_the_others_with_one_heading() {
        let root = root();
        create_all(root.path(), "c1", "Keep the web app fast.").unwrap();
        let files = read_all(root.path(), "c1").unwrap();
        assert_eq!(text_of(&files, Rules).text, "# Rules\n\nKeep the web app fast.\n");
        assert_eq!(text_of(&files, TaskRules).text, "# Task rules\n");
        assert_eq!(text_of(&files, Plan).text, "# Plan\n");
        assert_eq!(text_of(&files, Briefing).text, "# Briefing\n");
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
    fn a_cat_with_no_files_reads_as_four_empty_files() {
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
        assert!(root.path().join("cats/c1/PLAN.md.prev").exists());
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
        std::fs::write(root.path().join("cats/c1/PLAN.md"), &big).unwrap();
        let files = read_all(root.path(), "c1").unwrap();
        let plan = text_of(&files, Plan);
        assert!(plan.truncated && plan.text.len() <= 16_384 && !plan.text.contains('\u{fffd}'));
    }

    #[cfg(unix)]
    #[test]
    fn a_folder_or_file_that_is_a_link_is_refused_and_nothing_outside_is_touched() {
        let root = root();
        let outside = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("cats")).unwrap();
        std::os::unix::fs::symlink(outside.path(), root.path().join("cats/c1")).unwrap();
        assert!(write(root.path(), "c1", Rules, "x").is_err());
        assert!(read_all(root.path(), "c1").is_err());
        assert!(std::fs::read_dir(outside.path()).unwrap().next().is_none());

        create_all(root.path(), "c2", "").unwrap();
        let target = outside.path().join("secret.md");
        std::fs::write(&target, "secret").unwrap();
        std::fs::remove_file(root.path().join("cats/c2/RULES.md")).unwrap();
        std::os::unix::fs::symlink(&target, root.path().join("cats/c2/RULES.md")).unwrap();
        assert!(write(root.path(), "c2", Rules, "overwritten").is_err());
        assert!(read_all(root.path(), "c2").is_err());
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "secret");
    }

    #[test]
    fn body_drops_the_seed_heading_only() {
        assert_eq!(body("# Task rules\n"), "");
        assert_eq!(body("# Rules\n\nBe kind.\n"), "Be kind.");
        assert_eq!(body("Just text\n"), "Just text");
        assert_eq!(body(""), "");
    }
    #[test]
    fn task_rules_section_is_empty_without_a_cat_a_file_or_a_body_and_has_the_exact_heading_otherwise() {
        let root = root();
        assert_eq!(task_rules_section(root.path(), None, "miso"), "");
        assert_eq!(task_rules_section(root.path(), Some("nobody"), "miso"), "", "no folder");
        create_all(root.path(), "c1", "Look after the web app.").unwrap();
        assert_eq!(task_rules_section(root.path(), Some("c1"), "miso"), "", "a heading-only TASK_RULES.md is empty");
        write(root.path(), "c1", TaskRules, "# Task rules\n\nAlways run the linter.\nNo force pushes.\n\n").unwrap();
        assert_eq!(task_rules_section(root.path(), Some("c1"), "miso"), "\n\nRules from miso\nAlways run the linter.\nNo force pushes.\n");
        write(root.path(), "c1", TaskRules, "   \n").unwrap();
        assert_eq!(task_rules_section(root.path(), Some("c1"), "miso"), "", "clearing the file injects nothing");
        assert_eq!(task_rules_section(root.path(), Some("../x"), "miso"), "", "a bad id reads nothing");
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

    fn put(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    fn get(path: &Path) -> String {
        std::fs::read_to_string(path).unwrap()
    }

    #[test]
    fn captain_folders_and_files_move_to_cats_with_their_new_names_and_nothing_is_lost() {
        let root = root();
        let old = root.path().join("captains/c1");
        put(&old.join("RULES.md"), "# Rules\n\nNever touch main.\n");
        put(&old.join("RULES.md.prev"), "older rules");
        put(&old.join("CREW.md"), "# Crew rules\n\nRun the linter.\n");
        put(&old.join("CREW.md.prev"), "# Crew rules\n");
        put(&old.join("PLAN.md"), "# Plan\n\n1. Ship.\n");
        put(&old.join("DIGEST.md"), "# Digest\n");
        put(&old.join("DIGEST.md.prev"), "## Needs you\n- old");
        put(&root.path().join("captains/c2/DIGEST.md"), "# Digest\n\n## New\n- a PR");
        assert_eq!(migrate_legacy(root.path()), Vec::<String>::new());
        let new = root.path().join("cats/c1");
        assert_eq!(get(&new.join("RULES.md")), "# Rules\n\nNever touch main.\n");
        assert_eq!(get(&new.join("RULES.md.prev")), "older rules");
        assert_eq!(get(&new.join("TASK_RULES.md")), "# Crew rules\n\nRun the linter.\n", "edited text is kept as it was");
        assert_eq!(get(&new.join("TASK_RULES.md.prev")), "# Crew rules\n", ".prev is moved verbatim");
        assert_eq!(get(&new.join("PLAN.md")), "# Plan\n\n1. Ship.\n");
        assert_eq!(get(&new.join("BRIEFING.md")), "# Briefing\n", "an untouched seed gets the new heading");
        assert_eq!(get(&new.join("BRIEFING.md.prev")), "## Needs you\n- old");
        assert_eq!(get(&root.path().join("cats/c2/BRIEFING.md")), "# Digest\n\n## New\n- a PR");
        assert!(!new.join("CREW.md").exists() && !new.join("DIGEST.md").exists() && !root.path().join("captains").exists());
        // The cat's own readers see the moved files.
        assert_eq!(prompt_body(root.path(), "c1", TaskRules), "Run the linter.");
        assert_eq!(task_rules_section(root.path(), Some("c1"), "miso"), "\n\nRules from miso\nRun the linter.\n");
        assert_eq!(read_all(root.path(), "c1").unwrap().files.iter().find(|f| f.kind == Briefing).unwrap().prev.as_deref(), Some("## Needs you\n- old"));
    }

    #[test]
    fn moving_twice_or_with_nothing_to_move_changes_nothing_and_makes_no_folder() {
        let root = root();
        assert!(migrate_legacy(root.path()).is_empty());
        assert!(!root.path().join("cats").exists() && !root.path().join("captains").exists());
        put(&root.path().join("captains/c1/CREW.md"), "# Crew rules\n\nRun the linter.\n");
        assert!(migrate_legacy(root.path()).is_empty());
        let once = get(&root.path().join("cats/c1/TASK_RULES.md"));
        assert!(migrate_legacy(root.path()).is_empty());
        assert_eq!(get(&root.path().join("cats/c1/TASK_RULES.md")), once);
        // A cat made after the move keeps its files too.
        create_all(root.path(), "c3", "Look after infra.").unwrap();
        assert!(migrate_legacy(root.path()).is_empty());
        assert!(get(&root.path().join("cats/c3/RULES.md")).contains("Look after infra."));
    }

    #[test]
    fn a_new_name_that_is_already_taken_is_never_overwritten_and_the_old_file_stays() {
        let root = root();
        put(&root.path().join("captains/c1/CREW.md"), "old crew text");
        put(&root.path().join("captains/c1/PLAN.md"), "old plan");
        put(&root.path().join("cats/c1/TASK_RULES.md"), "new task rules");
        put(&root.path().join("cats/c1/PLAN.md"), "new plan");
        put(&root.path().join("captains/c9/RULES.md"), "only an old folder");
        put(&root.path().join("cats/c1/RULES.md"), "kept");
        let notes = migrate_legacy(root.path());
        assert_eq!(notes.len(), 2, "{notes:?}");
        assert_eq!(get(&root.path().join("cats/c1/TASK_RULES.md")), "new task rules");
        assert_eq!(get(&root.path().join("cats/c1/CREW.md")), "old crew text", "left in place, not deleted");
        assert_eq!(get(&root.path().join("cats/c1/PLAN.md")), "new plan");
        assert_eq!(get(&root.path().join("captains/c1/PLAN.md")), "old plan", "the old copy of a clashing file stays");
        assert_eq!(get(&root.path().join("cats/c9/RULES.md")), "only an old folder");
    }

    #[cfg(unix)]
    #[test]
    fn a_link_in_the_way_is_left_alone() {
        let root = root();
        let outside = tempfile::tempdir().unwrap();
        put(&outside.path().join("secret.md"), "outside");
        put(&root.path().join("captains/c1/CREW.md"), "crew");
        std::os::unix::fs::symlink(outside.path(), root.path().join("captains/c2")).unwrap();
        let notes = migrate_legacy(root.path());
        assert!(notes.iter().any(|n| n.contains("is a link")), "{notes:?}");
        assert!(root.path().join("captains/c2").exists() && !root.path().join("cats/c2").exists());
        assert_eq!(get(&outside.path().join("secret.md")), "outside");
        assert_eq!(get(&root.path().join("cats/c1/TASK_RULES.md")), "crew");
        // A link standing in for `captains` itself is not followed either.
        let other = root.path().join("other");
        std::fs::create_dir_all(other.join("c5")).unwrap();
        let linked = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(&other, linked.path().join("captains")).unwrap();
        assert!(!migrate_legacy(linked.path()).is_empty());
        assert!(!linked.path().join("cats").exists());
    }
}
