//! Keycaps — a key combination rendered as physical-looking keys.
//!
//! **Why this is written here rather than vendored.** `longbridge/gpui-component`
//! ships a `Kbd` component that does this, it is Apache-2.0, and this repo
//! already has the attribution machinery for exactly that (see
//! `components/vendor/MANIFEST.md`). It was read and declined on the
//! evidence: of its 324 lines, roughly 250 are `format(Keystroke) -> String`
//! (turning a keystroke into `⌘⌥⇧` symbols) and `binding_for_action`
//! lookups. neko already produces that string itself —
//! `neko_protocol::HotkeyCombo::display` — and has no bound-action lookup to
//! do. The part with real value is a styled div, which has to be rewritten
//! against this app's own tokens regardless. Importing mostly-duplicate code
//! to obtain a div is a worse outcome than writing the div.
//!
//! It also renders **one cap per key** rather than the whole combination in a
//! single box, which is what Raycast does and what makes `⌃⇧K` read as three
//! keys you press instead of one string.

use gpui::{IntoElement, ParentElement, SharedString, Styled, div, px};

use crate::theme;

/// The four modifier symbols `HotkeyCombo::display` emits, in the order it
/// emits them. Each is one `char`, which is what makes [`split_combo`] a
/// scan rather than a parser.
const MODIFIER_SYMBOLS: [char; 4] = ['⌘', '⌥', '⌃', '⇧'];

/// Splits `"⌥Space"` into `["⌥", "Space"]`.
///
/// Works on the *rendered* form rather than on a `HotkeyCombo`, because the
/// daemon sends this label already rendered (it is the accessory of the
/// Summon Hotkey row) and re-deriving it client-side would mean two places
/// that can disagree about what a combination looks like.
///
/// Anything that is not a leading modifier symbol is the key, taken whole —
/// so `"F13"` and `"Space"` stay single caps rather than becoming one cap
/// per letter.
pub fn split_combo(display: &str) -> Vec<String> {
    let mut caps: Vec<String> = Vec::new();
    let mut rest = display;
    while let Some(first) = rest.chars().next() {
        if MODIFIER_SYMBOLS.contains(&first) {
            caps.push(first.to_string());
            rest = &rest[first.len_utf8()..];
        } else {
            break;
        }
    }
    if !rest.is_empty() {
        caps.push(rest.to_string());
    }
    caps
}

/// One key, drawn as a cap.
pub fn keycap(label: impl Into<SharedString>, danger: bool) -> impl IntoElement {
    div()
        .px(px(9.))
        .py(px(5.))
        .min_w(px(28.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(theme::CHIP_RADIUS_PX))
        .bg(theme::active().surface_input)
        .border_1()
        .border_color(if danger {
            theme::active().state_danger_border
        } else {
            theme::active().border_hairline_strong
        })
        .text_size(px(14.))
        .text_color(theme::active().text_primary)
        .child(label.into())
}

/// A whole combination — one cap per key.
///
/// An empty or unset combination renders a single placeholder cap rather
/// than nothing, so the control keeps its shape and stays clickable instead
/// of collapsing to a bare label.
pub fn keycaps(display: &str, danger: bool) -> impl IntoElement {
    let caps = split_combo(display);
    let mut row = div().flex().items_center().gap(px(4.));
    if caps.is_empty() {
        return row.child(keycap("Not set", danger));
    }
    for cap in caps {
        row = row.child(keycap(cap, danger));
    }
    row
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_single_modifier_and_key_becomes_two_caps() {
        assert_eq!(split_combo("⌥Space"), vec!["⌥", "Space"]);
    }

    #[test]
    fn every_modifier_gets_its_own_cap_and_the_key_stays_whole() {
        assert_eq!(split_combo("⌘⌥⌃⇧K"), vec!["⌘", "⌥", "⌃", "⇧", "K"]);
    }

    #[test]
    fn a_multi_character_key_is_one_cap_not_one_per_letter() {
        assert_eq!(split_combo("⌃⇧F13"), vec!["⌃", "⇧", "F13"]);
        assert_eq!(split_combo("Space"), vec!["Space"]);
    }

    #[test]
    fn an_empty_combination_produces_no_caps_so_the_caller_can_show_a_placeholder() {
        assert!(split_combo("").is_empty());
    }

    #[test]
    fn a_modifier_appearing_after_the_key_is_left_inside_it_rather_than_reordered() {
        // `HotkeyCombo::display` always emits modifiers first, so this shape
        // cannot arise from it. If it ever did, the honest rendering is to
        // show it verbatim rather than silently rearrange somebody's
        // combination into one this app finds tidier.
        assert_eq!(split_combo("K⌘"), vec!["K⌘"]);
    }
}
