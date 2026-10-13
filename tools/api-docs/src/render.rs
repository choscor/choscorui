//! The Markdown pages: one or more files for each topic, `modifiers.md`
//! for the modifier trait, and the `README.md` index.

use std::collections::{BTreeMap, BTreeSet};

use crate::doc::{self, Link};
use crate::model::{ItemRef, World};
use crate::scope::{ENTRY_POINTS, Entry, PRELUDE, Scope};
use crate::sig;

/// A file over this many lines splits by item kind.
pub const LINE_LIMIT: usize = 2000;

/// The command that writes the pages.
pub const REGENERATE: &str = "cargo run -p choscorui-api-docs --locked";

/// The topic that holds the modifier trait and its methods.
pub const MODIFIERS: &str = "modifiers";

/// Marks a link target until the anchors are known.
const MARK: char = '\u{1}';

/// Lines of Markdown, with the heading lines that anchor an item.
#[derive(Default)]
struct Block {
    lines: Vec<String>,
    anchors: Vec<(usize, ItemRef)>,
    kind: String,
}

impl Block {
    fn push(&mut self, line: impl Into<String>) {
        self.lines.push(line.into());
    }

    fn heading(&mut self, line: String, item: ItemRef) {
        self.anchors.push((self.lines.len(), item));
        self.lines.push(line);
    }

    fn extend(&mut self, text: &str) {
        self.lines.extend(text.split('\n').map(str::to_owned));
    }

    fn append(&mut self, other: Block) {
        let offset = self.lines.len();
        self.anchors.extend(
            other
                .anchors
                .into_iter()
                .map(|(line, item)| (line + offset, item)),
        );
        self.lines.extend(other.lines);
    }
}

/// Renders every page, or lists each content rule that fails.
pub fn render(world: &World, scope: &Scope) -> Result<BTreeMap<String, String>, Vec<String>> {
    let anchored = anchored_items(world, scope);
    check_content(world, scope)?;
    let mut targets: Vec<ItemRef> = anchored.iter().copied().collect();
    targets.sort();
    let renderer = Renderer {
        world,
        scope,
        targets: &targets,
    };

    let mut files: Vec<(String, String, Block)> = Vec::new();
    for topic in &scope.topics {
        let blocks: Vec<Block> = scope
            .entries
            .iter()
            .filter(|entry| &entry.topic == topic)
            .map(|entry| renderer.entry(entry))
            .collect();
        if topic == MODIFIERS {
            files.extend(renderer.modifier_files(blocks));
        } else if !blocks.is_empty() {
            files.extend(split_by_kind(topic, blocks));
        }
    }
    if !scope.topics.iter().any(|t| t == MODIFIERS) && !scope.modifiers.is_empty() {
        files.extend(renderer.modifier_files(Vec::new()));
    }

    let mut anchors: BTreeMap<ItemRef, (String, String)> = BTreeMap::new();
    for (file, _, block) in &files {
        let slugs = heading_slugs(&block.lines);
        for (line, item) in &block.anchors {
            if let Some(slug) = slugs.get(line) {
                anchors
                    .entry(*item)
                    .or_insert_with(|| (file.clone(), slug.clone()));
            }
        }
    }
    let index = renderer.index(&files);
    let mut pages = BTreeMap::new();
    for (file, title, block) in files {
        let mut page = page_header(&title);
        page.append(block);
        pages.insert(file.clone(), finish(&page.lines, &file, &targets, &anchors));
    }
    pages.insert(
        "README.md".to_owned(),
        finish(&index.lines, "README.md", &targets, &anchors),
    );
    Ok(pages)
}

/// The items that get a heading: the entries, their members, and the
/// modifiers.
fn anchored_items(world: &World, scope: &Scope) -> BTreeSet<ItemRef> {
    let mut set = BTreeSet::new();
    for entry in &scope.entries {
        set.insert(entry.item);
        for members in members_of(world, scope, entry.item).values() {
            set.extend(members.iter().copied());
        }
    }
    set.extend(scope.modifiers.iter().map(|m| m.item));
    set
}

/// The self type of a generic alias in scope, such as
/// `Configured<Badge>`. The impl blocks for that type belong to the alias.
fn alias_self_types(world: &World, scope: &Scope) -> BTreeSet<String> {
    scope
        .entries
        .iter()
        .filter(|entry| world.inner(entry.item).0 == "type_alias")
        .map(|entry| sig::ty(world.inner(entry.item).1.get("type")))
        .filter(|text| text.contains('<'))
        .collect()
}

/// The members that an entry's section lists: the inherent items of a
/// type, without the impl blocks of its generic aliases; for a generic
/// alias, the items of its own impl blocks; the items of a trait other
/// than the modifier trait.
fn members_of(world: &World, scope: &Scope, item: ItemRef) -> BTreeMap<String, Vec<ItemRef>> {
    match world.inner(item).0 {
        "struct" | "enum" | "union" => {
            let claimed = alias_self_types(world, scope);
            world.inherent_members(item, &|self_ty| !claimed.contains(&sig::ty(self_ty)))
        }
        "type_alias" => {
            let own = sig::ty(world.inner(item).1.get("type"));
            match world.alias_target(item) {
                Some(target) if own.contains('<') => {
                    world.inherent_members(target, &|self_ty| sig::ty(self_ty) == own)
                }
                _ => BTreeMap::new(),
            }
        }
        "trait" if Some(item) != scope.modifier_trait => world.trait_members(item),
        _ => BTreeMap::new(),
    }
}

/// Whether an item is a type, or an alias of a type, that implements
/// the view trait.
fn is_view_type(world: &World, scope: &Scope, item: ItemRef) -> bool {
    let item = world.alias_target(item).unwrap_or(item);
    matches!(world.inner(item).0, "struct" | "enum" | "union")
        && scope
            .view_trait
            .is_some_and(|view| world.implements(item, view))
}

/// Checks the doc summary and example rules.
fn check_content(world: &World, scope: &Scope) -> Result<(), Vec<String>> {
    let mut missing_docs = Vec::new();
    let mut missing_examples = Vec::new();
    for entry in &scope.entries {
        let docs = world.docs(entry.item);
        if doc::summary(docs).is_empty() {
            missing_docs.push(entry.import.clone());
        }
        if is_view_type(world, scope, entry.item) && !doc::has_examples(docs) {
            missing_examples.push(format!("{} (view type)", entry.import));
        }
        let entry_point =
            ENTRY_POINTS.contains(&entry.name.as_str()) && entry.import.starts_with(PRELUDE);
        for (name, members) in members_of(world, scope, entry.item) {
            for member in members {
                let docs = world.docs(member);
                let path = format!("{}::{name}", entry.import);
                if doc::summary(docs).is_empty() {
                    missing_docs.push(path.clone());
                }
                if entry_point && world.inner(member).0 == "function" && !doc::has_examples(docs) {
                    missing_examples.push(format!("{path} (entry point method)"));
                }
            }
        }
    }
    for modifier in &scope.modifiers {
        let docs = world.docs(modifier.item);
        let path = format!(
            "{PRELUDE}::{}::{}",
            crate::scope::MODIFIER_TRAIT,
            modifier.name
        );
        if doc::summary(docs).is_empty() {
            missing_docs.push(path.clone());
        }
        if !doc::has_examples(docs) {
            missing_examples.push(format!("{path} (modifier)"));
        }
    }
    let mut errors: Vec<String> = Vec::new();
    missing_docs.sort();
    missing_docs.dedup();
    errors.extend(
        missing_docs
            .into_iter()
            .map(|path| format!("no doc summary: {path}")),
    );
    missing_examples.sort();
    missing_examples.dedup();
    errors.extend(
        missing_examples
            .into_iter()
            .map(|path| format!("no `# Examples` section with a Rust code block: {path}")),
    );
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

struct Renderer<'a> {
    world: &'a World,
    scope: &'a Scope,
    targets: &'a [ItemRef],
}

impl Renderer<'_> {
    /// A placeholder link target for an item with a heading.
    fn target(&self, item: ItemRef) -> Option<String> {
        self.targets
            .binary_search(&item)
            .ok()
            .map(|index| format!("{MARK}{index}{MARK}"))
    }

    /// The doc comment of an item with its links resolved.
    fn docs(&self, item: ItemRef, shift: usize) -> String {
        let links = self.world.item(item).get("links");
        let resolve = |key: &str| {
            let id = links.get(key);
            if id.is_null() {
                return Link::Unknown;
            }
            match self
                .world
                .resolve(item.krate, id)
                .and_then(|target| self.target(target))
            {
                Some(target) => Link::To(target),
                None => Link::Plain,
            }
        };
        doc::render(self.world.docs(item), shift, &resolve)
    }

    fn deprecation(&self, item: ItemRef, block: &mut Block) {
        let deprecation = self.world.item(item).get("deprecation");
        if deprecation.is_null() {
            return;
        }
        let note = deprecation
            .get("note")
            .as_str()
            .unwrap_or("no replacement given");
        block.push(format!("Deprecated: {note}"));
        block.push("");
    }

    fn code(block: &mut Block, text: &str) {
        block.push("```rust");
        block.extend(text);
        block.push("```");
        block.push("");
    }

    fn entry(&self, entry: &Entry) -> Block {
        let world = self.world;
        let (kind, inner) = world.inner(entry.item);
        let mut block = Block {
            kind: kind.to_owned(),
            ..Block::default()
        };
        block.heading(format!("## `{}`", entry.name), entry.item);
        block.push("");
        block.push(format!("{} in `{}`", kind_word(kind), entry.import));
        block.push("");
        Self::code(&mut block, &sig::declaration(world, entry.item));
        self.deprecation(entry.item, &mut block);
        let docs = self.docs(entry.item, 3);
        if !docs.is_empty() {
            block.extend(&docs);
            block.push("");
        }
        if is_view_type(world, self.scope, entry.item)
            && let Some(target) = self.scope.modifier_trait.and_then(|t| self.target(t))
        {
            block.push(format!(
                "Modifiers: every [`{}`]({target}) method applies to this view.",
                crate::scope::MODIFIER_TRAIT
            ));
            block.push("");
        }
        match kind {
            "enum" => self.variants(entry.item, inner, &mut block),
            "struct" => self.fields(entry.item, inner, &mut block),
            _ => {}
        }
        let members = members_of(world, self.scope, entry.item);
        if !members.is_empty() {
            block.push(if kind == "trait" {
                "### Items"
            } else {
                "### Methods"
            });
            block.push("");
            for (name, items) in members {
                let mut items: Vec<(String, ItemRef)> = items
                    .into_iter()
                    .map(|item| (sig::declaration(world, item), item))
                    .collect();
                items.sort();
                for (declaration, item) in items {
                    block.heading(format!("#### `{}::{name}`", entry.name), item);
                    block.push("");
                    Self::code(&mut block, &declaration);
                    self.deprecation(item, &mut block);
                    let docs = self.docs(item, 4);
                    if !docs.is_empty() {
                        block.extend(&docs);
                        block.push("");
                    }
                }
            }
        }
        if kind == "trait" {
            self.implementors(entry.item, &mut block);
        }
        block
    }

    fn variants(&self, item: ItemRef, inner: &crate::json::Json, block: &mut Block) {
        let variants: Vec<ItemRef> = inner
            .get("variants")
            .items()
            .iter()
            .filter_map(|id| self.world.resolve(item.krate, id))
            .collect();
        if variants.iter().all(|v| self.world.docs(*v).is_empty()) {
            return;
        }
        block.push("### Variants");
        block.push("");
        for variant in variants {
            self.bullet(
                &format!("`{}`", sig::variant_decl(self.world, variant)),
                variant,
                block,
            );
        }
        block.push("");
    }

    fn fields(&self, item: ItemRef, inner: &crate::json::Json, block: &mut Block) {
        let Some((_, kind)) = inner.get("kind").variant() else {
            return;
        };
        let fields: Vec<ItemRef> = kind
            .get("fields")
            .items()
            .iter()
            .filter_map(|id| self.world.resolve(item.krate, id))
            .collect();
        if fields.iter().all(|f| self.world.docs(*f).is_empty()) {
            return;
        }
        block.push("### Fields");
        block.push("");
        for field in fields {
            self.bullet(&format!("`{}`", self.world.name(field)), field, block);
        }
        block.push("");
    }

    /// A list item with the full doc comment of a variant or a field.
    fn bullet(&self, label: &str, item: ItemRef, block: &mut Block) {
        let docs = self.docs(item, 4);
        let mut lines = docs.split('\n');
        match lines.next().filter(|first| !first.is_empty()) {
            Some(first) => block.push(format!("- {label}: {first}")),
            None => block.push(format!("- {label}")),
        }
        for line in lines {
            block.push(if line.is_empty() {
                String::new()
            } else {
                format!("  {line}")
            });
        }
    }

    fn implementors(&self, item: ItemRef, block: &mut Block) {
        let candidates: Vec<ItemRef> = self.scope.entries.iter().map(|e| e.item).collect();
        let mut names: Vec<(String, String)> = self
            .world
            .implementors(item, &candidates)
            .into_iter()
            .filter_map(|imp| {
                let entry = self.scope.entry(imp)?;
                Some((entry.name.clone(), self.target(imp)?))
            })
            .collect();
        if names.is_empty() {
            return;
        }
        names.sort();
        block.push("### Implementors");
        block.push("");
        for (name, target) in names {
            block.push(format!("- [`{name}`]({target})"));
        }
        block.push("");
    }

    fn modifier(&self, modifier: &crate::scope::Modifier) -> Block {
        let mut block = Block {
            kind: "modifier".into(),
            ..Block::default()
        };
        block.heading(format!("### `{}`", modifier.name), modifier.item);
        block.push("");
        Self::code(&mut block, &sig::declaration(self.world, modifier.item));
        self.deprecation(modifier.item, &mut block);
        let docs = self.docs(modifier.item, 3);
        if !docs.is_empty() {
            block.extend(&docs);
            block.push("");
        }
        block
    }

    /// `modifiers.md` with the modifier trait's entries and the modifier
    /// groups; one file per group when it is over the limit.
    fn modifier_files(&self, entries: Vec<Block>) -> Vec<(String, String, Block)> {
        let mut groups: Vec<(String, Block)> = Vec::new();
        for topic in &self.scope.topics {
            let mut group = Block::default();
            for modifier in self.scope.modifiers.iter().filter(|m| &m.group == topic) {
                group.append(self.modifier(modifier));
            }
            if !group.lines.is_empty() {
                let mut block = Block::default();
                block.push(format!("## {} modifiers", title(topic)));
                block.push("");
                block.append(group);
                groups.push((topic.clone(), block));
            }
        }
        let mut main = Block::default();
        for block in entries {
            main.append(block);
        }
        let total = main.lines.len() + groups.iter().map(|g| g.1.lines.len()).sum::<usize>();
        if total <= LINE_LIMIT {
            for (_, block) in groups {
                main.append(block);
            }
            return vec![(format!("{MODIFIERS}.md"), title(MODIFIERS), main)];
        }
        let mut files = vec![(format!("{MODIFIERS}.md"), title(MODIFIERS), main)];
        for (group, block) in groups {
            files.push((
                format!("{MODIFIERS}-{group}.md"),
                format!("{} modifiers", title(&group)),
                block,
            ));
        }
        files
    }

    fn index(&self, files: &[(String, String, Block)]) -> Block {
        let mut block = Block::default();
        block.push(format!(
            "<!-- Generated by choscorui-api-docs. Do not edit. Regenerate: {REGENERATE} -->"
        ));
        block.push("# ChoscorUI app-author API");
        block.push("");
        block.push(format!(
            "> Generated from the source doc comments. Do not edit by hand. Regenerate with `{REGENERATE}`."
        ));
        block.push("");
        block.push("This reference covers the API that an application uses: the items of `choscorui::prelude` and the extension points for custom controls, settings, and capabilities.");
        block.push("Every example assumes `use choscorui::prelude::*;`.");
        block.push("");
        block.push("## Topics");
        block.push("");
        let mut sections = Vec::new();
        for topic in &self.scope.topics {
            let topic_files: Vec<&str> = files
                .iter()
                .filter(|(file, _, _)| file_topic(file) == topic.as_str())
                .map(|(file, _, _)| file.as_str())
                .collect();
            if topic_files.is_empty() {
                continue;
            }
            let links: Vec<String> = topic_files.iter().map(|f| format!("[{f}]({f})")).collect();
            block.push(format!("- {}: {}", title(topic), links.join(", ")));
            sections.push(topic);
        }
        block.push("");
        for topic in sections {
            block.push(format!("## {}", title(topic)));
            block.push("");
            for entry in self.scope.entries.iter().filter(|e| &e.topic == topic) {
                self.index_line(
                    &entry.name,
                    kind_word(self.world.inner(entry.item).0),
                    entry.item,
                    &mut block,
                );
            }
            if topic == MODIFIERS {
                for group in &self.scope.topics {
                    let modifiers: Vec<_> = self
                        .scope
                        .modifiers
                        .iter()
                        .filter(|m| &m.group == group)
                        .collect();
                    if modifiers.is_empty() {
                        continue;
                    }
                    block.push("");
                    block.push(format!("### {} modifiers", title(group)));
                    block.push("");
                    for modifier in modifiers {
                        self.index_line(&modifier.name, "modifier", modifier.item, &mut block);
                    }
                }
            }
            block.push("");
        }
        block
    }

    fn index_line(&self, name: &str, kind: &str, item: ItemRef, block: &mut Block) {
        let summary = doc::summary(self.world.docs(item));
        let target = self.target(item).unwrap_or_default();
        block.push(format!("- [`{name}`]({target}) {kind}: {summary}"));
    }
}

fn page_header(title: &str) -> Block {
    let mut block = Block::default();
    block.push(format!(
        "<!-- Generated by choscorui-api-docs. Do not edit. Regenerate: {REGENERATE} -->"
    ));
    block.push(format!("# {title}"));
    block.push("");
    block.push(format!(
        "> Generated from the source doc comments. Do not edit by hand. Regenerate with `{REGENERATE}`. \
         The [index](README.md) lists every item. Every example assumes `use choscorui::prelude::*;`."
    ));
    block.push("");
    block
}

/// The topic of a generated file name.
fn file_topic(file: &str) -> &str {
    let stem = file.trim_end_matches(".md");
    if stem.starts_with(&format!("{MODIFIERS}-")) {
        return MODIFIERS;
    }
    stem.split_once("--").map_or(stem, |(topic, _)| topic)
}

fn title(topic: &str) -> String {
    let text = topic.replace('-', " ");
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

fn kind_word(kind: &str) -> &str {
    match kind {
        "type_alias" => "type alias",
        "assoc_const" => "constant",
        "assoc_type" => "associated type",
        other => other,
    }
}

fn kind_plural(kind: &str) -> String {
    match kind {
        "type_alias" => "type-aliases".into(),
        "struct" | "enum" | "trait" | "function" | "macro" | "union" | "constant" | "static" => {
            format!("{kind}s")
        }
        other => other.replace('_', "-"),
    }
}

/// One file for the topic, or, over the limit, one file for each item
/// kind (`<topic>--structs.md`), in numbered parts when a kind is still
/// over the limit.
fn split_by_kind(topic: &str, blocks: Vec<Block>) -> Vec<(String, String, Block)> {
    let total: usize = blocks.iter().map(|b| b.lines.len()).sum();
    if total <= LINE_LIMIT {
        let mut page = Block::default();
        for block in blocks {
            page.append(block);
        }
        return vec![(format!("{topic}.md"), title(topic), page)];
    }
    let mut kinds: BTreeMap<String, Vec<Block>> = BTreeMap::new();
    for block in blocks {
        kinds.entry(block.kind.clone()).or_default().push(block);
    }
    let mut files = Vec::new();
    for (kind, blocks) in kinds {
        let plural = kind_plural(&kind);
        let mut parts: Vec<Block> = Vec::new();
        let mut current = Block::default();
        for block in blocks {
            if !current.lines.is_empty() && current.lines.len() + block.lines.len() > LINE_LIMIT {
                parts.push(std::mem::take(&mut current));
            }
            current.append(block);
        }
        parts.push(current);
        let count = parts.len();
        for (number, part) in parts.into_iter().enumerate() {
            let (file, heading) = if count == 1 {
                (
                    format!("{topic}--{plural}.md"),
                    format!("{}: {}", title(topic), plural.replace('-', " ")),
                )
            } else {
                (
                    format!("{topic}--{plural}-{}.md", number + 1),
                    format!(
                        "{}: {}, part {}",
                        title(topic),
                        plural.replace('-', " "),
                        number + 1
                    ),
                )
            };
            files.push((file, heading, part));
        }
    }
    files
}

/// The GitHub anchor of each heading line, outside code blocks.
fn heading_slugs(lines: &[String]) -> BTreeMap<usize, String> {
    let mut slugs = BTreeMap::new();
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    let mut fence: Option<String> = None;
    for (index, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        let marker: String = trimmed
            .chars()
            .take_while(|c| *c == '`' || *c == '~')
            .collect();
        if marker.len() >= 3 {
            match &fence {
                Some(open) if marker.starts_with(&open[..1]) && marker.len() >= open.len() => {
                    fence = None
                }
                Some(_) => {}
                None => fence = Some(marker),
            }
            continue;
        }
        if fence.is_some() {
            continue;
        }
        let hashes = line.chars().take_while(|c| *c == '#').count();
        if hashes == 0 || hashes > 6 || !line[hashes..].starts_with(' ') {
            continue;
        }
        let base = slug(line[hashes..].trim());
        let count = seen.entry(base.clone()).or_insert(0);
        let slug = if *count == 0 {
            base.clone()
        } else {
            format!("{base}-{count}")
        };
        *count += 1;
        slugs.insert(index, slug);
    }
    slugs
}

/// GitHub's heading anchor: lowercase, keep letters, digits, spaces,
/// `-`, and `_`, then spaces become `-`.
pub fn slug(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric() || matches!(c, ' ' | '-' | '_'))
        .map(|c| if c == ' ' { '-' } else { c })
        .collect()
}

/// Joins the lines, resolves the link marks, and ends the text with one
/// newline.
fn finish(
    lines: &[String],
    file: &str,
    targets: &[ItemRef],
    anchors: &BTreeMap<ItemRef, (String, String)>,
) -> String {
    let mut text = String::new();
    for line in lines {
        text.push_str(&resolve_marks(line.trim_end(), file, targets, anchors));
        text.push('\n');
    }
    while text.ends_with("\n\n") {
        text.pop();
    }
    text
}

fn resolve_marks(
    line: &str,
    file: &str,
    targets: &[ItemRef],
    anchors: &BTreeMap<ItemRef, (String, String)>,
) -> String {
    let mut out = String::new();
    let mut parts = line.split(MARK);
    out.push_str(parts.next().unwrap_or_default());
    while let (Some(number), Some(rest)) = (parts.next(), parts.next()) {
        let anchor = number
            .parse::<usize>()
            .ok()
            .and_then(|n| targets.get(n))
            .and_then(|item| anchors.get(item));
        match anchor {
            Some((target_file, slug)) if target_file == file => out.push_str(&format!("#{slug}")),
            Some((target_file, slug)) => out.push_str(&format!("{target_file}#{slug}")),
            None => out.push_str("README.md"),
        }
        out.push_str(rest);
    }
    out
}
