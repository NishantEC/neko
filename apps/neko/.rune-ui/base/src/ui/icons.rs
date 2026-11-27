//! rune-ui registry: icons 0.11.0 A small set of 16 point glyphs on one 1.5 stroke, drawn in `currentColor` so a control recolours
//! them per state. Replace or extend with your app's own set. `rune-ui init` copies it.

use rune::Icon;

macro_rules! glyph {
    ($name:ident, $id:literal, $body:literal) => {
        pub static $name: Icon = Icon { name: concat!("rune-ui-", $id), view: 16.0, body: $body };
    };
}


glyph!(CHECK, "check", r#"<path d="M3.5 8.4 6.6 11.3 12.5 4.8" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>"#);
glyph!(MINUS, "minus", r#"<path d="M4 8h8" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>"#);
glyph!(PLUS, "plus", r#"<path d="M8 3.5v9M3.5 8h9" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>"#);
glyph!(CHEVRON_DOWN, "chevron-down", r#"<path d="M4.5 6.4 8 9.9l3.5-3.5" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>"#);
glyph!(CHEVRON_RIGHT, "chevron-right", r#"<path d="M6.4 4.5 9.9 8l-3.5 3.5" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>"#);
glyph!(CHEVRON_LEFT, "chevron-left", r#"<path d="M9.6 4.5 6.1 8l3.5 3.5" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>"#);
glyph!(CHEVRON_UP_DOWN, "chevron-up-down", r#"<path d="M5.2 6.2 8 3.6l2.8 2.6M5.2 9.8 8 12.4l2.8-2.6" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>"#);
glyph!(CLOSE, "close", r#"<path d="M4.8 4.8l6.4 6.4M11.2 4.8l-6.4 6.4" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>"#);
glyph!(SEARCH, "search", r#"<circle cx="7.2" cy="7.2" r="4.2" fill="none" stroke="currentColor" stroke-width="1.5"/><path d="M10.4 10.4 13 13" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>"#);
glyph!(ARROW_RIGHT, "arrow-right", r#"<path d="M3.5 8h8.5M8.8 4.6 12.2 8l-3.4 3.4" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>"#);
glyph!(TRASH, "trash", r#"<path d="M3.5 4.8h9M6.4 4.6V3.4h3.2v1.2M4.8 4.8l.6 7.8h5.2l.6-7.8" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>"#);
glyph!(MORE, "more", r#"<circle cx="4" cy="8" r="1.1" fill="currentColor"/><circle cx="8" cy="8" r="1.1" fill="currentColor"/><circle cx="12" cy="8" r="1.1" fill="currentColor"/>"#);
glyph!(INFO, "info", r#"<circle cx="8" cy="8" r="5.6" fill="none" stroke="currentColor" stroke-width="1.5"/><path d="M8 7.3v3.6" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/><circle cx="8" cy="5.2" r="0.9" fill="currentColor"/>"#);
glyph!(WARNING, "warning", r#"<path d="M8 2.8 13.6 12.6H2.4Z" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round"/><path d="M8 6.6v2.8" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/><circle cx="8" cy="11" r="0.85" fill="currentColor"/>"#);
glyph!(CIRCLE_CHECK, "circle-check", r#"<circle cx="8" cy="8" r="5.6" fill="none" stroke="currentColor" stroke-width="1.5"/><path d="M5.6 8.2 7.3 9.8 10.4 6.4" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>"#);
glyph!(CIRCLE_X, "circle-x", r#"<circle cx="8" cy="8" r="5.6" fill="none" stroke="currentColor" stroke-width="1.5"/><path d="M6.2 6.2l3.6 3.6M9.8 6.2l-3.6 3.6" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>"#);
glyph!(COPY, "copy", r#"<rect x="5.5" y="5.5" width="7" height="7" rx="1.5" fill="none" stroke="currentColor" stroke-width="1.5"/><path d="M3.5 10.2V4.6c0-.6.5-1.1 1.1-1.1h5.6" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>"#);
glyph!(GEAR, "gear", r#"<circle cx="8" cy="8" r="2.2" fill="none" stroke="currentColor" stroke-width="1.5"/><path d="M8 2.5v1.6M8 11.9v1.6M2.5 8h1.6M11.9 8h1.6M4.1 4.1l1.1 1.1M10.8 10.8l1.1 1.1M4.1 11.9l1.1-1.1M10.8 5.2l1.1-1.1" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>"#);
glyph!(USER, "user", r#"<circle cx="8" cy="5.8" r="2.5" fill="none" stroke="currentColor" stroke-width="1.5"/><path d="M3.4 13c.6-2.3 2.4-3.6 4.6-3.6s4 1.3 4.6 3.6" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>"#);
glyph!(HOME, "home", r#"<path d="M2.8 7.2 8 3l5.2 4.2V13H2.8Z" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round"/>"#);
glyph!(INBOX, "inbox", r#"<path d="M2.5 9.5 4.3 3.5h7.4l1.8 6V12.5h-11Z" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round"/><path d="M2.5 9.5h3l1 1.5h3l1-1.5h3" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round"/>"#);
glyph!(FOLDER, "folder", r#"<path d="M2.5 4.5c0-.8.7-1.5 1.5-1.5h2.4L8 4.8h4c.8 0 1.5.7 1.5 1.5v5.2c0 .8-.7 1.5-1.5 1.5H4c-.8 0-1.5-.7-1.5-1.5Z" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round"/>"#);
glyph!(FILE, "file", r#"<path d="M4 2.5h5l3 3v8H4Z" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round"/><path d="M9 2.5v3h3" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round"/>"#);
glyph!(BELL, "bell", r#"<path d="M4.2 11V7.4a3.8 3.8 0 0 1 7.6 0V11l1 1.2H3.2Z" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round"/><path d="M6.6 13.6h2.8" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>"#);
glyph!(CALENDAR, "calendar", r#"<rect x="2.5" y="3.5" width="11" height="10" rx="1.5" fill="none" stroke="currentColor" stroke-width="1.5"/><path d="M5.5 2v2.5M10.5 2v2.5M2.5 6.8h11" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>"#);
glyph!(BOLD, "bold", r#"<path d="M5 3h3.6a2.4 2.4 0 0 1 0 4.8H5Zm0 4.8h4.2a2.6 2.6 0 0 1 0 5.2H5Z" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round"/>"#);
glyph!(ITALIC, "italic", r#"<path d="M7 3h5M4 13h5M9.6 3 6.4 13" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>"#);
glyph!(UNDERLINE, "underline", r#"<path d="M4.5 2.8v4.4a3.5 3.5 0 0 0 7 0V2.8M3.5 13.5h9" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>"#);
glyph!(TAG, "tag", r#"<path d="M3 3h4.6L13 8.4 8.4 13 3 7.6Z" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round"/><circle cx="5.8" cy="5.8" r="0.9" fill="currentColor"/>"#);
glyph!(RING, "ring", r#"<circle cx="8" cy="8" r="5.5" fill="none" stroke="currentColor" stroke-width="1.75"/>"#);
glyph!(ARC, "arc", r#"<path d="M8 2.5A5.5 5.5 0 0 1 13.5 8" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round"/>"#);

// The agent set (0.6.0): sending, stopping, attaching, the message actions, tools, steps and sources.
glyph!(ARROW_UP, "arrow-up", r#"<path d="M8 12.5V3.6M4.6 7 8 3.6 11.4 7" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>"#);
glyph!(ARROW_DOWN, "arrow-down", r#"<path d="M8 3.5v8.9M4.6 9 8 12.4 11.4 9" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>"#);
glyph!(CHEVRON_UP, "chevron-up", r#"<path d="M4.5 9.6 8 6.1l3.5 3.5" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>"#);
glyph!(STOP, "stop", r#"<rect x="4.6" y="4.6" width="6.8" height="6.8" rx="1.4" fill="currentColor"/>"#);
glyph!(PAPERCLIP, "paperclip", r#"<path d="M11 5.4 6.4 10a1.2 1.2 0 0 0 1.7 1.7l4.5-4.5a2.6 2.6 0 0 0-3.7-3.7L4.4 8a4 4 0 0 0 5.7 5.7l2.9-2.9" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>"#);
glyph!(IMAGE, "image", r#"<rect x="2.6" y="3.2" width="10.8" height="9.6" rx="1.6" fill="none" stroke="currentColor" stroke-width="1.5"/><circle cx="6.1" cy="6.6" r="1.1" fill="currentColor"/><path d="M3.2 11.8 6.6 8.6l2.2 2 1.6-1.4 2.6 2.4" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>"#);
glyph!(RETRY, "retry", r#"<path d="M12.5 8.2a4.5 4.5 0 1 1-1.5-3.6M12.3 2.9v2.6H9.7" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>"#);
glyph!(THUMB_UP, "thumb-up", r#"<path d="M5.6 7.1v5.7M5.6 7.1 7.9 3.3c.9 0 1.5.7 1.3 1.6L8.8 6.8h2.7c.8 0 1.4.8 1.2 1.6l-.9 3.4c-.2.6-.7 1-1.3 1H5.6M3.2 7.1h2.4v5.7H3.2z" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>"#);
glyph!(THUMB_DOWN, "thumb-down", r#"<path d="M5.6 8.9V3.2M5.6 8.9 7.9 12.7c.9 0 1.5-.7 1.3-1.6L8.8 9.2h2.7c.8 0 1.4-.8 1.2-1.6l-.9-3.4c-.2-.6-.7-1-1.3-1H5.6M3.2 8.9h2.4V3.2H3.2z" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>"#);
glyph!(TERMINAL, "terminal", r#"<rect x="2.4" y="3.2" width="11.2" height="9.6" rx="1.6" fill="none" stroke="currentColor" stroke-width="1.5"/><path d="M5 6.6 6.9 8 5 9.4M8.4 9.8h2.6" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>"#);
glyph!(WRENCH, "wrench", r#"<path d="M10 2.9a3 3 0 0 0-3 3.8L3.4 10.3a1.25 1.25 0 0 0 1.8 1.8L8.8 8.5A3 3 0 0 0 12.6 5.5l-1.8 1.1-1.5-.4-.4-1.5z" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>"#);
glyph!(SPARKLE, "sparkle", r#"<path d="M8 2.6 9.3 6.2 12.9 7.5 9.3 8.8 8 12.4 6.7 8.8 3.1 7.5 6.7 6.2Z" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round"/>"#);
glyph!(CLOCK, "clock", r#"<circle cx="8" cy="8" r="5.6" fill="none" stroke="currentColor" stroke-width="1.5"/><path d="M8 5v3.2l2.1 1.3" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>"#);
glyph!(CIRCLE, "circle", r#"<circle cx="8" cy="8" r="5.4" fill="none" stroke="currentColor" stroke-width="1.5"/>"#);
glyph!(CIRCLE_DOT, "circle-dot", r#"<circle cx="8" cy="8" r="5.4" fill="none" stroke="currentColor" stroke-width="1.5"/><circle cx="8" cy="8" r="2.2" fill="currentColor"/>"#);
glyph!(CIRCLE_SLASH, "circle-slash", r#"<circle cx="8" cy="8" r="5.4" fill="none" stroke="currentColor" stroke-width="1.5"/><path d="M5.2 10.8 10.8 5.2" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>"#);
glyph!(LINK, "link", r#"<path d="M6.7 9.3 9.3 6.7M7.4 4.8l.9-.9a2.6 2.6 0 0 1 3.8 3.8l-.9.9M8.6 11.2l-.9.9a2.6 2.6 0 0 1-3.8-3.8l.9-.9" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>"#);
glyph!(RESTORE, "restore", r#"<path d="M5.4 4.2 3.2 6.4l2.2 2.2M3.4 6.4h5.8a3.3 3.3 0 0 1 0 6.6H7" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>"#);
glyph!(GLOBE, "globe", r#"<circle cx="8" cy="8" r="5.6" fill="none" stroke="currentColor" stroke-width="1.5"/><path d="M2.6 8h10.8M8 2.4c1.5 1.6 2.2 3.6 2.2 5.6S9.5 12 8 13.6C6.5 12 5.8 10 5.8 8S6.5 4 8 2.4Z" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round"/>"#);
glyph!(SHIELD, "shield", r#"<path d="M8 2.6 12.6 4.4v3.4c0 2.8-2 4.8-4.6 5.6-2.6-.8-4.6-2.8-4.6-5.6V4.4Z" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round"/>"#);
glyph!(GRIP, "grip", r#"<circle cx="6" cy="4.5" r="1" fill="currentColor"/><circle cx="10" cy="4.5" r="1" fill="currentColor"/><circle cx="6" cy="8" r="1" fill="currentColor"/><circle cx="10" cy="8" r="1" fill="currentColor"/><circle cx="6" cy="11.5" r="1" fill="currentColor"/><circle cx="10" cy="11.5" r="1" fill="currentColor"/>"#);
glyph!(FLAG, "flag", r#"<path d="M4 13.5V3M4 3.4h7.6l-1.6 2.8 1.6 2.8H4" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>"#);
glyph!(FOLDER_OPEN, "folder-open", r#"<path d="M2.5 11.8V4.5c0-.8.7-1.5 1.5-1.5h2.4L8 4.8h3.5c.8 0 1.5.7 1.5 1.5v.9" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round"/><path d="M2.6 12.6 4.4 7.9c.2-.5.6-.8 1.1-.8h8c.5 0 .9.5.7 1l-1.5 3.9c-.2.5-.6.8-1.1.8H3.2" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round"/>"#);
glyph!(FILE_CODE, "file-code", r#"<path d="M4 2.5h5l3 3v8H4Z" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round"/><path d="M6.9 8.2 5.8 9.4l1.1 1.2M9.1 8.2l1.1 1.2-1.1 1.2" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round"/>"#);
glyph!(FILE_TEXT, "file-text", r#"<path d="M4 2.5h5l3 3v8H4Z" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round"/><path d="M6 8.4h4M6 10.8h2.6" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round"/>"#);
glyph!(FILE_CONFIG, "file-config", r#"<path d="M4 2.5h5l3 3v8H4Z" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round"/><circle cx="8" cy="9.6" r="1.5" fill="none" stroke="currentColor" stroke-width="1.3"/>"#);
glyph!(COMMIT, "commit", r#"<circle cx="8" cy="8" r="2.6" fill="none" stroke="currentColor" stroke-width="1.5"/><path d="M2 8h3.4M10.6 8H14" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>"#);
glyph!(AT, "at", r#"<circle cx="8" cy="8" r="2.4" fill="none" stroke="currentColor" stroke-width="1.5"/><path d="M10.4 6v2.6c0 1 .7 1.7 1.6 1.7s1.6-.9 1.6-2.3a5.6 5.6 0 1 0-2.2 4.4" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>"#);
glyph!(SLASH, "slash", r#"<rect x="2.6" y="2.6" width="10.8" height="10.8" rx="2.4" fill="none" stroke="currentColor" stroke-width="1.5"/><path d="M9.6 5 6.4 11" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>"#);
glyph!(BUG, "bug", r#"<rect x="5" y="5" width="6" height="8" rx="3" fill="none" stroke="currentColor" stroke-width="1.5"/><path d="M6.3 5.2 5.4 3.4M9.7 5.2l.9-1.8M5 8.4H2.8M13.2 8.4H11M5.2 11.2l-1.8 1M10.8 11.2l1.8 1M8 8v4.6" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round"/>"#);

// 0.11.0: a plug (an integration or plugin), a double chevron (the rest of a row that did not fit), the side panels (a sidebar or a
// right pane that opens and closes), and the modifier keys, which `Kbd` draws itself so they look the same in every font.
glyph!(PLUG, "plug", r#"<path d="M6 2.5v3M10 2.5v3M4.5 5.5h7v2.2a3.5 3.5 0 0 1-7 0ZM8 11.2v2.3" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>"#);
glyph!(CHEVRONS_RIGHT, "chevrons-right", r#"<path d="M3.8 4.5 7.3 8l-3.5 3.5M8.6 4.5 12.1 8l-3.5 3.5" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>"#);
glyph!(PANEL_LEFT, "panel-left", r#"<rect x="2.5" y="3" width="11" height="10" rx="2" fill="none" stroke="currentColor" stroke-width="1.5"/><path d="M6.3 3.2v9.6" fill="none" stroke="currentColor" stroke-width="1.5"/>"#);
glyph!(PANEL_RIGHT, "panel-right", r#"<rect x="2.5" y="3" width="11" height="10" rx="2" fill="none" stroke="currentColor" stroke-width="1.5"/><path d="M9.7 3.2v9.6" fill="none" stroke="currentColor" stroke-width="1.5"/>"#);
glyph!(KEY_COMMAND, "key-command", r#"<path d="M5.5 5.5h5v5h-5ZM5.5 5.5V4A1.5 1.5 0 1 0 4 5.5h1.5M10.5 5.5V4A1.5 1.5 0 1 1 12 5.5h-1.5M5.5 10.5V12A1.5 1.5 0 1 1 4 10.5h1.5M10.5 10.5V12A1.5 1.5 0 1 0 12 10.5h-1.5" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round"/>"#);
glyph!(KEY_OPTION, "key-option", r#"<path d="M3 4.5h3l4 7.5h3M9.5 4.5H13" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>"#);
glyph!(KEY_SHIFT, "key-shift", r#"<path d="M8 3 13 8.4h-2.6V13H5.6V8.4H3Z" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round"/>"#);
glyph!(KEY_RETURN, "key-return", r#"<path d="M12.5 4.5v3.4A1.6 1.6 0 0 1 10.9 9.5H3.8M6.3 6.9 3.7 9.5l2.6 2.6" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>"#);
glyph!(KEY_CONTROL, "key-control", r#"<path d="M4 10.2 8 5.8l4 4.4" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>"#);
