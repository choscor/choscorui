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
//! Light/dark is an application theme choice; contrast is a token that the
//! user preference can only raise. Metric changes (type size, weight, line
//! height, spacing, radius, border width) invalidate layout; color,
//! elevation, and motion changes invalidate paint only.

use crate::{Color, Dirty, FontWeight, NodeId, Tree};
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MotionTokens {
    pub duration_ms: u32,
    pub reduced: bool,
}

/// Resolved design values. Geometry-affecting values are explicit so a theme
/// update can distinguish layout from paint work.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DesignTokens {
    pub background: Color,
    pub surface: Color,
    pub content: Color,
    pub accent: Color,
    pub danger: Color,
    pub font_size: f64,
    pub font_weight: FontWeight,
    /// Line height as a multiple of `font_size`.
    pub line_height: f64,
    pub spacing: f64,
    pub radius: f64,
    pub border_width: f64,
    pub elevation: f64,
    pub motion: MotionTokens,
    pub contrast: Contrast,
}

impl DesignTokens {
    /// Body text style for `family` built from the typography tokens.
    pub fn text_style(&self, family: &str) -> crate::TextStyle {
        crate::TextStyle::new(family, self.font_size)
            .with_weight(self.font_weight)
            .with_line_height(self.line_height)
    }

    pub const fn dark() -> Self {
        Self {
            background: Color::rgba8(24, 32, 48, 255),
            surface: Color::rgba8(40, 52, 72, 255),
            content: Color::rgba8(240, 243, 250, 255),
            accent: Color::rgba8(40, 140, 230, 255),
            danger: Color::rgba8(230, 70, 90, 255),
            font_size: 17.0,
            font_weight: FontWeight::REGULAR,
            line_height: 1.0,
            spacing: 8.0,
            radius: 4.0,
            border_width: 2.0,
            elevation: 1.0,
            motion: MotionTokens {
                duration_ms: 160,
                reduced: false,
            },
            contrast: Contrast::Normal,
        }
    }

    pub const fn light() -> Self {
        Self {
            background: Color::rgba8(244, 246, 250, 255),
            surface: Color::rgba8(255, 255, 255, 255),
            content: Color::rgba8(25, 31, 42, 255),
            accent: Color::rgba8(120, 55, 190, 255),
            danger: Color::rgba8(190, 35, 55, 255),
            font_size: 17.0,
            font_weight: FontWeight::REGULAR,
            line_height: 1.2,
            spacing: 8.0,
            radius: 10.0,
            border_width: 1.0,
            elevation: 3.0,
            motion: MotionTokens {
                duration_ms: 220,
                reduced: false,
            },
            contrast: Contrast::Normal,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ThemeOverride {
    pub background: Option<Color>,
    pub surface: Option<Color>,
    pub content: Option<Color>,
    pub accent: Option<Color>,
    pub danger: Option<Color>,
    pub font_size: Option<f64>,
    pub font_weight: Option<FontWeight>,
    pub line_height: Option<f64>,
    pub spacing: Option<f64>,
    pub radius: Option<f64>,
    pub border_width: Option<f64>,
    pub elevation: Option<f64>,
    pub motion: Option<MotionTokens>,
    pub contrast: Option<Contrast>,
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
    if let Some(value) = values.background {
        tokens.background = value;
    }
    if let Some(value) = values.surface {
        tokens.surface = value;
    }
    if let Some(value) = values.content {
        tokens.content = value;
    }
    if let Some(value) = values.accent {
        tokens.accent = value;
    }
    if let Some(value) = values.danger {
        tokens.danger = value;
    }
    if let Some(value) = values.font_size {
        tokens.font_size = value;
    }
    if let Some(value) = values.font_weight {
        tokens.font_weight = value;
    }
    if let Some(value) = values.line_height {
        tokens.line_height = value;
    }
    if let Some(value) = values.spacing {
        tokens.spacing = value;
    }
    if let Some(value) = values.radius {
        tokens.radius = value;
    }
    if let Some(value) = values.border_width {
        tokens.border_width = value;
    }
    if let Some(value) = values.elevation {
        tokens.elevation = value;
    }
    if let Some(value) = values.motion {
        tokens.motion = value;
    }
    if let Some(value) = values.contrast {
        tokens.contrast = value;
    }
    tokens
}

fn apply_accessibility(
    mut tokens: DesignTokens,
    preferences: AccessibilityPreferences,
) -> DesignTokens {
    tokens.font_size *= preferences.text_scale;
    if preferences.high_contrast {
        tokens.contrast = Contrast::High;
    }
    if preferences.reduced_motion {
        tokens.motion.reduced = true;
        tokens.motion.duration_ms = 0;
    }
    tokens
}

fn validate(tokens: DesignTokens) -> Result<(), ThemeError> {
    let values = [
        tokens.font_size,
        tokens.spacing,
        tokens.radius,
        tokens.border_width,
        tokens.elevation,
    ];
    if tokens.font_size <= 0.0
        || tokens.line_height <= 0.0
        || !tokens.line_height.is_finite()
        || !(1..=1000).contains(&tokens.font_weight.0)
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
    } else if old.font_size != new.font_size
        || old.font_weight != new.font_weight
        || old.line_height != new.line_height
        || old.spacing != new.spacing
        || old.radius != new.radius
        || old.border_width != new.border_width
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
