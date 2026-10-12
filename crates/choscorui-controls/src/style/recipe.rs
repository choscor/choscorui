//! The recipe trait, the registry of recipes per recipe set, and the
//! resolved-style cache.

use super::{ControlKind, ControlSize, ResolvedStyle, Slot, Variant, VisualState, default};
use choscorui_core::{DesignTokens, RecipeSet};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

/// Everything a recipe may read about the slot it styles. It holds no
/// input, focus target, command, or semantics, so a recipe cannot change
/// behavior.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct StyleQuery {
    pub kind: ControlKind,
    pub slot: Slot,
    pub variant: Variant,
    pub size: ControlSize,
    pub state: VisualState,
}

impl StyleQuery {
    pub fn new(kind: ControlKind, slot: Slot) -> Self {
        Self {
            kind,
            slot,
            variant: Variant::default(),
            size: ControlSize::Default,
            state: VisualState::NONE,
        }
    }

    #[must_use]
    pub fn variant(mut self, variant: impl Into<Variant>) -> Self {
        self.variant = variant.into();
        self
    }

    #[must_use]
    pub fn size(mut self, size: ControlSize) -> Self {
        self.size = size;
        self
    }

    #[must_use]
    pub fn state(mut self, state: VisualState) -> Self {
        self.state = state;
        self
    }

    #[must_use]
    pub fn slot(mut self, slot: Slot) -> Self {
        self.slot = slot;
        self
    }
}

/// Maps a style query and the tokens to a resolved style. One trait object
/// type serves every control kind. Return `None` to fall back to the
/// default recipe, so a custom recipe can replace only some slots.
pub trait Recipe {
    fn resolve(&self, query: &StyleQuery, tokens: &DesignTokens) -> Option<ResolvedStyle>;
}

/// The built-in look of every control kind.
#[derive(Clone, Copy, Debug, Default)]
pub struct DefaultRecipe;

impl Recipe for DefaultRecipe {
    fn resolve(&self, query: &StyleQuery, tokens: &DesignTokens) -> Option<ResolvedStyle> {
        Some(default::resolve(query, tokens))
    }
}

/// Cache counters, for the performance tests.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct StyleCacheStats {
    pub hits: u64,
    pub misses: u64,
    pub entries: usize,
}

const CACHE_LIMIT: usize = 8192;

/// The recipe registry and the resolved-style cache. Register a recipe for
/// a control kind in a [`RecipeSet`]; a scope selects the set through the
/// `recipes` theme token. Paint looks styles up here, so it does not
/// compute a style again for each node.
#[derive(Default)]
pub struct Styles {
    recipes: HashMap<(RecipeSet, ControlKind), Rc<dyn Recipe>>,
    cache: RefCell<HashMap<(u64, StyleQuery), ResolvedStyle>>,
    hits: Cell<u64>,
    misses: Cell<u64>,
}

impl Styles {
    pub fn new() -> Self {
        Self::default()
    }

    /// Paints `kind` with `recipe` in every scope whose tokens select
    /// `set`. Use [`RecipeSet::DEFAULT`] to replace a recipe globally.
    pub fn set_recipe(&mut self, set: RecipeSet, kind: ControlKind, recipe: Rc<dyn Recipe>) {
        self.recipes.insert((set, kind), recipe);
        self.cache.borrow_mut().clear();
    }

    /// The recipe registered for `kind` in `set`, if any.
    pub fn recipe(&self, set: RecipeSet, kind: ControlKind) -> Option<Rc<dyn Recipe>> {
        self.recipes.get(&(set, kind)).cloned()
    }

    /// Registers in `to` every recipe that `from` has, so that a new set
    /// starts as a copy of an existing one.
    pub fn copy_set(&mut self, from: RecipeSet, to: RecipeSet) {
        let copies: Vec<_> = self
            .recipes
            .iter()
            .filter(|((set, _), _)| *set == from)
            .map(|((_, kind), recipe)| (*kind, recipe.clone()))
            .collect();
        for (kind, recipe) in copies {
            self.recipes.insert((to, kind), recipe);
        }
        self.cache.borrow_mut().clear();
    }

    /// Removes every recipe of `set`.
    pub fn remove_set(&mut self, set: RecipeSet) {
        self.recipes.retain(|(candidate, _), _| *candidate != set);
        self.cache.borrow_mut().clear();
    }

    /// Forgets resolved styles, after a registered recipe changed what it
    /// returns.
    pub fn clear_cache(&self) {
        self.cache.borrow_mut().clear();
    }

    /// Removes a registered recipe; the kind paints with the default again.
    pub fn remove_recipe(&mut self, set: RecipeSet, kind: ControlKind) -> bool {
        self.cache.borrow_mut().clear();
        self.recipes.remove(&(set, kind)).is_some()
    }

    /// The style of `query` under `tokens`, from the cache when possible.
    pub fn resolve(&self, tokens: &DesignTokens, query: StyleQuery) -> ResolvedStyle {
        self.resolve_keyed(tokens.fingerprint(), tokens, query)
    }

    /// As [`Styles::resolve`], with a precomputed token fingerprint.
    pub fn resolve_keyed(
        &self,
        key: u64,
        tokens: &DesignTokens,
        query: StyleQuery,
    ) -> ResolvedStyle {
        if let Some(style) = self.cache.borrow().get(&(key, query)) {
            self.hits.set(self.hits.get() + 1);
            return style.clone();
        }
        self.misses.set(self.misses.get() + 1);
        let style = self.compute(tokens, &query);
        let mut cache = self.cache.borrow_mut();
        if cache.len() >= CACHE_LIMIT {
            cache.clear();
        }
        cache.insert((key, query), style.clone());
        style
    }

    fn compute(&self, tokens: &DesignTokens, query: &StyleQuery) -> ResolvedStyle {
        let custom = [tokens.recipes, RecipeSet::DEFAULT]
            .into_iter()
            .filter_map(|set| self.recipes.get(&(set, query.kind)))
            .find_map(|recipe| recipe.resolve(query, tokens));
        custom.unwrap_or_else(|| default::resolve(query, tokens))
    }

    pub fn stats(&self) -> StyleCacheStats {
        StyleCacheStats {
            hits: self.hits.get(),
            misses: self.misses.get(),
            entries: self.cache.borrow().len(),
        }
    }
}

impl std::fmt::Debug for Styles {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Styles")
            .field("recipes", &self.recipes.len())
            .field("stats", &self.stats())
            .finish()
    }
}
