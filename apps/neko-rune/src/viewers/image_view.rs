//! The image and SVG viewers. Pictures decode with the `image` crate (PNG, JPEG, WebP, GIF's first
//! frame), animated GIFs with the `gif` crate whose frames advance on the app's frame clock
//! ([`tick`]), and SVG with `resvg` (the version Rune pins). Pixels go to the screen through
//! `El::pixels_fit`; they are dropped when the viewer leaves the screen ([`ViewerState::release`]).

use std::io::Cursor;

use rune::{El, Pixels, SurfaceFit, Tx, col, el, text};

use super::{ViewerInput, ViewerState};
use crate::readers::size_words;
use crate::theme::theme;

/// No picture is held larger than this on its longer side: a bigger one is scaled down once.
pub const MAX_SIDE: u32 = 4096;
/// An animation keeps at most this many bytes of frames; a longer one shows its first frames.
pub const MAX_FRAME_BYTES: usize = 96 * 1024 * 1024;

/// One animated frame: the whole canvas as it looks, and how long it stays.
pub struct GifFrame {
    pub rgba: Vec<u8>,
    pub ms: u32,
}

/// The frames of an animated GIF and where the clock has got to.
pub struct Gif {
    pub frames: Vec<GifFrame>,
    pub total_ms: u32,
    pub shown: usize,
    /// The clock's seconds when frame 0 was shown.
    pub started: Option<f64>,
}

/// A decoded picture: the image the screen shows, and its animation when it has one.
pub struct ImageData {
    pub px: Pixels,
    pub size: (u32, u32),
    pub gif: Option<Gif>,
    pub error: Option<String>,
}

impl ImageData {
    fn failed(message: impl Into<String>) -> ImageData {
        ImageData { px: Pixels::new(), size: (0, 0), gif: None, error: Some(message.into()) }
    }

    fn still(rgba: Vec<u8>, size: (u32, u32)) -> ImageData {
        let px = Pixels::new();
        px.set(size.0, size.1, rgba);
        ImageData { px, size, gif: None, error: None }
    }
}

/// The side lengths that bring `(w, h)` under [`MAX_SIDE`], keeping its shape.
pub fn bounded(w: u32, h: u32) -> (u32, u32) {
    let longest = w.max(h);
    if longest <= MAX_SIDE {
        return (w, h);
    }
    let k = f64::from(MAX_SIDE) / f64::from(longest);
    (((f64::from(w) * k).round() as u32).max(1), ((f64::from(h) * k).round() as u32).max(1))
}

/// Decodes a PNG, JPEG, WebP or GIF (the first frame of one; [`decode_gif`] has the rest).
pub fn decode_still(bytes: &[u8]) -> Result<(Vec<u8>, (u32, u32)), String> {
    let img = image::load_from_memory(bytes).map_err(|e| format!("This picture couldn’t be read ({e})."))?;
    let (w, h) = bounded(img.width(), img.height());
    let img = if (w, h) == (img.width(), img.height()) { img } else { img.resize_exact(w, h, image::imageops::FilterType::Triangle) };
    Ok((img.to_rgba8().into_raw(), (w, h)))
}

/// Decodes every frame of a GIF onto a canvas, honouring each frame's rectangle, transparency and
/// disposal, up to [`MAX_FRAME_BYTES`] of frames.
pub fn decode_gif(bytes: &[u8]) -> Result<(Vec<GifFrame>, (u32, u32)), String> {
    let mut options = gif::DecodeOptions::new();
    options.set_color_output(gif::ColorOutput::RGBA);
    let mut decoder = options.read_info(Cursor::new(bytes)).map_err(|e| format!("This GIF couldn’t be read ({e})."))?;
    let (w, h) = (u32::from(decoder.width()), u32::from(decoder.height()));
    if w == 0 || h == 0 || w > MAX_SIDE || h > MAX_SIDE {
        return Err("This GIF is too large to play.".to_owned());
    }
    let stride = w as usize * 4;
    let mut canvas = vec![0u8; stride * h as usize];
    let mut frames: Vec<GifFrame> = Vec::new();
    let mut held = 0usize;
    while let Some(frame) = decoder.read_next_frame().map_err(|e| format!("This GIF couldn’t be read ({e})."))? {
        let before = (frame.dispose == gif::DisposalMethod::Previous).then(|| canvas.clone());
        let (left, top, fw, fh) = (usize::from(frame.left), usize::from(frame.top), usize::from(frame.width), usize::from(frame.height));
        for y in 0..fh.min(h as usize - top.min(h as usize)) {
            for x in 0..fw.min(w as usize - left.min(w as usize)) {
                let src = (y * fw + x) * 4;
                let dst = (top + y) * stride + (left + x) * 4;
                // A transparent pixel leaves what was there.
                if frame.buffer[src + 3] != 0 {
                    canvas[dst..dst + 4].copy_from_slice(&frame.buffer[src..src + 4]);
                }
            }
        }
        held += canvas.len();
        if held > MAX_FRAME_BYTES && !frames.is_empty() {
            break;
        }
        // Zero and one hundredth of a second are what browsers show as a tenth.
        let ms = if frame.delay <= 1 { 100 } else { u32::from(frame.delay) * 10 };
        frames.push(GifFrame { rgba: canvas.clone(), ms });
        match frame.dispose {
            gif::DisposalMethod::Background => {
                for y in top..(top + fh).min(h as usize) {
                    let a = y * stride + left * 4;
                    let b = y * stride + (left + fw).min(w as usize) * 4;
                    canvas[a..b].fill(0);
                }
            }
            gif::DisposalMethod::Previous => {
                if let Some(before) = before {
                    canvas = before;
                }
            }
            _ => {}
        }
    }
    if frames.is_empty() {
        return Err("This GIF has no pictures.".to_owned());
    }
    Ok((frames, (w, h)))
}

fn decode(kind: &str, bytes: &[u8]) -> ImageData {
    if kind.eq_ignore_ascii_case("gif") {
        match decode_gif(bytes) {
            Ok((frames, size)) if frames.len() > 1 => {
                let px = Pixels::new();
                px.set(size.0, size.1, frames[0].rgba.clone());
                let total_ms = frames.iter().map(|f| f.ms).sum();
                return ImageData { px, size, gif: Some(Gif { frames, total_ms, shown: 0, started: None }), error: None };
            }
            Ok((mut frames, size)) => return ImageData::still(frames.remove(0).rgba, size),
            // A GIF the animation decoder dislikes may still have a first frame the picture decoder reads.
            Err(_) => {}
        }
    }
    match decode_still(bytes) {
        Ok((rgba, size)) => ImageData::still(rgba, size),
        Err(e) => ImageData::failed(e),
    }
}

/// Rasterises an SVG with `resvg` at a size that is sharp in the panel (about 1,200 pixels wide).
pub fn rasterize_svg(bytes: &[u8]) -> Result<(Vec<u8>, (u32, u32)), String> {
    use resvg::{tiny_skia, usvg};
    // An SVG is data from outside: it may name another file (`<image href="/path">`), and resvg would read it.
    // Only pictures embedded in the SVG itself (`data:` links) are drawn.
    let mut options = usvg::Options::default();
    options.image_href_resolver.resolve_string = Box::new(|_, _| None);
    let tree = usvg::Tree::from_data(bytes, &options).map_err(|e| format!("This SVG couldn’t be read ({e})."))?;
    let size = tree.size();
    let (w, h) = (size.width().max(1.0), size.height().max(1.0));
    let k = (1200.0 / w).clamp(0.05, 8.0);
    let (pw, ph) = bounded(((w * k).ceil() as u32).max(1), ((h * k).ceil() as u32).max(1));
    let mut pixmap = tiny_skia::Pixmap::new(pw, ph).ok_or_else(|| "This SVG is empty.".to_owned())?;
    resvg::render(&tree, tiny_skia::Transform::from_scale(pw as f32 / w, ph as f32 / h), &mut pixmap.as_mut());
    // The pixmap is premultiplied; pixels want straight alpha.
    let mut rgba = pixmap.take();
    for p in rgba.chunks_exact_mut(4) {
        let a = u32::from(p[3]);
        if a != 0 && a != 255 {
            for c in &mut p[..3] {
                *c = ((u32::from(*c) * 255 + a / 2) / a).min(255) as u8;
            }
        }
    }
    Ok((rgba, (pw, ph)))
}

fn show(input: &ViewerInput, key: &'static str, data: &ImageData) -> El {
    let t = theme(input.theme_dark);
    if let Some(error) = &data.error {
        return col().key(key).w_full().grow(1.0).min_h(0.0).center().px(24.0).child(text(error.clone()).font(t.ty.meta).color(t.text.muted).wrap());
    }
    let caption = format!("{} × {}{}", data.size.0, data.size.1, input.bytes.map(|b| format!(" · {}", size_words(b.len() as u64))).unwrap_or_default());
    col()
        .key(key)
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .padding(12.0)
        .gap(8.0)
        .child(el().key("image-pixels").w_full().grow(1.0).min_h(0.0).pixels_fit(&data.px, SurfaceFit::Fit).role("img").label(input.title.to_owned()))
        .child(text(caption).key("image-caption").font(t.ty.id).color(t.text.faint).self_center())
}

pub fn build_image(input: &ViewerInput, state: &mut ViewerState) -> El {
    let (kind, bytes) = (input.kind.to_owned(), input.bytes.unwrap_or(&[]));
    let data = state.data(|| decode(&kind, bytes));
    show(input, "viewer-image", data)
}

pub fn build_svg(input: &ViewerInput, state: &mut ViewerState) -> El {
    let bytes = input.bytes.unwrap_or(&[]);
    let data = state.data(|| match rasterize_svg(bytes) {
        Ok((rgba, size)) => ImageData::still(rgba, size),
        Err(e) => ImageData::failed(e),
    });
    show(input, "viewer-svg", data)
}

/// Advances an animated picture on the app's frame clock `t` (seconds): sets the picture's pixels
/// to the frame due, which wakes only the window showing them, and asks to be woken when the next
/// frame is due. Nothing else runs while it is still. True never (no view needs to run).
pub fn tick(state: &mut ViewerState, t: f64, tx: &mut Tx) -> bool {
    let Some(data) = state.peek::<ImageData>() else { return false };
    let Some(gif) = data.gif.as_mut() else { return false };
    let start = *gif.started.get_or_insert(t);
    let into = (((t - start) * 1000.0) as u64 % u64::from(gif.total_ms.max(1))) as u32;
    let (mut index, mut at) = (0usize, 0u32);
    for (i, f) in gif.frames.iter().enumerate() {
        if into < at + f.ms {
            index = i;
            break;
        }
        at += f.ms;
        index = i;
    }
    if index != gif.shown {
        gif.shown = index;
        data.px.set(data.size.0, data.size.1, gif.frames[index].rgba.clone());
    }
    let left = (at + gif.frames[index].ms).saturating_sub(into).max(10);
    tx.wake_at(t + f64::from(left) / 1000.0);
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(w: u32, h: u32) -> Vec<u8> {
        let mut out = Vec::new();
        let img = image::RgbaImage::from_fn(w, h, |x, y| image::Rgba([x as u8, y as u8, 128, 255]));
        img.write_to(&mut Cursor::new(&mut out), image::ImageFormat::Png).unwrap();
        out
    }

    #[test]
    fn a_png_decodes_to_its_size_and_a_huge_one_is_scaled_to_the_cap_once() {
        let (rgba, size) = decode_still(&png(40, 30)).unwrap();
        assert_eq!((size, rgba.len()), ((40, 30), 40 * 30 * 4));
        assert_eq!(bounded(8192, 4096), (4096, 2048));
        assert_eq!(bounded(100, 5000), (82, 4096));
        assert_eq!(bounded(4096, 4096), (4096, 4096));
        assert!(decode_still(b"not a picture").is_err());
    }

    /// An animated GIF of `n` frames, each a solid colour, 10 hundredths of a second apiece.
    pub fn gif_bytes(n: u8) -> Vec<u8> {
        let mut out = Vec::new();
        {
            let mut enc = gif::Encoder::new(&mut out, 16, 16, &[]).unwrap();
            enc.set_repeat(gif::Repeat::Infinite).unwrap();
            for i in 0..n {
                let pixels: Vec<u8> = (0..16 * 16).flat_map(|_| [i.wrapping_mul(60), 40, 200]).collect();
                let mut frame = gif::Frame::from_rgb(16, 16, &pixels);
                frame.delay = 10;
                enc.write_frame(&frame).unwrap();
            }
        }
        out
    }

    #[test]
    fn a_gif_keeps_every_frame_with_its_own_delay_and_one_frame_is_a_still() {
        let (frames, size) = decode_gif(&gif_bytes(4)).unwrap();
        assert_eq!((frames.len(), size), (4, (16, 16)));
        assert!(frames.iter().all(|f| f.ms == 100 && f.rgba.len() == 16 * 16 * 4));
        assert_ne!(frames[0].rgba[..3], frames[1].rgba[..3], "frames differ");
        let one = decode("gif", &gif_bytes(1));
        assert!(one.gif.is_none() && one.error.is_none(), "a single frame is a picture");
        let many = decode("gif", &gif_bytes(3));
        assert_eq!(many.gif.as_ref().map(|g| g.total_ms), Some(300));
    }

    #[test]
    fn an_svg_never_reads_a_file_it_names() {
        let dir = std::env::temp_dir().join(format!("nrs-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let red = dir.join("red.png");
        let img = image::RgbaImage::from_pixel(8, 8, image::Rgba([255, 0, 0, 255]));
        img.save(&red).unwrap();
        let svg = format!(r#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100"><image href="{}" width="100" height="100"/></svg>"#, red.display());
        let (rgba, (w, h)) = rasterize_svg(svg.as_bytes()).unwrap();
        let mid = (((h / 2) * w + w / 2) * 4) as usize;
        assert_eq!(rgba[mid + 3], 0, "the picture at that path was not drawn: {:?}", &rgba[mid..mid + 4]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_svg_rasterises_to_pixels_at_its_shape() {
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="100"><rect width="200" height="100" fill="#ff0000"/></svg>"##;
        let (rgba, (w, h)) = rasterize_svg(svg).unwrap();
        assert_eq!((w, h), (1200, 600));
        let mid = ((300 * w + 600) * 4) as usize;
        assert_eq!(&rgba[mid..mid + 4], &[255, 0, 0, 255]);
        assert!(rasterize_svg(b"<svg").is_err());
    }
}
