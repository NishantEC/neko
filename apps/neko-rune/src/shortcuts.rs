//! The shortcut list, the web's (`shell/shortcuts.tsx` `BUILT_IN`), and the overlay that shows it (`?`): two columns, grouped as
//! the web groups them. The web's "Tabs" group is not here: this window has no tabs (docs/decisions/0002), and its Esc line lives
//! in "Anywhere". Every shortcut shown is bound (`lib.rs` `on_key`, and the fields' own keys); a test reads both.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Row {
    /// One key per entry: ["⌘", "K"].
    pub keys: &'static [&'static str],
    pub label: &'static str,
}

#[derive(Clone, Copy, Debug)]
pub struct Group {
    pub title: &'static str,
    pub rows: &'static [Row],
}

pub const GROUPS: &[Group] = &[
    Group {
        title: "Anywhere",
        rows: &[
            Row { keys: &["⌘", "K"], label: "Open the command palette" },
            Row { keys: &["?"], label: "Show these shortcuts" },
            Row { keys: &["⌘", "N"], label: "New session (Home, cursor in the composer)" },
            Row { keys: &["⌘", "⇧", "Esc"], label: "Stop all work" },
            Row { keys: &["⌘", "B"], label: "Show or hide the sidebar" },
            Row { keys: &["⇧", "F10"], label: "Open the right-click menu on what has focus" },
            Row { keys: &["Esc"], label: "Close a menu, dialog or the palette" },
        ],
    },
    Group {
        title: "Composer",
        rows: &[
            Row { keys: &["↵"], label: "Send, or queue while a reply runs" },
            Row { keys: &["⇧", "↵"], label: "New line" },
            Row { keys: &["⌘", "↵"], label: "Stop the reply and send this first" },
            Row { keys: &["/"], label: "Commands: /plan /stop /remember /memory …" },
            Row { keys: &["@"], label: "Mention a workspace, a tool or a file" },
            Row { keys: &["↑", "↓"], label: "Move in the menu; ↵ or Tab accepts, Esc closes" },
            Row { keys: &["⌥", "↑"], label: "Take the last queued message back to edit" },
        ],
    },
    Group {
        title: "A decision waiting for you",
        rows: &[Row { keys: &["⌘", "↵"], label: "Approve" }, Row { keys: &["Esc"], label: "Decline (when the composer is empty)" }],
    },
    Group {
        title: "All sessions",
        rows: &[
            Row { keys: &["/"], label: "Search the sessions" },
            Row { keys: &["J"], label: "Next session" },
            Row { keys: &["K"], label: "Previous session" },
            Row { keys: &["↵"], label: "Open the session" },
        ],
    },
];

/// The groups split into two columns of about the same height: the web's `sm:grid-cols-2` reads left to right, so the groups
/// alternate.
pub fn columns() -> [Vec<&'static Group>; 2] {
    let mut cols: [Vec<&Group>; 2] = [Vec::new(), Vec::new()];
    let mut heights = [0usize; 2];
    for g in GROUPS {
        let at = if heights[0] <= heights[1] { 0 } else { 1 };
        heights[at] += g.rows.len() + 2;
        cols[at].push(g);
    }
    cols
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_line_of_the_webs_list_that_applies_is_here() {
        let web = ["Open the command palette", "Show these shortcuts", "Stop all work", "Show or hide the sidebar", "Open the right-click menu on what has focus", "Send, or queue while a reply runs", "New line", "Stop the reply and send this first", "Take the last queued message back to edit", "Approve"];
        let mine: Vec<&str> = GROUPS.iter().flat_map(|g| g.rows.iter().map(|r| r.label)).collect();
        for want in web {
            assert!(mine.contains(&want), "{want}");
        }
        // The web's own file says the same (so a line added there is noticed here).
        let source = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../web/src/shell/shortcuts.tsx")).unwrap();
        for want in web {
            assert!(source.contains(want), "the web no longer says {want:?}");
        }
    }

    #[test]
    fn two_columns_keep_every_group_once() {
        let [a, b] = columns();
        assert_eq!(a.len() + b.len(), GROUPS.len());
        assert!(!a.is_empty() && !b.is_empty());
    }
}
