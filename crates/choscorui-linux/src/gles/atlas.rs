//! The glyph atlas: one coverage texture with the glyphs that Cairo
//! rasterized, packed in shelves.
//!
//! A glyph is keyed by its run's face, size, and device matrix, and by
//! the quarter-pixel phase of its device position. Cairo rasterizes it
//! at the middle of the phase, so the glyph lands on the pixel that
//! Cairo's own rounding picks. When the atlas is full, it starts over.

use crate::ffi::gles::{Gl, Texture, TextureFormat};
use crate::paint::mask::Mask;
use std::collections::HashMap;

/// The phases of a pixel in each direction.
pub(super) const PHASES: f64 = 4.0;

/// The face, the size, and the linear part of the device matrix of a
/// run, as bits.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(super) struct RunKey {
    pub(super) font: String,
    pub(super) size: u64,
    pub(super) matrix: [u64; 4],
}

/// A glyph and its phase in x and y.
type GlyphKey = (u16, u8, u8);

/// Where a glyph is in the atlas, and the offset of its top-left pixel
/// from the pixel of its origin.
#[derive(Clone, Copy, Debug)]
pub(super) struct Slot {
    pub(super) x: u32,
    pub(super) y: u32,
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) left: i32,
    pub(super) top: i32,
}

/// A row of glyphs of at most `height` pixels.
struct Shelf {
    y: u32,
    height: u32,
    /// The next free column.
    x: u32,
}

pub(super) struct Atlas {
    pub(super) texture: Texture,
    pub(super) size: u32,
    shelves: Vec<Shelf>,
    /// `None` records a glyph without ink.
    glyphs: HashMap<RunKey, HashMap<GlyphKey, Option<Slot>>>,
}

/// The atlas has no room for a glyph; it gives the coverage back.
pub(super) struct Full(pub(super) Mask);

impl Atlas {
    pub(super) fn new(gl: &Gl, size: u32) -> Option<Self> {
        Some(Self {
            texture: gl.texture(TextureFormat::R8, size, size, None, false)?,
            size,
            shelves: Vec::new(),
            glyphs: HashMap::new(),
        })
    }

    /// The slot of a glyph that the atlas has, or `None` when it has not
    /// rasterized the glyph yet.
    pub(super) fn get(&self, run: &RunKey, glyph: GlyphKey) -> Option<Option<Slot>> {
        self.glyphs.get(run)?.get(&glyph).copied()
    }

    /// Drops every glyph. Draws that use the atlas must be issued first.
    pub(super) fn clear(&mut self) {
        self.shelves.clear();
        self.glyphs.clear();
    }

    /// Adds the coverage of a glyph, with `(left, top)` the pixel of its
    /// origin in the mask's coordinates. `None` is a glyph without ink.
    pub(super) fn insert(
        &mut self,
        gl: &Gl,
        run: &RunKey,
        glyph: GlyphKey,
        mask: Option<Mask>,
        (left, top): (i32, i32),
    ) -> Result<Option<Slot>, Full> {
        let slot = match mask {
            None => None,
            Some(mask) => {
                let Some((x, y)) = self.allocate(mask.width, mask.height) else {
                    return Err(Full(mask));
                };
                gl.update_texture(
                    self.texture,
                    TextureFormat::R8,
                    [x, y, mask.width, mask.height],
                    &mask.coverage,
                );
                Some(Slot {
                    x,
                    y,
                    width: mask.width,
                    height: mask.height,
                    left: mask.x - left,
                    top: mask.y - top,
                })
            }
        };
        if let Some(glyphs) = self.glyphs.get_mut(run) {
            glyphs.insert(glyph, slot);
        } else {
            self.glyphs
                .insert(run.clone(), HashMap::from([(glyph, slot)]));
        }
        Ok(slot)
    }

    /// The top-left corner of a free `width` × `height` area, with one
    /// pixel of space around it.
    fn allocate(&mut self, width: u32, height: u32) -> Option<(u32, u32)> {
        let (width, height) = (width + 1, height + 1);
        if width > self.size || height > self.size {
            return None;
        }
        let size = self.size;
        // The lowest shelf that fits wastes the least height.
        if let Some(shelf) = self
            .shelves
            .iter_mut()
            .filter(|shelf| shelf.height >= height && shelf.x + width <= size)
            .min_by_key(|shelf| shelf.height)
        {
            let x = shelf.x;
            shelf.x += width;
            return Some((x, shelf.y));
        }
        let y = self
            .shelves
            .last()
            .map_or(0, |shelf| shelf.y + shelf.height);
        if y + height > size {
            return None;
        }
        self.shelves.push(Shelf {
            y,
            height,
            x: width,
        });
        Some((0, y))
    }
}
