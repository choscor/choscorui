use super::*;
use crate::ffi::cf::CfRetained;
use crate::ffi::graphics::{
    CGContext, CGContextAddCurveToPoint, CGContextAddLineToPoint, CGContextAddQuadCurveToPoint,
    CGContextBeginPath, CGContextBeginTransparencyLayer, CGContextClip, CGContextClipToRect,
    CGContextClosePath, CGContextConcatCTM, CGContextDrawImage, CGContextDrawLinearGradient,
    CGContextDrawRadialGradient, CGContextEOClip, CGContextEOFillPath,
    CGContextEndTransparencyLayer, CGContextFillPath, CGContextFillRect,
    CGContextGetUserSpaceToDeviceSpaceTransform, CGContextMoveToPoint,
    CGContextReplacePathWithStrokedPath, CGContextRestoreGState, CGContextSaveGState,
    CGContextScaleCTM, CGContextSetAlpha, CGContextSetFillColorWithColor,
    CGContextSetInterpolationQuality, CGContextSetLineCap, CGContextSetLineJoin,
    CGContextSetLineWidth, CGContextSetMiterLimit, CGContextSetStrokeColorWithColor,
    CGContextStrokePath, CGContextStrokeRectWithWidth, CGContextTranslateCTM, CGImage,
    GRADIENT_EXTEND, Interpolation, LineCap as CgLineCap, LineJoin as CgLineJoin, PixelLayout,
    STRAIGHT_RGBA, image_from_pixels, srgb, srgb_color, srgb_gradient,
};
use choscorui_core::{
    FillRule, Image, ImageSampling, LineCap, LineJoin, Paint, Path, PathElement, Shadow,
    StrokeStyle, Transform,
};

thread_local! {
    /// Decoded `CGImage` objects keyed by core image identity. Bounded so
    /// a stream of distinct images cannot grow memory without limit.
    static IMAGES: RefCell<HashMap<u64, CfRetained<CGImage>>> = RefCell::new(HashMap::new());
}
const IMAGE_CACHE_LIMIT: usize = 256;

/// Creates a `CGImage` for straight-alpha sRGB RGBA8 pixels.
pub(crate) fn cg_image(image: &Image) -> Option<CfRetained<CGImage>> {
    let width = image.width() as usize;
    image_from_pixels(
        image.pixels(),
        PixelLayout {
            width,
            height: image.height() as usize,
            bytes_per_row: width * 4,
            bitmap_info: STRAIGHT_RGBA,
            interpolate: true,
        },
        srgb()?,
    )
}

fn with_cached_image<R>(image: &Image, draw: impl FnOnce(&CGImage) -> R) -> Option<R> {
    IMAGES.with(|cache| {
        let mut cache = cache.borrow_mut();
        if !cache.contains_key(&image.id()) {
            if cache.len() >= IMAGE_CACHE_LIMIT {
                cache.clear();
            }
            let created = cg_image(image)?;
            cache.insert(image.id(), created);
        }
        cache.get(&image.id()).map(|owned| draw(owned))
    })
}

/// Drops cached native images; called when a window tears down.
pub(crate) fn clear_image_cache() {
    IMAGES.with(|cache| cache.borrow_mut().clear());
    SHADOW_MASKS.with(|cache| cache.borrow_mut().clear());
}

/// Paints a logical top-left display list into a context already configured
/// with top-left, y-down coordinates. Every push maps to exactly one
/// graphics-state save, and all saves are balanced before returning.
pub(super) fn paint_commands(context: &CGContext, dirty: NSRect, list: &DisplayList) -> bool {
    CGContextSaveGState(context);
    CGContextClipToRect(context, dirty);
    let mut stack: Vec<bool> = Vec::new(); // true = transparency layer
    let mut success = true;
    for command in list.commands() {
        match command {
            DisplayCommand::FillRect { rect, color } => {
                set_fill_color(context, *color);
                CGContextFillRect(context, native_rect(*rect));
            }
            DisplayCommand::StrokeRect { rect, color, width } => {
                set_stroke_color(context, *color);
                CGContextSetLineJoin(context, CgLineJoin::Miter);
                CGContextStrokeRectWithWidth(context, native_rect(*rect), *width);
            }
            DisplayCommand::FillRoundedRect {
                rect,
                radius,
                color,
            } => {
                set_fill_color(context, *color);
                add_path(context, &Path::rounded_rect(*rect, *radius));
                CGContextFillPath(context);
            }
            DisplayCommand::StrokeRoundedRect {
                rect,
                radius,
                color,
                width,
            } => {
                set_stroke_color(context, *color);
                CGContextSetLineWidth(context, *width);
                add_path(context, &Path::rounded_rect(*rect, *radius));
                CGContextStrokePath(context);
            }
            DisplayCommand::FillPath { path, paint, rule } => {
                fill_path(context, path, paint, *rule)
            }
            DisplayCommand::StrokePath { path, paint, style } => {
                stroke_path(context, path, paint, style)
            }
            DisplayCommand::DrawImage {
                image,
                rect,
                sampling,
            } => draw_image(context, image, *rect, *sampling),
            DisplayCommand::DrawShadow {
                rect,
                radius,
                shadow,
            } => draw_shadow(context, *rect, [*radius; 4], *shadow),
            DisplayCommand::DrawShadowCorners {
                rect,
                radii,
                shadow,
            } => draw_shadow(context, *rect, *radii, *shadow),
            DisplayCommand::BackdropBlur { rect, radius, blur } => {
                backdrop::backdrop_blur(context, *rect, [*radius; 4], *blur);
            }
            DisplayCommand::BackdropBlurCorners { rect, radii, blur } => {
                backdrop::backdrop_blur(context, *rect, *radii, *blur);
            }
            DisplayCommand::DrawText {
                baseline,
                text,
                font_family,
                font_size,
                font_weight,
                font_slant,
                tracking,
                color,
            } => {
                let run = choscorui_core::TextRun {
                    family: font_family,
                    size: *font_size,
                    weight: *font_weight,
                    slant: *font_slant,
                    tracking: *tracking,
                };
                if !draw_text_run(context, *baseline, text, &run, *color) {
                    success = false;
                    break;
                }
            }
            DisplayCommand::DrawGlyphs { origin, run, color } => {
                if !text_layout::draw_glyph_run(context, *origin, run, *color) {
                    success = false;
                    break;
                }
            }
            DisplayCommand::PushClip(rect) => {
                CGContextSaveGState(context);
                CGContextClipToRect(context, native_rect(*rect));
                stack.push(false);
            }
            DisplayCommand::PushClipPath(path) => {
                CGContextSaveGState(context);
                add_path(context, path);
                CGContextClip(context);
                stack.push(false);
            }
            DisplayCommand::PushOpacity(opacity) => {
                CGContextSaveGState(context);
                CGContextSetAlpha(context, *opacity);
                CGContextBeginTransparencyLayer(context, None);
                stack.push(true);
            }
            DisplayCommand::PushTransform(transform) => {
                CGContextSaveGState(context);
                CGContextConcatCTM(context, cg_transform(*transform));
                stack.push(false);
            }
            DisplayCommand::PopClip | DisplayCommand::PopOpacity | DisplayCommand::PopTransform => {
                if let Some(layer) = stack.pop() {
                    if layer {
                        CGContextEndTransparencyLayer(context);
                    }
                    CGContextRestoreGState(context);
                }
            }
        }
    }
    while let Some(layer) = stack.pop() {
        if layer {
            CGContextEndTransparencyLayer(context);
        }
        CGContextRestoreGState(context);
    }
    CGContextRestoreGState(context);
    success
}

fn cg_transform(t: Transform) -> CGAffineTransform {
    CGAffineTransform {
        a: t.a,
        b: t.b,
        c: t.c,
        d: t.d,
        tx: t.tx,
        ty: t.ty,
    }
}

fn with_color(color: Color, apply: impl FnOnce(&ffi::graphics::CGColor)) {
    let components = [
        channel(color.red),
        channel(color.green),
        channel(color.blue),
        channel(color.alpha),
    ];
    // The context retains its own reference to the color.
    if let Some(cg) = srgb_color(components) {
        apply(&cg);
    }
}

pub(crate) fn set_fill_color(context: &CGContext, color: Color) {
    with_color(color, |cg| CGContextSetFillColorWithColor(context, cg))
}

fn set_stroke_color(context: &CGContext, color: Color) {
    with_color(color, |cg| CGContextSetStrokeColorWithColor(context, cg))
}

fn channel(value: u8) -> f64 {
    f64::from(value) / 255.0
}

pub(crate) fn add_path(context: &CGContext, path: &Path) {
    CGContextBeginPath(context);
    for element in path.elements() {
        match *element {
            PathElement::MoveTo(p) => CGContextMoveToPoint(context, p.x, p.y),
            PathElement::LineTo(p) => CGContextAddLineToPoint(context, p.x, p.y),
            PathElement::QuadTo(c, p) => CGContextAddQuadCurveToPoint(context, c.x, c.y, p.x, p.y),
            PathElement::CubicTo(a, b, p) => {
                CGContextAddCurveToPoint(context, a.x, a.y, b.x, b.y, p.x, p.y)
            }
            PathElement::Close => CGContextClosePath(context),
        }
    }
}

/// Fills the current clip with a gradient.
fn draw_gradient(context: &CGContext, paint: &Paint) {
    let (stops, linear) = match paint {
        Paint::Solid(_) => return,
        Paint::LinearGradient { start, end, stops } => (stops, Some((*start, *end))),
        Paint::RadialGradient { stops, .. } => (stops, None),
    };
    let mut components = Vec::with_capacity(stops.len() * 4);
    let mut locations = Vec::with_capacity(stops.len());
    for stop in stops {
        components.extend_from_slice(&[
            channel(stop.color.red),
            channel(stop.color.green),
            channel(stop.color.blue),
            channel(stop.color.alpha),
        ]);
        locations.push(stop.offset);
    }
    let Some(gradient) = srgb_gradient(&components, &locations) else {
        return;
    };
    match (linear, paint) {
        (Some((start, end)), _) => CGContextDrawLinearGradient(
            context,
            &gradient,
            NSPoint {
                x: start.x,
                y: start.y,
            },
            NSPoint { x: end.x, y: end.y },
            GRADIENT_EXTEND,
        ),
        (None, Paint::RadialGradient { center, radius, .. }) => {
            let c = NSPoint {
                x: center.x,
                y: center.y,
            };
            CGContextDrawRadialGradient(context, &gradient, c, 0.0, c, *radius, GRADIENT_EXTEND)
        }
        _ => {}
    }
}

fn fill_path(context: &CGContext, path: &Path, paint: &Paint, rule: FillRule) {
    add_path(context, path);
    match paint {
        Paint::Solid(color) => {
            set_fill_color(context, *color);
            match rule {
                FillRule::NonZero => CGContextFillPath(context),
                FillRule::EvenOdd => CGContextEOFillPath(context),
            }
        }
        gradient => {
            CGContextSaveGState(context);
            match rule {
                FillRule::NonZero => CGContextClip(context),
                FillRule::EvenOdd => CGContextEOClip(context),
            }
            draw_gradient(context, gradient);
            CGContextRestoreGState(context);
        }
    }
}

fn apply_stroke_style(context: &CGContext, style: &StrokeStyle) {
    CGContextSetLineWidth(context, style.width);
    CGContextSetLineCap(
        context,
        match style.cap {
            LineCap::Butt => CgLineCap::Butt,
            LineCap::Round => CgLineCap::Round,
            LineCap::Square => CgLineCap::Square,
        },
    );
    CGContextSetLineJoin(
        context,
        match style.join {
            LineJoin::Miter => CgLineJoin::Miter,
            LineJoin::Round => CgLineJoin::Round,
            LineJoin::Bevel => CgLineJoin::Bevel,
        },
    );
    CGContextSetMiterLimit(context, 10.0);
    context.set_line_dash(&style.dash);
}

fn stroke_path(context: &CGContext, path: &Path, paint: &Paint, style: &StrokeStyle) {
    CGContextSaveGState(context);
    apply_stroke_style(context, style);
    add_path(context, path);
    match paint {
        Paint::Solid(color) => {
            set_stroke_color(context, *color);
            CGContextStrokePath(context);
        }
        gradient => {
            CGContextReplacePathWithStrokedPath(context);
            CGContextClip(context);
            draw_gradient(context, gradient);
        }
    }
    CGContextRestoreGState(context);
}

fn draw_image(context: &CGContext, image: &Image, rect: Rect, sampling: ImageSampling) {
    if rect.size.width <= 0.0 || rect.size.height <= 0.0 {
        return;
    }
    with_cached_image(image, |cg| {
        // The context is flipped (y down); CGContextDrawImage draws y-up,
        // so flip locally around the destination rectangle.
        CGContextSaveGState(context);
        CGContextSetInterpolationQuality(
            context,
            match sampling {
                ImageSampling::Nearest => Interpolation::None,
                ImageSampling::Linear => Interpolation::High,
            },
        );
        CGContextTranslateCTM(context, rect.origin.x, rect.origin.y + rect.size.height);
        CGContextScaleCTM(context, 1.0, -1.0);
        CGContextDrawImage(
            context,
            NSRect {
                origin: NSPoint { x: 0.0, y: 0.0 },
                size: NSSize {
                    width: rect.size.width,
                    height: rect.size.height,
                },
            },
            cg,
        );
        CGContextRestoreGState(context);
    });
}

const SHADOW_CACHE_LIMIT: usize = 128;

thread_local! {
    /// Blurred masks by everything but position: a mask is translation
    /// invariant, and recomputing it per pixel each frame dominated replay
    /// of shadowed controls.
    static SHADOW_MASKS: RefCell<HashMap<[u64; 13], Image>> = RefCell::new(HashMap::new());
}

fn draw_shadow(context: &CGContext, rect: Rect, radii: [f64; 4], shadow: Shadow) {
    // The user-to-device transform includes the backing scale and every
    // pushed transform, so the mask stays sharp on 2x displays.
    let scale = device_scale(context).max(1.0);
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
        draw_image(context, &mask, shadow.bounds(rect), ImageSampling::Linear);
    }
}

fn device_scale(context: &CGContext) -> f64 {
    let t = CGContextGetUserSpaceToDeviceSpaceTransform(context);
    (t.a * t.d - t.b * t.c).abs().sqrt()
}
