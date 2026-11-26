//! Neko's own glyphs, drawn on rune-ui's 16 point grid and 1.5 stroke in `currentColor`, for what rune-ui's set lacks.

use rune::Icon;

/// A window with its sidebar: Show / Hide sidebar.
pub static SIDEBAR: Icon = Icon {
    name: "neko-sidebar",
    view: 16.0,
    body: r#"<rect x="2.25" y="3" width="11.5" height="10" rx="2.25" fill="none" stroke="currentColor" stroke-width="1.5"/><path d="M6.25 3.25v9.5" fill="none" stroke="currentColor" stroke-width="1.5"/>"#,
};

/// Two arrows chasing round: Rounds, work that starts by itself.
pub static ROUNDS: Icon = Icon {
    name: "neko-rounds",
    view: 16.0,
    body: r#"<path d="M2.75 7.25V7A3 3 0 0 1 5.75 4h6.5" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/><path d="m10.25 1.75 2.25 2.25-2.25 2.25" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/><path d="M13.25 8.75V9a3 3 0 0 1-3 3h-6.5" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/><path d="m5.75 14.25-2.25-2.25 2.25-2.25" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>"#,
};

/// A pencil over a page: New task.
pub static COMPOSE: Icon = Icon {
    name: "neko-compose",
    view: 16.0,
    body: r#"<path d="M7.25 3H5a2.25 2.25 0 0 0-2.25 2.25v5.75A2.25 2.25 0 0 0 5 13.25h5.75A2.25 2.25 0 0 0 13 11V8.75" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/><path d="M11.6 2.6a1.35 1.35 0 0 1 1.9 1.9L8.4 9.6l-2.4.5.5-2.4z" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round"/>"#,
};

/// A folder with a plus: Add workspace….
pub static FOLDER_PLUS: Icon = Icon {
    name: "neko-folder-plus",
    view: 16.0,
    body: r#"<path d="M13.5 8V6.3c0-.8-.7-1.5-1.5-1.5H8L6.4 3H4c-.8 0-1.5.7-1.5 1.5v7c0 .8.7 1.5 1.5 1.5h4.5" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round" stroke-linecap="round"/><path d="M12 10v4M10 12h4" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>"#,
};
