//! A small design lab (the web UI's counterpart is `#/ui`): the theme's tokens and the status
//! glyphs drawn with the real components, so a change to `theme.rs` or `status.rs` can be looked
//! at in a headless snapshot (`--scene lab`) instead of imagined.

use rune::{El, col, el, row, text};

use crate::status::{StatusKind, status_glyph};
use crate::theme::Neko;
use crate::view::COLUMN;

fn heading(label: &str, t: &Neko) -> El {
    text(label.to_owned()).font(t.ty.group).color(t.text.muted)
}

fn swatch(name: &str, colour: rune::Color, t: &Neko) -> El {
    row().gap(8.0).center_y().child(el().size(14.0, 14.0).radius(7.0).fill(colour)).child(text(name.to_owned()).font(t.ty.meta).color(t.text.muted))
}

pub fn lab(t: &Neko) -> El {
    let mut glyphs = row().key("lab-glyphs").gap(20.0).center_y();
    for kind in StatusKind::ALL {
        glyphs = glyphs.child(row().gap(8.0).center_y().child(status_glyph(kind, t, 14.0, Some(""))).child(text(kind.label()).font(t.ty.row).color(t.text.base)));
    }
    let meanings = row().key("lab-meanings").gap(20.0).child(swatch("Iris: now, action", t.meaning.iris, t)).child(swatch("Amber: needs you", t.meaning.amber, t)).child(swatch("Sky: in review", t.meaning.sky, t)).child(swatch("Mint: done", t.meaning.mint, t)).child(swatch("Coral: failed", t.meaning.coral, t));
    let greys = col()
        .key("lab-greys")
        .gap(2.0)
        .child(text("Base: row titles and page text").font(t.ty.row).color(t.text.base))
        .child(text("Muted: group headings, meta").font(t.ty.row).color(t.text.muted))
        .child(text("Faint: ids and times").font(t.ty.row).color(t.text.faint));
    let scale = col()
        .key("lab-scale")
        .gap(4.0)
        .child(text("Page title 20 semibold").font(t.ty.title).color(t.text.base))
        .child(text("Group heading 12.5 medium").font(t.ty.group).color(t.text.muted))
        .child(text("Row title 13.5 regular").font(t.ty.row).color(t.text.base))
        .child(text("Meta 12.5 muted").font(t.ty.meta).color(t.text.muted))
        .child(text("NK-36  ids 12 faint tabular").font(t.ty.id).color(t.text.faint));
    col()
        .key("lab")
        .w_full()
        .max_w(COLUMN)
        .px(32.0)
        .pt(40.0)
        .gap(18.0)
        .items_start()
        .child(text("Design lab").font(t.ty.title).color(t.text.base).role("heading"))
        .child(heading("Status", t))
        .child(glyphs)
        .child(heading("Meaning", t))
        .child(meanings)
        .child(heading("Text", t))
        .child(greys)
        .child(heading("Type scale", t))
        .child(scale)
}
