//! The binding check: every declaration of the `ffi` macros compiled
//! against the system headers.
//!
//! The check writes one C file. It assigns each loaded symbol to a
//! function pointer of the declared Rust signature, asserts the size and
//! the field offsets of each struct, and asserts the value of each
//! constant. It compiles the file with `cc -Werror` and the flags that
//! `pkg-config` gives for each library. A mismatch fails the test, and the
//! report names each symbol, struct, or constant on a failing line.

mod boundary;
mod protocols;

use super::dl::Library;
use std::fmt::Write as _;
use std::process::Command;

/// The declarations of one library.
pub(crate) struct LibraryItems {
    pub(crate) library: Library,
    pub(crate) functions: Vec<FunctionItem>,
}

/// One declared C function.
pub(crate) struct FunctionItem {
    pub(crate) name: &'static str,
    pub(crate) ret: String,
    pub(crate) args: Vec<String>,
    pub(crate) variadic: bool,
    /// The header's function pointer typedef, for a function that a
    /// lookup call returns instead of a symbol.
    pub(crate) typedef: Option<&'static str>,
}

/// The layout of one declared struct or union: its C name, its size, and
/// the C name, offset, and size of each field.
pub(crate) struct StructItem {
    pub(crate) name: &'static str,
    pub(crate) size: usize,
    pub(crate) fields: Vec<(String, usize, usize)>,
}

/// One declared constant.
pub(crate) struct ConstItem {
    pub(crate) name: &'static str,
    pub(crate) value: i128,
}

/// The C name of a Rust field: a raw identifier loses its `r#`.
pub(crate) fn c_field(rust: &str) -> String {
    rust.trim_start_matches("r#").to_owned()
}

/// One line of the generated file and the item that it checks.
struct Line {
    text: String,
    item: String,
}

/// The C function pointer declarator of `function`, named `check`.
fn function_pointer(function: &FunctionItem) -> String {
    let mut args = function.args.join(", ");
    if function.variadic {
        args.push_str(", ...");
    } else if args.is_empty() {
        args.push_str("void");
    }
    format!(
        "{} (*const check_{})({})",
        function.ret, function.name, args
    )
}

/// The lines of the C file, without the includes.
fn check_lines(
    libraries: &[LibraryItems],
    structs: &[StructItem],
    constants: &[ConstItem],
) -> Vec<Line> {
    let mut lines = Vec::new();
    for library in libraries {
        for function in &library.functions {
            lines.push(Line {
                text: match function.typedef {
                    // A null pointer of the typedef converts to the declared
                    // type only when the two types match.
                    Some(typedef) => format!(
                        "__attribute__((unused)) static {} = ({typedef})0;",
                        function_pointer(function)
                    ),
                    None => format!(
                        "__attribute__((unused)) static {} = {};",
                        function_pointer(function),
                        function.name
                    ),
                },
                item: format!("{} in {}", function.name, library.library.name),
            });
        }
    }
    for item in structs {
        lines.push(Line {
            text: format!(
                "_Static_assert(sizeof({}) == {}, \"size of {}\");",
                item.name, item.size, item.name
            ),
            item: format!("struct {}", item.name),
        });
        for (field, offset, size) in &item.fields {
            lines.push(Line {
                text: format!(
                    "_Static_assert(offsetof({}, {field}) == {offset}, \"offset of {}.{field}\");",
                    item.name, item.name
                ),
                item: format!("field {}.{field}", item.name),
            });
            lines.push(Line {
                text: format!(
                    "_Static_assert(sizeof((({} *)0)->{field}) == {size}, \"size of {}.{field}\");",
                    item.name, item.name
                ),
                item: format!("field {}.{field}", item.name),
            });
        }
    }
    for constant in constants {
        let (cast, value) = if constant.value < 0 {
            ("long long", format!("{}LL", constant.value))
        } else {
            ("unsigned long long", format!("{}ULL", constant.value))
        };
        lines.push(Line {
            text: format!(
                "_Static_assert(({cast})({}) == {value}, \"value of {}\");",
                constant.name, constant.name
            ),
            item: format!("constant {}", constant.name),
        });
    }
    lines
}

/// The C file: the includes of every library, then the check lines. The
/// result also gives the first line number of the checks.
fn c_source(libraries: &[LibraryItems], lines: &[Line]) -> (String, usize) {
    let mut source = String::from(
        "#define _GNU_SOURCE\n#include <stdbool.h>\n#include <stddef.h>\n\
         #include <stdint.h>\n#include <sys/types.h>\n",
    );
    let mut seen = Vec::new();
    for library in libraries {
        for header in library.library.headers {
            if !seen.contains(header) {
                seen.push(*header);
                let _ = writeln!(source, "#include <{header}>");
            }
        }
    }
    let first = source.lines().count() + 1;
    for line in lines {
        source.push_str(&line.text);
        source.push('\n');
    }
    (source, first)
}

/// The compiler flags of the headers of `libraries`, from `pkg-config`.
/// A module that `pkg-config` does not find fails with its package.
fn header_flags(libraries: &[LibraryItems]) -> Result<Vec<String>, String> {
    let mut modules: Vec<&Library> = Vec::new();
    for library in libraries {
        let module = library.library.pkg_config;
        if !module.is_empty() && !modules.iter().any(|known| known.pkg_config == module) {
            modules.push(&library.library);
        }
    }
    let mut flags = Vec::new();
    let mut missing = Vec::new();
    for library in modules {
        let output = Command::new("pkg-config")
            .args(["--cflags", library.pkg_config])
            .output();
        match output {
            Ok(output) if output.status.success() => {
                let text = String::from_utf8_lossy(&output.stdout).into_owned();
                flags.extend(text.split_whitespace().map(str::to_owned));
            }
            Ok(_) => missing.push(format!("{} ({})", library.pkg_config, library.package)),
            Err(error) => {
                return Err(format!(
                    "pkg-config did not start ({error}). Install pkg-config and the -dev \
                     packages that CONTRIBUTING.md lists"
                ));
            }
        }
    }
    if missing.is_empty() {
        Ok(flags)
    } else {
        Err(format!(
            "pkg-config does not find these modules. Install the -dev packages that \
             CONTRIBUTING.md lists: {}",
            missing.join(", ")
        ))
    }
}

/// The items that the failing lines of `diagnostics` check.
fn failing_items(diagnostics: &str, file: &str, first: usize, lines: &[Line]) -> Vec<String> {
    let mut items: Vec<String> = Vec::new();
    for diagnostic in diagnostics.lines() {
        let Some(rest) = diagnostic.strip_prefix(file) else {
            continue;
        };
        let number = rest
            .trim_start_matches(':')
            .split(':')
            .next()
            .and_then(|number| number.parse::<usize>().ok());
        if let Some(line) = number
            .and_then(|number| number.checked_sub(first))
            .and_then(|index| lines.get(index))
            && !items.contains(&line.item)
        {
            items.push(line.item.clone());
        }
    }
    items
}

/// Compiles the checks of the declarations and reports each mismatch.
pub(crate) fn check_bindings(
    libraries: &[LibraryItems],
    structs: &[StructItem],
    constants: &[ConstItem],
) -> Result<(), String> {
    let flags = header_flags(libraries)?;
    let lines = check_lines(libraries, structs, constants);
    let (source, first) = c_source(libraries, &lines);
    let directory =
        std::env::temp_dir().join(format!("choscorui-linux-check-{}", std::process::id()));
    std::fs::create_dir_all(&directory)
        .map_err(|error| format!("cannot create {}: {error}", directory.display()))?;
    // Tests run in parallel, so each check writes its own file.
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let number = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let file = directory.join(format!("binding_check_{number}.c"));
    std::fs::write(&file, &source).map_err(|error| format!("cannot write the check: {error}"))?;
    let output = Command::new("cc")
        .args(["-std=c11", "-Wall", "-Wextra", "-Werror", "-fsyntax-only"])
        .args(["-fmax-errors=0"])
        .args(&flags)
        .arg(&file)
        .output()
        .map_err(|error| format!("cc did not start ({error}). Install build-essential"))?;
    if output.status.success() {
        return Ok(());
    }
    let diagnostics = String::from_utf8_lossy(&output.stderr);
    let items = failing_items(&diagnostics, &file.to_string_lossy(), first, &lines);
    Err(format!(
        "the declarations do not match the system headers.\nmismatched: {}\n\n{}",
        if items.is_empty() {
            "(see the compiler output)".to_owned()
        } else {
            items.join(", ")
        },
        diagnostics
    ))
}

/// Every declaration of the crate.
fn crate_declarations() -> (Vec<LibraryItems>, Vec<StructItem>, Vec<ConstItem>) {
    super::declarations()
}

#[test]
fn bindings_match_the_system_headers() {
    let (libraries, structs, constants) = crate_declarations();
    if let Err(report) = check_bindings(&libraries, &structs, &constants) {
        panic!("{report}");
    }
}

#[test]
fn a_changed_declaration_fails_and_names_the_symbol() {
    let (mut libraries, mut structs, mut constants) = crate_declarations();
    // Change one argument of the first function, the size of the first
    // struct, and the value of the first constant.
    let function = libraries
        .iter_mut()
        .find_map(|library| library.functions.first_mut())
        .unwrap();
    function.args.push("double".to_owned());
    let function_name = function.name;
    structs[0].size += 8;
    let struct_name = structs[0].name;
    constants[0].value += 1;
    let constant_name = constants[0].name;
    let report = check_bindings(&libraries, &structs, &constants).unwrap_err();
    assert!(report.contains(function_name), "{report}");
    assert!(
        report.contains(&format!("struct {struct_name}")),
        "{report}"
    );
    assert!(
        report.contains(&format!("constant {constant_name}")),
        "{report}"
    );
}

#[test]
fn spellings_put_qualifiers_after_the_type() {
    use super::ctype::CType;
    use super::ctypes::CChar;
    assert_eq!(
        <*const *const CChar as CType>::spelling(),
        "char const * const *"
    );
    assert_eq!(
        <Option<unsafe extern "C" fn(*mut CChar, u32) -> i32> as CType>::spelling(),
        "int32_t (*)(char *, uint32_t)"
    );
}
