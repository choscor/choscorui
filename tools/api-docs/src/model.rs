//! The rustdoc JSON of several crates as one set. Rustdoc writes the
//! items of one crate per file and refers to the items of other crates
//! by path, so a re-export resolves through the path tables.

use std::collections::{BTreeMap, HashMap};

use crate::json::Json;

/// The JSON `format_version` that this renderer reads. Bump it together
/// with `tools/api-docs/nightly.txt`.
pub const FORMAT_VERSION: u32 = 61;

/// One item: the index of its crate in the [`World`] and its id there.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ItemRef {
    pub krate: usize,
    pub id: u32,
}

/// The rustdoc JSON of one crate.
pub struct Krate {
    pub name: String,
    root: u32,
    index: HashMap<u32, Json>,
    /// Id to (crate id, path, kind) for local and external items.
    paths: HashMap<u32, (u32, Vec<String>, String)>,
    /// The id of each local item by its path and kind.
    by_path: HashMap<(Vec<String>, String), u32>,
    externals: HashMap<u32, String>,
}

/// An error in the structure of the JSON.
#[derive(Debug, PartialEq, Eq)]
pub enum ModelError {
    FormatVersion { expected: u32, actual: Option<u32> },
    Malformed(String),
}

impl std::fmt::Display for ModelError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ModelError::FormatVersion { expected, actual } => {
                let actual = actual.map_or_else(|| "none".to_owned(), |v| v.to_string());
                write!(
                    f,
                    "rustdoc json format_version is {actual}, the renderer reads {expected}; \
                     bump tools/api-docs/nightly.txt and FORMAT_VERSION in \
                     tools/api-docs/src/model.rs together"
                )
            }
            ModelError::Malformed(message) => write!(f, "malformed rustdoc json: {message}"),
        }
    }
}

fn id_of(value: &Json) -> Option<u32> {
    value
        .as_u32()
        .or_else(|| value.as_str().and_then(|text| text.parse().ok()))
}

impl Krate {
    /// Reads one parsed rustdoc JSON document.
    pub fn from_json(json: Json) -> Result<Krate, ModelError> {
        let version = json.get("format_version").as_u32();
        if version != Some(FORMAT_VERSION) {
            return Err(ModelError::FormatVersion {
                expected: FORMAT_VERSION,
                actual: version,
            });
        }
        let Json::Object(mut top) = json else {
            return Err(ModelError::Malformed("top level is not an object".into()));
        };
        let root = top
            .get("root")
            .and_then(id_of)
            .ok_or_else(|| missing("root"))?;
        let mut index = HashMap::new();
        if let Some(Json::Object(items)) = top.remove("index") {
            for (key, item) in items {
                let id = key
                    .parse()
                    .map_err(|_| ModelError::Malformed(format!("id {key}")))?;
                index.insert(id, item);
            }
        }
        let mut paths = HashMap::new();
        let mut by_path = HashMap::new();
        for (key, entry) in top
            .get("paths")
            .and_then(Json::as_object)
            .into_iter()
            .flatten()
        {
            let id: u32 = key
                .parse()
                .map_err(|_| ModelError::Malformed(format!("id {key}")))?;
            let crate_id = entry
                .get("crate_id")
                .as_u32()
                .ok_or_else(|| missing("crate_id"))?;
            let path: Vec<String> = entry
                .get("path")
                .items()
                .iter()
                .filter_map(Json::as_str)
                .map(str::to_owned)
                .collect();
            let kind = entry.get("kind").as_str().unwrap_or_default().to_owned();
            if crate_id == 0 {
                by_path.insert((path.clone(), kind.clone()), id);
            }
            paths.insert(id, (crate_id, path, kind));
        }
        let mut externals = HashMap::new();
        for (key, entry) in top
            .get("external_crates")
            .and_then(Json::as_object)
            .into_iter()
            .flatten()
        {
            if let (Ok(id), Some(name)) = (key.parse(), entry.get("name").as_str()) {
                externals.insert(id, name.to_owned());
            }
        }
        let name = index
            .get(&root)
            .and_then(|item| item.get("name").as_str())
            .ok_or_else(|| missing("root item"))?
            .to_owned();
        Ok(Krate {
            name,
            root,
            index,
            paths,
            by_path,
            externals,
        })
    }

    /// The names of the crates that this crate refers to.
    pub fn external_names(&self) -> impl Iterator<Item = &str> {
        self.externals.values().map(String::as_str)
    }
}

fn missing(what: &str) -> ModelError {
    ModelError::Malformed(format!("no {what}"))
}

/// The crates that the renderer reads. The first crate is the facade.
pub struct World {
    crates: Vec<Krate>,
}

/// One public name in a module: the name it has there and its item.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Child {
    pub name: String,
    pub item: ItemRef,
}

impl World {
    pub fn new(crates: Vec<Krate>) -> World {
        World { crates }
    }

    pub fn root(&self) -> ItemRef {
        ItemRef {
            krate: 0,
            id: self.crates[0].root,
        }
    }

    /// The JSON object of an item. Every [`ItemRef`] that the world gives
    /// out names an indexed item.
    pub fn item(&self, item: ItemRef) -> &Json {
        static NULL: Json = Json::Null;
        self.crates[item.krate].index.get(&item.id).unwrap_or(&NULL)
    }

    pub fn name(&self, item: ItemRef) -> &str {
        self.item(item).get("name").as_str().unwrap_or_default()
    }

    /// The kind and the value of an item's `inner`.
    pub fn inner(&self, item: ItemRef) -> (&str, &Json) {
        self.item(item)
            .get("inner")
            .variant()
            .unwrap_or(("", &Json::Null))
    }

    pub fn docs(&self, item: ItemRef) -> &str {
        self.item(item).get("docs").as_str().unwrap_or_default()
    }

    /// Resolves an id that appears inside the JSON of crate `krate` to
    /// the item that defines it, in this crate or in another loaded one.
    pub fn resolve(&self, krate: usize, id: &Json) -> Option<ItemRef> {
        let id = id_of(id)?;
        let local = &self.crates[krate];
        if local.index.contains_key(&id) {
            return Some(ItemRef { krate, id });
        }
        let (crate_id, path, kind) = local.paths.get(&id)?;
        let name = if *crate_id == 0 {
            &local.name
        } else {
            local.externals.get(crate_id)?
        };
        let target = self.crates.iter().position(|other| &other.name == name)?;
        let key = (path.clone(), kind.clone());
        let id = *self.crates[target].by_path.get(&key)?;
        self.crates[target]
            .index
            .contains_key(&id)
            .then_some(ItemRef { krate: target, id })
    }

    /// The crate and path of an id that does not resolve, for messages.
    pub fn describe_unresolved(&self, krate: usize, id: &Json) -> String {
        let entry = id_of(id).and_then(|id| self.crates[krate].paths.get(&id));
        match entry {
            Some((_, path, kind)) => format!("{kind} {}", path.join("::")),
            None => "an item without a path".to_owned(),
        }
    }

    /// The public names of a module, with `use` items and glob imports
    /// resolved. Each error names a re-export that does not resolve.
    pub fn children(&self, module: ItemRef) -> Result<Vec<Child>, String> {
        let mut out = Vec::new();
        self.collect_children(module, &mut out, 0)?;
        Ok(out)
    }

    fn collect_children(
        &self,
        module: ItemRef,
        out: &mut Vec<Child>,
        depth: usize,
    ) -> Result<(), String> {
        if depth > 32 {
            return Err(format!(
                "glob imports of module {} nest too deeply",
                self.name(module)
            ));
        }
        let (kind, inner) = self.inner(module);
        if kind != "module" {
            return Ok(());
        }
        for id in inner.get("items").items() {
            let Some(child) = self.resolve(module.krate, id) else {
                continue;
            };
            let (kind, inner) = self.inner(child);
            match kind {
                "use" => {
                    let target = inner.get("id");
                    if target.is_null() {
                        continue;
                    }
                    let resolved = self.resolve(child.krate, target).ok_or_else(|| {
                        format!(
                            "re-export {} does not resolve; is its crate loaded?",
                            self.describe_unresolved(child.krate, target)
                        )
                    })?;
                    if inner.get("is_glob").as_bool() {
                        self.collect_children(resolved, out, depth + 1)?;
                    } else {
                        let name = inner.get("name").as_str().unwrap_or_default().to_owned();
                        out.push(Child {
                            name,
                            item: resolved,
                        });
                    }
                }
                "impl" => {}
                _ => out.push(Child {
                    name: self.name(child).to_owned(),
                    item: child,
                }),
            }
        }
        Ok(())
    }

    /// The impl blocks of a struct, enum, or union, from its own crate.
    pub fn impls(&self, item: ItemRef) -> Vec<ItemRef> {
        let (_, inner) = self.inner(item);
        inner
            .get("impls")
            .items()
            .iter()
            .filter_map(|id| self.resolve(item.krate, id))
            .collect()
    }

    /// The trait that an impl block implements, when it resolves.
    pub fn impl_trait(&self, imp: ItemRef) -> Option<ItemRef> {
        let (_, inner) = self.inner(imp);
        let path = inner.get("trait");
        if path.is_null() {
            return None;
        }
        self.resolve(imp.krate, path.get("id"))
    }

    /// The public functions, constants, and types of the inherent impl
    /// blocks of an item, by name.
    /// `accept` sees the self type of each impl block and picks the
    /// blocks to read.
    pub fn inherent_members(
        &self,
        item: ItemRef,
        accept: &dyn Fn(&Json) -> bool,
    ) -> BTreeMap<String, Vec<ItemRef>> {
        let mut members: BTreeMap<String, Vec<ItemRef>> = BTreeMap::new();
        for imp in self.impls(item) {
            let (_, inner) = self.inner(imp);
            if !inner.get("trait").is_null()
                || inner.get("is_synthetic").as_bool()
                || !accept(inner.get("for"))
            {
                continue;
            }
            for id in inner.get("items").items() {
                let Some(member) = self.resolve(imp.krate, id) else {
                    continue;
                };
                if self.item(member).get("visibility").as_str() == Some("public") {
                    members
                        .entry(self.name(member).to_owned())
                        .or_default()
                        .push(member);
                }
            }
        }
        members
    }

    /// The type that a type alias names, when it is a resolved path.
    pub fn alias_target(&self, item: ItemRef) -> Option<ItemRef> {
        let (kind, inner) = self.inner(item);
        let path = inner.get("type").get("resolved_path");
        (kind == "type_alias" && !path.is_null())
            .then(|| self.resolve(item.krate, path.get("id")))
            .flatten()
    }

    /// The items of a trait, by name.
    pub fn trait_members(&self, item: ItemRef) -> BTreeMap<String, Vec<ItemRef>> {
        let mut members: BTreeMap<String, Vec<ItemRef>> = BTreeMap::new();
        let (_, inner) = self.inner(item);
        for id in inner.get("items").items() {
            if let Some(member) = self.resolve(item.krate, id) {
                members
                    .entry(self.name(member).to_owned())
                    .or_default()
                    .push(member);
            }
        }
        members
    }

    /// The types with a non-blanket impl of `trait_item`, from the
    /// trait's list of implementations and from `candidates`' own impls.
    pub fn implementors(&self, trait_item: ItemRef, candidates: &[ItemRef]) -> Vec<ItemRef> {
        let mut out = Vec::new();
        let (_, inner) = self.inner(trait_item);
        for id in inner.get("implementations").items() {
            let Some(imp) = self.resolve(trait_item.krate, id) else {
                continue;
            };
            if let Some(ty) = self.impl_self(imp) {
                out.push(ty);
            }
        }
        for &candidate in candidates {
            if self.implements(candidate, trait_item) {
                out.push(candidate);
            }
        }
        out.sort();
        out.dedup();
        out
    }

    /// The type of a non-blanket impl block, when it is a resolved path.
    fn impl_self(&self, imp: ItemRef) -> Option<ItemRef> {
        let (_, inner) = self.inner(imp);
        if !inner.get("blanket_impl").is_null() || inner.get("is_synthetic").as_bool() {
            return None;
        }
        let (kind, path) = inner.get("for").variant()?;
        (kind == "resolved_path")
            .then(|| self.resolve(imp.krate, path.get("id")))
            .flatten()
    }

    /// Whether `item` has a non-blanket impl of `trait_item`.
    pub fn implements(&self, item: ItemRef, trait_item: ItemRef) -> bool {
        self.impls(item)
            .into_iter()
            .any(|imp| self.impl_self(imp).is_some() && self.impl_trait(imp) == Some(trait_item))
    }
}
