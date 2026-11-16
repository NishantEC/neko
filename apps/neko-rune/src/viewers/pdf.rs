//! The PDF viewer: `hayro` (MIT or Apache-2.0, pure Rust, from the canvas spike) draws pages on the
//! CPU, 1 to 3 ms each. Pages are rows of one scrolling column; a page is drawn when it comes on
//! screen, at twice the size it shows at (so it is sharp), and at most [`KEEP_PAGES`] drawn pages are
//! kept: the ones furthest from the reader are let go first. The page counter reads "3 / 16", and
//! ⌘+, ⌘− and ⌘0 zoom ([`ZOOM_STEPS`]); a new zoom draws the pages on screen again.

use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use hayro::hayro_interpret::InterpreterSettings;
use hayro::hayro_syntax::Pdf;
use hayro::vello_cpu::color::palette::css::WHITE;
use hayro::{PixmapSettings, RenderCache, RenderSettings, render};
use rune::{El, GrowthSpec, Pixels, SurfaceFit, col, el, text};

use super::{ViewerInput, ViewerKey, ViewerState};
use crate::app::App;
use crate::theme::theme;

/// Drawn pages kept at once.
pub const KEEP_PAGES: usize = 6;
/// A page's width in points at zoom 1 (the panel's default width less its margins).
pub const PAGE_WIDTH: f32 = 388.0;
/// The air under each page.
const GAP: f32 = 12.0;
/// Pages are drawn at this many pixels per point of the size they show at.
pub const DRAW_SCALE: f32 = 2.0;
/// Zoom steps, and the least and most.
pub const ZOOM_STEPS: [f32; 9] = [0.5, 0.67, 0.8, 1.0, 1.25, 1.5, 2.0, 2.5, 3.0];

/// The bytes of the panel's copy, as hayro wants them: shared, never copied.
struct Shared(Arc<[u8]>);

impl AsRef<[u8]> for Shared {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

/// A PDF read: the document (kept to draw pages from, on any thread) and each page's size in points.
pub struct Parsed {
    pdf: Option<Arc<Pdf>>,
    sizes: Vec<(f32, f32)>,
    error: Option<String>,
}

/// Reads a PDF. Runs off the UI thread.
pub fn parse(bytes: Arc<[u8]>) -> Parsed {
    match Pdf::new(Arc::new(Shared(bytes))) {
        Ok(pdf) => {
            let sizes: Vec<(f32, f32)> = pdf.pages().iter().map(|p| p.render_dimensions()).collect();
            let error = sizes.is_empty().then(|| "This PDF has no pages.".to_owned());
            Parsed { pdf: Some(Arc::new(pdf)), sizes, error }
        }
        Err(e) => Parsed { pdf: None, sizes: Vec::new(), error: Some(format!("This PDF couldn’t be read ({e:?}).")) },
    }
}

/// One page drawn, as plain pixels that can cross threads.
pub struct Rendered {
    pub page: usize,
    pub zoom_key: u32,
    pub size: (u32, u32),
    pub rgba: Vec<u8>,
}

/// What a zoom is called in a job's tag and a page's key (hundredths).
pub fn zoom_key(zoom: f32) -> u32 {
    (zoom * 100.0).round() as u32
}

/// Draws page `page` of `pdf` at `zoom`, at [`DRAW_SCALE`] pixels per point of the size it shows at.
/// Runs off the UI thread.
pub fn render_page(pdf: &Pdf, page: usize, zoom: f32) -> Option<Rendered> {
    let pages = pdf.pages();
    let p = pages.get(page)?;
    let (w, _) = p.render_dimensions();
    let scale = (DRAW_SCALE * PAGE_WIDTH * zoom / w.max(1.0)).clamp(0.2, 8.0);
    let pixmap = render(p, &RenderCache::new(), &InterpreterSettings::default(), &RenderSettings::default(), &PixmapSettings { x_scale: scale, y_scale: scale, bg_color: WHITE });
    // The page is drawn on opaque white, so the premultiplied pixels are already straight alpha.
    Some(Rendered { page, zoom_key: zoom_key(zoom), size: (u32::from(pixmap.width()), u32::from(pixmap.height())), rgba: pixmap.data_as_u8_slice().to_vec() })
}

/// A parsed PDF and the pages drawn so far.
pub struct PdfDoc {
    pdf: Option<Arc<Pdf>>,
    /// Each page's size in points.
    pub sizes: Vec<(f32, f32)>,
    drawn: HashMap<usize, Pixels>,
    /// Drawn pages, least recently asked for first.
    order: Vec<usize>,
    /// Pages being drawn now, with the zoom they were asked at.
    pending: Vec<(usize, u32)>,
    /// The zoom, the first visible page and how many are visible, as the last [`PdfDoc::want`] saw them.
    zoom: u32,
    middle: usize,
    visible_len: usize,
    /// Changes whenever a page lands or the pages are forgotten, so the rows are built again.
    pub generation: u32,
    pub error: Option<String>,
}

impl PdfDoc {
    pub fn from_parsed(parsed: Parsed) -> PdfDoc {
        PdfDoc { pdf: parsed.pdf, sizes: parsed.sizes, drawn: HashMap::new(), order: Vec::new(), pending: Vec::new(), zoom: 0, middle: 0, visible_len: 0, generation: 0, error: parsed.error }
    }

    /// Reads a PDF here and now (a test; the viewer reads it off the UI thread).
    pub fn load(bytes: &[u8]) -> PdfDoc {
        PdfDoc::from_parsed(parse(Arc::from(bytes)))
    }

    pub fn pages(&self) -> usize {
        self.sizes.len()
    }

    /// Drawn pages held now.
    pub fn held(&self) -> usize {
        self.drawn.len()
    }

    pub fn is_drawn(&self, page: usize) -> bool {
        self.drawn.contains_key(&page)
    }

    pub fn pixels(&self, page: usize) -> Option<&Pixels> {
        self.drawn.get(&page)
    }

    /// The document, to draw pages from on another thread.
    pub fn document(&self) -> Option<Arc<Pdf>> {
        self.pdf.clone()
    }

    /// Pages being drawn now (page, zoom key).
    pub fn pending(&self) -> Vec<(usize, u32)> {
        self.pending.clone()
    }

    /// Forgets every drawn page (the zoom changed, so they are the wrong size).
    pub fn forget(&mut self) {
        self.drawn.clear();
        self.order.clear();
        self.pending.clear();
        self.generation = self.generation.wrapping_add(1);
    }

    /// The pages in `visible` that are not drawn and not being drawn, at `zoom`: the ones to ask for. The
    /// drawn ones among them count as just read, and the pages furthest from these are let go so no more
    /// than [`KEEP_PAGES`] stay.
    pub fn want(&mut self, visible: std::ops::Range<usize>, zoom: f32) -> Vec<usize> {
        let zk = zoom_key(zoom);
        let visible: Vec<usize> = visible.filter(|p| *p < self.pages()).collect();
        let mut ask = Vec::new();
        for &page in &visible {
            if let Some(at) = self.order.iter().position(|p| *p == page) {
                self.order.remove(at);
                self.order.push(page);
            } else if !self.pending.contains(&(page, zk)) {
                self.pending.push((page, zk));
                ask.push(page);
            }
        }
        (self.zoom, self.middle, self.visible_len) = (zk, visible.first().copied().unwrap_or(0), visible.len());
        self.trim();
        ask
    }

    fn trim(&mut self) {
        while self.drawn.len() > KEEP_PAGES.max(self.visible_len) {
            // The drawn page furthest from the reader goes first.
            let Some(far) = self.order.iter().copied().max_by_key(|p| p.abs_diff(self.middle)) else { break };
            self.order.retain(|p| *p != far);
            self.drawn.remove(&far);
        }
    }

    /// A page was drawn off the UI thread. Kept when it is still at the zoom on screen; a page drawn for a
    /// zoom the reader has left is dropped. True when it was kept.
    pub fn accept(&mut self, done: Option<Rendered>, page: usize, zoom_key: u32) -> bool {
        self.pending.retain(|p| *p != (page, zoom_key));
        let Some(r) = done else { return false };
        if r.zoom_key != self.zoom {
            return false;
        }
        let px = Pixels::new();
        px.set(r.size.0, r.size.1, r.rgba);
        self.drawn.insert(r.page, px);
        self.order.retain(|p| *p != r.page);
        self.order.push(r.page);
        self.trim();
        self.generation = self.generation.wrapping_add(1);
        true
    }

    /// Draws the pages in `visible` here and now (a test; the viewer draws them off the UI thread). True
    /// when a page was drawn.
    pub fn draw(&mut self, visible: std::ops::Range<usize>, zoom: f32) -> bool {
        let mut drew = false;
        let pdf = self.pdf.clone();
        for page in self.want(visible, zoom) {
            let done = pdf.as_ref().and_then(|p| render_page(p, page, zoom));
            drew |= self.accept(done, page, zoom_key(zoom));
        }
        drew
    }

    /// How tall page `page` shows at `zoom`, with its gap.
    pub fn row_height(&self, page: usize, zoom: f32) -> f32 {
        let (w, h) = self.sizes.get(page).copied().unwrap_or((612.0, 792.0));
        PAGE_WIDTH * zoom * h / w.max(1.0) + GAP
    }
}

pub type SharedDoc = Rc<RefCell<PdfDoc>>;

/// The next zoom step up (`+1`) or down (`-1`) from `zoom`, or the default (`0`).
pub fn zoom_step(zoom: f32, dir: i32) -> f32 {
    if dir == 0 {
        return 1.0;
    }
    let now = ZOOM_STEPS.iter().position(|z| (z - zoom).abs() < 0.01).unwrap_or_else(|| ZOOM_STEPS.iter().rposition(|z| *z <= zoom).unwrap_or(0));
    ZOOM_STEPS[(now as i32 + dir).clamp(0, ZOOM_STEPS.len() as i32 - 1) as usize]
}

/// "3 / 16 · 100%".
pub fn counter(page: usize, pages: usize, zoom: f32) -> String {
    format!("{} / {} · {}%", (page + 1).min(pages.max(1)), pages, (zoom * 100.0).round() as u32)
}

/// A viewer pressed ⌘+, ⌘− or ⌘0: sets the zoom and has the pages drawn again. True when it changed.
pub fn zoom(state: &mut ViewerState, dir: i32) -> bool {
    let next = zoom_step(state.zoom(), dir);
    if (next - state.zoom()).abs() < f32::EPSILON {
        return false;
    }
    state.zoom = next;
    if let Some(doc) = state.peek::<SharedDoc>() {
        doc.borrow_mut().forget();
    }
    state.drop_jobs("page:");
    true
}

fn page_tag(page: usize, zoom_key: u32) -> String {
    format!("page:{page}:{zoom_key}")
}

/// Takes the pages drawn off the UI thread since the last look into the document. True when one landed.
pub fn collect(state: &mut ViewerState, doc: &SharedDoc) -> bool {
    let pending = doc.borrow().pending();
    let mut landed = false;
    for (page, zk) in pending {
        if let Some(done) = state.ready::<Option<Rendered>>(&page_tag(page, zk)) {
            landed |= doc.borrow_mut().accept(done, page, zk);
        }
    }
    landed
}

/// The pages `range` came on screen: draws them and says which page the reader is on. True when the
/// view must run again (a page was drawn, or the counter moved).
pub fn visible(state: &mut ViewerState, range: std::ops::Range<usize>) -> bool {
    let zoom = state.zoom();
    let Some(doc) = state.peek::<SharedDoc>().cloned() else { return false };
    let zk = zoom_key(zoom);
    let wanted = doc.borrow_mut().want(range.clone(), zoom);
    let pdf = doc.borrow().document();
    if let Some(pdf) = pdf {
        for page in wanted {
            let pdf = pdf.clone();
            state.spawn(&page_tag(page, zk), move || render_page(&pdf, page, zoom));
        }
    }
    let drew = collect(state, &doc);
    // The page the reader is on: the first at the top of the document, else the middle of the pages showing.
    let page = if range.start == 0 { 0 } else { ((range.start + range.end) / 2).min(range.end.saturating_sub(1)).max(range.start) };
    let moved = state.page != page;
    state.page = page;
    drew || moved
}

pub fn build(input: &ViewerInput, state: &mut ViewerState) -> El {
    let t = theme(input.theme_dark);
    let bytes = input.shared();
    let Some(doc) = state.load("parse", move || parse(bytes), |p| Rc::new(RefCell::new(PdfDoc::from_parsed(p)))).cloned() else {
        return super::image_view::skeleton(input, "viewer-pdf", None);
    };
    collect(state, &doc);
    if let Some(error) = doc.borrow().error.clone() {
        return col().key("viewer-pdf").w_full().grow(1.0).min_h(0.0).center().px(24.0).child(text(error).font(t.ty.meta).color(t.text.muted).wrap());
    }
    let (pages, zoom, key, generation): (usize, f32, ViewerKey, u32) = (doc.borrow().pages(), state.zoom(), state.key.clone(), doc.borrow().generation);
    let page_el = {
        let doc = doc.clone();
        let paper = t.surface.card;
        move |_: &App, i: usize| {
            let d = doc.borrow();
            let (w, h) = (PAGE_WIDTH * zoom, d.row_height(i, zoom) - GAP);
            let sheet = match d.pixels(i) {
                Some(px) => el().key(format!("pdf-sheet-{i}")).w(w).h(h).pixels_fit(px, SurfaceFit::Fit),
                None => el().key(format!("pdf-sheet-{i}")).w(w).h(h).fill(paper),
            };
            col().w_full().items_center().pb(GAP).child(sheet.role("img").label(format!("Page {}", i + 1)))
        }
    };
    let rows = {
        let key = key.clone();
        let estimate = doc.borrow().row_height(0, zoom);
        col()
            .key("pdf-pages")
            .w_full()
            .growth(GrowthSpec::default())
            .virtual_rows(pages, estimate, |_: &App, i| Cow::Owned(format!("p{i}")), page_el)
            .virtual_rows_version((key.clone(), pages, zoom_key(zoom), generation))
            .on_visible_rows(move |s: &mut App, range| s.panel.states.with(&key, |st| visible(st, range)))
    };
    col()
        .key("viewer-pdf")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .child(col().key("pdf-scroll").w_full().grow(1.0).min_h(0.0).clip_rounded(0.0).scroll_xy().child(col().w_full().pt(12.0).child(rows)))
        .child(text(counter(state.page, pages, zoom)).key("pdf-counter").font(t.ty.id).color(t.text.faint).px(12.0).py(6.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pdf_reports_its_pages_and_draws_only_what_it_is_asked_to_and_keeps_six() {
        let mut doc = PdfDoc::load(&crate::samples::pdf(16));
        assert_eq!((doc.pages(), doc.error.clone()), (16, None));
        assert_eq!(doc.sizes[0], (612.0, 792.0));
        assert_eq!(doc.held(), 0, "nothing is drawn until a page is on screen");
        assert!(doc.draw(0..2, 1.0));
        assert_eq!((doc.held(), doc.is_drawn(0), doc.is_drawn(1), doc.is_drawn(2)), (2, true, true, false));
        // Reading on: never more than six drawn pages, and the ones left behind go first.
        for start in 2..16 {
            doc.draw(start..start + 2, 1.0);
            assert!(doc.held() <= KEEP_PAGES, "{} pages held at {start}", doc.held());
        }
        assert!(doc.is_drawn(14) && doc.is_drawn(15), "the pages on screen are drawn");
        assert!(!doc.is_drawn(0) && !doc.is_drawn(1), "the ones far behind are let go");
        // A new zoom forgets them all so they are drawn at the new size.
        doc.forget();
        assert_eq!(doc.held(), 0);
        // Pages past the end are ignored.
        assert!(!doc.draw(40..44, 1.0));
    }

    #[test]
    fn a_page_is_drawn_at_twice_the_size_it_shows_at() {
        let doc = PdfDoc::load(&crate::samples::pdf(1));
        let mut doc = doc;
        doc.draw(0..1, 1.0);
        let (w, h) = doc.pixels(0).unwrap().size();
        assert_eq!(w, (PAGE_WIDTH * DRAW_SCALE) as u32, "388 points shown at 776 pixels");
        assert!((h as f32 / w as f32 - 792.0 / 612.0).abs() < 0.01);
        assert!((doc.row_height(0, 1.0) - (PAGE_WIDTH * 792.0 / 612.0 + GAP)).abs() < 0.01);
    }

    #[test]
    fn zoom_steps_and_the_counter_read_as_the_spec_says() {
        assert_eq!(zoom_step(1.0, 1), 1.25);
        assert_eq!(zoom_step(1.0, -1), 0.8);
        assert_eq!(zoom_step(2.0, 0), 1.0, "⌘0 is the default");
        assert_eq!(zoom_step(3.0, 1), 3.0, "the most");
        assert_eq!(zoom_step(0.5, -1), 0.5, "the least");
        assert_eq!(counter(2, 16, 1.0), "3 / 16 · 100%");
        assert_eq!(counter(0, 0, 1.25), "1 / 0 · 125%");
    }

    #[test]
    fn a_file_that_is_not_a_pdf_says_so() {
        let doc = PdfDoc::load(b"not a pdf");
        assert!(doc.error.as_deref().is_some_and(|e| e.contains("couldn’t be read")), "{:?}", doc.error);
        assert_eq!(doc.pages(), 0);
    }

    #[test]
    fn leaving_the_screen_releases_every_drawn_page() {
        let mut state = ViewerState::new(("pdf1".into(), 1));
        let doc: SharedDoc = state.load("parse", || parse(Arc::from(crate::samples::pdf(4))), |p| Rc::new(RefCell::new(PdfDoc::from_parsed(p)))).cloned().unwrap();
        assert!(visible(&mut state, 0..2));
        assert_eq!(doc.borrow().held(), 2);
        let states = crate::viewers::ViewerStates::default();
        states.with(&("pdf1".into(), 1), |s| *s = state);
        assert_eq!(states.release_except(None), 1);
        assert_eq!(Rc::strong_count(&doc), 1, "the viewer let go of the document; only this test still holds it");
    }
}
