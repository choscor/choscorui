//! A safe interface to libGLESv2: the OpenGL ES 3.0 calls of the GLES
//! painter.
//!
//! A call goes to the context that is current on the thread; without
//! one, libglvnd ignores it. Each method checks the lengths of the
//! slices that it passes, so a call never reads or writes past them.
//! Objects are plain names: the painter deletes them, or the context
//! frees them when it closes.

use super::ctypes::{CChar, c_void};
use super::dl::LoadError;
use super::macros::{c_consts, libraries};
use std::ffi::CStr;

type GlEnum = u32;
type GlUint = u32;
type GlInt = i32;
type GlSizei = i32;
type GlBoolean = u8;
type GlBitfield = u32;
type GlFloat = f32;
type GlSizeiptr = isize;

libraries! {
    /// libGLESv2: OpenGL ES 3.0.
    pub(crate) struct GlesV2 {
        library: "libGLESv2",
        sonames: ["libGLESv2.so.2"],
        headers: ["GLES3/gl3.h"],
        pkg_config: "glesv2",
        package: "libgles-dev",
        functions {
            fn glGetError() -> GlEnum;
            fn glGetIntegerv(pname: GlEnum, data: *mut GlInt);
            fn glViewport(x: GlInt, y: GlInt, width: GlSizei, height: GlSizei);
            fn glScissor(x: GlInt, y: GlInt, width: GlSizei, height: GlSizei);
            fn glEnable(cap: GlEnum);
            fn glDisable(cap: GlEnum);
            fn glBlendFunc(sfactor: GlEnum, dfactor: GlEnum);
            fn glClearColor(red: GlFloat, green: GlFloat, blue: GlFloat, alpha: GlFloat);
            fn glClear(mask: GlBitfield);
            fn glCreateShader(kind: GlEnum) -> GlUint;
            fn glShaderSource(
                shader: GlUint,
                count: GlSizei,
                string: *const *const CChar,
                length: *const GlInt,
            );
            fn glCompileShader(shader: GlUint);
            fn glGetShaderiv(shader: GlUint, pname: GlEnum, params: *mut GlInt);
            fn glGetShaderInfoLog(
                shader: GlUint,
                buf_size: GlSizei,
                length: *mut GlSizei,
                info_log: *mut CChar,
            );
            fn glDeleteShader(shader: GlUint);
            fn glCreateProgram() -> GlUint;
            fn glAttachShader(program: GlUint, shader: GlUint);
            fn glBindAttribLocation(program: GlUint, index: GlUint, name: *const CChar);
            fn glLinkProgram(program: GlUint);
            fn glGetProgramiv(program: GlUint, pname: GlEnum, params: *mut GlInt);
            fn glGetProgramInfoLog(
                program: GlUint,
                buf_size: GlSizei,
                length: *mut GlSizei,
                info_log: *mut CChar,
            );
            fn glUseProgram(program: GlUint);
            fn glGetUniformLocation(program: GlUint, name: *const CChar) -> GlInt;
            fn glUniform1i(location: GlInt, v0: GlInt);
            fn glUniform2f(location: GlInt, v0: GlFloat, v1: GlFloat);
            fn glUniform4f(location: GlInt, v0: GlFloat, v1: GlFloat, v2: GlFloat, v3: GlFloat);
            fn glUniformMatrix3fv(
                location: GlInt,
                count: GlSizei,
                transpose: GlBoolean,
                value: *const GlFloat,
            );
            fn glGenBuffers(n: GlSizei, buffers: *mut GlUint);
            fn glBindBuffer(target: GlEnum, buffer: GlUint);
            fn glBufferData(target: GlEnum, size: GlSizeiptr, data: *const c_void, usage: GlEnum);
            fn glGenVertexArrays(n: GlSizei, arrays: *mut GlUint);
            fn glBindVertexArray(array: GlUint);
            fn glEnableVertexAttribArray(index: GlUint);
            fn glVertexAttribPointer(
                index: GlUint,
                size: GlInt,
                kind: GlEnum,
                normalized: GlBoolean,
                stride: GlSizei,
                pointer: *const c_void,
            );
            fn glDrawArrays(mode: GlEnum, first: GlInt, count: GlSizei);
            fn glGenTextures(n: GlSizei, textures: *mut GlUint);
            fn glDeleteTextures(n: GlSizei, textures: *const GlUint);
            fn glBindTexture(target: GlEnum, texture: GlUint);
            fn glActiveTexture(texture: GlEnum);
            fn glTexParameteri(target: GlEnum, pname: GlEnum, param: GlInt);
            fn glPixelStorei(pname: GlEnum, param: GlInt);
            fn glTexImage2D(
                target: GlEnum,
                level: GlInt,
                internalformat: GlInt,
                width: GlSizei,
                height: GlSizei,
                border: GlInt,
                format: GlEnum,
                kind: GlEnum,
                pixels: *const c_void,
            );
            fn glTexSubImage2D(
                target: GlEnum,
                level: GlInt,
                xoffset: GlInt,
                yoffset: GlInt,
                width: GlSizei,
                height: GlSizei,
                format: GlEnum,
                kind: GlEnum,
                pixels: *const c_void,
            );
            fn glGenFramebuffers(n: GlSizei, framebuffers: *mut GlUint);
            fn glDeleteFramebuffers(n: GlSizei, framebuffers: *const GlUint);
            fn glBindFramebuffer(target: GlEnum, framebuffer: GlUint);
            fn glFramebufferTexture2D(
                target: GlEnum,
                attachment: GlEnum,
                textarget: GlEnum,
                texture: GlUint,
                level: GlInt,
            );
            fn glCheckFramebufferStatus(target: GlEnum) -> GlEnum;
            fn glReadPixels(
                x: GlInt,
                y: GlInt,
                width: GlSizei,
                height: GlSizei,
                format: GlEnum,
                kind: GlEnum,
                pixels: *mut c_void,
            );
        }
    }
}

c_consts! {
    /// The OpenGL ES constants, for the binding check.
    pub(crate) fn constants;
    const GL_FALSE: GlBoolean = 0;
    const GL_NO_ERROR: GlEnum = 0;
    const GL_ONE: GlEnum = 1;
    const GL_TRIANGLES: GlEnum = 0x0004;
    const GL_ONE_MINUS_SRC_ALPHA: GlEnum = 0x0303;
    const GL_BLEND: GlEnum = 0x0BE2;
    const GL_SCISSOR_TEST: GlEnum = 0x0C11;
    const GL_UNPACK_ALIGNMENT: GlEnum = 0x0CF5;
    const GL_PACK_ALIGNMENT: GlEnum = 0x0D05;
    const GL_MAX_TEXTURE_SIZE: GlEnum = 0x0D33;
    const GL_TEXTURE_2D: GlEnum = 0x0DE1;
    const GL_UNSIGNED_BYTE: GlEnum = 0x1401;
    const GL_FLOAT: GlEnum = 0x1406;
    const GL_RED: GlEnum = 0x1903;
    const GL_RGBA: GlEnum = 0x1908;
    const GL_NEAREST: GlEnum = 0x2600;
    const GL_LINEAR: GlEnum = 0x2601;
    const GL_TEXTURE_MAG_FILTER: GlEnum = 0x2800;
    const GL_TEXTURE_MIN_FILTER: GlEnum = 0x2801;
    const GL_TEXTURE_WRAP_S: GlEnum = 0x2802;
    const GL_TEXTURE_WRAP_T: GlEnum = 0x2803;
    const GL_COLOR_BUFFER_BIT: GlBitfield = 0x4000;
    const GL_RGBA8: GlEnum = 0x8058;
    const GL_CLAMP_TO_EDGE: GlEnum = 0x812F;
    const GL_R8: GlEnum = 0x8229;
    const GL_TEXTURE0: GlEnum = 0x84C0;
    const GL_ARRAY_BUFFER: GlEnum = 0x8892;
    const GL_STREAM_DRAW: GlEnum = 0x88E0;
    const GL_FRAGMENT_SHADER: GlEnum = 0x8B30;
    const GL_VERTEX_SHADER: GlEnum = 0x8B31;
    const GL_COMPILE_STATUS: GlEnum = 0x8B81;
    const GL_LINK_STATUS: GlEnum = 0x8B82;
    const GL_INFO_LOG_LENGTH: GlEnum = 0x8B84;
    const GL_FRAMEBUFFER_COMPLETE: GlEnum = 0x8CD5;
    const GL_COLOR_ATTACHMENT0: GlEnum = 0x8CE0;
    const GL_FRAMEBUFFER: GlEnum = 0x8D40;
}

/// A linked program.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Program(GlUint);

/// A uniform location; -1 is a uniform that the program does not use.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Uniform(GlInt);

/// A vertex array object with its vertex buffer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Vertices {
    array: GlUint,
    buffer: GlUint,
    /// The floats of each vertex.
    stride: usize,
}

/// A texture name.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct Texture(GlUint);

/// A framebuffer name.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Framebuffer(GlUint);

/// The pixel format of a texture.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TextureFormat {
    /// Four 8-bit channels, premultiplied in the painter.
    Rgba8,
    /// One 8-bit channel: coverage.
    R8,
}

impl TextureFormat {
    fn bytes(self) -> usize {
        match self {
            Self::Rgba8 => 4,
            Self::R8 => 1,
        }
    }

    fn internal(self) -> GlEnum {
        match self {
            Self::Rgba8 => GL_RGBA8,
            Self::R8 => GL_R8,
        }
    }

    fn format(self) -> GlEnum {
        match self {
            Self::Rgba8 => GL_RGBA,
            Self::R8 => GL_RED,
        }
    }
}

/// How a draw combines with the target.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Blend {
    /// Premultiplied source over: `src + dst × (1 − src.a)`.
    Over,
    /// The source replaces the target.
    Replace,
}

/// The OpenGL ES functions.
#[derive(Clone, Copy)]
pub(crate) struct Gl {
    lib: &'static GlesV2,
}

/// `value` as a GL size, or 0 when it does not fit.
fn sizei(value: usize) -> GlSizei {
    GlSizei::try_from(value).unwrap_or(0)
}

impl Gl {
    pub(crate) fn get() -> Result<Self, LoadError> {
        Ok(Self {
            lib: GlesV2::get()?,
        })
    }

    /// The oldest recorded error, or 0. Reading it clears it.
    pub(crate) fn error(&self) -> u32 {
        // SAFETY: the call reads the context state.
        unsafe { (self.lib.glGetError)() }
    }

    /// The largest texture side.
    pub(crate) fn max_texture_size(&self) -> u32 {
        let mut value = 0;
        // SAFETY: GL_MAX_TEXTURE_SIZE writes one integer to the local.
        unsafe { (self.lib.glGetIntegerv)(GL_MAX_TEXTURE_SIZE, &raw mut value) };
        u32::try_from(value).unwrap_or(0)
    }

    /// Maps the clip space to `width` × `height` pixels.
    pub(crate) fn viewport(&self, width: u32, height: u32) {
        let (width, height) = (sizei(width as usize), sizei(height as usize));
        // SAFETY: the call sets context state.
        unsafe { (self.lib.glViewport)(0, 0, width, height) }
    }

    /// Limits drawing to the pixels of `[x, y, width, height]`, with y up
    /// from the bottom edge, or turns the limit off.
    pub(crate) fn scissor(&self, rect: Option<[i32; 4]>) {
        match rect {
            Some([x, y, width, height]) => {
                // SAFETY: the call sets context state.
                unsafe { (self.lib.glEnable)(GL_SCISSOR_TEST) };
                // SAFETY: as above.
                unsafe { (self.lib.glScissor)(x, y, width.max(0), height.max(0)) };
            }
            // SAFETY: as above.
            None => unsafe { (self.lib.glDisable)(GL_SCISSOR_TEST) },
        }
    }

    pub(crate) fn blend(&self, blend: Blend) {
        match blend {
            Blend::Over => {
                // SAFETY: the call sets context state.
                unsafe { (self.lib.glEnable)(GL_BLEND) };
                // SAFETY: as above.
                unsafe { (self.lib.glBlendFunc)(GL_ONE, GL_ONE_MINUS_SRC_ALPHA) };
            }
            // SAFETY: as above.
            Blend::Replace => unsafe { (self.lib.glDisable)(GL_BLEND) },
        }
    }

    /// Clears the pixels inside the scissor to transparent.
    pub(crate) fn clear(&self) {
        // SAFETY: the calls set context state and clear the target.
        unsafe { (self.lib.glClearColor)(0.0, 0.0, 0.0, 0.0) };
        // SAFETY: as above.
        unsafe { (self.lib.glClear)(GL_COLOR_BUFFER_BIT) };
    }

    /// The info log of a shader or a program.
    fn info_log(
        &self,
        name: GlUint,
        parameter: unsafe extern "C" fn(GlUint, GlEnum, *mut GlInt),
        log: unsafe extern "C" fn(GlUint, GlSizei, *mut GlSizei, *mut CChar),
    ) -> String {
        let mut length = 0;
        // SAFETY: the call writes one integer to the local.
        unsafe { parameter(name, GL_INFO_LOG_LENGTH, &raw mut length) };
        let mut buffer = vec![CChar::default(); usize::try_from(length).unwrap_or(0).max(1)];
        let mut written = 0;
        // SAFETY: the buffer has `buffer.len()` chars, and the call writes
        // at most that many.
        unsafe {
            log(
                name,
                sizei(buffer.len()),
                &raw mut written,
                buffer.as_mut_ptr(),
            )
        };
        let written = usize::try_from(written).unwrap_or(0).min(buffer.len());
        let bytes: Vec<u8> = buffer[..written]
            .iter()
            .map(|c| c.0.to_ne_bytes()[0])
            .collect();
        String::from_utf8_lossy(&bytes).into_owned()
    }

    fn shader(&self, kind: GlEnum, source: &str) -> Result<GlUint, String> {
        // SAFETY: the call creates a shader object.
        let shader = unsafe { (self.lib.glCreateShader)(kind) };
        if shader == 0 {
            return Err("glCreateShader failed".to_owned());
        }
        let pointer = source.as_ptr().cast::<CChar>();
        let length = GlInt::try_from(source.len()).map_err(|_| "the shader is too long")?;
        // SAFETY: one string of `length` bytes, which GL copies.
        unsafe { (self.lib.glShaderSource)(shader, 1, &raw const pointer, &raw const length) };
        // SAFETY: the shader has its source.
        unsafe { (self.lib.glCompileShader)(shader) };
        let mut status = 0;
        // SAFETY: the call writes one integer to the local.
        unsafe { (self.lib.glGetShaderiv)(shader, GL_COMPILE_STATUS, &raw mut status) };
        if status == 0 {
            let log = self.info_log(shader, self.lib.glGetShaderiv, self.lib.glGetShaderInfoLog);
            // SAFETY: the shader is unused.
            unsafe { (self.lib.glDeleteShader)(shader) };
            return Err(log);
        }
        Ok(shader)
    }

    /// Compiles and links a program. Attribute `i` of `attributes` gets
    /// location `i`. The error is the compiler or linker log.
    pub(crate) fn program(
        &self,
        vertex: &str,
        fragment: &str,
        attributes: &[&CStr],
    ) -> Result<Program, String> {
        let vertex = self.shader(GL_VERTEX_SHADER, vertex)?;
        let fragment = match self.shader(GL_FRAGMENT_SHADER, fragment) {
            Ok(fragment) => fragment,
            Err(log) => {
                // SAFETY: the shader is unused.
                unsafe { (self.lib.glDeleteShader)(vertex) };
                return Err(log);
            }
        };
        // SAFETY: the call creates a program object.
        let program = unsafe { (self.lib.glCreateProgram)() };
        // SAFETY: both shaders and the program are live.
        unsafe { (self.lib.glAttachShader)(program, vertex) };
        // SAFETY: as above.
        unsafe { (self.lib.glAttachShader)(program, fragment) };
        for (index, name) in (0..).zip(attributes) {
            // SAFETY: `name` is NUL-terminated.
            unsafe { (self.lib.glBindAttribLocation)(program, index, name.as_ptr().cast()) };
        }
        // SAFETY: the program has its shaders.
        unsafe { (self.lib.glLinkProgram)(program) };
        // SAFETY: the program keeps the shaders while it lives.
        unsafe { (self.lib.glDeleteShader)(vertex) };
        // SAFETY: as above.
        unsafe { (self.lib.glDeleteShader)(fragment) };
        let mut status = 0;
        // SAFETY: the call writes one integer to the local.
        unsafe { (self.lib.glGetProgramiv)(program, GL_LINK_STATUS, &raw mut status) };
        if status == 0 || program == 0 {
            return Err(self.info_log(
                program,
                self.lib.glGetProgramiv,
                self.lib.glGetProgramInfoLog,
            ));
        }
        Ok(Program(program))
    }

    pub(crate) fn uniform(&self, program: Program, name: &CStr) -> Uniform {
        // SAFETY: `name` is NUL-terminated.
        Uniform(unsafe { (self.lib.glGetUniformLocation)(program.0, name.as_ptr().cast()) })
    }

    pub(crate) fn use_program(&self, program: Program) {
        // SAFETY: the call sets context state.
        unsafe { (self.lib.glUseProgram)(program.0) }
    }

    pub(crate) fn set_int(&self, uniform: Uniform, value: i32) {
        // SAFETY: the call sets a uniform of the current program.
        unsafe { (self.lib.glUniform1i)(uniform.0, value) }
    }

    pub(crate) fn set_vec2(&self, uniform: Uniform, [x, y]: [f32; 2]) {
        // SAFETY: the call sets a uniform of the current program.
        unsafe { (self.lib.glUniform2f)(uniform.0, x, y) }
    }

    pub(crate) fn set_vec4(&self, uniform: Uniform, [x, y, z, w]: [f32; 4]) {
        // SAFETY: the call sets a uniform of the current program.
        unsafe { (self.lib.glUniform4f)(uniform.0, x, y, z, w) }
    }

    /// Sets a 3 × 3 matrix, in column-major order.
    pub(crate) fn set_mat3(&self, uniform: Uniform, value: &[f32; 9]) {
        // SAFETY: the array has the nine floats of one matrix.
        unsafe { (self.lib.glUniformMatrix3fv)(uniform.0, 1, GL_FALSE, value.as_ptr()) }
    }

    /// A vertex array whose vertices are `components` floats each, one
    /// attribute for each entry, at locations 0, 1, and on.
    pub(crate) fn vertices(&self, components: &[i32]) -> Option<Vertices> {
        let (mut array, mut buffer) = (0, 0);
        // SAFETY: the call writes one name to the local.
        unsafe { (self.lib.glGenVertexArrays)(1, &raw mut array) };
        // SAFETY: as above.
        unsafe { (self.lib.glGenBuffers)(1, &raw mut buffer) };
        if array == 0 || buffer == 0 {
            return None;
        }
        let stride: i32 = components.iter().sum();
        let stride_bytes = stride * size_of::<f32>() as i32;
        // SAFETY: the calls set context state.
        unsafe { (self.lib.glBindVertexArray)(array) };
        // SAFETY: as above.
        unsafe { (self.lib.glBindBuffer)(GL_ARRAY_BUFFER, buffer) };
        let mut offset = 0;
        for (index, &count) in (0..).zip(components) {
            // SAFETY: as above.
            unsafe { (self.lib.glEnableVertexAttribArray)(index) };
            // A buffer is bound, so the pointer is an offset into it.
            let pointer = (offset as usize * size_of::<f32>()) as *const c_void;
            // SAFETY: the vertex array and its buffer are bound, so GL
            // reads no client memory through the offset.
            unsafe {
                (self.lib.glVertexAttribPointer)(
                    index,
                    count,
                    GL_FLOAT,
                    GL_FALSE,
                    stride_bytes,
                    pointer,
                );
            }
            offset += count;
        }
        // SAFETY: as above.
        unsafe { (self.lib.glBindVertexArray)(0) };
        Some(Vertices {
            array,
            buffer,
            stride: usize::try_from(stride).unwrap_or(0),
        })
    }

    /// Draws triangles from `data`, whole vertices of `vertices`.
    pub(crate) fn draw_triangles(&self, vertices: Vertices, data: &[f32]) {
        let count = data.len().checked_div(vertices.stride).unwrap_or(0);
        if count == 0 {
            return;
        }
        let bytes = count * vertices.stride * size_of::<f32>();
        let Ok(size) = GlSizeiptr::try_from(bytes) else {
            return;
        };
        // SAFETY: the calls set context state.
        unsafe { (self.lib.glBindVertexArray)(vertices.array) };
        // SAFETY: as above.
        unsafe { (self.lib.glBindBuffer)(GL_ARRAY_BUFFER, vertices.buffer) };
        // SAFETY: `data` has at least `bytes` bytes, which GL copies.
        unsafe {
            (self.lib.glBufferData)(GL_ARRAY_BUFFER, size, data.as_ptr().cast(), GL_STREAM_DRAW);
        }
        // SAFETY: the buffer holds `count` whole vertices.
        unsafe { (self.lib.glDrawArrays)(GL_TRIANGLES, 0, sizei(count)) };
        // SAFETY: the call sets context state.
        unsafe { (self.lib.glBindVertexArray)(0) };
    }

    /// Checks that `pixels` holds `width` × `height` pixels of `format`.
    fn fits(format: TextureFormat, width: u32, height: u32, pixels: &[u8]) -> bool {
        (width as usize)
            .checked_mul(height as usize)
            .and_then(|count| count.checked_mul(format.bytes()))
            .is_some_and(|bytes| pixels.len() >= bytes)
    }

    /// A clamped texture of `width` × `height`, filled with `pixels` in
    /// rows from the first texture row, or undefined without them.
    pub(crate) fn texture(
        &self,
        format: TextureFormat,
        width: u32,
        height: u32,
        pixels: Option<&[u8]>,
        linear: bool,
    ) -> Option<Texture> {
        if width == 0
            || height == 0
            || pixels.is_some_and(|pixels| !Self::fits(format, width, height, pixels))
        {
            return None;
        }
        let mut name = 0;
        // SAFETY: the call writes one name to the local.
        unsafe { (self.lib.glGenTextures)(1, &raw mut name) };
        if name == 0 {
            return None;
        }
        let texture = Texture(name);
        self.bind_texture(0, Some(texture));
        for (parameter, value) in [
            (GL_TEXTURE_WRAP_S, GL_CLAMP_TO_EDGE),
            (GL_TEXTURE_WRAP_T, GL_CLAMP_TO_EDGE),
        ] {
            // SAFETY: the call sets state of the bound texture.
            unsafe { (self.lib.glTexParameteri)(GL_TEXTURE_2D, parameter, value as GlInt) };
        }
        self.set_filter(texture, linear);
        // SAFETY: rows are packed; the painter never binds an unpack
        // buffer, so `pixels` is client memory.
        unsafe { (self.lib.glPixelStorei)(GL_UNPACK_ALIGNMENT, 1) };
        let data = pixels.map_or(std::ptr::null(), |pixels| pixels.as_ptr().cast());
        // SAFETY: `data` is null or holds the whole image (checked above).
        unsafe {
            (self.lib.glTexImage2D)(
                GL_TEXTURE_2D,
                0,
                format.internal() as GlInt,
                sizei(width as usize),
                sizei(height as usize),
                0,
                format.format(),
                GL_UNSIGNED_BYTE,
                data,
            );
        }
        Some(texture)
    }

    /// Replaces the `[x, y, width, height]` pixels of `texture`.
    pub(crate) fn update_texture(
        &self,
        texture: Texture,
        format: TextureFormat,
        [x, y, width, height]: [u32; 4],
        pixels: &[u8],
    ) {
        if width == 0 || height == 0 || !Self::fits(format, width, height, pixels) {
            return;
        }
        self.bind_texture(0, Some(texture));
        // SAFETY: as in `texture`.
        unsafe { (self.lib.glPixelStorei)(GL_UNPACK_ALIGNMENT, 1) };
        // SAFETY: `pixels` holds the whole rectangle (checked above).
        unsafe {
            (self.lib.glTexSubImage2D)(
                GL_TEXTURE_2D,
                0,
                sizei(x as usize),
                sizei(y as usize),
                sizei(width as usize),
                sizei(height as usize),
                format.format(),
                GL_UNSIGNED_BYTE,
                pixels.as_ptr().cast(),
            );
        }
    }

    /// Samples `texture` with bilinear (`linear`) or nearest filtering.
    pub(crate) fn set_filter(&self, texture: Texture, linear: bool) {
        self.bind_texture(0, Some(texture));
        let filter = if linear { GL_LINEAR } else { GL_NEAREST };
        for parameter in [GL_TEXTURE_MIN_FILTER, GL_TEXTURE_MAG_FILTER] {
            // SAFETY: the call sets state of the bound texture.
            unsafe { (self.lib.glTexParameteri)(GL_TEXTURE_2D, parameter, filter as GlInt) };
        }
    }

    /// Binds `texture` to texture unit `unit`.
    pub(crate) fn bind_texture(&self, unit: u32, texture: Option<Texture>) {
        // SAFETY: the calls set context state.
        unsafe { (self.lib.glActiveTexture)(GL_TEXTURE0 + unit) };
        // SAFETY: as above.
        unsafe { (self.lib.glBindTexture)(GL_TEXTURE_2D, texture.map_or(0, |texture| texture.0)) };
    }

    pub(crate) fn delete_texture(&self, texture: Texture) {
        // SAFETY: one name from the local.
        unsafe { (self.lib.glDeleteTextures)(1, &raw const texture.0) }
    }

    /// A framebuffer that draws into `texture`, or `None` when the texture
    /// cannot be a color target.
    pub(crate) fn framebuffer(&self, texture: Texture) -> Option<Framebuffer> {
        let mut name = 0;
        // SAFETY: the call writes one name to the local.
        unsafe { (self.lib.glGenFramebuffers)(1, &raw mut name) };
        if name == 0 {
            return None;
        }
        let framebuffer = Framebuffer(name);
        self.bind_framebuffer(Some(framebuffer));
        // SAFETY: the framebuffer is bound, and the texture is live.
        unsafe {
            (self.lib.glFramebufferTexture2D)(
                GL_FRAMEBUFFER,
                GL_COLOR_ATTACHMENT0,
                GL_TEXTURE_2D,
                texture.0,
                0,
            );
        }
        // SAFETY: the call reads context state.
        let status = unsafe { (self.lib.glCheckFramebufferStatus)(GL_FRAMEBUFFER) };
        if status != GL_FRAMEBUFFER_COMPLETE {
            self.bind_framebuffer(None);
            self.delete_framebuffer(framebuffer);
            return None;
        }
        Some(framebuffer)
    }

    pub(crate) fn delete_framebuffer(&self, framebuffer: Framebuffer) {
        // SAFETY: one name from the local.
        unsafe { (self.lib.glDeleteFramebuffers)(1, &raw const framebuffer.0) }
    }

    /// Draws into `framebuffer`, or into the window surface.
    pub(crate) fn bind_framebuffer(&self, framebuffer: Option<Framebuffer>) {
        let name = framebuffer.map_or(0, |framebuffer| framebuffer.0);
        // SAFETY: the call sets context state.
        unsafe { (self.lib.glBindFramebuffer)(GL_FRAMEBUFFER, name) }
    }

    /// Reads the RGBA8 pixels of `[x, y, width, height]` of the bound
    /// framebuffer, rows from the bottom up, into `out`.
    pub(crate) fn read_pixels(&self, [x, y, width, height]: [u32; 4], out: &mut Vec<u8>) {
        out.clear();
        let Some(bytes) = (width as usize)
            .checked_mul(height as usize)
            .and_then(|count| count.checked_mul(4))
        else {
            return;
        };
        out.resize(bytes, 0);
        if bytes == 0 {
            return;
        }
        // SAFETY: rows are packed, and no pack buffer is bound.
        unsafe { (self.lib.glPixelStorei)(GL_PACK_ALIGNMENT, 1) };
        // SAFETY: `out` has room for the whole rectangle.
        unsafe {
            (self.lib.glReadPixels)(
                sizei(x as usize),
                sizei(y as usize),
                sizei(width as usize),
                sizei(height as usize),
                GL_RGBA,
                GL_UNSIGNED_BYTE,
                out.as_mut_ptr().cast(),
            );
        }
    }
}

/// True when `error` is not `GL_NO_ERROR`.
pub(crate) fn failed(error: u32) -> bool {
    error != GL_NO_ERROR
}
