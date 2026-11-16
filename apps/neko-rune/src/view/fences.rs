//! A reply's fenced block, drawn the way the web's `chat/fences.tsx` draws it once the reply has finished (a block still
//! arriving, and every language nothing registered, stays the code block it is):
//!
//! * ```` ```csv ```` and ```` ```tsv ````: the table viewer's own index (`viewers/table.rs`) drawn inline, header first;
//! * ```` ```3d ```` naming an artifact (`a1b2c3d4e5f6`, `artifact <id> v2`, an `artifact?id=` link): a card that opens it in the
//!   panel's Canvas tab, where the 3D viewer draws it (a reply holds no GPU surface of its own);
//! * any other language a plugin registered a viewer for: the code, with a line that says the web panel shows it.

use std::sync::Arc;

use rune::{El, TextStyle, col, row, text};
use rune_kit::CodeFrame;

use crate::app::App;
use crate::theme::Neko;
use crate::viewers::table::CsvData;

/// The artifact a fence names, if it names one (the web's `artifactInFence`): `a1b2c3d4e5f6`, `artifact <id> v2`, or an
/// `artifact?id=<id>&v=2` link on one line.
pub fn artifact_in_fence(code: &str) -> Option<(String, Option<u32>)> {
    let text = code.trim();
    let id_ok = |s: &str| !s.is_empty() && s.len() <= 64 && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    let version = |s: Option<&str>| -> Option<Option<u32>> {
        match s {
            None => Some(None),
            Some(v) => v.strip_prefix(['v', 'V']).filter(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit())).and_then(|n| n.parse().ok()).map(Some),
        }
    };
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.first().is_some_and(|w| w.eq_ignore_ascii_case("artifact"))
        && (2..=3).contains(&words.len())
        && id_ok(words[1])
        && let Some(v) = version(words.get(2).copied())
    {
        return Some((words[1].to_owned(), v));
    }
    if (1..=2).contains(&words.len())
        && words[0].len() == 12
        && words[0].chars().all(|c| c.is_ascii_hexdigit())
        && let Some(v) = version(words.get(1).copied())
    {
        return Some((words[0].to_owned(), v));
    }
    if !text.contains('\n')
        && let Some(at) = text.find("artifact?id=")
    {
        let rest = &text[at + "artifact?id=".len()..];
        let id: String = rest.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-').collect();
        if id_ok(&id) {
            let v = rest[id.len()..].strip_prefix("&v=").map(|n| n.chars().take_while(char::is_ascii_digit).collect::<String>()).and_then(|n| n.parse().ok());
            return Some((id, v));
        }
    }
    None
}

/// Rows drawn, and columns.
const MAX_ROWS: usize = 40;
const MAX_COLUMNS: usize = 8;

/// The table viewer's index drawn inline: a bordered, rounded block, the header in medium weight, rows of 28 points.
pub fn inline_table(t: &Neko, source: &str, delimiter: u8) -> El {
    let data = CsvData::new(Arc::from(source.as_bytes()), delimiter);
    if data.columns() == 0 {
        return text("This block has no rows.").font(t.ty.meta).color(t.text.muted);
    }
    let columns = data.columns().min(MAX_COLUMNS);
    let width = |i: usize| data.widths.get(i).copied().unwrap_or(120.0);
    let mut header = row().key("fence-table-head").h(28.0).fill(t.surface.card);
    for i in 0..columns {
        header = header.child(row().w(width(i)).h_full().px(10.0).center_y().child(text(data.header.get(i).cloned().unwrap_or_default()).font(TextStyle { weight: 500.0, ..t.ty.meta }).color(t.text.base).truncate()));
    }
    let mut body = col().key("fence-table").role("table").radius(8.0).border(1.0, t.surface.field_border).clip_rounded(8.0).child(header);
    for r in 0..data.rows().min(MAX_ROWS) {
        let cells = data.row(r);
        let mut line = row().h(28.0);
        for i in 0..columns {
            line = line.child(row().w(width(i)).h_full().px(10.0).center_y().child(text(cells.get(i).cloned().unwrap_or_default()).font(t.ty.meta).color(t.text.muted).truncate()));
        }
        body = body.child(line);
    }
    if data.rows() > MAX_ROWS {
        body = body.child(row().h(28.0).px(10.0).center_y().child(text(format!("{} more rows", data.rows() - MAX_ROWS)).font(t.ty.meta).color(t.text.faint)));
    }
    col().w_full().py(4.0).child(body.max_w(720.0))
}

/// A card that sends the reader to the panel: "3D model a1b2c3d4e5f6 · Open in panel".
fn artifact_card(t: &Neko, id: String, version: Option<u32>) -> El {
    let label = format!("3D model {id}");
    row()
        .key(format!("fence-3d-{id}"))
        .role("button")
        .label(format!("{label}. Open in the panel"))
        .focusable(true)
        .w_full()
        .h(40.0)
        .px(12.0)
        .gap(8.0)
        .center_y()
        .radius(8.0)
        .border(1.0, t.surface.field_border)
        .hover(t.surface.hover)
        .focus_ring(2.0, 2.0, t.surface.ring)
        .on_press(move |s: &mut App| {
            if s.selected.is_none() {
                s.show_toast("Open a session to see it in the panel.");
            } else {
                s.show_artifact(&id, version);
            }
        })
        .child(text(label).font(t.ty.row).color(t.text.base).truncate().grow(1.0).min_w(0.0).a11y_hidden())
        .child(text("Open in panel").font(t.ty.meta).color(t.meaning.iris).a11y_hidden())
}

/// Wraps one fenced block: `card` is the code block the kit drew.
pub fn frame(dark: bool, card: El, f: &CodeFrame<'_>) -> El {
    if f.open {
        return card;
    }
    let language = f.language.map(str::to_ascii_lowercase).unwrap_or_default();
    let source = f.source.trim_end_matches('\n');
    // Only a block that is drawn differently needs the theme (a plain code block is returned as it came).
    let special = matches!(language.as_str(), "csv" | "tsv" | "3d") || (!language.is_empty() && crate::viewers::resolve(&language).is_some_and(|spec| crate::viewers::web::is_web(spec.id)));
    if !special {
        return card;
    }
    let t = &crate::theme::theme(dark);
    match language.as_str() {
        "csv" | "tsv" if artifact_in_fence(source).is_none() => inline_table(t, source, if language == "tsv" { b'\t' } else { b',' }),
        "3d" => match artifact_in_fence(source) {
            Some((id, version)) => col().w_full().py(4.0).child(artifact_card(t, id, version)),
            None => card,
        },
        other if !other.is_empty() && crate::viewers::resolve(other).is_some_and(|spec| crate::viewers::web::is_web(spec.id)) => {
            col().w_full().gap(4.0).child(card).child(text("A plugin draws this block in the web panel.").font(t.ty.meta).color(t.text.faint))
        }
        _ => card,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fence_names_an_artifact_in_the_three_shapes_the_web_reads() {
        assert_eq!(artifact_in_fence("a1b2c3d4e5f6"), Some(("a1b2c3d4e5f6".into(), None)));
        assert_eq!(artifact_in_fence("  a1b2c3d4e5f6 v2 \n"), Some(("a1b2c3d4e5f6".into(), Some(2))));
        assert_eq!(artifact_in_fence("artifact my-model_1 v3"), Some(("my-model_1".into(), Some(3))));
        assert_eq!(artifact_in_fence("artifact my-model"), Some(("my-model".into(), None)));
        assert_eq!(artifact_in_fence("#/artifact?id=abc123&v=4"), Some(("abc123".into(), Some(4))));
        assert_eq!(artifact_in_fence("name,age\nada,36"), None, "csv rows are not an artifact");
        assert_eq!(artifact_in_fence("hello"), None, "a bare word is an id only in Neko's own 12-hex shape");
        assert_eq!(artifact_in_fence("a1b2c3d4e5f6 vx"), None);
    }
}
