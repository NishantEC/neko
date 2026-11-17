//! The nodes a Luau plugin may place besides stock `@rune/ui` (`row col box text spacer virtualList each`): `ui.input`,
//! `ui.button`, `ui.image` and `ui.glyph`. Every one is drawn here, with Neko's own theme (`host::theme`), and none takes a
//! colour: a plugin names a role (`"muted"`, `"mint"`), and the stock nodes refuse a `#rrggbb` (the setup chunk in `host.rs`).
//!
//! * `ui.button{ label = "Open", on_press = function() ... end, primary = true }`
//! * `ui.input{ key = "q", value = state.q, on_change = function(text) ... end, placeholder = "Filter" }`: a one-line field. Its text
//!   lives in the window (`Luau::inputs`), so typing survives a redraw. A plugin that places one is redrawn every frame, never
//!   from a memo: a memoized subtree would hand the runtime a stale field and undo each keystroke (the canvas spike's bug).
//! * `ui.image{ src = "data:image/png;base64,..." | "<artifact id>/v1.png", w = 96, h = 64 }`: a picture. An artifact path is read
//!   inside the artifact store only (`readers::Confined`); a data address is at most 256 KB.
//! * `ui.glyph{ state = "in-progress" | "Building" | ... }`: Neko's status glyph (DESIGN.md §3).

use std::cell::RefCell;
use std::collections::HashMap;

use rune::{Color, El, Pixels, SurfaceFit, TextEdit, TextStyle, el, row, text, text_input};
use rune_luau::{HostNode, NodeArgs};

use super::host::{Context, theme};
use crate::app::App;
use crate::status::{StatusKind, status_glyph};

/// The most a data address may hold.
pub const MAX_DATA_URI: usize = 256 * 1024;
/// Pictures kept decoded at once, and the most pixels one may have (a data address; an artifact file may have four times as many).
const MAX_IMAGES: usize = 16;
pub const MAX_DATA_PIXELS: u64 = 1_048_576;
pub const MAX_FILE_PIXELS: u64 = 4_194_304;

/// A picture's size from its header alone, before anything is decoded: a small file can unpack to a huge canvas.
fn checked_size(bytes: &[u8], most: u64) -> Result<(), String> {
    let (w, h) = image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format().map_err(|e| e.to_string())?.into_dimensions().map_err(|_| "this picture can't be read".to_owned())?;
    if u64::from(w) * u64::from(h) > most {
        return Err(format!("the picture is {w} by {h}, over {} megapixels", most / 1_048_576));
    }
    Ok(())
}

thread_local! {
    /// The window's inputs as of this frame: the nodes build from it.
    static INPUTS: RefCell<HashMap<(String, String), TextEdit>> = RefCell::new(HashMap::new());
    /// What the plugin last passed as `value`, so a changed `value` replaces the text and an unchanged one leaves what was typed.
    static LAST_VALUE: RefCell<HashMap<(String, String), String>> = RefCell::new(HashMap::new());
    /// Texts to put into inputs at the next tick.
    static SET_VALUES: RefCell<Vec<((String, String), String)>> = const { RefCell::new(Vec::new()) };
    static IMAGES: RefCell<Vec<(String, Pixels)>> = const { RefCell::new(Vec::new()) };
    static WAKER: RefCell<Option<rune::Waker>> = const { RefCell::new(None) };
}

/// The element key of a plugin's input: a hash of the plugin's address, so two plugins whose names run together cannot share one.
pub fn input_element_key(plugin: &str, key: &str) -> String {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    plugin.hash(&mut h);
    format!("luau-input-{:08x}-{key}", h.finish() as u32)
}

/// Forgets what the inputs of a plugin last said (it was reloaded or removed).
pub fn forget_plugin(plugin: &str) {
    LAST_VALUE.with(|m| m.borrow_mut().retain(|(p, _), _| p != plugin));
}

/// The window's inputs, copied for this frame's draw.
pub fn mirror_inputs(inputs: &HashMap<(String, String), TextEdit>) {
    INPUTS.with(|m| *m.borrow_mut() = inputs.clone());
}

/// Texts a plugin set through `value` since the last tick.
pub fn take_set_values() -> Vec<((String, String), String)> {
    SET_VALUES.with(|v| std::mem::take(&mut *v.borrow_mut()))
}

pub fn set_waker(waker: rune::Waker) {
    WAKER.with(|w| *w.borrow_mut() = Some(waker));
}

/// A status glyph's kind from a word: the glyph names (`in-progress`) and the daemon's own (`Building`, `Completed`).
pub fn parse_state(state: &str) -> StatusKind {
    let key: String = state.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_ascii_lowercase();
    match key.as_str() {
        "todo" | "queued" | "on" => StatusKind::Todo,
        "inprogress" | "planning" | "building" | "reviewing" | "awaitingapproval" | "working" | "running" | "held" | "needsyou" => StatusKind::InProgress,
        "review" | "readyforreview" | "inreview" => StatusKind::Review,
        "done" | "completed" | "ok" | "success" | "acted" | "quiet" => StatusKind::Done,
        "failed" | "failure" | "error" => StatusKind::Failed,
        "cancelled" | "canceled" | "stopped" => StatusKind::Cancelled,
        _ => StatusKind::Backlog,
    }
}

fn base64(input: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(input.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0u32);
    for c in input.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            b'=' | b'\n' | b'\r' | b' ' => continue,
            _ => return None,
        };
        acc = (acc << 6) | u32::from(v);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Some(out)
}

/// The picture a `src` names, decoded (a data address now, an artifact file on a worker thread; a placeholder until it lands).
fn picture(src: &str) -> Result<Pixels, String> {
    if let Some(found) = IMAGES.with(|m| m.borrow().iter().find(|(k, _)| k == src).map(|(_, p)| p.clone())) {
        return Ok(found);
    }
    let pixels = Pixels::new();
    if let Some(rest) = src.strip_prefix("data:") {
        if src.len() > MAX_DATA_URI {
            return Err(format!("the picture is over {} KB", MAX_DATA_URI / 1024));
        }
        let (head, body) = rest.split_once(',').ok_or("not a data address")?;
        if !head.starts_with("image/") || !head.ends_with(";base64") || head.starts_with("image/svg") {
            return Err("only base64 PNG, JPEG, GIF and WebP data addresses are shown".to_owned());
        }
        let bytes = base64(body).ok_or("the picture is not valid base64")?;
        checked_size(&bytes, MAX_DATA_PIXELS)?;
        let (rgba, (w, h)) = crate::viewers::image_view::decode_still(&bytes)?;
        pixels.set(w, h, rgba);
    } else {
        let root = neko_protocol::support_dir().join("artifacts");
        let confined = crate::readers::Confined::new(&root).ok_or("there are no artifacts yet")?;
        let (target, done) = (pixels.clone(), WAKER.with(|w| w.borrow().clone()));
        let path = src.to_owned();
        rune_luau::spawn_blocking(move || {
            let Ok((_, bytes)) = confined.read(std::path::Path::new(&path)) else { return };
            if checked_size(&bytes, MAX_FILE_PIXELS).is_err() {
                return;
            }
            if let Ok((rgba, (w, h))) = crate::viewers::image_view::decode_still(&bytes) {
                target.set(w, h, rgba);
                if let Some(wake) = done {
                    wake();
                }
            }
        });
    }
    IMAGES.with(|m| {
        let mut m = m.borrow_mut();
        if m.len() >= MAX_IMAGES {
            m.remove(0);
        }
        m.push((src.to_owned(), pixels.clone()));
    });
    Ok(pixels)
}

fn button(a: &mut NodeArgs) -> Result<El, String> {
    let t = theme();
    let label = a.string("label").or_else(|| a.content()).unwrap_or("").trim().to_owned();
    if label.is_empty() {
        return Err("needs a label".to_owned());
    }
    let primary = a.flag("primary");
    let b = row()
        .role("button")
        .label(label.clone())
        .focusable(true)
        .h(if primary { 28.0 } else { 26.0 })
        .px(if primary { 12.0 } else { 10.0 })
        .radius(t.radius.control)
        .center()
        .fill(if primary { t.meaning.iris } else { t.surface.hover })
        .hover(if primary { Color::WHITE.alpha(0.14) } else { t.surface.active })
        .focus_ring(2.0, 2.0, t.kit.c.border_focus)
        .child(text(label).font(if primary { TextStyle { weight: 500.0, ..t.ty.row } } else { t.ty.meta }).color(if primary { Color::WHITE } else { t.text.base }).truncate().a11y_hidden());
    Ok(a.on_press(b, "on_press"))
}

fn glyph(a: &mut NodeArgs) -> Result<El, String> {
    let t = theme();
    let kind = parse_state(a.string("state").or_else(|| a.content()).unwrap_or(""));
    let size = a.number("size").unwrap_or(14.0).clamp(8.0, 32.0) as f32;
    let label = a.string("label").map(str::to_owned);
    Ok(status_glyph(kind, &t, size, label.as_deref()))
}

fn image(a: &mut NodeArgs) -> Result<El, String> {
    let src = a.string("src").ok_or("needs src")?.to_owned();
    let (w, h) = (a.number("w").unwrap_or(64.0).clamp(8.0, 640.0) as f32, a.number("h").unwrap_or(64.0).clamp(8.0, 640.0) as f32);
    let t = theme();
    let label = a.string("label").unwrap_or("Picture").to_owned();
    match picture(&src) {
        Ok(px) => Ok(el().size(w, h).radius(t.radius.control).role("img").label(label).pixels_fit(&px, SurfaceFit::Fit)),
        Err(why) => Ok(el().size(w, h).radius(t.radius.control).fill(t.surface.hover).role("img").label(format!("{label} (not shown: {why})"))),
    }
}

/// The raw field behind the wrapper `ui.input` in `host.rs` (which keeps `on_change` and calls it when the text moves).
fn input(plugin: &str, a: &mut NodeArgs) -> Result<El, String> {
    let t = theme();
    let key = a.string("key").unwrap_or("input").to_owned();
    let value = a.string("value").unwrap_or("").to_owned();
    let placeholder = a.string("placeholder").unwrap_or("").to_owned();
    let id = (plugin.to_owned(), key.clone());
    // A `value` the plugin changed replaces the text; the same one leaves what the person typed.
    let moved = LAST_VALUE.with(|m| {
        let mut m = m.borrow_mut();
        // A plugin that makes a new key every frame cannot grow this without end.
        if m.len() >= 512 {
            m.clear();
        }
        let was = m.insert(id.clone(), value.clone());
        was.is_some_and(|w| w != value)
    });
    if moved {
        SET_VALUES.with(|v| v.borrow_mut().push((id.clone(), value.clone())));
    }
    let edit = INPUTS.with(|m| m.borrow().get(&id).cloned()).unwrap_or_else(|| TextEdit::new(value.clone()));
    let style = TextStyle { line_height: (13.0f32 * 1.35 * 2.0).round() / 2.0, ..t.kit.ty.ui };
    let accessor = id.clone();
    let field = text_input(&edit, style)
        .key(input_element_key(plugin, &key))
        .single_line()
        .w_full()
        .color(t.text.base)
        .placeholder(placeholder, t.text.faint)
        .caret(t.text.base, t.kit.selection())
        .label(a.string("label").unwrap_or("Text field").to_owned())
        .on_edit(rune::edit_access(move |s: &mut App| s.luau.inputs.entry(accessor.clone()).or_insert_with(|| TextEdit::new(value.clone()))));
    Ok(row().w_full().min_h(28.0).px(8.0).center_y().radius(t.radius.control).border(1.0, t.surface.field_border).child(field))
}

/// The host nodes of one plugin file.
pub fn all(ctx: &Context) -> Vec<HostNode> {
    let plugin = ctx.key.clone();
    let seen = ctx.has_input.clone();
    vec![
        HostNode::new("button", button),
        HostNode::new("glyph", glyph),
        HostNode::new("image", image),
        HostNode::new("input", move |a| {
            seen.set(true);
            input(&plugin, a)
        }),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_status_word_is_a_glyph() {
        assert_eq!(parse_state("in-progress"), StatusKind::InProgress);
        assert_eq!(parse_state("Building"), StatusKind::InProgress);
        assert_eq!(parse_state("AwaitingApproval"), StatusKind::InProgress);
        assert_eq!(parse_state("ReadyForReview"), StatusKind::Review);
        assert_eq!(parse_state("Completed"), StatusKind::Done);
        assert_eq!(parse_state("failed"), StatusKind::Failed);
        assert_eq!(parse_state("cancelled"), StatusKind::Cancelled);
        assert_eq!(parse_state("todo"), StatusKind::Todo);
        assert_eq!(parse_state("???"), StatusKind::Backlog);
    }

    #[test]
    fn base64_decodes_standard_and_url_safe() {
        assert_eq!(base64("aGVsbG8=").unwrap(), b"hello");
        assert_eq!(base64("aGVs\nbG8").unwrap(), b"hello");
        assert_eq!(base64("-_8=").unwrap(), [0xfb, 0xff]);
        assert!(base64("a$b").is_none());
    }

    fn encode(bytes: &[u8]) -> String {
        const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::new();
        for chunk in bytes.chunks(3) {
            let n = chunk.iter().enumerate().fold(0u32, |n, (i, b)| n | (u32::from(*b) << (16 - 8 * i)));
            for i in 0..4 {
                out.push(if i <= chunk.len() { CHARS[((n >> (18 - 6 * i)) & 63) as usize] as char } else { '=' });
            }
        }
        out
    }

    fn png(w: u32, h: u32) -> Vec<u8> {
        let mut out = Vec::new();
        let mut encoder = png::Encoder::new(&mut out, w, h);
        encoder.set_color(png::ColorType::Grayscale);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(&vec![0u8; (w * h) as usize]).unwrap();
        drop(writer);
        out
    }

    #[test]
    fn a_small_file_that_unpacks_to_a_huge_canvas_is_refused_before_it_is_decoded() {
        let big = png(2000, 2000);
        assert!(big.len() < MAX_DATA_URI / 2, "an all-black 2000 by 2000 picture is only {} bytes", big.len());
        let why = picture(&format!("data:image/png;base64,{}", encode(&big))).unwrap_err();
        assert!(why.contains("2000 by 2000"), "{why}");
        assert!(picture(&format!("data:image/png;base64,{}", encode(&png(300, 200)))).is_ok());
    }

    #[test]
    fn a_data_address_is_checked_before_it_is_decoded() {
        assert!(picture("data:text/html;base64,AAAA").is_err());
        assert!(picture("data:image/svg+xml;base64,AAAA").is_err(), "an SVG can name other files");
        assert!(picture("data:image/png,AAAA").is_err(), "base64 only");
        assert!(picture(&format!("data:image/png;base64,{}", "A".repeat(MAX_DATA_URI))).unwrap_err().contains("over"));
    }
}
