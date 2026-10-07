//! Rich text documents: blocks (paragraphs, headings, list items, quotes,
//! code) holding runs of formatted text, with range editing, format
//! toggling, and conversion to and from plain text, a Markdown subset, and
//! HTML.
//!
//! Positions are `(block, byte offset in the block's text)`. Documents are
//! cheap to clone, so undo keeps snapshots in an `UndoStack<RichDocument>`.

use crate::Color;
use std::ops::Range;

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct SpanFormat {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strikethrough: bool,
    /// Monospaced inline code.
    pub code: bool,
    pub color: Option<Color>,
    pub link: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FormatFlag {
    Bold,
    Italic,
    Underline,
    Strikethrough,
    Code,
}

impl SpanFormat {
    pub fn get(&self, flag: FormatFlag) -> bool {
        match flag {
            FormatFlag::Bold => self.bold,
            FormatFlag::Italic => self.italic,
            FormatFlag::Underline => self.underline,
            FormatFlag::Strikethrough => self.strikethrough,
            FormatFlag::Code => self.code,
        }
    }
    pub fn set(&mut self, flag: FormatFlag, value: bool) {
        match flag {
            FormatFlag::Bold => self.bold = value,
            FormatFlag::Italic => self.italic = value,
            FormatFlag::Underline => self.underline = value,
            FormatFlag::Strikethrough => self.strikethrough = value,
            FormatFlag::Code => self.code = value,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BlockKind {
    Paragraph,
    /// Level 1–6.
    Heading(u8),
    Bullet,
    Numbered,
    Quote,
    Code,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum BlockAlignment {
    #[default]
    Start,
    Center,
    End,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RichRun {
    pub text: String,
    pub format: SpanFormat,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RichBlock {
    pub kind: BlockKind,
    pub alignment: BlockAlignment,
    /// Never contains `\n`; adjacent runs have different formats.
    runs: Vec<RichRun>,
}

impl RichBlock {
    pub fn new(kind: BlockKind) -> Self {
        Self {
            kind,
            alignment: BlockAlignment::Start,
            runs: Vec::new(),
        }
    }
    pub fn runs(&self) -> &[RichRun] {
        &self.runs
    }
    pub fn text(&self) -> String {
        self.runs.iter().map(|run| run.text.as_str()).collect()
    }
    pub fn len(&self) -> usize {
        self.runs.iter().map(|run| run.text.len()).sum()
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    /// Runs with their byte ranges in the block text.
    pub fn spans(&self) -> impl Iterator<Item = (Range<usize>, &RichRun)> {
        let mut start = 0;
        self.runs.iter().map(move |run| {
            let range = start..start + run.text.len();
            start = range.end;
            (range, run)
        })
    }
    fn push(&mut self, text: &str, format: &SpanFormat) {
        if text.is_empty() {
            return;
        }
        match self.runs.last_mut() {
            Some(last) if last.format == *format => last.text.push_str(text),
            _ => self.runs.push(RichRun {
                text: text.to_owned(),
                format: format.clone(),
            }),
        }
    }
    fn normalize(&mut self) {
        let runs = std::mem::take(&mut self.runs);
        for run in runs {
            self.push(&run.text, &run.format);
        }
    }
    /// Splits at `offset`, returning the tail runs.
    fn split_off(&mut self, offset: usize) -> Vec<RichRun> {
        let mut tail = Vec::new();
        let mut head = Vec::new();
        for (range, run) in self.spans() {
            if range.end <= offset {
                head.push(run.clone());
            } else if range.start >= offset {
                tail.push(run.clone());
            } else {
                let cut = offset - range.start;
                head.push(RichRun {
                    text: run.text[..cut].to_owned(),
                    format: run.format.clone(),
                });
                tail.push(RichRun {
                    text: run.text[cut..].to_owned(),
                    format: run.format.clone(),
                });
            }
        }
        self.runs = head;
        tail
    }
    fn format_at(&self, offset: usize) -> SpanFormat {
        // The run before the offset wins, so typing continues its format.
        let mut found = None;
        for (range, run) in self.spans() {
            if range.start < offset || (offset == 0 && range.start == 0) {
                found = Some(&run.format);
            }
        }
        found.cloned().unwrap_or_default()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RichPosition {
    pub block: usize,
    /// Byte offset within the block's text, on a char boundary.
    pub offset: usize,
}

impl RichPosition {
    pub const fn new(block: usize, offset: usize) -> Self {
        Self { block, offset }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RichTextError {
    InvalidPosition(RichPosition),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RichDocument {
    blocks: Vec<RichBlock>,
}

impl Default for RichDocument {
    fn default() -> Self {
        Self::new()
    }
}

impl RichDocument {
    /// One empty paragraph.
    pub fn new() -> Self {
        Self {
            blocks: vec![RichBlock::new(BlockKind::Paragraph)],
        }
    }

    pub fn from_plain_text(text: &str) -> Self {
        let mut document = Self::new();
        document.blocks.clear();
        for line in text.split('\n') {
            let mut block = RichBlock::new(BlockKind::Paragraph);
            block.push(line.trim_end_matches('\r'), &SpanFormat::default());
            document.blocks.push(block);
        }
        document
    }

    pub fn blocks(&self) -> &[RichBlock] {
        &self.blocks
    }
    pub fn set_block_kind(&mut self, block: usize, kind: BlockKind) {
        if let Some(target) = self.blocks.get_mut(block) {
            target.kind = match kind {
                BlockKind::Heading(level) => BlockKind::Heading(level.clamp(1, 6)),
                kind => kind,
            };
        }
    }
    pub fn set_alignment(&mut self, block: usize, alignment: BlockAlignment) {
        if let Some(target) = self.blocks.get_mut(block) {
            target.alignment = alignment;
        }
    }
    pub fn end(&self) -> RichPosition {
        let last = self.blocks.len() - 1;
        RichPosition::new(last, self.blocks[last].len())
    }

    /// Text with blocks joined by `\n`.
    pub fn plain_text(&self) -> String {
        self.blocks
            .iter()
            .map(RichBlock::text)
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Converts a plain-text byte offset (as from `plain_text`) to a
    /// position, and back.
    pub fn position_from_plain(&self, mut offset: usize) -> RichPosition {
        for (index, block) in self.blocks.iter().enumerate() {
            if offset <= block.len() {
                return RichPosition::new(index, offset);
            }
            offset -= block.len() + 1;
        }
        self.end()
    }
    pub fn plain_offset(&self, position: RichPosition) -> usize {
        self.blocks[..position.block.min(self.blocks.len())]
            .iter()
            .map(|block| block.len() + 1)
            .sum::<usize>()
            + position.offset
    }

    fn check(&self, position: RichPosition) -> Result<(), RichTextError> {
        let valid = self.blocks.get(position.block).is_some_and(|block| {
            position.offset <= block.len() && block.text().is_char_boundary(position.offset)
        });
        if valid {
            Ok(())
        } else {
            Err(RichTextError::InvalidPosition(position))
        }
    }

    /// The format new text typed at `position` takes.
    pub fn format_at(&self, position: RichPosition) -> SpanFormat {
        self.blocks
            .get(position.block)
            .map(|block| block.format_at(position.offset))
            .unwrap_or_default()
    }

    /// Inserts text; `\n` splits blocks (a new block after a heading is a
    /// paragraph; list items continue the list). Returns the position
    /// after the inserted text.
    pub fn insert(
        &mut self,
        position: RichPosition,
        text: &str,
        format: Option<&SpanFormat>,
    ) -> Result<RichPosition, RichTextError> {
        self.check(position)?;
        let format = format.cloned().unwrap_or_else(|| self.format_at(position));
        let mut block_index = position.block;
        let mut offset = position.offset;
        for (index, line) in text.split('\n').enumerate() {
            if index > 0 {
                let block = &mut self.blocks[block_index];
                let tail = block.split_off(offset);
                let kind = match block.kind {
                    BlockKind::Heading(_) => BlockKind::Paragraph,
                    kind => kind,
                };
                let mut next = RichBlock::new(kind);
                next.alignment = block.alignment;
                next.runs = tail;
                next.normalize();
                block_index += 1;
                self.blocks.insert(block_index, next);
                offset = 0;
            }
            let line = line.trim_end_matches('\r');
            let block = &mut self.blocks[block_index];
            let tail = block.split_off(offset);
            block.push(line, &format);
            for run in tail {
                block.push(&run.text, &run.format);
            }
            offset += line.len();
        }
        Ok(RichPosition::new(block_index, offset))
    }

    /// Deletes `range`, merging the first and last blocks.
    pub fn delete(&mut self, range: Range<RichPosition>) -> Result<(), RichTextError> {
        self.check(range.start)?;
        self.check(range.end)?;
        let (start, end) = if range.start <= range.end {
            (range.start, range.end)
        } else {
            (range.end, range.start)
        };
        let tail = self.blocks[end.block].split_off(end.offset);
        let first = &mut self.blocks[start.block];
        first.split_off(start.offset);
        for run in tail {
            first.push(&run.text, &run.format);
        }
        self.blocks.drain(start.block + 1..=end.block);
        Ok(())
    }

    /// Applies `edit` to the formats in `range`.
    pub fn format_range(
        &mut self,
        range: Range<RichPosition>,
        edit: impl Fn(&mut SpanFormat),
    ) -> Result<(), RichTextError> {
        self.check(range.start)?;
        self.check(range.end)?;
        let (start, end) = (range.start.min(range.end), range.start.max(range.end));
        for index in start.block..=end.block {
            let block = &mut self.blocks[index];
            let from = if index == start.block {
                start.offset
            } else {
                0
            };
            let to = if index == end.block {
                end.offset
            } else {
                block.len()
            };
            let tail = block.split_off(to);
            let mut middle = block.split_off(from);
            for run in &mut middle {
                edit(&mut run.format);
            }
            for run in middle.into_iter().chain(tail) {
                block.push(&run.text, &run.format);
            }
        }
        Ok(())
    }

    /// Whether every character in `range` has `flag`.
    pub fn has_flag(&self, range: Range<RichPosition>, flag: FormatFlag) -> bool {
        let (start, end) = (range.start.min(range.end), range.start.max(range.end));
        let mut any = false;
        for index in start.block..=end.block.min(self.blocks.len() - 1) {
            let block = &self.blocks[index];
            let from = if index == start.block {
                start.offset
            } else {
                0
            };
            let to = if index == end.block {
                end.offset
            } else {
                block.len()
            };
            for (span, run) in block.spans() {
                if span.end > from && span.start < to {
                    any = true;
                    if !run.format.get(flag) {
                        return false;
                    }
                }
            }
        }
        any
    }

    /// ⌘B-style toggle: sets `flag` unless the whole range already has it.
    pub fn toggle(
        &mut self,
        range: Range<RichPosition>,
        flag: FormatFlag,
    ) -> Result<bool, RichTextError> {
        let value = !self.has_flag(range.clone(), flag);
        self.format_range(range, |format| format.set(flag, value))?;
        Ok(value)
    }

    /// Links in document order with their ranges.
    pub fn links(&self) -> Vec<(Range<RichPosition>, String)> {
        let mut links: Vec<(Range<RichPosition>, String)> = Vec::new();
        for (index, block) in self.blocks.iter().enumerate() {
            for (span, run) in block.spans() {
                if let Some(url) = &run.format.link {
                    let start = RichPosition::new(index, span.start);
                    let end = RichPosition::new(index, span.end);
                    // Adjacent runs (for example a partly bold link) merge.
                    match links.last_mut() {
                        Some((range, last)) if *last == *url && range.end == start => {
                            range.end = end
                        }
                        _ => links.push((start..end, url.clone())),
                    }
                }
            }
        }
        links
    }

    /// Parses a Markdown subset: `#`–`######` headings, `-`/`*` and `1.`
    /// list items, `>` quotes, fenced code blocks, `**bold**`, `*italic*`
    /// or `_italic_`, `~~strike~~`, `` `code` ``, `[text](url)`, and `\`
    /// escapes. Anything else is literal text.
    pub fn from_markdown(source: &str) -> Self {
        let mut blocks = Vec::new();
        let mut in_fence = false;
        for raw in source.split('\n') {
            let line = raw.trim_end_matches('\r');
            if line.trim_start().starts_with("```") {
                in_fence = !in_fence;
                continue;
            }
            if in_fence {
                let mut block = RichBlock::new(BlockKind::Code);
                block.push(line, &SpanFormat::default());
                blocks.push(block);
                continue;
            }
            let trimmed = line.trim_start();
            let hashes = trimmed.bytes().take_while(|b| *b == b'#').count();
            let (kind, body) = if (1..=6).contains(&hashes) && trimmed[hashes..].starts_with(' ') {
                (BlockKind::Heading(hashes as u8), &trimmed[hashes + 1..])
            } else if let Some(rest) = trimmed
                .strip_prefix("- ")
                .or_else(|| trimmed.strip_prefix("* "))
            {
                (BlockKind::Bullet, rest)
            } else if let Some(rest) = trimmed.strip_prefix("> ") {
                (BlockKind::Quote, rest)
            } else if let Some(dot) = trimmed.find(". ")
                && dot > 0
                && trimmed[..dot].bytes().all(|b| b.is_ascii_digit())
            {
                (BlockKind::Numbered, &trimmed[dot + 2..])
            } else {
                (BlockKind::Paragraph, line)
            };
            let mut block = RichBlock::new(kind);
            parse_inline(body, &mut block);
            blocks.push(block);
        }
        if blocks.is_empty() {
            blocks.push(RichBlock::new(BlockKind::Paragraph));
        }
        Self { blocks }
    }

    /// Serializes to the same Markdown subset; `from_markdown` round-trips
    /// it.
    pub fn to_markdown(&self) -> String {
        let mut out = Vec::new();
        let mut number = 0;
        let mut index = 0;
        while index < self.blocks.len() {
            let block = &self.blocks[index];
            if block.kind == BlockKind::Code {
                out.push("```".to_owned());
                while index < self.blocks.len() && self.blocks[index].kind == BlockKind::Code {
                    out.push(self.blocks[index].text());
                    index += 1;
                }
                out.push("```".to_owned());
                continue;
            }
            number = if block.kind == BlockKind::Numbered {
                number + 1
            } else {
                0
            };
            let prefix = match block.kind {
                BlockKind::Paragraph | BlockKind::Code => String::new(),
                BlockKind::Heading(level) => format!("{} ", "#".repeat(level as usize)),
                BlockKind::Bullet => "- ".into(),
                BlockKind::Numbered => format!("{number}. "),
                BlockKind::Quote => "> ".into(),
            };
            let mut line = prefix;
            line.push_str(&inline_markdown(&block.runs));
            // A paragraph that looks like markup is escaped.
            if block.kind == BlockKind::Paragraph && looks_like_block_markup(&line) {
                line.insert(0, '\\');
            }
            out.push(line);
            index += 1;
        }
        out.join("\n")
    }

    /// HTML fragment with escaped text, semantic tags, and grouped lists.
    pub fn to_html(&self) -> String {
        let mut out = String::new();
        let mut open_list: Option<&str> = None;
        for block in &self.blocks {
            let list = match block.kind {
                BlockKind::Bullet => Some("ul"),
                BlockKind::Numbered => Some("ol"),
                _ => None,
            };
            if open_list != list {
                if let Some(tag) = open_list {
                    out.push_str(&format!("</{tag}>"));
                }
                if let Some(tag) = list {
                    out.push_str(&format!("<{tag}>"));
                }
                open_list = list;
            }
            let tag = match block.kind {
                BlockKind::Paragraph => "p".to_owned(),
                BlockKind::Heading(level) => format!("h{level}"),
                BlockKind::Bullet | BlockKind::Numbered => "li".into(),
                BlockKind::Quote => "blockquote".into(),
                BlockKind::Code => "pre".into(),
            };
            let style = match block.alignment {
                BlockAlignment::Start => "",
                BlockAlignment::Center => " style=\"text-align:center\"",
                BlockAlignment::End => " style=\"text-align:end\"",
            };
            out.push_str(&format!("<{tag}{style}>"));
            for run in &block.runs {
                out.push_str(&inline_html(run));
            }
            out.push_str(&format!("</{tag}>"));
        }
        if let Some(tag) = open_list {
            out.push_str(&format!("</{tag}>"));
        }
        out
    }
}

fn looks_like_block_markup(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with('#')
        || trimmed.starts_with("- ")
        || trimmed.starts_with("* ")
        || trimmed.starts_with("> ")
        || trimmed.starts_with("```")
        || trimmed
            .find(". ")
            .is_some_and(|dot| dot > 0 && trimmed[..dot].bytes().all(|b| b.is_ascii_digit()))
}

fn escape_markdown(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        if matches!(ch, '\\' | '*' | '_' | '`' | '[' | ']' | '~') {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

/// Bold, italic, and strikethrough: the formats Markdown toggles inline.
#[derive(Clone, Copy, Default, PartialEq)]
struct Toggles {
    bold: bool,
    italic: bool,
    strikethrough: bool,
}

impl Toggles {
    fn of(format: &SpanFormat) -> Self {
        Self {
            bold: format.bold,
            italic: format.italic,
            strikethrough: format.strikethrough,
        }
    }
    fn and(self, other: Self) -> Self {
        Self {
            bold: self.bold && other.bold,
            italic: self.italic && other.italic,
            strikethrough: self.strikethrough && other.strikethrough,
        }
    }
    fn without(self, other: Self) -> Self {
        Self {
            bold: self.bold && !other.bold,
            italic: self.italic && !other.italic,
            strikethrough: self.strikethrough && !other.strikethrough,
        }
    }
}

/// Emits the markers that switch `state` to `target`. The parser treats
/// `**`, `_`, and `~~` as independent toggles, so any adjacent combination
/// reads back unambiguously (`_` avoids `*`/`**` runs such as `***`).
fn transition(out: &mut String, state: &mut Toggles, target: Toggles) {
    if state.bold != target.bold {
        out.push_str("**");
    }
    if state.italic != target.italic {
        out.push('_');
    }
    if state.strikethrough != target.strikethrough {
        out.push_str("~~");
    }
    *state = target;
}

fn push_run_text(out: &mut String, run: &RichRun) {
    // Code spans cannot contain escapes; fall back to escaping when the
    // text has a backtick.
    if run.format.code && !run.text.contains('`') {
        out.push('`');
        out.push_str(&run.text);
        out.push('`');
    } else {
        out.push_str(&escape_markdown(&run.text));
    }
}

/// Serializes a block's runs as format transitions, mirroring the
/// toggle-based parser so that parsing the output reproduces the runs.
fn inline_markdown(runs: &[RichRun]) -> String {
    let mut out = String::new();
    let mut state = Toggles::default();
    let mut index = 0;
    while index < runs.len() {
        let Some(link) = &runs[index].format.link else {
            transition(&mut out, &mut state, Toggles::of(&runs[index].format));
            push_run_text(&mut out, &runs[index]);
            index += 1;
            continue;
        };
        let end = runs[index..]
            .iter()
            .position(|run| run.format.link.as_ref() != Some(link))
            .map_or(runs.len(), |offset| index + offset);
        let group = &runs[index..end];
        // The parser applies the surrounding formats to the whole label, so
        // only formats shared by every run in the group stay outside.
        let all = Toggles {
            bold: true,
            italic: true,
            strikethrough: true,
        };
        let common = group
            .iter()
            .fold(all, |acc, run| acc.and(Toggles::of(&run.format)));
        transition(&mut out, &mut state, common);
        out.push('[');
        let mut inner = Toggles::default();
        for run in group {
            transition(
                &mut out,
                &mut inner,
                Toggles::of(&run.format).without(common),
            );
            push_run_text(&mut out, run);
        }
        transition(&mut out, &mut inner, Toggles::default());
        out.push_str("](");
        out.push_str(&link.replace(')', "%29").replace(' ', "%20"));
        out.push(')');
        index = end;
    }
    transition(&mut out, &mut state, Toggles::default());
    out
}

fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn inline_html(run: &RichRun) -> String {
    let format = &run.format;
    let mut text = escape_html(&run.text);
    let wrap = |text: String, tag: &str| format!("<{tag}>{text}</{tag}>");
    if format.code {
        text = wrap(text, "code");
    }
    if format.strikethrough {
        text = wrap(text, "s");
    }
    if format.underline {
        text = wrap(text, "u");
    }
    if format.italic {
        text = wrap(text, "em");
    }
    if format.bold {
        text = wrap(text, "strong");
    }
    if let Some(color) = format.color {
        text = format!(
            "<span style=\"color:#{:02x}{:02x}{:02x}\">{text}</span>",
            color.red, color.green, color.blue
        );
    }
    if let Some(link) = &format.link {
        text = format!("<a href=\"{}\">{text}</a>", escape_html(link));
    }
    text
}

/// Inline Markdown parser with a format stack.
fn parse_inline(source: &str, block: &mut RichBlock) {
    let mut format = SpanFormat::default();
    let mut text = String::new();
    let chars: Vec<char> = source.chars().collect();
    let mut i = 0;
    let flush = |text: &mut String, block: &mut RichBlock, format: &SpanFormat| {
        block.push(text, format);
        text.clear();
    };
    while i < chars.len() {
        let rest = &chars[i..];
        let starts = |pattern: &str| {
            let pattern: Vec<char> = pattern.chars().collect();
            rest.len() >= pattern.len() && rest[..pattern.len()] == pattern[..]
        };
        if rest[0] == '\\' && rest.len() > 1 {
            text.push(rest[1]);
            i += 2;
        } else if starts("**") {
            flush(&mut text, block, &format);
            format.bold = !format.bold;
            i += 2;
        } else if starts("~~") {
            flush(&mut text, block, &format);
            format.strikethrough = !format.strikethrough;
            i += 2;
        } else if rest[0] == '*' || rest[0] == '_' {
            flush(&mut text, block, &format);
            format.italic = !format.italic;
            i += 1;
        } else if rest[0] == '`'
            && let Some(close) = rest[1..].iter().position(|c| *c == '`')
        {
            flush(&mut text, block, &format);
            let code: String = rest[1..1 + close].iter().collect();
            let mut code_format = format.clone();
            code_format.code = true;
            block.push(&code, &code_format);
            i += close + 2;
        } else if rest[0] == '['
            && let Some(close) = find_link_end(rest)
        {
            let (label_end, url_end) = close;
            flush(&mut text, block, &format);
            let label: String = rest[1..label_end].iter().collect();
            let url: String = rest[label_end + 2..url_end].iter().collect();
            let mut inner = RichBlock::new(BlockKind::Paragraph);
            parse_inline(&label, &mut inner);
            for mut run in inner.runs {
                run.format.bold |= format.bold;
                run.format.italic |= format.italic;
                run.format.strikethrough |= format.strikethrough;
                run.format.link = Some(url.replace("%20", " ").replace("%29", ")"));
                block.push(&run.text, &run.format);
            }
            i += url_end + 1;
        } else {
            text.push(rest[0]);
            i += 1;
        }
    }
    flush(&mut text, block, &format);
}

/// For `[label](url)` at the start of `chars`, returns the index of `]`
/// and of `)`.
fn find_link_end(chars: &[char]) -> Option<(usize, usize)> {
    let mut depth = 0;
    let mut i = 1;
    while i < chars.len() {
        match chars[i] {
            '\\' => i += 1,
            // Brackets inside a code span are literal, as in `parse_inline`.
            '`' => {
                if let Some(close) = chars[i + 1..].iter().position(|c| *c == '`') {
                    i += close + 1;
                }
            }
            '[' => depth += 1,
            ']' if depth > 0 => depth -= 1,
            ']' => {
                if chars.get(i + 1) != Some(&'(') {
                    return None;
                }
                let close = chars[i + 2..].iter().position(|c| *c == ')')?;
                return Some((i, i + 2 + close));
            }
            _ => {}
        }
        i += 1;
    }
    None
}
