//! Design tokens and their resolution.
//!
//! Resolution order, lowest to highest precedence:
//! 1. the application theme ([`ThemeResolver::set_application`]);
//! 2. scoped overrides on each ancestor, applied root first
//!    ([`ThemeResolver::set_scope`]);
//! 3. the node's own instance override ([`ThemeResolver::set_instance`]);
//! 4. user accessibility preferences (text scale, high contrast, reduced
//!    motion), which always win.
//!
//! Light and dark are color schemes of a base palette. Metric changes (text
//! scale, fonts, spacing, radius, border and ring width) invalidate layout;
//! color, shadow, and motion changes invalidate paint only.

mod contrast;
mod palette;
mod scale;

pub use palette::{BaseColor, ColorRole, ColorScheme, Oklch, Palette, source as palette_source};
pub use scale::{
    FULL_RADIUS, FontFamily, RadiusScale, ShadowLayers, ShadowScale, ShadowTable, Space,
    TextPreset, TextSize, Tracking, TypeScale, TypeSpec, Typeset,
};

use crate::{Color, CubicBezier, Dirty, FontSlant, FontWeight, NodeId, Rect, Tree};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Contrast {
    Normal,
    High,
}

/// User accessibility settings applied after application, scope, and instance
/// theme values. A scale of `1.0` preserves the authored type size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AccessibilityPreferences {
    pub high_contrast: bool,
    pub reduced_motion: bool,
    pub text_scale: f64,
}

impl Default for AccessibilityPreferences {
    fn default() -> Self {
        Self {
            high_contrast: false,
            reduced_motion: false,
            text_scale: 1.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MotionTokens {
    pub duration_ms: u32,
    pub easing: CubicBezier,
    pub reduced: bool,
}

impl Default for MotionTokens {
    fn default() -> Self {
        Self {
            duration_ms: 150,
            easing: CubicBezier::STANDARD,
            reduced: false,
        }
    }
}

/// Resolved design values. Geometry-affecting values are explicit so a theme
/// update can distinguish layout from paint work.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DesignTokens {
    pub base: BaseColor,
    pub scheme: ColorScheme,
    pub palette: Palette,
    /// Base corner radius; [`RadiusScale`] steps are multiples of it.
    pub radius: f64,
    /// Spacing unit; `space(n)` is `n` units.
    pub spacing: f64,
    pub border_width: f64,
    /// Width of the outset focus ring.
    pub ring_width: f64,
    /// Alpha of the focus ring color.
    pub ring_alpha: f64,
    /// Multiplies every step of the type scale.
    pub text_scale: f64,
    /// Font size and line height of each [`TextSize`] step.
    pub type_scale: TypeScale,
    /// The layers of each [`ShadowScale`] step.
    pub shadows: ShadowTable,
    pub font_sans: FontFamily,
    pub font_mono: FontFamily,
    pub motion: MotionTokens,
    pub contrast: Contrast,
    /// The recipe set that paints controls. Controls map it to their
    /// registered recipes; [`RecipeSet::DEFAULT`] is the built-in look.
    pub recipes: RecipeSet,
}

/// Names a set of control recipes, so that one scope can paint with
/// replaced recipes. The set itself lives with the controls.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RecipeSet(pub u32);

impl RecipeSet {
    pub const DEFAULT: Self = Self(0);
}

impl DesignTokens {
    /// The built-in theme of `base` in `scheme`.
    pub fn new(base: BaseColor, scheme: ColorScheme) -> Self {
        Self {
            base,
            scheme,
            palette: Palette::new(base, scheme),
            radius: 10.0,
            spacing: 4.0,
            border_width: 1.0,
            ring_width: 3.0,
            ring_alpha: 0.5,
            text_scale: 1.0,
            type_scale: TypeScale::DEFAULT,
            shadows: ShadowTable::standard(),
            font_sans: FontFamily::SYSTEM_UI,
            font_mono: FontFamily::SYSTEM_MONO,
            motion: MotionTokens::default(),
            contrast: Contrast::Normal,
            recipes: RecipeSet::DEFAULT,
        }
    }

    /// The neutral base in the dark scheme.
    pub fn dark() -> Self {
        Self::new(BaseColor::Neutral, ColorScheme::Dark)
    }

    /// The neutral base in the light scheme.
    pub fn light() -> Self {
        Self::new(BaseColor::Neutral, ColorScheme::Light)
    }

    /// The color of `role` in these tokens.
    pub const fn color(&self, role: ColorRole) -> Color {
        self.palette.get(role)
    }

    /// The radius of `step` for a box of `bounds`, clamped to a pill.
    pub fn radius_for(&self, step: RadiusScale, bounds: Rect) -> f64 {
        let half = bounds.size.width.min(bounds.size.height).max(0.0) / 2.0;
        match step.factor() {
            Some(factor) => (self.radius * factor).min(half),
            None => half,
        }
    }

    /// The unclamped radius of `step`; [`RadiusScale::Full`] is
    /// [`FULL_RADIUS`].
    pub fn radius(&self, step: RadiusScale) -> f64 {
        step.factor()
            .map_or(FULL_RADIUS, |factor| self.radius * factor)
    }

    /// `units` steps of the spacing unit.
    pub fn space(&self, units: f64) -> f64 {
        self.spacing * units
    }

    /// The length of a step of the spacing scale.
    pub fn gap(&self, step: Space) -> f64 {
        self.space(step.units())
    }

    /// The sans text spec of `size` at `weight`, scaled by the text scale.
    pub fn text(&self, size: TextSize, weight: FontWeight) -> TypeSpec {
        let (font_size, line_height) = self.type_scale.metrics(size);
        TypeSpec {
            family: self.font_sans,
            size: font_size * self.text_scale,
            line_height: line_height * self.text_scale,
            weight,
            slant: FontSlant::Upright,
            tracking: 0.0,
        }
    }

    /// The monospace text spec of `size` at `weight`.
    pub fn mono(&self, size: TextSize, weight: FontWeight) -> TypeSpec {
        TypeSpec {
            family: self.font_mono,
            ..self.text(size, weight)
        }
    }

    /// A named typography preset.
    pub fn preset(&self, preset: TextPreset) -> TypeSpec {
        let rule = preset.rule();
        let mut spec = if rule.mono {
            self.mono(rule.size, rule.weight)
        } else {
            self.text(rule.size, rule.weight)
        }
        .with_tracking(rule.tracking);
        spec.slant = rule.slant;
        if let Some(line_height) = rule.line_height {
            spec.line_height = line_height * self.text_scale;
        }
        spec
    }

    /// The layers of `step` in the theme's shadow table.
    pub fn shadow(&self, step: ShadowScale) -> ShadowLayers {
        self.shadows.get(step)
    }

    /// The body text of `typeset`, scaled by the text scale: the sans face
    /// at the typeset size and leading.
    pub fn typeset(&self, typeset: Typeset) -> TypeSpec {
        TypeSpec {
            family: self.font_sans,
            size: typeset.size * self.text_scale,
            line_height: typeset.line_height() * self.text_scale,
            weight: FontWeight::REGULAR,
            slant: FontSlant::Upright,
            tracking: 0.0,
        }
    }

    /// The focus ring color: `color` at the ring alpha.
    pub fn ring_color(&self, color: Color) -> Color {
        color.with_alpha(self.ring_alpha)
    }

    /// A fingerprint of every token, for caches keyed by theme.
    pub fn fingerprint(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        (self.base, self.scheme, self.palette).hash(&mut hasher);
        for value in [
            self.radius,
            self.spacing,
            self.border_width,
            self.ring_width,
            self.ring_alpha,
            self.text_scale,
            self.motion.easing.x1,
            self.motion.easing.y1,
            self.motion.easing.x2,
            self.motion.easing.y2,
        ] {
            value.to_bits().hash(&mut hasher);
        }
        for (size, line) in TextSize::ALL.map(|step| self.type_scale.metrics(step)) {
            (size.to_bits(), line.to_bits()).hash(&mut hasher);
        }
        self.shadows.hash_into(&mut hasher);
        (self.font_sans, self.font_mono, self.recipes).hash(&mut hasher);
        (self.motion.duration_ms, self.motion.reduced).hash(&mut hasher);
        (self.contrast == Contrast::High).hash(&mut hasher);
        hasher.finish()
    }

    /// The duration of a standard transition, zero with reduced motion.
    pub fn duration(&self) -> std::time::Duration {
        std::time::Duration::from_millis(u64::from(self.motion.duration_ms))
    }
}

/// Optional replacements for each token. A base or scheme rebuilds the
/// palette first; then a whole palette, then single roles, apply.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ThemeOverride {
    pub base: Option<BaseColor>,
    pub scheme: Option<ColorScheme>,
    pub palette: Option<Palette>,
    pub roles: RoleOverrides,
    pub radius: Option<f64>,
    pub spacing: Option<f64>,
    pub border_width: Option<f64>,
    pub ring_width: Option<f64>,
    pub ring_alpha: Option<f64>,
    pub text_scale: Option<f64>,
    pub type_scale: Option<TypeScale>,
    pub shadows: Option<ShadowTable>,
    pub font_sans: Option<FontFamily>,
    pub font_mono: Option<FontFamily>,
    pub motion: Option<MotionTokens>,
    pub contrast: Option<Contrast>,
    pub recipes: Option<RecipeSet>,
}

impl ThemeOverride {
    /// An override of one color role.
    pub fn role(role: ColorRole, color: Color) -> Self {
        let mut overrides = Self::default();
        overrides.roles.set(role, color);
        overrides
    }

    /// An override that switches the base and the scheme.
    pub fn theme(base: BaseColor, scheme: ColorScheme) -> Self {
        Self {
            base: Some(base),
            scheme: Some(scheme),
            ..Self::default()
        }
    }
}

/// Per-role color overrides.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RoleOverrides([Option<Color>; ColorRole::COUNT]);

impl RoleOverrides {
    pub fn set(&mut self, role: ColorRole, color: Color) {
        self.0[role as usize] = Some(color);
    }

    pub fn clear(&mut self, role: ColorRole) {
        self.0[role as usize] = None;
    }

    pub fn get(&self, role: ColorRole) -> Option<Color> {
        self.0[role as usize]
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ThemeInvalidation {
    None,
    Paint,
    Layout,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ThemeError {
    MissingNode,
    InvalidTokens,
    InvalidPreferences,
}

impl std::fmt::Display for ThemeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingNode => f.write_str("the node is not in the tree"),
            Self::InvalidTokens => f.write_str("theme tokens have a non-finite or negative value"),
            Self::InvalidPreferences => f.write_str("the text scale must be positive and finite"),
        }
    }
}

impl std::error::Error for ThemeError {}

pub struct ThemeResolver {
    application: DesignTokens,
    scopes: HashMap<NodeId, ThemeOverride>,
    accessibility: AccessibilityPreferences,
}

impl ThemeResolver {
    pub fn new(application: DesignTokens) -> Self {
        Self {
            application,
            scopes: HashMap::new(),
            accessibility: AccessibilityPreferences::default(),
        }
    }

    /// The application tokens with accessibility preferences applied,
    /// without scope overrides.
    pub fn application(&self) -> DesignTokens {
        apply_accessibility(self.application, self.accessibility)
    }

    pub fn resolve(&self, tree: &Tree, id: NodeId) -> Result<DesignTokens, ThemeError> {
        if !tree.contains(id) {
            return Err(ThemeError::MissingNode);
        }
        let mut path = Vec::new();
        let mut current = Some(id);
        while let Some(node) = current {
            path.push(node);
            current = tree.parent(node);
        }
        let mut tokens = self.application;
        for node in path.into_iter().rev() {
            if let Some(overrides) = self.scopes.get(&node) {
                tokens = apply(tokens, *overrides);
            }
        }
        let tokens = apply_accessibility(tokens, self.accessibility);
        validate(tokens)?;
        Ok(tokens)
    }

    pub fn set_accessibility_preferences(
        &mut self,
        tree: &mut Tree,
        preferences: AccessibilityPreferences,
    ) -> Result<ThemeInvalidation, ThemeError> {
        if !preferences.text_scale.is_finite() || preferences.text_scale <= 0.0 {
            return Err(ThemeError::InvalidPreferences);
        }
        let old = self.resolve(tree, tree.root())?;
        let previous = self.accessibility;
        self.accessibility = preferences;
        let new = match self.resolve(tree, tree.root()) {
            Ok(tokens) => tokens,
            Err(error) => {
                self.accessibility = previous;
                return Err(error);
            }
        };
        let change = classify(old, new);
        invalidate(tree, tree.root(), change)?;
        Ok(change)
    }

    pub fn set_application(
        &mut self,
        tree: &mut Tree,
        tokens: DesignTokens,
    ) -> Result<ThemeInvalidation, ThemeError> {
        validate(tokens)?;
        let old = self.application;
        self.application = tokens;
        let change = classify(old, tokens);
        invalidate(tree, tree.root(), change)?;
        Ok(change)
    }

    pub fn set_scope(
        &mut self,
        tree: &mut Tree,
        id: NodeId,
        overrides: ThemeOverride,
    ) -> Result<ThemeInvalidation, ThemeError> {
        let old = self.resolve(tree, id)?;
        let previous = self.scopes.insert(id, overrides);
        let new = match self.resolve(tree, id) {
            Ok(tokens) => tokens,
            Err(error) => {
                if let Some(previous) = previous {
                    self.scopes.insert(id, previous);
                } else {
                    self.scopes.remove(&id);
                }
                return Err(error);
            }
        };
        let change = classify(old, new);
        invalidate(tree, id, change)?;
        Ok(change)
    }

    pub fn set_instance(
        &mut self,
        tree: &mut Tree,
        id: NodeId,
        overrides: ThemeOverride,
    ) -> Result<ThemeInvalidation, ThemeError> {
        self.set_scope(tree, id, overrides)
    }

    /// Drops the scopes of nodes that are no longer in `tree`.
    pub fn prune(&mut self, tree: &Tree) {
        self.scopes.retain(|node, _| tree.contains(*node));
    }

    /// Whether `id` has scope overrides.
    pub fn has_scope(&self, id: NodeId) -> bool {
        self.scopes.contains_key(&id)
    }

    /// The application tokens before accessibility preferences, where a
    /// tree walk with [`ThemeResolver::scoped`] starts.
    pub fn base(&self) -> DesignTokens {
        self.application
    }

    /// `parent`'s tokens with `id`'s own overrides, if any, still before
    /// accessibility. A tree walk carries these from parent to child and
    /// calls [`ThemeResolver::finish`] for each node it paints, so it
    /// resolves each node once.
    pub fn scoped(&self, parent: &DesignTokens, id: NodeId) -> DesignTokens {
        match self.scopes.get(&id) {
            Some(overrides) => apply(*parent, *overrides),
            None => *parent,
        }
    }

    /// Tokens from [`ThemeResolver::scoped`] with the accessibility
    /// preferences applied, as [`ThemeResolver::resolve`] returns them.
    pub fn finish(&self, tokens: DesignTokens) -> DesignTokens {
        apply_accessibility(tokens, self.accessibility)
    }

    pub fn remove_scope(
        &mut self,
        tree: &mut Tree,
        id: NodeId,
    ) -> Result<ThemeInvalidation, ThemeError> {
        let old = self.resolve(tree, id)?;
        let Some(previous) = self.scopes.remove(&id) else {
            return Ok(ThemeInvalidation::None);
        };
        let new = match self.resolve(tree, id) {
            Ok(tokens) => tokens,
            Err(error) => {
                self.scopes.insert(id, previous);
                return Err(error);
            }
        };
        let change = classify(old, new);
        invalidate(tree, id, change)?;
        Ok(change)
    }
}

fn apply(mut tokens: DesignTokens, values: ThemeOverride) -> DesignTokens {
    if values.base.is_some() || values.scheme.is_some() {
        tokens.base = values.base.unwrap_or(tokens.base);
        tokens.scheme = values.scheme.unwrap_or(tokens.scheme);
        tokens.palette = Palette::new(tokens.base, tokens.scheme);
    }
    if let Some(palette) = values.palette {
        tokens.palette = palette;
    }
    for role in ColorRole::ALL {
        if let Some(color) = values.roles.get(role) {
            tokens.palette.set(role, color);
        }
    }
    let metrics = [
        (&mut tokens.radius, values.radius),
        (&mut tokens.spacing, values.spacing),
        (&mut tokens.border_width, values.border_width),
        (&mut tokens.ring_width, values.ring_width),
        (&mut tokens.ring_alpha, values.ring_alpha),
        (&mut tokens.text_scale, values.text_scale),
    ];
    for (slot, value) in metrics {
        if let Some(value) = value {
            *slot = value;
        }
    }
    if let Some(value) = values.type_scale {
        tokens.type_scale = value;
    }
    if let Some(value) = values.shadows {
        tokens.shadows = value;
    }
    if let Some(value) = values.font_sans {
        tokens.font_sans = value;
    }
    if let Some(value) = values.font_mono {
        tokens.font_mono = value;
    }
    if let Some(value) = values.motion {
        tokens.motion = value;
    }
    if let Some(value) = values.contrast {
        tokens.contrast = value;
    }
    if let Some(value) = values.recipes {
        tokens.recipes = value;
    }
    tokens
}

fn apply_accessibility(
    mut tokens: DesignTokens,
    preferences: AccessibilityPreferences,
) -> DesignTokens {
    tokens.text_scale *= preferences.text_scale;
    if preferences.high_contrast {
        tokens.contrast = Contrast::High;
    }
    if tokens.contrast == Contrast::High {
        tokens.palette = contrast::raise(tokens.palette);
        tokens.ring_alpha = 1.0;
    }
    if preferences.reduced_motion {
        tokens.motion.reduced = true;
        tokens.motion.duration_ms = 0;
    }
    tokens
}

fn validate(tokens: DesignTokens) -> Result<(), ThemeError> {
    let values = [
        tokens.radius,
        tokens.spacing,
        tokens.border_width,
        tokens.ring_width,
    ];
    let easing = tokens.motion.easing;
    if tokens.text_scale <= 0.0
        || !tokens.text_scale.is_finite()
        || !(16.0 * tokens.text_scale * 2.6).is_finite()
        || !(0.0..=1.0).contains(&tokens.ring_alpha)
        || tokens.font_sans.name().is_empty()
        || tokens.font_mono.name().is_empty()
        || !easing.is_valid()
        || !tokens.type_scale.is_valid()
        || !tokens.shadows.is_valid()
        || values
            .into_iter()
            .any(|value| !value.is_finite() || value < 0.0)
    {
        Err(ThemeError::InvalidTokens)
    } else {
        Ok(())
    }
}

fn classify(old: DesignTokens, new: DesignTokens) -> ThemeInvalidation {
    if old == new {
        ThemeInvalidation::None
    } else if old.text_scale != new.text_scale
        || old.type_scale != new.type_scale
        || old.font_sans != new.font_sans
        || old.font_mono != new.font_mono
        || old.spacing != new.spacing
        || old.radius != new.radius
        || old.border_width != new.border_width
        || old.ring_width != new.ring_width
    {
        ThemeInvalidation::Layout
    } else {
        ThemeInvalidation::Paint
    }
}

fn invalidate(tree: &mut Tree, id: NodeId, change: ThemeInvalidation) -> Result<(), ThemeError> {
    match change {
        ThemeInvalidation::None => {}
        ThemeInvalidation::Paint => tree
            .invalidate(id, Dirty::PAINT)
            .map_err(|_| ThemeError::MissingNode)?,
        ThemeInvalidation::Layout => {
            tree.invalidate(id, Dirty::LAYOUT)
                .map_err(|_| ThemeError::MissingNode)?;
            tree.invalidate(id, Dirty::PAINT)
                .map_err(|_| ThemeError::MissingNode)?;
            tree.invalidate(id, Dirty::SEMANTICS)
                .map_err(|_| ThemeError::MissingNode)?;
        }
    }
    Ok(())
}
