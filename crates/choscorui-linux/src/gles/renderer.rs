//! Paints a display list with OpenGL ES 3.0, command by command as the
//! Cairo painter does.
//!
//! - Rectangles under an axis-aligned transform have exact coverage in
//!   the shader. Cairo rasterizes the coverage of other paths and
//!   strokes into masks (`crate::paint::mask`), which the painter keeps
//!   while they repeat.
//! - Glyphs come from the atlas, rasterized by Cairo with the faces of
//!   the Cairo painter.
//! - A clip is a scissor rectangle. A clip that is not pixel-aligned also
//!   renders its coverage, times the outer clip, into a coverage texture.
//! - An opacity push draws into a layer texture, which the pop composites
//!   with the opacity.
//! - Backdrop blur reads the base target back and blurs it with the core
//!   blur, as the other painters do.

use super::atlas::{Atlas, Full, PHASES, RunKey, Slot};
use super::shaders;
use crate::ffi::gles::{
    Blend, Framebuffer, Gl, Program, Texture, TextureFormat, Uniform, Vertices, failed,
};
use crate::paint::glyphs;
use crate::paint::mask::{self, Mask, MaskFailure, Shape};
use choscorui_core::{
    Color, DisplayCommand, DisplayList, FillRule, GlyphRun, Image, ImageSampling, LineCap,
    LineJoin, Paint, Path, PathElement, Point, Rect, Shadow, StrokeStyle, TextRun, Transform,
    blur_pixels,
};
use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};

const MASK_LIMIT: usize = 512;
const IMAGE_LIMIT: usize = 256;
const SHADOW_LIMIT: usize = 128;
/// The side of the glyph atlas, when the GPU allows it.
const ATLAS_SIZE: u32 = 2048;
/// The entries of the gradient ramp.
const RAMP: u32 = 256;

/// Device pixels `[x0, y0, x1, y1]`, y down from the top edge.
pub(crate) type Area = [i32; 4];

/// An affine map `[a, b, c, d, tx, ty]`, as `Transform` names its fields.
type Matrix = [f64; 6];

const IDENTITY: Matrix = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

fn intersect(a: Area, b: Area) -> Area {
    [
        a[0].max(b[0]),
        a[1].max(b[1]),
        a[2].min(b[2]),
        a[3].min(b[3]),
    ]
}

fn is_empty(area: Area) -> bool {
    area[2] <= area[0] || area[3] <= area[1]
}

/// `m` after `t`: `t` maps first.
fn concat(m: Matrix, t: Transform) -> Matrix {
    let [a, b, c, d, tx, ty] = m;
    [
        a * t.a + c * t.b,
        b * t.a + d * t.b,
        a * t.c + c * t.d,
        b * t.c + d * t.d,
        a * t.tx + c * t.ty + tx,
        b * t.tx + d * t.ty + ty,
    ]
}

fn apply([a, b, c, d, tx, ty]: Matrix, x: f64, y: f64) -> (f64, f64) {
    (a * x + c * y + tx, b * x + d * y + ty)
}

fn inverse([a, b, c, d, tx, ty]: Matrix) -> Option<Matrix> {
    let det = a * d - b * c;
    if det == 0.0 || !det.is_finite() {
        return None;
    }
    Some([
        d / det,
        -b / det,
        -c / det,
        a / det,
        (c * ty - d * tx) / det,
        (b * tx - a * ty) / det,
    ])
}

fn axis_aligned(m: Matrix) -> bool {
    m[1] == 0.0 && m[2] == 0.0
}

/// The scale from user space to device pixels.
fn device_scale([a, b, c, d, _, _]: Matrix) -> f64 {
    (a * d - b * c).abs().sqrt()
}

/// The device box of `rect` under `m`, as `[x0, y0, x1, y1]`.
fn device_box(m: Matrix, rect: Rect) -> [f64; 4] {
    let corners = [
        apply(m, rect.min_x(), rect.min_y()),
        apply(m, rect.max_x(), rect.min_y()),
        apply(m, rect.min_x(), rect.max_y()),
        apply(m, rect.max_x(), rect.max_y()),
    ];
    corners.iter().fold(
        [
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        ],
        |[x0, y0, x1, y1], &(x, y)| [x0.min(x), y0.min(y), x1.max(x), y1.max(y)],
    )
}

/// The pixels that touch a device box.
fn round_out([x0, y0, x1, y1]: [f64; 4]) -> Area {
    // Clamp far outside any target, so the casts stay in range.
    let pixel = |value: f64| value.clamp(-1.0e7, 1.0e7) as i32;
    if !(x0 <= x1 && y0 <= y1) {
        return [0; 4];
    }
    [
        pixel(x0.floor()),
        pixel(y0.floor()),
        pixel(x1.ceil()),
        pixel(y1.ceil()),
    ]
}

fn premultiplied(color: Color) -> [f32; 4] {
    let alpha = f32::from(color.alpha) / 255.0;
    [
        f32::from(color.red) / 255.0 * alpha,
        f32::from(color.green) / 255.0 * alpha,
        f32::from(color.blue) / 255.0 * alpha,
        alpha,
    ]
}

/// The bytes of a premultiplied color.
fn premultiplied_bytes(color: Color) -> [u8; 4] {
    let a = u32::from(color.alpha);
    let mul = |c: u8| ((u32::from(c) * a + 127) / 255) as u8;
    [
        mul(color.red),
        mul(color.green),
        mul(color.blue),
        color.alpha,
    ]
}

/// Straight-alpha RGBA8 as premultiplied RGBA8.
fn premultiply(pixels: &[u8]) -> Vec<u8> {
    pixels
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|&[r, g, b, a]| premultiplied_bytes(Color::rgba8(r, g, b, a)))
        .collect()
}

/// A matrix as the column-major floats of a `mat3`.
fn mat3([a, b, c, d, tx, ty]: Matrix) -> [f32; 9] {
    [a, b, 0.0, c, d, 0.0, tx, ty, 1.0].map(|value| value as f32)
}

fn hash_f64(hasher: &mut DefaultHasher, values: &[f64]) {
    for value in values {
        value.to_bits().hash(hasher);
    }
}

fn hash_path(hasher: &mut DefaultHasher, path: &Path) {
    for element in path.elements() {
        match *element {
            PathElement::MoveTo(p) => {
                0u8.hash(hasher);
                hash_f64(hasher, &[p.x, p.y]);
            }
            PathElement::LineTo(p) => {
                1u8.hash(hasher);
                hash_f64(hasher, &[p.x, p.y]);
            }
            PathElement::QuadTo(c, p) => {
                2u8.hash(hasher);
                hash_f64(hasher, &[c.x, c.y, p.x, p.y]);
            }
            PathElement::CubicTo(a, b, p) => {
                3u8.hash(hasher);
                hash_f64(hasher, &[a.x, a.y, b.x, b.y, p.x, p.y]);
            }
            PathElement::Close => 4u8.hash(hasher),
        }
    }
}

/// The cache key of a fill or a stroke mask.
fn mask_key(shape: &Shape<'_>, matrix: Matrix, area: Area) -> Option<u64> {
    let mut hasher = DefaultHasher::new();
    match shape {
        Shape::Fill(path, rule) => {
            0u8.hash(&mut hasher);
            hash_path(&mut hasher, path);
            matches!(rule, FillRule::EvenOdd).hash(&mut hasher);
        }
        Shape::Stroke(path, style) => {
            1u8.hash(&mut hasher);
            hash_path(&mut hasher, path);
            hash_f64(&mut hasher, &[style.width]);
            hash_f64(&mut hasher, &style.dash);
            (style.cap as u8).hash(&mut hasher);
            (style.join as u8).hash(&mut hasher);
        }
        Shape::Run(..) => return None,
    }
    hash_f64(&mut hasher, &matrix);
    area.hash(&mut hasher);
    Some(hasher.finish())
}

/// A uniform of each name of the program.
struct Uniforms {
    target: Uniform,
    paint: Uniform,
    color: Uniform,
    inverse: Uniform,
    geometry: Uniform,
    coverage: Uniform,
    rect: Uniform,
    atlas: Uniform,
    clip: Uniform,
}

/// The paint of a draw.
#[derive(Clone, Copy)]
enum Fill {
    Solid([f32; 4]),
    /// The ramp holds the stops; `geometry` is the start and the end, or
    /// the center and the radius.
    Gradient {
        linear: bool,
        geometry: [f32; 4],
        inverse: Matrix,
    },
    /// `rect` is the image rectangle in the space that `inverse` maps
    /// device pixels to.
    Image {
        texture: Texture,
        linear: bool,
        rect: [f32; 4],
        inverse: Matrix,
        opacity: f32,
    },
    Layer {
        texture: Texture,
        opacity: f32,
    },
}

/// The coverage of a draw.
#[derive(Clone, Copy)]
enum Cover {
    Whole,
    /// A device box with fractional edges.
    Rect([f64; 4]),
    /// A mask texture at `[x, y, width, height]`.
    Mask(Texture, Area),
    /// Glyph quads with atlas coordinates.
    Atlas,
}

/// A cached mask: its texture and its pixels, or `None` without
/// coverage.
struct CachedMask {
    texture: Option<(Texture, Area)>,
    frame: u64,
}

/// A clip: the device pixels it may touch, and the depth of its coverage
/// texture.
#[derive(Clone, Copy)]
struct Clip {
    area: Area,
    mask: Option<usize>,
}

/// What a push saved.
enum Saved {
    State {
        matrix: Matrix,
        clip: Clip,
    },
    Group {
        matrix: Matrix,
        clip: Clip,
        opacity: f64,
        parent: Option<Framebuffer>,
    },
}

/// Where a paint draws: a framebuffer, or the window surface.
#[derive(Clone, Copy)]
pub(crate) struct Target {
    pub(crate) framebuffer: Option<Framebuffer>,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

/// Why a paint was not complete.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PaintIssue {
    /// A text command was not drawn, because its face did not open.
    Text,
    /// OpenGL ES reported an error.
    Gl(u32),
}

pub(crate) struct Renderer {
    gl: Gl,
    program: Program,
    uniforms: Uniforms,
    quads: Vertices,
    ramp: Texture,
    atlas: Atlas,
    masks: HashMap<u64, CachedMask>,
    images: HashMap<u64, (Texture, u64)>,
    shadows: HashMap<[u64; 13], (Texture, u64)>,
    /// Layer and clip textures of the target size, by depth.
    layers: Vec<(Texture, Framebuffer)>,
    clips: Vec<(Texture, Framebuffer)>,
    pool_size: (u32, u32),
    frame: u64,
    max_texture: u32,
}

impl Renderer {
    /// Compiles the program and makes the shared resources. The context
    /// must be current.
    pub(crate) fn new(gl: Gl) -> Result<Self, String> {
        let program = gl.program(
            shaders::VERTEX,
            shaders::FRAGMENT,
            &[c"a_position", c"a_uv"],
        )?;
        let uniform = |name| gl.uniform(program, name);
        let uniforms = Uniforms {
            target: uniform(c"u_target"),
            paint: uniform(c"u_paint"),
            color: uniform(c"u_color"),
            inverse: uniform(c"u_inverse"),
            geometry: uniform(c"u_geometry"),
            coverage: uniform(c"u_coverage"),
            rect: uniform(c"u_rect"),
            atlas: uniform(c"u_atlas"),
            clip: uniform(c"u_clip"),
        };
        gl.use_program(program);
        gl.set_int(uniform(c"u_source"), 0);
        gl.set_int(uniform(c"u_mask"), 1);
        gl.set_int(uniform(c"u_clip_mask"), 2);
        let quads = gl
            .vertices(&[2, 2])
            .ok_or("the vertex array was not created")?;
        let ramp = gl
            .texture(TextureFormat::Rgba8, RAMP, 1, None, true)
            .ok_or("the gradient ramp was not created")?;
        let max_texture = gl.max_texture_size();
        let atlas = Atlas::new(&gl, ATLAS_SIZE.min(max_texture))
            .ok_or("the glyph atlas was not created")?;
        let error = gl.error();
        if failed(error) {
            return Err(format!("OpenGL ES error 0x{error:04X} at setup"));
        }
        Ok(Self {
            gl,
            program,
            uniforms,
            quads,
            ramp,
            atlas,
            masks: HashMap::new(),
            images: HashMap::new(),
            shadows: HashMap::new(),
            layers: Vec::new(),
            clips: Vec::new(),
            pool_size: (0, 0),
            frame: 0,
            max_texture,
        })
    }

    /// The largest target side.
    pub(crate) fn max_texture(&self) -> u32 {
        self.max_texture
    }

    pub(crate) fn gl(&self) -> Gl {
        self.gl
    }

    /// Paints `list` at `scale` into `target`, inside `area` (the whole
    /// target for `None`). The pixels of the area are cleared first. The
    /// context must be current.
    pub(crate) fn paint(
        &mut self,
        list: &DisplayList,
        target: Target,
        scale: f64,
        area: Option<Area>,
    ) -> Result<(), PaintIssue> {
        let gl = self.gl;
        // Errors of earlier calls are not this paint's.
        while failed(gl.error()) {}
        self.frame += 1;
        if self.pool_size != (target.width, target.height) {
            self.drop_pools();
            self.pool_size = (target.width, target.height);
        }
        let full = [
            0,
            0,
            i32::try_from(target.width).unwrap_or(i32::MAX),
            i32::try_from(target.height).unwrap_or(i32::MAX),
        ];
        let area = intersect(area.unwrap_or(full), full);
        gl.bind_framebuffer(target.framebuffer);
        gl.viewport(target.width, target.height);
        gl.use_program(self.program);
        gl.set_vec2(
            self.uniforms.target,
            [target.width as f32, target.height as f32],
        );
        let mut painter = Painter {
            r: self,
            target,
            full,
            current: target.framebuffer,
            matrix: [scale, 0.0, 0.0, scale, 0.0, 0.0],
            clip: Clip { area, mask: None },
            stack: Vec::new(),
            layer_depth: 0,
            text_failed: false,
        };
        if !is_empty(area) {
            painter.clear_area(area);
            for command in list.commands() {
                painter.execute(command);
            }
            painter.unwind();
        }
        let text_failed = painter.text_failed;
        gl.scissor(None);
        self.evict();
        let error = gl.error();
        if failed(error) {
            Err(PaintIssue::Gl(error))
        } else if text_failed {
            Err(PaintIssue::Text)
        } else {
            Ok(())
        }
    }

    fn drop_pools(&mut self) {
        for (texture, framebuffer) in self.layers.drain(..).chain(self.clips.drain(..)) {
            self.gl.delete_framebuffer(framebuffer);
            self.gl.delete_texture(texture);
        }
    }

    /// Deletes the cached textures that the last frames did not use, when
    /// a cache is over its limit.
    fn evict(&mut self) {
        let (gl, frame) = (self.gl, self.frame);
        if self.masks.len() > MASK_LIMIT {
            self.masks.retain(|_, mask| {
                let keep = mask.frame == frame;
                if !keep && let Some((texture, _)) = mask.texture {
                    gl.delete_texture(texture);
                }
                keep
            });
        }
        let keep = |(texture, used): &mut (Texture, u64)| {
            let keep = *used == frame;
            if !keep {
                gl.delete_texture(*texture);
            }
            keep
        };
        if self.images.len() > IMAGE_LIMIT {
            self.images.retain(|_, entry| keep(entry));
        }
        if self.shadows.len() > SHADOW_LIMIT {
            self.shadows.retain(|_, entry| keep(entry));
        }
    }

    /// A texture of the target size and its framebuffer, from `pool` at
    /// `depth`.
    fn pooled(&mut self, layers: bool, depth: usize) -> Option<(Texture, Framebuffer)> {
        let (width, height) = self.pool_size;
        let gl = self.gl;
        let format = if layers {
            TextureFormat::Rgba8
        } else {
            TextureFormat::R8
        };
        let pool = if layers {
            &mut self.layers
        } else {
            &mut self.clips
        };
        while pool.len() <= depth {
            let texture = gl.texture(format, width, height, None, false)?;
            let Some(framebuffer) = gl.framebuffer(texture) else {
                gl.delete_texture(texture);
                return None;
            };
            pool.push((texture, framebuffer));
        }
        Some(pool[depth])
    }
}

/// The state of one paint.
struct Painter<'r> {
    r: &'r mut Renderer,
    target: Target,
    full: Area,
    /// The framebuffer that draws go to: the target or a layer.
    current: Option<Framebuffer>,
    matrix: Matrix,
    clip: Clip,
    stack: Vec<Saved>,
    layer_depth: usize,
    text_failed: bool,
}

impl Painter<'_> {
    /// The scissor box of `area`, with y up from the bottom edge.
    fn scissor(&self, [x0, y0, x1, y1]: Area) -> [i32; 4] {
        let height = self.full[3];
        [x0, height - y1, x1 - x0, y1 - y0]
    }

    fn clear_area(&self, area: Area) {
        let gl = self.r.gl;
        gl.scissor(Some(self.scissor(area)));
        gl.clear();
        gl.blend(Blend::Over);
    }

    /// Draws `quad`, inside the clip, with `fill` and `cover`. Atlas
    /// draws pass their glyph vertices.
    fn draw(&mut self, quad: Area, fill: Fill, cover: Cover, glyphs: Option<&[f32]>) {
        let area = intersect(quad, self.clip.area);
        if is_empty(area) {
            return;
        }
        let (gl, u) = (self.r.gl, &self.r.uniforms);
        gl.scissor(Some(self.scissor(area)));
        let mut source = None;
        match fill {
            Fill::Solid(color) => {
                gl.set_int(u.paint, 0);
                gl.set_vec4(u.color, color);
            }
            Fill::Gradient {
                linear,
                geometry,
                inverse,
            } => {
                gl.set_int(u.paint, if linear { 1 } else { 2 });
                gl.set_vec4(u.geometry, geometry);
                gl.set_mat3(u.inverse, &mat3(inverse));
                source = Some(self.r.ramp);
            }
            Fill::Image {
                texture,
                linear,
                rect,
                inverse,
                opacity,
            } => {
                gl.set_filter(texture, linear);
                gl.set_int(u.paint, 3);
                gl.set_vec4(u.color, [0.0, 0.0, 0.0, opacity]);
                gl.set_vec4(u.geometry, rect);
                gl.set_mat3(u.inverse, &mat3(inverse));
                source = Some(texture);
            }
            Fill::Layer { texture, opacity } => {
                gl.set_int(u.paint, 4);
                gl.set_vec4(u.color, [0.0, 0.0, 0.0, opacity]);
                source = Some(texture);
            }
        }
        let mut mask = None;
        match cover {
            Cover::Whole => gl.set_int(u.coverage, 0),
            Cover::Rect(rect) => {
                gl.set_int(u.coverage, 1);
                gl.set_vec4(u.rect, rect.map(|value| value as f32));
            }
            Cover::Mask(texture, [x, y, width, height]) => {
                gl.set_int(u.coverage, 2);
                gl.set_vec4(u.rect, [x, y, width, height].map(|value| value as f32));
                mask = Some(texture);
            }
            Cover::Atlas => {
                gl.set_int(u.coverage, 3);
                let size = self.r.atlas.size as f32;
                gl.set_vec2(u.atlas, [size, size]);
                mask = Some(self.r.atlas.texture);
            }
        }
        let clip = self.clip.mask.map(|depth| self.r.clips[depth].0);
        gl.set_int(u.clip, i32::from(clip.is_some()));
        gl.bind_texture(0, source);
        gl.bind_texture(1, mask);
        gl.bind_texture(2, clip);
        match glyphs {
            Some(vertices) => gl.draw_triangles(self.r.quads, vertices),
            None => {
                let [x0, y0, x1, y1] = area.map(|value| value as f32);
                let vertices = [
                    x0, y0, 0.0, 0.0, x1, y0, 0.0, 0.0, x0, y1, 0.0, 0.0, x0, y1, 0.0, 0.0, x1, y0,
                    0.0, 0.0, x1, y1, 0.0, 0.0,
                ];
                gl.draw_triangles(self.r.quads, &vertices);
            }
        }
    }

    fn execute(&mut self, command: &DisplayCommand) {
        match command {
            DisplayCommand::FillRect { rect, color } => {
                self.fill_rect(*rect, Fill::Solid(premultiplied(*color)));
            }
            DisplayCommand::StrokeRect { rect, color, width } => {
                let style = StrokeStyle {
                    width: *width,
                    cap: LineCap::Butt,
                    join: LineJoin::Miter,
                    dash: Vec::new(),
                };
                self.stroke(
                    &Path::rect(*rect),
                    &style,
                    Fill::Solid(premultiplied(*color)),
                );
            }
            DisplayCommand::FillRoundedRect {
                rect,
                radius,
                color,
            } => {
                let path = Path::rounded_rect(*rect, *radius);
                self.fill(&path, FillRule::NonZero, Fill::Solid(premultiplied(*color)));
            }
            DisplayCommand::StrokeRoundedRect {
                rect,
                radius,
                color,
                width,
            } => {
                let style = StrokeStyle {
                    width: *width,
                    cap: LineCap::Butt,
                    join: LineJoin::Miter,
                    dash: Vec::new(),
                };
                let path = Path::rounded_rect(*rect, *radius);
                self.stroke(&path, &style, Fill::Solid(premultiplied(*color)));
            }
            DisplayCommand::FillPath { path, paint, rule } => {
                if let Some(fill) = self.paint(paint) {
                    self.fill(path, *rule, fill);
                }
            }
            DisplayCommand::StrokePath { path, paint, style } => {
                if let Some(fill) = self.paint(paint) {
                    self.stroke(path, style, fill);
                }
            }
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
                self.backdrop_blur(*rect, [*radius; 4], *blur);
            }
            DisplayCommand::BackdropBlurCorners { rect, radii, blur } => {
                self.backdrop_blur(*rect, *radii, *blur);
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
                let run = TextRun {
                    family: font_family,
                    size: *font_size,
                    weight: *font_weight,
                    slant: *font_slant,
                    tracking: *tracking,
                };
                match glyphs::text_runs(*baseline, text, &run) {
                    Some(runs) => {
                        for (origin, glyph_run) in &runs {
                            self.glyph_run(*origin, glyph_run, *color);
                        }
                    }
                    None => self.text_failed = true,
                }
            }
            DisplayCommand::DrawGlyphs { origin, run, color } => {
                self.glyph_run(*origin, run, *color);
            }
            DisplayCommand::PushClip(rect) => self.push_clip_rect(*rect),
            DisplayCommand::PushClipPath(path) => {
                self.save();
                self.push_clip_path(path);
            }
            DisplayCommand::PushOpacity(opacity) => self.push_opacity(*opacity),
            DisplayCommand::PushTransform(transform) => {
                self.save();
                self.matrix = concat(self.matrix, *transform);
            }
            DisplayCommand::PopClip | DisplayCommand::PopOpacity | DisplayCommand::PopTransform => {
                self.pop();
            }
        }
    }

    /// The fill of `paint`, or `None` for a gradient without stops or a
    /// singular transform.
    fn paint(&mut self, paint: &Paint) -> Option<Fill> {
        // The ramp samples the gradient along a line from `from`, where
        // `t` is 0, by `step` for each unit of `t`.
        let (linear, geometry, from, step) = match paint {
            Paint::Solid(color) => return Some(Fill::Solid(premultiplied(*color))),
            Paint::LinearGradient { start, end, stops } => {
                if stops.is_empty() {
                    return None;
                }
                (
                    true,
                    [start.x, start.y, end.x, end.y],
                    *start,
                    (end.x - start.x, end.y - start.y),
                )
            }
            Paint::RadialGradient {
                center,
                radius,
                stops,
            } => {
                if stops.is_empty() {
                    return None;
                }
                (
                    false,
                    [center.x, center.y, *radius, 0.0],
                    *center,
                    (*radius, 0.0),
                )
            }
        };
        let inverse = inverse(self.matrix)?;
        let ramp: Vec<u8> = (0..RAMP)
            .flat_map(|index| {
                let t = f64::from(index) / f64::from(RAMP - 1);
                let point = Point::new(from.x + step.0 * t, from.y + step.1 * t);
                premultiplied_bytes(paint.color_at(point))
            })
            .collect();
        self.r
            .gl
            .update_texture(self.r.ramp, TextureFormat::Rgba8, [0, 0, RAMP, 1], &ramp);
        Some(Fill::Gradient {
            linear,
            geometry: geometry.map(|value| value as f32),
            inverse,
        })
    }

    fn fill_rect(&mut self, rect: Rect, fill: Fill) {
        if axis_aligned(self.matrix) {
            let device = device_box(self.matrix, rect);
            self.draw(round_out(device), fill, Cover::Rect(device), None);
        } else {
            self.fill(&Path::rect(rect), FillRule::NonZero, fill);
        }
    }

    fn fill(&mut self, path: &Path, rule: FillRule, fill: Fill) {
        let area = intersect(round_out(device_box(self.matrix, path.bounds())), self.full);
        self.masked(&Shape::Fill(path, rule), area, fill);
    }

    fn stroke(&mut self, path: &Path, style: &StrokeStyle, fill: Fill) {
        let reach = mask::stroke_reach(style) + 1.0;
        let bounds = path.bounds().inset(-reach);
        let area = intersect(round_out(device_box(self.matrix, bounds)), self.full);
        self.masked(&Shape::Stroke(path, style), area, fill);
    }

    /// Draws `fill` through the Cairo coverage of `shape` in `area`.
    fn masked(&mut self, shape: &Shape<'_>, area: Area, fill: Fill) {
        if is_empty(intersect(area, self.clip.area)) {
            return;
        }
        if let Some((texture, mask_area)) = self.mask(shape, area) {
            self.draw(area, fill, Cover::Mask(texture, mask_area), None);
        }
    }

    /// The cached coverage texture of `shape` in `area`, and its pixels.
    fn mask(&mut self, shape: &Shape<'_>, area: Area) -> Option<(Texture, Area)> {
        let key = mask_key(shape, self.matrix, area)?;
        let frame = self.r.frame;
        if let Some(cached) = self.r.masks.get_mut(&key) {
            cached.frame = frame;
            return cached.texture;
        }
        let texture = match mask::rasterize(shape, self.matrix, area) {
            Ok(mask) => self.upload_mask(&mask),
            Err(MaskFailure::Text) => {
                self.text_failed = true;
                return None;
            }
            Err(MaskFailure::Cairo) => return None,
        };
        self.r.masks.insert(key, CachedMask { texture, frame });
        texture
    }

    fn upload_mask(&self, mask: &Mask) -> Option<(Texture, Area)> {
        let texture = self.r.gl.texture(
            TextureFormat::R8,
            mask.width,
            mask.height,
            Some(&mask.coverage),
            false,
        )?;
        Some((
            texture,
            [
                mask.x,
                mask.y,
                i32::try_from(mask.width).ok()?,
                i32::try_from(mask.height).ok()?,
            ],
        ))
    }

    /// The texture of `image`, premultiplied.
    fn image_texture(&mut self, image: &Image) -> Option<Texture> {
        let frame = self.r.frame;
        if let Some((texture, used)) = self.r.images.get_mut(&image.id()) {
            *used = frame;
            return Some(*texture);
        }
        if image.width() > self.r.max_texture || image.height() > self.r.max_texture {
            return None;
        }
        let pixels = premultiply(image.pixels());
        let texture = self.r.gl.texture(
            TextureFormat::Rgba8,
            image.width(),
            image.height(),
            Some(&pixels),
            true,
        )?;
        self.r.images.insert(image.id(), (texture, frame));
        Some(texture)
    }

    /// Draws `texture` scaled to fill `rect`, as the Cairo painter draws
    /// an image: edge pixels repeat outside it.
    fn textured(&mut self, texture: Texture, rect: Rect, linear: bool) {
        let Some(inverse) = inverse(self.matrix) else {
            return;
        };
        let fill = Fill::Image {
            texture,
            linear,
            rect: [
                rect.min_x(),
                rect.min_y(),
                rect.size.width,
                rect.size.height,
            ]
            .map(|value| value as f32),
            inverse,
            opacity: 1.0,
        };
        self.fill_rect(rect, fill);
    }

    fn image(&mut self, image: &Image, rect: Rect, sampling: ImageSampling) {
        if rect.size.width <= 0.0 || rect.size.height <= 0.0 {
            return;
        }
        if let Some(texture) = self.image_texture(image) {
            self.textured(texture, rect, matches!(sampling, ImageSampling::Linear));
        }
    }

    /// Draws the core shadow mask of `rect` like an image, at the device
    /// resolution.
    fn shadow(&mut self, rect: Rect, radii: [f64; 4], shadow: Shadow) {
        let scale = device_scale(self.matrix).max(1.0);
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
        let frame = self.r.frame;
        let texture = if let Some((texture, used)) = self.r.shadows.get_mut(&key) {
            *used = frame;
            *texture
        } else {
            let Some(mask) = shadow.mask_corners(rect, radii, scale) else {
                return;
            };
            if mask.width() > self.r.max_texture || mask.height() > self.r.max_texture {
                return;
            }
            let pixels = premultiply(mask.pixels());
            let Some(texture) = self.r.gl.texture(
                TextureFormat::Rgba8,
                mask.width(),
                mask.height(),
                Some(&pixels),
                true,
            ) else {
                return;
            };
            self.r.shadows.insert(key, (texture, frame));
            texture
        };
        self.textured(texture, shadow.bounds(rect), true);
    }

    /// Blurs the target under `rect`, rounded with `radii`, with a
    /// Gaussian of standard deviation `blur` points. It reads the target
    /// beneath any open layer, as the Cairo painter reads its base
    /// surface.
    fn backdrop_blur(&mut self, rect: Rect, radii: [f64; 4], blur: f64) {
        let sigma = blur * device_scale(self.matrix);
        let margin = if sigma.is_finite() {
            (sigma * 3.0).ceil().clamp(0.0, 1.0e6) as i32
        } else {
            0
        };
        let [x0, y0, x1, y1] = round_out(device_box(self.matrix, rect));
        let region = intersect(
            [x0 - margin, y0 - margin, x1 + margin, y1 + margin],
            self.full,
        );
        if is_empty(region) || is_empty(intersect(region, self.clip.area)) {
            return;
        }
        let gl = self.r.gl;
        let (columns, rows) = (
            (region[2] - region[0]) as usize,
            (region[3] - region[1]) as usize,
        );
        let mut pixels = Vec::new();
        gl.bind_framebuffer(self.target.framebuffer);
        let [x, y, _, _] = self.scissor(region);
        gl.read_pixels(
            [x as u32, y as u32, columns as u32, rows as u32],
            &mut pixels,
        );
        gl.bind_framebuffer(self.current);
        if pixels.len() != columns * rows * 4 {
            return;
        }
        // The rows come from the bottom up.
        let flipped: Vec<u8> = pixels
            .chunks_exact(columns * 4)
            .rev()
            .flatten()
            .copied()
            .collect();
        let mut pixels = flipped;
        blur_pixels(&mut pixels, columns, rows, columns * 4, sigma);
        let Some(texture) = gl.texture(
            TextureFormat::Rgba8,
            columns as u32,
            rows as u32,
            Some(&pixels),
            false,
        ) else {
            return;
        };
        let path = Path::rounded_rect_corners(rect, radii);
        let area = intersect(round_out(device_box(self.matrix, rect)), self.full);
        if let Some((mask, mask_area)) = self.mask(&Shape::Fill(&path, FillRule::NonZero), area) {
            let fill = Fill::Image {
                texture,
                linear: false,
                rect: [
                    region[0],
                    region[1],
                    region[2] - region[0],
                    region[3] - region[1],
                ]
                .map(|value| value as f32),
                inverse: IDENTITY,
                opacity: 1.0,
            };
            self.draw(area, fill, Cover::Mask(mask, mask_area), None);
        }
        // The draw keeps its source until it runs.
        gl.delete_texture(texture);
    }

    /// Draws a glyph run with its baseline origin at `origin`, from the
    /// atlas.
    fn glyph_run(&mut self, origin: Point, run: &GlyphRun, color: Color) {
        if run.glyphs.is_empty() {
            return;
        }
        let fill = Fill::Solid(premultiplied(color));
        let [a, b, c, d, _, _] = self.matrix;
        let em = run.size * a.hypot(b).max(c.hypot(d));
        if !em.is_finite() || em * 4.0 + 8.0 > f64::from(self.r.atlas.size) / 2.0 {
            self.large_run(origin, run, fill);
            return;
        }
        let key = RunKey {
            font: run.font.clone(),
            size: run.size.to_bits(),
            matrix: [a, b, c, d].map(f64::to_bits),
        };
        // The canvas that a glyph is rasterized on, and its origin pixel.
        let pad = (em.ceil() as i32 + 2, (em * 2.0).ceil() as i32 + 2);
        let canvas = [
            0,
            0,
            (em * 4.0).ceil() as i32 + 4,
            (em * 3.0).ceil() as i32 + 4,
        ];
        let mut batch = GlyphBatch::new(fill);
        for (&glyph, position) in run.glyphs.iter().zip(&run.positions) {
            let (x, y) = apply(self.matrix, origin.x + position.x, origin.y + position.y);
            if !(x.is_finite() && y.is_finite()) {
                continue;
            }
            let phase = |value: f64| ((value - value.floor()) * PHASES).floor().min(PHASES - 1.0);
            let (px, py) = (phase(x), phase(y));
            let glyph_key = (glyph, px as u8, py as u8);
            let slot = match self.r.atlas.get(&key, glyph_key) {
                Some(slot) => slot,
                None => {
                    // Rasterize at the middle of the phase, so Cairo rounds
                    // the position as it rounds every position of the phase.
                    let at = (
                        f64::from(pad.0) + (px + 0.5) / PHASES,
                        f64::from(pad.1) + (py + 0.5) / PHASES,
                    );
                    let single = GlyphRun {
                        glyphs: vec![glyph],
                        positions: vec![Point::new(0.0, 0.0)],
                        ..run.clone()
                    };
                    let mask = match mask::rasterize(
                        &Shape::Run(Point::new(0.0, 0.0), &single),
                        [a, b, c, d, at.0, at.1],
                        canvas,
                    ) {
                        Ok(mask) => mask.trimmed(),
                        Err(MaskFailure::Text) => {
                            self.text_failed = true;
                            return;
                        }
                        Err(MaskFailure::Cairo) => continue,
                    };
                    match self.insert_glyph(&key, glyph_key, mask, pad, &mut batch) {
                        Some(slot) => slot,
                        None => continue,
                    }
                }
            };
            if let Some(slot) = slot {
                batch.push(slot, x.floor(), y.floor());
            }
        }
        self.flush_glyphs(&mut batch);
    }

    /// Adds a glyph to the atlas. A full atlas draws the pending glyphs
    /// and starts over. `None` is a glyph larger than the atlas.
    fn insert_glyph(
        &mut self,
        key: &RunKey,
        glyph: (u16, u8, u8),
        mask: Option<Mask>,
        pad: (i32, i32),
        batch: &mut GlyphBatch,
    ) -> Option<Option<Slot>> {
        let gl = self.r.gl;
        let Err(Full(mask)) = self.r.atlas.insert(&gl, key, glyph, mask, pad) else {
            return self.r.atlas.get(key, glyph);
        };
        self.flush_glyphs(batch);
        self.r.atlas.clear();
        self.r.atlas.insert(&gl, key, glyph, Some(mask), pad).ok()
    }

    fn flush_glyphs(&mut self, batch: &mut GlyphBatch) {
        if !batch.vertices.is_empty() {
            self.draw(
                batch.bounds,
                batch.fill,
                Cover::Atlas,
                Some(&batch.vertices),
            );
        }
        *batch = GlyphBatch::new(batch.fill);
    }

    /// Draws a run too large for the atlas through one Cairo mask.
    fn large_run(&mut self, origin: Point, run: &GlyphRun, fill: Fill) {
        // Ink may reach past the typographic box.
        let bounds = run.bounds.translate(origin.x, origin.y).inset(-run.size);
        let area = intersect(round_out(device_box(self.matrix, bounds)), self.full);
        if is_empty(intersect(area, self.clip.area)) {
            return;
        }
        match mask::rasterize(&Shape::Run(origin, run), self.matrix, area) {
            Ok(mask) => {
                if let Some((texture, mask_area)) = self.upload_mask(&mask) {
                    self.draw(area, fill, Cover::Mask(texture, mask_area), None);
                    self.r.gl.delete_texture(texture);
                }
            }
            Err(MaskFailure::Text) => self.text_failed = true,
            Err(MaskFailure::Cairo) => {}
        }
    }

    fn save(&mut self) {
        self.stack.push(Saved::State {
            matrix: self.matrix,
            clip: self.clip,
        });
    }

    fn push_clip_rect(&mut self, rect: Rect) {
        self.save();
        if !axis_aligned(self.matrix) {
            self.push_clip_path(&Path::rect(rect));
            return;
        }
        let device = device_box(self.matrix, rect);
        let whole = device
            .iter()
            .all(|value| (value - value.round()).abs() < 1.0e-6);
        if whole {
            self.clip.area = intersect(self.clip.area, round_out(device));
        } else {
            self.mask_clip(round_out(device), Cover::Rect(device));
        }
    }

    fn push_clip_path(&mut self, path: &Path) {
        let area = intersect(round_out(device_box(self.matrix, path.bounds())), self.full);
        if is_empty(intersect(area, self.clip.area)) {
            self.clip = Clip {
                area: [0; 4],
                mask: None,
            };
            return;
        }
        match self.mask(&Shape::Fill(path, FillRule::NonZero), area) {
            Some((texture, mask_area)) => self.mask_clip(area, Cover::Mask(texture, mask_area)),
            // Nothing is covered.
            None => {
                self.clip = Clip {
                    area: [0; 4],
                    mask: None,
                }
            }
        }
    }

    /// Narrows the clip to `cover` inside `area`: renders the coverage,
    /// times the outer clip, into the next clip texture.
    fn mask_clip(&mut self, area: Area, cover: Cover) {
        let inner = intersect(area, self.clip.area);
        if is_empty(inner) {
            self.clip = Clip {
                area: [0; 4],
                mask: None,
            };
            return;
        }
        let depth = self.clip.mask.map_or(0, |depth| depth + 1);
        let Some((_, framebuffer)) = self.r.pooled(false, depth) else {
            // Without a texture, the clip keeps its box.
            self.clip.area = inner;
            return;
        };
        let gl = self.r.gl;
        gl.bind_framebuffer(Some(framebuffer));
        gl.blend(Blend::Replace);
        self.draw(inner, Fill::Solid([1.0; 4]), cover, None);
        gl.blend(Blend::Over);
        gl.bind_framebuffer(self.current);
        self.clip = Clip {
            area: inner,
            mask: Some(depth),
        };
    }

    fn push_opacity(&mut self, opacity: f64) {
        self.stack.push(Saved::Group {
            matrix: self.matrix,
            clip: self.clip,
            opacity,
            parent: self.current,
        });
        match self.r.pooled(true, self.layer_depth) {
            Some((_, framebuffer)) => {
                self.layer_depth += 1;
                self.current = Some(framebuffer);
                self.r.gl.bind_framebuffer(self.current);
                if !is_empty(self.clip.area) {
                    self.r.gl.blend(Blend::Replace);
                    self.clear_area(self.clip.area);
                }
            }
            // Without a layer, the group draws nothing.
            None => {
                self.current = None;
                self.clip.area = [0; 4];
            }
        }
    }

    fn pop(&mut self) {
        match self.stack.pop() {
            Some(Saved::State { matrix, clip }) => {
                self.matrix = matrix;
                self.clip = clip;
            }
            Some(Saved::Group {
                matrix,
                clip,
                opacity,
                parent,
            }) => {
                let layered = self.current.is_some() && self.current != parent;
                self.matrix = matrix;
                self.clip = clip;
                self.current = parent;
                self.r.gl.bind_framebuffer(parent);
                if layered {
                    self.layer_depth -= 1;
                    let texture = self.r.layers[self.layer_depth].0;
                    let fill = Fill::Layer {
                        texture,
                        opacity: opacity.clamp(0.0, 1.0) as f32,
                    };
                    self.draw(clip.area, fill, Cover::Whole, None);
                }
            }
            None => {}
        }
    }

    /// Closes every push that the list left open.
    fn unwind(&mut self) {
        while !self.stack.is_empty() {
            self.pop();
        }
    }
}

/// The glyph quads of a run that wait for one draw.
struct GlyphBatch {
    fill: Fill,
    vertices: Vec<f32>,
    bounds: Area,
}

impl GlyphBatch {
    fn new(fill: Fill) -> Self {
        Self {
            fill,
            vertices: Vec::new(),
            bounds: [i32::MAX, i32::MAX, i32::MIN, i32::MIN],
        }
    }

    /// Adds the two triangles of a glyph whose origin pixel is `(x, y)`.
    fn push(&mut self, slot: Slot, x: f64, y: f64) {
        let left = (x as i32).saturating_add(slot.left);
        let top = (y as i32).saturating_add(slot.top);
        let (width, height) = (slot.width as i32, slot.height as i32);
        let bounds = &mut self.bounds;
        *bounds = [
            bounds[0].min(left),
            bounds[1].min(top),
            bounds[2].max(left + width),
            bounds[3].max(top + height),
        ];
        let [x0, y0, x1, y1] = [left, top, left + width, top + height].map(|value| value as f32);
        let [u0, v0] = [slot.x as f32, slot.y as f32];
        let [u1, v1] = [u0 + width as f32, v0 + height as f32];
        self.vertices.extend_from_slice(&[
            x0, y0, u0, v0, x1, y0, u1, v0, x0, y1, u0, v1, x0, y1, u0, v1, x1, y0, u1, v0, x1, y1,
            u1, v1,
        ]);
    }
}
