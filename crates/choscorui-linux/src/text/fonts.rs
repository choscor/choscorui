//! Font resolution for the shaper and the painter.
//!
//! - A generic family resolves first: `system-ui` is the desktop font when
//!   the backend knows it, else `sans-serif`. `ui-monospace`, `ui-serif`,
//!   and `ui-sans-serif` are the fontconfig aliases.
//! - fontconfig matches the primary face of a style. The fallback
//!   cascade is the families of the style, then the fontconfig sort list.
//!   Each grapheme cluster uses the first face that has all its
//!   characters.
//! - A face has a unique name, which a `GlyphRun` carries. The registry
//!   maps the name back to the font file, so the painter draws a run with
//!   the face that shaped it.
//!
//! Faces, chains, and fallback choices are cached for each thread.

use crate::ffi::fontconfig::{self, FontMatch, FontQuery, Hinting};
use crate::ffi::freetype::FtFace;
use crate::ffi::harfbuzz::HbFont;
use choscorui_core::{FontSlant, TextStyle};
use std::cell::{OnceCell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{Mutex, RwLock};

/// The family of `system-ui` when the backend reads no desktop font.
const DEFAULT_UI_FAMILY: &str = "sans-serif";

/// The desktop font family, from the settings portal or XSETTINGS.
static DESKTOP_FAMILY: RwLock<Option<String>> = RwLock::new(None);

/// Sets the family of `system-ui` from a desktop font name such as
/// `Cantarell 11` or `Noto Sans Bold 10`. The size and the style words
/// are not part of the family. `None` restores `sans-serif`.
pub fn set_desktop_font(name: Option<&str>) {
    let family = name.and_then(desktop_family);
    let mut slot = DESKTOP_FAMILY
        .write()
        .unwrap_or_else(|poison| poison.into_inner());
    *slot = family;
}

/// The family part of a desktop font name.
fn desktop_family(name: &str) -> Option<String> {
    const STYLES: &[&str] = &[
        "thin",
        "light",
        "regular",
        "book",
        "medium",
        "semi-bold",
        "semibold",
        "demi-bold",
        "bold",
        "heavy",
        "black",
        "italic",
        "oblique",
        "condensed",
    ];
    let mut words: Vec<&str> = name.split_whitespace().collect();
    if words.last().is_some_and(|word| word.parse::<f64>().is_ok()) {
        words.pop();
    }
    while words.len() > 1
        && words
            .last()
            .is_some_and(|word| STYLES.contains(&word.to_ascii_lowercase().as_str()))
    {
        words.pop();
    }
    let family = words.join(" ");
    (!family.is_empty()).then_some(family)
}

/// The fontconfig family for a style family.
pub(crate) fn resolve_family(family: &str) -> String {
    match family {
        "system-ui" => DESKTOP_FAMILY
            .read()
            .ok()
            .and_then(|slot| slot.clone())
            .unwrap_or_else(|| DEFAULT_UI_FAMILY.to_owned()),
        "ui-monospace" => "monospace".to_owned(),
        "ui-serif" => "serif".to_owned(),
        "ui-sans-serif" => "sans-serif".to_owned(),
        other => other.to_owned(),
    }
}

/// Where the face of a run name is.
#[derive(Clone, Debug)]
pub(crate) struct FaceSource {
    pub(crate) file: String,
    pub(crate) index: i64,
    pub(crate) hinting: Hinting,
}

/// The face of each run name, for every thread.
static REGISTRY: Mutex<Option<HashMap<String, FaceSource>>> = Mutex::new(None);

/// A unique run name for the face at `file` and `index`.
fn register(postscript: Option<String>, font: &FontMatch) -> String {
    let mut registry = REGISTRY.lock().unwrap_or_else(|poison| poison.into_inner());
    let registry = registry.get_or_insert_with(HashMap::new);
    let base = postscript
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| font.family.clone());
    let mut name = base.clone();
    let mut suffix = 1;
    while let Some(known) = registry.get(&name) {
        if known.file == font.file && known.index == font.index {
            return name;
        }
        suffix += 1;
        name = format!("{base}#{suffix}");
    }
    registry.insert(
        name.clone(),
        FaceSource {
            file: font.file.clone(),
            index: font.index,
            hinting: font.hinting,
        },
    );
    name
}

/// The face of a run name: a face that this process shaped, or else the
/// face that fontconfig finds by PostScript name.
pub(crate) fn face_source(name: &str) -> Option<FaceSource> {
    let known = REGISTRY
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .as_ref()
        .and_then(|registry| registry.get(name).cloned());
    known.or_else(|| {
        let font = fontconfig::match_postscript_name(name)?;
        Some(FaceSource {
            file: font.file,
            index: font.index,
            hinting: font.hinting,
        })
    })
}

/// A face at one size, with its vertical metrics in points.
pub(crate) struct SizedFont {
    pub(crate) hb: HbFont,
    pub(crate) ascent: f64,
    pub(crate) descent: f64,
    pub(crate) leading: f64,
}

/// A loaded face.
pub(crate) struct Face {
    /// The unique run name.
    pub(crate) name: String,
    font: FontMatch,
    parent: HbFont,
    sizes: RefCell<HashMap<u64, Rc<SizedFont>>>,
}

impl Face {
    /// The face at `size` points.
    pub(crate) fn sized(&self, size: f64) -> Option<Rc<SizedFont>> {
        let mut sizes = self.sizes.borrow_mut();
        if let Some(font) = sizes.get(&size.to_bits()) {
            return Some(Rc::clone(font));
        }
        if sizes.len() >= SIZE_CACHE_LIMIT {
            sizes.clear();
        }
        let hb = self.parent.sized(size)?;
        let (ascent, descent, leading) = hb.extents().unwrap_or((size * 0.8, size * 0.2, 0.0));
        let font = Rc::new(SizedFont {
            hb,
            ascent,
            descent,
            leading: leading.max(0.0),
        });
        sizes.insert(size.to_bits(), Rc::clone(&font));
        Some(font)
    }

    /// True when the face has a glyph for each character of `grapheme`
    /// that is not default-ignorable.
    pub(crate) fn covers(&self, grapheme: &str) -> bool {
        covers(&self.font, grapheme)
    }
}

/// True for characters that need no glyph of their own: controls,
/// joiners, variation selectors, and tags.
fn ignorable(character: char) -> bool {
    matches!(u32::from(character),
        0x00..=0x1F | 0x7F..=0x9F | 0x200B..=0x200F | 0x2028..=0x202E | 0x2060..=0x206F
        | 0xFE00..=0xFE0F | 0xFEFF | 0xE0000..=0xE0FFF)
}

const SIZE_CACHE_LIMIT: usize = 32;
const FACE_CACHE_LIMIT: usize = 128;
const CHAIN_CACHE_LIMIT: usize = 64;
const PICK_CACHE_LIMIT: usize = 4096;

/// Loaded faces by file and index; `None` records a face that did not
/// load.
type FaceCache = HashMap<(String, i64), Option<Rc<Face>>>;

thread_local! {
    static FACES: RefCell<FaceCache> = RefCell::new(HashMap::new());
    /// Font chains by style.
    static CHAINS: RefCell<HashMap<ChainKey, Rc<FontChain>>> = RefCell::new(HashMap::new());
}

/// The face of a fontconfig match, loaded at most one time.
fn load_face(font: &FontMatch) -> Option<Rc<Face>> {
    let key = (font.file.clone(), font.index);
    if let Some(face) = FACES.with(|faces| faces.borrow().get(&key).cloned()) {
        return face;
    }
    let face = FtFace::open(&font.file, font.index).and_then(|ft| {
        let parent = HbFont::from_face(&ft)?;
        Some(Rc::new(Face {
            name: register(ft.postscript_name(), font),
            font: font.clone(),
            parent,
            sizes: RefCell::new(HashMap::new()),
        }))
    });
    FACES.with(|faces| {
        let mut faces = faces.borrow_mut();
        if faces.len() >= FACE_CACHE_LIMIT {
            faces.clear();
        }
        faces.insert(key, face.clone());
    });
    face
}

/// The style fields that choose the faces.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct ChainKey {
    query: FontQuery,
    fallback: Vec<String>,
}

/// A fallback candidate, loaded on first use.
struct Candidate {
    font: FontMatch,
    face: OnceCell<Option<Rc<Face>>>,
}

impl Candidate {
    fn face(&self) -> Option<&Rc<Face>> {
        self.face.get_or_init(|| load_face(&self.font)).as_ref()
    }
}

/// The primary face of a style and its fallback cascade.
pub(crate) struct FontChain {
    pub(crate) primary: Rc<Face>,
    candidates: Vec<Candidate>,
    /// The candidate for each grapheme that the primary face lacks.
    picks: RefCell<HashMap<String, Option<usize>>>,
}

impl FontChain {
    /// The face for `grapheme`: the primary face, or the first candidate
    /// that covers it. With no such face, the primary face draws its
    /// missing glyph.
    pub(crate) fn face_for(&self, grapheme: &str) -> Rc<Face> {
        if self.primary.covers(grapheme) {
            return Rc::clone(&self.primary);
        }
        let cached = self.picks.borrow().get(grapheme).copied();
        let pick = cached.unwrap_or_else(|| {
            let pick = self.candidates.iter().position(|candidate| {
                covers(&candidate.font, grapheme) && candidate.face().is_some()
            });
            let mut picks = self.picks.borrow_mut();
            if picks.len() >= PICK_CACHE_LIMIT {
                picks.clear();
            }
            picks.insert(grapheme.to_owned(), pick);
            pick
        });
        pick.and_then(|index| self.candidates[index].face().cloned())
            .unwrap_or_else(|| Rc::clone(&self.primary))
    }
}

/// True when `font` has a glyph for each character of `grapheme` that is
/// not default-ignorable.
fn covers(font: &FontMatch, grapheme: &str) -> bool {
    grapheme
        .chars()
        .filter(|character| !ignorable(*character))
        .all(|character| font.has_char(character))
}

/// The fonts of `style`, or `None` when the font libraries do not load.
pub(crate) fn chain(style: &TextStyle) -> Option<Rc<FontChain>> {
    let query = FontQuery {
        family: resolve_family(&style.family),
        weight: style.weight.0,
        italic: style.slant == FontSlant::Italic,
        language: style.locale.clone(),
    };
    let key = ChainKey {
        query,
        fallback: style.fallback.clone(),
    };
    if let Some(chain) = CHAINS.with(|chains| chains.borrow().get(&key).cloned()) {
        return Some(chain);
    }
    let primary = load_face(&fontconfig::match_font(&key.query)?)?;
    let mut candidates: Vec<Candidate> = Vec::new();
    for family in &key.fallback {
        let query = FontQuery {
            family: resolve_family(family),
            ..key.query.clone()
        };
        if let Some(font) = fontconfig::match_font(&query) {
            candidates.push(Candidate {
                font,
                face: OnceCell::new(),
            });
        }
    }
    candidates.extend(
        fontconfig::sort_fonts(&key.query)
            .into_iter()
            .map(|font| Candidate {
                font,
                face: OnceCell::new(),
            }),
    );
    let chain = Rc::new(FontChain {
        primary,
        candidates,
        picks: RefCell::new(HashMap::new()),
    });
    CHAINS.with(|chains| {
        let mut chains = chains.borrow_mut();
        if chains.len() >= CHAIN_CACHE_LIMIT {
            chains.clear();
        }
        chains.insert(key, Rc::clone(&chain));
    });
    Some(chain)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_font_names_lose_their_size_and_style() {
        assert_eq!(desktop_family("Cantarell 11").as_deref(), Some("Cantarell"));
        assert_eq!(
            desktop_family("Noto Sans Bold Italic 10").as_deref(),
            Some("Noto Sans")
        );
        assert_eq!(desktop_family("Ubuntu").as_deref(), Some("Ubuntu"));
        assert_eq!(desktop_family("  12 "), None);
    }

    #[test]
    fn generic_families_resolve_to_fontconfig_aliases() {
        assert_eq!(resolve_family("ui-monospace"), "monospace");
        assert_eq!(resolve_family("DejaVu Sans"), "DejaVu Sans");
    }
}
