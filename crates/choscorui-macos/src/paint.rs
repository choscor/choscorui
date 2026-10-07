use super::*;
use choscorui_core::{
    FillRule, Image, ImageSampling, LineCap, LineJoin, Paint, Path, PathElement, Shadow,
    StrokeStyle, Transform,
};

#[repr(C)]
#[derive(Clone, Copy)]
struct CGPoint {
    x: f64,
    y: f64,
}

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    static kCGColorSpaceSRGB: Id;
    fn CGColorSpaceCreateWithName(name: Id) -> Id;
    fn CGColorCreate(space: Id, components: *const f64) -> Id;
    fn CGContextSetFillColorWithColor(context: Id, color: Id);
    fn CGContextSetStrokeColorWithColor(context: Id, color: Id);
    fn CGContextAddLineToPoint(context: Id, x: f64, y: f64);
    fn CGContextAddQuadCurveToPoint(context: Id, cx: f64, cy: f64, x: f64, y: f64);
    fn CGContextAddCurveToPoint(
        context: Id,
        c1x: f64,
        c1y: f64,
        c2x: f64,
        c2y: f64,
        x: f64,
        y: f64,
    );
    fn CGContextEOFillPath(context: Id);
    fn CGContextClip(context: Id);
    fn CGContextEOClip(context: Id);
    fn CGContextSetLineCap(context: Id, cap: i32);
    fn CGContextSetLineJoin(context: Id, join: i32);
    fn CGContextSetMiterLimit(context: Id, limit: f64);
    fn CGContextSetLineDash(context: Id, phase: f64, lengths: *const f64, count: usize);
    fn CGContextReplacePathWithStrokedPath(context: Id);
    fn CGContextConcatCTM(context: Id, transform: CGAffineTransform);
    fn CGContextBeginTransparencyLayer(context: Id, auxiliary: Id);
    fn CGContextEndTransparencyLayer(context: Id);
    fn CGContextSetInterpolationQuality(context: Id, quality: i32);
    fn CGContextDrawImage(context: Id, rect: NSRect, image: Id);
    fn CGGradientCreateWithColorComponents(
        space: Id,
        components: *const f64,
        locations: *const f64,
        count: usize,
    ) -> Id;
    fn CGContextDrawLinearGradient(
        context: Id,
        gradient: Id,
        start: CGPoint,
        end: CGPoint,
        options: u32,
    );
    fn CGContextDrawRadialGradient(
        context: Id,
        gradient: Id,
        start_center: CGPoint,
        start_radius: f64,
        end_center: CGPoint,
        end_radius: f64,
        options: u32,
    );
    fn CGDataProviderCreateWithCFData(data: Id) -> Id;
    fn CGImageCreate(
        width: usize,
        height: usize,
        bits_per_component: usize,
        bits_per_pixel: usize,
        bytes_per_row: usize,
        space: Id,
        bitmap_info: u32,
        provider: Id,
        decode: *const f64,
        should_interpolate: bool,
        intent: i32,
    ) -> Id;
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFDataCreate(allocator: Id, bytes: *const u8, length: isize) -> Id;
}

/// `kCGImageAlphaLast`: straight (non-premultiplied) RGBA source pixels.
const IMAGE_ALPHA_LAST: u32 = 3;
const GRADIENT_EXTEND: u32 = 1 | 2;

/// The process-wide sRGB color space, created once and never released.
pub(crate) fn srgb() -> Id {
    static SPACE: OnceLock<usize> = OnceLock::new();
    // SAFETY: The SDK constant names a valid color space; the +1 result is
    // intentionally kept for the life of the process.
    *SPACE.get_or_init(|| unsafe { CGColorSpaceCreateWithName(kCGColorSpaceSRGB) as usize }) as Id
}

thread_local! {
    /// Decoded `CGImage` objects keyed by core image identity. Bounded so
    /// a stream of distinct images cannot grow memory without limit.
    static IMAGES: RefCell<HashMap<u64, CfOwned>> = RefCell::new(HashMap::new());
}
const IMAGE_CACHE_LIMIT: usize = 256;

/// Creates a +1 `CGImage` for straight-alpha sRGB RGBA8 pixels.
pub(crate) unsafe fn cg_image(image: &Image) -> Option<CfOwned> {
    unsafe {
        let data = CfOwned::new(CFDataCreate(
            std::ptr::null_mut(),
            image.pixels().as_ptr(),
            image.pixels().len() as isize,
        ))?;
        let provider = CfOwned::new(CGDataProviderCreateWithCFData(data.as_ptr()))?;
        CfOwned::new(CGImageCreate(
            image.width() as usize,
            image.height() as usize,
            8,
            32,
            image.width() as usize * 4,
            srgb(),
            IMAGE_ALPHA_LAST,
            provider.as_ptr(),
            std::ptr::null(),
            true,
            0,
        ))
    }
}

fn with_cached_image<R>(image: &Image, draw: impl FnOnce(Id) -> R) -> Option<R> {
    IMAGES.with(|cache| {
        let mut cache = cache.borrow_mut();
        if !cache.contains_key(&image.id()) {
            if cache.len() >= IMAGE_CACHE_LIMIT {
                cache.clear();
            }
            // SAFETY: cg_image copies the pixels into CFData.
            let created = unsafe { cg_image(image) }?;
            cache.insert(image.id(), created);
        }
        cache.get(&image.id()).map(|owned| draw(owned.as_ptr()))
    })
}

/// Drops cached native images; called when a window tears down.
pub(crate) fn clear_image_cache() {
    IMAGES.with(|cache| cache.borrow_mut().clear());
}

/// Paints a logical top-left display list into a context already configured
/// with top-left, y-down coordinates. Every push maps to exactly one
/// graphics-state save, and all saves are balanced before returning.
pub(super) unsafe fn paint_commands(context: Id, dirty: NSRect, list: &DisplayList) -> bool {
    unsafe {
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
                    CGContextSetLineJoin(context, 0);
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
                } => draw_shadow(context, *rect, *radius, *shadow),
                DisplayCommand::DrawText {
                    baseline,
                    text,
                    font_family,
                    font_size,
                    color,
                } => {
                    if !draw_text_run(context, *baseline, text, font_family, *font_size, *color) {
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
                    CGContextBeginTransparencyLayer(context, std::ptr::null_mut());
                    stack.push(true);
                }
                DisplayCommand::PushTransform(transform) => {
                    CGContextSaveGState(context);
                    CGContextConcatCTM(context, cg_transform(*transform));
                    stack.push(false);
                }
                DisplayCommand::PopClip
                | DisplayCommand::PopOpacity
                | DisplayCommand::PopTransform => {
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

unsafe fn with_color(color: Color, apply: impl FnOnce(Id)) {
    let components = [
        channel(color.red),
        channel(color.green),
        channel(color.blue),
        channel(color.alpha),
    ];
    // SAFETY: sRGB has three components plus alpha; the +1 CGColor is
    // released after the context retains its own reference.
    unsafe {
        if let Some(cg) = CfOwned::new(CGColorCreate(srgb(), components.as_ptr())) {
            apply(cg.as_ptr());
        }
    }
}

pub(crate) unsafe fn set_fill_color(context: Id, color: Color) {
    unsafe { with_color(color, |cg| CGContextSetFillColorWithColor(context, cg)) }
}

unsafe fn set_stroke_color(context: Id, color: Color) {
    unsafe { with_color(color, |cg| CGContextSetStrokeColorWithColor(context, cg)) }
}

fn channel(value: u8) -> f64 {
    f64::from(value) / 255.0
}

unsafe fn add_path(context: Id, path: &Path) {
    unsafe {
        CGContextBeginPath(context);
        for element in path.elements() {
            match *element {
                PathElement::MoveTo(p) => CGContextMoveToPoint(context, p.x, p.y),
                PathElement::LineTo(p) => CGContextAddLineToPoint(context, p.x, p.y),
                PathElement::QuadTo(c, p) => {
                    CGContextAddQuadCurveToPoint(context, c.x, c.y, p.x, p.y)
                }
                PathElement::CubicTo(a, b, p) => {
                    CGContextAddCurveToPoint(context, a.x, a.y, b.x, b.y, p.x, p.y)
                }
                PathElement::Close => CGContextClosePath(context),
            }
        }
    }
}

/// Fills the current clip with a gradient.
unsafe fn draw_gradient(context: Id, paint: &Paint) {
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
    // SAFETY: Component and location arrays match `count`; the +1 gradient
    // is released by CfOwned after drawing.
    unsafe {
        let Some(gradient) = CfOwned::new(CGGradientCreateWithColorComponents(
            srgb(),
            components.as_ptr(),
            locations.as_ptr(),
            stops.len(),
        )) else {
            return;
        };
        match (linear, paint) {
            (Some((start, end)), _) => CGContextDrawLinearGradient(
                context,
                gradient.as_ptr(),
                CGPoint {
                    x: start.x,
                    y: start.y,
                },
                CGPoint { x: end.x, y: end.y },
                GRADIENT_EXTEND,
            ),
            (None, Paint::RadialGradient { center, radius, .. }) => {
                let c = CGPoint {
                    x: center.x,
                    y: center.y,
                };
                CGContextDrawRadialGradient(
                    context,
                    gradient.as_ptr(),
                    c,
                    0.0,
                    c,
                    *radius,
                    GRADIENT_EXTEND,
                )
            }
            _ => {}
        }
    }
}

unsafe fn fill_path(context: Id, path: &Path, paint: &Paint, rule: FillRule) {
    unsafe {
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
}

unsafe fn apply_stroke_style(context: Id, style: &StrokeStyle) {
    unsafe {
        CGContextSetLineWidth(context, style.width);
        CGContextSetLineCap(
            context,
            match style.cap {
                LineCap::Butt => 0,
                LineCap::Round => 1,
                LineCap::Square => 2,
            },
        );
        CGContextSetLineJoin(
            context,
            match style.join {
                LineJoin::Miter => 0,
                LineJoin::Round => 1,
                LineJoin::Bevel => 2,
            },
        );
        CGContextSetMiterLimit(context, 10.0);
        CGContextSetLineDash(context, 0.0, style.dash.as_ptr(), style.dash.len());
    }
}

unsafe fn stroke_path(context: Id, path: &Path, paint: &Paint, style: &StrokeStyle) {
    unsafe {
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
}

unsafe fn draw_image(context: Id, image: &Image, rect: Rect, sampling: ImageSampling) {
    if rect.size.width <= 0.0 || rect.size.height <= 0.0 {
        return;
    }
    with_cached_image(image, |cg| {
        // SAFETY: The context is flipped (y down); CGContextDrawImage draws
        // y-up, so flip locally around the destination rectangle.
        unsafe {
            CGContextSaveGState(context);
            CGContextSetInterpolationQuality(
                context,
                match sampling {
                    ImageSampling::Nearest => 1,
                    ImageSampling::Linear => 3,
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
        }
    });
}

unsafe fn draw_shadow(context: Id, rect: Rect, radius: f64, shadow: Shadow) {
    // The user-to-device transform includes the backing scale and every
    // pushed transform, so the mask stays sharp on 2x displays.
    let scale = unsafe { device_scale(context) };
    if let Some(mask) = shadow.mask(rect, radius, scale.max(1.0)) {
        unsafe { draw_image(context, &mask, shadow.bounds(rect), ImageSampling::Linear) };
    }
}

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGContextGetUserSpaceToDeviceSpaceTransform(context: Id) -> CGAffineTransform;
}

unsafe fn device_scale(context: Id) -> f64 {
    let t = unsafe { CGContextGetUserSpaceToDeviceSpaceTransform(context) };
    (t.a * t.d - t.b * t.c).abs().sqrt()
}
