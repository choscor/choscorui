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

#[cfg(test)]
mod checks;
pub(crate) mod codecs;
#[cfg(test)]
mod ctype;
mod ctypes;
pub(crate) mod dl;
pub(crate) mod libc;
pub(crate) mod wayland;
pub(crate) mod x11;
pub(crate) mod xkb;

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
    let mut libraries = vec![
        dl::check_items(),
        libc::check_items(),
        wl::WaylandClient::check_items(),
        xkb::XkbCommon::check_items(),
    ];
    libraries.extend(codecs::check_items());
    libraries.extend(x11::check_items());
    let mut structs = vec![
        libc::PollFd::layout(),
        libc::Timespec::layout(),
        libc::Itimerspec::layout(),
        wl::WlMessage::layout(),
        wl::WlInterface::layout(),
        wl::WlArray::layout(),
        wl::WlArgument::layout(),
    ];
    structs.extend(codecs::layouts());
    structs.extend(x11::layouts());
    let constants = [
        libc::constants(),
        wl::constants(),
        codecs::constants(),
        xkb::constants(),
        x11::all_constants(),
    ]
    .into_iter()
    .flatten()
    .collect();
    (libraries, structs, constants)
}
