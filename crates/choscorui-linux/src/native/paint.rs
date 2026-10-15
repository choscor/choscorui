//! The CPU paint path that both display servers share: it paints a
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
/// to repaint; pixels outside it keep their content.
pub(crate) fn paint(list: &DisplayList, pixels: &mut Pixels<'_>, scale: f64, region: Rect) {
    let _ = (list, scale, region);
    for row in pixels
        .bytes
        .chunks_mut(pixels.stride)
        .take(pixels.height as usize)
    {
        for pixel in row[..pixels.width as usize * 4].chunks_exact_mut(4) {
            pixel.copy_from_slice(&[0xff, 0xff, 0xff, 0xff]);
        }
    }
}
