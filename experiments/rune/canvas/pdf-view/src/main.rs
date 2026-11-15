//! A PDF page viewer: `hayro` (MIT OR Apache-2.0, pure Rust, CPU) renders each page to RGBA and
//! the page is shown with `El::pixels_fit`. `PDF_FILE=assets/sample.pdf pdf-view --snap out.png`.
//!
//! Render timings go to stderr: `pdf-view: page N 612x792 pt at 2x: X ms`.

use hayro::hayro_interpret::InterpreterSettings;
use hayro::hayro_syntax::Pdf;
use hayro::vello_cpu::color::palette::css::WHITE;
use hayro::{PixmapSettings, RenderCache, RenderSettings, render};
use rune::tok::{self, radius, space, type_};
use rune::{AppSpec, Cx, El, Pixels, SurfaceFit, col, el, row, text};
use std::time::Instant;

struct State {
    pages: Vec<Pixels>,
    page: usize,
    note: String,
}

fn load() -> (Vec<Pixels>, String) {
    let path = std::env::var("PDF_FILE").unwrap_or_else(|_| "assets/sample.pdf".into());
    let data = match std::fs::read(&path) {
        Ok(d) => d,
        Err(e) => return (vec![], format!("cannot read {path}: {e}")),
    };
    let t0 = Instant::now();
    let pdf = match Pdf::new(data) {
        Ok(p) => p,
        Err(e) => return (vec![], format!("not a PDF: {e:?}")),
    };
    let parse_ms = t0.elapsed().as_secs_f64() * 1e3;
    let scale: f32 = std::env::var("PDF_SCALE").ok().and_then(|s| s.parse().ok()).unwrap_or(2.0);
    let cache = RenderCache::new();
    let settings = InterpreterSettings::default();
    let render_settings = RenderSettings::default();
    let pixmap_settings = PixmapSettings { x_scale: scale, y_scale: scale, bg_color: WHITE };
    let max_pages: usize = std::env::var("PDF_MAX_PAGES").ok().and_then(|s| s.parse().ok()).unwrap_or(usize::MAX);
    let mut out = vec![];
    let mut total = 0.0;
    for (i, page) in pdf.pages().iter().enumerate().take(max_pages) {
        let t = Instant::now();
        let pixmap = render(page, &cache, &settings, &render_settings, &pixmap_settings);
        let ms = t.elapsed().as_secs_f64() * 1e3;
        total += ms;
        let (w, h) = (pixmap.width() as u32, pixmap.height() as u32);
        // The pixmap is premultiplied; the page is drawn on an opaque white background, so it is
        // already the straight-alpha sRGB that Pixels wants.
        let px = Pixels::new();
        px.set(w, h, pixmap.data_as_u8_slice().to_vec());
        eprintln!("pdf-view: page {} {w}x{h} px at {scale}x: {ms:.1} ms", i + 1);
        out.push(px);
    }
    let note = format!("{path}: {} pages, parse {parse_ms:.1} ms, render {total:.0} ms total at {scale}x", pdf.pages().len());
    eprintln!("pdf-view: {note}");
    (out, note)
}

fn init() -> State {
    let (pages, note) = load();
    State { pages, page: 0, note }
}

fn button(label: &'static str, key: &'static str, f: impl Fn(&mut State) + 'static) -> El {
    let t = tok::theme(false);
    row().key(key).role("button").label(label).focusable(true).px(space::S3).py(space::S1).radius(radius::MD).fill(t.control_off)
        .child(text(label).font(type_::BODY).color(t.text))
        .on_press(move |s: &mut State| f(s))
}

fn view(s: &State, _: &mut Cx<State>) -> El {
    let t = tok::theme(false);
    let n = s.pages.len();
    let bar = row().gap(space::S2).items_center()
        .child(button("Previous", "prev", |s| s.page = s.page.saturating_sub(1)))
        .child(button("Next", "next", |s| s.page = (s.page + 1).min(s.pages.len().saturating_sub(1))))
        .child(text(format!("page {} of {n}", s.page + 1)).font(type_::BODY).color(t.text))
        .child(text(s.note.clone()).font(type_::BODY).color(t.text_secondary));
    let page = match s.pages.get(s.page) {
        Some(p) => el().key("page").grow(1.0).w_full().radius(radius::MD).pixels_fit(p, SurfaceFit::Fit),
        None => el().grow(1.0).w_full(),
    };
    col().w_full().h_full().padding(space::S3).gap(space::S3).fill(t.bg).child(bar).child(page)
}

fn main() {
    let spec = AppSpec::<State> {
        scenes: &[("page1", |_| {}), ("page2", |s| s.page = 1)],
        ..AppSpec::new("PDF view", (760.0, 900.0), init, view)
    };
    rune::run(spec);
}
