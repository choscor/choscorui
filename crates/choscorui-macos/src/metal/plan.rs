//! Turns a display list into the instances, coverage edges, and passes of
//! one frame. The layouts here match the structs of `ui.metal`.
//!
//! One state stack follows the pushes and pops of the list, as the Core
//! Graphics painter does. Each push saves the state, and each pop
//! restores the most recent save.

use super::Gpu;
use super::coverage::{
    ATLAS_SIZE, PixelBox, Region, Shelves, append_edges, intersect, max_side, pixel_bounds,
    samples_for, tiles,
};
use super::glyphs::{FontId, SUBPIXELS, Text};
use crate::ffi::metal::replace_region;
use crate::ffi::metal::{MTLOrigin, MTLRegion, MTLSize, MTLTexture, PIXEL_FORMAT_RGBA8};
use crate::ffi::retained::Retained;
use choscorui_core::{
    Color, DisplayCommand, DisplayList, FillRule, Image, ImageSampling, LineJoin, Paint, Path,
    Point, Rect, Shadow, StrokeStyle, TextRun, Transform, blur_box_radii, dash_polylines,
    stroke_polygons,
};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

/// The 32-bit words of one `Instance`.
pub(super) const INSTANCE_WORDS: usize = 52;
/// The 32-bit words of one `Stop`.
pub(super) const STOP_WORDS: usize = 8;
/// The 32-bit words of one `Clip`.
pub(super) const CLIP_WORDS: usize = 16;

const KIND_FILL: u32 = 0;
const KIND_STROKE: u32 = 1;
const KIND_SHADOW: u32 = 2;
const KIND_IMAGE: u32 = 3;
const KIND_PATH: u32 = 4;
const KIND_GLYPH: u32 = 5;
const KIND_COLOR_GLYPH: u32 = 6;
const KIND_BLUR: u32 = 7;
const KIND_GROUP: u32 = 8;

const FLAG_INSET: u32 = 1;
const FLAG_NEAREST: u32 = 2;
const FLAG_EVEN_ODD: u32 = 4;

const PAINT_LINEAR: u32 = 1;
const PAINT_RADIAL: u32 = 2;

const CLIP_COVERAGE: u32 = 1;

/// The flattening tolerance of paths, in device pixels.
const TOLERANCE: f64 = 0.05;
/// The cached image textures. A full cache starts over.
const IMAGE_LIMIT: usize = 256;

/// The texture that a pass draws into.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Target {
    Frame,
    /// The texture of the opacity group at this depth, from 1.
    Group(u32),
}

/// The texture that the instances of a draw read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Source {
    None,
    /// An index into [`Plan::textures`].
    Texture(u32),
    /// The texture of the opacity group at this depth.
    Group(u32),
    /// The result of a blur, by index into [`Plan::blurs`].
    Blur(u32),
}

/// One step of a frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Op {
    /// Draws `count` instances from `first`. `coverage` is the coverage
    /// atlas that they read.
    Draw {
        target: Target,
        source: Source,
        coverage: Option<u32>,
        first: u32,
        count: u32,
    },
    /// The next pass into the group at this depth clears it first.
    ClearGroup(u32),
    /// Blurs a region of the frame, by index into [`Plan::blurs`].
    Blur(u32),
}

/// A region of the frame to blur.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct BlurPass {
    pub(super) origin: (usize, usize),
    pub(super) size: (usize, usize),
    /// The box radii of each direction, or `None` to copy unblurred.
    pub(super) radii: Option<[usize; 3]>,
}

/// Everything that the encoder needs to draw one frame.
pub(super) struct Plan {
    pub(super) width: usize,
    pub(super) height: usize,
    pub(super) instances: Vec<u32>,
    pub(super) stops: Vec<u32>,
    pub(super) clips: Vec<u32>,
    /// The edges of each coverage atlas.
    pub(super) edges: Vec<Vec<f32>>,
    pub(super) ops: Vec<Op>,
    pub(super) textures: Vec<Retained<MTLTexture>>,
    pub(super) blurs: Vec<BlurPass>,
    /// The deepest opacity group.
    pub(super) groups: u32,
    /// True when a blur reads the frame, so the frame is a texture of its
    /// own and not the drawable.
    pub(super) reads_frame: bool,
}

#[derive(Clone, Copy, Default)]
struct Instance {
    bounds: [f32; 4],
    xform: [f32; 4],
    origin: [f32; 4],
    paint_xform: [f32; 4],
    gradient: [f32; 4],
    shape: [f32; 4],
    radii: [f32; 4],
    color: [f32; 4],
    params: [f32; 4],
    params2: [f32; 4],
    clip: [f32; 4],
    info: [u32; 4],
    paint: [u32; 4],
}

impl Instance {
    /// An instance of `kind` whose local space maps to device pixels with
    /// `device`.
    fn local(kind: u32, device: Transform) -> Self {
        Self {
            xform: [device.a, device.b, device.c, device.d].map(|v| v as f32),
            origin: [device.tx as f32, device.ty as f32, 0.0, 0.0],
            paint_xform: [1.0, 0.0, 0.0, 1.0],
            info: [kind, 0, 0, 0],
            ..Self::default()
        }
    }

    /// An instance of `kind` in device pixels.
    fn device(kind: u32) -> Self {
        Self::local(kind, Transform::IDENTITY)
    }

    fn write(&self, out: &mut Vec<u32>) {
        for value in [
            self.bounds,
            self.xform,
            self.origin,
            self.paint_xform,
            self.gradient,
            self.shape,
            self.radii,
            self.color,
            self.params,
            self.params2,
            self.clip,
        ] {
            out.extend(value.map(f32::to_bits));
        }
        out.extend(self.info);
        out.extend(self.paint);
    }
}

fn rect_words(rect: Rect) -> [f32; 4] {
    [rect.min_x(), rect.min_y(), rect.max_x(), rect.max_y()].map(|v| v as f32)
}

fn color_words(color: Color) -> [f32; 4] {
    [color.red, color.green, color.blue, color.alpha].map(|v| f32::from(v) / 255.0)
}

/// How far glyphs reach above and below the baseline, in font sizes.
/// The values hold the tall marks of Vietnamese and of decorative fonts.
const LINE_REACH: [f64; 2] = [3.0, 2.0];

/// A rectangle far from every pixel, for a stroke without an inside.
const NOWHERE: [f32; 4] = [1.0e9, 1.0e9, 1.0e9 + 1.0, 1.0e9 + 1.0];

/// One clip of the state, in addition to the axis-aligned clip box.
#[derive(Clone)]
enum ClipShape {
    /// A rounded rectangle in a local space, which `inverse` and `offset`
    /// map from device pixels.
    Shape {
        rect: [f32; 4],
        radii: [f32; 4],
        inverse: [f32; 4],
        offset: [f32; 2],
    },
    /// A path in device pixels, whose winding numbers go into an atlas.
    Path {
        id: usize,
        polygons: Rc<Vec<Vec<Point>>>,
        bounds: PixelBox,
    },
}

/// The clips of an instance in [`Plan::clips`], and the coverage atlas
/// that they read.
#[derive(Clone, Copy)]
struct ClipList {
    atlas: Option<u32>,
    start: u32,
    count: u32,
}

#[derive(Clone)]
struct State {
    /// Local space to logical points.
    transform: Transform,
    /// Local space to device pixels.
    device: Transform,
    /// The axis-aligned clip box in device pixels: left, top, right,
    /// bottom.
    clip: [f64; 4],
    clips: Rc<Vec<ClipShape>>,
    clip_list: Cell<Option<ClipList>>,
    target: Target,
}

struct Saved {
    state: State,
    /// The opacity of the group that the push opened.
    group: Option<f64>,
}

/// The process-wide texture caches. Only the main thread uses them.
struct Caches {
    images: HashMap<u64, Retained<MTLTexture>>,
    text: Text,
}

thread_local! {
    static CACHES: RefCell<Option<Caches>> = const { RefCell::new(None) };
}

/// Drops the image textures, as the Core Graphics painter drops its
/// images.
pub(super) fn clear_caches() {
    CACHES.with(|caches| {
        if let Some(caches) = caches.borrow_mut().as_mut() {
            caches.images.clear();
        }
    });
}

/// The plan of `list` for a frame of `width × height` pixels at `scale`
/// pixels per point.
pub(super) fn build(
    gpu: &Gpu,
    list: &DisplayList,
    width: usize,
    height: usize,
    scale: f64,
) -> Plan {
    CACHES.with(|caches| {
        let mut caches = caches.borrow_mut();
        let caches = caches.get_or_insert_with(|| Caches {
            images: HashMap::new(),
            text: Text::new(),
        });
        let mut builder = Builder::new(gpu, caches, width, height, scale);
        builder.plan.reads_frame = list.commands().iter().any(|command| {
            matches!(
                command,
                DisplayCommand::BackdropBlur { .. } | DisplayCommand::BackdropBlurCorners { .. }
            )
        });
        for command in list.commands() {
            builder.command(command);
        }
        while !builder.stack.is_empty() {
            builder.pop();
        }
        builder.plan
    })
}

struct Builder<'a> {
    gpu: &'a Gpu,
    caches: &'a mut Caches,
    plan: Plan,
    scale: f64,
    viewport: [f64; 4],
    state: State,
    stack: Vec<Saved>,
    texture_ids: HashMap<usize, u32>,
    shelves: Shelves,
    atlas: u32,
    clip_regions: HashMap<(usize, u32), Region>,
    next_clip: usize,
    /// The device bounds of what each open group holds.
    group_bounds: Vec<Option<[f64; 4]>>,
}

impl<'a> Builder<'a> {
    fn new(gpu: &'a Gpu, caches: &'a mut Caches, width: usize, height: usize, scale: f64) -> Self {
        let viewport = [0.0, 0.0, width as f64, height as f64];
        let device = Transform::scale(scale, scale);
        Self {
            gpu,
            caches,
            plan: Plan {
                width,
                height,
                instances: Vec::new(),
                stops: Vec::new(),
                clips: Vec::new(),
                edges: vec![Vec::new()],
                ops: Vec::new(),
                textures: Vec::new(),
                blurs: Vec::new(),
                groups: 0,
                reads_frame: false,
            },
            scale,
            viewport,
            state: State {
                transform: Transform::IDENTITY,
                device,
                clip: viewport,
                clips: Rc::new(Vec::new()),
                clip_list: Cell::new(None),
                target: Target::Frame,
            },
            stack: Vec::new(),
            texture_ids: HashMap::new(),
            shelves: Shelves::new(ATLAS_SIZE),
            atlas: 0,
            clip_regions: HashMap::new(),
            next_clip: 0,
            group_bounds: Vec::new(),
        }
    }

    fn command(&mut self, command: &DisplayCommand) {
        match command {
            DisplayCommand::FillRect { rect, color } => {
                self.fill_shape(*rect, [0.0; 4], &Paint::Solid(*color));
            }
            DisplayCommand::StrokeRect { rect, color, width } => {
                let mut style = StrokeStyle::new(*width);
                style.join = LineJoin::Miter;
                self.stroke_rounded(*rect, [0.0; 4], &Paint::Solid(*color), &style);
            }
            DisplayCommand::FillRoundedRect {
                rect,
                radius,
                color,
            } => {
                let radii = [clamp_radius(*rect, *radius); 4];
                self.fill_shape(*rect, radii, &Paint::Solid(*color));
            }
            DisplayCommand::StrokeRoundedRect {
                rect,
                radius,
                color,
                width,
            } => {
                let radii = [clamp_radius(*rect, *radius); 4];
                let style = StrokeStyle::new(*width);
                self.stroke_rounded(*rect, radii, &Paint::Solid(*color), &style);
            }
            DisplayCommand::FillPath { path, paint, rule } => self.fill(path, paint, *rule),
            DisplayCommand::StrokePath { path, paint, style } => self.stroke(path, paint, style),
            DisplayCommand::DrawImage {
                image,
                rect,
                sampling,
            } => self.image(image, *rect, *sampling),
            DisplayCommand::DrawShadow {
                rect,
                radius,
                shadow,
            } => self.shadow(*rect, [*radius; 4], *shadow),
            DisplayCommand::DrawShadowCorners {
                rect,
                radii,
                shadow,
            } => self.shadow(*rect, *radii, *shadow),
            DisplayCommand::BackdropBlur { rect, radius, blur } => {
                self.backdrop(*rect, [*radius; 4], *blur);
            }
            DisplayCommand::BackdropBlurCorners { rect, radii, blur } => {
                self.backdrop(*rect, *radii, *blur);
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
                if color.alpha == 0 || text.is_empty() || !self.line_visible(*baseline, *font_size)
                {
                    return;
                }
                let run = TextRun {
                    family: font_family,
                    size: *font_size,
                    weight: *font_weight,
                    slant: *font_slant,
                    tracking: *tracking,
                };
                let runs = self.caches.text.line(text, &run);
                for run in runs.iter().flat_map(|runs| runs.iter()) {
                    for (glyph, position) in run.glyphs.iter().zip(&run.positions) {
                        let pen = Point::new(baseline.x + position.x, baseline.y + position.y);
                        self.glyph(run.font, *glyph, pen, *color);
                    }
                }
            }
            DisplayCommand::DrawGlyphs { origin, run, color } => {
                if color.alpha == 0
                    || run.glyphs.is_empty()
                    || !self.line_visible(*origin, run.size)
                {
                    return;
                }
                let Some(font) = self.caches.text.named_font(&run.font, run.size) else {
                    return;
                };
                for (glyph, position) in run.glyphs.iter().zip(&run.positions) {
                    let pen = Point::new(origin.x + position.x, origin.y + position.y);
                    self.glyph(font, *glyph, pen, *color);
                }
            }
            DisplayCommand::PushClip(rect) => {
                self.save(None);
                self.clip_rect(*rect, [0.0; 4]);
            }
            DisplayCommand::PushClipPath(path) => {
                self.save(None);
                self.clip_path(path);
            }
            DisplayCommand::PushOpacity(opacity) => self.save(Some(*opacity)),
            DisplayCommand::PushTransform(transform) => {
                self.save(None);
                self.state.transform = transform.then(self.state.transform);
                self.state.device = self
                    .state
                    .transform
                    .then(Transform::scale(self.scale, self.scale));
            }
            DisplayCommand::PopClip | DisplayCommand::PopOpacity | DisplayCommand::PopTransform => {
                self.pop();
            }
        }
    }

    fn save(&mut self, group: Option<f64>) {
        self.stack.push(Saved {
            state: self.state.clone(),
            group,
        });
        if group.is_some() {
            let depth = self
                .stack
                .iter()
                .filter(|saved| saved.group.is_some())
                .count() as u32;
            self.plan.groups = self.plan.groups.max(depth);
            self.plan.ops.push(Op::ClearGroup(depth));
            self.state.target = Target::Group(depth);
            self.group_bounds.push(None);
        }
    }

    fn pop(&mut self) {
        let Some(saved) = self.stack.pop() else {
            return;
        };
        let target = self.state.target;
        self.state = saved.state;
        if let (Some(opacity), Target::Group(depth)) = (saved.group, target) {
            let bounds = self.group_bounds.pop().flatten();
            if let Some(bounds) = bounds {
                self.composite(depth, opacity, bounds);
            }
        }
    }

    /// Draws the group at `depth` into the current target.
    fn composite(&mut self, depth: u32, opacity: f64, bounds: [f64; 4]) {
        let mut instance = Instance::device(KIND_GROUP);
        let words = bounds.map(|v| v as f32);
        instance.bounds = words;
        instance.shape = words;
        instance.params = [opacity as f32, 0.0, 0.0, 0.0];
        let list = ClipList {
            atlas: None,
            start: 0,
            count: 0,
        };
        let viewport = self.viewport;
        self.push_instance(instance, Source::Group(depth), None, list, viewport, bounds);
    }

    /// The scale of local lengths in device pixels.
    fn local_scale(&self) -> f64 {
        self.state.device.determinant().abs().sqrt().max(1e-9)
    }

    /// The device bounds of the local rectangle `rect` inside the clip,
    /// or `None` when nothing of it is visible.
    fn visible(&self, rect: Rect) -> Option<[f64; 4]> {
        let device = self.state.device.apply_rect(rect);
        let clip = self.state.clip;
        let bounds = [
            device.min_x().max(clip[0]),
            device.min_y().max(clip[1]),
            device.max_x().min(clip[2]),
            device.max_y().min(clip[3]),
        ];
        (bounds[0] < bounds[2] && bounds[1] < bounds[3]).then_some(bounds)
    }

    /// False when no glyph of a line with its baseline at `baseline` and
    /// a font of `size` points can be in the clip. Lines out of view then
    /// need no shaping and no glyph lookups.
    fn line_visible(&self, baseline: Point, size: f64) -> bool {
        // Glyphs reach at most this many font sizes above and below the
        // baseline. The line can be of any width.
        let (above, below) = (LINE_REACH[0] * size, LINE_REACH[1] * size);
        let band = Rect::from_xywh(-1.0e7, baseline.y - above, 2.0e7, above + below);
        self.visible(band).is_some()
    }

    /// Records an instance with the current clips.
    fn add(&mut self, instance: Instance, source: Source, coverage: Option<u32>, bounds: [f64; 4]) {
        let list = self.clip_list();
        let clip = self.state.clip;
        self.push_instance(instance, source, coverage, list, clip, bounds);
    }

    fn push_instance(
        &mut self,
        mut instance: Instance,
        source: Source,
        coverage: Option<u32>,
        list: ClipList,
        clip: [f64; 4],
        bounds: [f64; 4],
    ) {
        instance.clip = clip.map(|v| v as f32);
        instance.info[2] = list.start;
        instance.info[3] = list.count;
        let coverage = coverage.or(list.atlas);
        let index = (self.plan.instances.len() / INSTANCE_WORDS) as u32;
        instance.write(&mut self.plan.instances);
        let target = self.state.target;
        let merged = match self.plan.ops.last_mut() {
            Some(Op::Draw {
                target: last_target,
                source: last_source,
                coverage: last_coverage,
                first,
                count,
            }) if *last_target == target
                && *first + *count == index
                && (*last_source == source
                    || *last_source == Source::None
                    || source == Source::None)
                && (*last_coverage == coverage
                    || last_coverage.is_none()
                    || coverage.is_none()) =>
            {
                *count += 1;
                if *last_source == Source::None {
                    *last_source = source;
                }
                if last_coverage.is_none() {
                    *last_coverage = coverage;
                }
                true
            }
            _ => false,
        };
        if !merged {
            self.plan.ops.push(Op::Draw {
                target,
                source,
                coverage,
                first: index,
                count: 1,
            });
        }
        if let Some(group) = self.group_bounds.last_mut() {
            let union = group.map_or(bounds, |g| {
                [
                    g[0].min(bounds[0]),
                    g[1].min(bounds[1]),
                    g[2].max(bounds[2]),
                    g[3].max(bounds[3]),
                ]
            });
            *group = Some(union);
        }
    }

    fn texture_index(&mut self, texture: &Retained<MTLTexture>) -> u32 {
        let next = self.plan.textures.len() as u32;
        *self.texture_ids.entry(texture.addr()).or_insert_with(|| {
            self.plan.textures.push(texture.clone());
            next
        })
    }

    /// Sets the paint of `instance`. `to_paint` maps its local space to
    /// the space of the paint.
    fn set_paint(&mut self, instance: &mut Instance, paint: &Paint, to_paint: Transform) {
        instance.paint_xform = [to_paint.a, to_paint.b, to_paint.c, to_paint.d].map(|v| v as f32);
        instance.origin[2] = to_paint.tx as f32;
        instance.origin[3] = to_paint.ty as f32;
        let stops = match paint {
            Paint::Solid(color) => {
                instance.color = color_words(*color);
                return;
            }
            Paint::LinearGradient { start, end, stops } => {
                instance.paint[0] = PAINT_LINEAR;
                instance.gradient = [start.x, start.y, end.x, end.y].map(|v| v as f32);
                stops
            }
            Paint::RadialGradient {
                center,
                radius,
                stops,
            } => {
                instance.paint[0] = PAINT_RADIAL;
                instance.gradient = [center.x as f32, center.y as f32, *radius as f32, 0.0];
                stops
            }
        };
        instance.paint[1] = (self.plan.stops.len() / STOP_WORDS) as u32;
        instance.paint[2] = stops.len() as u32;
        for stop in stops {
            self.plan
                .stops
                .extend(color_words(stop.color).map(f32::to_bits));
            self.plan
                .stops
                .extend([stop.offset as f32, 0.0, 0.0, 0.0].map(f32::to_bits));
        }
    }

    fn fill(&mut self, path: &Path, paint: &Paint, rule: FillRule) {
        if paint.is_transparent() {
            return;
        }
        if let Some((rect, radii)) = path.as_rounded_rect() {
            self.fill_shape(rect, radii, paint);
            return;
        }
        if let Some(rect) = path.as_ellipse().filter(is_circle) {
            self.fill_shape(rect, [rect.size.width / 2.0; 4], paint);
            return;
        }
        let device = path.transformed(self.state.device);
        let polygons: Vec<Vec<Point>> = device
            .flatten(TOLERANCE)
            .into_iter()
            .map(|(points, _)| points)
            .collect();
        self.fill_polygons(polygons, paint, rule);
    }

    /// Fills a rounded rectangle with an SDF instance.
    fn fill_shape(&mut self, rect: Rect, radii: [f64; 4], paint: &Paint) {
        if paint.is_transparent() || rect.size.width <= 0.0 || rect.size.height <= 0.0 {
            return;
        }
        let margin = 1.0 / self.local_scale();
        let quad = rect.inset(-margin);
        let Some(bounds) = self.visible(quad) else {
            return;
        };
        let mut instance = Instance::local(KIND_FILL, self.state.device);
        instance.bounds = rect_words(quad);
        instance.shape = rect_words(rect);
        instance.radii = radii.map(|v| v as f32);
        self.set_paint(&mut instance, paint, Transform::IDENTITY);
        self.add(instance, Source::None, None, bounds);
    }

    fn stroke(&mut self, path: &Path, paint: &Paint, style: &StrokeStyle) {
        if paint.is_transparent() || !style.is_valid() {
            return;
        }
        if style.dash.is_empty()
            && let Some((rect, radii)) = path.as_rounded_rect()
            && self.stroke_rounded(rect, radii, paint, style)
        {
            return;
        }
        if style.dash.is_empty()
            && let Some(rect) = path.as_ellipse().filter(is_circle)
            && self.stroke_rounded(rect, [rect.size.width / 2.0; 4], paint, style)
        {
            return;
        }
        let local_scale = self.local_scale();
        let tolerance = TOLERANCE / local_scale;
        let lines = dash_polylines(path.flatten(tolerance), &style.dash);
        let device = self.state.device;
        let polygons = stroke_polygons(&lines, style, tolerance)
            .into_iter()
            .map(|polygon| polygon.into_iter().map(|p| device.apply(p)).collect())
            .collect();
        self.fill_polygons(polygons, paint, FillRule::NonZero);
    }

    /// Strokes the outline of a rounded rectangle without dashes. Returns
    /// `false`, and draws nothing, for bevel joins at square corners,
    /// which need the path stroke.
    fn stroke_rounded(
        &mut self,
        rect: Rect,
        radii: [f64; 4],
        paint: &Paint,
        style: &StrokeStyle,
    ) -> bool {
        // Joins only show at square corners.
        let square_corner = radii.iter().any(|radius| *radius <= 0.0);
        if square_corner && style.join == LineJoin::Bevel {
            return false;
        }
        if paint.is_transparent() || !style.is_valid() {
            return true;
        }
        let half = style.width / 2.0;
        let outer = radii.map(|radius| {
            if radius > 0.0 || style.join == LineJoin::Round {
                radius + half
            } else {
                0.0
            }
        });
        let inner = radii.map(|radius| (radius - half).max(0.0));
        self.stroke_shape(rect, outer, inner, half, paint);
        true
    }

    /// Strokes a rounded rectangle: the area between its outline grown
    /// and shrunk by `half`.
    fn stroke_shape(
        &mut self,
        rect: Rect,
        outer_radii: [f64; 4],
        inner_radii: [f64; 4],
        half: f64,
        paint: &Paint,
    ) {
        let outer = rect.inset(-half);
        let margin = 1.0 / self.local_scale();
        let quad = outer.inset(-margin);
        let Some(bounds) = self.visible(quad) else {
            return;
        };
        let inner = rect.inset(half);
        let mut instance = Instance::local(KIND_STROKE, self.state.device);
        instance.bounds = rect_words(quad);
        instance.shape = rect_words(outer);
        instance.radii = outer_radii.map(|v| v as f32);
        instance.params2 = if inner.size.width > 0.0 && inner.size.height > 0.0 {
            rect_words(inner)
        } else {
            NOWHERE
        };
        instance.params = inner_radii.map(|v| v as f32);
        self.set_paint(&mut instance, paint, Transform::IDENTITY);
        self.add(instance, Source::None, None, bounds);
    }

    /// Fills closed polygons in device pixels through coverage atlases.
    fn fill_polygons(&mut self, polygons: Vec<Vec<Point>>, paint: &Paint, rule: FillRule) {
        let Some(to_paint) = self.state.device.invert() else {
            return;
        };
        let Some(bounds) = pixel_bounds(&polygons)
            .and_then(|bounds| intersect(bounds, round_out(self.state.clip)))
        else {
            return;
        };
        let width = (bounds[2] - bounds[0]) as u32;
        let height = (bounds[3] - bounds[1]) as u32;
        let samples = samples_for(width, height).max(2);
        for tile in tiles(bounds, max_side(samples)) {
            let Some((region, list)) = self.allocate_with_clips(tile, samples, &polygons) else {
                continue;
            };
            let mut instance = Instance::device(KIND_PATH);
            let words = tile.map(|v| v as f32);
            instance.bounds = words;
            instance.shape = words;
            instance.params = [region.offset.0 as f32, region.offset.1 as f32, 0.0, 0.0];
            instance.paint[3] = samples;
            if rule == FillRule::EvenOdd {
                instance.info[1] = FLAG_EVEN_ODD;
            }
            self.set_paint(&mut instance, paint, to_paint);
            let clip = self.state.clip;
            let tile_bounds = tile.map(f64::from);
            let atlas = self.atlas;
            self.push_instance(instance, Source::None, Some(atlas), list, clip, tile_bounds);
        }
    }

    /// A coverage region for `tile` and the clip list, both in the same
    /// atlas.
    fn allocate_with_clips(
        &mut self,
        tile: PixelBox,
        samples: u32,
        polygons: &[Vec<Point>],
    ) -> Option<(Region, ClipList)> {
        for _ in 0..3 {
            let region = self.allocate(tile, samples, polygons)?;
            let atlas = self.atlas;
            let list = self.clip_list();
            if self.atlas == atlas {
                return Some((region, list));
            }
        }
        None
    }

    /// A region of the current atlas for `pixels`, with the edges of
    /// `polygons` in it. A full atlas moves to the next one.
    fn allocate(
        &mut self,
        pixels: PixelBox,
        samples: u32,
        polygons: &[Vec<Point>],
    ) -> Option<Region> {
        let width = (pixels[2] - pixels[0]) as u32 * samples;
        let height = (pixels[3] - pixels[1]) as u32 * samples;
        let offset = match self.shelves.allocate(width, height) {
            Some(offset) => offset,
            None => {
                self.atlas += 1;
                self.shelves.clear();
                self.plan.edges.push(Vec::new());
                self.shelves.allocate(width, height)?
            }
        };
        let region = Region {
            pixels,
            offset,
            samples,
        };
        append_edges(polygons, &region, &mut self.plan.edges[self.atlas as usize]);
        Some(region)
    }

    /// The clips of the state in [`Plan::clips`]. Path clips get regions
    /// in the current atlas.
    fn clip_list(&mut self) -> ClipList {
        if let Some(list) = self.state.clip_list.get()
            && list.atlas.is_none_or(|atlas| atlas == self.atlas)
        {
            return list;
        }
        let clips = Rc::clone(&self.state.clips);
        loop {
            let atlas = self.atlas;
            let start = self.plan.clips.len();
            let mut uses_atlas = false;
            for clip in clips.iter() {
                match clip {
                    ClipShape::Shape {
                        rect,
                        radii,
                        inverse,
                        offset,
                    } => {
                        let mut words = Vec::with_capacity(CLIP_WORDS);
                        words.extend(rect.map(f32::to_bits));
                        words.extend(radii.map(f32::to_bits));
                        words.extend(inverse.map(f32::to_bits));
                        words.extend(offset.map(f32::to_bits));
                        words.extend([0, 0]);
                        self.plan.clips.extend(words);
                    }
                    ClipShape::Path {
                        id,
                        polygons,
                        bounds,
                    } => {
                        uses_atlas = true;
                        let region = match self.clip_regions.get(&(*id, self.atlas)) {
                            Some(region) => *region,
                            None => {
                                let width = (bounds[2] - bounds[0]) as u32;
                                let height = (bounds[3] - bounds[1]) as u32;
                                let samples = samples_for(width, height);
                                let Some(region) = self.allocate(*bounds, samples, polygons) else {
                                    continue;
                                };
                                self.clip_regions.insert((*id, self.atlas), region);
                                region
                            }
                        };
                        let rect = region.pixels.map(|v| v as f32);
                        self.plan.clips.extend(rect.map(f32::to_bits));
                        self.plan.clips.extend([0u32; 8]);
                        self.plan.clips.extend(
                            [region.offset.0 as f32, region.offset.1 as f32].map(f32::to_bits),
                        );
                        self.plan.clips.extend([CLIP_COVERAGE, region.samples]);
                    }
                }
            }
            if self.atlas != atlas {
                // An allocation moved to a new atlas: list the clips again.
                self.plan.clips.truncate(start);
                continue;
            }
            let list = ClipList {
                atlas: uses_atlas.then_some(atlas),
                start: (start / CLIP_WORDS) as u32,
                count: ((self.plan.clips.len() - start) / CLIP_WORDS) as u32,
            };
            self.state.clip_list.set(Some(list));
            return list;
        }
    }

    /// Clips to `rect` with corner `radii` in the local space.
    fn clip_rect(&mut self, rect: Rect, radii: [f64; 4]) {
        let device = self.state.device;
        let bounds = device.apply_rect(rect);
        let clip = &mut self.state.clip;
        clip[0] = clip[0].max(bounds.min_x());
        clip[1] = clip[1].max(bounds.min_y());
        clip[2] = clip[2].min(bounds.max_x());
        clip[3] = clip[3].min(bounds.max_y());
        let axis_aligned =
            (device.b == 0.0 && device.c == 0.0) || (device.a == 0.0 && device.d == 0.0);
        if axis_aligned && radii.iter().all(|radius| *radius <= 0.0) {
            return;
        }
        let Some(inverse) = device.invert() else {
            self.state.clip = [0.0; 4];
            return;
        };
        self.add_clip(ClipShape::Shape {
            rect: rect_words(rect),
            radii: radii.map(|v| v as f32),
            inverse: [inverse.a, inverse.b, inverse.c, inverse.d].map(|v| v as f32),
            offset: [inverse.tx as f32, inverse.ty as f32],
        });
    }

    fn clip_path(&mut self, path: &Path) {
        if let Some((rect, radii)) = path.as_rounded_rect() {
            self.clip_rect(rect, radii);
            return;
        }
        let polygons: Vec<Vec<Point>> = path
            .transformed(self.state.device)
            .flatten(TOLERANCE)
            .into_iter()
            .map(|(points, _)| points)
            .collect();
        let Some(bounds) = pixel_bounds(&polygons)
            .and_then(|bounds| intersect(bounds, round_out(self.state.clip)))
        else {
            self.state.clip = [0.0; 4];
            return;
        };
        // One region holds a clip, so it is at most one atlas wide.
        let limit = ATLAS_SIZE as i32;
        let bounds = [
            bounds[0],
            bounds[1],
            bounds[2].min(bounds[0] + limit),
            bounds[3].min(bounds[1] + limit),
        ];
        let clip = &mut self.state.clip;
        clip[0] = clip[0].max(f64::from(bounds[0]));
        clip[1] = clip[1].max(f64::from(bounds[1]));
        clip[2] = clip[2].min(f64::from(bounds[2]));
        clip[3] = clip[3].min(f64::from(bounds[3]));
        let id = self.next_clip;
        self.next_clip += 1;
        self.add_clip(ClipShape::Path {
            id,
            polygons: Rc::new(polygons),
            bounds,
        });
    }

    fn add_clip(&mut self, clip: ClipShape) {
        let mut clips = (*self.state.clips).clone();
        clips.push(clip);
        self.state.clips = Rc::new(clips);
        self.state.clip_list = Cell::new(None);
    }

    fn image(&mut self, image: &Image, rect: Rect, sampling: ImageSampling) {
        if rect.size.width <= 0.0 || rect.size.height <= 0.0 {
            return;
        }
        let margin = 1.0 / self.local_scale();
        let quad = rect.inset(-margin);
        let Some(bounds) = self.visible(quad) else {
            return;
        };
        let Some(texture) = self.image_texture(image) else {
            return;
        };
        let source = Source::Texture(self.texture_index(&texture));
        let mut instance = Instance::local(KIND_IMAGE, self.state.device);
        instance.bounds = rect_words(quad);
        instance.shape = rect_words(rect);
        if sampling == ImageSampling::Nearest {
            instance.info[1] = FLAG_NEAREST;
        }
        self.add(instance, source, None, bounds);
    }

    /// The texture of `image`, with premultiplied alpha.
    fn image_texture(&mut self, image: &Image) -> Option<Retained<MTLTexture>> {
        if let Some(texture) = self.caches.images.get(&image.id()) {
            return Some(texture.clone());
        }
        let (width, height) = (image.width() as usize, image.height() as usize);
        let texture = self.gpu.upload_texture(PIXEL_FORMAT_RGBA8, width, height)?;
        let pixels: Vec<u8> = image
            .pixels()
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|p| {
                let alpha = u16::from(p[3]);
                let scale = |c: u8| ((u16::from(c) * alpha + 127) / 255) as u8;
                [scale(p[0]), scale(p[1]), scale(p[2]), p[3]]
            })
            .collect();
        let region = MTLRegion {
            origin: MTLOrigin::new(0, 0),
            size: MTLSize::new(width, height),
        };
        if !replace_region(&texture, region, &pixels, width * 4, 4) {
            return None;
        }
        if self.caches.images.len() >= IMAGE_LIMIT {
            self.caches.images.clear();
        }
        self.caches.images.insert(image.id(), texture.clone());
        Some(texture)
    }

    fn shadow(&mut self, rect: Rect, radii: [f64; 4], shadow: Shadow) {
        let finite = shadow.offset.x.is_finite()
            && shadow.offset.y.is_finite()
            && shadow.blur.is_finite()
            && shadow.blur >= 0.0
            && shadow.spread.is_finite();
        if !finite || shadow.color.alpha == 0 {
            return;
        }
        let area = shadow.bounds(rect);
        let margin = 1.0 / self.local_scale();
        let quad = area.inset(-margin);
        let Some(bounds) = self.visible(quad) else {
            return;
        };
        let shape = rect
            .translate(shadow.offset.x, shadow.offset.y)
            .inset(if shadow.inset {
                shadow.spread
            } else {
                -shadow.spread
            });
        let shape_radii = radii.map(|radius| {
            if shadow.inset {
                (radius - shadow.spread).max(0.0)
            } else {
                (radius + shadow.spread).max(0.0)
            }
        });
        let sigma = (shadow.blur / 2.0).max(1e-6) * std::f64::consts::SQRT_2;
        let mut instance = Instance::local(KIND_SHADOW, self.state.device);
        instance.bounds = rect_words(quad);
        instance.shape = rect_words(rect);
        instance.radii = radii.map(|v| v as f32);
        instance.params2 = rect_words(shape);
        instance.params = shape_radii.map(|v| v as f32);
        instance.origin[2] = sigma as f32;
        instance.gradient = rect_words(area);
        instance.color = color_words(shadow.color);
        if shadow.inset {
            instance.info[1] = FLAG_INSET;
        }
        self.add(instance, Source::None, None, bounds);
    }

    /// Blurs the frame under `rect` and draws it inside the rounded
    /// rectangle, with the regions and the sigma of `choscorui-render`.
    fn backdrop(&mut self, rect: Rect, radii: [f64; 4], blur: f64) {
        let transform = self.state.transform;
        let scale = self.scale;
        let sigma = blur * transform.determinant().abs().sqrt() * scale;
        let device = transform.apply_rect(rect);
        let margin = if sigma.is_finite() {
            (sigma * 3.0).ceil()
        } else {
            0.0
        };
        let (width, height) = (self.plan.width as f64, self.plan.height as f64);
        let x0 = ((device.min_x() * scale).floor() - margin).clamp(0.0, width) as usize;
        let y0 = ((device.min_y() * scale).floor() - margin).clamp(0.0, height) as usize;
        let x1 = ((device.max_x() * scale).ceil() + margin).clamp(0.0, width) as usize;
        let y1 = ((device.max_y() * scale).ceil() + margin).clamp(0.0, height) as usize;
        if x1 <= x0 || y1 <= y0 {
            return;
        }
        let quad = rect.inset(-1.0 / self.local_scale());
        let Some(bounds) = self.visible(quad) else {
            return;
        };
        let index = self.plan.blurs.len() as u32;
        self.plan.blurs.push(BlurPass {
            origin: (x0, y0),
            size: (x1 - x0, y1 - y0),
            radii: (sigma.is_finite() && sigma > 0.0).then(|| blur_box_radii(sigma)),
        });
        self.plan.ops.push(Op::Blur(index));
        self.plan.reads_frame = true;
        let mut instance = Instance::local(KIND_BLUR, self.state.device);
        instance.bounds = rect_words(quad);
        instance.shape = rect_words(rect);
        instance.radii = radii.map(|v| v as f32);
        instance.params = [-(x0 as f32), -(y0 as f32), 0.0, 0.0];
        self.add(instance, Source::Blur(index), None, bounds);
    }

    /// Draws one glyph whose origin is at `pen` in the local space.
    fn glyph(&mut self, font: FontId, glyph: u16, pen: Point, color: Color) {
        let device = self.state.device;
        let luma = 0.2126 * f64::from(color.red)
            + 0.7152 * f64::from(color.green)
            + 0.0722 * f64::from(color.blue);
        let light = luma > 127.5;
        let upright = device.b == 0.0 && device.c == 0.0 && device.a > 0.0 && device.a == device.d;
        let (instance, bounds, color_glyph) = if upright {
            let point = device.apply(pen);
            let (x, sub_x) = split_pixel(point.x, SUBPIXELS[0]);
            let (y, sub_y) = split_pixel(point.y, SUBPIXELS[1]);
            let Some(entry) =
                self.caches
                    .text
                    .glyph(self.gpu, font, glyph, device.a, [sub_x, sub_y], light)
            else {
                return;
            };
            let [tx0, ty0, tx1, ty1] = entry.texels;
            let left = x + f64::from(entry.left);
            let top = y + f64::from(entry.top);
            let rect = Rect::new(
                Point::new(left, top),
                choscorui_core::Size::new(f64::from(tx1 - tx0), f64::from(ty1 - ty0)),
            );
            let mut instance = Instance::device(KIND_GLYPH);
            instance.bounds = rect_words(rect);
            instance.shape = rect_words(rect);
            instance.params = entry.texels.map(|v| v as f32);
            (instance, rect, entry.color)
        } else {
            let scale = self.local_scale();
            let Some(entry) = self
                .caches
                .text
                .glyph(self.gpu, font, glyph, scale, [0, 0], light)
            else {
                return;
            };
            let [tx0, ty0, tx1, ty1] = entry.texels;
            let rect = Rect::new(
                Point::new(
                    pen.x + f64::from(entry.left) / scale,
                    pen.y + f64::from(entry.top) / scale,
                ),
                choscorui_core::Size::new(
                    f64::from(tx1 - tx0) / scale,
                    f64::from(ty1 - ty0) / scale,
                ),
            );
            let mut instance = Instance::local(KIND_GLYPH, device);
            instance.bounds = rect_words(rect);
            instance.shape = rect_words(rect);
            instance.params = entry.texels.map(|v| v as f32);
            (instance, device.apply_rect(rect), entry.color)
        };
        let clip = self.state.clip;
        let visible = [
            bounds.min_x().max(clip[0]),
            bounds.min_y().max(clip[1]),
            bounds.max_x().min(clip[2]),
            bounds.max_y().min(clip[3]),
        ];
        if visible[0] >= visible[2] || visible[1] >= visible[3] {
            return;
        }
        let Some(texture) = self.caches.text.atlas(color_glyph).cloned() else {
            return;
        };
        let source = Source::Texture(self.texture_index(&texture));
        let mut instance = instance;
        if color_glyph {
            instance.info[0] = KIND_COLOR_GLYPH;
        }
        instance.color = color_words(color);
        self.add(instance, source, None, visible);
    }
}

/// True when an ellipse with the bounds `rect` is a circle, which the
/// rounded rectangle shader draws.
fn is_circle(rect: &Rect) -> bool {
    (rect.size.width - rect.size.height).abs() <= 1e-9 * rect.size.width.abs().max(1.0)
}

/// The radius of [`Path::rounded_rect`] for `rect` and `radius`.
fn clamp_radius(rect: Rect, radius: f64) -> f64 {
    radius
        .min(rect.size.width / 2.0)
        .min(rect.size.height / 2.0)
        .max(0.0)
}

/// Splits a device coordinate into a whole pixel and the nearest of
/// `steps` glyph offsets in that pixel.
fn split_pixel(value: f64, steps: u8) -> (f64, u8) {
    let whole = value.floor();
    let part = ((value - whole) * f64::from(steps)).round();
    if part >= f64::from(steps) {
        (whole + 1.0, 0)
    } else {
        (whole, part as u8)
    }
}

/// The whole pixels that a device box touches.
fn round_out(bounds: [f64; 4]) -> PixelBox {
    let clamp = |value: f64| value.clamp(-1.0e6, 1.0e6) as i32;
    [
        clamp(bounds[0].floor()),
        clamp(bounds[1].floor()),
        clamp(bounds[2].ceil()),
        clamp(bounds[3].ceil()),
    ]
}
