//! The two places this client reads files itself, read-only and confined (P3a): a session's working
//! folder (the Files tab) and the daemon's artifact store (the Canvas tab). The daemon says where
//! each is (`TaskFolder`, `ArtifactFile`); this module decides what may be opened under it, because a
//! path the daemon names is data, not a permission.
//!
//! The rules, in order, for every read:
//!
//! 1. A path with a `..` component is refused outright, whether or not it would land inside.
//! 2. The path (relative ones taken from the root) is canonicalized, which follows every symlink.
//! 3. The result must be inside the canonical root, so an absolute path elsewhere, and a symlink
//!    that resolves elsewhere, are refused.
//! 4. Nothing under a `.git` (a folder, or the file a linked worktree has) is read or listed.
//! 5. It must be a regular file; a file over [`MAX_PREVIEW`] is not read (`Refused::TooLarge`).
//!
//! Nothing here writes, creates, renames or removes anything (`tests/allow_list.rs` scans for it).

use std::collections::VecDeque;
use std::io::Read;
use std::path::{Component, Path, PathBuf};

/// The most of a file a preview reads: 5 MB, the artifact store's own cap.
pub const MAX_PREVIEW: u64 = 5 * 1024 * 1024;
/// The most files a folder listing keeps (breadth first, so the shallow files come first).
pub const MAX_LISTED: usize = 20_000;

/// Why a path was not opened.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refused {
    /// Outside the root: a `..`, an absolute path elsewhere, or a link that leaves.
    Outside,
    /// Under a `.git`.
    Hidden,
    Missing,
    NotAFile,
    /// Over [`MAX_PREVIEW`] bytes: its size.
    TooLarge(u64),
    Unreadable,
}

impl Refused {
    /// One calm line for the person.
    pub fn line(&self) -> String {
        match self {
            Refused::Outside => "That file is outside this folder, so it isn’t shown.".to_owned(),
            Refused::Hidden => "Git’s own files aren’t shown.".to_owned(),
            Refused::Missing => "That file isn’t there any more.".to_owned(),
            Refused::NotAFile => "That isn’t a file.".to_owned(),
            Refused::TooLarge(_) => "Too large to preview.".to_owned(),
            Refused::Unreadable => "That file couldn’t be read.".to_owned(),
        }
    }
}

/// A root that reads are confined to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Confined {
    root: PathBuf,
}

/// What a folder listing found.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Listing {
    /// Files by path from the root, `/`-separated, sorted.
    pub files: Vec<String>,
    /// More than [`MAX_LISTED`] files exist.
    pub truncated: bool,
}

fn is_git(name: &std::ffi::OsStr) -> bool {
    name == ".git"
}

impl Confined {
    /// A root that exists and is a folder; its canonical form is kept.
    pub fn new(root: &Path) -> Option<Confined> {
        let root = root.canonicalize().ok().filter(|r| r.is_dir())?;
        Some(Confined { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The canonical path `path` names inside the root, or why not (the rules in the module docs).
    pub fn resolve(&self, path: &Path) -> Result<PathBuf, Refused> {
        if path.components().any(|c| matches!(c, Component::ParentDir)) {
            return Err(Refused::Outside);
        }
        let joined = if path.is_absolute() { path.to_path_buf() } else { self.root.join(path) };
        let real = match joined.canonicalize() {
            Ok(real) => real,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // A path that is not there is still refused for leaving the root before it is "missing".
                let lexical_inside = joined.starts_with(&self.root);
                return Err(if lexical_inside { Refused::Missing } else { Refused::Outside });
            }
            Err(_) => return Err(Refused::Unreadable),
        };
        let inside = real.strip_prefix(&self.root).map_err(|_| Refused::Outside)?;
        if inside.components().any(|c| matches!(c, Component::Normal(n) if is_git(n))) {
            return Err(Refused::Hidden);
        }
        Ok(real)
    }

    /// The file's canonical path and size, checked as [`Confined::read`] checks them.
    pub fn file(&self, path: &Path) -> Result<(PathBuf, u64), Refused> {
        let real = self.resolve(path)?;
        let meta = std::fs::metadata(&real).map_err(|_| Refused::Unreadable)?;
        if !meta.is_file() {
            return Err(Refused::NotAFile);
        }
        if meta.len() > MAX_PREVIEW {
            return Err(Refused::TooLarge(meta.len()));
        }
        Ok((real, meta.len()))
    }

    /// The file's bytes: at most [`MAX_PREVIEW`], read through `take` so a file that grows while it
    /// is read is refused too.
    pub fn read(&self, path: &Path) -> Result<(PathBuf, Vec<u8>), Refused> {
        let (real, _) = self.file(path)?;
        let file = std::fs::File::open(&real).map_err(|_| Refused::Unreadable)?;
        let mut bytes = Vec::new();
        file.take(MAX_PREVIEW + 1).read_to_end(&mut bytes).map_err(|_| Refused::Unreadable)?;
        if bytes.len() as u64 > MAX_PREVIEW {
            return Err(Refused::TooLarge(bytes.len() as u64));
        }
        Ok((real, bytes))
    }

    /// The root's files, breadth first, skipping `.git`; folders that are links are not entered
    /// (a link out of the root would list what the reader refuses to open).
    pub fn list(&self) -> Listing {
        let mut files = Vec::new();
        let mut truncated = false;
        let mut queue: VecDeque<PathBuf> = VecDeque::from([PathBuf::new()]);
        'walk: while let Some(rel) = queue.pop_front() {
            let Ok(entries) = std::fs::read_dir(self.root.join(&rel)) else { continue };
            let mut entries: Vec<_> = entries.flatten().collect();
            entries.sort_by_key(|e| e.file_name());
            for entry in entries {
                let name = entry.file_name();
                if is_git(&name) {
                    continue;
                }
                let Ok(kind) = entry.file_type() else { continue };
                let child = rel.join(&name);
                if kind.is_dir() {
                    queue.push_back(child);
                } else if kind.is_file() || kind.is_symlink() {
                    if files.len() >= MAX_LISTED {
                        truncated = true;
                        break 'walk;
                    }
                    files.push(child.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect::<Vec<_>>().join("/"));
                }
            }
        }
        files.sort();
        Listing { files, truncated }
    }
}

/// `1.2 MB`, `340 KB`, `12 bytes`.
pub fn size_words(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    let b = bytes as f64;
    if b >= KB * KB {
        format!("{:.1} MB", b / (KB * KB))
    } else if b >= KB {
        format!("{:.0} KB", b / KB)
    } else {
        format!("{bytes} bytes")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::symlink;

    /// A folder with a file, a nested file, a `.git`, and a secret outside it.
    fn folder(name: &str) -> (PathBuf, PathBuf) {
        let base = std::env::temp_dir().join(format!("nrr-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let root = base.join("work");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(root.join(".git")).unwrap();
        fs::write(root.join("a.txt"), "hello").unwrap();
        fs::write(root.join("src/b.rs"), "fn main() {}").unwrap();
        fs::write(root.join(".git/config"), "secret").unwrap();
        fs::write(base.join("secret.txt"), "outside").unwrap();
        (base, root)
    }

    #[test]
    fn a_file_inside_is_read_by_name_or_by_its_absolute_path() {
        let (base, root) = folder("inside");
        let c = Confined::new(&root).unwrap();
        assert_eq!(c.read(Path::new("a.txt")).unwrap().1, b"hello");
        assert_eq!(c.read(Path::new("src/b.rs")).unwrap().1, b"fn main() {}");
        assert_eq!(c.read(&root.join("a.txt")).unwrap().1, b"hello");
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn dot_dot_an_absolute_path_elsewhere_and_an_escaping_symlink_are_refused() {
        let (base, root) = folder("escape");
        let c = Confined::new(&root).unwrap();
        // `..`, even where it would land inside.
        assert_eq!(c.read(Path::new("../secret.txt")), Err(Refused::Outside));
        assert_eq!(c.read(Path::new("src/../a.txt")), Err(Refused::Outside));
        assert_eq!(c.read(&root.join("src/../../secret.txt")), Err(Refused::Outside));
        // An absolute path outside the folder, existing or not.
        assert_eq!(c.read(&base.join("secret.txt")), Err(Refused::Outside));
        assert_eq!(c.read(Path::new("/etc/hosts")), Err(Refused::Outside));
        assert_eq!(c.read(Path::new("/definitely/not/here")), Err(Refused::Outside));
        // A symlink inside the folder that resolves outside it, to a file and to a folder.
        symlink(base.join("secret.txt"), root.join("link.txt")).unwrap();
        symlink(&base, root.join("up")).unwrap();
        assert_eq!(c.read(Path::new("link.txt")), Err(Refused::Outside));
        assert_eq!(c.read(Path::new("up/secret.txt")), Err(Refused::Outside));
        // A symlink to a file that stays inside is fine.
        symlink(root.join("a.txt"), root.join("same.txt")).unwrap();
        assert_eq!(c.read(Path::new("same.txt")).unwrap().1, b"hello");
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn git_is_never_read_or_listed_and_a_missing_file_is_missing() {
        let (base, root) = folder("git");
        let c = Confined::new(&root).unwrap();
        assert_eq!(c.read(Path::new(".git/config")), Err(Refused::Hidden));
        assert_eq!(c.read(&root.join(".git/config")), Err(Refused::Hidden));
        assert_eq!(c.read(Path::new("nope.txt")), Err(Refused::Missing));
        assert_eq!(c.read(Path::new("src")), Err(Refused::NotAFile));
        assert_eq!(c.list().files, vec!["a.txt".to_owned(), "src/b.rs".to_owned()]);
        // A linked worktree has a `.git` file, not a folder.
        fs::remove_dir_all(root.join(".git")).unwrap();
        fs::write(root.join(".git"), "gitdir: /elsewhere").unwrap();
        assert_eq!(c.read(Path::new(".git")), Err(Refused::Hidden));
        assert!(!c.list().files.iter().any(|f| f.starts_with(".git")));
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn a_file_over_five_megabytes_is_not_read() {
        let (base, root) = folder("big");
        let c = Confined::new(&root).unwrap();
        fs::write(root.join("big.bin"), vec![0u8; MAX_PREVIEW as usize + 1]).unwrap();
        fs::write(root.join("edge.bin"), vec![0u8; MAX_PREVIEW as usize]).unwrap();
        assert_eq!(c.read(Path::new("big.bin")), Err(Refused::TooLarge(MAX_PREVIEW + 1)));
        assert_eq!(c.read(Path::new("edge.bin")).unwrap().1.len() as u64, MAX_PREVIEW);
        assert_eq!(Refused::TooLarge(1).line(), "Too large to preview.");
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn a_listing_skips_links_to_folders_and_stops_at_the_cap_with_the_shallow_files_first() {
        let (base, root) = folder("list");
        symlink(&base, root.join("up")).unwrap();
        let c = Confined::new(&root).unwrap();
        let listing = c.list();
        assert!(listing.files.iter().all(|f| !f.starts_with("up/")), "a link to a folder is not entered: {:?}", listing.files);
        assert!(!listing.truncated);
        // Deep files come after shallow ones when there are too many.
        fs::create_dir_all(root.join("deep/er/est")).unwrap();
        for i in 0..MAX_LISTED {
            fs::write(root.join(format!("deep/er/est/f{i:05}")), "").unwrap();
        }
        let listing = c.list();
        assert!(listing.truncated && listing.files.len() == MAX_LISTED);
        assert!(listing.files.contains(&"a.txt".to_owned()) && listing.files.contains(&"src/b.rs".to_owned()));
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn the_root_must_be_a_folder_that_exists() {
        assert!(Confined::new(Path::new("/definitely/not/here")).is_none());
        assert!(Confined::new(Path::new("/etc/hosts")).is_none());
        assert_eq!(size_words(12), "12 bytes");
        assert_eq!(size_words(2048), "2 KB");
        assert_eq!(size_words(5 * 1024 * 1024 + 512 * 1024), "5.5 MB");
    }
}
