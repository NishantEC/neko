//! The cats' faces (docs/knowledge/cats.md): one small drawing per face id the daemon knows (`neko_protocol::cats::FACES`), on
//! rune-ui's 16 point grid in `currentColor`, so each takes the ink it is given. Plain on purpose: the redesign replaces them.

use rune::{El, Icon};

use crate::ui::base::{Theme, glyph};

/// A cat's head: two ears, a round face, two eyes and a small nose. Every face starts from it.
macro_rules! head {
    ($marks:literal) => {
        concat!(
            r#"<path d="M3.1 6.6 3.5 2.4l2.9 2.1a6.4 6.4 0 0 1 3.2 0l2.9-2.1.4 4.2c.4.8.6 1.6.6 2.5 0 2.7-2.4 4.6-5.5 4.6S2.5 11.8 2.5 9.1c0-.9.2-1.7.6-2.5z" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linejoin="round"/>"#,
            $marks,
            r#"<circle cx="6" cy="8.4" r=".85" fill="currentColor"/><circle cx="10" cy="8.4" r=".85" fill="currentColor"/><path d="M7.4 10.2h1.2L8 10.9z" fill="currentColor" stroke="currentColor" stroke-width=".6" stroke-linejoin="round"/>"#
        )
    };
}

pub static TABBY: Icon = Icon { name: "neko-face-tabby", view: 16.0, body: head!(r#"<path d="M6.9 4.9v1.3M8 4.6v1.6M9.1 4.9v1.3" fill="none" stroke="currentColor" stroke-width="1" stroke-linecap="round"/>"#) };
pub static GINGER: Icon = Icon { name: "neko-face-ginger", view: 16.0, body: head!(r#"<path d="M1.2 9.6h2.3M1.5 11.1l2.1-.6M14.8 9.6h-2.3M14.5 11.1l-2.1-.6" fill="none" stroke="currentColor" stroke-width=".9" stroke-linecap="round"/>"#) };
pub static TUXEDO: Icon = Icon { name: "neko-face-tuxedo", view: 16.0, body: head!(r#"<path d="M6.2 14.8 8 13.9l1.8.9v-1.9l-1.8.9-1.8-.9z" fill="currentColor"/>"#) };
pub static CALICO: Icon = Icon { name: "neko-face-calico", view: 16.0, body: head!(r#"<path d="M3.4 6.2 3.7 3l2.4 1.8C4.9 5 4 5.5 3.4 6.2z" fill="currentColor"/><circle cx="11.4" cy="11" r="1" fill="currentColor" fill-opacity=".45"/>"#) };
pub static SIAMESE: Icon = Icon { name: "neko-face-siamese", view: 16.0, body: head!(r#"<ellipse cx="8" cy="10.4" rx="2.4" ry="1.7" fill="currentColor" fill-opacity=".35"/>"#) };
pub static VOID: Icon = Icon { name: "neko-face-void", view: 16.0, body: head!(r#"<path d="M3.1 6.6 3.5 2.4l2.9 2.1a6.4 6.4 0 0 1 3.2 0l2.9-2.1.4 4.2c.4.8.6 1.6.6 2.5 0 2.7-2.4 4.6-5.5 4.6S2.5 11.8 2.5 9.1c0-.9.2-1.7.6-2.5z" fill="currentColor" fill-opacity=".4"/>"#) };
pub static SNOW: Icon = Icon { name: "neko-face-snow", view: 16.0, body: head!("") };
pub static SMOKE: Icon = Icon { name: "neko-face-smoke", view: 16.0, body: head!(r#"<path d="M3.1 6.6 3.5 2.4l2.9 2.1a6.4 6.4 0 0 1 3.2 0l2.9-2.1.4 4.2c.4.8.6 1.6.6 2.5 0 2.7-2.4 4.6-5.5 4.6S2.5 11.8 2.5 9.1c0-.9.2-1.7.6-2.5z" fill="currentColor" fill-opacity=".18"/><path d="M1.4 10h2M14.6 10h-2" fill="none" stroke="currentColor" stroke-width=".9" stroke-linecap="round"/>"#) };

/// Each face id with its drawing and its name in words, in the daemon's order.
pub const ALL: &[(&str, &Icon, &str)] = &[
    ("tabby", &TABBY, "Tabby"),
    ("ginger", &GINGER, "Ginger"),
    ("tuxedo", &TUXEDO, "Tuxedo"),
    ("calico", &CALICO, "Calico"),
    ("siamese", &SIAMESE, "Siamese"),
    ("void", &VOID, "Black"),
    ("snow", &SNOW, "White"),
    ("smoke", &SMOKE, "Grey"),
];

/// The drawing of a face id (an unknown one gets the plain head).
pub fn icon(face: &str) -> &'static Icon {
    ALL.iter().find(|(id, ..)| *id == face).map_or(&SNOW, |(_, i, _)| i)
}

/// A face's name in words ("Tabby").
pub fn words(face: &str) -> &'static str {
    ALL.iter().find(|(id, ..)| *id == face).map_or("Cat", |(.., w)| w)
}

/// A face, drawn at `size` in the second ink. Hidden from assistive technology: the cat's name says who it is.
pub fn face(t: &Theme, face: &str, size: f32) -> El {
    glyph(icon(face), size, t.ink_muted())
}

#[cfg(test)]
mod tests {
    #[test]
    fn every_face_the_daemon_knows_is_drawn_once() {
        let ids: Vec<&str> = super::ALL.iter().map(|(id, ..)| *id).collect();
        assert_eq!(ids, neko_protocol::cats::FACES);
    }
}
