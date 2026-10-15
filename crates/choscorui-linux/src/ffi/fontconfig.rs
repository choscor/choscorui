//! The declarations of fontconfig, and safe queries: the best face for a
//! pattern, the fallback list of a pattern, and the installed families.

use super::ctypes::{CChar, CInt};
use super::macros::{c_consts, ffi_enum, ffi_struct, libraries, opaque_types};
use std::ffi::{CStr, CString};
use std::sync::OnceLock;

opaque_types! {
    /// `FcConfig`.
    pub(crate) FcConfig = "FcConfig";
    /// `FcPattern`.
    pub(crate) FcPattern = "FcPattern";
    /// `FcCharSet`.
    pub(crate) FcCharSet = "FcCharSet";
    /// `FcObjectSet`.
    pub(crate) FcObjectSet = "FcObjectSet";
}

ffi_struct! {
    /// `FcFontSet`.
    pub(crate) struct FcFontSet = "FcFontSet" {
        pub(crate) nfont: CInt,
        pub(crate) sfont: CInt,
        pub(crate) fonts: *mut *mut FcPattern,
    }
}

ffi_enum! {
    /// `FcResult`.
    pub(crate) enum FcResult: u32 = "FcResult" {
        #[allow(non_upper_case_globals, reason = "the binding check needs the C name")]
        FcResultMatch = 0,
    }

    /// `FcMatchKind`.
    pub(crate) enum FcMatchKind: u32 = "FcMatchKind" {
        #[allow(non_upper_case_globals, reason = "the binding check needs the C name")]
        FcMatchPattern = 0,
    }
}

c_consts! {
    /// The constants of this module, for the binding check.
    pub(crate) fn constants;
    pub(crate) const FC_SLANT_ROMAN: CInt = 0;
    pub(crate) const FC_SLANT_ITALIC: CInt = 100;
    pub(crate) const FC_HINT_NONE: CInt = 0;
    pub(crate) const FC_HINT_MEDIUM: CInt = 2;
    pub(crate) const FC_HINT_FULL: CInt = 3;
}

// The values of the `FC_*` object name macros, which are strings.
const FC_FAMILY: &CStr = c"family";
const FC_STYLE: &CStr = c"style";
const FC_SLANT: &CStr = c"slant";
const FC_WEIGHT: &CStr = c"weight";
const FC_FILE: &CStr = c"file";
const FC_INDEX: &CStr = c"index";
const FC_LANG: &CStr = c"lang";
const FC_CHARSET: &CStr = c"charset";
const FC_HINTING: &CStr = c"hinting";
const FC_HINT_STYLE: &CStr = c"hintstyle";
const FC_POSTSCRIPT_NAME: &CStr = c"postscriptname";

libraries! {
    /// libfontconfig: font matching and fallback.
    pub(crate) struct Fontconfig {
        library: "libfontconfig",
        sonames: ["libfontconfig.so.1"],
        headers: ["fontconfig/fontconfig.h"],
        pkg_config: "fontconfig",
        package: "libfontconfig-dev",
        functions {
            fn FcInit() -> CInt;
            fn FcPatternCreate() -> *mut FcPattern;
            fn FcPatternDestroy(pattern: *mut FcPattern);
            fn FcPatternAddString(pattern: *mut FcPattern, object: *const CChar, value: *const u8) -> CInt;
            fn FcPatternAddInteger(pattern: *mut FcPattern, object: *const CChar, value: CInt) -> CInt;
            fn FcPatternGetString(
                pattern: *const FcPattern,
                object: *const CChar,
                n: CInt,
                value: *mut *mut u8,
            ) -> FcResult;
            fn FcPatternGetInteger(
                pattern: *const FcPattern,
                object: *const CChar,
                n: CInt,
                value: *mut CInt,
            ) -> FcResult;
            fn FcPatternGetBool(
                pattern: *const FcPattern,
                object: *const CChar,
                n: CInt,
                value: *mut CInt,
            ) -> FcResult;
            fn FcPatternGetCharSet(
                pattern: *const FcPattern,
                object: *const CChar,
                n: CInt,
                value: *mut *mut FcCharSet,
            ) -> FcResult;
            fn FcCharSetHasChar(set: *const FcCharSet, character: u32) -> CInt;
            fn FcConfigSubstitute(config: *mut FcConfig, pattern: *mut FcPattern, kind: FcMatchKind) -> CInt;
            fn FcDefaultSubstitute(pattern: *mut FcPattern);
            fn FcFontMatch(config: *mut FcConfig, pattern: *mut FcPattern, result: *mut FcResult) -> *mut FcPattern;
            fn FcFontSort(
                config: *mut FcConfig,
                pattern: *mut FcPattern,
                trim: CInt,
                sets: *mut *mut FcCharSet,
                result: *mut FcResult,
            ) -> *mut FcFontSet;
            fn FcFontRenderPrepare(
                config: *mut FcConfig,
                pattern: *mut FcPattern,
                font: *mut FcPattern,
            ) -> *mut FcPattern;
            fn FcFontList(config: *mut FcConfig, pattern: *mut FcPattern, objects: *mut FcObjectSet) -> *mut FcFontSet;
            fn FcFontSetDestroy(set: *mut FcFontSet);
            fn FcObjectSetCreate() -> *mut FcObjectSet;
            fn FcObjectSetAdd(set: *mut FcObjectSet, object: *const CChar) -> CInt;
            fn FcObjectSetDestroy(set: *mut FcObjectSet);
            fn FcWeightFromOpenType(weight: CInt) -> CInt;
        }
    }
}

/// The library after `FcInit`, or `None` when it does not load.
fn library() -> Option<&'static Fontconfig> {
    static READY: OnceLock<bool> = OnceLock::new();
    let lib = Fontconfig::get().ok()?;
    // SAFETY: `FcInit` loads the default configuration; fontconfig makes
    // it safe to call from any thread.
    let ready = *READY.get_or_init(|| unsafe { (lib.FcInit)() } != 0);
    ready.then_some(lib)
}

/// True when fontconfig loads and finds its configuration.
pub(crate) fn available() -> bool {
    library().is_some()
}

/// What a font query asks for.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub(crate) struct FontQuery {
    pub(crate) family: String,
    /// CSS weight, 1 to 1000.
    pub(crate) weight: u16,
    pub(crate) italic: bool,
    /// A BCP 47 tag, which fontconfig uses to prefer local forms.
    pub(crate) language: Option<String>,
}

/// How fontconfig says to hint a face.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub(crate) enum Hinting {
    None,
    #[default]
    Slight,
    Medium,
    Full,
}

/// A face that a query matched.
#[derive(Clone, Debug)]
pub(crate) struct FontMatch {
    pub(crate) file: String,
    pub(crate) index: i64,
    pub(crate) family: String,
    pub(crate) hinting: Hinting,
    charset: Option<CharSet>,
}

impl FontMatch {
    /// True when the face has a glyph for `character`. A face whose
    /// coverage is unknown claims every character.
    pub(crate) fn has_char(&self, character: char) -> bool {
        self.charset
            .as_ref()
            .is_none_or(|charset| charset.has(character))
    }
}

/// An owned pattern.
struct Pattern {
    raw: *mut FcPattern,
}

impl Pattern {
    fn new(lib: &Fontconfig) -> Option<Self> {
        // SAFETY: plain constructor.
        let raw = unsafe { (lib.FcPatternCreate)() };
        (!raw.is_null()).then_some(Self { raw })
    }

    fn add_string(&self, lib: &Fontconfig, object: &CStr, value: &str) {
        let Ok(value) = CString::new(value) else {
            return;
        };
        // SAFETY: the pattern is live and both strings are NUL-terminated;
        // fontconfig copies the value.
        unsafe {
            (lib.FcPatternAddString)(self.raw, object.as_ptr().cast(), value.as_ptr().cast())
        };
    }

    fn add_integer(&self, lib: &Fontconfig, object: &CStr, value: CInt) {
        // SAFETY: the pattern is live and the object name NUL-terminated.
        unsafe { (lib.FcPatternAddInteger)(self.raw, object.as_ptr().cast(), value) };
    }

    /// The pattern of `query`, with the configuration and default
    /// substitutions applied.
    fn for_query(lib: &Fontconfig, query: &FontQuery) -> Option<Self> {
        let pattern = Self::new(lib)?;
        pattern.add_string(lib, FC_FAMILY, &query.family);
        // SAFETY: plain conversion of an integer.
        let weight = unsafe { (lib.FcWeightFromOpenType)(CInt::from(query.weight)) };
        pattern.add_integer(lib, FC_WEIGHT, weight);
        let slant = if query.italic {
            FC_SLANT_ITALIC
        } else {
            FC_SLANT_ROMAN
        };
        pattern.add_integer(lib, FC_SLANT, slant);
        if let Some(language) = &query.language {
            pattern.add_string(lib, FC_LANG, language);
        }
        // SAFETY: the pattern is live; a null config is the current one.
        unsafe {
            (lib.FcConfigSubstitute)(
                std::ptr::null_mut(),
                pattern.raw,
                FcMatchKind::FcMatchPattern,
            )
        };
        // SAFETY: the pattern is live.
        unsafe { (lib.FcDefaultSubstitute)(pattern.raw) };
        Some(pattern)
    }
}

impl Drop for Pattern {
    fn drop(&mut self) {
        if let Ok(lib) = Fontconfig::get() {
            // SAFETY: `self` owns the live pattern.
            unsafe { (lib.FcPatternDestroy)(self.raw) };
        }
    }
}

/// An owned font set.
struct FontSet {
    raw: *mut FcFontSet,
}

impl FontSet {
    /// The patterns of the set.
    fn patterns(&self) -> Vec<*mut FcPattern> {
        if self.raw.is_null() {
            return Vec::new();
        }
        // SAFETY: the set is live; the struct is read by value.
        let set = unsafe { *self.raw };
        if set.fonts.is_null() || set.nfont <= 0 {
            return Vec::new();
        }
        // SAFETY: fontconfig stores `nfont` live patterns in `fonts`.
        unsafe { std::slice::from_raw_parts(set.fonts, set.nfont as usize) }.to_vec()
    }
}

impl Drop for FontSet {
    fn drop(&mut self) {
        if let Ok(lib) = Fontconfig::get()
            && !self.raw.is_null()
        {
            // SAFETY: `self` owns the live set and its patterns.
            unsafe { (lib.FcFontSetDestroy)(self.raw) };
        }
    }
}

/// The character coverage of a face. It keeps the pattern that owns the
/// character set alive.
#[derive(Clone, Debug)]
struct CharSet {
    owner: std::sync::Arc<OwnedPattern>,
    raw: *mut FcCharSet,
}

impl CharSet {
    fn has(&self, character: char) -> bool {
        let Ok(lib) = Fontconfig::get() else {
            return false;
        };
        let _ = &self.owner;
        // SAFETY: the owner keeps the character set alive.
        unsafe { (lib.FcCharSetHasChar)(self.raw, u32::from(character)) != 0 }
    }
}

/// A pattern that a match returned, kept for its character set.
#[derive(Debug)]
struct OwnedPattern {
    raw: *mut FcPattern,
}

// SAFETY: the pattern is never changed after the match. fontconfig is
// thread-safe for reads, and it counts pattern references atomically, so
// any thread may read or destroy it.
unsafe impl Send for OwnedPattern {}
// SAFETY: as above.
unsafe impl Sync for OwnedPattern {}

impl Drop for OwnedPattern {
    fn drop(&mut self) {
        if let Ok(lib) = Fontconfig::get() {
            // SAFETY: `self` owns the live pattern.
            unsafe { (lib.FcPatternDestroy)(self.raw) };
        }
    }
}

/// String `object` of `pattern`, copied.
fn get_string(lib: &Fontconfig, pattern: *mut FcPattern, object: &CStr) -> Option<String> {
    let mut value = std::ptr::null_mut();
    // SAFETY: the pattern is live and the out-pointer valid.
    let result =
        unsafe { (lib.FcPatternGetString)(pattern, object.as_ptr().cast(), 0, &raw mut value) };
    if result != FcResult::FcResultMatch || value.is_null() {
        return None;
    }
    // SAFETY: fontconfig returns a NUL-terminated string owned by the
    // pattern, copied at once.
    let text = unsafe { CStr::from_ptr(value.cast()) };
    Some(text.to_string_lossy().into_owned())
}

/// Integer `object` of `pattern`.
fn get_integer(lib: &Fontconfig, pattern: *mut FcPattern, object: &CStr) -> Option<CInt> {
    let mut value = 0;
    // SAFETY: the pattern is live and the out-pointer valid.
    let result =
        unsafe { (lib.FcPatternGetInteger)(pattern, object.as_ptr().cast(), 0, &raw mut value) };
    (result == FcResult::FcResultMatch).then_some(value)
}

/// Boolean `object` of `pattern`.
fn get_bool(lib: &Fontconfig, pattern: *mut FcPattern, object: &CStr) -> Option<bool> {
    let mut value = 0;
    // SAFETY: the pattern is live and the out-pointer valid.
    let result =
        unsafe { (lib.FcPatternGetBool)(pattern, object.as_ptr().cast(), 0, &raw mut value) };
    (result == FcResult::FcResultMatch).then_some(value != 0)
}

/// The face that `pattern` names. The match takes ownership of the
/// pattern, to keep its character set.
fn font_match(lib: &Fontconfig, pattern: *mut FcPattern) -> Option<FontMatch> {
    let owner = std::sync::Arc::new(OwnedPattern { raw: pattern });
    let file = get_string(lib, pattern, FC_FILE)?;
    let index = get_integer(lib, pattern, FC_INDEX).unwrap_or(0);
    let family = get_string(lib, pattern, FC_FAMILY).unwrap_or_default();
    let hinting = if get_bool(lib, pattern, FC_HINTING) == Some(false) {
        Hinting::None
    } else {
        match get_integer(lib, pattern, FC_HINT_STYLE) {
            Some(FC_HINT_NONE) => Hinting::None,
            Some(FC_HINT_MEDIUM) => Hinting::Medium,
            Some(FC_HINT_FULL) => Hinting::Full,
            // Slight hinting (1), and the default of a face without a style.
            _ => Hinting::Slight,
        }
    };
    let mut charset = std::ptr::null_mut();
    // SAFETY: the pattern is live and the out-pointer valid; the character
    // set stays owned by the pattern, which `owner` keeps.
    let result = unsafe {
        (lib.FcPatternGetCharSet)(pattern, FC_CHARSET.as_ptr().cast(), 0, &raw mut charset)
    };
    let charset = (result == FcResult::FcResultMatch && !charset.is_null()).then(|| CharSet {
        owner,
        raw: charset,
    });
    Some(FontMatch {
        file,
        index: i64::from(index),
        family,
        hinting,
        charset,
    })
}

/// The best face for `query`, with its render settings.
pub(crate) fn match_font(query: &FontQuery) -> Option<FontMatch> {
    let lib = library()?;
    let pattern = Pattern::for_query(lib, query)?;
    let mut result = FcResult::FcResultMatch;
    // SAFETY: the pattern is live and the out-pointer valid; the result
    // is a new pattern that the caller owns.
    let matched = unsafe { (lib.FcFontMatch)(std::ptr::null_mut(), pattern.raw, &raw mut result) };
    if matched.is_null() {
        return None;
    }
    font_match(lib, matched)
}

/// The faces that fontconfig orders for `query`, best first. The list is
/// the fallback cascade of the query.
pub(crate) fn sort_fonts(query: &FontQuery) -> Vec<FontMatch> {
    let Some(lib) = library() else {
        return Vec::new();
    };
    let Some(pattern) = Pattern::for_query(lib, query) else {
        return Vec::new();
    };
    let mut result = FcResult::FcResultMatch;
    // SAFETY: the pattern is live and the out-pointers are valid or null;
    // the set is new and owned by `set`.
    let raw = unsafe {
        (lib.FcFontSort)(
            std::ptr::null_mut(),
            pattern.raw,
            1,
            std::ptr::null_mut(),
            &raw mut result,
        )
    };
    let set = FontSet { raw };
    set.patterns()
        .into_iter()
        .filter_map(|font| {
            // SAFETY: both patterns are live; the result is a new pattern
            // with the render settings, which the match then owns.
            let prepared =
                unsafe { (lib.FcFontRenderPrepare)(std::ptr::null_mut(), pattern.raw, font) };
            (!prepared.is_null())
                .then(|| font_match(lib, prepared))
                .flatten()
        })
        .collect()
}

/// The file and index of the face whose PostScript name is `name`.
pub(crate) fn match_postscript_name(name: &str) -> Option<FontMatch> {
    let lib = library()?;
    let pattern = Pattern::new(lib)?;
    pattern.add_string(lib, FC_POSTSCRIPT_NAME, name);
    let objects = ObjectSet::new(lib, &[FC_FILE, FC_INDEX, FC_FAMILY, FC_CHARSET])?;
    // SAFETY: the pattern and the object set are live; the set is new.
    let raw = unsafe { (lib.FcFontList)(std::ptr::null_mut(), pattern.raw, objects.raw) };
    let set = FontSet { raw };
    let first = *set.patterns().first()?;
    let file = get_string(lib, first, FC_FILE)?;
    let index = get_integer(lib, first, FC_INDEX).unwrap_or(0);
    Some(FontMatch {
        file,
        index: i64::from(index),
        family: get_string(lib, first, FC_FAMILY).unwrap_or_default(),
        hinting: Hinting::default(),
        charset: None,
    })
}

/// An owned object set.
struct ObjectSet {
    raw: *mut FcObjectSet,
}

impl ObjectSet {
    fn new(lib: &Fontconfig, objects: &[&CStr]) -> Option<Self> {
        // SAFETY: plain constructor.
        let raw = unsafe { (lib.FcObjectSetCreate)() };
        if raw.is_null() {
            return None;
        }
        let set = Self { raw };
        for object in objects {
            // SAFETY: the set is live and the name is a static string.
            unsafe { (lib.FcObjectSetAdd)(set.raw, object.as_ptr().cast()) };
        }
        Some(set)
    }
}

impl Drop for ObjectSet {
    fn drop(&mut self) {
        if let Ok(lib) = Fontconfig::get() {
            // SAFETY: `self` owns the live set.
            unsafe { (lib.FcObjectSetDestroy)(self.raw) };
        }
    }
}

/// The family names of the installed faces, sorted and without
/// duplicates.
pub(crate) fn families() -> Vec<String> {
    let Some(lib) = library() else {
        return Vec::new();
    };
    let Some(pattern) = Pattern::new(lib) else {
        return Vec::new();
    };
    let Some(objects) = ObjectSet::new(lib, &[FC_FAMILY, FC_STYLE]) else {
        return Vec::new();
    };
    // SAFETY: the pattern and the object set are live; the set is new.
    let raw = unsafe { (lib.FcFontList)(std::ptr::null_mut(), pattern.raw, objects.raw) };
    let set = FontSet { raw };
    let mut names: Vec<String> = set
        .patterns()
        .into_iter()
        .filter_map(|font| get_string(lib, font, FC_FAMILY))
        .collect();
    names.sort();
    names.dedup();
    names
}
