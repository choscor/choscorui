//! Records a [`Plan`] into a command buffer: the coverage passes of the
//! first atlases, then the frame, its opacity groups, its blurs, and the
//! coverage passes of later atlases in list order.

use super::coverage::{ATLAS_SIZE, EDGE_WORDS, PixelBox};
use super::plan::{ATLAS_LIMIT, BlurPass, Op, Plan, Source, Target};
use super::{COLOR_FORMAT, Gpu};
use crate::ffi::Obj;
use crate::ffi::metal::{
    BLIT_ENCODER, COPY_TEXTURE, DRAW_PRIMITIVES, END_BLIT_ENCODING, END_RENDER_ENCODING,
    LOAD_ACTION_CLEAR, LOAD_ACTION_DONT_CARE, LOAD_ACTION_LOAD, MTLBuffer, MTLClearColor,
    MTLCommandBuffer, MTLOrigin, MTLRenderCommandEncoder, MTLScissorRect, MTLSize, MTLTexture,
    PASS_ATTACHMENT_AT, PASS_COLOR_ATTACHMENTS, PIXEL_FORMAT_R16_FLOAT, PIXEL_FORMAT_RGBA8,
    PRIMITIVE_TRIANGLE, PRIMITIVE_TRIANGLE_STRIP, RENDER_ENCODER, RENDER_PASS_DESCRIPTOR,
    SET_ATTACHMENT_TEXTURE, SET_CLEAR_COLOR, SET_FRAGMENT_BUFFER, SET_FRAGMENT_SAMPLER,
    SET_FRAGMENT_TEXTURE, SET_LOAD_ACTION, SET_PIPELINE, SET_SCISSOR_RECT, SET_STORE_ACTION,
    SET_VERTEX_BUFFER, STORE_ACTION_STORE, new_buffer, set_fragment_ints, set_vertex_floats,
};
use crate::ffi::retained::Retained;
use std::collections::HashMap;

/// The most blur texture pairs that a window keeps.
const BLUR_LIMIT: usize = 8;

/// The textures that one window draws with, kept between frames.
#[derive(Default)]
pub(super) struct Targets {
    size: (usize, usize),
    frame: Option<Retained<MTLTexture>>,
    groups: Vec<Retained<MTLTexture>>,
    /// The coverage atlas texture slots, at most [`ATLAS_LIMIT`].
    atlases: Vec<Retained<MTLTexture>>,
    blurs: HashMap<(usize, usize), [Retained<MTLTexture>; 2]>,
    empty_color: Option<Retained<MTLTexture>>,
    empty_coverage: Option<Retained<MTLTexture>>,
}

impl Targets {
    /// Drops the textures of another frame size.
    fn fit(&mut self, width: usize, height: usize) {
        if self.size != (width, height) {
            self.size = (width, height);
            self.frame = None;
            self.groups.clear();
        }
    }

    /// Releases the coverage atlas slots past the first after a frame
    /// that needed more of them.
    fn trim(&mut self) {
        self.atlases.truncate(1);
    }

    /// The number of textures that the window keeps.
    pub(super) fn count(&self) -> usize {
        usize::from(self.frame.is_some())
            + self.groups.len()
            + self.atlases.len()
            + self.blurs.len() * 2
            + usize::from(self.empty_color.is_some())
            + usize::from(self.empty_coverage.is_some())
    }
}

/// Records a pass that clears `output` to transparent.
pub(super) fn clear(
    command_buffer: &Obj<MTLCommandBuffer>,
    output: &Obj<MTLTexture>,
) -> Option<()> {
    let pass = begin_render(command_buffer, output, LOAD_ACTION_CLEAR)?;
    END_RENDER_ENCODING.send(&pass, ());
    Some(())
}

/// A texture of `targets`, created on first use.
fn texture_in(
    slot: &mut Option<Retained<MTLTexture>>,
    create: impl FnOnce() -> Option<Retained<MTLTexture>>,
) -> Option<&Retained<MTLTexture>> {
    if slot.is_none() {
        *slot = create();
    }
    slot.as_ref()
}

/// The open render pass.
struct Pass {
    encoder: Retained<MTLRenderCommandEncoder>,
    target: Target,
    /// The scissor box that the pass has, or `None` for the whole
    /// target.
    scissor: Option<PixelBox>,
}

struct Encoder<'a> {
    gpu: &'a Gpu,
    command_buffer: &'a Obj<MTLCommandBuffer>,
    plan: &'a Plan,
    instances: Retained<MTLBuffer>,
    stops: Retained<MTLBuffer>,
    clips: Retained<MTLBuffer>,
    frame: Retained<MTLTexture>,
    groups: Vec<Retained<MTLTexture>>,
    atlases: Vec<Retained<MTLTexture>>,
    blur_results: Vec<Option<Retained<MTLTexture>>>,
    empty_color: Retained<MTLTexture>,
    empty_coverage: Retained<MTLTexture>,
    pass: Option<Pass>,
    frame_started: bool,
    clear_groups: Vec<bool>,
}

/// Records `plan` into `command_buffer`, drawing into `output`, a BGRA8
/// texture of the plan's size. Returns `None` when Metal could not create
/// a buffer, a texture, or an encoder; the command buffer must then not
/// be committed.
pub(super) fn encode(
    gpu: &Gpu,
    targets: &mut Targets,
    plan: &Plan,
    command_buffer: &Obj<MTLCommandBuffer>,
    output: &Obj<MTLTexture>,
) -> Option<()> {
    targets.fit(plan.width, plan.height);
    let device = &gpu.device;
    let frame = if plan.reads_frame {
        texture_in(&mut targets.frame, || {
            gpu.target_texture(COLOR_FORMAT, plan.width, plan.height)
        })?
        .clone()
    } else {
        output.retain()
    };
    while targets.groups.len() < plan.groups as usize {
        let group = gpu.target_texture(COLOR_FORMAT, plan.width, plan.height)?;
        targets.groups.push(group);
    }
    let slots = atlas_slots(plan);
    while targets.atlases.len() < slots {
        let size = ATLAS_SIZE as usize;
        let atlas = gpu.target_texture(PIXEL_FORMAT_R16_FLOAT, size, size)?;
        targets.atlases.push(atlas);
    }
    let atlases = targets.atlases[..slots].to_vec();
    let empty_color = texture_in(&mut targets.empty_color, || {
        gpu.upload_texture(PIXEL_FORMAT_RGBA8, 1, 1)
    })?
    .clone();
    let empty_coverage = texture_in(&mut targets.empty_coverage, || {
        gpu.target_texture(PIXEL_FORMAT_R16_FLOAT, 1, 1)
    })?
    .clone();
    let mut blur_results = Vec::with_capacity(plan.blurs.len());
    blur_results.resize(plan.blurs.len(), None);
    let mut encoder = Encoder {
        gpu,
        command_buffer,
        plan,
        instances: new_buffer(device, &plan.instances)?,
        stops: new_buffer(device, &plan.stops)?,
        clips: new_buffer(device, &plan.clips)?,
        frame,
        groups: targets.groups.clone(),
        atlases,
        blur_results,
        empty_color,
        empty_coverage,
        pass: None,
        frame_started: false,
        clear_groups: vec![false; plan.groups as usize + 1],
    };
    // The first atlas of each slot needs no draw before it, so its
    // coverage pass goes first and does not split the frame pass.
    for op in &plan.ops {
        if let Op::Coverage(atlas) = *op
            && atlas < ATLAS_LIMIT
        {
            encoder.coverage(atlas)?;
        }
    }
    for op in &plan.ops {
        match *op {
            Op::Draw {
                target,
                source,
                coverage,
                scissor,
                first,
                count,
            } => encoder.draw(target, source, coverage, scissor, first, count)?,
            Op::ClearGroup(depth) => encoder.clear_groups[depth as usize] = true,
            Op::Blur(index) => encoder.blur(targets, index)?,
            Op::Coverage(atlas) if atlas >= ATLAS_LIMIT => {
                encoder.end_pass();
                encoder.coverage(atlas)?;
            }
            Op::Coverage(_) => {}
        }
    }
    encoder.end_pass();
    if !encoder.frame_started {
        encoder.begin_pass(Target::Frame)?;
        encoder.end_pass();
    }
    if plan.reads_frame {
        let pass = begin_render(command_buffer, output, LOAD_ACTION_DONT_CARE)?;
        SET_PIPELINE.send(&pass, (&gpu.copy,));
        SET_FRAGMENT_TEXTURE.send(&pass, (Some(&*encoder.frame), 0));
        DRAW_PRIMITIVES.send(&pass, (PRIMITIVE_TRIANGLE, 0, 3, 1, 0));
        END_RENDER_ENCODING.send(&pass, ());
    }
    if slots > 1 {
        targets.trim();
    }
    Some(())
}

/// The coverage atlas texture slots that `plan` uses.
fn atlas_slots(plan: &Plan) -> usize {
    let atlases = plan
        .ops
        .iter()
        .filter_map(|op| match op {
            Op::Coverage(atlas) => Some(atlas + 1),
            _ => None,
        })
        .max()
        .unwrap_or(0);
    atlases.min(ATLAS_LIMIT) as usize
}

/// A render pass into `texture` with `load` for its one attachment.
fn begin_render(
    command_buffer: &Obj<MTLCommandBuffer>,
    texture: &Obj<MTLTexture>,
    load: usize,
) -> Option<Retained<MTLRenderCommandEncoder>> {
    let descriptor = RENDER_PASS_DESCRIPTOR.class_send(()).ok()??;
    let attachments = PASS_COLOR_ATTACHMENTS.send(&descriptor, ())?;
    let attachment = PASS_ATTACHMENT_AT.send(attachments, (0,))?;
    SET_ATTACHMENT_TEXTURE.send(attachment, (Some(texture),));
    SET_LOAD_ACTION.send(attachment, (load,));
    SET_STORE_ACTION.send(attachment, (STORE_ACTION_STORE,));
    SET_CLEAR_COLOR.send(attachment, (MTLClearColor::default(),));
    RENDER_ENCODER.send(command_buffer, (&descriptor,))
}

impl Encoder<'_> {
    /// Adds the edges of `atlas` into its texture slot.
    fn coverage(&mut self, atlas: u32) -> Option<()> {
        let texture = self.slot(atlas)?;
        let pass = begin_render(self.command_buffer, texture, LOAD_ACTION_CLEAR)?;
        let edges = self.plan.edges.get(atlas as usize)?;
        if !edges.is_empty() {
            let words: Vec<u32> = edges.iter().map(|value| value.to_bits()).collect();
            let buffer = new_buffer(&self.gpu.device, &words)?;
            SET_PIPELINE.send(&pass, (&self.gpu.edges,));
            SET_VERTEX_BUFFER.send(&pass, (Some(&*buffer), 0, 0));
            SET_FRAGMENT_BUFFER.send(&pass, (Some(&*buffer), 0, 0));
            let size = ATLAS_SIZE as f32;
            set_vertex_floats(&pass, &[size, size], 1);
            let count = edges.len() / EDGE_WORDS;
            DRAW_PRIMITIVES.send(&pass, (PRIMITIVE_TRIANGLE_STRIP, 0, 4, count, 0));
        }
        END_RENDER_ENCODING.send(&pass, ());
        Some(())
    }

    /// The texture slot of `atlas`.
    fn slot(&self, atlas: u32) -> Option<&Retained<MTLTexture>> {
        self.atlases.get((atlas % ATLAS_LIMIT) as usize)
    }

    fn target_texture(&self, target: Target) -> &Retained<MTLTexture> {
        match target {
            Target::Frame => &self.frame,
            Target::Group(depth) => &self.groups[depth as usize - 1],
        }
    }

    fn end_pass(&mut self) {
        if let Some(pass) = self.pass.take() {
            END_RENDER_ENCODING.send(&pass.encoder, ());
        }
    }

    /// Opens a pass into `target`, clearing it on its first pass.
    fn begin_pass(&mut self, target: Target) -> Option<()> {
        self.end_pass();
        let clear = match target {
            Target::Frame => !std::mem::replace(&mut self.frame_started, true),
            Target::Group(depth) => std::mem::take(&mut self.clear_groups[depth as usize]),
        };
        let load = if clear {
            LOAD_ACTION_CLEAR
        } else {
            LOAD_ACTION_LOAD
        };
        let texture = self.target_texture(target).clone();
        let encoder = begin_render(self.command_buffer, &texture, load)?;
        SET_PIPELINE.send(&encoder, (&self.gpu.ui,));
        SET_VERTEX_BUFFER.send(&encoder, (Some(&*self.instances), 0, 0));
        let viewport = [self.plan.width as f32, self.plan.height as f32];
        set_vertex_floats(&encoder, &viewport, 1);
        SET_FRAGMENT_BUFFER.send(&encoder, (Some(&*self.instances), 0, 0));
        SET_FRAGMENT_BUFFER.send(&encoder, (Some(&*self.stops), 0, 2));
        SET_FRAGMENT_BUFFER.send(&encoder, (Some(&*self.clips), 0, 3));
        SET_FRAGMENT_SAMPLER.send(&encoder, (Some(&*self.gpu.linear), 0));
        SET_FRAGMENT_SAMPLER.send(&encoder, (Some(&*self.gpu.nearest), 1));
        self.pass = Some(Pass {
            encoder,
            target,
            scissor: None,
        });
        Some(())
    }

    fn draw(
        &mut self,
        target: Target,
        source: Source,
        coverage: Option<u32>,
        scissor: PixelBox,
        first: u32,
        count: u32,
    ) -> Option<()> {
        let open = matches!(&self.pass, Some(pass) if pass.target == target);
        let pending_clear = match target {
            Target::Frame => !self.frame_started,
            Target::Group(depth) => self.clear_groups[depth as usize],
        };
        if !open || pending_clear {
            self.begin_pass(target)?;
        }
        let source = match source {
            Source::None => &self.empty_color,
            Source::Texture(index) => &self.plan.textures[index as usize],
            Source::Group(depth) => &self.groups[depth as usize - 1],
            Source::Blur(index) => self.blur_results[index as usize].as_ref()?,
        };
        let coverage = coverage
            .and_then(|atlas| self.atlases.get((atlas % ATLAS_LIMIT) as usize))
            .unwrap_or(&self.empty_coverage);
        let pass = self.pass.as_mut()?;
        let encoder = &pass.encoder;
        if pass.scissor != Some(scissor) {
            // The plan keeps each scissor box inside the frame, and the
            // group textures have the size of the frame.
            let [left, top, right, bottom] = scissor.map(|v| v.max(0) as usize);
            let rect = MTLScissorRect {
                x: left,
                y: top,
                width: right.saturating_sub(left),
                height: bottom.saturating_sub(top),
            };
            SET_SCISSOR_RECT.send(encoder, (rect,));
            pass.scissor = Some(scissor);
        }
        SET_FRAGMENT_TEXTURE.send(encoder, (Some(&**source), 0));
        SET_FRAGMENT_TEXTURE.send(encoder, (Some(&**coverage), 1));
        DRAW_PRIMITIVES.send(
            encoder,
            (
                PRIMITIVE_TRIANGLE_STRIP,
                0,
                4,
                count as usize,
                first as usize,
            ),
        );
        Some(())
    }

    /// Copies a region of the frame and blurs it with three box passes in
    /// each direction, as `blur_pixels` does.
    fn blur(&mut self, targets: &mut Targets, index: u32) -> Option<()> {
        let BlurPass {
            origin,
            size,
            radii,
        } = self.plan.blurs[index as usize];
        if !self.frame_started {
            self.begin_pass(Target::Frame)?;
        }
        self.end_pass();
        if !targets.blurs.contains_key(&size) {
            if targets.blurs.len() >= BLUR_LIMIT {
                targets.blurs.clear();
            }
            let first = self.gpu.target_texture(COLOR_FORMAT, size.0, size.1)?;
            let second = self.gpu.target_texture(COLOR_FORMAT, size.0, size.1)?;
            targets.blurs.insert(size, [first, second]);
        }
        let [first, second] = targets.blurs.get(&size)?.clone();
        let blit = BLIT_ENCODER.send(self.command_buffer, ())?;
        COPY_TEXTURE.send(
            &blit,
            (
                &self.frame,
                0,
                0,
                MTLOrigin::new(origin.0, origin.1),
                MTLSize::new(size.0, size.1),
                &first,
                0,
                0,
                MTLOrigin::new(0, 0),
            ),
        );
        END_BLIT_ENCODING.send(&blit, ());
        let (mut from, mut to) = (first, second);
        if let Some(radii) = radii {
            for direction in [[1, 0], [0, 1]] {
                for radius in radii {
                    let pass = begin_render(self.command_buffer, &to, LOAD_ACTION_DONT_CARE)?;
                    SET_PIPELINE.send(&pass, (&self.gpu.blur,));
                    SET_FRAGMENT_TEXTURE.send(&pass, (Some(&*from), 0));
                    let radius = i32::try_from(radius).unwrap_or(i32::MAX);
                    set_fragment_ints(&pass, &[direction[0], direction[1], radius, 0], 0);
                    DRAW_PRIMITIVES.send(&pass, (PRIMITIVE_TRIANGLE, 0, 3, 1, 0));
                    END_RENDER_ENCODING.send(&pass, ());
                    std::mem::swap(&mut from, &mut to);
                }
            }
        }
        self.blur_results[index as usize] = Some(from);
        Some(())
    }
}
