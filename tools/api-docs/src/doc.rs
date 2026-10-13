//! Doc comment Markdown as the generated pages show it: rustdoc code
//! block attributes become `rust`, hidden lines go away, headings move
//! below the item heading, and intra-doc links point to generated
//! anchors or become plain text.

use std::collections::BTreeMap;

/// What an intra-doc link key resolves to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Link {
    /// A link target in the generated pages.
    To(String),
    /// A rustdoc link to an item that the pages do not show.
    Plain,
    /// Not a rustdoc link.
    Unknown,
}

/// The fence of an open code block and whether it holds Rust.
struct Fence {
    marker: String,
    rust: bool,
}

/// Returns the fence run (```` ``` ```` or `~~~`) and the info string of a
/// line that opens or closes a code block.
fn fence_of(line: &str) -> Option<(String, &str)> {
    let trimmed = line.trim_start_matches(' ');
    if line.len() - trimmed.len() > 3 {
        return None;
    }
    let ch = trimmed.chars().next().filter(|c| *c == '`' || *c == '~')?;
    let run = trimmed.chars().take_while(|c| *c == ch).count();
    (run >= 3).then(|| (trimmed[..run].to_owned(), trimmed[run..].trim()))
}

/// Whether a rustdoc code block info string means Rust code.
pub fn is_rust_info(info: &str) -> bool {
    info.split([',', ' ', '\t'])
        .filter(|token| !token.is_empty())
        .all(|token| {
            matches!(
                token,
                "rust"
                    | "no_run"
                    | "ignore"
                    | "should_panic"
                    | "compile_fail"
                    | "test_harness"
                    | "standalone_crate"
            ) || token.starts_with("edition")
                || token.starts_with("ignore-")
                || (token.starts_with('E') && token[1..].chars().all(|c| c.is_ascii_digit()))
        })
}

/// Splits a doc comment into lines, removing the hidden lines of Rust
/// code blocks and rewriting each opening fence of Rust code to `rust`.
/// The callback rewrites each line of prose.
fn transform(doc: &str, mut prose: impl FnMut(&str) -> Option<String>) -> Vec<String> {
    let mut out = Vec::new();
    let mut fence: Option<Fence> = None;
    for line in doc.lines() {
        if let Some(open) = &fence {
            if let Some((marker, info)) = fence_of(line)
                && info.is_empty()
                && marker.starts_with(&open.marker[..1])
                && marker.len() >= open.marker.len()
            {
                out.push(line.to_owned());
                fence = None;
                continue;
            }
            if open.rust {
                let trimmed = line.trim_start();
                if trimmed == "#" || trimmed.starts_with("# ") {
                    continue;
                }
                if trimmed.starts_with("##") {
                    let indent = line.len() - trimmed.len();
                    out.push(format!("{}{}", &line[..indent], &trimmed[1..]));
                    continue;
                }
            }
            out.push(line.to_owned());
            continue;
        }
        if let Some((marker, info)) = fence_of(line) {
            let rust = is_rust_info(info);
            let indent = &line[..line.len() - line.trim_start_matches(' ').len()];
            out.push(if rust {
                format!("{indent}{marker}rust")
            } else {
                line.to_owned()
            });
            fence = Some(Fence { marker, rust });
            continue;
        }
        if let Some(text) = prose(line) {
            out.push(text);
        }
    }
    out
}

/// Renders a doc comment. `shift` levels go before each heading, and
/// `link` resolves the intra-doc link keys.
pub fn render(doc: &str, shift: usize, link: &dyn Fn(&str) -> Link) -> String {
    let definitions = reference_definitions(doc);
    let lines = transform(doc, |line| {
        if reference_definition(line).is_some() {
            return None;
        }
        let line = rewrite_links(line, &definitions, link);
        let hashes = line.chars().take_while(|c| *c == '#').count();
        if hashes > 0 && line[hashes..].starts_with(' ') {
            let level = (hashes + shift).min(6);
            return Some(format!("{}{}", "#".repeat(level), &line[hashes..]));
        }
        Some(line)
    });
    let mut text = lines.join("\n");
    while text.ends_with('\n') {
        text.pop();
    }
    text
}

/// The first sentence of the first paragraph, as plain text on one line.
pub fn summary(doc: &str) -> String {
    let definitions = reference_definitions(doc);
    let mut words = Vec::new();
    for line in doc.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || fence_of(line).is_some() || trimmed.starts_with('#') {
            if words.is_empty() && trimmed.is_empty() {
                continue;
            }
            break;
        }
        words.push(rewrite_links(trimmed, &definitions, &|_| Link::Plain));
    }
    let paragraph = words.join(" ");
    let bytes = paragraph.as_bytes();
    let mut in_code = false;
    for (index, byte) in bytes.iter().enumerate() {
        match byte {
            b'`' => in_code = !in_code,
            b'.' | b'?' | b'!'
                if !in_code && bytes.get(index + 1).is_none_or(|next| *next == b' ') =>
            {
                return paragraph[..=index].to_owned();
            }
            _ => {}
        }
    }
    paragraph
}

/// Whether the doc comment has an `Examples` heading with a Rust code
/// block in its section.
pub fn has_examples(doc: &str) -> bool {
    let mut section: Option<usize> = None;
    let mut fence = false;
    for line in doc.lines() {
        if let Some((_, info)) = fence_of(line) {
            if !fence && section.is_some() && is_rust_info(info) {
                return true;
            }
            fence = !fence;
            continue;
        }
        if fence {
            continue;
        }
        let hashes = line.chars().take_while(|c| *c == '#').count();
        if hashes > 0 && line[hashes..].starts_with(' ') {
            if line[hashes..].trim() == "Examples" {
                section = Some(hashes);
            } else if section.is_some_and(|level| hashes <= level) {
                section = None;
            }
        }
    }
    false
}

/// The `[label]: destination` line of a reference definition.
fn reference_definition(line: &str) -> Option<(String, String)> {
    let rest = line.trim_start().strip_prefix('[')?;
    let close = rest.find("]:")?;
    let label = &rest[..close];
    if label.is_empty() || label.starts_with('^') {
        return None;
    }
    let dest = rest[close + 2..].split_whitespace().next()?;
    Some((
        label.to_owned(),
        dest.trim_start_matches('<')
            .trim_end_matches('>')
            .to_owned(),
    ))
}

fn reference_definitions(doc: &str) -> BTreeMap<String, String> {
    doc.lines().filter_map(reference_definition).collect()
}

fn is_url(dest: &str) -> bool {
    let scheme: String = dest
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || "+.-".contains(*c))
        .collect();
    !scheme.is_empty() && dest[scheme.len()..].starts_with("://") || dest.starts_with("mailto:")
}

/// The index after the `]` that closes the `[` at `open`, skipping code
/// spans, or `None`.
fn closing_bracket(chars: &[char], open: usize) -> Option<usize> {
    let mut depth = 0;
    let mut i = open;
    while i < chars.len() {
        match chars[i] {
            '`' => {
                let run = chars[i..].iter().take_while(|c| **c == '`').count();
                let mut j = i + run;
                loop {
                    if j >= chars.len() {
                        return None;
                    }
                    if chars[j] == '`' {
                        let close = chars[j..].iter().take_while(|c| **c == '`').count();
                        if close == run {
                            i = j + close;
                            break;
                        }
                        j += close;
                    } else {
                        j += 1;
                    }
                }
                continue;
            }
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i + 1);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// Rewrites the links of one prose line.
fn rewrite_links(
    line: &str,
    definitions: &BTreeMap<String, String>,
    link: &dyn Fn(&str) -> Link,
) -> String {
    let chars: Vec<char> = line.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '`' {
            let run = chars[i..].iter().take_while(|c| **c == '`').count();
            let rest: String = chars[i + run..].iter().collect();
            let fence = "`".repeat(run);
            if let Some(end) = rest.find(&fence) {
                let span: String =
                    chars[i..i + run].iter().collect::<String>() + &rest[..end] + &fence;
                i += span.chars().count();
                out.push_str(&span);
                continue;
            }
            out.extend(&chars[i..i + run]);
            i += run;
            continue;
        }
        if c != '[' || (i > 0 && chars[i - 1] == '!') {
            out.push(c);
            i += 1;
            continue;
        }
        let Some(after) = closing_bracket(&chars, i) else {
            out.push(c);
            i += 1;
            continue;
        };
        let text: String = chars[i + 1..after - 1].iter().collect();
        let (dest, next, explicit) = match chars.get(after) {
            Some('(') => match chars[after..].iter().position(|c| *c == ')') {
                Some(close) => {
                    let dest: String = chars[after + 1..after + close].iter().collect();
                    (Some(dest.trim().to_owned()), after + close + 1, true)
                }
                None => (None, after, false),
            },
            Some('[') => match chars[after..].iter().position(|c| *c == ']') {
                Some(close) => {
                    let label: String = chars[after + 1..after + close].iter().collect();
                    let label = if label.is_empty() {
                        text.clone()
                    } else {
                        label
                    };
                    (
                        Some(definitions.get(&label).cloned().unwrap_or(label)),
                        after + close + 1,
                        true,
                    )
                }
                None => (None, after, false),
            },
            _ => (
                definitions
                    .get(&text)
                    .cloned()
                    .or_else(|| Some(text.clone())),
                after,
                false,
            ),
        };
        let dest = dest.unwrap_or_default();
        let resolved = if is_url(&dest) {
            Link::To(dest.clone())
        } else {
            link(&dest)
        };
        match resolved {
            Link::To(target) => out.push_str(&format!("[{text}]({target})")),
            Link::Plain => out.push_str(&text),
            Link::Unknown if explicit => out.push_str(&text),
            Link::Unknown => out.extend(&chars[i..after]),
        }
        i = next;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn links(key: &str) -> Link {
        match key {
            "`Styled`" | "Styled" => Link::To("modifiers.md#styled".into()),
            "Tree" => Link::Plain,
            _ => Link::Unknown,
        }
    }

    #[test]
    fn rewrites_code_blocks_and_headings() {
        let doc = "Text.\n\n# Examples\n\n```no_run\n# use x::*;\nlet a = 1;\n## not hidden\n```\n\n```text\n# kept\n```";
        let out = render(doc, 3, &links);
        assert_eq!(
            out,
            "Text.\n\n#### Examples\n\n```rust\nlet a = 1;\n# not hidden\n```\n\n```text\n# kept\n```"
        );
    }

    #[test]
    fn rewrites_links() {
        let doc = "See [`Styled`], [the tree](Tree), [site](https://example.com), \
                   [guide](../docs/guide.md), [x], `[`Styled`]`, and [ref][r].\n\n[r]: Styled";
        let out = render(doc, 0, &links);
        assert_eq!(
            out,
            "See [`Styled`](modifiers.md#styled), the tree, [site](https://example.com), \
             guide, [x], `[`Styled`]`, and [ref](modifiers.md#styled).\n"
                .trim_end()
        );
    }

    #[test]
    fn summary_is_the_first_sentence() {
        assert_eq!(summary("A view. More text."), "A view.");
        assert_eq!(
            summary("Runs `a.b()` then\nstops. Next."),
            "Runs `a.b()` then stops."
        );
        assert_eq!(summary("See [`Styled`] here\n\nNext"), "See `Styled` here");
        assert_eq!(summary(""), "");
    }

    #[test]
    fn finds_example_sections() {
        assert!(has_examples("Doc.\n\n# Examples\n\n```\nlet a = 1;\n```"));
        assert!(has_examples("# Examples\n\n```rust,no_run\nmain();\n```"));
        assert!(!has_examples("Doc.\n\n```\nlet a = 1;\n```"));
        assert!(!has_examples("# Examples\n\n```text\nout\n```"));
        assert!(!has_examples(
            "# Examples\n\nNone.\n\n# Errors\n\n```\nlet a = 1;\n```"
        ));
    }
}
