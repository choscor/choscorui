//! Text for the Cairo painter. A glyph run names its face; the painter
//! opens a Cairo font face on the same font file and draws each glyph at
//! the position that the shaper gave it.

use crate::ffi::cairo::{Context, FontFace, FontOptions, HintStyle};
use crate::ffi::fontconfig::Hinting;
use crate::ffi::freetype::FtFace;
use crate::text::{LinuxTextShaper, fonts};
use choscorui_core::{Color, GlyphRun, Point, TextRun, TextShaper, TextStyle};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

const FACE_CACHE_LIMIT: usize = 64;

/// A Cairo font face and how fontconfig says to hint it.
struct CachedFace {
    face: FontFace,
    hint: HintStyle,
}

thread_local! {
    /// Cairo font faces by run name; `None` records a face that did not
    /// open.
    static FACES: RefCell<HashMap<String, Option<Rc<CachedFace>>>> = RefCell::new(HashMap::new());
    /// Font options for each hint style.
    static OPTIONS: RefCell<HashMap<HintStyle, Rc<FontOptions>>> = RefCell::new(HashMap::new());
}

fn hint_style(hinting: Hinting) -> HintStyle {
    match hinting {
        Hinting::None => HintStyle::None,
        Hinting::Slight => HintStyle::Slight,
        Hinting::Medium => HintStyle::Medium,
        Hinting::Full => HintStyle::Full,
    }
}

/// The Cairo face of a run name.
fn face(name: &str) -> Option<Rc<CachedFace>> {
    if let Some(face) = FACES.with(|faces| faces.borrow().get(name).cloned()) {
        return face;
    }
    let face = fonts::face_source(name).and_then(|source| {
        let ft = FtFace::open(&source.file, source.index)?;
        Some(Rc::new(CachedFace {
            face: FontFace::from_ft_face(ft)?,
            hint: hint_style(source.hinting),
        }))
    });
    FACES.with(|faces| {
        let mut faces = faces.borrow_mut();
        if faces.len() >= FACE_CACHE_LIMIT {
            faces.clear();
        }
        faces.insert(name.to_owned(), face.clone());
    });
    face
}

fn options(hint: HintStyle) -> Option<Rc<FontOptions>> {
    if let Some(options) = OPTIONS.with(|cache| cache.borrow().get(&hint).cloned()) {
        return Some(options);
    }
    let options = Rc::new(FontOptions::new(hint)?);
    OPTIONS.with(|cache| cache.borrow_mut().insert(hint, Rc::clone(&options)));
    Some(options)
}

/// Draws a pre-shaped run with its baseline origin at `origin`. Returns
/// false when its face does not open.
pub(super) fn draw_run(cx: &Context<'_>, origin: Point, run: &GlyphRun, color: Color) -> bool {
    if run.glyphs.is_empty() {
        return true;
    }
    let Some(face) = face(&run.font) else {
        return false;
    };
    let Some(options) = options(face.hint) else {
        return false;
    };
    let glyphs: Vec<(u32, f64, f64)> = run
        .glyphs
        .iter()
        .zip(&run.positions)
        .map(|(glyph, position)| {
            (
                u32::from(*glyph),
                origin.x + position.x,
                origin.y + position.y,
            )
        })
        .collect();
    super::set_color(cx, color);
    cx.show_glyphs(&face.face, run.size, &options, &glyphs);
    true
}

/// Shapes one line of text in the font of `run` and draws it with its
/// first baseline at `baseline`, as `TextLayout::draw` would place it.
pub(super) fn draw_text(
    cx: &Context<'_>,
    baseline: Point,
    text: &str,
    run: &TextRun<'_>,
    color: Color,
) -> bool {
    let style = TextStyle::new(run.family, run.size)
        .with_weight(run.weight)
        .with_slant(run.slant)
        .with_tracking(run.tracking);
    let Ok(layout) = LinuxTextShaper.shape(text, &style, None) else {
        return false;
    };
    let top = baseline.y - layout.first_baseline();
    let mut drawn = true;
    for line in layout.lines() {
        let origin = Point::new(baseline.x + line.offset_x, top + line.baseline);
        for glyph_run in &line.runs {
            drawn &= draw_run(cx, origin, glyph_run, color);
        }
    }
    drawn
}
