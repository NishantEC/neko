//! Neko's own glyphs, drawn on rune-ui's 16 point grid and 1.5 stroke in `currentColor`, for what rune-ui's set lacks.

use rune::Icon;

/// A window with its sidebar: Show / Hide sidebar.
pub static SIDEBAR: Icon = Icon {
    name: "neko-sidebar",
    view: 16.0,
    body: r#"<rect x="2.25" y="3" width="11.5" height="10" rx="2.25" fill="none" stroke="currentColor" stroke-width="1.5"/><path d="M6.25 3.25v9.5" fill="none" stroke="currentColor" stroke-width="1.5"/>"#,
};

/// A pencil over a page: New task.
pub static COMPOSE: Icon = Icon {
    name: "neko-compose",
    view: 16.0,
    body: r#"<path d="M7.25 3H5a2.25 2.25 0 0 0-2.25 2.25v5.75A2.25 2.25 0 0 0 5 13.25h5.75A2.25 2.25 0 0 0 13 11V8.75" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/><path d="M11.6 2.6a1.35 1.35 0 0 1 1.9 1.9L8.4 9.6l-2.4.5.5-2.4z" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round"/>"#,
};
