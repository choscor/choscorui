//! The C spelling of each Rust type of a declaration, for the binding
//! check. A spelling is a C type name that the check can put in a cast or
//! a function pointer type. Qualifiers follow the type that they qualify,
//! so `*const *const CChar` is `char const * const *`.

use super::ctypes::{CChar, c_void};

/// A Rust type with a C spelling.
pub(crate) trait CType {
    /// The C type, for example `const char *` or `uint32_t`.
    fn spelling() -> String;
}

/// Implements [`CType`] for types with a fixed spelling.
macro_rules! spelled {
    ($($ty:ty => $c:literal),* $(,)?) => {$(
        impl CType for $ty {
            fn spelling() -> String {
                $c.to_owned()
            }
        }
    )*};
}

spelled!(
    () => "void",
    c_void => "void",
    CChar => "char",
    bool => "bool",
    i8 => "int8_t",
    u8 => "uint8_t",
    i16 => "int16_t",
    u16 => "uint16_t",
    i32 => "int32_t",
    u32 => "uint32_t",
    i64 => "int64_t",
    u64 => "uint64_t",
    isize => "ssize_t",
    usize => "size_t",
    f32 => "float",
    f64 => "double",
);

impl<T: CType> CType for *const T {
    fn spelling() -> String {
        format!("{} const *", T::spelling())
    }
}

impl<T: CType> CType for *mut T {
    fn spelling() -> String {
        format!("{} *", T::spelling())
    }
}

/// Implements [`CType`] for C function pointers of each arity, with and
/// without `Option`.
macro_rules! function_spelling {
    ($($arg:ident),*) => {
        impl<R: CType $(, $arg: CType)*> CType for unsafe extern "C" fn($($arg),*) -> R {
            fn spelling() -> String {
                let args: Vec<String> = vec![$($arg::spelling()),*];
                let args = if args.is_empty() { "void".to_owned() } else { args.join(", ") };
                format!("{} (*)({})", R::spelling(), args)
            }
        }
        impl<R: CType $(, $arg: CType)*> CType for Option<unsafe extern "C" fn($($arg),*) -> R> {
            fn spelling() -> String {
                <unsafe extern "C" fn($($arg),*) -> R as CType>::spelling()
            }
        }
    };
}

function_spelling!();
function_spelling!(A0);
function_spelling!(A0, A1);
function_spelling!(A0, A1, A2);
function_spelling!(A0, A1, A2, A3);
function_spelling!(A0, A1, A2, A3, A4);
function_spelling!(A0, A1, A2, A3, A4, A5);
function_spelling!(A0, A1, A2, A3, A4, A5, A6);
function_spelling!(A0, A1, A2, A3, A4, A5, A6, A7);
