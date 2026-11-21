//! The one icon set (polish spec §4): drawn on a 16-unit grid at 16 px, stroke 1.5, round caps and joins,
//! in `currentColor` so the ink of the button or text they sit in tints them. One place, so a change of
//! icon style is one edit. No emoji and no other stroke width: the tests at the foot check both. The only
//! things outside it are the Neko mark (a filled 24-unit grid, as the web draws it), the dot (filled) and
//! the progress ring, whose 2 px track the spec sets.

use rune::Icon;

pub type IconRef = Icon;

macro_rules! icon_body {
    ($name:ident, $id:literal, $body:literal) => {
        pub static $name: Icon = Icon { name: concat!("neko-", $id), view: 16.0, body: $body };
    };
}

icon_body!(INBOX, "inbox", r#"<path d="M2.5 9.5L4.3 3.5H11.7L13.5 9.5V12.5H2.5ZM2.5 9.5H5.8L6.6 11H9.4L10.2 9.5H13.5" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round"/>"#);
icon_body!(CALENDAR, "calendar", r#"<rect x="2.5" y="3.5" width="11" height="10" rx="1.5" fill="none" stroke="currentColor" stroke-width="1.5"/><path d="M2.5 6.5H13.5M5.5 2V4.5M10.5 2V4.5" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>"#);
icon_body!(GEAR, "gear", r#"<circle cx="8" cy="8" r="2.2" fill="none" stroke="currentColor" stroke-width="1.5"/><path d="M8 2V3.6M8 12.4V14M2 8H3.6M12.4 8H14M3.8 3.8L4.9 4.9M11.1 11.1L12.2 12.2M3.8 12.2L4.9 11.1M11.1 4.9L12.2 3.8" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>"#);
icon_body!(MORE, "more", r#"<circle cx="3.5" cy="8" r="1.1" fill="currentColor"/><circle cx="8" cy="8" r="1.1" fill="currentColor"/><circle cx="12.5" cy="8" r="1.1" fill="currentColor"/>"#);
icon_body!(PLUS, "plus", r#"<path d="M8 3.5V12.5M3.5 8H12.5" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>"#);
icon_body!(CLOSE_SMALL, "close-small", r#"<path d="M5 5L11 11M11 5L5 11" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>"#);
icon_body!(SEARCH, "search", r#"<circle cx="7" cy="7" r="3.75" fill="none" stroke="currentColor" stroke-width="1.5"/><path d="M10 10L12.75 12.75" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>"#);
icon_body!(PULL_REQUEST, "pull-request", r#"<circle cx="4.5" cy="3.5" r="1.4" fill="none" stroke="currentColor" stroke-width="1.5"/><circle cx="4.5" cy="12.5" r="1.4" fill="none" stroke="currentColor" stroke-width="1.5"/><circle cx="11.5" cy="12.5" r="1.4" fill="none" stroke="currentColor" stroke-width="1.5"/><path d="M4.5 5V11M11.5 11V7.5C11.5 6.4 10.6 5.5 9.5 5.5H8M9.5 3.8L8 5.5L9.5 7.2" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>"#);
icon_body!(EYE, "eye", r#"<path d="M1.8 8C3 5.6 5.3 4.2 8 4.2C10.7 4.2 13 5.6 14.2 8C13 10.4 10.7 11.8 8 11.8C5.3 11.8 3 10.4 1.8 8Z" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round"/><circle cx="8" cy="8" r="1.8" fill="none" stroke="currentColor" stroke-width="1.5"/>"#);
icon_body!(CLOCK, "clock", r#"<circle cx="8" cy="8" r="5.5" fill="none" stroke="currentColor" stroke-width="1.5"/><path d="M8 5V8L10 9.5" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>"#);
icon_body!(MEMORY, "memory", r#"<path d="M5.5 3C4 3 3 4.1 3 5.4C2.3 5.9 2 6.6 2 7.3C2 8.2 2.5 8.9 3.2 9.2C3.2 10.6 4.2 11.6 5.5 11.6V13H8.5V3Z M10.5 3C12 3 13 4.1 13 5.4C13.7 5.9 14 6.6 14 7.3C14 8.2 13.5 8.9 12.8 9.2C12.8 10.6 11.8 11.6 10.5 11.6V13H8V3Z" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round"/>"#);
icon_body!(PEOPLE, "people", r#"<circle cx="6" cy="5.5" r="2.2" fill="none" stroke="currentColor" stroke-width="1.5"/><path d="M2 13C2 10.8 3.8 9.2 6 9.2C8.2 9.2 10 10.8 10 13M10.5 3.8C11.9 3.9 13 5 13 6.3C13 7.3 12.4 8.1 11.5 8.5M11.5 9.4C13 9.8 14 11.2 14 13" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>"#);
icon_body!(WRENCH, "wrench", r#"<path d="M10.2 2.5C8.4 2.5 7 3.9 7 5.7C7 6.1 7.1 6.5 7.2 6.8L2.7 11.3C2.2 11.8 2.2 12.6 2.7 13.1C3.2 13.6 4 13.6 4.5 13.1L9 8.6C9.3 8.7 9.7 8.8 10.1 8.8C11.9 8.8 13.3 7.4 13.3 5.6L11.5 7.4L9.6 6.4L8.7 4.5L10.5 2.7Z" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round"/>"#);
icon_body!(GAUGE, "gauge", r#"<path d="M2.5 11.5A5.5 5.5 0 1 1 13.5 11.5M8 9L10.5 5.5" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/><circle cx="8" cy="9.2" r="1" fill="currentColor"/>"#);
icon_body!(PANEL_RIGHT, "panel-right", r#"<rect x="2.5" y="3" width="11" height="10" rx="1.8" fill="none" stroke="currentColor" stroke-width="1.5"/><path d="M9.5 3V13" fill="none" stroke="currentColor" stroke-width="1.5"/>"#);
icon_body!(CIRCLE_HALF, "circle-half", r#"<circle cx="8" cy="8" r="5.75" fill="none" stroke="currentColor" stroke-width="1.5"/><path d="M8 2.25V13.75A5.75 5.75 0 0 1 8 2.25Z" fill="currentColor"/>"#);
icon_body!(SUN, "sun", r#"<circle cx="8" cy="8" r="2.75" fill="none" stroke="currentColor" stroke-width="1.5"/><path d="M8 1.75V3M8 13V14.25M1.75 8H3M13 8H14.25M3.6 3.6L4.5 4.5M11.5 11.5L12.4 12.4M3.6 12.4L4.5 11.5M11.5 4.5L12.4 3.6" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>"#);
icon_body!(MOON, "moon", r#"<path d="M12.75 9.6A5.5 5.5 0 0 1 6.4 3.25A5.5 5.5 0 1 0 12.75 9.6Z" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round"/>"#);
icon_body!(GLOBE, "globe", r#"<circle cx="8" cy="8" r="5.75" fill="none" stroke="currentColor" stroke-width="1.5"/><path d="M2.25 8H13.75M8 2.25C9.6 4 10.4 6 10.4 8S9.6 12 8 13.75C6.4 12 5.6 10 5.6 8S6.4 4 8 2.25Z" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round"/>"#);
icon_body!(PAUSE, "pause", r#"<path d="M5.5 3.5V12.5M10.5 3.5V12.5" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>"#);
icon_body!(SPARK, "spark", r#"<path d="M8 2.5L9.3 6.7L13.5 8L9.3 9.3L8 13.5L6.7 9.3L2.5 8L6.7 6.7Z" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round"/>"#);
icon_body!(SEND, "send", r#"<path d="M8 13V3.5M4 7.5L8 3.5L12 7.5" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>"#);
icon_body!(CHECK_LINE, "check-line", r#"<path d="M3.5 8.5L6.5 11.5L12.5 4.5" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>"#);
icon_body!(CHEVRON_LEFT, "chevron-left", r#"<path d="M10 3.5L5.5 8L10 12.5" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>"#);
icon_body!(CHEVRON_RIGHT, "chevron-right", r#"<path d="M6 3.5L10.5 8L6 12.5" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>"#);
icon_body!(PLAY, "play", r#"<path d="M5 3.5L12 8L5 12.5Z" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round"/>"#);
icon_body!(EXTERNAL, "external", r#"<path d="M9 3H13V7M13 3L7.5 8.5M11 9.5V12.5H3.5V5H6.5" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>"#);
icon_body!(EXPAND, "expand", r#"<path d="M9.5 2.5H13.5V6.5M6.5 13.5H2.5V9.5M13.5 2.5L9 7M2.5 13.5L7 9" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>"#);
icon_body!(COLLAPSE, "collapse", r#"<path d="M13 6H9V2M3 10H7V14M9 6L13.5 1.5M7 10L2.5 14.5" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>"#);
// The prototype's own glyphs (2026-11-21, `_src/sprite.html`): Home, Tasks (the issues glyph), Rounds, the briefing sparkle, attach, a cat, the model chip, a merge.
icon_body!(HOME, "home", r#"<g fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M2.5 7.2 8 2.8l5.5 4.4V13a.5.5 0 0 1-.5.5H3a.5.5 0 0 1-.5-.5Z"/></g>"#);
icon_body!(ISSUES, "issues", r#"<g fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><circle cx="8" cy="8" r="5.5"/><circle cx="8" cy="8" r="1.5"/></g>"#);
icon_body!(ROUND, "round", r#"<g fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M3 8a5 5 0 0 1 8.6-3.4L13 6M13 3v3h-3M13 8a5 5 0 0 1-8.6 3.4L3 10M3 13v-3h3"/></g>"#);
icon_body!(SPARKLES, "sparkles", r#"<g fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M7 2.5 8.3 6l3.5 1.3L8.3 8.7 7 12.2 5.7 8.7 2.2 7.3 5.7 6Z"/></g>"#);
icon_body!(CLIP, "clip", r#"<g fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="m12.8 7.4-4.6 4.6a3 3 0 0 1-4.2-4.2l5-5a2 2 0 0 1 2.8 2.8l-5 5a1 1 0 0 1-1.4-1.4l4.4-4.4"/></g>"#);
icon_body!(CAT, "cat", r#"<g fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M3 7V2.8l3 2a6.5 6.5 0 0 1 4 0l3-2V7c0 3.5-2.2 6.2-5 6.2S3 10.5 3 7Z"/><path d="M6 8h.01M10 8h.01M7.4 10.1c.3.3 1.0.3 1.2 0"/></g>"#);
icon_body!(SLIDERS, "sliders", r#"<g fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M2.5 5h5.5M12 5h1.5M2.5 11H4M8 11h5.5"/><circle cx="10" cy="5" r="1.7"/><circle cx="6" cy="11" r="1.7"/></g>"#);
icon_body!(MERGE, "merge", r#"<g fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><circle cx="4.5" cy="3.8" r="1.5"/><circle cx="4.5" cy="12.2" r="1.5"/><circle cx="11.5" cy="8" r="1.5"/><path d="M4.5 5.3v5.4M4.5 6c0 1.4 1.2 2 2.6 2H10"/></g>"#);
// The context panel's four tabs when the panel is too narrow for their words (`view/context.rs`).
icon_body!(CANVAS, "canvas", r#"<path d="M8 2L14 8L8 14L2 8Z" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round"/>"#);
icon_body!(DIFF, "diff", r#"<path d="M8 2.5V8.5M5 5.5H11M5 12H11" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>"#);
icon_body!(FILES, "files", r#"<rect x="2.5" y="2.5" width="11" height="11" rx="1.5" fill="none" stroke="currentColor" stroke-width="1.5"/><path d="M5 6H11M5 8.5H11M5 11H9" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>"#);
icon_body!(TERMINAL, "terminal", r#"<path d="M3.5 4.5L7 8L3.5 11.5M8.5 12H12.5" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>"#);

// The sessions column (polish spec §1).
/// The cat's head on a module grid, as the web's `neko-mark.svg` draws it (24-unit grid, filled).
pub static NEKO_MARK: Icon = Icon { name: "neko-mark", view: 24.0, body: r#"<path d="M4.248 3.115h1.904v1.904h-1.904zM17.848 3.115h1.904v1.904h-1.904zM4.248 5.381h1.904v1.904h-1.904zM6.515 5.381h1.904v1.904h-1.904zM15.581 5.381h1.904v1.904h-1.904zM17.848 5.381h1.904v1.904h-1.904zM4.248 7.648h1.904v1.904h-1.904zM6.515 7.648h1.904v1.904h-1.904zM8.781 7.648h1.904v1.904h-1.904zM13.315 7.648h1.904v1.904h-1.904zM15.581 7.648h1.904v1.904h-1.904zM17.848 7.648h1.904v1.904h-1.904zM4.248 9.915h1.904v1.904h-1.904zM6.515 9.915h1.904v1.904h-1.904zM8.781 9.915h1.904v1.904h-1.904zM11.048 9.915h1.904v1.904h-1.904zM13.315 9.915h1.904v1.904h-1.904zM15.581 9.915h1.904v1.904h-1.904zM17.848 9.915h1.904v1.904h-1.904zM4.248 12.181h1.904v1.904h-1.904zM8.781 12.181h1.904v1.904h-1.904zM11.048 12.181h1.904v1.904h-1.904zM13.315 12.181h1.904v1.904h-1.904zM17.848 12.181h1.904v1.904h-1.904zM1.981 14.448h1.904v1.904h-1.904zM4.248 14.448h1.904v1.904h-1.904zM6.515 14.448h1.904v1.904h-1.904zM8.781 14.448h1.904v1.904h-1.904zM11.048 14.448h1.904v1.904h-1.904zM13.315 14.448h1.904v1.904h-1.904zM15.581 14.448h1.904v1.904h-1.904zM17.848 14.448h1.904v1.904h-1.904zM20.115 14.448h1.904v1.904h-1.904zM6.515 16.715h1.904v1.904h-1.904zM8.781 16.715h1.904v1.904h-1.904zM11.048 16.715h1.904v1.904h-1.904zM13.315 16.715h1.904v1.904h-1.904zM15.581 16.715h1.904v1.904h-1.904zM8.781 18.981h1.904v1.904h-1.904zM11.048 18.981h1.904v1.904h-1.904zM13.315 18.981h1.904v1.904h-1.904z" fill="currentColor"/>"# };
icon_body!(SIDEBAR, "sidebar", r#"<rect x="2.5" y="3" width="11" height="10" rx="2" fill="none" stroke="currentColor" stroke-width="1.5"/><path d="M6.5 3V13" fill="none" stroke="currentColor" stroke-width="1.5"/>"#);
// The "Show sidebar" button of a folded sessions column: a window with its left pane marked off.
icon_body!(SIDEBAR_LEFT, "sidebar-left", r#"<rect x="2" y="2.5" width="12" height="11" rx="2.5" fill="none" stroke="currentColor" stroke-width="1.5"/><path d="M6 2.5V13.5" fill="none" stroke="currentColor" stroke-width="1.5"/>"#);
icon_body!(CHEVRON_DOWN, "chevron-down", r#"<path d="M3.5 6L8 10.5L12.5 6" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>"#);
icon_body!(CHEVRON_UP_DOWN, "chevron-up-down", r#"<path d="M5 6.5L8 3.5L11 6.5M5 9.5L8 12.5L11 9.5" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>"#);
icon_body!(FOLDER, "folder", r#"<path d="M2.5 4.5C2.5 3.7 3.2 3 4 3H6.4L8 4.8H12C12.8 4.8 13.5 5.5 13.5 6.3V11.5C13.5 12.3 12.8 13 12 13H4C3.2 13 2.5 12.3 2.5 11.5Z" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round"/>"#);
// Why a task waits on the person (Needs you rows).
icon_body!(REASON_LIST, "reason-list", r#"<path d="M3 4.5H13M3 8H13M3 11.5H9.5" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>"#);
icon_body!(REASON_QUESTION, "reason-question", r#"<path d="M5.8 6.2C5.8 5 6.8 4.2 8 4.2C9.2 4.2 10.2 5 10.2 6.1C10.2 7.6 8 7.6 8 9.2" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/><path d="M8 11.7V11.8" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>"#);
icon_body!(REASON_BRANCH, "reason-branch", r#"<circle cx="4.5" cy="3.8" r="1.5" fill="none" stroke="currentColor" stroke-width="1.5"/><circle cx="4.5" cy="12.2" r="1.5" fill="none" stroke="currentColor" stroke-width="1.5"/><circle cx="11.5" cy="6.2" r="1.5" fill="none" stroke="currentColor" stroke-width="1.5"/><path d="M4.5 5.3V10.7M11.5 7.7C11.5 9.6 9.5 9.4 7.6 9.6C5.9 9.8 4.5 10.2 4.5 10.7" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>"#);
// The context panel's Plan tab (a checklist) and Info tab.
icon_body!(CHECKLIST, "checklist", r#"<path d="M2.5 4L3.8 5.3L6 3M2.5 9L3.8 10.3L6 8M8.5 4.5H13.5M8.5 9.5H13.5M2.5 13.5H3.5M6 13.5H13.5" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>"#);
icon_body!(INFO, "info", r#"<circle cx="8" cy="8" r="5.5" fill="none" stroke="currentColor" stroke-width="1.5"/><path d="M8 7.2V10.8M8 5.2V5.3" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>"#);
icon_body!(CROSS, "cross", r#"<path d="M4.5 4.5L11.5 11.5M11.5 4.5L4.5 11.5" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>"#);
// The "anything else" reason: a filled dot (the one filled glyph besides the mark).
icon_body!(DOT, "dot", r#"<circle cx="8" cy="8" r="2" fill="currentColor"/>"#);
// The composer and the account menu.
icon_body!(CLAUDE_GLYPH, "runtime-claude", r#"<path d="M8 2.5V13.5M2.9 5.2L13.1 10.8M13.1 5.2L2.9 10.8" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>"#);
icon_body!(CODEX_GLYPH, "runtime-codex", r#"<path d="M3.5 5L6.5 8L3.5 11M8 11.5H12.5" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>"#);
icon_body!(OPENCODE_GLYPH, "runtime-opencode", r#"<path d="M5.5 4.5L2.5 8L5.5 11.5M10.5 4.5L13.5 8L10.5 11.5" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>"#);
icon_body!(POWER, "power", r#"<path d="M8 2.5V7.5M4.9 4.6A5 5 0 1 0 11.1 4.6" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>"#);
icon_body!(ARROW_RIGHT, "arrow-right", r#"<path d="M3 8H12.5M8.5 4L12.5 8L8.5 12" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>"#);
icon_body!(ROWS, "rows", r#"<path d="M2.5 4H13.5M2.5 8H13.5M2.5 12H13.5" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>"#);
icon_body!(KANBAN, "kanban", r#"<rect x="2.5" y="2.5" width="4.5" height="11" rx="1.2" fill="none" stroke="currentColor" stroke-width="1.5"/><rect x="9" y="2.5" width="4.5" height="6.5" rx="1.2" fill="none" stroke="currentColor" stroke-width="1.5"/>"#);
icon_body!(TREE, "tree", r#"<circle cx="4" cy="4" r="1.6" fill="none" stroke="currentColor" stroke-width="1.5"/><circle cx="12" cy="8" r="1.6" fill="none" stroke="currentColor" stroke-width="1.5"/><circle cx="12" cy="12.5" r="1.6" fill="none" stroke="currentColor" stroke-width="1.5"/><path d="M4 5.6V8.5C4 10 5 12.5 10.4 12.5M4 8H10.4" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>"#);
icon_body!(ARROW_UP, "arrow-up", r#"<path d="M8 13V3.5M4 7.5L8 3.5L12 7.5" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>"#);
icon_body!(REFRESH, "refresh", r#"<path d="M13 8A5 5 0 1 1 11.2 4.2M13 2.5V5.2H10.3" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>"#);
icon_body!(FILE, "file", r#"<path d="M4 2.5H9L12 5.5V13.5H4Z M9 2.5V5.5H12" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round"/>"#);
icon_body!(QUOTE, "quote", r#"<path d="M3.5 4V12M6.5 5H12.5M6.5 8H11M6.5 11H12.5" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>"#);
icon_body!(COPY, "copy", r#"<rect x="5.5" y="5.5" width="8" height="8" rx="1.5" fill="none" stroke="currentColor" stroke-width="1.5"/><path d="M10.5 3.5V3.2C10.5 2.8 10.2 2.5 9.8 2.5H3.7C3.3 2.5 3 2.8 3 3.2V9.3C3 9.7 3.3 10 3.7 10H4" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>"#);
icon_body!(UNDO, "undo", r#"<path d="M3 6.5H9.5A3.5 3.5 0 0 1 9.5 13.5H6M3 6.5L5.8 3.7M3 6.5L5.8 9.3" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>"#);
icon_body!(KEYBOARD, "keyboard", r#"<rect x="1.8" y="3.8" width="12.4" height="8.4" rx="1.6" fill="none" stroke="currentColor" stroke-width="1.5"/><path d="M4.5 7H5M7.7 7H8.3M11 7H11.5M5 10H11" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>"#);
icon_body!(EDIT, "edit", r#"<path d="M9.5 3.5L12.5 6.5M3 13L3.6 10L10.8 2.8C11.2 2.4 11.8 2.4 12.2 2.8L13.2 3.8C13.6 4.2 13.6 4.8 13.2 5.2L6 12.4Z" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>"#);
icon_body!(SEND_NOW, "send-now", r#"<path d="M3.5 12.5V8.5A2 2 0 0 1 5.5 6.5H12M9 3.5L12 6.5L9 9.5" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>"#);
icon_body!(DONE, "done", r#"<circle cx="8" cy="8" r="5.5" fill="none" stroke="currentColor" stroke-width="1.5"/><path d="M5.7 8.3L7.3 9.9L10.4 6.4" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>"#);

// The Setup pages' row actions (edit, delete, sync or refresh, sign in).
icon_body!(PENCIL, "pencil", r#"<path d="M3 13L3.6 10.2L10.8 3L13 5.2L5.8 12.4L3 13Z M9.4 4.4L11.6 6.6" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>"#);
icon_body!(TRASH, "trash", r#"<path d="M3 4.5H13M6.2 4.5V3H9.8V4.5M4.5 4.5L5 13H11L11.5 4.5M7 7V10.5M9 7V10.5" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>"#);
icon_body!(LOCK, "lock", r#"<rect x="3.5" y="7" width="9" height="6.5" rx="1.5" fill="none" stroke="currentColor" stroke-width="1.5"/><path d="M5.5 7V5.2C5.5 3.9 6.6 3 8 3C9.4 3 10.5 3.9 10.5 5.2V7" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>"#);
// Cats (`view/cats.rs`): a pushpin for Pin, a box for Archive.
icon_body!(PIN, "pin", r#"<path d="M9.8 2.5L13.5 6.2L11.6 7.2L10.6 10.6L5.4 5.4L8.8 4.4ZM5.6 10.4L2.5 13.5" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>"#);
icon_body!(ARCHIVE, "archive", r#"<rect x="2.5" y="3" width="11" height="3.2" rx="1" fill="none" stroke="currentColor" stroke-width="1.5"/><path d="M3.5 6.2V12C3.5 12.6 4 13 4.6 13H11.4C12 13 12.5 12.6 12.5 12V6.2M6.5 9H9.5" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>"#);
icon_body!(KEY, "key", r#"<circle cx="5.5" cy="10.5" r="2.5" fill="none" stroke="currentColor" stroke-width="1.5"/><path d="M7.3 8.7L13 3M11 5L12.5 6.5" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>"#);

/// A progress ring's arc of `fraction` (0 to 1) on the 16-unit grid: 2 px stroke, starting at the top and
/// running clockwise. Quantised to 2% so the icons cached for it stay few (icons are static, as the kit's
/// own progress circle does it).
pub fn ring_arc(fraction: f32) -> &'static Icon {
    use std::sync::Mutex;
    static CACHE: Mutex<Vec<Option<&'static Icon>>> = Mutex::new(Vec::new());
    let step = (fraction.clamp(0.0, 1.0) * 50.0).round() as usize;
    let mut cache = CACHE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if cache.len() < 51 {
        cache.resize(51, None);
    }
    if let Some(icon) = cache[step] {
        return icon;
    }
    let angle = step as f32 / 50.0 * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
    let (x, y) = (8.0 + 7.0 * angle.cos(), 8.0 + 7.0 * angle.sin());
    let large = i32::from(step > 25);
    let body: &'static str = match step {
        0 => "",
        50 => r#"<circle cx="8" cy="8" r="7" fill="none" stroke="currentColor" stroke-width="2"/>"#,
        _ => Box::leak(format!(r#"<path d="M8 1A7 7 0 {large} 1 {x:.3} {y:.3}" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"/>"#).into_boxed_str()),
    };
    let name: &'static str = Box::leak(format!("neko-ring-{step}").into_boxed_str());
    let icon: &'static Icon = Box::leak(Box::new(Icon { name, view: 16.0, body }));
    cache[step] = Some(icon);
    icon
}

/// Every icon of the set, for a name a plugin gives (`icon = "inbox"`): the name without the `neko-` prefix.
static ALL: &[&Icon] = &[
    &CIRCLE_HALF, &SUN, &MOON, &GLOBE, &PAUSE, &INBOX, &CALENDAR, &GEAR, &MORE, &PLUS, &CLOSE_SMALL, &SEARCH, &PULL_REQUEST, &EYE, &CLOCK, &MEMORY, &PEOPLE, &WRENCH, &GAUGE, &PANEL_RIGHT, &SPARK, &SEND, &CHECK_LINE, &CHEVRON_LEFT, &CHEVRON_RIGHT, &PLAY, &EXTERNAL, &EXPAND, &COLLAPSE, &CANVAS, &DIFF,
    &FILES, &TERMINAL, &SIDEBAR, &SIDEBAR_LEFT, &CHEVRON_DOWN, &CHEVRON_UP_DOWN, &FOLDER, &REASON_LIST, &REASON_QUESTION, &REASON_BRANCH, &CHECKLIST, &INFO, &CROSS, &DOT, &CLAUDE_GLYPH, &CODEX_GLYPH, &OPENCODE_GLYPH, &POWER, &ARROW_UP, &ARROW_RIGHT, &ROWS, &KANBAN, &TREE, &REFRESH, &FILE, &QUOTE, &COPY, &UNDO,
    &KEYBOARD, &EDIT, &SEND_NOW, &DONE, &PENCIL, &TRASH, &LOCK, &PIN, &ARCHIVE, &KEY, &HOME, &ISSUES, &ROUND, &SPARKLES, &CLIP, &CAT, &SLIDERS, &MERGE,
];

/// The icon named `name` (`"inbox"`, `"clock"`, ...), or none: a plugin that names an icon the set does not have gets the dot.
pub fn by_name(name: &str) -> Option<&'static Icon> {
    ALL.iter().copied().find(|i| i.name.strip_prefix("neko-") == Some(name))
}

/// The ring's 2 px track: a full circle (drawn at the foreground's 12%).
pub static RING_TRACK: Icon = Icon { name: "neko-ring-track", view: 16.0, body: r#"<circle cx="8" cy="8" r="7" fill="none" stroke="currentColor" stroke-width="2"/>"# };
/// The indeterminate ring's arc: a quarter turn, rotated by the loop.
pub static RING_QUARTER: Icon = Icon { name: "neko-ring-quarter", view: 16.0, body: r#"<path d="M8 1A7 7 0 0 1 15 8" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"/>"# };

#[cfg(test)]
mod tests {
    use super::*;

    /// Every line icon the shell draws (all but the mark, the dot and the ring, which are named below).
    fn line_icons() -> Vec<&'static Icon> {
        let src = include_str!("icons.rs");
        let declared = src.lines().filter(|l| l.starts_with("icon_body!(")).count();
        let icons = vec![
            &CIRCLE_HALF, &SUN, &MOON, &GLOBE, &PAUSE, &INBOX, &CALENDAR, &GEAR, &MORE, &PLUS, &CLOSE_SMALL, &SEARCH, &PULL_REQUEST, &EYE, &CLOCK, &MEMORY, &PEOPLE, &WRENCH, &GAUGE, &PANEL_RIGHT, &SPARK, &SEND, &CHECK_LINE, &CHEVRON_LEFT, &CHEVRON_RIGHT, &PLAY, &EXTERNAL, &EXPAND,
            &COLLAPSE, &CANVAS, &DIFF, &FILES, &TERMINAL, &SIDEBAR, &SIDEBAR_LEFT, &CHEVRON_DOWN, &CHEVRON_UP_DOWN, &FOLDER, &REASON_LIST, &REASON_QUESTION, &REASON_BRANCH, &CROSS, &CLAUDE_GLYPH, &CODEX_GLYPH, &OPENCODE_GLYPH, &POWER, &ARROW_UP, &ARROW_RIGHT, &ROWS, &KANBAN, &TREE, &REFRESH, &FILE, &QUOTE, &COPY, &UNDO, &KEYBOARD, &EDIT, &SEND_NOW, &DONE, &CHECKLIST, &INFO, &PENCIL, &TRASH, &KEY, &LOCK, &PIN, &ARCHIVE, &HOME, &ISSUES, &ROUND, &SPARKLES, &CLIP, &CAT, &SLIDERS, &MERGE,
        ];
        // The two filled ones (MORE's dots count as line icons here: they carry no stroke) are `icon_body!` lines too.
        assert_eq!(icons.len() + 1, declared, "a declared icon is missing from this list (the dot is the +1)");
        icons
    }

    #[test]
    fn a_plugin_names_an_icon_by_its_short_name_and_an_unknown_name_is_none() {
        let src = include_str!("icons.rs");
        let declared = src.lines().filter(|l| l.starts_with("icon_body!(")).count();
        assert_eq!(ALL.len(), declared, "every declared icon is in the table a plugin picks from");
        assert_eq!(by_name("inbox").map(|i| i.name), Some("neko-inbox"));
        assert_eq!(by_name("clock").map(|i| i.name), Some("neko-clock"));
        assert!(by_name("neko-inbox").is_none() && by_name("nope").is_none() && by_name("").is_none());
        let mut names: Vec<&str> = ALL.iter().map(|i| i.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), ALL.len(), "no icon is listed twice");
    }

    #[test]
    fn one_icon_set_16_unit_grid_stroke_1_5() {
        for icon in line_icons() {
            assert_eq!(icon.view, 16.0, "{} is on the 16 grid", icon.name);
            for width in icon.body.split("stroke-width=\"").skip(1) {
                let value = width.split('"').next().unwrap_or("");
                assert_eq!(value, "1.5", "{} mixes a stroke width: {value}", icon.name);
            }
        }
        // The exceptions are named: the mark is filled on its 24 grid, the dot is filled, the ring's track is 2.
        assert_eq!(NEKO_MARK.view, 24.0);
        assert!(NEKO_MARK.body.contains("fill=\"currentColor\"") && !NEKO_MARK.body.contains("stroke"));
        assert!(DOT.body.contains("fill=\"currentColor\"") && !DOT.body.contains("stroke"));
        assert!(RING_TRACK.body.contains("stroke-width=\"2\"") && ring_arc(0.5).body.contains("stroke-width=\"2\""));
    }

    #[test]
    fn no_icon_is_an_emoji_and_every_name_is_unique() {
        let mut names: Vec<&str> = line_icons().iter().map(|i| i.name).collect();
        names.extend([NEKO_MARK.name, DOT.name]);
        let count = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), count, "unique names");
        let src = include_str!("icons.rs");
        let drawing: String = src.lines().filter(|l| l.contains("icon_body!") || l.contains("Icon {")).collect();
        assert!(drawing.chars().all(|c| (c as u32) < 0x2000), "a pictograph in an icon");
    }

    #[test]
    fn ring_arcs_start_empty_end_full_and_are_cached() {
        assert_eq!(ring_arc(0.0).body, "");
        assert!(ring_arc(1.0).body.contains("<circle"));
        assert!(std::ptr::eq(ring_arc(0.5), ring_arc(0.49)), "quantised to 2%");
        assert!(ring_arc(0.25).body.contains("A7 7 0 0 1"), "a quarter is a short arc");
        assert!(ring_arc(0.75).body.contains("A7 7 0 1 1"), "three quarters is the long way round");
    }
}
