//! The app-author surface: every item of `choscorui::prelude`, plus the
//! extension points that `topics.txt` lists, each with its topic.

use std::collections::{BTreeMap, BTreeSet};

use crate::model::{ItemRef, World};

/// The module whose items are always in scope.
pub const PRELUDE: &str = "choscorui::prelude";
/// The backend escape hatch. It is not part of the app-author surface.
pub const PLATFORM: &str = "choscorui::platform";
/// The trait whose methods are the modifiers of every view.
pub const MODIFIER_TRAIT: &str = "ViewExt";
/// The trait that a view type implements.
pub const VIEW_TRAIT: &str = "View";
/// The prelude types whose public methods need examples.
pub const ENTRY_POINTS: [&str; 3] = ["App", "AppContext", "Scene"];

/// One `<topic> <item path>` line of `topics.txt`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TopicLine {
    pub number: usize,
    pub topic: String,
    pub path: String,
}

/// Reads `topics.txt`. Blank lines and lines that start with `#` are
/// comments.
pub fn parse_topics(text: &str) -> Result<Vec<TopicLine>, Vec<String>> {
    let mut lines = Vec::new();
    let mut errors = Vec::new();
    for (index, raw) in text.lines().enumerate() {
        let number = index + 1;
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        let valid_topic = |t: &str| {
            t.starts_with(|c: char| c.is_ascii_lowercase())
                && t.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        };
        match parts.as_slice() {
            [topic, path] if valid_topic(topic) && path.starts_with("choscorui::") => {
                lines.push(TopicLine { number, topic: (*topic).to_owned(), path: (*path).to_owned() });
            }
            _ => errors.push(format!(
                "topics.txt:{number}: expected `<topic> <item path>`, with a lowercase topic and a path that starts with `choscorui::`"
            )),
        }
    }
    if errors.is_empty() {
        Ok(lines)
    } else {
        Err(errors)
    }
}

/// An in-scope item with the path that an author imports it by.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub import: String,
    pub item: ItemRef,
    pub topic: String,
}

/// A method of the modifier trait and the topic that groups it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Modifier {
    pub name: String,
    pub item: ItemRef,
    pub group: String,
}

/// The whole surface that the pages show.
#[derive(Debug)]
pub struct Scope {
    /// Topic names in their order of first use in `topics.txt`.
    pub topics: Vec<String>,
    pub entries: Vec<Entry>,
    pub modifiers: Vec<Modifier>,
    pub modifier_trait: Option<ItemRef>,
    pub view_trait: Option<ItemRef>,
}

impl Scope {
    pub fn entry(&self, item: ItemRef) -> Option<&Entry> {
        self.entries.iter().find(|entry| entry.item == item)
    }
}

/// What a path in `topics.txt` names.
enum Target {
    Item(ItemRef),
    Method { owner: ItemRef, item: ItemRef },
}

/// Walks a `choscorui::…` path through the modules of the facade.
fn resolve_path(world: &World, path: &str) -> Result<Target, String> {
    let segments: Vec<&str> = path.split("::").collect();
    let mut current = world.root();
    for (index, segment) in segments.iter().enumerate().skip(1) {
        let (kind, _) = world.inner(current);
        if kind == "module" {
            let children = world.children(current)?;
            let mut found: Vec<ItemRef> = children
                .iter()
                .filter(|child| child.name == *segment)
                .map(|child| child.item)
                .collect();
            found.sort();
            found.dedup();
            current = match found.as_slice() {
                [one] => *one,
                [] => {
                    return Err(format!(
                        "no item `{segment}` in `{}`",
                        segments[..index].join("::")
                    ));
                }
                _ => {
                    return Err(format!(
                        "`{}` names more than one item",
                        segments[..=index].join("::")
                    ));
                }
            };
            continue;
        }
        if index + 1 != segments.len() {
            return Err(format!(
                "`{}` is not a module",
                segments[..index].join("::")
            ));
        }
        let members = if kind == "trait" {
            world.trait_members(current)
        } else {
            world.inherent_members(current, &|_| true)
        };
        return match members.get(*segment).map(Vec::as_slice) {
            Some([one]) => Ok(Target::Method {
                owner: current,
                item: *one,
            }),
            _ => Err(format!(
                "no method `{segment}` on `{}`",
                segments[..index].join("::")
            )),
        };
    }
    Ok(Target::Item(current))
}

/// Collects the surface and checks the topic assignment. Each error is
/// one line of the failure report.
pub fn collect(world: &World, lines: &[TopicLine]) -> Result<Scope, Vec<String>> {
    let mut errors = Vec::new();
    let prelude = match resolve_path(world, PRELUDE).and_then(|target| match target {
        Target::Item(item) => world.children(item),
        Target::Method { .. } => Err(format!("`{PRELUDE}` is not a module")),
    }) {
        Ok(children) => children,
        Err(message) => return Err(vec![message]),
    };
    let mut prelude_names: BTreeMap<ItemRef, String> = BTreeMap::new();
    for child in &prelude {
        prelude_names
            .entry(child.item)
            .or_insert_with(|| child.name.clone());
    }
    let modifier_trait = prelude
        .iter()
        .find(|c| c.name == MODIFIER_TRAIT)
        .map(|c| c.item);
    let view_trait = prelude
        .iter()
        .find(|c| c.name == VIEW_TRAIT)
        .map(|c| c.item);

    let mut topics = Vec::new();
    let mut seen: BTreeMap<ItemRef, usize> = BTreeMap::new();
    let mut entries = Vec::new();
    let mut modifiers = Vec::new();
    for line in lines {
        if line.path == PLATFORM || line.path.starts_with(&format!("{PLATFORM}::")) {
            errors.push(format!(
                "topics.txt:{}: `{PLATFORM}` is not part of the app-author surface",
                line.number
            ));
            continue;
        }
        let target = match resolve_path(world, &line.path) {
            Ok(target) => target,
            Err(message) => {
                errors.push(format!(
                    "topics.txt:{}: unknown item `{}`: {message}",
                    line.number, line.path
                ));
                continue;
            }
        };
        let item = match target {
            Target::Item(item) | Target::Method { item, .. } => item,
        };
        if let Some(first) = seen.insert(item, line.number) {
            errors.push(format!(
                "topics.txt:{}: `{}` is listed twice (first on line {first})",
                line.number, line.path
            ));
            continue;
        }
        if !topics.contains(&line.topic) {
            topics.push(line.topic.clone());
        }
        match target {
            Target::Method { owner, item } => {
                if Some(owner) != modifier_trait {
                    errors.push(format!(
                        "topics.txt:{}: only the methods of `{MODIFIER_TRAIT}` take a topic, not `{}`",
                        line.number, line.path
                    ));
                    continue;
                }
                let name = world.name(item).to_owned();
                modifiers.push(Modifier {
                    name,
                    item,
                    group: line.topic.clone(),
                });
            }
            Target::Item(item) => {
                let (name, import) = match prelude_names.get(&item) {
                    Some(name) => (name.clone(), format!("{PRELUDE}::{name}")),
                    None => {
                        let name = line.path.rsplit("::").next().unwrap_or_default().to_owned();
                        (name, line.path.clone())
                    }
                };
                entries.push(Entry {
                    name,
                    import,
                    item,
                    topic: line.topic.clone(),
                });
            }
        }
    }
    let listed: BTreeSet<ItemRef> = seen.keys().copied().collect();
    for (item, name) in &prelude_names {
        if !listed.contains(item) {
            errors.push(format!("`{PRELUDE}::{name}` has no topic in topics.txt"));
        }
    }
    if let Some(modifier_trait) = modifier_trait {
        for (name, items) in world.trait_members(modifier_trait) {
            if items.iter().any(|item| !listed.contains(item)) {
                errors.push(format!(
                    "`{PRELUDE}::{MODIFIER_TRAIT}::{name}` has no topic in topics.txt"
                ));
            }
        }
    }
    if !errors.is_empty() {
        errors.sort();
        return Err(errors);
    }
    entries.sort_by(|a, b| (&a.topic, &a.name, a.item).cmp(&(&b.topic, &b.name, b.item)));
    modifiers.sort_by(|a, b| (&a.group, &a.name, a.item).cmp(&(&b.group, &b.name, b.item)));
    Ok(Scope {
        topics,
        entries,
        modifiers,
        modifier_trait,
        view_trait,
    })
}
