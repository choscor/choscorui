//! The Metal painter: one device, one command queue, and the pipelines
//! of the embedded shader library, shared by every window.
//!
//! `plan` turns a display list into instances and passes, `encode`
//! records them into a command buffer, and `surface` connects a window's
//! `CAMetalLayer` and display link to them. Core Graphics paints a
//! window when this module has no GPU.

mod coverage;
mod encode;
mod glyphs;
mod plan;
pub(crate) mod surface;

use crate::ffi::Obj;
use crate::ffi::metal::{
    BLEND_FACTOR_ONE, BLEND_FACTOR_ONE_MINUS_SOURCE_ALPHA, HAS_UNIFIED_MEMORY,
    MTLCreateSystemDefaultDevice, MTLDevice, MTLRenderPipelineDescriptor, MTLRenderPipelineState,
    MTLSamplerDescriptor, MTLSamplerState, MTLTexture, NEW_COMMAND_QUEUE, NEW_SAMPLER, NEW_TEXTURE,
    PIPELINE_ATTACHMENT_AT, PIPELINE_COLOR_ATTACHMENTS, PIXEL_FORMAT_BGRA8, PIXEL_FORMAT_R16_FLOAT,
    SAMPLER_FILTER_LINEAR, SAMPLER_FILTER_NEAREST, SET_BLENDING_ENABLED,
    SET_DESTINATION_ALPHA_BLEND_FACTOR, SET_DESTINATION_RGB_BLEND_FACTOR, SET_FRAGMENT_FUNCTION,
    SET_MAG_FILTER, SET_MIN_FILTER, SET_PIXEL_FORMAT, SET_SOURCE_ALPHA_BLEND_FACTOR,
    SET_SOURCE_RGB_BLEND_FACTOR, SET_TEXTURE_STORAGE_MODE, SET_TEXTURE_USAGE, SET_VERTEX_FUNCTION,
    STORAGE_MANAGED, STORAGE_PRIVATE, STORAGE_SHARED, TEXTURE_2D_DESCRIPTOR,
    TEXTURE_USAGE_RENDER_TARGET, TEXTURE_USAGE_SHADER_READ, ThreadSafe, new_function, new_library,
    new_pipeline,
};
use crate::ffi::retained::{Retained, alloc_init};
use std::sync::OnceLock;

pub use surface::Painter;

/// The compiled shaders. `scripts/macos/build-shaders.sh` writes them.
const LIBRARY: &[u8] = include_bytes!("../shaders/ui.metallib");

/// The pixel format of every color target.
const COLOR_FORMAT: usize = PIXEL_FORMAT_BGRA8;

/// The process-wide Metal objects.
pub(crate) struct Gpu {
    device: ThreadSafe<MTLDevice>,
    queue: ThreadSafe<crate::ffi::metal::MTLCommandQueue>,
    /// Draws instances into a color target with premultiplied blending.
    ui: ThreadSafe<MTLRenderPipelineState>,
    /// Adds the winding numbers of path edges into a coverage atlas.
    edges: ThreadSafe<MTLRenderPipelineState>,
    /// One box blur pass.
    blur: ThreadSafe<MTLRenderPipelineState>,
    /// Copies a texture into a target of the same size.
    copy: ThreadSafe<MTLRenderPipelineState>,
    linear: ThreadSafe<MTLSamplerState>,
    nearest: ThreadSafe<MTLSamplerState>,
    /// True when the CPU and the GPU share memory, so textures that the
    /// CPU writes use shared storage.
    unified_memory: bool,
}

/// The environment variable that selects the painter.
const PAINTER_VARIABLE: &str = "CHOSCORUI_PAINTER";

/// True when `CHOSCORUI_PAINTER=core-graphics` selects Core Graphics.
fn core_graphics_requested() -> bool {
    static REQUESTED: OnceLock<bool> = OnceLock::new();
    *REQUESTED
        .get_or_init(|| std::env::var(PAINTER_VARIABLE).is_ok_and(|value| value == "core-graphics"))
}

/// The shared GPU objects, or `None` when the process paints with Core
/// Graphics: no Metal device, a library or pipeline failure, or the
/// `CHOSCORUI_PAINTER` override.
pub(crate) fn gpu() -> Option<&'static Gpu> {
    static GPU: OnceLock<Option<Gpu>> = OnceLock::new();
    if core_graphics_requested() {
        return None;
    }
    GPU.get_or_init(Gpu::new).as_ref()
}

impl Gpu {
    fn new() -> Option<Self> {
        let _pool = crate::ffi::Pool::new().ok()?;
        let device = MTLCreateSystemDefaultDevice()?;
        let queue = NEW_COMMAND_QUEUE.send(&device, ())?;
        let library = new_library(&device, LIBRARY)?;
        let function = |name: &str| new_function(&library, name);
        let ui_vertex = function("ui_vertex")?;
        let ui_fragment = function("ui_fragment")?;
        let edge_vertex = function("edge_vertex")?;
        let edge_fragment = function("edge_fragment")?;
        let screen_vertex = function("screen_vertex")?;
        let box_fragment = function("box_fragment")?;
        let copy_fragment = function("copy_fragment")?;
        let ui = pipeline(&device, &ui_vertex, &ui_fragment, COLOR_FORMAT, Blend::Over)?;
        let edges = pipeline(
            &device,
            &edge_vertex,
            &edge_fragment,
            PIXEL_FORMAT_R16_FLOAT,
            Blend::Add,
        )?;
        let blur = pipeline(
            &device,
            &screen_vertex,
            &box_fragment,
            COLOR_FORMAT,
            Blend::None,
        )?;
        let copy = pipeline(
            &device,
            &screen_vertex,
            &copy_fragment,
            COLOR_FORMAT,
            Blend::None,
        )?;
        let sampler = |filter: usize| {
            let descriptor = alloc_init::<MTLSamplerDescriptor>().ok()?;
            SET_MIN_FILTER.send(&descriptor, (filter,));
            SET_MAG_FILTER.send(&descriptor, (filter,));
            NEW_SAMPLER.send(&device, (&descriptor,))
        };
        let linear = sampler(SAMPLER_FILTER_LINEAR)?;
        let nearest = sampler(SAMPLER_FILTER_NEAREST)?;
        let unified_memory = HAS_UNIFIED_MEMORY.send(&device, ());
        Some(Self {
            device: ThreadSafe::new(device),
            queue: ThreadSafe::new(queue),
            ui: ThreadSafe::new(ui),
            edges: ThreadSafe::new(edges),
            blur: ThreadSafe::new(blur),
            copy: ThreadSafe::new(copy),
            linear: ThreadSafe::new(linear),
            nearest: ThreadSafe::new(nearest),
            unified_memory,
        })
    }
}

impl Gpu {
    /// A texture that the CPU writes and shaders read.
    fn upload_texture(
        &self,
        format: usize,
        width: usize,
        height: usize,
    ) -> Option<Retained<MTLTexture>> {
        let storage = if self.unified_memory {
            STORAGE_SHARED
        } else {
            STORAGE_MANAGED
        };
        self.texture(format, width, height, TEXTURE_USAGE_SHADER_READ, storage)
    }

    /// A texture in GPU memory that passes draw into and shaders read.
    fn target_texture(
        &self,
        format: usize,
        width: usize,
        height: usize,
    ) -> Option<Retained<MTLTexture>> {
        self.texture(
            format,
            width,
            height,
            TEXTURE_USAGE_SHADER_READ | TEXTURE_USAGE_RENDER_TARGET,
            STORAGE_PRIVATE,
        )
    }

    fn texture(
        &self,
        format: usize,
        width: usize,
        height: usize,
        usage: usize,
        storage: usize,
    ) -> Option<Retained<MTLTexture>> {
        if width == 0 || height == 0 {
            return None;
        }
        let descriptor = TEXTURE_2D_DESCRIPTOR
            .class_send((format, width, height, false))
            .ok()??;
        SET_TEXTURE_USAGE.send(&descriptor, (usage,));
        SET_TEXTURE_STORAGE_MODE.send(&descriptor, (storage,));
        NEW_TEXTURE.send(&self.device, (&descriptor,))
    }
}

/// How a pipeline combines its output with the target.
#[derive(Clone, Copy)]
enum Blend {
    /// Replaces the target.
    None,
    /// Premultiplied source over the target.
    Over,
    /// Adds the source to the target.
    Add,
}

fn pipeline(
    device: &Obj<MTLDevice>,
    vertex: &Obj<crate::ffi::metal::MTLFunction>,
    fragment: &Obj<crate::ffi::metal::MTLFunction>,
    format: usize,
    blend: Blend,
) -> Option<crate::ffi::retained::Retained<MTLRenderPipelineState>> {
    let descriptor = alloc_init::<MTLRenderPipelineDescriptor>().ok()?;
    SET_VERTEX_FUNCTION.send(&descriptor, (Some(vertex),));
    SET_FRAGMENT_FUNCTION.send(&descriptor, (Some(fragment),));
    let attachments = PIPELINE_COLOR_ATTACHMENTS.send(&descriptor, ())?;
    let attachment = PIPELINE_ATTACHMENT_AT.send(attachments, (0,))?;
    SET_PIXEL_FORMAT.send(attachment, (format,));
    let destination = match blend {
        Blend::None => None,
        Blend::Over => Some(BLEND_FACTOR_ONE_MINUS_SOURCE_ALPHA),
        Blend::Add => Some(BLEND_FACTOR_ONE),
    };
    if let Some(destination) = destination {
        SET_BLENDING_ENABLED.send(attachment, (true,));
        SET_SOURCE_RGB_BLEND_FACTOR.send(attachment, (BLEND_FACTOR_ONE,));
        SET_SOURCE_ALPHA_BLEND_FACTOR.send(attachment, (BLEND_FACTOR_ONE,));
        SET_DESTINATION_RGB_BLEND_FACTOR.send(attachment, (destination,));
        SET_DESTINATION_ALPHA_BLEND_FACTOR.send(attachment, (destination,));
    }
    new_pipeline(device, &descriptor)
}

/// True when windows paint with Metal. It is the value of the
/// `gpu_painting` capability.
pub(crate) fn gpu_painting() -> bool {
    gpu().is_some()
}

/// Drops the cached image textures; called when the last window closes.
pub(crate) fn clear_image_cache() {
    plan::clear_caches();
}

#[cfg(test)]
mod tests {
    /// The SHA-256 digest of `bytes` in lowercase hex. The test has no
    /// dependency, so it carries its own digest.
    fn sha256(bytes: &[u8]) -> String {
        const K: [u32; 64] = [
            0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
            0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
            0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
            0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
            0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
            0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
            0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
            0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
            0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
            0xc67178f2,
        ];
        let mut h: [u32; 8] = [
            0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
            0x5be0cd19,
        ];
        let mut message = bytes.to_vec();
        message.push(0x80);
        while message.len() % 64 != 56 {
            message.push(0);
        }
        message.extend_from_slice(&((bytes.len() as u64) * 8).to_be_bytes());
        for block in message.chunks(64) {
            let mut w = [0u32; 64];
            for (index, word) in block.chunks(4).enumerate() {
                w[index] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
            }
            for i in 16..64 {
                let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
                let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
                w[i] = w[i - 16]
                    .wrapping_add(s0)
                    .wrapping_add(w[i - 7])
                    .wrapping_add(s1);
            }
            let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = h;
            for i in 0..64 {
                let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
                let choice = (e & f) ^ (!e & g);
                let t1 = hh
                    .wrapping_add(s1)
                    .wrapping_add(choice)
                    .wrapping_add(K[i])
                    .wrapping_add(w[i]);
                let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
                let majority = (a & b) ^ (a & c) ^ (b & c);
                let t2 = s0.wrapping_add(majority);
                hh = g;
                g = f;
                f = e;
                e = d.wrapping_add(t1);
                d = c;
                c = b;
                b = a;
                a = t1.wrapping_add(t2);
            }
            for (state, value) in h.iter_mut().zip([a, b, c, d, e, f, g, hh]) {
                *state = state.wrapping_add(value);
            }
        }
        h.iter().map(|word| format!("{word:08x}")).collect()
    }

    #[test]
    fn the_digest_matches_known_values() {
        assert_eq!(
            sha256(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    /// Fails when `ui.metal` or `ui.metallib` changed without
    /// `scripts/macos/build-shaders.sh`, which compiles the library and
    /// writes the stamp.
    #[test]
    fn shader_library_matches_source() {
        let stamp = include_str!("../shaders/ui.metallib.stamp");
        let expected = format!(
            "ui.metal {}\nui.metallib {}\n",
            sha256(include_bytes!("../shaders/ui.metal")),
            sha256(super::LIBRARY)
        );
        assert_eq!(
            stamp, expected,
            "run scripts/macos/build-shaders.sh after a shader change"
        );
    }
}
