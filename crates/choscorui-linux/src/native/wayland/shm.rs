//! Double-buffered `wl_shm` buffers of a window.
//!
//! Each buffer is a memfd with its own pool. A buffer is busy from its
//! attach until the compositor sends `wl_buffer.release`. Each buffer
//! keeps the region that changed since it was last painted, so a frame
//! repaints only that region plus the new damage.

use crate::ffi::libc::SharedMemory;
use crate::ffi::wayland::{Arg, ObjectId, Proxy, WL_SHM_POOL};
use choscorui_core::Rect;

/// `WL_SHM_FORMAT_ARGB8888`.
const FORMAT_ARGB8888: u32 = 0;
const SHM_CREATE_POOL: u16 = 0;
const POOL_CREATE_BUFFER: u16 = 0;
const POOL_DESTROY: u16 = 1;

pub(crate) struct Buffer {
    pub(crate) memory: SharedMemory,
    pub(crate) proxy: Proxy,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) busy: bool,
    /// The logical region that changed since this buffer was painted, or
    /// None when the buffer is current. A new buffer is stale everywhere.
    pub(crate) stale: Option<Rect>,
}

impl Buffer {
    pub(crate) fn stride(&self) -> usize {
        self.width as usize * 4
    }
}

/// The buffers of a window.
#[derive(Default)]
pub(crate) struct Buffers {
    buffers: Vec<Buffer>,
}

impl Buffers {
    /// A free buffer of `width` x `height` pixels, creating one when no
    /// free buffer has that size. Returns None when memory or the
    /// protocol fails. At most two buffers exist at a time; a third
    /// request while both are busy returns None.
    pub(crate) fn acquire(
        &mut self,
        shm: &Proxy,
        width: u32,
        height: u32,
        everything: Rect,
    ) -> Option<&mut Buffer> {
        // Drop free buffers of another size.
        self.buffers
            .retain(|buffer| buffer.busy || (buffer.width == width && buffer.height == height));
        if let Some(index) = self
            .buffers
            .iter()
            .position(|buffer| !buffer.busy && buffer.width == width)
        {
            return self.buffers.get_mut(index);
        }
        if self.buffers.len() >= 2 {
            return None;
        }
        let buffer = create(shm, width, height, everything)?;
        self.buffers.push(buffer);
        self.buffers.last_mut()
    }

    /// Adds `region` to the stale region of every buffer but `except`.
    pub(crate) fn mark_stale(&mut self, region: Rect, except: ObjectId) {
        for buffer in &mut self.buffers {
            if buffer.proxy.id() != except {
                buffer.stale = Some(match buffer.stale {
                    Some(stale) => stale.union(region),
                    None => region,
                });
            }
        }
    }

    /// Handles `wl_buffer.release`. Returns true when `id` is a buffer of
    /// this window.
    pub(crate) fn release(&mut self, id: ObjectId) -> bool {
        match self
            .buffers
            .iter_mut()
            .find(|buffer| buffer.proxy.id() == id)
        {
            Some(buffer) => {
                buffer.busy = false;
                true
            }
            None => false,
        }
    }

    /// Whether any buffer is free or another can be created.
    pub(crate) fn has_free(&self) -> bool {
        self.buffers.len() < 2 || self.buffers.iter().any(|buffer| !buffer.busy)
    }

    pub(crate) fn clear(&mut self) {
        self.buffers.clear();
    }
}

fn create(shm: &Proxy, width: u32, height: u32, everything: Rect) -> Option<Buffer> {
    let stride = width.checked_mul(4)?;
    let length = usize::try_from(stride).ok()?.checked_mul(height as usize)?;
    if length == 0 {
        return None;
    }
    let memory = SharedMemory::new(c"choscorui-buffer", length).ok()?;
    let size = i32::try_from(length).ok()?;
    let pool = shm
        .request(
            SHM_CREATE_POOL,
            &[
                Arg::NewId,
                Arg::Fd(std::os::fd::AsFd::as_fd(&memory)),
                Arg::Int(size),
            ],
        )
        .ok()??;
    debug_assert_eq!(pool.interface().name, WL_SHM_POOL.name);
    let proxy = pool
        .request(
            POOL_CREATE_BUFFER,
            &[
                Arg::NewId,
                Arg::Int(0),
                Arg::Int(i32::try_from(width).ok()?),
                Arg::Int(i32::try_from(height).ok()?),
                Arg::Int(i32::try_from(stride).ok()?),
                Arg::Uint(FORMAT_ARGB8888),
            ],
        )
        .ok()??;
    // The buffer keeps the memory of the pool alive on the compositor
    // side, so the pool can go.
    let _ = pool.destroy(POOL_DESTROY, &[]);
    Some(Buffer {
        memory,
        proxy,
        width,
        height,
        busy: false,
        stale: Some(everything),
    })
}
