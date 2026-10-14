//! Metal protocols, descriptor classes, structs, constants, and messages,
//! with the safe helpers that pass slices and pointers to Metal.
//!
//! The Metal descriptor classes are class clusters: the public class
//! declares the methods and a private `Internal` subclass implements
//! them. Each such marker names that subclass for the runtime check.

use super::block::{Block, GlobalBlock, ObjectHandler};
use super::class::{Obj, objc_class, objc_protocol};
use super::encode::encode_as;
use super::foundation::{NSObject, NSString, ns_string};
use super::msg::{Autoreleased, messages};
use super::retained::Retained;
use super::sdk::{c_functions, ffi_struct, sdk_const};
use std::ffi::c_void;

objc_protocol!(MTLDevice);
objc_protocol!(MTLCommandQueue);
objc_protocol!(MTLCommandBuffer);
objc_protocol!(MTLRenderCommandEncoder);
objc_protocol!(MTLBlitCommandEncoder);
objc_protocol!(MTLLibrary);
objc_protocol!(MTLFunction);
objc_protocol!(MTLRenderPipelineState);
objc_protocol!(MTLSamplerState);
objc_protocol!(MTLTexture);
objc_protocol!(MTLBuffer);
objc_protocol!(MTLDrawable);
objc_protocol!(
    /// `dispatch_data_t`, the bytes of a Metal library.
    DispatchData = "OS_dispatch_data"
);

objc_class!(MTLRenderPassDescriptor in "MTLRenderPassDescriptorInternal": NSObject);
objc_class!(
    MTLRenderPassColorAttachmentDescriptorArray
        in "MTLRenderPassColorAttachmentDescriptorArrayInternal": NSObject
);
objc_class!(
    MTLRenderPassColorAttachmentDescriptor
        in "MTLRenderPassColorAttachmentDescriptorInternal": NSObject
);
objc_class!(MTLRenderPipelineDescriptor in "MTLRenderPipelineDescriptorInternal": NSObject);
objc_class!(
    MTLRenderPipelineColorAttachmentDescriptorArray
        in "MTLRenderPipelineColorAttachmentDescriptorArrayInternal": NSObject
);
objc_class!(
    MTLRenderPipelineColorAttachmentDescriptor
        in "MTLRenderPipelineColorAttachmentDescriptorInternal": NSObject
);
objc_class!(MTLTextureDescriptor in "MTLTextureDescriptorInternal": NSObject);
objc_class!(MTLSamplerDescriptor in "MTLSamplerDescriptorInternal": NSObject);

ffi_struct! {
    /// `MTLClearColor`.
    #[derive(Clone, Copy, Debug, Default, PartialEq)]
    pub(crate) struct MTLClearColor = MTLClearColor {
        pub(crate) red: f64,
        pub(crate) green: f64,
        pub(crate) blue: f64,
        pub(crate) alpha: f64,
    }

    /// `MTLOrigin`.
    #[derive(Clone, Copy, Debug, Default, PartialEq)]
    pub(crate) struct MTLOrigin = MTLOrigin {
        pub(crate) x: usize,
        pub(crate) y: usize,
        pub(crate) z: usize,
    }

    /// `MTLSize`.
    #[derive(Clone, Copy, Debug, Default, PartialEq)]
    pub(crate) struct MTLSize = MTLSize {
        pub(crate) width: usize,
        pub(crate) height: usize,
        pub(crate) depth: usize,
    }

    /// `MTLRegion`.
    #[derive(Clone, Copy, Debug, Default, PartialEq)]
    pub(crate) struct MTLRegion = MTLRegion {
        pub(crate) origin: MTLOrigin,
        pub(crate) size: MTLSize,
    }

    /// `MTLScissorRect`.
    #[derive(Clone, Copy, Debug, Default, PartialEq)]
    pub(crate) struct MTLScissorRect = MTLScissorRect {
        pub(crate) x: usize,
        pub(crate) y: usize,
        pub(crate) width: usize,
        pub(crate) height: usize,
    }
}

encode_as!(MTLClearColor, "{?=dddd}");
encode_as!(MTLOrigin, "{?=QQQ}");
encode_as!(MTLSize, "{?=QQQ}");
encode_as!(MTLRegion, "{?={?=QQQ}{?=QQQ}}");
encode_as!(MTLScissorRect, "{?=QQQQ}");

impl MTLOrigin {
    /// The origin `(x, y, 0)` of a 2D texture.
    pub(crate) const fn new(x: usize, y: usize) -> Self {
        Self { x, y, z: 0 }
    }
}

impl MTLSize {
    /// The size `width × height × 1` of a 2D texture.
    pub(crate) const fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            depth: 1,
        }
    }
}

sdk_const! {
    pub(crate) PIXEL_FORMAT_R8: usize = 10, sdk = MTLPixelFormatR8Unorm;
    pub(crate) PIXEL_FORMAT_R16_FLOAT: usize = 25, sdk = MTLPixelFormatR16Float;
    pub(crate) PIXEL_FORMAT_RGBA8: usize = 70, sdk = MTLPixelFormatRGBA8Unorm;
    pub(crate) PIXEL_FORMAT_BGRA8: usize = 80, sdk = MTLPixelFormatBGRA8Unorm;
    pub(crate) TEXTURE_USAGE_SHADER_READ: usize = 1, sdk = MTLTextureUsageShaderRead;
    pub(crate) TEXTURE_USAGE_RENDER_TARGET: usize = 4, sdk = MTLTextureUsageRenderTarget;
    pub(crate) STORAGE_SHARED: usize = 0, sdk = MTLStorageModeShared;
    pub(crate) STORAGE_MANAGED: usize = 1, sdk = MTLStorageModeManaged;
    pub(crate) STORAGE_PRIVATE: usize = 2, sdk = MTLStorageModePrivate;
    pub(crate) RESOURCE_STORAGE_SHARED: usize = 0, sdk = MTLResourceStorageModeShared;
    pub(crate) LOAD_ACTION_DONT_CARE: usize = 0, sdk = MTLLoadActionDontCare;
    pub(crate) LOAD_ACTION_LOAD: usize = 1, sdk = MTLLoadActionLoad;
    pub(crate) LOAD_ACTION_CLEAR: usize = 2, sdk = MTLLoadActionClear;
    pub(crate) STORE_ACTION_STORE: usize = 1, sdk = MTLStoreActionStore;
    pub(crate) BLEND_FACTOR_ONE: usize = 1, sdk = MTLBlendFactorOne;
    pub(crate) BLEND_FACTOR_ONE_MINUS_SOURCE_ALPHA: usize = 5,
        sdk = MTLBlendFactorOneMinusSourceAlpha;
    pub(crate) SAMPLER_FILTER_NEAREST: usize = 0, sdk = MTLSamplerMinMagFilterNearest;
    pub(crate) SAMPLER_FILTER_LINEAR: usize = 1, sdk = MTLSamplerMinMagFilterLinear;
    pub(crate) PRIMITIVE_TRIANGLE: usize = 3, sdk = MTLPrimitiveTypeTriangle;
    pub(crate) PRIMITIVE_TRIANGLE_STRIP: usize = 4, sdk = MTLPrimitiveTypeTriangleStrip;
    pub(crate) COMMAND_BUFFER_STATUS_ERROR: usize = 5, sdk = MTLCommandBufferStatusError;
}

/// The arguments of a command buffer completion block.
pub(crate) type CommandBufferArgs = (Option<&'static Obj<MTLCommandBuffer>>,);
/// The arguments of a drawable presented block.
pub(crate) type DrawableArgs = (Option<&'static Obj<MTLDrawable>>,);

/// The error out-parameter of a Metal message. The helpers pass null.
type ErrorOut = *const Option<&'static Obj<NSObject>>;

messages! {
    pub(crate) NEW_COMMAND_QUEUE: instance MTLDevice, c"newCommandQueue",
        () -> Option<Retained<MTLCommandQueue>>;
    NEW_LIBRARY_WITH_DATA: instance MTLDevice, c"newLibraryWithData:error:",
        (&Obj<DispatchData>, ErrorOut) -> Option<Retained<MTLLibrary>>;
    NEW_RENDER_PIPELINE: instance MTLDevice, c"newRenderPipelineStateWithDescriptor:error:",
        (&Obj<MTLRenderPipelineDescriptor>, ErrorOut) -> Option<Retained<MTLRenderPipelineState>>;
    pub(crate) NEW_TEXTURE: instance MTLDevice, c"newTextureWithDescriptor:",
        (&Obj<MTLTextureDescriptor>) -> Option<Retained<MTLTexture>>;
    NEW_BUFFER_WITH_BYTES: instance MTLDevice, c"newBufferWithBytes:length:options:",
        (*const c_void, usize, usize) -> Option<Retained<MTLBuffer>>;
    pub(crate) NEW_BUFFER_WITH_LENGTH: instance MTLDevice, c"newBufferWithLength:options:",
        (usize, usize) -> Option<Retained<MTLBuffer>>;
    pub(crate) NEW_SAMPLER: instance MTLDevice, c"newSamplerStateWithDescriptor:",
        (&Obj<MTLSamplerDescriptor>) -> Option<Retained<MTLSamplerState>>;
    pub(crate) HAS_UNIFIED_MEMORY: instance MTLDevice, c"hasUnifiedMemory", () -> bool;
    NEW_FUNCTION: instance MTLLibrary, c"newFunctionWithName:",
        (&Obj<NSString>) -> Option<Retained<MTLFunction>>;

    pub(crate) COMMAND_BUFFER: instance MTLCommandQueue, c"commandBuffer",
        () -> Autoreleased<MTLCommandBuffer>;
    pub(crate) RENDER_ENCODER: instance MTLCommandBuffer,
        c"renderCommandEncoderWithDescriptor:",
        (&Obj<MTLRenderPassDescriptor>) -> Autoreleased<MTLRenderCommandEncoder>;
    pub(crate) BLIT_ENCODER: instance MTLCommandBuffer, c"blitCommandEncoder",
        () -> Autoreleased<MTLBlitCommandEncoder>;
    pub(crate) PRESENT_DRAWABLE: instance MTLCommandBuffer, c"presentDrawable:",
        (&Obj<MTLDrawable>) -> ();
    pub(crate) COMMIT: instance MTLCommandBuffer, c"commit", () -> ();
    pub(crate) WAIT_UNTIL_SCHEDULED: instance MTLCommandBuffer, c"waitUntilScheduled", () -> ();
    pub(crate) WAIT_UNTIL_COMPLETED: instance MTLCommandBuffer, c"waitUntilCompleted", () -> ();
    pub(crate) ADD_COMPLETED_HANDLER: instance MTLCommandBuffer, c"addCompletedHandler:",
        (&Block<CommandBufferArgs>) -> ();
    pub(crate) COMMAND_BUFFER_STATUS: instance MTLCommandBuffer, c"status", () -> usize;
    pub(crate) GPU_END_TIME: instance MTLCommandBuffer, c"GPUEndTime", () -> f64;

    pub(crate) SET_PIPELINE: instance MTLRenderCommandEncoder, c"setRenderPipelineState:",
        (&Obj<MTLRenderPipelineState>) -> ();
    pub(crate) SET_VERTEX_BUFFER: instance MTLRenderCommandEncoder,
        c"setVertexBuffer:offset:atIndex:", (Option<&Obj<MTLBuffer>>, usize, usize) -> ();
    pub(crate) SET_FRAGMENT_BUFFER: instance MTLRenderCommandEncoder,
        c"setFragmentBuffer:offset:atIndex:", (Option<&Obj<MTLBuffer>>, usize, usize) -> ();
    SET_VERTEX_BYTES: instance MTLRenderCommandEncoder, c"setVertexBytes:length:atIndex:",
        (*const c_void, usize, usize) -> ();
    SET_FRAGMENT_BYTES: instance MTLRenderCommandEncoder, c"setFragmentBytes:length:atIndex:",
        (*const c_void, usize, usize) -> ();
    pub(crate) SET_FRAGMENT_TEXTURE: instance MTLRenderCommandEncoder,
        c"setFragmentTexture:atIndex:", (Option<&Obj<MTLTexture>>, usize) -> ();
    pub(crate) SET_FRAGMENT_SAMPLER: instance MTLRenderCommandEncoder,
        c"setFragmentSamplerState:atIndex:", (Option<&Obj<MTLSamplerState>>, usize) -> ();
    pub(crate) DRAW_PRIMITIVES: instance MTLRenderCommandEncoder,
        c"drawPrimitives:vertexStart:vertexCount:instanceCount:baseInstance:",
        (usize, usize, usize, usize, usize) -> ();
    pub(crate) SET_SCISSOR_RECT: instance MTLRenderCommandEncoder, c"setScissorRect:",
        (MTLScissorRect) -> ();
    pub(crate) END_RENDER_ENCODING: instance MTLRenderCommandEncoder, c"endEncoding", () -> ();

    pub(crate) COPY_TEXTURE: instance MTLBlitCommandEncoder,
        c"copyFromTexture:sourceSlice:sourceLevel:sourceOrigin:sourceSize:toTexture:destinationSlice:destinationLevel:destinationOrigin:",
        (&Obj<MTLTexture>, usize, usize, MTLOrigin, MTLSize, &Obj<MTLTexture>, usize, usize, MTLOrigin)
            -> ();
    pub(crate) COPY_TEXTURE_TO_BUFFER: instance MTLBlitCommandEncoder,
        c"copyFromTexture:sourceSlice:sourceLevel:sourceOrigin:sourceSize:toBuffer:destinationOffset:destinationBytesPerRow:destinationBytesPerImage:",
        (&Obj<MTLTexture>, usize, usize, MTLOrigin, MTLSize, &Obj<MTLBuffer>, usize, usize, usize)
            -> ();
    pub(crate) END_BLIT_ENCODING: instance MTLBlitCommandEncoder, c"endEncoding", () -> ();

    REPLACE_REGION: instance MTLTexture, c"replaceRegion:mipmapLevel:withBytes:bytesPerRow:",
        (MTLRegion, usize, *const c_void, usize) -> ();
    GET_BYTES: instance MTLTexture, c"getBytes:bytesPerRow:fromRegion:mipmapLevel:",
        (*mut c_void, usize, MTLRegion, usize) -> ();
    pub(crate) TEXTURE_WIDTH: instance MTLTexture, c"width", () -> usize;
    pub(crate) TEXTURE_HEIGHT: instance MTLTexture, c"height", () -> usize;
    BUFFER_CONTENTS: instance MTLBuffer, c"contents", () -> *const c_void;
    BUFFER_LENGTH: instance MTLBuffer, c"length", () -> usize;

    pub(crate) ADD_PRESENTED_HANDLER: instance MTLDrawable, c"addPresentedHandler:",
        (&Block<DrawableArgs>) -> ();
    pub(crate) PRESENTED_TIME: instance MTLDrawable, c"presentedTime", () -> f64;
    pub(crate) PRESENT: instance MTLDrawable, c"present", () -> ();

    pub(crate) RENDER_PASS_DESCRIPTOR: class MTLRenderPassDescriptor, c"renderPassDescriptor",
        () -> Autoreleased<MTLRenderPassDescriptor>;
    pub(crate) PASS_COLOR_ATTACHMENTS: instance MTLRenderPassDescriptor, c"colorAttachments",
        () -> Option<&Obj<MTLRenderPassColorAttachmentDescriptorArray>>;
    pub(crate) PASS_ATTACHMENT_AT: instance MTLRenderPassColorAttachmentDescriptorArray,
        c"objectAtIndexedSubscript:",
        (usize) -> Option<&Obj<MTLRenderPassColorAttachmentDescriptor>>;
    pub(crate) SET_ATTACHMENT_TEXTURE: instance MTLRenderPassColorAttachmentDescriptor,
        c"setTexture:", (Option<&Obj<MTLTexture>>) -> ();
    pub(crate) SET_LOAD_ACTION: instance MTLRenderPassColorAttachmentDescriptor,
        c"setLoadAction:", (usize) -> ();
    pub(crate) SET_STORE_ACTION: instance MTLRenderPassColorAttachmentDescriptor,
        c"setStoreAction:", (usize) -> ();
    pub(crate) SET_CLEAR_COLOR: instance MTLRenderPassColorAttachmentDescriptor,
        c"setClearColor:", (MTLClearColor) -> ();

    pub(crate) SET_VERTEX_FUNCTION: instance MTLRenderPipelineDescriptor, c"setVertexFunction:",
        (Option<&Obj<MTLFunction>>) -> ();
    pub(crate) SET_FRAGMENT_FUNCTION: instance MTLRenderPipelineDescriptor,
        c"setFragmentFunction:", (Option<&Obj<MTLFunction>>) -> ();
    pub(crate) PIPELINE_COLOR_ATTACHMENTS: instance MTLRenderPipelineDescriptor,
        c"colorAttachments", () -> Option<&Obj<MTLRenderPipelineColorAttachmentDescriptorArray>>;
    pub(crate) PIPELINE_ATTACHMENT_AT: instance MTLRenderPipelineColorAttachmentDescriptorArray,
        c"objectAtIndexedSubscript:",
        (usize) -> Option<&Obj<MTLRenderPipelineColorAttachmentDescriptor>>;
    pub(crate) SET_PIXEL_FORMAT: instance MTLRenderPipelineColorAttachmentDescriptor,
        c"setPixelFormat:", (usize) -> ();
    pub(crate) SET_BLENDING_ENABLED: instance MTLRenderPipelineColorAttachmentDescriptor,
        c"setBlendingEnabled:", (bool) -> ();
    pub(crate) SET_SOURCE_RGB_BLEND_FACTOR: instance MTLRenderPipelineColorAttachmentDescriptor,
        c"setSourceRGBBlendFactor:", (usize) -> ();
    pub(crate) SET_DESTINATION_RGB_BLEND_FACTOR:
        instance MTLRenderPipelineColorAttachmentDescriptor,
        c"setDestinationRGBBlendFactor:", (usize) -> ();
    pub(crate) SET_SOURCE_ALPHA_BLEND_FACTOR:
        instance MTLRenderPipelineColorAttachmentDescriptor,
        c"setSourceAlphaBlendFactor:", (usize) -> ();
    pub(crate) SET_DESTINATION_ALPHA_BLEND_FACTOR:
        instance MTLRenderPipelineColorAttachmentDescriptor,
        c"setDestinationAlphaBlendFactor:", (usize) -> ();

    pub(crate) TEXTURE_2D_DESCRIPTOR: class MTLTextureDescriptor,
        c"texture2DDescriptorWithPixelFormat:width:height:mipmapped:",
        (usize, usize, usize, bool) -> Autoreleased<MTLTextureDescriptor>;
    pub(crate) SET_TEXTURE_USAGE: instance MTLTextureDescriptor, c"setUsage:", (usize) -> ();
    pub(crate) SET_TEXTURE_STORAGE_MODE: instance MTLTextureDescriptor, c"setStorageMode:",
        (usize) -> ();

    pub(crate) SET_MIN_FILTER: instance MTLSamplerDescriptor, c"setMinFilter:", (usize) -> ();
    pub(crate) SET_MAG_FILTER: instance MTLSamplerDescriptor, c"setMagFilter:", (usize) -> ();
}

c_functions! {
    #[link(name = "Metal", kind = "framework")]
    functions {
        pub(crate) safe fn MTLCreateSystemDefaultDevice() -> Option<Retained<MTLDevice>>;
        // In libSystem, which every process links.
        unsafe fn dispatch_data_create(
            buffer: *const c_void,
            size: usize,
            queue: *const c_void,
            destructor: *const c_void,
        ) -> Option<Retained<DispatchData>>;
    }
}

/// A Metal object that any thread may use. Metal documents devices,
/// command queues, pipeline states, and sampler states as thread-safe.
pub(crate) struct ThreadSafe<C: MetalThreadSafe>(Retained<C>);

/// The Metal protocols whose objects any thread may use.
pub(crate) trait MetalThreadSafe {}
impl MetalThreadSafe for MTLDevice {}
impl MetalThreadSafe for MTLCommandQueue {}
impl MetalThreadSafe for MTLRenderPipelineState {}
impl MetalThreadSafe for MTLSamplerState {}

// SAFETY: the protocols of `MetalThreadSafe` are thread-safe, and
// `release` may run on any thread.
unsafe impl<C: MetalThreadSafe> Send for ThreadSafe<C> {}
// SAFETY: as above, every message to these objects is thread-safe.
unsafe impl<C: MetalThreadSafe> Sync for ThreadSafe<C> {}

impl<C: MetalThreadSafe> ThreadSafe<C> {
    pub(crate) fn new(object: Retained<C>) -> Self {
        Self(object)
    }
}

impl<C: MetalThreadSafe> std::ops::Deref for ThreadSafe<C> {
    type Target = Obj<C>;
    fn deref(&self) -> &Obj<C> {
        &self.0
    }
}

/// A new library from the bytes of a `.metallib` file. Metal copies the
/// bytes.
pub(crate) fn new_library(device: &Obj<MTLDevice>, bytes: &[u8]) -> Option<Retained<MTLLibrary>> {
    // SAFETY: the pointer and the size describe `bytes`. With a null
    // destructor, `dispatch_data_create` copies the bytes before it
    // returns, so the data does not borrow them.
    let data = unsafe {
        dispatch_data_create(
            bytes.as_ptr().cast(),
            bytes.len(),
            std::ptr::null(),
            std::ptr::null(),
        )
    }?;
    NEW_LIBRARY_WITH_DATA.send(device, (&data, std::ptr::null()))
}

/// The function `name` of `library`.
pub(crate) fn new_function(library: &Obj<MTLLibrary>, name: &str) -> Option<Retained<MTLFunction>> {
    let name = ns_string(name).ok()?;
    NEW_FUNCTION.send(library, (&name,))
}

/// A new pipeline state, or `None` when Metal rejects the descriptor.
pub(crate) fn new_pipeline(
    device: &Obj<MTLDevice>,
    descriptor: &Obj<MTLRenderPipelineDescriptor>,
) -> Option<Retained<MTLRenderPipelineState>> {
    NEW_RENDER_PIPELINE.send(device, (descriptor, std::ptr::null()))
}

/// A new shared buffer with a copy of `words`. An empty slice makes a
/// buffer of one zero word, because Metal rejects a zero length.
pub(crate) fn new_buffer(device: &Obj<MTLDevice>, words: &[u32]) -> Option<Retained<MTLBuffer>> {
    let zero = [0u32];
    let words = if words.is_empty() { &zero[..] } else { words };
    NEW_BUFFER_WITH_BYTES.send(
        device,
        (
            words.as_ptr().cast(),
            std::mem::size_of_val(words),
            RESOURCE_STORAGE_SHARED,
        ),
    )
}

/// Sets the vertex constant at `index` to a copy of `values`.
pub(crate) fn set_vertex_floats(
    encoder: &Obj<MTLRenderCommandEncoder>,
    values: &[f32],
    index: usize,
) {
    SET_VERTEX_BYTES.send(
        encoder,
        (values.as_ptr().cast(), std::mem::size_of_val(values), index),
    );
}

/// Sets the fragment constant at `index` to a copy of `values`.
pub(crate) fn set_fragment_ints(
    encoder: &Obj<MTLRenderCommandEncoder>,
    values: &[i32],
    index: usize,
) {
    SET_FRAGMENT_BYTES.send(
        encoder,
        (values.as_ptr().cast(), std::mem::size_of_val(values), index),
    );
}

/// Copies `bytes` into `region` of `texture`, whose pixels are
/// `bytes_per_pixel` bytes. Returns `false`, and copies nothing, when
/// `bytes` is shorter than the region or the region leaves the texture.
pub(crate) fn replace_region(
    texture: &Obj<MTLTexture>,
    region: MTLRegion,
    bytes: &[u8],
    bytes_per_row: usize,
    bytes_per_pixel: usize,
) -> bool {
    let (origin, size) = (region.origin, region.size);
    let fits = origin.x.checked_add(size.width) <= Some(TEXTURE_WIDTH.send(texture, ()))
        && origin.y.checked_add(size.height) <= Some(TEXTURE_HEIGHT.send(texture, ()))
        && size.depth == 1
        && origin.z == 0;
    let row = size.width.checked_mul(bytes_per_pixel);
    let needed = bytes_per_row.checked_mul(size.height);
    if !fits || row.is_none_or(|row| row > bytes_per_row) || needed.is_none_or(|n| n > bytes.len())
    {
        return false;
    }
    REPLACE_REGION.send(texture, (region, 0, bytes.as_ptr().cast(), bytes_per_row));
    true
}

/// A copy of `region` of `texture`, whose pixels are `bytes_per_pixel`
/// bytes, in rows without padding. Returns `None` when the region leaves
/// the texture. Call it only for a texture that no GPU work writes.
pub(crate) fn read_region(
    texture: &Obj<MTLTexture>,
    region: MTLRegion,
    bytes_per_pixel: usize,
) -> Option<Vec<u8>> {
    let (origin, size) = (region.origin, region.size);
    let fits = origin.x.checked_add(size.width) <= Some(TEXTURE_WIDTH.send(texture, ()))
        && origin.y.checked_add(size.height) <= Some(TEXTURE_HEIGHT.send(texture, ()))
        && size.depth == 1
        && origin.z == 0;
    let row = size.width.checked_mul(bytes_per_pixel)?;
    let length = row.checked_mul(size.height)?;
    if !fits {
        return None;
    }
    let mut bytes = vec![0u8; length];
    // `getBytes` writes `row` bytes for each of the `size.height` rows,
    // which is the length of `bytes`.
    GET_BYTES.send(texture, (bytes.as_mut_ptr().cast(), row, region, 0));
    Some(bytes)
}

/// A copy of the first `length` bytes of a shared buffer, or `None` when
/// the buffer is shorter. Call it after the GPU work that writes the
/// buffer completes.
pub(crate) fn read_buffer(buffer: &Obj<MTLBuffer>, length: usize) -> Option<Vec<u8>> {
    if BUFFER_LENGTH.send(buffer, ()) < length {
        return None;
    }
    let contents = BUFFER_CONTENTS.send(buffer, ()).cast::<u8>();
    if contents.is_null() {
        return None;
    }
    // SAFETY: a shared buffer of at least `length` bytes starts at
    // `contents`. The GPU work that wrote it has completed, and nothing
    // writes it while the bytes are copied.
    let bytes = unsafe { std::slice::from_raw_parts(contents, length) };
    Some(bytes.to_vec())
}

/// A completion block for `addCompletedHandler:` that calls `H`.
pub(crate) fn completed_block<H: ObjectHandler<MTLCommandBuffer>>() -> GlobalBlock<CommandBufferArgs>
{
    GlobalBlock::object::<H>()
}

/// A presented block for `addPresentedHandler:` that calls `H`.
pub(crate) fn presented_block<H: ObjectHandler<MTLDrawable>>() -> GlobalBlock<DrawableArgs> {
    GlobalBlock::object::<H>()
}
