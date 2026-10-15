//! The macros that declare the C side of the boundary.
//!
//! - `libraries!` declares a shared object, its sonames and headers, and
//!   the exact C signature of each symbol that the crate loads from it.
//! - `libc_functions!` declares the C library functions that the crate
//!   links directly.
//! - `ffi_struct!` and `ffi_union!` declare `repr(C)` types with their C
//!   names.
//! - `ffi_enum!` declares a C enum type as a transparent integer, with its
//!   constants.
//! - `c_consts!` declares the values of C macros and enum constants.
//! - `lookup_functions!` declares extension functions that a library
//!   returns from a lookup call, with the header typedef of each.
//! - `callback!` declares a C callback that catches a panic of its body.
//!
//! Each macro emits the real declaration and, in tests, a descriptor from
//! the same tokens. `checks::binding` writes one C file from the
//! descriptors and compiles it against the system headers, so a
//! declaration cannot drift from its header.

/// Declares a shared library and the symbols that the crate loads from it.
///
/// ```text
/// libraries! {
///     /// What the library is for.
///     pub(crate) struct Cairo {
///         library: "libcairo",
///         sonames: ["libcairo.so.2"],
///         headers: ["cairo.h"],
///         pkg_config: "cairo",
///         package: "libcairo2-dev",
///         functions {
///             fn cairo_create(target: *mut CairoSurface) -> *mut Cairo_t;
///         }
///         optional {
///             fn cairo_newer_call(cr: *mut Cairo_t);
///         }
///         variadic {
///             fn printf_like(format: *const CChar) -> i32;
///         }
///     }
/// }
/// ```
///
/// `get` loads the table at most one time for each process. It returns a
/// `LoadError` that names the library and the first missing symbol. An
/// optional symbol is an `Option`, and a missing one is `None`.
macro_rules! libraries {
    ($(
        $(#[$meta:meta])*
        $vis:vis struct $table:ident {
            library: $library:literal,
            sonames: [$($soname:literal),+ $(,)?],
            headers: [$($header:literal),* $(,)?],
            pkg_config: $pkg:literal,
            package: $package:literal,
            functions {$(
                $(#[$fmeta:meta])*
                fn $name:ident($($arg:ident : $ty:ty),* $(,)?) $(-> $ret:ty)?;
            )*}
            $(optional {$(
                $(#[$ometa:meta])*
                fn $oname:ident($($oarg:ident : $oty:ty),* $(,)?) $(-> $oret:ty)?;
            )*})?
            $(variadic {$(
                $(#[$vmeta:meta])*
                fn $vname:ident($($varg:ident : $vty:ty),* $(,)?) $(-> $vret:ty)?;
            )*})?
        }
    )*) => {$(
        $(#[$meta])*
        #[allow(non_snake_case, reason = "the fields keep the C symbol names")]
        $vis struct $table {
            $(
                $(#[$fmeta])*
                pub(crate) $name: unsafe extern "C" fn($($ty),*) $(-> $ret)?,
            )*
            $($(
                $(#[$ometa])*
                pub(crate) $oname: Option<unsafe extern "C" fn($($oty),*) $(-> $oret)?>,
            )*)?
            $($(
                $(#[$vmeta])*
                pub(crate) $vname: unsafe extern "C" fn($($vty,)* ...) $(-> $vret)?,
            )*)?
        }

        impl $table {
            /// The library: its name, sonames, headers, and package.
            pub(crate) const LIBRARY: $crate::ffi::dl::Library = $crate::ffi::dl::Library {
                name: $library,
                sonames: &[$($soname),+],
                headers: &[$($header),*],
                pkg_config: $pkg,
                package: $package,
            };

            /// The symbol table. The first call loads the library; later
            /// calls return the same table or the same error.
            pub(crate) fn get() -> Result<&'static Self, $crate::ffi::dl::LoadError> {
                static TABLE: std::sync::OnceLock<Result<$table, $crate::ffi::dl::LoadError>> =
                    std::sync::OnceLock::new();
                TABLE.get_or_init(Self::load).as_ref().map_err(Clone::clone)
            }

            /// True when the library and all its required symbols load.
            #[allow(dead_code, reason = "not every library is probed by name")]
            pub(crate) fn available() -> bool {
                Self::get().is_ok()
            }

            fn load() -> Result<Self, $crate::ffi::dl::LoadError> {
                let handle = $crate::ffi::dl::Handle::open(&Self::LIBRARY)?;
                Ok(Self {
                    $(
                        $name: {
                            let symbol = handle.address(
                                &Self::LIBRARY,
                                concat!(stringify!($name), "\0"),
                            )?;
                            // SAFETY: the symbol is the C function of this
                            // name, and the binding check compares the
                            // declared signature with its header.
                            unsafe {
                                $crate::ffi::dl::function::<
                                    unsafe extern "C" fn($($ty),*) $(-> $ret)?,
                                >(symbol)
                            }
                        },
                    )*
                    $($(
                        $oname: match handle.address(
                            &Self::LIBRARY,
                            concat!(stringify!($oname), "\0"),
                        ) {
                            // SAFETY: as for the required symbols above.
                            Ok(symbol) => Some(unsafe {
                                $crate::ffi::dl::function::<
                                    unsafe extern "C" fn($($oty),*) $(-> $oret)?,
                                >(symbol)
                            }),
                            Err(_) => None,
                        },
                    )*)?
                    $($(
                        $vname: {
                            let symbol = handle.address(
                                &Self::LIBRARY,
                                concat!(stringify!($vname), "\0"),
                            )?;
                            // SAFETY: as for the required symbols above.
                            unsafe {
                                $crate::ffi::dl::function::<
                                    unsafe extern "C" fn($($vty,)* ...) $(-> $vret)?,
                                >(symbol)
                            }
                        },
                    )*)?
                })
            }

            /// The descriptor of each symbol, for the binding check.
            #[cfg(test)]
            pub(crate) fn check_items() -> $crate::ffi::checks::LibraryItems {
                use $crate::ffi::ctype::CType;
                let mut functions = Vec::new();
                $(
                    functions.push($crate::ffi::checks::FunctionItem {
                        name: stringify!($name),
                        ret: <$crate::ffi::macros::libraries!(@ret $($ret)?) as CType>::spelling(),
                        args: vec![$(<$ty as CType>::spelling()),*],
                        variadic: false,
                        typedef: None,
                    });
                )*
                $($(
                    functions.push($crate::ffi::checks::FunctionItem {
                        name: stringify!($oname),
                        ret: <$crate::ffi::macros::libraries!(@ret $($oret)?) as CType>::spelling(),
                        args: vec![$(<$oty as CType>::spelling()),*],
                        variadic: false,
                        typedef: None,
                    });
                )*)?
                $($(
                    functions.push($crate::ffi::checks::FunctionItem {
                        name: stringify!($vname),
                        ret: <$crate::ffi::macros::libraries!(@ret $($vret)?) as CType>::spelling(),
                        args: vec![$(<$vty as CType>::spelling()),*],
                        variadic: true,
                        typedef: None,
                    });
                )*)?
                $crate::ffi::checks::LibraryItems {
                    library: Self::LIBRARY,
                    functions,
                }
            }
        }
    )*};
    (@ret) => { () };
    (@ret $ret:ty) => { $ret };
}

/// Declares C library functions that the crate links directly, with the
/// headers that declare them.
macro_rules! libc_functions {
    (
        headers: [$($header:literal),* $(,)?];
        $(
            $(#[$meta:meta])*
            $vis:vis fn $name:ident($($arg:ident : $ty:ty),* $(,)?) $(-> $ret:ty)?;
        )*
    ) => {
        unsafe extern "C" {
            $(
                $(#[$meta])*
                $vis fn $name($($arg: $ty),*) $(-> $ret)?;
            )*
        }

        /// The descriptor of each function, for the binding check.
        #[cfg(test)]
        pub(crate) fn check_items() -> $crate::ffi::checks::LibraryItems {
            use $crate::ffi::ctype::CType;
            $crate::ffi::checks::LibraryItems {
                library: $crate::ffi::dl::Library {
                    name: "libc",
                    sonames: &["libc.so.6"],
                    headers: &[$($header),*],
                    pkg_config: "",
                    package: "libc6-dev",
                },
                functions: vec![$(
                    $crate::ffi::checks::FunctionItem {
                        name: stringify!($name),
                        ret: <$crate::ffi::macros::libc_functions!(@ret $($ret)?) as CType>::spelling(),
                        args: vec![$(<$ty as CType>::spelling()),*],
                        variadic: false,
                        typedef: None,
                    },
                )*],
            }
        }
    };
    (@ret) => { () };
    (@ret $ret:ty) => { $ret };
}

/// Declares `repr(C)` structs with the C name of each. The binding check
/// compares the size, and the offset and size of each field, with the C
/// type.
macro_rules! ffi_struct {
    ($(
        $(#[$meta:meta])*
        $vis:vis struct $name:ident = $cname:literal {
            $(
                $(#[$fmeta:meta])*
                $fvis:vis $field:ident : $fty:ty,
            )*
        }
    )*) => {$(
        $(#[$meta])*
        #[repr(C)]
        #[derive(Clone, Copy, Debug)]
        $vis struct $name {
            $(
                $(#[$fmeta])*
                $fvis $field: $fty,
            )*
        }

        #[cfg(test)]
        impl $crate::ffi::ctype::CType for $name {
            fn spelling() -> String {
                $cname.to_owned()
            }
        }

        impl $name {
            /// The layout of the type, for the binding check.
            #[cfg(test)]
            pub(crate) fn layout() -> $crate::ffi::checks::StructItem {
                $crate::ffi::checks::StructItem {
                    name: $cname,
                    size: size_of::<Self>(),
                    fields: vec![$(
                        (
                            $crate::ffi::checks::c_field(stringify!($field)),
                            std::mem::offset_of!(Self, $field),
                            size_of::<$fty>(),
                        ),
                    )*],
                }
            }
        }
    )*};
}

/// Declares `repr(C)` unions with the C name of each, checked as structs.
macro_rules! ffi_union {
    ($(
        $(#[$meta:meta])*
        $vis:vis union $name:ident = $cname:literal {
            $(
                $(#[$fmeta:meta])*
                $fvis:vis $field:ident : $fty:ty,
            )*
        }
    )*) => {$(
        $(#[$meta])*
        #[repr(C)]
        #[derive(Clone, Copy)]
        $vis union $name {
            $(
                $(#[$fmeta])*
                $fvis $field: $fty,
            )*
        }

        #[cfg(test)]
        impl $crate::ffi::ctype::CType for $name {
            fn spelling() -> String {
                $cname.to_owned()
            }
        }

        impl $name {
            /// The layout of the type, for the binding check.
            #[cfg(test)]
            pub(crate) fn layout() -> $crate::ffi::checks::StructItem {
                $crate::ffi::checks::StructItem {
                    name: $cname,
                    size: size_of::<Self>(),
                    fields: vec![$(
                        (
                            $crate::ffi::checks::c_field(stringify!($field)),
                            std::mem::offset_of!(Self, $field),
                            size_of::<$fty>(),
                        ),
                    )*],
                }
            }
        }
    )*};
}

/// Declares opaque C types: a pointer to one is a handle, and Rust never
/// reads through it.
macro_rules! opaque_types {
    ($($(#[$meta:meta])* $vis:vis $name:ident = $cname:literal;)*) => {$(
        $(#[$meta])*
        #[repr(C)]
        $vis struct $name {
            _data: [u8; 0],
            _marker: std::marker::PhantomData<(*mut u8, std::marker::PhantomPinned)>,
        }

        #[cfg(test)]
        impl $crate::ffi::ctype::CType for $name {
            fn spelling() -> String {
                $cname.to_owned()
            }
        }
    )*};
}

/// Declares a C enum type as a transparent integer, and its constants.
/// The binding check compares each value with the header.
macro_rules! ffi_enum {
    ($(
        $(#[$meta:meta])*
        $vis:vis enum $name:ident: $repr:ty = $cname:literal {
            $($(#[$cmeta:meta])* $constant:ident = $value:expr,)*
        }
    )*) => {$(
        $(#[$meta])*
        #[repr(transparent)]
        #[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
        $vis struct $name(pub(crate) $repr);

        #[allow(dead_code, reason = "the table lists every value of the C enum that the crate may use")]
        impl $name {
            $($(#[$cmeta])* pub(crate) const $constant: Self = Self($value);)*
        }

        #[cfg(test)]
        impl $crate::ffi::ctype::CType for $name {
            fn spelling() -> String {
                $cname.to_owned()
            }
        }

        impl $name {
            /// The constants, for the binding check.
            #[cfg(test)]
            pub(crate) fn constants() -> Vec<$crate::ffi::checks::ConstItem> {
                vec![$($crate::ffi::checks::ConstItem {
                    name: stringify!($constant),
                    value: i128::from(Self::$constant.0),
                }),*]
            }
        }
    )*};
}

/// Declares the values of C macros and enum constants. The binding check
/// compares each value with the header. A constant whose C name is not an
/// upper-case Rust name gives it after `=>`.
macro_rules! c_consts {
    (
        $(#[$tmeta:meta])*
        $tvis:vis fn $table:ident;
        $($(#[$meta:meta])* $vis:vis const $name:ident: $ty:ty = $value:expr $(=> $cname:literal)?;)*
    ) => {
        $($(#[$meta])* $vis const $name: $ty = $value;)*

        $(#[$tmeta])*
        #[cfg(test)]
        $tvis fn $table() -> Vec<$crate::ffi::checks::ConstItem> {
            vec![$($crate::ffi::checks::ConstItem {
                name: $crate::ffi::macros::c_consts!(@name $name $($cname)?),
                value: i128::from($name),
            }),*]
        }
    };
    (@name $name:ident) => { stringify!($name) };
    (@name $name:ident $cname:literal) => { $cname };
}

/// Declares extension functions that a library returns from a lookup
/// call, such as `eglGetProcAddress`, instead of exporting them. Each
/// function names the typedef of its header. The binding check converts
/// a typedef pointer to the declared type, so a declaration cannot drift.
///
/// ```text
/// lookup_functions! {
///     /// What the extensions are for.
///     pub(crate) struct EglExtensions in Egl {
///         fn eglSwapBuffersWithDamageKHR(dpy: *mut c_void) -> u32
///             = "PFNEGLSWAPBUFFERSWITHDAMAGEKHRPROC";
///     }
/// }
/// ```
///
/// `load` asks `lookup` for each name. A null address is `None`.
macro_rules! lookup_functions {
    (
        $(#[$meta:meta])*
        $vis:vis struct $table:ident in $library:ident {$(
            $(#[$fmeta:meta])*
            fn $name:ident($($arg:ident : $ty:ty),* $(,)?) $(-> $ret:ty)? = $typedef:literal;
        )*}
    ) => {
        $(#[$meta])*
        #[allow(non_snake_case, reason = "the fields keep the C symbol names")]
        #[derive(Clone, Copy)]
        $vis struct $table {
            $(
                $(#[$fmeta])*
                pub(crate) $name: Option<unsafe extern "C" fn($($ty),*) $(-> $ret)?>,
            )*
        }

        impl $table {
            /// Looks up each function. `lookup` returns the address of a
            /// NUL-terminated name, or null when the function is missing.
            ///
            /// # Safety
            ///
            /// A non-null address that `lookup` returns for a name is the
            /// C function of that name.
            pub(crate) unsafe fn load(lookup: impl Fn(&'static str) -> *mut $crate::ffi::ctypes::c_void) -> Self {
                Self {
                    $(
                        $name: {
                            let symbol = lookup(concat!(stringify!($name), "\0"));
                            if symbol.is_null() {
                                None
                            } else {
                                // SAFETY: the caller guarantees the function
                                // of the name, and the binding check compares
                                // the declared signature with its typedef.
                                Some(unsafe {
                                    $crate::ffi::dl::function::<
                                        unsafe extern "C" fn($($ty),*) $(-> $ret)?,
                                    >(symbol)
                                })
                            }
                        },
                    )*
                }
            }

            /// The descriptor of each function, for the binding check.
            #[cfg(test)]
            pub(crate) fn check_items() -> $crate::ffi::checks::LibraryItems {
                use $crate::ffi::ctype::CType;
                $crate::ffi::checks::LibraryItems {
                    library: $library::LIBRARY,
                    functions: vec![$(
                        $crate::ffi::checks::FunctionItem {
                            name: stringify!($name),
                            ret: <$crate::ffi::macros::lookup_functions!(@ret $($ret)?) as CType>::spelling(),
                            args: vec![$(<$ty as CType>::spelling()),*],
                            variadic: false,
                            typedef: Some($typedef),
                        },
                    )*],
                }
            }
        }
    };
    (@ret) => { () };
    (@ret $ret:ty) => { $ret };
}

/// Declares a C callback.The body runs inside `catch_unwind`; a panic
/// writes the boundary name to standard error, and the callback returns
/// the fallback. A panic never unwinds into C.
///
/// ```text
/// callback! {
///     /// What C calls it for.
///     /// # Safety
///     /// What C guarantees about the arguments.
///     pub(crate) fn on_event(data: *mut c_void) -> i32
///         [boundary "wayland dispatcher", fallback -1]
///     {
///         ...
///     }
/// }
/// ```
macro_rules! callback {
    ($(
        $(#[$meta:meta])*
        $vis:vis fn $name:ident($($arg:ident : $ty:ty),* $(,)?) $(-> $ret:ty)?
            [boundary $boundary:literal, fallback $fallback:expr]
        $body:block
    )*) => {$(
        $(#[$meta])*
        $vis unsafe extern "C" fn $name($($arg: $ty),*) $(-> $ret)? {
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| $body)) {
                Ok(value) => value,
                Err(_) => {
                    $crate::ffi::report_callback_panic($boundary);
                    $fallback
                }
            }
        }
    )*};
}

pub(crate) use {
    c_consts, callback, ffi_enum, ffi_struct, ffi_union, libc_functions, libraries,
    lookup_functions, opaque_types,
};
