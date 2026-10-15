//! Images, shadows, and backdrop blur for the Cairo painter.

use super::add_path;
use crate::ffi::cairo::{Context, Filter, Matrix, Pattern, Surface};
use choscorui_core::{Image, ImageSampling, Path, Rect, Shadow, blur_pixels};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

const IMAGE_CACHE_LIMIT: usize = 256;
const SHADOW_CACHE_LIMIT: usize = 128;

thread_local! {
    /// Premultiplied surfaces keyed by core image identity. Bounded so a
    /// stream of distinct images cannot grow memory without limit.
    static IMAGES: RefCell<HashMap<u64, Rc<Surface<'static>>>> = RefCell::new(HashMap::new());
    /// Shadow masks by everything but position: a mask is translation
    /// invariant.
    static SHADOW_MASKS: RefCell<HashMap<[u64; 13], Image>> = RefCell::new(HashMap::new());
}

/// Straight-alpha RGBA8 pixels as premultiplied native-endian ARGB32.
fn premultiply(image: &Image) -> Vec<u8> {
    image
        .pixels()
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|pixel| {
            let [r, g, b, a] = pixel.map(u32::from);
            let mul = |c: u32| (c * a + 127) / 255;
            ((a << 24) | (mul(r) << 16) | (mul(g) << 8) | mul(b)).to_ne_bytes()
        })
        .collect()
}

/// The cached surface of `image`.
fn image_surface(image: &Image) -> Option<Rc<Surface<'static>>> {
    IMAGES.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some(surface) = cache.get(&image.id()) {
            return Some(Rc::clone(surface));
        }
        if cache.len() >= IMAGE_CACHE_LIMIT {
            cache.clear();
        }
        let pixels = premultiply(image);
        let surface = Rc::new(Surface::from_pixels(&pixels, image.width(), image.height()).ok()?);
        cache.insert(image.id(), Rc::clone(&surface));
        Some(surface)
    })
}

/// Draws `image` scaled to fill `rect`. Samples near the edges repeat
/// the edge pixels, as the reference renderer clamps them.
pub(super) fn draw_image(cx: &Context<'_>, image: &Image, rect: Rect, sampling: ImageSampling) {
    if rect.size.width <= 0.0 || rect.size.height <= 0.0 {
        return;
    }
    let Some(surface) = image_surface(image) else {
        return;
    };
    let filter = match sampling {
        ImageSampling::Nearest => Filter::Nearest,
        ImageSampling::Linear => Filter::Bilinear,
    };
    let Some(pattern) = Pattern::image(&surface, filter, true) else {
        return;
    };
    let (width, height) = (f64::from(image.width()), f64::from(image.height()));
    cx.save();
    cx.translate(rect.min_x(), rect.min_y());
    cx.scale(rect.size.width / width, rect.size.height / height);
    cx.set_source(&pattern);
    cx.new_path();
    cx.rectangle(0.0, 0.0, width, height);
    cx.fill();
    cx.restore();
}

/// The scale from user space to device pixels.
fn device_scale(matrix: Matrix) -> f64 {
    let [xx, yx, xy, yy, _, _] = matrix;
    (xx * yy - yx * xy).abs().sqrt()
}

/// Draws the core shadow mask of `rect` like an image. The mask has the
/// device resolution, so it stays sharp at every scale.
pub(super) fn draw_shadow(cx: &Context<'_>, rect: Rect, radii: [f64; 4], shadow: Shadow) {
    let scale = device_scale(cx.matrix()).max(1.0);
    let color = shadow.color;
    let key = [
        rect.size.width.to_bits(),
        rect.size.height.to_bits(),
        radii[0].to_bits(),
        radii[1].to_bits(),
        radii[2].to_bits(),
        radii[3].to_bits(),
        shadow.offset.x.to_bits(),
        shadow.offset.y.to_bits(),
        shadow.blur.to_bits(),
        shadow.spread.to_bits(),
        u64::from(shadow.inset),
        scale.to_bits(),
        u64::from_le_bytes([color.red, color.green, color.blue, color.alpha, 0, 0, 0, 0]),
    ];
    let mask = SHADOW_MASKS.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some(mask) = cache.get(&key) {
            return Some(mask.clone());
        }
        let mask = shadow.mask_corners(rect, radii, scale)?;
        if cache.len() >= SHADOW_CACHE_LIMIT {
            cache.clear();
        }
        cache.insert(key, mask.clone());
        Some(mask)
    });
    if let Some(mask) = mask {
        draw_image(cx, &mask, shadow.bounds(rect), ImageSampling::Linear);
    }
}

/// The device pixels under `rect` and `margin` pixels around it, as
/// `(x0, y0, x1, y1)` clamped to the surface.
fn region(
    matrix: Matrix,
    rect: Rect,
    margin: f64,
    width: usize,
    height: usize,
) -> (usize, usize, usize, usize) {
    let [xx, yx, xy, yy, x0, y0] = matrix;
    let corners = [
        (rect.min_x(), rect.min_y()),
        (rect.max_x(), rect.min_y()),
        (rect.min_x(), rect.max_y()),
        (rect.max_x(), rect.max_y()),
    ]
    .map(|(x, y)| (xx * x + xy * y + x0, yx * x + yy * y + y0));
    let fold = |pick: fn(&(f64, f64)) -> f64, start: f64, combine: fn(f64, f64) -> f64| {
        corners.iter().map(pick).fold(start, combine)
    };
    let clamp = |value: f64, limit: usize| value.clamp(0.0, limit as f64) as usize;
    (
        clamp(
            fold(|c| c.0, f64::INFINITY, f64::min).floor() - margin,
            width,
        ),
        clamp(
            fold(|c| c.1, f64::INFINITY, f64::min).floor() - margin,
            height,
        ),
        clamp(
            fold(|c| c.0, f64::NEG_INFINITY, f64::max).ceil() + margin,
            width,
        ),
        clamp(
            fold(|c| c.1, f64::NEG_INFINITY, f64::max).ceil() + margin,
            height,
        ),
    )
}

/// Blurs the surface under `rect`, rounded with `radii` clockwise from
/// the top left, with a Gaussian of standard deviation `blur` points. It
/// reads the base surface, beneath any open opacity group, and paints
/// into the current group.
pub(super) fn backdrop_blur(
    cx: &Context<'_>,
    surface: &Surface<'_>,
    rect: Rect,
    radii: [f64; 4],
    blur: f64,
) {
    let matrix = cx.matrix();
    let sigma = blur * device_scale(matrix);
    let (x0, y0, x1, y1) = region(
        matrix,
        rect,
        (sigma * 3.0).ceil(),
        surface.width(),
        surface.height(),
    );
    if x1 <= x0 || y1 <= y0 {
        return;
    }
    let (columns, rows) = (x1 - x0, y1 - y0);
    let mut pixels = Vec::with_capacity(columns * rows * 4);
    surface.copy_rows(y0..y1, x0 * 4..x1 * 4, &mut pixels);
    if pixels.len() != columns * rows * 4 {
        return;
    }
    blur_pixels(&mut pixels, columns, rows, columns * 4, sigma);
    let Ok(blurred) = Surface::from_pixels(&pixels, columns as u32, rows as u32) else {
        return;
    };
    cx.save();
    add_path(cx, &Path::rounded_rect_corners(rect, radii));
    cx.set_fill_rule(choscorui_core::FillRule::NonZero);
    cx.clip();
    // Draw in device space, so each blurred pixel lands where it was read.
    cx.identity_matrix();
    cx.set_source_surface(&blurred, x0 as f64, y0 as f64);
    cx.paint();
    cx.restore();
}
