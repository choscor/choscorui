//! The rules of the D-Bus specification for names, object paths, and
//! type signatures. libdbus stops the process when it gets an invalid
//! name, so the crate checks each name before it reaches libdbus.

/// The longest name or signature that the specification allows.
const MAX_LENGTH: usize = 255;

/// The deepest nesting of arrays, and of structs, in one signature.
const MAX_NESTING: usize = 32;

fn element_ok(element: &str, first: impl Fn(u8) -> bool, rest: impl Fn(u8) -> bool) -> bool {
    let bytes = element.as_bytes();
    !bytes.is_empty() && first(bytes[0]) && bytes[1..].iter().all(|&byte| rest(byte))
}

fn word_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn word(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// True for an object path, such as `/org/example/Item`.
pub fn is_object_path(path: &str) -> bool {
    if path == "/" {
        return true;
    }
    path.strip_prefix('/').is_some_and(|rest| {
        rest.split('/')
            .all(|segment| element_ok(segment, word, word))
    })
}

/// True for an interface name, such as `org.freedesktop.Notifications`.
pub fn is_interface_name(name: &str) -> bool {
    name.len() <= MAX_LENGTH
        && name.split('.').count() >= 2
        && name
            .split('.')
            .all(|element| element_ok(element, word_start, word))
}

/// True for an error name. Error names follow the interface rules.
pub fn is_error_name(name: &str) -> bool {
    is_interface_name(name)
}

/// True for a member name: a method or a signal, such as `Notify`.
pub fn is_member_name(name: &str) -> bool {
    name.len() <= MAX_LENGTH && element_ok(name, word_start, word)
}

/// True for a bus name: a unique name such as `:1.42`, or a well-known
/// name such as `org.kde.StatusNotifierWatcher`.
pub fn is_bus_name(name: &str) -> bool {
    if name.len() > MAX_LENGTH {
        return false;
    }
    let dash_word = |byte: u8| word(byte) || byte == b'-';
    if let Some(unique) = name.strip_prefix(':') {
        return unique.split('.').count() >= 2
            && unique
                .split('.')
                .all(|element| element_ok(element, dash_word, dash_word));
    }
    name.split('.').count() >= 2
        && name
            .split('.')
            .all(|element| element_ok(element, |byte| word_start(byte) || byte == b'-', dash_word))
}

/// True for a basic type code: the types that a dictionary key can have.
pub fn is_basic_type(code: u8) -> bool {
    b"ybnqiuxtdsogh".contains(&code)
}

/// The length of the single complete type at the start of `signature`,
/// or `None` when it does not start with one. `arrays` and `structs`
/// count the enclosing containers.
fn complete_type(signature: &[u8], arrays: usize, structs: usize) -> Option<usize> {
    let (&code, rest) = signature.split_first()?;
    match code {
        _ if is_basic_type(code) || code == b'v' => Some(1),
        b'a' => {
            if arrays >= MAX_NESTING {
                return None;
            }
            if rest.first() == Some(&b'{') {
                // A dictionary entry: a basic key and one value type.
                let key = *rest.get(1)?;
                if !is_basic_type(key) {
                    return None;
                }
                let value = complete_type(rest.get(2..)?, arrays + 1, structs)?;
                (rest.get(2 + value) == Some(&b'}')).then_some(value + 4)
            } else {
                complete_type(rest, arrays + 1, structs).map(|length| length + 1)
            }
        }
        b'(' => {
            if structs >= MAX_NESTING {
                return None;
            }
            let mut offset = 0;
            loop {
                match rest.get(offset)? {
                    b')' if offset > 0 => return Some(offset + 2),
                    _ => offset += complete_type(&rest[offset..], arrays, structs + 1)?,
                }
            }
        }
        _ => None,
    }
}

/// True for a signature: zero or more single complete types.
pub fn is_signature(signature: &str) -> bool {
    let bytes = signature.as_bytes();
    if bytes.len() > MAX_LENGTH {
        return false;
    }
    let mut offset = 0;
    while offset < bytes.len() {
        match complete_type(&bytes[offset..], 0, 0) {
            Some(length) => offset += length,
            None => return false,
        }
    }
    true
}

/// True for a signature of exactly one complete type, such as `a{sv}`.
pub fn is_single_type(signature: &str) -> bool {
    signature.len() <= MAX_LENGTH
        && complete_type(signature.as_bytes(), 0, 0) == Some(signature.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn object_paths_follow_the_specification() {
        assert!(is_object_path("/"));
        assert!(is_object_path("/org/freedesktop/portal/desktop"));
        assert!(is_object_path("/a_1/B2"));
        assert!(!is_object_path(""));
        assert!(!is_object_path("org"));
        assert!(!is_object_path("/org/"));
        assert!(!is_object_path("//org"));
        assert!(!is_object_path("/org.example"));
    }

    #[test]
    fn names_follow_the_specification() {
        assert!(is_interface_name("org.freedesktop.Notifications"));
        assert!(!is_interface_name("Notifications"));
        assert!(!is_interface_name("org.1freedesktop"));
        assert!(!is_interface_name("org..x"));
        assert!(is_member_name("Notify"));
        assert!(!is_member_name("2Notify"));
        assert!(!is_member_name("No-tify"));
        assert!(is_bus_name(":1.42"));
        assert!(is_bus_name("org.kde.StatusNotifierItem-12-1"));
        assert!(!is_bus_name("org"));
        assert!(!is_bus_name("org.kde.1x"));
        assert!(is_error_name("org.freedesktop.DBus.Error.Failed"));
    }

    #[test]
    fn signatures_follow_the_specification() {
        for good in ["", "s", "a{sv}", "(ia{sv}as)", "aa(iiay)", "a{s(bv)}", "v"] {
            assert!(is_signature(good), "{good}");
        }
        for bad in ["a", "()", "a{vs}", "a{s}", "(s", "a{sv", "{sv}", "z"] {
            assert!(!is_signature(bad), "{bad}");
        }
        assert!(is_single_type("a{sv}"));
        assert!(!is_single_type("ss"));
        assert!(!is_single_type(""));
        let deep = format!("{}y", "a".repeat(33));
        assert!(!is_signature(&deep));
    }
}
