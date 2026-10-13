//! Records a [`Plan`] into a command buffer: coverage passes first, then
//! the frame, its opacity groups, and its blurs in list order.

use super::coverage::{ATLAS_SIZE, EDGE_WORDS};
use super::plan::{BlurPass, Op, Plan, Source, Target};
use super::{COLOR_FORMAT, Gpu};
use crate::ffi::Obj;
use crate::ffi::metal::{
    BLIT_ENCODER, COPY_TEXTURE, DRAW_PRIMITIVES, END_BLIT_ENCODING, END_RENDER_ENCODING,
    LOAD_ACTION_CLEAR, LOAD_ACTION_DONT_CARE, LOAD_ACTION_LOAD, MTLBuffer, MTLClearColor,
    MTLCommandBuffer, MTLOrigin, MTLRenderCommandEncoder, MTLSize, MTLTexture, PASS_ATTACHMENT_AT,
    PASS_COLOR_ATTACHMENTS, PIXEL_FORMAT_R16_FLOAT, PIXEL_FORMAT_RGBA8, PRIMITIVE_TRIANGLE,
    PRIMITIVE_TRIANGLE_STRIP, RENDER_ENCODER, RENDER_PASS_DESCRIPTOR, SET_ATTACHMENT_TEXTURE,
    SET_CLEAR_COLOR, SET_FRAGMENT_BUFFER, SET_FRAGMENT_SAMPLER, SET_FRAGMENT_TEXTURE,
    SET_LOAD_ACTION, SET_PIPELINE, SET_STORE_ACTION, SET_VERTEX_BUFFER, STORE_ACTION_STORE,
    new_buffer, set_fragment_ints, set_vertex_floats,
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

    /// Releases the coverage atlases past the first after a frame that
    /// needed more of them.
    fn trim(&mut self) {
        self.atlases.truncate(1);
    }
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
    atlases: Vec<Option<Retained<MTLTexture>>>,
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
    let used_atlases = used_atlases(plan);
    let mut atlases = Vec::with_capacity(plan.edges.len());
    for (index, used) in used_atlases.iter().enumerate() {
        if !used {
            atlases.push(None);
            continue;
        }
        while targets.atlases.len() <= index {
            let size = ATLAS_SIZE as usize;
            let atlas = gpu.target_texture(PIXEL_FORMAT_R16_FLOAT, size, size)?;
            targets.atlases.push(atlas);
        }
        atlases.push(Some(targets.atlases[index].clone()));
    }
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
    encoder.coverage()?;
    for op in &plan.ops {
        match *op {
            Op::Draw {
                target,
                source,
                coverage,
                first,
                count,
            } => encoder.draw(target, source, coverage, first, count)?,
            Op::ClearGroup(depth) => encoder.clear_groups[depth as usize] = true,
            Op::Blur(index) => encoder.blur(targets, index)?,
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
    if used_atlases.len() > 1 {
        targets.trim();
    }
    Some(())
}

/// Which coverage atlases the draws of `plan` read.
fn used_atlases(plan: &Plan) -> Vec<bool> {
    let mut used = vec![false; plan.edges.len()];
    for op in &plan.ops {
        if let Op::Draw {
            coverage: Some(atlas),
            ..
        } = op
            && let Some(slot) = used.get_mut(*atlas as usize)
        {
            *slot = true;
        }
    }
    used
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
    /// Adds the edges of each used atlas into it.
    fn coverage(&mut self) -> Option<()> {
        for (index, atlas) in self.atlases.iter().enumerate() {
            let Some(atlas) = atlas else {
                continue;
            };
            let pass = begin_render(self.command_buffer, atlas, LOAD_ACTION_CLEAR)?;
            let edges = &self.plan.edges[index];
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
        }
        Some(())
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
        self.pass = Some(Pass { encoder, target });
        Some(())
    }

    fn draw(
        &mut self,
        target: Target,
        source: Source,
        coverage: Option<u32>,
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
            .and_then(|atlas| self.atlases.get(atlas as usize)?.as_ref())
            .unwrap_or(&self.empty_coverage);
        let encoder = &self.pass.as_ref()?.encoder;
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
