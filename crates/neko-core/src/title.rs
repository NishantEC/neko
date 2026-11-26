//! A task's title: what the sidebar and the window's title show.

/// The longest title, in characters, before it is cut at a word.
pub const MAX: usize = 80;

/// The first line with words in it, its spaces collapsed, cut at a word before [`MAX`] characters with "…".
pub fn title(words: &str) -> String {
    let line = words.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("");
    let line: String = line.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.chars().count() <= MAX {
        return line;
    }
    let cut: String = line.chars().take(MAX).collect();
    let at = cut.rfind(' ').filter(|&i| i >= MAX / 2).unwrap_or(cut.len());
    format!("{}…", cut[..at].trim_end())
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
}
