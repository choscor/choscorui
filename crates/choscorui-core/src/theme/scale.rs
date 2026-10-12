//! Token scales: radius, spacing, type, tracking, shadows, fonts, and the
//! named text presets. Every value is in logical points (1 CSS px = 1 pt,
//! 1 rem = 16 pt).

use crate::{Color, ColorRole, FontSlant, FontWeight, Point, Shadow, TextStyle};

/// A step of the corner radius scale, relative to the base radius token.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RadiusScale {
    None,
    Sm,
    Md,
    Lg,
    Xl,
    Xl2,
    Xl3,
    Xl4,
    /// Half the shorter side: a pill or a circle.
    Full,
}

impl RadiusScale {
    /// The multiple of the base radius, or `None` for [`RadiusScale::Full`].
    pub const fn factor(self) -> Option<f64> {
        match self {
            Self::None => Some(0.0),
            Self::Sm => Some(0.6),
            Self::Md => Some(0.8),
            Self::Lg => Some(1.0),
            Self::Xl => Some(1.4),
            Self::Xl2 => Some(1.8),
            Self::Xl3 => Some(2.2),
            Self::Xl4 => Some(2.6),
            Self::Full => None,
        }
    }
}

/// A step of the spacing scale, in units of the spacing token.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Space {
    S0,
    S1,
    S2,
    S3,
    S4,
    S5,
    S6,
    S8,
    S10,
    S12,
    S16,
}

impl Space {
    /// The number of spacing units.
    pub const fn units(self) -> f64 {
        match self {
            Self::S0 => 0.0,
            Self::S1 => 1.0,
            Self::S2 => 2.0,
            Self::S3 => 3.0,
            Self::S4 => 4.0,
            Self::S5 => 5.0,
            Self::S6 => 6.0,
            Self::S8 => 8.0,
            Self::S10 => 10.0,
            Self::S12 => 12.0,
            Self::S16 => 16.0,
        }
    }
}

/// Large enough to round any control into a pill; painters clamp a radius
/// to half the shorter side.
pub const FULL_RADIUS: f64 = 9999.0;

/// A step of the type scale.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum TextSize {
    Xs,
    Sm,
    Base,
    Lg,
    Xl,
    Xl2,
    Xl3,
    Xl4,
}

impl TextSize {
    pub const ALL: [TextSize; 8] = [
        Self::Xs,
        Self::Sm,
        Self::Base,
        Self::Lg,
        Self::Xl,
        Self::Xl2,
        Self::Xl3,
        Self::Xl4,
    ];

    /// Font size and line height in points, before the text scale.
    pub const fn metrics(self) -> (f64, f64) {
        match self {
            Self::Xs => (12.0, 16.0),
            Self::Sm => (14.0, 20.0),
            Self::Base => (16.0, 24.0),
            Self::Lg => (18.0, 28.0),
            Self::Xl => (20.0, 28.0),
            Self::Xl2 => (24.0, 32.0),
            Self::Xl3 => (30.0, 36.0),
            Self::Xl4 => (36.0, 40.0),
        }
    }
}

/// Letter spacing as a fraction of the font size.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Tracking {
    Tighter,
    Tight,
    Normal,
    Wide,
    Wider,
    Widest,
}

impl Tracking {
    pub const fn em(self) -> f64 {
        match self {
            Self::Tighter => -0.05,
            Self::Tight => -0.025,
            Self::Normal => 0.0,
            Self::Wide => 0.025,
            Self::Wider => 0.05,
            Self::Widest => 0.1,
        }
    }
}

/// A font family token. The generic names select the platform UI faces,
/// which no application has to bundle.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct FontFamily(pub &'static str);

impl FontFamily {
    /// The platform user-interface font (SF Pro on macOS).
    pub const SYSTEM_UI: Self = Self("system-ui");
    /// The platform monospaced user-interface font (SF Mono on macOS).
    pub const SYSTEM_MONO: Self = Self("ui-monospace");

    pub const fn name(self) -> &'static str {
        self.0
    }
}

/// Up to three shadow layers, painted first to last.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ShadowLayers {
    layers: [Option<Shadow>; 3],
}

impl ShadowLayers {
    pub const NONE: Self = Self {
        layers: [None, None, None],
    };

    /// Keeps the first three layers of `layers`.
    pub fn new(layers: &[Shadow]) -> Self {
        let mut out = Self::NONE;
        for (slot, layer) in out.layers.iter_mut().zip(layers) {
            *slot = Some(*layer);
        }
        out
    }

    pub fn iter(&self) -> impl Iterator<Item = &Shadow> {
        self.layers.iter().flatten()
    }

    pub fn is_empty(&self) -> bool {
        self.layers[0].is_none()
    }

    /// Every layer with its alpha multiplied by `factor`.
    pub fn faded(mut self, factor: f64) -> Self {
        for layer in self.layers.iter_mut().flatten() {
            layer.color = layer.color.with_alpha(factor);
        }
        self
    }
}

/// A step of the elevation scale. Values are the reference drop and inset
/// shadows: offset, blur, spread, and black at a fixed alpha.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ShadowScale {
    None,
    Xs2,
    Xs,
    Sm,
    Md,
    Lg,
    Xl,
    Xl2,
    InsetXs2,
    InsetXs,
    InsetSm,
}

impl ShadowScale {
    pub const ALL: [ShadowScale; 11] = [
        Self::None,
        Self::Xs2,
        Self::Xs,
        Self::Sm,
        Self::Md,
        Self::Lg,
        Self::Xl,
        Self::Xl2,
        Self::InsetXs2,
        Self::InsetXs,
        Self::InsetSm,
    ];

    pub fn layers(self) -> ShadowLayers {
        let drop = |y: f64, blur: f64, spread: f64, alpha: f64| Shadow {
            offset: Point::new(0.0, y),
            blur,
            spread,
            color: Color::BLACK.with_alpha(alpha),
            inset: false,
        };
        let inset = |y: f64, blur: f64, alpha: f64| Shadow {
            inset: true,
            ..drop(y, blur, 0.0, alpha)
        };
        match self {
            Self::None => ShadowLayers::NONE,
            Self::Xs2 => ShadowLayers::new(&[drop(1.0, 0.0, 0.0, 0.05)]),
            Self::Xs => ShadowLayers::new(&[drop(1.0, 2.0, 0.0, 0.05)]),
            Self::Sm => ShadowLayers::new(&[drop(1.0, 3.0, 0.0, 0.1), drop(1.0, 2.0, -1.0, 0.1)]),
            Self::Md => ShadowLayers::new(&[drop(4.0, 6.0, -1.0, 0.1), drop(2.0, 4.0, -2.0, 0.1)]),
            Self::Lg => {
                ShadowLayers::new(&[drop(10.0, 15.0, -3.0, 0.1), drop(4.0, 6.0, -4.0, 0.1)])
            }
            Self::Xl => {
                ShadowLayers::new(&[drop(20.0, 25.0, -5.0, 0.1), drop(8.0, 10.0, -6.0, 0.1)])
            }
            Self::Xl2 => ShadowLayers::new(&[drop(25.0, 50.0, -12.0, 0.25)]),
            Self::InsetXs2 => ShadowLayers::new(&[inset(1.0, 0.0, 0.05)]),
            Self::InsetXs => ShadowLayers::new(&[inset(1.0, 1.0, 0.05)]),
            Self::InsetSm => ShadowLayers::new(&[inset(2.0, 4.0, 0.05)]),
        }
    }
}

/// A fully resolved text appearance: what a control needs to measure and
/// draw one run of text.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TypeSpec {
    pub family: FontFamily,
    pub size: f64,
    /// Absolute line height in points.
    pub line_height: f64,
    pub weight: FontWeight,
    pub slant: FontSlant,
    /// Letter spacing in points.
    pub tracking: f64,
}

impl TypeSpec {
    /// A core text style for shaping. The line height becomes a multiple
    /// of a natural line height of 1.2 × the font size.
    pub fn text_style(&self) -> TextStyle {
        TextStyle::new(self.family.name(), self.size)
            .with_weight(self.weight)
            .with_slant(self.slant)
            .with_line_height(self.line_height / (self.size * 1.2))
            .with_tracking(self.tracking)
    }

    pub fn with_weight(mut self, weight: FontWeight) -> Self {
        self.weight = weight;
        self
    }

    pub fn with_tracking(mut self, tracking: Tracking) -> Self {
        self.tracking = tracking.em() * self.size;
        self
    }

    /// The baseline offset from the top of a line box of this spec, for
    /// a typical UI face (ascent ≈ 0.95 em with 1.2 em natural height).
    pub fn baseline(&self) -> f64 {
        (self.line_height - self.size * 1.2) / 2.0 + self.size * 0.95
    }
}

/// The reference typography rules as named presets.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum TextPreset {
    H1,
    H2,
    H3,
    H4,
    Paragraph,
    Lead,
    Large,
    Small,
    Muted,
    Blockquote,
    List,
    InlineCode,
}

impl TextPreset {
    pub const ALL: [TextPreset; 12] = [
        Self::H1,
        Self::H2,
        Self::H3,
        Self::H4,
        Self::Paragraph,
        Self::Lead,
        Self::Large,
        Self::Small,
        Self::Muted,
        Self::Blockquote,
        Self::List,
        Self::InlineCode,
    ];

    /// Size step, line height override in points, weight, tracking, slant,
    /// monospace, and the color role of the text.
    pub(crate) const fn rule(self) -> PresetRule {
        let rule = PresetRule {
            size: TextSize::Base,
            line_height: None,
            weight: FontWeight::REGULAR,
            tracking: Tracking::Normal,
            slant: FontSlant::Upright,
            mono: false,
            color: ColorRole::Foreground,
        };
        match self {
            Self::H1 => PresetRule {
                size: TextSize::Xl4,
                weight: FontWeight::EXTRABOLD,
                tracking: Tracking::Tight,
                ..rule
            },
            Self::H2 => PresetRule {
                size: TextSize::Xl3,
                weight: FontWeight::SEMIBOLD,
                tracking: Tracking::Tight,
                ..rule
            },
            Self::H3 => PresetRule {
                size: TextSize::Xl2,
                weight: FontWeight::SEMIBOLD,
                tracking: Tracking::Tight,
                ..rule
            },
            Self::H4 => PresetRule {
                size: TextSize::Xl,
                weight: FontWeight::SEMIBOLD,
                tracking: Tracking::Tight,
                ..rule
            },
            Self::Paragraph | Self::List => PresetRule {
                line_height: Some(28.0),
                ..rule
            },
            Self::Blockquote => PresetRule {
                slant: FontSlant::Italic,
                ..rule
            },
            Self::Lead => PresetRule {
                size: TextSize::Xl,
                color: ColorRole::MutedForeground,
                ..rule
            },
            Self::Large => PresetRule {
                size: TextSize::Lg,
                weight: FontWeight::SEMIBOLD,
                ..rule
            },
            Self::Small => PresetRule {
                size: TextSize::Sm,
                line_height: Some(14.0),
                weight: FontWeight::MEDIUM,
                ..rule
            },
            Self::Muted => PresetRule {
                size: TextSize::Sm,
                color: ColorRole::MutedForeground,
                ..rule
            },
            Self::InlineCode => PresetRule {
                size: TextSize::Sm,
                weight: FontWeight::SEMIBOLD,
                mono: true,
                ..rule
            },
        }
    }

    /// The role the preset draws its text in.
    pub const fn color(self) -> ColorRole {
        self.rule().color
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct PresetRule {
    pub size: TextSize,
    pub line_height: Option<f64>,
    pub weight: FontWeight,
    pub tracking: Tracking,
    pub slant: FontSlant,
    pub mono: bool,
    pub color: ColorRole,
}
