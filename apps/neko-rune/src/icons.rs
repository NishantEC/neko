//! The small line icons the shell draws: 16-unit, 1.2 stroke, in `currentColor` so the ink of the
//! button or text they sit in tints them. One place, so a change of icon style is one edit.

use rune::Icon;

macro_rules! icon_body {
    ($name:ident, $id:literal, $body:literal) => {
        pub static $name: Icon = Icon { name: concat!("neko-", $id), view: 16.0, body: $body };
    };
}

icon_body!(CALENDAR, "calendar", r#"<rect x="2.5" y="3.5" width="11" height="10" rx="1.5" fill="none" stroke="currentColor" stroke-width="1.2"/><path d="M2.5 6.5H13.5M5.5 2V4.5M10.5 2V4.5" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/>"#);
icon_body!(GEAR, "gear", r#"<circle cx="8" cy="8" r="2.2" fill="none" stroke="currentColor" stroke-width="1.2"/><path d="M8 2V3.6M8 12.4V14M2 8H3.6M12.4 8H14M3.8 3.8L4.9 4.9M11.1 11.1L12.2 12.2M3.8 12.2L4.9 11.1M11.1 4.9L12.2 3.8" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/>"#);
icon_body!(MORE, "more", r#"<circle cx="3.5" cy="8" r="1.1" fill="currentColor"/><circle cx="8" cy="8" r="1.1" fill="currentColor"/><circle cx="12.5" cy="8" r="1.1" fill="currentColor"/>"#);
icon_body!(PLUS, "plus", r#"<path d="M8 3.5V12.5M3.5 8H12.5" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/>"#);
icon_body!(CLOSE_SMALL, "close-small", r#"<path d="M5 5L11 11M11 5L5 11" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/>"#);
icon_body!(SEARCH, "search", r#"<circle cx="7" cy="7" r="3.75" fill="none" stroke="currentColor" stroke-width="1.2"/><path d="M10 10L12.75 12.75" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/>"#);
icon_body!(PULL_REQUEST, "pull-request", r#"<circle cx="4.5" cy="3.5" r="1.4" fill="none" stroke="currentColor" stroke-width="1.2"/><circle cx="4.5" cy="12.5" r="1.4" fill="none" stroke="currentColor" stroke-width="1.2"/><circle cx="11.5" cy="12.5" r="1.4" fill="none" stroke="currentColor" stroke-width="1.2"/><path d="M4.5 5V11M11.5 11V7.5C11.5 6.4 10.6 5.5 9.5 5.5H8M9.5 3.8L8 5.5L9.5 7.2" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round"/>"#);
icon_body!(EYE, "eye", r#"<path d="M1.8 8C3 5.6 5.3 4.2 8 4.2C10.7 4.2 13 5.6 14.2 8C13 10.4 10.7 11.8 8 11.8C5.3 11.8 3 10.4 1.8 8Z" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linejoin="round"/><circle cx="8" cy="8" r="1.8" fill="none" stroke="currentColor" stroke-width="1.2"/>"#);
icon_body!(CLOCK, "clock", r#"<circle cx="8" cy="8" r="5.5" fill="none" stroke="currentColor" stroke-width="1.2"/><path d="M8 5V8L10 9.5" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round"/>"#);
icon_body!(MEMORY, "memory", r#"<path d="M5.5 3C4 3 3 4.1 3 5.4C2.3 5.9 2 6.6 2 7.3C2 8.2 2.5 8.9 3.2 9.2C3.2 10.6 4.2 11.6 5.5 11.6V13H8.5V3Z M10.5 3C12 3 13 4.1 13 5.4C13.7 5.9 14 6.6 14 7.3C14 8.2 13.5 8.9 12.8 9.2C12.8 10.6 11.8 11.6 10.5 11.6V13H8V3Z" fill="none" stroke="currentColor" stroke-width="1.1" stroke-linejoin="round"/>"#);
icon_body!(PEOPLE, "people", r#"<circle cx="6" cy="5.5" r="2.2" fill="none" stroke="currentColor" stroke-width="1.2"/><path d="M2 13C2 10.8 3.8 9.2 6 9.2C8.2 9.2 10 10.8 10 13M10.5 3.8C11.9 3.9 13 5 13 6.3C13 7.3 12.4 8.1 11.5 8.5M11.5 9.4C13 9.8 14 11.2 14 13" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/>"#);
icon_body!(WRENCH, "wrench", r#"<path d="M10.2 2.5C8.4 2.5 7 3.9 7 5.7C7 6.1 7.1 6.5 7.2 6.8L2.7 11.3C2.2 11.8 2.2 12.6 2.7 13.1C3.2 13.6 4 13.6 4.5 13.1L9 8.6C9.3 8.7 9.7 8.8 10.1 8.8C11.9 8.8 13.3 7.4 13.3 5.6L11.5 7.4L9.6 6.4L8.7 4.5L10.5 2.7Z" fill="none" stroke="currentColor" stroke-width="1.1" stroke-linejoin="round"/>"#);
icon_body!(GAUGE, "gauge", r#"<path d="M2.5 11.5A5.5 5.5 0 1 1 13.5 11.5M8 9L10.5 5.5" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/><circle cx="8" cy="9.2" r="1" fill="currentColor"/>"#);
icon_body!(PANEL_RIGHT, "panel-right", r#"<rect x="2.5" y="3" width="11" height="10" rx="1.8" fill="none" stroke="currentColor" stroke-width="1.2"/><path d="M9.5 3V13" fill="none" stroke="currentColor" stroke-width="1.2"/>"#);
icon_body!(SPARK, "spark", r#"<path d="M8 2.5L9.3 6.7L13.5 8L9.3 9.3L8 13.5L6.7 9.3L2.5 8L6.7 6.7Z" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linejoin="round"/>"#);
icon_body!(SEND, "send", r#"<path d="M8 13V3.5M4 7.5L8 3.5L12 7.5" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round"/>"#);
icon_body!(CHECK_LINE, "check-line", r#"<path d="M3.5 8.5L6.5 11.5L12.5 4.5" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round"/>"#);
icon_body!(CHEVRON_LEFT, "chevron-left", r#"<path d="M10 3.5L5.5 8L10 12.5" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round"/>"#);
icon_body!(CHEVRON_RIGHT, "chevron-right", r#"<path d="M6 3.5L10.5 8L6 12.5" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round"/>"#);
icon_body!(PLAY, "play", r#"<path d="M5 3.5L12 8L5 12.5Z" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linejoin="round"/>"#);
icon_body!(EXTERNAL, "external", r#"<path d="M9 3H13V7M13 3L7.5 8.5M11 9.5V12.5H3.5V5H6.5" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round"/>"#);
icon_body!(EXPAND, "expand", r#"<path d="M9.5 2.5H13.5V6.5M6.5 13.5H2.5V9.5M13.5 2.5L9 7M2.5 13.5L7 9" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round"/>"#);
icon_body!(COLLAPSE, "collapse", r#"<path d="M13 6H9V2M3 10H7V14M9 6L13.5 1.5M7 10L2.5 14.5" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round"/>"#);
// The context panel's four tabs when the panel is too narrow for their words (`view/context.rs`).
icon_body!(CANVAS, "canvas", r#"<path d="M8 2L14 8L8 14L2 8Z" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linejoin="round"/>"#);
icon_body!(DIFF, "diff", r#"<path d="M8 2.5V8.5M5 5.5H11M5 12H11" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/>"#);
icon_body!(FILES, "files", r#"<rect x="2.5" y="2.5" width="11" height="11" rx="1.5" fill="none" stroke="currentColor" stroke-width="1.2"/><path d="M5 6H11M5 8.5H11M5 11H9" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/>"#);
icon_body!(TERMINAL, "terminal", r#"<path d="M3.5 4.5L7 8L3.5 11.5M8.5 12H12.5" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round"/>"#);
