//! The boundary scan: stringly-typed FFI stays inside `src/ffi`.
//!
//! The `unsafe_code` lint keeps `unsafe` inside `ffi`. This scan finds the
//! patterns that the lint cannot see: selector literals, raw message
//! sends, transmutes, C ABI declarations, and methods added to a class
//! outside `define_classes!`.

use std::path::{Path, PathBuf};

/// The text that no file outside `src/ffi` contains.
const OUTSIDE_FFI: &[&str] = &[
    "objc_msgSend",
    "transmute",
    "extern \"C\"",
    "class_addMethod",
];

/// True when `line` holds a selector literal: a C string that ends in `:`.
fn has_selector_literal(line: &str) -> bool {
    let mut rest = line;
    while let Some(start) = rest.find("c\"") {
        let literal = &rest[start + 2..];
        let Some(end) = literal.find('"') else {
            return false;
        };
        if literal[..end].ends_with(':') {
            return true;
        }
        rest = &literal[end + 1..];
    }
    false
}

/// Reports each line of a file outside `ffi` that crosses the boundary.
fn scan_outside(path: &str, source: &str, failures: &mut Vec<String>) {
    for (number, line) in source.lines().enumerate() {
        let number = number + 1;
        if has_selector_literal(line) {
            failures.push(format!("{path}:{number}: selector literal outside ffi"));
        }
        for pattern in OUTSIDE_FFI {
            if line.contains(pattern) {
                failures.push(format!("{path}:{number}: {pattern} outside ffi"));
            }
        }
    }
}

/// Reports each line of a file in `ffi` that adds a method to a class
/// without `define_classes!`. Only the runtime wrapper calls
/// `class_addMethod`, and only `callback.rs`, which holds the macro, calls
/// the wrapper.
fn scan_inside(path: &str, source: &str, failures: &mut Vec<String>) {
    let file = Path::new(path).file_name().and_then(|name| name.to_str());
    for (number, line) in source.lines().enumerate() {
        let number = number + 1;
        if line.contains("class_addMethod") && file != Some("runtime.rs") {
            failures.push(format!(
                "{path}:{number}: class_addMethod outside the runtime wrapper"
            ));
        }
        if line.contains("runtime::add_method") && file != Some("callback.rs") {
            failures.push(format!(
                "{path}:{number}: a method added outside define_classes!"
            ));
        }
    }
}

/// Checks the sources: each entry is a path relative to the crate root
/// and the file text.
pub(crate) fn check_sources<'a>(
    sources: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> Result<(), String> {
    let mut failures = Vec::new();
    for (path, source) in sources {
        // The checks quote the patterns that they look for.
        if path.starts_with("src/ffi/checks/") {
            continue;
        }
        if path.starts_with("src/ffi/") {
            scan_inside(path, source, &mut failures);
        } else {
            scan_outside(path, source, &mut failures);
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "{} lines cross the FFI boundary:\n{}",
            failures.len(),
            failures.join("\n")
        ))
    }
}

/// Every `.rs` file under `directory`, sorted.
fn rust_files(directory: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(directory)? {
        let path = entry?.path();
        if path.is_dir() {
            rust_files(&path, out)?;
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            out.push(path);
        }
    }
    out.sort();
    Ok(())
}

/// The path and text of each source file of the crate.
pub(crate) fn crate_sources() -> Vec<(String, String)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    rust_files(&root.join("src"), &mut files).unwrap();
    files
        .into_iter()
        .map(|path| {
            let text = std::fs::read_to_string(&path).unwrap();
            let relative = path.strip_prefix(root).unwrap();
            (relative.to_string_lossy().replace('\\', "/"), text)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selector_literals_are_found() {
        assert!(has_selector_literal(r#"const A: &CStr = c"doIt:";"#));
        assert!(has_selector_literal(r#"f(c"name", c"a:b:")"#));
        assert!(!has_selector_literal(r#"f(c"NSWindow", "a:")"#));
    }

    #[test]
    fn every_crossing_names_its_file_and_line() {
        let outside = "fn f() {}\nlet s = c\"close:\";\nstd::mem::transmute(x);\n";
        let inside = "runtime::add_method(class, sel, imp, types);\n";
        let report =
            check_sources([("src/menu.rs", outside), ("src/ffi/appkit.rs", inside)]).unwrap_err();
        assert!(report.starts_with("3 lines"), "{report}");
        assert!(
            report.contains("src/menu.rs:2: selector literal"),
            "{report}"
        );
        assert!(report.contains("src/menu.rs:3: transmute"), "{report}");
        assert!(
            report.contains("src/ffi/appkit.rs:1: a method added outside define_classes!"),
            "{report}"
        );
    }

    #[test]
    fn the_runtime_and_the_callback_macro_may_add_methods() {
        let wrapper = "unsafe { class_addMethod(class, name, imp, types) }\n";
        let macro_use = "unsafe { runtime::add_method(class, selector, imp, types) }\n";
        assert!(
            check_sources([
                ("src/ffi/runtime.rs", wrapper),
                ("src/ffi/callback.rs", macro_use),
            ])
            .is_ok()
        );
    }
}
