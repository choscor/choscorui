//! Runtime loading of the system libraries with `dlopen` and `dlsym`.
//!
//! The crate links only the C library. Each other library loads when a
//! feature first needs it, and a missing library or symbol is a
//! `LoadError` that names both. The process never unloads a library, so a
//! loaded symbol stays valid for the life of the process.

use super::ctypes::{CChar, CInt, c_void};
use std::ffi::CStr;
use std::fmt;

const RTLD_NOW: CInt = 2;
const RTLD_LOCAL: CInt = 0;

super::macros::libc_functions! {
    headers: ["dlfcn.h"];
    fn dlopen(filename: *const CChar, flags: CInt) -> *mut c_void;
    fn dlsym(handle: *mut c_void, symbol: *const CChar) -> *mut c_void;
}

/// A shared library that the crate loads at run time.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Library {
    /// The short name, such as `libtiff`. `CHOSCORUI_TEST_HIDE_LIB` names
    /// libraries by it.
    pub(crate) name: &'static str,
    /// The sonames to try, in order.
    pub(crate) sonames: &'static [&'static str],
    /// The headers that declare the symbols, for the binding check.
    pub(crate) headers: &'static [&'static str],
    /// The pkg-config module of the headers, or empty for the C library.
    pub(crate) pkg_config: &'static str,
    /// The Debian package with the headers.
    pub(crate) package: &'static str,
}

/// Why a library did not load.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LoadError {
    /// No soname of the library loaded.
    MissingLibrary {
        library: &'static str,
        reason: String,
    },
    /// The library loaded, but it does not export a required symbol.
    MissingSymbol {
        library: &'static str,
        symbol: &'static str,
    },
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingLibrary { library, reason } => {
                write!(f, "{library} did not load: {reason}")
            }
            Self::MissingSymbol { library, symbol } => {
                write!(f, "{library} does not export {symbol}")
            }
        }
    }
}

impl std::error::Error for LoadError {}

/// True when `CHOSCORUI_TEST_HIDE_LIB` names `library`. The tests use it
/// to check the behavior without a library. The value is a list of short
/// names separated by commas.
fn hidden(library: &str) -> bool {
    std::env::var("CHOSCORUI_TEST_HIDE_LIB").is_ok_and(|names| {
        names
            .split(',')
            .any(|name| !name.is_empty() && name.trim() == library)
    })
}

/// A loaded library. The handle is never closed.
pub(crate) struct Handle {
    raw: *mut c_void,
}

impl Handle {
    /// Loads the first soname of `library` that loads.
    pub(crate) fn open(library: &Library) -> Result<Self, LoadError> {
        if hidden(library.name) {
            return Err(LoadError::MissingLibrary {
                library: library.name,
                reason: "hidden by CHOSCORUI_TEST_HIDE_LIB".to_owned(),
            });
        }
        for soname in library.sonames {
            let Ok(name) = std::ffi::CString::new(*soname) else {
                continue;
            };
            // SAFETY: `name` is a NUL-terminated string. Loading runs the
            // library's initializers, which the system libraries allow on
            // any thread.
            let raw = unsafe { dlopen(name.as_ptr().cast(), RTLD_NOW | RTLD_LOCAL) };
            if !raw.is_null() {
                return Ok(Self { raw });
            }
        }
        Err(LoadError::MissingLibrary {
            library: library.name,
            reason: format!("none of {} is installed", library.sonames.join(", ")),
        })
    }

    /// The address of `symbol`, a NUL-terminated name.
    pub(crate) fn address(
        &self,
        library: &Library,
        symbol: &'static str,
    ) -> Result<*mut c_void, LoadError> {
        let missing = || LoadError::MissingSymbol {
            library: library.name,
            symbol: symbol.trim_end_matches('\0'),
        };
        let name = CStr::from_bytes_with_nul(symbol.as_bytes()).map_err(|_| missing())?;
        // SAFETY: the handle came from `dlopen` and stays open; `name` is
        // NUL-terminated.
        let address = unsafe { dlsym(self.raw, name.as_ptr().cast()) };
        if address.is_null() {
            Err(missing())
        } else {
            Ok(address)
        }
    }
}

/// The function at `address` as the function pointer type `F`.
///
/// # Safety
///
/// `address` is the address of a C function whose signature is `F`.
pub(crate) unsafe fn function<F: Copy>(address: *mut c_void) -> F {
    const {
        assert!(size_of::<F>() == size_of::<*mut c_void>());
    }
    // SAFETY: `F` is a function pointer of the same size as the address,
    // and the caller guarantees the signature.
    unsafe { std::mem::transmute_copy::<*mut c_void, F>(&address) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_library_names_itself() {
        let library = Library {
            name: "libdoesnotexist",
            sonames: &["libdoesnotexist.so.42"],
            headers: &[],
            pkg_config: "",
            package: "",
        };
        let error = Handle::open(&library).err().unwrap();
        assert!(
            error
                .to_string()
                .starts_with("libdoesnotexist did not load")
        );
    }

    #[test]
    fn a_missing_symbol_names_the_library_and_the_symbol() {
        let library = Library {
            name: "libc",
            sonames: &["libc.so.6"],
            headers: &[],
            pkg_config: "",
            package: "",
        };
        let handle = Handle::open(&library).unwrap();
        assert!(handle.address(&library, "malloc\0").is_ok());
        let error = handle
            .address(&library, "choscorui_no_such_symbol\0")
            .unwrap_err();
        assert_eq!(
            error.to_string(),
            "libc does not export choscorui_no_such_symbol"
        );
    }
}
