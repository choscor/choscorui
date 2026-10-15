//! The CPU paint path that both display servers share: Cairo paints a
//! display list into a premultiplied ARGB32 pixel buffer.

use choscorui_core::{DisplayList, Rect};

/// A pixel buffer in premultiplied ARGB32, native byte order: the format
/// of `WL_SHM_FORMAT_ARGB8888` and of 32-bit X11 images on little-endian
/// machines.
pub(crate) struct Pixels<'a> {
    pub(crate) bytes: &'a mut [u8],
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) stride: usize,
}

/// Paints `list` at `scale` into `pixels`. `region` is the logical region
/// to repaint; pixels outside it keep their content. A failure is
/// reported on stderr and leaves the region cleared, so the window stays
/// usable.
pub(crate) fn paint(list: &DisplayList, pixels: &mut Pixels<'_>, scale: f64, region: Rect) {
    let result = crate::paint::paint_into(
        list,
        pixels.bytes,
        pixels.width,
        pixels.height,
        pixels.stride,
        scale,
        Some(region),
    );
    if let Err(error) = result {
        eprintln!("choscorui: painting failed: {error}");
    }
}
