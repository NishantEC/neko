//! A task's title: what the sidebar, the task's band and the window's title show. One function, used by the daemon when it stores a
//! task and by the app when it draws one (so a title stored long ago reads the same way).

/// The longest title, in characters, before it is cut at a word.
pub const MAX: usize = 60;

/// The first line with words in it, its spaces collapsed, cut at a word before [`MAX`] characters with "…". Never more than one
/// line.
pub fn title(words: &str) -> String {
    let line = words.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("");
    let line: String = line.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.chars().count() <= MAX {
        return line;
    }
    let cut: String = line.chars().take(MAX).collect();
    let at = cut.rfind(' ').filter(|&i| i >= MAX / 2).unwrap_or(cut.len());
    format!("{}…", cut[..at].trim_end().trim_end_matches([',', ';', ':', '-']))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_line_with_words_is_the_title() {
        assert_eq!(title("\n\n  Fix the   build \nthen tell Lee"), "Fix the build");
        assert_eq!(title("one"), "one");
    }

    #[test]
    fn a_long_line_is_cut_at_a_word() {
        let long = "Rename every label in the sidebar so it reads the way a Mac app reads and check each one again tomorrow";
        let t = title(long);
        assert!(t.ends_with('…'));
        assert!(t.chars().count() <= MAX + 1);
        assert!(long.starts_with(t.trim_end_matches('…')));
        let unbroken = "x".repeat(200);
        assert_eq!(title(&unbroken).chars().count(), MAX + 1, "no space: cut at the limit");
    }

    #[test]
    fn a_pasted_message_gives_one_short_line() {
        let pasted = "Look at the failing nightly build, find the cause, fix it, and then write up what happened for Asha and Lee\n\nLogs:\nerror[E0308]: mismatched types\n  at src/app.rs:12";
        let t = title(pasted);
        assert!(!t.contains('\n') && t.chars().count() <= MAX + 1, "{t}");
        assert!(t.starts_with("Look at the failing nightly build"), "{t}");
    }
}
