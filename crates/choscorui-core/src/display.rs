use crate::{
    FillRule, Image, ImageSampling, Paint, Path, Point, Rect, Size, StrokeStyle, Transform,
};
use std::hash::{DefaultHasher, Hash, Hasher};

/// Straight-alpha sRGB color, stored as eight-bit channels.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Color {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
    pub alpha: u8,
}

impl Color {
    /// A fully transparent color.
    pub const TRANSPARENT: Self = Self::rgba8(0, 0, 0, 0);
    /// Opaque black.
    pub const BLACK: Self = Self::rgba8(0, 0, 0, 255);
    /// Opaque white.
    pub const WHITE: Self = Self::rgba8(255, 255, 255, 255);

    /// A color from eight-bit red, green, blue, and alpha channels.
    pub const fn rgba8(red: u8, green: u8, blue: u8, alpha: u8) -> Self {
        Self {
            red,
            green,
            blue,
            alpha,
        }
    }

    /// An opaque color from eight-bit red, green, and blue channels.
    pub const fn rgb8(red: u8, green: u8, blue: u8) -> Self {
        Self::rgba8(red, green, blue, 255)
    }

    /// Same color with alpha multiplied by `factor` (0..=1).
    pub fn with_alpha(self, factor: f64) -> Self {
        Self {
            alpha: (f64::from(self.alpha) * factor.clamp(0.0, 1.0)).round() as u8,
            ..self
        }
    }

    /// Converts an sRGB channel to linear light, for contrast computation.
    fn linear(channel: u8) -> f64 {
        let c = f64::from(channel) / 255.0;
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    }

    /// WCAG relative luminance of the opaque color.
    pub fn relative_luminance(self) -> f64 {
        0.2126 * Self::linear(self.red)
            + 0.7152 * Self::linear(self.green)
            + 0.0722 * Self::linear(self.blue)
    }

    /// WCAG contrast ratio between two opaque colors, 1..=21.
    pub fn contrast_ratio(self, other: Self) -> f64 {
        let (a, b) = (self.relative_luminance(), other.relative_luminance());
        let (light, dark) = if a > b { (a, b) } else { (b, a) };
        (light + 0.05) / (dark + 0.05)
    }
}

/// Mouse cursor appearance requested by the hovered control.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum CursorShape {
    #[default]
    Arrow,
    IBeam,
    PointingHand,
    ClosedHand,
    OpenHand,
    Crosshair,
    NotAllowed,
    ResizeLeftRight,
    ResizeUpDown,
    /// Image-based cursor; `hotspot` is in image points.
    Custom {
        image: Image,
        hotspot: Point,
    },
}

/// A soft shadow of a rounded rectangle, with CSS `box-shadow` meaning:
/// `blur` is the blur radius (a Gaussian with sigma `blur / 2`), `spread`
/// grows or shrinks the shadow shape, and an inset shadow falls inside
/// the shape instead of outside.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shadow {
    pub offset: Point,
    pub blur: f64,
    pub spread: f64,
    pub color: Color,
    pub inset: bool,
}

impl Shadow {
    /// A drop shadow with no spread.
    pub const fn drop(offset: Point, blur: f64, color: Color) -> Self {
        Self {
            offset,
            blur,
            spread: 0.0,
            color,
            inset: false,
        }
    }

    /// Area the shadow may cover, in the same space as `rect`.
    pub fn bounds(&self, rect: Rect) -> Rect {
        if self.inset {
            return rect;
        }
        rect.translate(self.offset.x, self.offset.y)
            .inset(-(self.blur * 1.5 + self.spread.max(0.0) + 1.0))
    }

    fn is_valid(&self) -> bool {
        self.offset.x.is_finite()
            && self.offset.y.is_finite()
            && self.blur.is_finite()
            && self.blur >= 0.0
            && self.spread.is_finite()
    }

    /// Shared alpha mask so every renderer draws identical shadows. Pixels
    /// cover [`Shadow::bounds`] at `scale` pixels per point; color channels
    /// carry the shadow color and alpha its coverage. A drop shadow cuts
    /// out the casting shape (`rect` with `radius`); an inset shadow is
    /// clipped to it.
    pub fn mask(&self, rect: Rect, radius: f64, scale: f64) -> Option<crate::Image> {
        self.mask_corners(rect, [radius; 4], scale)
    }

    /// [`Shadow::mask`] for a casting shape with its own radius per
    /// corner, clockwise from the top left.
    pub fn mask_corners(&self, rect: Rect, radii: [f64; 4], scale: f64) -> Option<crate::Image> {
        let bounds = self.bounds(rect);
        let width = (bounds.size.width * scale).ceil().max(1.0) as u32;
        let height = (bounds.size.height * scale).ceil().max(1.0) as u32;
        if u64::from(width) * u64::from(height) > 16 * 1024 * 1024 {
            return None;
        }
        let shape = rect
            .translate(self.offset.x, self.offset.y)
            .inset(if self.inset {
                self.spread
            } else {
                -self.spread
            });
        let shape_radii = radii.map(|radius| {
            if self.inset {
                (radius - self.spread).max(0.0)
            } else {
                (radius + self.spread).max(0.0)
            }
        });
        let sigma = (self.blur / 2.0).max(1e-6) * std::f64::consts::SQRT_2;
        let mut pixels = Vec::with_capacity(width as usize * height as usize * 4);
        for y in 0..height {
            for x in 0..width {
                let p = Point::new(
                    bounds.min_x() + (f64::from(x) + 0.5) / scale,
                    bounds.min_y() + (f64::from(y) + 0.5) / scale,
                );
                let inside_caster = rounded_rect_distance(rect, radii, p) < 0.0;
                let d = rounded_rect_distance(shape, shape_radii, p);
                let coverage = match (self.inset, inside_caster) {
                    (false, true) | (true, false) => 0.0,
                    (false, false) => 0.5 * (1.0 - erf(d / sigma)),
                    (true, true) => 0.5 * (1.0 + erf(d / sigma)),
                };
                let alpha = (coverage * f64::from(self.color.alpha)).round() as u8;
                pixels.extend_from_slice(&[
                    self.color.red,
                    self.color.green,
                    self.color.blue,
                    alpha,
                ]);
            }
        }
        crate::Image::from_rgba8(width, height, pixels).ok()
    }
}

/// Signed distance to a rounded rectangle (negative inside), with the
/// radius of the corner in the quadrant of `p`. `radii` go clockwise from
/// the top left.
fn rounded_rect_distance(rect: Rect, radii: [f64; 4], p: Point) -> f64 {
    let c = rect.center();
    let radius = match (p.x >= c.x, p.y >= c.y) {
        (false, false) => radii[0],
        (true, false) => radii[1],
        (true, true) => radii[2],
        (false, true) => radii[3],
    };
    let r = radius
        .min(rect.size.width / 2.0)
        .min(rect.size.height / 2.0)
        .max(0.0);
    let qx = (p.x - c.x).abs() - (rect.size.width / 2.0 - r);
    let qy = (p.y - c.y).abs() - (rect.size.height / 2.0 - r);
    let outside = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt();
    outside + qx.max(qy).min(0.0) - r
}

/// Abramowitz and Stegun 7.1.26 approximation of the error function.
fn erf(x: f64) -> f64 {
    let sign = x.signum();
    let x = x.abs();
    let t = 1.0 / (1.0 + 0.327_591_1 * x);
    let y = 1.0
        - (((((1.061_405_429 * t - 1.453_152_027) * t) + 1.421_413_741) * t - 0.284_496_736) * t
            + 0.254_829_592)
            * t
            * (-x * x).exp();
    sign * y
}

/// Renderer-neutral commands in logical points, emitted in painter's order.
#[derive(Clone, Debug, PartialEq)]
pub enum DisplayCommand {
    FillRect {
        rect: Rect,
        color: Color,
    },
    StrokeRect {
        rect: Rect,
        color: Color,
        width: f64,
    },
    FillRoundedRect {
        rect: Rect,
        radius: f64,
        color: Color,
    },
    StrokeRoundedRect {
        rect: Rect,
        radius: f64,
        color: Color,
        width: f64,
    },
    FillPath {
        path: Path,
        paint: Paint,
        rule: FillRule,
    },
    StrokePath {
        path: Path,
        paint: Paint,
        style: StrokeStyle,
    },
    DrawImage {
        image: Image,
        /// Destination in logical points; the image scales to fill it.
        rect: Rect,
        sampling: ImageSampling,
    },
    DrawShadow {
        rect: Rect,
        radius: f64,
        shadow: Shadow,
    },
    /// [`DisplayCommand::DrawShadow`] for a shape with its own radius per
    /// corner, clockwise from the top left.
    DrawShadowCorners {
        rect: Rect,
        radii: [f64; 4],
        shadow: Shadow,
    },
    DrawText {
        baseline: Point,
        text: String,
        font_family: String,
        font_size: f64,
        /// Weight of the face; painters pick the nearest available face.
        font_weight: crate::FontWeight,
        /// Extra space after each character, in points.
        tracking: f64,
        color: Color,
    },
    /// Pre-shaped glyphs from a [`crate::TextLayout`]; `origin` is the
    /// line's baseline origin.
    DrawGlyphs {
        origin: Point,
        run: std::sync::Arc<crate::GlyphRun>,
        color: Color,
    },
    /// Blurs what the surface already shows inside the rounded `rect`,
    /// with a Gaussian of standard deviation `blur` points. It reads the
    /// surface beneath any open opacity group and writes into the group.
    BackdropBlur {
        rect: Rect,
        radius: f64,
        blur: f64,
    },
    /// [`DisplayCommand::BackdropBlur`] inside a rectangle with its own
    /// radius per corner, clockwise from the top left.
    BackdropBlurCorners {
        rect: Rect,
        radii: [f64; 4],
        blur: f64,
    },
    PushClip(Rect),
    /// Clip to the interior of a path (non-zero rule).
    PushClipPath(Path),
    PopClip,
    PushOpacity(f64),
    PopOpacity,
    /// Applies to later commands until the matching pop. Composes with
    /// earlier transforms; clip rectangles are in the transformed space.
    PushTransform(Transform),
    PopTransform,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DisplayListError {
    UnbalancedClip,
    InvalidText,
    InvalidGeometry,
    InvalidOpacity,
    UnbalancedOpacity,
    InvalidPaint,
    UnbalancedTransform,
}

impl std::fmt::Display for DisplayListError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnbalancedClip => f.write_str("pop_clip without a matching push_clip"),
            Self::InvalidText => {
                f.write_str("text has a non-finite position or size, or an empty font family")
            }
            Self::InvalidGeometry => f.write_str("geometry is not finite"),
            Self::InvalidOpacity => f.write_str("opacity must be finite and between 0 and 1"),
            Self::UnbalancedOpacity => f.write_str("pop_opacity without a matching push_opacity"),
            Self::InvalidPaint => f.write_str(
                "a gradient needs two or more ordered stops in 0..=1 and finite geometry",
            ),
            Self::UnbalancedTransform => {
                f.write_str("pop_transform without a matching push_transform")
            }
        }
    }
}

impl std::error::Error for DisplayListError {}

/// Font of one text line drawn with [`DisplayList::draw_text_run`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextRun<'a> {
    pub family: &'a str,
    pub size: f64,
    pub weight: crate::FontWeight,
    /// Extra space after each character, in points.
    pub tracking: f64,
}

/// An ordered display list. Clipping is nested and applies to later commands.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DisplayList {
    commands: Vec<DisplayCommand>,
    /// Open state in push order. Pops must close the innermost state, so
    /// painters can map every push to one graphics-state save.
    stack: Vec<StateKind>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum StateKind {
    Clip,
    Opacity,
    Transform,
}

impl DisplayList {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn fill_rect(&mut self, rect: Rect, color: Color) {
        self.commands.push(DisplayCommand::FillRect { rect, color });
    }

    pub fn stroke_rect(
        &mut self,
        rect: Rect,
        color: Color,
        width: f64,
    ) -> Result<(), DisplayListError> {
        validate_positive(width)?;
        self.commands
            .push(DisplayCommand::StrokeRect { rect, color, width });
        Ok(())
    }

    pub fn fill_rounded_rect(
        &mut self,
        rect: Rect,
        radius: f64,
        color: Color,
    ) -> Result<(), DisplayListError> {
        validate_radius(radius)?;
        self.commands.push(DisplayCommand::FillRoundedRect {
            rect,
            radius,
            color,
        });
        Ok(())
    }

    pub fn stroke_rounded_rect(
        &mut self,
        rect: Rect,
        radius: f64,
        color: Color,
        width: f64,
    ) -> Result<(), DisplayListError> {
        validate_radius(radius)?;
        validate_positive(width)?;
        self.commands.push(DisplayCommand::StrokeRoundedRect {
            rect,
            radius,
            color,
            width,
        });
        Ok(())
    }

    pub fn fill_path(
        &mut self,
        path: Path,
        paint: Paint,
        rule: FillRule,
    ) -> Result<(), DisplayListError> {
        if !path.is_finite() {
            return Err(DisplayListError::InvalidGeometry);
        }
        if !paint.is_valid() {
            return Err(DisplayListError::InvalidPaint);
        }
        self.commands
            .push(DisplayCommand::FillPath { path, paint, rule });
        Ok(())
    }

    pub fn stroke_path(
        &mut self,
        path: Path,
        paint: Paint,
        style: StrokeStyle,
    ) -> Result<(), DisplayListError> {
        if !path.is_finite() || !style.is_valid() {
            return Err(DisplayListError::InvalidGeometry);
        }
        if !paint.is_valid() {
            return Err(DisplayListError::InvalidPaint);
        }
        self.commands
            .push(DisplayCommand::StrokePath { path, paint, style });
        Ok(())
    }

    pub fn draw_image(
        &mut self,
        image: Image,
        rect: Rect,
        sampling: ImageSampling,
    ) -> Result<(), DisplayListError> {
        if !rect.is_finite() || rect.size.width < 0.0 || rect.size.height < 0.0 {
            return Err(DisplayListError::InvalidGeometry);
        }
        self.commands.push(DisplayCommand::DrawImage {
            image,
            rect,
            sampling,
        });
        Ok(())
    }

    pub fn draw_shadow(
        &mut self,
        rect: Rect,
        radius: f64,
        shadow: Shadow,
    ) -> Result<(), DisplayListError> {
        validate_radius(radius)?;
        if !rect.is_finite() || !shadow.is_valid() {
            return Err(DisplayListError::InvalidGeometry);
        }
        self.commands.push(DisplayCommand::DrawShadow {
            rect,
            radius,
            shadow,
        });
        Ok(())
    }

    /// A shadow of a rectangle with its own radius per corner, clockwise
    /// from the top left; see [`DisplayCommand::DrawShadowCorners`].
    pub fn draw_shadow_corners(
        &mut self,
        rect: Rect,
        radii: [f64; 4],
        shadow: Shadow,
    ) -> Result<(), DisplayListError> {
        validate_radii(radii)?;
        if !rect.is_finite() || !shadow.is_valid() {
            return Err(DisplayListError::InvalidGeometry);
        }
        self.commands.push(DisplayCommand::DrawShadowCorners {
            rect,
            radii,
            shadow,
        });
        Ok(())
    }

    /// Blurs the painted surface inside `rect` with its own radius per
    /// corner; see [`DisplayCommand::BackdropBlurCorners`].
    pub fn backdrop_blur_corners(
        &mut self,
        rect: Rect,
        radii: [f64; 4],
        blur: f64,
    ) -> Result<(), DisplayListError> {
        validate_radii(radii)?;
        validate_positive(blur)?;
        if !rect.is_finite() {
            return Err(DisplayListError::InvalidGeometry);
        }
        self.commands
            .push(DisplayCommand::BackdropBlurCorners { rect, radii, blur });
        Ok(())
    }

    /// Blurs the painted surface inside the rounded `rect`; see
    /// [`DisplayCommand::BackdropBlur`].
    pub fn backdrop_blur(
        &mut self,
        rect: Rect,
        radius: f64,
        blur: f64,
    ) -> Result<(), DisplayListError> {
        validate_radius(radius)?;
        validate_positive(blur)?;
        if !rect.is_finite() {
            return Err(DisplayListError::InvalidGeometry);
        }
        self.commands
            .push(DisplayCommand::BackdropBlur { rect, radius, blur });
        Ok(())
    }

    /// Stores a single unwrapped text line and its baseline in logical points.
    pub fn draw_text(
        &mut self,
        baseline: Point,
        text: &str,
        font_family: &str,
        font_size: f64,
        color: Color,
    ) -> Result<(), DisplayListError> {
        self.draw_text_run(
            baseline,
            text,
            &TextRun {
                family: font_family,
                size: font_size,
                weight: crate::FontWeight::REGULAR,
                tracking: 0.0,
            },
            color,
        )
    }

    /// Stores a single text line in a font with weight and tracking.
    pub fn draw_text_run(
        &mut self,
        baseline: Point,
        text: &str,
        run: &TextRun<'_>,
        color: Color,
    ) -> Result<(), DisplayListError> {
        if !baseline.x.is_finite()
            || !baseline.y.is_finite()
            || !run.size.is_finite()
            || run.size <= 0.0
            || !run.tracking.is_finite()
            || run.family.is_empty()
            || !(1..=1000).contains(&run.weight.0)
        {
            return Err(DisplayListError::InvalidText);
        }
        self.commands.push(DisplayCommand::DrawText {
            baseline,
            text: text.to_owned(),
            font_family: run.family.to_owned(),
            font_size: run.size,
            font_weight: run.weight,
            tracking: run.tracking,
            color,
        });
        Ok(())
    }

    /// Draws every layer of `layers` for the rounded rectangle, in order.
    pub fn draw_shadows(
        &mut self,
        rect: Rect,
        radius: f64,
        layers: &crate::ShadowLayers,
    ) -> Result<(), DisplayListError> {
        for shadow in layers.iter() {
            self.draw_shadow(rect, radius, *shadow)?;
        }
        Ok(())
    }

    /// An outset focus ring: a stroke of `width` just outside the rounded
    /// rectangle, following its radius. Every painter draws it as the
    /// equivalent rounded-rectangle stroke.
    pub fn draw_ring(
        &mut self,
        rect: Rect,
        radius: f64,
        width: f64,
        color: Color,
    ) -> Result<(), DisplayListError> {
        validate_positive(width)?;
        let half = width / 2.0;
        self.stroke_rounded_rect(rect.inset(-half), radius.max(0.0) + half, color, width)
    }

    pub fn draw_glyphs(
        &mut self,
        origin: Point,
        run: std::sync::Arc<crate::GlyphRun>,
        color: Color,
    ) -> Result<(), DisplayListError> {
        if !origin.x.is_finite() || !origin.y.is_finite() || run.glyphs.len() != run.positions.len()
        {
            return Err(DisplayListError::InvalidText);
        }
        self.commands
            .push(DisplayCommand::DrawGlyphs { origin, run, color });
        Ok(())
    }

    pub fn push_clip(&mut self, rect: Rect) {
        self.commands.push(DisplayCommand::PushClip(rect));
        self.stack.push(StateKind::Clip);
    }

    fn pop_state(
        &mut self,
        kind: StateKind,
        error: DisplayListError,
    ) -> Result<(), DisplayListError> {
        if self.stack.last() != Some(&kind) {
            return Err(error);
        }
        self.stack.pop();
        Ok(())
    }

    pub fn push_clip_path(&mut self, path: Path) -> Result<(), DisplayListError> {
        if !path.is_finite() {
            return Err(DisplayListError::InvalidGeometry);
        }
        self.commands.push(DisplayCommand::PushClipPath(path));
        self.stack.push(StateKind::Clip);
        Ok(())
    }

    pub fn pop_clip(&mut self) -> Result<(), DisplayListError> {
        self.pop_state(StateKind::Clip, DisplayListError::UnbalancedClip)?;
        self.commands.push(DisplayCommand::PopClip);
        Ok(())
    }

    pub fn push_opacity(&mut self, opacity: f64) -> Result<(), DisplayListError> {
        if !opacity.is_finite() || !(0.0..=1.0).contains(&opacity) {
            return Err(DisplayListError::InvalidOpacity);
        }
        self.commands.push(DisplayCommand::PushOpacity(opacity));
        self.stack.push(StateKind::Opacity);
        Ok(())
    }

    pub fn pop_opacity(&mut self) -> Result<(), DisplayListError> {
        self.pop_state(StateKind::Opacity, DisplayListError::UnbalancedOpacity)?;
        self.commands.push(DisplayCommand::PopOpacity);
        Ok(())
    }

    pub fn push_transform(&mut self, transform: Transform) -> Result<(), DisplayListError> {
        if !transform.is_finite() {
            return Err(DisplayListError::InvalidGeometry);
        }
        self.commands.push(DisplayCommand::PushTransform(transform));
        self.stack.push(StateKind::Transform);
        Ok(())
    }

    pub fn pop_transform(&mut self) -> Result<(), DisplayListError> {
        self.pop_state(StateKind::Transform, DisplayListError::UnbalancedTransform)?;
        self.commands.push(DisplayCommand::PopTransform);
        Ok(())
    }

    /// Appends another list's commands. Fails if `other` leaves state open.
    pub fn append(&mut self, other: &DisplayList) -> Result<(), DisplayListError> {
        if !other.is_balanced() {
            return Err(DisplayListError::UnbalancedClip);
        }
        self.commands.extend(other.commands.iter().cloned());
        Ok(())
    }

    pub fn is_balanced(&self) -> bool {
        self.stack.is_empty()
    }

    pub fn commands(&self) -> &[DisplayCommand] {
        &self.commands
    }

    pub fn len(&self) -> usize {
        self.commands.len()
    }

    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    /// Device-independent bounds of every visible draw command, in logical
    /// points after transforms and clips, paired with a fingerprint of the
    /// command and the state it is drawn under.
    pub fn draw_regions(&self) -> Vec<(u64, Rect)> {
        let mut regions = Vec::new();
        let mut transforms = vec![Transform::IDENTITY];
        let mut clips: Vec<Option<Rect>> = vec![None];
        let mut state_hash: Vec<u64> = vec![0];
        for command in &self.commands {
            let transform = *transforms.last().expect("root transform");
            let clip = *clips.last().expect("root clip");
            let state = *state_hash.last().expect("root state");
            let push_state = |stack: &mut Vec<u64>, command: &DisplayCommand| {
                let mut hasher = DefaultHasher::new();
                state.hash(&mut hasher);
                hash_command(command, &mut hasher);
                stack.push(hasher.finish());
            };
            match command {
                DisplayCommand::PushClip(rect) => {
                    let device = transform.apply_rect(*rect);
                    clips.push(Some(match clip {
                        Some(c) => c.intersection(device).unwrap_or_default(),
                        None => device,
                    }));
                    push_state(&mut state_hash, command);
                }
                DisplayCommand::PushClipPath(path) => {
                    let device = transform.apply_rect(path.bounds());
                    clips.push(Some(match clip {
                        Some(c) => c.intersection(device).unwrap_or_default(),
                        None => device,
                    }));
                    push_state(&mut state_hash, command);
                }
                DisplayCommand::PopClip => {
                    if clips.len() > 1 {
                        clips.pop();
                        state_hash.pop();
                    }
                }
                DisplayCommand::PushOpacity(_) => {
                    push_state(&mut state_hash, command);
                }
                DisplayCommand::PopOpacity => {
                    if state_hash.len() > 1 {
                        state_hash.pop();
                    }
                }
                DisplayCommand::PushTransform(next) => {
                    transforms.push(next.then(transform));
                    push_state(&mut state_hash, command);
                }
                DisplayCommand::PopTransform => {
                    if transforms.len() > 1 {
                        transforms.pop();
                        state_hash.pop();
                    }
                }
                draw => {
                    let Some(local) = draw_bounds(draw) else {
                        continue;
                    };
                    let mut device = transform.apply_rect(local);
                    if let Some(clip) = clip {
                        match device.intersection(clip) {
                            Some(visible) => device = visible,
                            None => continue,
                        }
                    }
                    let mut hasher = DefaultHasher::new();
                    state.hash(&mut hasher);
                    hash_command(draw, &mut hasher);
                    regions.push((hasher.finish(), device));
                }
            }
        }
        regions
    }
}

/// Region that must be repainted to turn `old` into `new`, or None when the
/// lists draw identical content. Commands are matched as a multiset, so
/// reordering unrelated draws in the same state damages nothing.
pub fn damage(old: &DisplayList, new: &DisplayList) -> Option<Rect> {
    if old == new {
        return None;
    }
    // Multiset difference of (hash, region) pairs via a sorted merge: two
    // allocations regardless of list size.
    let mut old = old.draw_regions();
    let mut new = new.draw_regions();
    old.sort_unstable_by_key(|(hash, _)| *hash);
    new.sort_unstable_by_key(|(hash, _)| *hash);
    let mut region: Option<Rect> = None;
    let mut add = |rect: Rect| {
        region = Some(match region {
            None => rect,
            Some(existing) => existing.union(rect),
        });
    };
    let (mut i, mut j) = (0, 0);
    while i < old.len() && j < new.len() {
        match old[i].0.cmp(&new[j].0) {
            std::cmp::Ordering::Equal => {
                i += 1;
                j += 1;
            }
            std::cmp::Ordering::Less => {
                add(old[i].1);
                i += 1;
            }
            std::cmp::Ordering::Greater => {
                add(new[j].1);
                j += 1;
            }
        }
    }
    for (_, rect) in old[i..].iter().chain(&new[j..]) {
        add(*rect);
    }
    region
}

/// Untransformed bounds of a draw command, inflated for stroke width,
/// shadow blur, and glyph extents.
fn draw_bounds(command: &DisplayCommand) -> Option<Rect> {
    let inflate = |rect: Rect, amount: f64| rect.inset(-amount);
    Some(match command {
        DisplayCommand::FillRect { rect, .. } | DisplayCommand::FillRoundedRect { rect, .. } => {
            *rect
        }
        DisplayCommand::StrokeRect { rect, width, .. }
        | DisplayCommand::StrokeRoundedRect { rect, width, .. } => inflate(*rect, width / 2.0),
        DisplayCommand::FillPath { path, .. } => path.bounds(),
        DisplayCommand::StrokePath { path, style, .. } => {
            // Miter joins can extend beyond half the width; 4x covers the
            // default miter limit of Core Graphics (10) for common angles.
            inflate(path.bounds(), style.width * 2.0)
        }
        DisplayCommand::DrawImage { rect, .. } => *rect,
        DisplayCommand::DrawShadow { rect, shadow, .. }
        | DisplayCommand::DrawShadowCorners { rect, shadow, .. } => shadow.bounds(*rect),
        // The blur reads up to three standard deviations outside `rect`.
        DisplayCommand::BackdropBlur { rect, blur, .. }
        | DisplayCommand::BackdropBlurCorners { rect, blur, .. } => inflate(*rect, blur * 3.0),
        DisplayCommand::DrawText {
            baseline,
            text,
            font_size,
            tracking,
            ..
        } => {
            // Conservative: em-box height and up to 1.2 em per character.
            let width = text.chars().count() as f64 * (font_size * 1.2 + tracking.max(0.0));
            Rect::new(
                Point::new(baseline.x - font_size, baseline.y - font_size * 1.3),
                Size::new(width + font_size * 2.0, font_size * 1.9),
            )
        }
        DisplayCommand::DrawGlyphs { origin, run, .. } => {
            // Glyph ink can overhang the typographic box (italics, accents).
            let pad = run.size * 0.25;
            inflate(run.bounds.translate(origin.x, origin.y), pad)
        }
        _ => return None,
    })
}

fn hash_f64(value: f64, hasher: &mut impl Hasher) {
    value.to_bits().hash(hasher);
}

fn hash_point(point: Point, hasher: &mut impl Hasher) {
    hash_f64(point.x, hasher);
    hash_f64(point.y, hasher);
}

fn hash_rect(rect: Rect, hasher: &mut impl Hasher) {
    hash_point(rect.origin, hasher);
    hash_f64(rect.size.width, hasher);
    hash_f64(rect.size.height, hasher);
}

fn hash_path(path: &Path, hasher: &mut impl Hasher) {
    for element in path.elements() {
        match *element {
            crate::PathElement::MoveTo(p) => {
                0u8.hash(hasher);
                hash_point(p, hasher);
            }
            crate::PathElement::LineTo(p) => {
                1u8.hash(hasher);
                hash_point(p, hasher);
            }
            crate::PathElement::QuadTo(c, p) => {
                2u8.hash(hasher);
                hash_point(c, hasher);
                hash_point(p, hasher);
            }
            crate::PathElement::CubicTo(a, b, p) => {
                3u8.hash(hasher);
                hash_point(a, hasher);
                hash_point(b, hasher);
                hash_point(p, hasher);
            }
            crate::PathElement::Close => 4u8.hash(hasher),
        }
    }
}

fn hash_paint(paint: &Paint, hasher: &mut impl Hasher) {
    match paint {
        Paint::Solid(color) => {
            0u8.hash(hasher);
            color.hash(hasher);
        }
        Paint::LinearGradient { start, end, stops } => {
            1u8.hash(hasher);
            hash_point(*start, hasher);
            hash_point(*end, hasher);
            for stop in stops {
                hash_f64(stop.offset, hasher);
                stop.color.hash(hasher);
            }
        }
        Paint::RadialGradient {
            center,
            radius,
            stops,
        } => {
            2u8.hash(hasher);
            hash_point(*center, hasher);
            hash_f64(*radius, hasher);
            for stop in stops {
                hash_f64(stop.offset, hasher);
                stop.color.hash(hasher);
            }
        }
    }
}

fn hash_shadow(shadow: &Shadow, hasher: &mut impl Hasher) {
    hash_point(shadow.offset, hasher);
    hash_f64(shadow.blur, hasher);
    hash_f64(shadow.spread, hasher);
    shadow.inset.hash(hasher);
    shadow.color.hash(hasher);
}

fn hash_command(command: &DisplayCommand, hasher: &mut impl Hasher) {
    std::mem::discriminant(command).hash(hasher);
    match command {
        DisplayCommand::FillRect { rect, color } => {
            hash_rect(*rect, hasher);
            color.hash(hasher);
        }
        DisplayCommand::StrokeRect { rect, color, width } => {
            hash_rect(*rect, hasher);
            color.hash(hasher);
            hash_f64(*width, hasher);
        }
        DisplayCommand::FillRoundedRect {
            rect,
            radius,
            color,
        } => {
            hash_rect(*rect, hasher);
            hash_f64(*radius, hasher);
            color.hash(hasher);
        }
        DisplayCommand::StrokeRoundedRect {
            rect,
            radius,
            color,
            width,
        } => {
            hash_rect(*rect, hasher);
            hash_f64(*radius, hasher);
            color.hash(hasher);
            hash_f64(*width, hasher);
        }
        DisplayCommand::FillPath { path, paint, rule } => {
            hash_path(path, hasher);
            hash_paint(paint, hasher);
            rule.hash(hasher);
        }
        DisplayCommand::StrokePath { path, paint, style } => {
            hash_path(path, hasher);
            hash_paint(paint, hasher);
            hash_f64(style.width, hasher);
            style.cap.hash(hasher);
            style.join.hash(hasher);
            for dash in &style.dash {
                hash_f64(*dash, hasher);
            }
        }
        DisplayCommand::DrawImage {
            image,
            rect,
            sampling,
        } => {
            image.id().hash(hasher);
            hash_rect(*rect, hasher);
            sampling.hash(hasher);
        }
        DisplayCommand::DrawShadow {
            rect,
            radius,
            shadow,
        } => {
            hash_rect(*rect, hasher);
            hash_f64(*radius, hasher);
            hash_shadow(shadow, hasher);
        }
        DisplayCommand::DrawShadowCorners {
            rect,
            radii,
            shadow,
        } => {
            hash_rect(*rect, hasher);
            radii.iter().for_each(|radius| hash_f64(*radius, hasher));
            hash_shadow(shadow, hasher);
        }
        DisplayCommand::DrawText {
            baseline,
            text,
            font_family,
            font_size,
            font_weight,
            tracking,
            color,
        } => {
            hash_point(*baseline, hasher);
            text.hash(hasher);
            font_family.hash(hasher);
            hash_f64(*font_size, hasher);
            font_weight.hash(hasher);
            hash_f64(*tracking, hasher);
            color.hash(hasher);
        }
        DisplayCommand::DrawGlyphs { origin, run, color } => {
            hash_point(*origin, hasher);
            run.font.hash(hasher);
            hash_f64(run.size, hasher);
            run.glyphs.hash(hasher);
            for position in &run.positions {
                hash_point(*position, hasher);
            }
            color.hash(hasher);
        }
        DisplayCommand::BackdropBlur { rect, radius, blur } => {
            hash_rect(*rect, hasher);
            hash_f64(*radius, hasher);
            hash_f64(*blur, hasher);
        }
        DisplayCommand::BackdropBlurCorners { rect, radii, blur } => {
            hash_rect(*rect, hasher);
            radii.iter().for_each(|radius| hash_f64(*radius, hasher));
            hash_f64(*blur, hasher);
        }
        DisplayCommand::PushClip(rect) => hash_rect(*rect, hasher),
        DisplayCommand::PushClipPath(path) => hash_path(path, hasher),
        DisplayCommand::PushOpacity(opacity) => hash_f64(*opacity, hasher),
        DisplayCommand::PushTransform(t) => {
            for value in [t.a, t.b, t.c, t.d, t.tx, t.ty] {
                hash_f64(value, hasher);
            }
        }
        DisplayCommand::PopClip | DisplayCommand::PopOpacity | DisplayCommand::PopTransform => {}
    }
}

fn validate_positive(value: f64) -> Result<(), DisplayListError> {
    if value.is_finite() && value > 0.0 {
        Ok(())
    } else {
        Err(DisplayListError::InvalidGeometry)
    }
}

fn validate_radius(radius: f64) -> Result<(), DisplayListError> {
    if radius.is_finite() && radius >= 0.0 {
        Ok(())
    } else {
        Err(DisplayListError::InvalidGeometry)
    }
}

fn validate_radii(radii: [f64; 4]) -> Result<(), DisplayListError> {
    radii.into_iter().try_for_each(validate_radius)
}
