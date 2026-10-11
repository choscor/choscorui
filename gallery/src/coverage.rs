//! Keeps the catalog in step with `choscorui-controls`: every public name
//! the crate exports must be shown on a page or exempted with a reason.
//!
//! The parser is deliberately small and strict. It reads the crate root as
//! text (no third-party parser is allowed), understands the forms the root
//! uses today, and rejects anything else so that a new form updates the
//! parser instead of silently hiding names.

use crate::catalog::{EXEMPT, PAGES, SHOWN};
use std::collections::BTreeSet;

const CONTROLS_ROOT: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../crates/choscorui-controls/src/lib.rs"
);

/// Public names exported by a crate root, or the first construct the
/// parser does not understand as `(line, message)`.
pub fn exported_names(source: &str) -> Result<BTreeSet<String>, (usize, String)> {
    let mut names = BTreeSet::new();
    for (line, header) in top_level_items(&strip_comments_and_literals(source)) {
        classify(line, &header, &mut names)?;
    }
    Ok(names)
}

/// The outcome of comparing exports with the catalog.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Mismatch {
    /// Exported but in neither `SHOWN` nor `EXEMPT`.
    pub unclassified: Vec<String>,
    /// In the catalog but no longer exported.
    pub stale: Vec<String>,
    /// Listed more than once across `SHOWN` and `EXEMPT`.
    pub duplicated: Vec<String>,
}

impl Mismatch {
    pub fn is_empty(&self) -> bool {
        self.unclassified.is_empty() && self.stale.is_empty() && self.duplicated.is_empty()
    }
}

impl std::fmt::Display for Mismatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if !self.unclassified.is_empty() {
            writeln!(
                f,
                "unclassified (add a page entry in SHOWN or a reason in EXEMPT in gallery/src/catalog.rs): {}",
                self.unclassified.join(", ")
            )?;
        }
        if !self.stale.is_empty() {
            writeln!(
                f,
                "stale (in gallery/src/catalog.rs but no longer exported by choscorui-controls): {}",
                self.stale.join(", ")
            )?;
        }
        if !self.duplicated.is_empty() {
            writeln!(
                f,
                "listed more than once across SHOWN and EXEMPT: {}",
                self.duplicated.join(", ")
            )?;
        }
        Ok(())
    }
}

/// Compares exported names with the shown and exempt names.
pub fn compare(exports: &BTreeSet<String>, shown: &[&str], exempt: &[&str]) -> Mismatch {
    let mut seen = BTreeSet::new();
    let mut duplicated = BTreeSet::new();
    for name in shown.iter().chain(exempt) {
        if !seen.insert(*name) {
            duplicated.insert((*name).to_owned());
        }
    }
    Mismatch {
        unclassified: exports
            .iter()
            .filter(|name| !seen.contains(name.as_str()))
            .cloned()
            .collect(),
        stale: seen
            .iter()
            .filter(|name| !exports.contains(**name))
            .map(|name| (*name).to_owned())
            .collect(),
        duplicated: duplicated.into_iter().collect(),
    }
}

/// Replaces comments, string literals, and character literals with spaces,
/// keeping newlines so line numbers still match.
fn strip_comments_and_literals(source: &str) -> String {
    let chars: Vec<char> = source.chars().collect();
    let mut out = String::with_capacity(source.len());
    let mut i = 0;
    let blank = |c: char| if c == '\n' { '\n' } else { ' ' };
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        if c == '/' && next == Some('/') {
            while i < chars.len() && chars[i] != '\n' {
                out.push(' ');
                i += 1;
            }
        } else if c == '/' && next == Some('*') {
            let mut depth = 0;
            while i < chars.len() {
                if chars[i] == '/' && chars.get(i + 1) == Some(&'*') {
                    depth += 1;
                    out.push_str("  ");
                    i += 2;
                } else if chars[i] == '*' && chars.get(i + 1) == Some(&'/') {
                    depth -= 1;
                    out.push_str("  ");
                    i += 2;
                    if depth == 0 {
                        break;
                    }
                } else {
                    out.push(blank(chars[i]));
                    i += 1;
                }
            }
        } else if c == '"'
            || (c == 'r'
                && matches!(next, Some('"') | Some('#'))
                && !(i > 0 && (chars[i - 1].is_alphanumeric() || chars[i - 1] == '_')))
        {
            // Plain or raw string literal.
            let mut hashes = 0;
            if c == 'r' {
                out.push(' ');
                i += 1;
                while chars.get(i) == Some(&'#') {
                    hashes += 1;
                    out.push(' ');
                    i += 1;
                }
                if chars.get(i) != Some(&'"') {
                    // An identifier starting with `r`, not a raw string.
                    continue;
                }
            }
            out.push(' ');
            i += 1;
            while i < chars.len() {
                if c != 'r' && chars[i] == '\\' {
                    out.push(' ');
                    out.push(chars.get(i + 1).copied().map_or(' ', blank));
                    i += 2;
                    continue;
                }
                if chars[i] == '"' && (0..hashes).all(|k| chars.get(i + 1 + k) == Some(&'#')) {
                    for _ in 0..=hashes {
                        out.push(' ');
                    }
                    i += 1 + hashes;
                    break;
                }
                out.push(blank(chars[i]));
                i += 1;
            }
        } else if c == '\'' {
            // A character literal ('x', '\n', '\u{1F600}'); a lifetime has
            // no closing quote right after one character.
            let end = if next == Some('\\') {
                (i + 2..chars.len().min(i + 12)).find(|&k| chars[k] == '\'')
            } else if chars.get(i + 2) == Some(&'\'') {
                Some(i + 2)
            } else {
                None
            };
            match end {
                Some(end) => {
                    for _ in i..=end {
                        out.push(' ');
                    }
                    i = end + 1;
                }
                None => {
                    out.push(c);
                    i += 1;
                }
            }
        } else {
            out.push(c);
            i += 1;
        }
    }
    out
}

/// Splits sanitized source into top-level item headers with their starting
/// line: the text of each item up to its first top-level `{` or `;`, with
/// block bodies skipped. `use` items keep their braces.
fn top_level_items(source: &str) -> Vec<(usize, String)> {
    let mut items = Vec::new();
    let mut header = String::new();
    let mut start_line = 1;
    let mut line = 1;
    let mut depth = 0usize;
    let mut in_body = false;
    let mut use_braces = 0usize;
    for c in source.chars() {
        if c == '\n' {
            line += 1;
        }
        if in_body {
            match c {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        in_body = false;
                        header.clear();
                    }
                }
                _ => {}
            }
            continue;
        }
        if header.trim().is_empty() && !c.is_whitespace() {
            start_line = line;
        }
        let is_use = {
            let trimmed = strip_attributes(&header);
            trimmed.starts_with("use ") || trimmed.starts_with("pub use ")
        };
        match c {
            '{' if is_use => {
                use_braces += 1;
                header.push(c);
            }
            '}' if is_use && use_braces > 0 => {
                use_braces -= 1;
                header.push(c);
            }
            ';' if use_braces == 0 => {
                items.push((start_line, strip_attributes(&header)));
                header.clear();
            }
            '{' => {
                items.push((start_line, strip_attributes(&header)));
                in_body = true;
                depth = 1;
            }
            _ => header.push(c),
        }
    }
    if !strip_attributes(&header).is_empty() {
        items.push((start_line, strip_attributes(&header)));
    }
    items
}

/// Removes leading `#[...]` attributes and collapses whitespace.
fn strip_attributes(header: &str) -> String {
    let mut text = header.trim_start();
    while let Some(rest) = text.strip_prefix("#[").or_else(|| text.strip_prefix("#![")) {
        let mut depth = 1;
        let mut end = rest.len();
        for (index, c) in rest.char_indices() {
            match c {
                '[' => depth += 1,
                ']' => {
                    depth -= 1;
                    if depth == 0 {
                        end = index + 1;
                        break;
                    }
                }
                _ => {}
            }
        }
        text = rest[end..].trim_start();
    }
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn identifier(text: &str) -> Option<&str> {
    let end = text
        .find(|c: char| !(c.is_alphanumeric() || c == '_'))
        .unwrap_or(text.len());
    (end > 0).then(|| &text[..end])
}

fn classify(
    line: usize,
    header: &str,
    names: &mut BTreeSet<String>,
) -> Result<(), (usize, String)> {
    let unsupported = |what: &str| Err((line, format!("unsupported {what}: `{header}`")));
    if header.is_empty() || header.starts_with("pub(") {
        return Ok(());
    }
    if header.starts_with("macro_rules!")
        || identifier(header).is_some_and(|name| header[name.len()..].trim_start().starts_with('!'))
    {
        return unsupported("macro at the crate root (it may generate public items)");
    }
    let Some(rest) = header.strip_prefix("pub ") else {
        return Ok(());
    };
    if let Some(path) = rest.strip_prefix("use ") {
        return use_names(path.trim())
            .map(|found| names.extend(found))
            .or_else(|message| unsupported(&message));
    }
    let mut rest = rest;
    for qualifier in ["const fn ", "unsafe fn ", "async fn ", "extern \"C\" fn "] {
        if let Some(after) = rest.strip_prefix(qualifier) {
            rest = after;
            return match identifier(rest) {
                Some(name) => {
                    names.insert(name.to_owned());
                    Ok(())
                }
                None => unsupported("function"),
            };
        }
    }
    for keyword in [
        "struct", "enum", "trait", "fn", "const", "static", "type", "union",
    ] {
        if let Some(after) = rest.strip_prefix(keyword).and_then(|r| r.strip_prefix(' ')) {
            let after = after.strip_prefix("mut ").unwrap_or(after);
            return match identifier(after) {
                Some(name) => {
                    names.insert(name.to_owned());
                    Ok(())
                }
                None => unsupported(keyword),
            };
        }
    }
    if rest.starts_with("mod ") {
        return unsupported("public module");
    }
    unsupported("public item")
}

/// Names re-exported by `pub use <path>`.
fn use_names(path: &str) -> Result<Vec<String>, String> {
    if path.contains('*') {
        return Err("glob re-export".into());
    }
    if path.split_whitespace().any(|word| word == "as") {
        return Err("renamed re-export".into());
    }
    let leaf = |segment: &str| -> Result<String, String> {
        let name = segment.trim().rsplit("::").next().unwrap_or("").trim();
        match identifier(name) {
            Some(id) if id == name && id != "self" && id != "super" && id != "crate" => {
                Ok(id.to_owned())
            }
            _ => Err(format!("re-export segment `{segment}`")),
        }
    };
    match path.find('{') {
        None => Ok(vec![leaf(path)?]),
        Some(open) => {
            let inner = path[open + 1..]
                .strip_suffix('}')
                .ok_or_else(|| "re-export list without a closing brace".to_owned())?;
            if inner.contains('{') {
                return Err("nested re-export list".into());
            }
            inner
                .split(',')
                .filter(|segment| !segment.trim().is_empty())
                .map(leaf)
                .collect()
        }
    }
}

/// Names declared in a Cargo manifest's dependency tables (normal, dev,
/// build, and target-specific, in either inline or `[...dependencies.x]`
/// form).
pub fn manifest_dependencies(manifest: &str) -> Vec<String> {
    let is_table = |name: &str| {
        ["dependencies", "dev-dependencies", "build-dependencies"]
            .iter()
            .any(|table| name == *table || name.ends_with(&format!(".{table}")))
    };
    let mut names = Vec::new();
    let mut in_table = false;
    for line in manifest.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(header) = line.strip_prefix('[').and_then(|h| h.strip_suffix(']')) {
            let header = header.trim();
            in_table = is_table(header);
            // `[dependencies.name]` declares one dependency as a table.
            if !in_table
                && let Some((table, name)) = header.rsplit_once('.')
                && is_table(table)
            {
                names.push(name.trim_matches('"').to_owned());
            }
            continue;
        }
        if in_table && let Some((name, _)) = line.split_once('=') {
            names.push(name.trim().trim_matches('"').to_owned());
        }
    }
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(source: &str) -> Vec<String> {
        exported_names(source)
            .expect("source parses")
            .into_iter()
            .collect()
    }

    #[test]
    fn reads_use_lists_single_uses_and_items() {
        let source = r#"
            //! Docs with pub struct Hidden; inside.
            use std::collections::HashMap;
            mod widgets;
            pub use widgets::{
                Alpha, Beta,
                gamma_fn,
            };
            pub use other::Delta;
            pub(crate) struct Internal;
            #[derive(Clone)]
            pub struct Epsilon(pub u64);
            pub enum Zeta { A, B }
            pub const ETA: u32 = 1;
            pub const fn theta() {}
            pub trait Iota {}
            pub type Kappa = u8;
            pub static LAMBDA: u8 = 0;
            impl Epsilon {
                pub fn hidden(&self) -> &str { "pub struct NotAnItem {" }
            }
            fn private() { let c = '{'; }
            mod nested { pub struct NotExported; }
        "#;
        assert_eq!(
            names(source),
            [
                "Alpha", "Beta", "Delta", "ETA", "Epsilon", "Iota", "Kappa", "LAMBDA", "Zeta",
                "gamma_fn", "theta"
            ]
        );
    }

    #[test]
    fn rejects_constructs_the_parser_does_not_understand() {
        for source in [
            "pub mod x;",
            "pub use a::*;",
            "pub use a::{B, c::*};",
            "pub use a::B as C;",
            "pub use a::{B, c::{D}};",
            "make_controls! { Button }",
            "macro_rules! m { () => {} }",
            "pub extern crate foo;",
        ] {
            let error = exported_names(source).expect_err(source);
            assert_eq!(error.0, 1, "{source}: {error:?}");
        }
        let error = exported_names("pub struct A;\n\npub mod later;").unwrap_err();
        assert_eq!(error.0, 3, "the message names the line");
        assert!(error.1.contains("pub mod later"), "{}", error.1);
    }

    #[test]
    fn comparison_reports_unclassified_stale_and_duplicates() {
        let exports: BTreeSet<String> = ["Shown", "Exempt", "New"]
            .into_iter()
            .map(str::to_owned)
            .collect();
        let mismatch = compare(&exports, &["Shown", "Gone"], &["Exempt", "Shown"]);
        assert_eq!(mismatch.unclassified, ["New"]);
        assert_eq!(mismatch.stale, ["Gone"]);
        assert_eq!(mismatch.duplicated, ["Shown"]);
        let text = mismatch.to_string();
        assert!(
            text.contains("unclassified") && text.contains("New"),
            "{text}"
        );
        assert!(text.contains("stale") && text.contains("Gone"), "{text}");
        assert!(compare(&exports, &["Shown", "New"], &["Exempt"]).is_empty());
    }

    #[test]
    fn every_controls_export_is_shown_or_exempt() {
        let source = std::fs::read_to_string(CONTROLS_ROOT)
            .unwrap_or_else(|error| panic!("cannot read {CONTROLS_ROOT}: {error}"));
        let exports = exported_names(&source).unwrap_or_else(|(line, message)| {
            panic!(
                "choscorui-controls/src/lib.rs:{line}: {message}; update gallery/src/coverage.rs"
            )
        });
        assert!(
            exports.len() > 100,
            "parser found only {} names",
            exports.len()
        );
        let shown: Vec<&str> = SHOWN.iter().map(|(name, _)| *name).collect();
        let exempt: Vec<&str> = EXEMPT.iter().map(|(name, _)| *name).collect();
        let mismatch = compare(&exports, &shown, &exempt);
        assert!(mismatch.is_empty(), "\n{mismatch}");
    }

    #[test]
    fn catalog_entries_are_well_formed() {
        let mut ids = BTreeSet::new();
        for page in PAGES {
            assert!(
                !page.id.is_empty()
                    && page
                        .id
                        .bytes()
                        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'),
                "page id `{}` must be lowercase [a-z0-9-]+",
                page.id
            );
            assert!(ids.insert(page.id), "duplicate page id `{}`", page.id);
            assert!(!page.title.is_empty(), "page `{}` has no title", page.id);
        }
        for (name, page) in SHOWN {
            assert!(
                ids.contains(page),
                "SHOWN `{name}` names unknown page `{page}`"
            );
        }
        for (name, reason) in EXEMPT {
            assert!(!reason.trim().is_empty(), "EXEMPT `{name}` needs a reason");
        }
    }

    #[test]
    fn manifest_reader_finds_dependencies_in_every_table_form() {
        let manifest = "[package]\nname = \"x\"\n\n[dependencies]\nchoscorui = { path = \"..\" }\n# note = 1\n\n[dev-dependencies]\nserde = \"1\"\n\n[target.'cfg(unix)'.dependencies]\nlibc = \"0.2\"\n\n[build-dependencies.cc]\nversion = \"1\"\n\n[features]\nfast = []\n";
        assert_eq!(
            manifest_dependencies(manifest),
            ["choscorui", "serde", "libc", "cc"]
        );
    }

    #[test]
    fn the_gallery_depends_on_the_facade_alone() {
        let manifest = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"))
            .expect("gallery manifest is readable");
        assert_eq!(manifest_dependencies(&manifest), ["choscorui"]);
    }
}
