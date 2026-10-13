//! Objective-C type encodings of the FFI types.
//!
//! Every type that crosses the boundary implements [`Encode`]. The runtime
//! check compares the encodings with the Objective-C runtime. All
//! functions here are pure.

/// The Objective-C type encoding of one FFI type.
#[cfg(test)]
pub(crate) trait Encode {
    /// Appends the Objective-C type encoding, for example `@` or `q`.
    fn encode(out: &mut String);
}

/// Implements [`Encode`] for a type with a fixed encoding and spelling.
#[cfg(test)]
macro_rules! encode_as {
    ($ty:ty, $encoding:literal) => {
        impl $crate::ffi::encode::Encode for $ty {
            fn encode(out: &mut String) {
                out.push_str($encoding);
            }
        }
    };
}
#[cfg(test)]
pub(crate) use encode_as;

#[cfg(test)]
encode_as!((), "v");
#[cfg(test)]
encode_as!(isize, "q");
#[cfg(test)]
encode_as!(usize, "Q");
#[cfg(test)]
encode_as!(i32, "i");
#[cfg(test)]
encode_as!(u32, "I");
#[cfg(test)]
encode_as!(i16, "s");
#[cfg(test)]
encode_as!(u16, "S");
#[cfg(test)]
encode_as!(i8, "c");
#[cfg(test)]
encode_as!(u8, "C");
#[cfg(test)]
encode_as!(f64, "d");
#[cfg(test)]
encode_as!(std::ffi::c_void, "v");

/// Objective-C `BOOL`: `bool` on arm64 and `signed char` on x86-64. Rust
/// reads it as one byte, so a value other than 0 or 1 is still defined.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Bool(i8);

impl Bool {
    pub(crate) const fn new(value: bool) -> Self {
        Self(value as i8)
    }
    pub(crate) const fn get(self) -> bool {
        self.0 != 0
    }
}

#[cfg(test)]
impl Encode for Bool {
    fn encode(out: &mut String) {
        out.push(if cfg!(target_arch = "aarch64") {
            'B'
        } else {
            'c'
        });
    }
}

/// Removes the stack offsets and the method qualifiers `r n N o O R V`
/// from a method type encoding, so that `v24@0:8@16` becomes `v@:@` and
/// `r*16@0:8` becomes `*@:`. Text inside structs, arrays, and unions stays
/// unchanged, because there a digit is an array count or a bit-field width.
#[cfg(test)]
pub(crate) fn normalize(types: &str) -> String {
    let mut out = String::with_capacity(types.len());
    let mut depth = 0usize;
    for character in types.chars() {
        match character {
            '{' | '[' | '(' => depth += 1,
            '}' | ']' | ')' => depth = depth.saturating_sub(1),
            _ if depth > 0 => {}
            '0'..='9' | 'r' | 'n' | 'N' | 'o' | 'O' | 'R' | 'V' => continue,
            _ => {}
        }
        out.push(character);
    }
    out
}

/// Compares a derived encoding with a runtime encoding after normalizing
/// both sides.
#[cfg(test)]
pub(crate) fn same_encoding(derived: &str, runtime: &str) -> bool {
    normalize(derived) == normalize(runtime)
}

/// The Cocoa ownership family of a selector, after leading underscores:
/// `alloc`, `new`, `copy`, `mutableCopy`, or `init`, followed by the end,
/// a colon, or an uppercase letter.
#[cfg(test)]
pub(crate) fn selector_family(selector: &str) -> Option<&'static str> {
    let name = selector.trim_start_matches('_');
    ["alloc", "new", "copy", "mutableCopy", "init"]
        .into_iter()
        .find(|family| {
            name.strip_prefix(family).is_some_and(|rest| {
                rest.chars()
                    .next()
                    .is_none_or(|next| next == ':' || next.is_ascii_uppercase())
            })
        })
}

/// True when a C function name follows the Core Foundation create rule:
/// it contains `Create` or `Copy`, so the caller owns the result.
#[cfg(test)]
pub(crate) fn follows_create_rule(function: &str) -> bool {
    function.contains("Create") || function.contains("Copy")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encoding<T: Encode>() -> String {
        let mut out = String::new();
        T::encode(&mut out);
        out
    }

    #[test]
    fn normalize_removes_offsets_and_qualifiers() {
        assert_eq!(normalize("v24@0:8@16"), "v@:@");
        assert_eq!(normalize("r*16@0:8"), "*@:");
        assert_eq!(normalize("Vv16@0:8"), "v@:");
        assert_eq!(normalize("@32@0:8r^@16Q24"), "@@:^@Q");
        assert_eq!(
            normalize("@68@0:8{CGRect={CGPoint=dd}{CGSize=dd}}16Q48Q56B64"),
            "@@:{CGRect={CGPoint=dd}{CGSize=dd}}QQB"
        );
        assert_eq!(normalize("v40@0:8@16@24@?32"), "v@:@@@?");
    }

    #[test]
    fn normalize_keeps_struct_contents() {
        assert_eq!(normalize("v@:{x=[4c]b3}"), "v@:{x=[4c]b3}");
        assert_eq!(normalize("v@:{rN=d}"), "v@:{rN=d}");
    }

    #[test]
    fn comparison_normalizes_both_sides() {
        assert!(same_encoding("v@:@", "v24@0:8@16"));
        assert!(!same_encoding("v@:q", "v24@0:8Q16"));
    }

    #[test]
    fn primitive_encodings_match_the_runtime_spelling() {
        assert_eq!(encoding::<()>(), "v");
        assert_eq!(encoding::<isize>(), "q");
        assert_eq!(encoding::<usize>(), "Q");
        assert_eq!(encoding::<u16>(), "S");
        assert_eq!(encoding::<f64>(), "d");
        let expected = if cfg!(target_arch = "aarch64") {
            "B"
        } else {
            "c"
        };
        assert_eq!(encoding::<Bool>(), expected);
    }

    #[test]
    fn bool_keeps_any_nonzero_byte() {
        assert!(Bool(2).get());
        assert!(!Bool::new(false).get());
        assert!(Bool::new(true).get());
    }

    #[test]
    fn ownership_families_follow_the_cocoa_rules() {
        assert_eq!(selector_family("alloc"), Some("alloc"));
        assert_eq!(selector_family("new"), Some("new"));
        assert_eq!(selector_family("initWithFrame:"), Some("init"));
        assert_eq!(selector_family("copyWithZone:"), Some("copy"));
        assert_eq!(selector_family("mutableCopy"), Some("mutableCopy"));
        assert_eq!(selector_family("_init"), Some("init"));
        assert_eq!(selector_family("newspaper"), None);
        assert_eq!(selector_family("initialize"), None);
        assert_eq!(selector_family("stringWithUTF8String:"), None);
        assert!(follows_create_rule("CTLineCreateWithAttributedString"));
        assert!(follows_create_rule("CTFontCopyPostScriptName"));
        assert!(!follows_create_rule("CTLineGetGlyphRuns"));
    }
}
