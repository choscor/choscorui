//! The boundary scan: FFI stays inside `src/ffi`.
//!
//! The `unsafe_code` lint keeps `unsafe` inside `ffi`. This scan finds the
//! patterns that the lint cannot see: C ABI declarations, symbol lookups,
//! transmutes, and raw pointer types.

use std::path::{Path, PathBuf};

/// The text that no file outside `src/ffi` contains.
const OUTSIDE_FFI: &[&str] = &[
    "extern \"C\"",
    "dlsym",
    "dlopen",
    "transmute",
    "*const ",
    "*mut ",
];

/// Checks the sources: each entry is a path relative to the crate root
/// and the file text.
fn check_sources<'a>(sources: impl IntoIterator<Item = (&'a str, &'a str)>) -> Result<(), String> {
    let mut failures = Vec::new();
    for (path, source) in sources {
        if path.starts_with("src/ffi/") {
            continue;
        }
        for (number, line) in source.lines().enumerate() {
            let code = line.split("//").next().unwrap_or_default();
            for pattern in OUTSIDE_FFI {
                if code.contains(pattern) {
                    failures.push(format!("{path}:{}: {pattern} outside ffi", number + 1));
                }
            }
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
fn crate_sources() -> Vec<(String, String)> {
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

#[test]
fn boundary_is_closed() {
    let sources = crate_sources();
    let result = check_sources(
        sources
            .iter()
            .map(|(path, text)| (path.as_str(), text.as_str())),
    );
    if let Err(report) = result {
        panic!("{report}");
    }
}

#[test]
fn every_crossing_names_its_file_and_line() {
    let outside = "fn f() {}\nlet x = std::mem::transmute(y);\nunsafe extern \"C\" { }\n";
    let inside = "unsafe { dlsym(handle, name) }\n";
    let report =
        check_sources([("src/window.rs", outside), ("src/ffi/dl.rs", inside)]).unwrap_err();
    assert!(report.starts_with("2 lines"), "{report}");
    assert!(report.contains("src/window.rs:2: transmute"), "{report}");
    assert!(report.contains("src/window.rs:3: extern \"C\""), "{report}");
}
