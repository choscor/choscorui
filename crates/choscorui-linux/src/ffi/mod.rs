//! The FFI boundary of `choscorui-linux`.
//!
//! All hand-written bindings to the system libraries are here. The crate
//! links only the C library and loads every other library at run time
//! with `dlopen` (see `dl`). The macros in `macros` declare each symbol,
//! struct, and constant one time, and the checks in `checks` compile the
//! declarations against the system headers. Each `unsafe` block holds one
//! raw operation with its `SAFETY` comment, and each module gives a safe
//! interface to the rest of the crate.

mod macros;

pub(crate) mod cairo;

#[cfg(test)]
mod checks;
#[cfg(test)]
mod ctype;
mod ctypes;
pub(crate) mod dl;
pub(crate) mod fontconfig;
pub(crate) mod freetype;
pub(crate) mod harfbuzz;
pub(crate) mod libc;
pub(crate) mod wayland;

pub use dl::LoadError;

/// Reports a panic that a C callback caught. The callback then returns
/// its fallback value.
pub(crate) fn report_callback_panic(boundary: &str) {
    eprintln!("choscorui: a panic stopped at the {boundary} callback boundary");
}

/// Every declaration of the crate, for the binding check.
#[cfg(test)]
pub(crate) fn declarations() -> (
    Vec<checks::LibraryItems>,
    Vec<checks::StructItem>,
    Vec<checks::ConstItem>,
) {
    use wayland::sys as wl;
    let libraries = vec![
        dl::check_items(),
        libc::check_items(),
        wl::WaylandClient::check_items(),
        freetype::FreeType::check_items(),
        harfbuzz::HarfBuzz::check_items(),
        fontconfig::Fontconfig::check_items(),
        cairo::sys::Cairo::check_items(),
    ];
    let structs = vec![
        libc::PollFd::layout(),
        libc::Timespec::layout(),
        libc::Itimerspec::layout(),
        wl::WlMessage::layout(),
        wl::WlInterface::layout(),
        wl::WlArray::layout(),
        wl::WlArgument::layout(),
        harfbuzz::HbGlyphInfo::layout(),
        harfbuzz::HbGlyphPosition::layout(),
        harfbuzz::HbFontExtents::layout(),
        fontconfig::FcFontSet::layout(),
        cairo::sys::CairoMatrix::layout(),
        cairo::sys::CairoGlyph::layout(),
        cairo::sys::CairoUserDataKey::layout(),
    ];
    let constants = [
        libc::constants(),
        wl::constants(),
        freetype::constants(),
        harfbuzz::HbDirection::constants(),
        harfbuzz::HbClusterLevel::constants(),
        fontconfig::constants(),
        fontconfig::FcResult::constants(),
        fontconfig::FcMatchKind::constants(),
        cairo::sys::constants(),
    ]
    .into_iter()
    .flatten()
    .collect();
    (libraries, structs, constants)
}
