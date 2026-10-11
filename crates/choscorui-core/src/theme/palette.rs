//! Color roles, base palettes, and color schemes. The OKLCH tables are the
//! source of truth; sRGB values are derived when a palette is built.

use crate::Color;

/// A semantic color slot. Controls ask for a role, never a raw color, so
/// one control follows every base color and scheme.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ColorRole {
    Background,
    Foreground,
    Card,
    CardForeground,
    Popover,
    PopoverForeground,
    Primary,
    PrimaryForeground,
    Secondary,
    SecondaryForeground,
    Muted,
    MutedForeground,
    Accent,
    AccentForeground,
    Destructive,
    Border,
    Input,
    Ring,
    Chart1,
    Chart2,
    Chart3,
    Chart4,
    Chart5,
    Sidebar,
    SidebarForeground,
    SidebarPrimary,
    SidebarPrimaryForeground,
    SidebarAccent,
    SidebarAccentForeground,
    SidebarBorder,
    SidebarRing,
}

impl ColorRole {
    pub const COUNT: usize = 31;

    pub const ALL: [ColorRole; Self::COUNT] = [
        Self::Background,
        Self::Foreground,
        Self::Card,
        Self::CardForeground,
        Self::Popover,
        Self::PopoverForeground,
        Self::Primary,
        Self::PrimaryForeground,
        Self::Secondary,
        Self::SecondaryForeground,
        Self::Muted,
        Self::MutedForeground,
        Self::Accent,
        Self::AccentForeground,
        Self::Destructive,
        Self::Border,
        Self::Input,
        Self::Ring,
        Self::Chart1,
        Self::Chart2,
        Self::Chart3,
        Self::Chart4,
        Self::Chart5,
        Self::Sidebar,
        Self::SidebarForeground,
        Self::SidebarPrimary,
        Self::SidebarPrimaryForeground,
        Self::SidebarAccent,
        Self::SidebarAccentForeground,
        Self::SidebarBorder,
        Self::SidebarRing,
    ];

    /// Stable kebab-case name, used by golden files and settings.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Background => "background",
            Self::Foreground => "foreground",
            Self::Card => "card",
            Self::CardForeground => "card-foreground",
            Self::Popover => "popover",
            Self::PopoverForeground => "popover-foreground",
            Self::Primary => "primary",
            Self::PrimaryForeground => "primary-foreground",
            Self::Secondary => "secondary",
            Self::SecondaryForeground => "secondary-foreground",
            Self::Muted => "muted",
            Self::MutedForeground => "muted-foreground",
            Self::Accent => "accent",
            Self::AccentForeground => "accent-foreground",
            Self::Destructive => "destructive",
            Self::Border => "border",
            Self::Input => "input",
            Self::Ring => "ring",
            Self::Chart1 => "chart-1",
            Self::Chart2 => "chart-2",
            Self::Chart3 => "chart-3",
            Self::Chart4 => "chart-4",
            Self::Chart5 => "chart-5",
            Self::Sidebar => "sidebar",
            Self::SidebarForeground => "sidebar-foreground",
            Self::SidebarPrimary => "sidebar-primary",
            Self::SidebarPrimaryForeground => "sidebar-primary-foreground",
            Self::SidebarAccent => "sidebar-accent",
            Self::SidebarAccentForeground => "sidebar-accent-foreground",
            Self::SidebarBorder => "sidebar-border",
            Self::SidebarRing => "sidebar-ring",
        }
    }
}

/// The gray family a palette is built from.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum BaseColor {
    #[default]
    Neutral,
    Stone,
    Zinc,
    Mauve,
    Olive,
    Mist,
    Taupe,
}

impl BaseColor {
    pub const ALL: [BaseColor; 7] = [
        Self::Neutral,
        Self::Stone,
        Self::Zinc,
        Self::Mauve,
        Self::Olive,
        Self::Mist,
        Self::Taupe,
    ];

    /// Stable lowercase id, used by settings and translation keys.
    pub const fn id(self) -> &'static str {
        match self {
            Self::Neutral => "neutral",
            Self::Stone => "stone",
            Self::Zinc => "zinc",
            Self::Mauve => "mauve",
            Self::Olive => "olive",
            Self::Mist => "mist",
            Self::Taupe => "taupe",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|base| base.id() == id)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum ColorScheme {
    #[default]
    Light,
    Dark,
}

/// One OKLCH source value: lightness, chroma, hue in degrees, and alpha.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Oklch {
    pub lightness: f64,
    pub chroma: f64,
    pub hue: f64,
    pub alpha: f64,
}

impl Oklch {
    pub fn to_color(self) -> Color {
        Color::from_oklch(self.lightness, self.chroma, self.hue, self.alpha)
    }
}

/// A resolved color for every [`ColorRole`].
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Palette([Color; ColorRole::COUNT]);

impl Palette {
    /// The built-in palette of `base` in `scheme`.
    pub fn new(base: BaseColor, scheme: ColorScheme) -> Self {
        let table = source(base, scheme);
        Self(std::array::from_fn(|index| table[index].to_color()))
    }

    /// Every color equal to `color`; a starting point for custom palettes.
    pub const fn filled(color: Color) -> Self {
        Self([color; ColorRole::COUNT])
    }

    pub const fn get(&self, role: ColorRole) -> Color {
        self.0[role as usize]
    }

    pub fn set(&mut self, role: ColorRole, color: Color) {
        self.0[role as usize] = color;
    }

    pub fn with(mut self, role: ColorRole, color: Color) -> Self {
        self.set(role, color);
        self
    }
}

impl std::ops::Index<ColorRole> for Palette {
    type Output = Color;
    fn index(&self, role: ColorRole) -> &Color {
        &self.0[role as usize]
    }
}

/// The OKLCH source table of `base` in `scheme`, in [`ColorRole::ALL`] order.
pub fn source(base: BaseColor, scheme: ColorScheme) -> &'static [Oklch; ColorRole::COUNT] {
    match (base, scheme) {
        (BaseColor::Neutral, ColorScheme::Light) => &NEUTRAL_LIGHT,
        (BaseColor::Neutral, ColorScheme::Dark) => &NEUTRAL_DARK,
        (BaseColor::Stone, ColorScheme::Light) => &STONE_LIGHT,
        (BaseColor::Stone, ColorScheme::Dark) => &STONE_DARK,
        (BaseColor::Zinc, ColorScheme::Light) => &ZINC_LIGHT,
        (BaseColor::Zinc, ColorScheme::Dark) => &ZINC_DARK,
        (BaseColor::Mauve, ColorScheme::Light) => &MAUVE_LIGHT,
        (BaseColor::Mauve, ColorScheme::Dark) => &MAUVE_DARK,
        (BaseColor::Olive, ColorScheme::Light) => &OLIVE_LIGHT,
        (BaseColor::Olive, ColorScheme::Dark) => &OLIVE_DARK,
        (BaseColor::Mist, ColorScheme::Light) => &MIST_LIGHT,
        (BaseColor::Mist, ColorScheme::Dark) => &MIST_DARK,
        (BaseColor::Taupe, ColorScheme::Light) => &TAUPE_LIGHT,
        (BaseColor::Taupe, ColorScheme::Dark) => &TAUPE_DARK,
    }
}

type Table = [Oklch; ColorRole::COUNT];

const fn ok(lightness: f64, chroma: f64, hue: f64, alpha: f64) -> Oklch {
    Oklch {
        lightness,
        chroma,
        hue,
        alpha,
    }
}

const NEUTRAL_LIGHT: Table = [
    ok(1.0, 0.0, 0.0, 1.0),        // background
    ok(0.145, 0.0, 0.0, 1.0),      // foreground
    ok(1.0, 0.0, 0.0, 1.0),        // card
    ok(0.145, 0.0, 0.0, 1.0),      // card-foreground
    ok(1.0, 0.0, 0.0, 1.0),        // popover
    ok(0.145, 0.0, 0.0, 1.0),      // popover-foreground
    ok(0.205, 0.0, 0.0, 1.0),      // primary
    ok(0.985, 0.0, 0.0, 1.0),      // primary-foreground
    ok(0.97, 0.0, 0.0, 1.0),       // secondary
    ok(0.205, 0.0, 0.0, 1.0),      // secondary-foreground
    ok(0.97, 0.0, 0.0, 1.0),       // muted
    ok(0.556, 0.0, 0.0, 1.0),      // muted-foreground
    ok(0.97, 0.0, 0.0, 1.0),       // accent
    ok(0.205, 0.0, 0.0, 1.0),      // accent-foreground
    ok(0.577, 0.245, 27.325, 1.0), // destructive
    ok(0.922, 0.0, 0.0, 1.0),      // border
    ok(0.922, 0.0, 0.0, 1.0),      // input
    ok(0.708, 0.0, 0.0, 1.0),      // ring
    ok(0.87, 0.0, 0.0, 1.0),       // chart-1
    ok(0.556, 0.0, 0.0, 1.0),      // chart-2
    ok(0.439, 0.0, 0.0, 1.0),      // chart-3
    ok(0.371, 0.0, 0.0, 1.0),      // chart-4
    ok(0.269, 0.0, 0.0, 1.0),      // chart-5
    ok(0.985, 0.0, 0.0, 1.0),      // sidebar
    ok(0.145, 0.0, 0.0, 1.0),      // sidebar-foreground
    ok(0.205, 0.0, 0.0, 1.0),      // sidebar-primary
    ok(0.985, 0.0, 0.0, 1.0),      // sidebar-primary-foreground
    ok(0.97, 0.0, 0.0, 1.0),       // sidebar-accent
    ok(0.205, 0.0, 0.0, 1.0),      // sidebar-accent-foreground
    ok(0.922, 0.0, 0.0, 1.0),      // sidebar-border
    ok(0.708, 0.0, 0.0, 1.0),      // sidebar-ring
];

const NEUTRAL_DARK: Table = [
    ok(0.145, 0.0, 0.0, 1.0),       // background
    ok(0.985, 0.0, 0.0, 1.0),       // foreground
    ok(0.205, 0.0, 0.0, 1.0),       // card
    ok(0.985, 0.0, 0.0, 1.0),       // card-foreground
    ok(0.205, 0.0, 0.0, 1.0),       // popover
    ok(0.985, 0.0, 0.0, 1.0),       // popover-foreground
    ok(0.922, 0.0, 0.0, 1.0),       // primary
    ok(0.205, 0.0, 0.0, 1.0),       // primary-foreground
    ok(0.269, 0.0, 0.0, 1.0),       // secondary
    ok(0.985, 0.0, 0.0, 1.0),       // secondary-foreground
    ok(0.269, 0.0, 0.0, 1.0),       // muted
    ok(0.708, 0.0, 0.0, 1.0),       // muted-foreground
    ok(0.269, 0.0, 0.0, 1.0),       // accent
    ok(0.985, 0.0, 0.0, 1.0),       // accent-foreground
    ok(0.704, 0.191, 22.216, 1.0),  // destructive
    ok(1.0, 0.0, 0.0, 0.1),         // border
    ok(1.0, 0.0, 0.0, 0.15),        // input
    ok(0.556, 0.0, 0.0, 1.0),       // ring
    ok(0.87, 0.0, 0.0, 1.0),        // chart-1
    ok(0.556, 0.0, 0.0, 1.0),       // chart-2
    ok(0.439, 0.0, 0.0, 1.0),       // chart-3
    ok(0.371, 0.0, 0.0, 1.0),       // chart-4
    ok(0.269, 0.0, 0.0, 1.0),       // chart-5
    ok(0.205, 0.0, 0.0, 1.0),       // sidebar
    ok(0.985, 0.0, 0.0, 1.0),       // sidebar-foreground
    ok(0.488, 0.243, 264.376, 1.0), // sidebar-primary
    ok(0.985, 0.0, 0.0, 1.0),       // sidebar-primary-foreground
    ok(0.269, 0.0, 0.0, 1.0),       // sidebar-accent
    ok(0.985, 0.0, 0.0, 1.0),       // sidebar-accent-foreground
    ok(1.0, 0.0, 0.0, 0.1),         // sidebar-border
    ok(0.556, 0.0, 0.0, 1.0),       // sidebar-ring
];

const STONE_LIGHT: Table = [
    ok(1.0, 0.0, 0.0, 1.0),         // background
    ok(0.147, 0.004, 49.25, 1.0),   // foreground
    ok(1.0, 0.0, 0.0, 1.0),         // card
    ok(0.147, 0.004, 49.25, 1.0),   // card-foreground
    ok(1.0, 0.0, 0.0, 1.0),         // popover
    ok(0.147, 0.004, 49.25, 1.0),   // popover-foreground
    ok(0.216, 0.006, 56.043, 1.0),  // primary
    ok(0.985, 0.001, 106.423, 1.0), // primary-foreground
    ok(0.97, 0.001, 106.424, 1.0),  // secondary
    ok(0.216, 0.006, 56.043, 1.0),  // secondary-foreground
    ok(0.97, 0.001, 106.424, 1.0),  // muted
    ok(0.553, 0.013, 58.071, 1.0),  // muted-foreground
    ok(0.97, 0.001, 106.424, 1.0),  // accent
    ok(0.216, 0.006, 56.043, 1.0),  // accent-foreground
    ok(0.577, 0.245, 27.325, 1.0),  // destructive
    ok(0.923, 0.003, 48.717, 1.0),  // border
    ok(0.923, 0.003, 48.717, 1.0),  // input
    ok(0.709, 0.01, 56.259, 1.0),   // ring
    ok(0.869, 0.005, 56.366, 1.0),  // chart-1
    ok(0.553, 0.013, 58.071, 1.0),  // chart-2
    ok(0.444, 0.011, 73.639, 1.0),  // chart-3
    ok(0.374, 0.01, 67.558, 1.0),   // chart-4
    ok(0.268, 0.007, 34.298, 1.0),  // chart-5
    ok(0.985, 0.001, 106.423, 1.0), // sidebar
    ok(0.147, 0.004, 49.25, 1.0),   // sidebar-foreground
    ok(0.216, 0.006, 56.043, 1.0),  // sidebar-primary
    ok(0.985, 0.001, 106.423, 1.0), // sidebar-primary-foreground
    ok(0.97, 0.001, 106.424, 1.0),  // sidebar-accent
    ok(0.216, 0.006, 56.043, 1.0),  // sidebar-accent-foreground
    ok(0.923, 0.003, 48.717, 1.0),  // sidebar-border
    ok(0.709, 0.01, 56.259, 1.0),   // sidebar-ring
];

const STONE_DARK: Table = [
    ok(0.147, 0.004, 49.25, 1.0),   // background
    ok(0.985, 0.001, 106.423, 1.0), // foreground
    ok(0.216, 0.006, 56.043, 1.0),  // card
    ok(0.985, 0.001, 106.423, 1.0), // card-foreground
    ok(0.216, 0.006, 56.043, 1.0),  // popover
    ok(0.985, 0.001, 106.423, 1.0), // popover-foreground
    ok(0.923, 0.003, 48.717, 1.0),  // primary
    ok(0.216, 0.006, 56.043, 1.0),  // primary-foreground
    ok(0.268, 0.007, 34.298, 1.0),  // secondary
    ok(0.985, 0.001, 106.423, 1.0), // secondary-foreground
    ok(0.268, 0.007, 34.298, 1.0),  // muted
    ok(0.709, 0.01, 56.259, 1.0),   // muted-foreground
    ok(0.268, 0.007, 34.298, 1.0),  // accent
    ok(0.985, 0.001, 106.423, 1.0), // accent-foreground
    ok(0.704, 0.191, 22.216, 1.0),  // destructive
    ok(1.0, 0.0, 0.0, 0.1),         // border
    ok(1.0, 0.0, 0.0, 0.15),        // input
    ok(0.553, 0.013, 58.071, 1.0),  // ring
    ok(0.869, 0.005, 56.366, 1.0),  // chart-1
    ok(0.553, 0.013, 58.071, 1.0),  // chart-2
    ok(0.444, 0.011, 73.639, 1.0),  // chart-3
    ok(0.374, 0.01, 67.558, 1.0),   // chart-4
    ok(0.268, 0.007, 34.298, 1.0),  // chart-5
    ok(0.216, 0.006, 56.043, 1.0),  // sidebar
    ok(0.985, 0.001, 106.423, 1.0), // sidebar-foreground
    ok(0.488, 0.243, 264.376, 1.0), // sidebar-primary
    ok(0.985, 0.001, 106.423, 1.0), // sidebar-primary-foreground
    ok(0.268, 0.007, 34.298, 1.0),  // sidebar-accent
    ok(0.985, 0.001, 106.423, 1.0), // sidebar-accent-foreground
    ok(1.0, 0.0, 0.0, 0.1),         // sidebar-border
    ok(0.553, 0.013, 58.071, 1.0),  // sidebar-ring
];

const ZINC_LIGHT: Table = [
    ok(1.0, 0.0, 0.0, 1.0),         // background
    ok(0.141, 0.005, 285.823, 1.0), // foreground
    ok(1.0, 0.0, 0.0, 1.0),         // card
    ok(0.141, 0.005, 285.823, 1.0), // card-foreground
    ok(1.0, 0.0, 0.0, 1.0),         // popover
    ok(0.141, 0.005, 285.823, 1.0), // popover-foreground
    ok(0.21, 0.006, 285.885, 1.0),  // primary
    ok(0.985, 0.0, 0.0, 1.0),       // primary-foreground
    ok(0.967, 0.001, 286.375, 1.0), // secondary
    ok(0.21, 0.006, 285.885, 1.0),  // secondary-foreground
    ok(0.967, 0.001, 286.375, 1.0), // muted
    ok(0.552, 0.016, 285.938, 1.0), // muted-foreground
    ok(0.967, 0.001, 286.375, 1.0), // accent
    ok(0.21, 0.006, 285.885, 1.0),  // accent-foreground
    ok(0.577, 0.245, 27.325, 1.0),  // destructive
    ok(0.92, 0.004, 286.32, 1.0),   // border
    ok(0.92, 0.004, 286.32, 1.0),   // input
    ok(0.705, 0.015, 286.067, 1.0), // ring
    ok(0.871, 0.006, 286.286, 1.0), // chart-1
    ok(0.552, 0.016, 285.938, 1.0), // chart-2
    ok(0.442, 0.017, 285.786, 1.0), // chart-3
    ok(0.37, 0.013, 285.805, 1.0),  // chart-4
    ok(0.274, 0.006, 286.033, 1.0), // chart-5
    ok(0.985, 0.0, 0.0, 1.0),       // sidebar
    ok(0.141, 0.005, 285.823, 1.0), // sidebar-foreground
    ok(0.21, 0.006, 285.885, 1.0),  // sidebar-primary
    ok(0.985, 0.0, 0.0, 1.0),       // sidebar-primary-foreground
    ok(0.967, 0.001, 286.375, 1.0), // sidebar-accent
    ok(0.21, 0.006, 285.885, 1.0),  // sidebar-accent-foreground
    ok(0.92, 0.004, 286.32, 1.0),   // sidebar-border
    ok(0.705, 0.015, 286.067, 1.0), // sidebar-ring
];

const ZINC_DARK: Table = [
    ok(0.141, 0.005, 285.823, 1.0), // background
    ok(0.985, 0.0, 0.0, 1.0),       // foreground
    ok(0.21, 0.006, 285.885, 1.0),  // card
    ok(0.985, 0.0, 0.0, 1.0),       // card-foreground
    ok(0.21, 0.006, 285.885, 1.0),  // popover
    ok(0.985, 0.0, 0.0, 1.0),       // popover-foreground
    ok(0.92, 0.004, 286.32, 1.0),   // primary
    ok(0.21, 0.006, 285.885, 1.0),  // primary-foreground
    ok(0.274, 0.006, 286.033, 1.0), // secondary
    ok(0.985, 0.0, 0.0, 1.0),       // secondary-foreground
    ok(0.274, 0.006, 286.033, 1.0), // muted
    ok(0.705, 0.015, 286.067, 1.0), // muted-foreground
    ok(0.274, 0.006, 286.033, 1.0), // accent
    ok(0.985, 0.0, 0.0, 1.0),       // accent-foreground
    ok(0.704, 0.191, 22.216, 1.0),  // destructive
    ok(1.0, 0.0, 0.0, 0.1),         // border
    ok(1.0, 0.0, 0.0, 0.15),        // input
    ok(0.552, 0.016, 285.938, 1.0), // ring
    ok(0.871, 0.006, 286.286, 1.0), // chart-1
    ok(0.552, 0.016, 285.938, 1.0), // chart-2
    ok(0.442, 0.017, 285.786, 1.0), // chart-3
    ok(0.37, 0.013, 285.805, 1.0),  // chart-4
    ok(0.274, 0.006, 286.033, 1.0), // chart-5
    ok(0.21, 0.006, 285.885, 1.0),  // sidebar
    ok(0.985, 0.0, 0.0, 1.0),       // sidebar-foreground
    ok(0.488, 0.243, 264.376, 1.0), // sidebar-primary
    ok(0.985, 0.0, 0.0, 1.0),       // sidebar-primary-foreground
    ok(0.274, 0.006, 286.033, 1.0), // sidebar-accent
    ok(0.985, 0.0, 0.0, 1.0),       // sidebar-accent-foreground
    ok(1.0, 0.0, 0.0, 0.1),         // sidebar-border
    ok(0.552, 0.016, 285.938, 1.0), // sidebar-ring
];

const MAUVE_LIGHT: Table = [
    ok(1.0, 0.0, 0.0, 1.0),        // background
    ok(0.145, 0.008, 326.0, 1.0),  // foreground
    ok(1.0, 0.0, 0.0, 1.0),        // card
    ok(0.145, 0.008, 326.0, 1.0),  // card-foreground
    ok(1.0, 0.0, 0.0, 1.0),        // popover
    ok(0.145, 0.008, 326.0, 1.0),  // popover-foreground
    ok(0.212, 0.019, 322.12, 1.0), // primary
    ok(0.985, 0.0, 0.0, 1.0),      // primary-foreground
    ok(0.96, 0.003, 325.6, 1.0),   // secondary
    ok(0.212, 0.019, 322.12, 1.0), // secondary-foreground
    ok(0.96, 0.003, 325.6, 1.0),   // muted
    ok(0.542, 0.034, 322.5, 1.0),  // muted-foreground
    ok(0.96, 0.003, 325.6, 1.0),   // accent
    ok(0.212, 0.019, 322.12, 1.0), // accent-foreground
    ok(0.577, 0.245, 27.325, 1.0), // destructive
    ok(0.922, 0.005, 325.62, 1.0), // border
    ok(0.922, 0.005, 325.62, 1.0), // input
    ok(0.711, 0.019, 323.02, 1.0), // ring
    ok(0.865, 0.012, 325.68, 1.0), // chart-1
    ok(0.542, 0.034, 322.5, 1.0),  // chart-2
    ok(0.435, 0.029, 321.78, 1.0), // chart-3
    ok(0.364, 0.029, 323.89, 1.0), // chart-4
    ok(0.263, 0.024, 320.12, 1.0), // chart-5
    ok(0.985, 0.0, 0.0, 1.0),      // sidebar
    ok(0.145, 0.008, 326.0, 1.0),  // sidebar-foreground
    ok(0.212, 0.019, 322.12, 1.0), // sidebar-primary
    ok(0.985, 0.0, 0.0, 1.0),      // sidebar-primary-foreground
    ok(0.96, 0.003, 325.6, 1.0),   // sidebar-accent
    ok(0.212, 0.019, 322.12, 1.0), // sidebar-accent-foreground
    ok(0.922, 0.005, 325.62, 1.0), // sidebar-border
    ok(0.711, 0.019, 323.02, 1.0), // sidebar-ring
];

const MAUVE_DARK: Table = [
    ok(0.145, 0.008, 326.0, 1.0),   // background
    ok(0.985, 0.0, 0.0, 1.0),       // foreground
    ok(0.212, 0.019, 322.12, 1.0),  // card
    ok(0.985, 0.0, 0.0, 1.0),       // card-foreground
    ok(0.212, 0.019, 322.12, 1.0),  // popover
    ok(0.985, 0.0, 0.0, 1.0),       // popover-foreground
    ok(0.922, 0.005, 325.62, 1.0),  // primary
    ok(0.212, 0.019, 322.12, 1.0),  // primary-foreground
    ok(0.263, 0.024, 320.12, 1.0),  // secondary
    ok(0.985, 0.0, 0.0, 1.0),       // secondary-foreground
    ok(0.263, 0.024, 320.12, 1.0),  // muted
    ok(0.711, 0.019, 323.02, 1.0),  // muted-foreground
    ok(0.263, 0.024, 320.12, 1.0),  // accent
    ok(0.985, 0.0, 0.0, 1.0),       // accent-foreground
    ok(0.704, 0.191, 22.216, 1.0),  // destructive
    ok(1.0, 0.0, 0.0, 0.1),         // border
    ok(1.0, 0.0, 0.0, 0.15),        // input
    ok(0.542, 0.034, 322.5, 1.0),   // ring
    ok(0.865, 0.012, 325.68, 1.0),  // chart-1
    ok(0.542, 0.034, 322.5, 1.0),   // chart-2
    ok(0.435, 0.029, 321.78, 1.0),  // chart-3
    ok(0.364, 0.029, 323.89, 1.0),  // chart-4
    ok(0.263, 0.024, 320.12, 1.0),  // chart-5
    ok(0.212, 0.019, 322.12, 1.0),  // sidebar
    ok(0.985, 0.0, 0.0, 1.0),       // sidebar-foreground
    ok(0.488, 0.243, 264.376, 1.0), // sidebar-primary
    ok(0.985, 0.0, 0.0, 1.0),       // sidebar-primary-foreground
    ok(0.263, 0.024, 320.12, 1.0),  // sidebar-accent
    ok(0.985, 0.0, 0.0, 1.0),       // sidebar-accent-foreground
    ok(1.0, 0.0, 0.0, 0.1),         // sidebar-border
    ok(0.542, 0.034, 322.5, 1.0),   // sidebar-ring
];

const OLIVE_LIGHT: Table = [
    ok(1.0, 0.0, 0.0, 1.0),        // background
    ok(0.153, 0.006, 107.1, 1.0),  // foreground
    ok(1.0, 0.0, 0.0, 1.0),        // card
    ok(0.153, 0.006, 107.1, 1.0),  // card-foreground
    ok(1.0, 0.0, 0.0, 1.0),        // popover
    ok(0.153, 0.006, 107.1, 1.0),  // popover-foreground
    ok(0.228, 0.013, 107.4, 1.0),  // primary
    ok(0.988, 0.003, 106.5, 1.0),  // primary-foreground
    ok(0.966, 0.005, 106.5, 1.0),  // secondary
    ok(0.228, 0.013, 107.4, 1.0),  // secondary-foreground
    ok(0.966, 0.005, 106.5, 1.0),  // muted
    ok(0.58, 0.031, 107.3, 1.0),   // muted-foreground
    ok(0.966, 0.005, 106.5, 1.0),  // accent
    ok(0.228, 0.013, 107.4, 1.0),  // accent-foreground
    ok(0.577, 0.245, 27.325, 1.0), // destructive
    ok(0.93, 0.007, 106.5, 1.0),   // border
    ok(0.93, 0.007, 106.5, 1.0),   // input
    ok(0.737, 0.021, 106.9, 1.0),  // ring
    ok(0.88, 0.011, 106.6, 1.0),   // chart-1
    ok(0.58, 0.031, 107.3, 1.0),   // chart-2
    ok(0.466, 0.025, 107.3, 1.0),  // chart-3
    ok(0.394, 0.023, 107.4, 1.0),  // chart-4
    ok(0.286, 0.016, 107.4, 1.0),  // chart-5
    ok(0.988, 0.003, 106.5, 1.0),  // sidebar
    ok(0.153, 0.006, 107.1, 1.0),  // sidebar-foreground
    ok(0.228, 0.013, 107.4, 1.0),  // sidebar-primary
    ok(0.988, 0.003, 106.5, 1.0),  // sidebar-primary-foreground
    ok(0.966, 0.005, 106.5, 1.0),  // sidebar-accent
    ok(0.228, 0.013, 107.4, 1.0),  // sidebar-accent-foreground
    ok(0.93, 0.007, 106.5, 1.0),   // sidebar-border
    ok(0.737, 0.021, 106.9, 1.0),  // sidebar-ring
];

const OLIVE_DARK: Table = [
    ok(0.153, 0.006, 107.1, 1.0),   // background
    ok(0.988, 0.003, 106.5, 1.0),   // foreground
    ok(0.228, 0.013, 107.4, 1.0),   // card
    ok(0.988, 0.003, 106.5, 1.0),   // card-foreground
    ok(0.228, 0.013, 107.4, 1.0),   // popover
    ok(0.988, 0.003, 106.5, 1.0),   // popover-foreground
    ok(0.93, 0.007, 106.5, 1.0),    // primary
    ok(0.228, 0.013, 107.4, 1.0),   // primary-foreground
    ok(0.286, 0.016, 107.4, 1.0),   // secondary
    ok(0.988, 0.003, 106.5, 1.0),   // secondary-foreground
    ok(0.286, 0.016, 107.4, 1.0),   // muted
    ok(0.737, 0.021, 106.9, 1.0),   // muted-foreground
    ok(0.286, 0.016, 107.4, 1.0),   // accent
    ok(0.988, 0.003, 106.5, 1.0),   // accent-foreground
    ok(0.704, 0.191, 22.216, 1.0),  // destructive
    ok(1.0, 0.0, 0.0, 0.1),         // border
    ok(1.0, 0.0, 0.0, 0.15),        // input
    ok(0.58, 0.031, 107.3, 1.0),    // ring
    ok(0.88, 0.011, 106.6, 1.0),    // chart-1
    ok(0.58, 0.031, 107.3, 1.0),    // chart-2
    ok(0.466, 0.025, 107.3, 1.0),   // chart-3
    ok(0.394, 0.023, 107.4, 1.0),   // chart-4
    ok(0.286, 0.016, 107.4, 1.0),   // chart-5
    ok(0.228, 0.013, 107.4, 1.0),   // sidebar
    ok(0.988, 0.003, 106.5, 1.0),   // sidebar-foreground
    ok(0.488, 0.243, 264.376, 1.0), // sidebar-primary
    ok(0.988, 0.003, 106.5, 1.0),   // sidebar-primary-foreground
    ok(0.286, 0.016, 107.4, 1.0),   // sidebar-accent
    ok(0.988, 0.003, 106.5, 1.0),   // sidebar-accent-foreground
    ok(1.0, 0.0, 0.0, 0.1),         // sidebar-border
    ok(0.58, 0.031, 107.3, 1.0),    // sidebar-ring
];

const MIST_LIGHT: Table = [
    ok(1.0, 0.0, 0.0, 1.0),        // background
    ok(0.148, 0.004, 228.8, 1.0),  // foreground
    ok(1.0, 0.0, 0.0, 1.0),        // card
    ok(0.148, 0.004, 228.8, 1.0),  // card-foreground
    ok(1.0, 0.0, 0.0, 1.0),        // popover
    ok(0.148, 0.004, 228.8, 1.0),  // popover-foreground
    ok(0.218, 0.008, 223.9, 1.0),  // primary
    ok(0.987, 0.002, 197.1, 1.0),  // primary-foreground
    ok(0.963, 0.002, 197.1, 1.0),  // secondary
    ok(0.218, 0.008, 223.9, 1.0),  // secondary-foreground
    ok(0.963, 0.002, 197.1, 1.0),  // muted
    ok(0.56, 0.021, 213.5, 1.0),   // muted-foreground
    ok(0.963, 0.002, 197.1, 1.0),  // accent
    ok(0.218, 0.008, 223.9, 1.0),  // accent-foreground
    ok(0.577, 0.245, 27.325, 1.0), // destructive
    ok(0.925, 0.005, 214.3, 1.0),  // border
    ok(0.925, 0.005, 214.3, 1.0),  // input
    ok(0.723, 0.014, 214.4, 1.0),  // ring
    ok(0.872, 0.007, 219.6, 1.0),  // chart-1
    ok(0.56, 0.021, 213.5, 1.0),   // chart-2
    ok(0.45, 0.017, 213.2, 1.0),   // chart-3
    ok(0.378, 0.015, 216.0, 1.0),  // chart-4
    ok(0.275, 0.011, 216.9, 1.0),  // chart-5
    ok(0.987, 0.002, 197.1, 1.0),  // sidebar
    ok(0.148, 0.004, 228.8, 1.0),  // sidebar-foreground
    ok(0.218, 0.008, 223.9, 1.0),  // sidebar-primary
    ok(0.987, 0.002, 197.1, 1.0),  // sidebar-primary-foreground
    ok(0.963, 0.002, 197.1, 1.0),  // sidebar-accent
    ok(0.218, 0.008, 223.9, 1.0),  // sidebar-accent-foreground
    ok(0.925, 0.005, 214.3, 1.0),  // sidebar-border
    ok(0.723, 0.014, 214.4, 1.0),  // sidebar-ring
];

const MIST_DARK: Table = [
    ok(0.148, 0.004, 228.8, 1.0),   // background
    ok(0.987, 0.002, 197.1, 1.0),   // foreground
    ok(0.218, 0.008, 223.9, 1.0),   // card
    ok(0.987, 0.002, 197.1, 1.0),   // card-foreground
    ok(0.218, 0.008, 223.9, 1.0),   // popover
    ok(0.987, 0.002, 197.1, 1.0),   // popover-foreground
    ok(0.925, 0.005, 214.3, 1.0),   // primary
    ok(0.218, 0.008, 223.9, 1.0),   // primary-foreground
    ok(0.275, 0.011, 216.9, 1.0),   // secondary
    ok(0.987, 0.002, 197.1, 1.0),   // secondary-foreground
    ok(0.275, 0.011, 216.9, 1.0),   // muted
    ok(0.723, 0.014, 214.4, 1.0),   // muted-foreground
    ok(0.275, 0.011, 216.9, 1.0),   // accent
    ok(0.987, 0.002, 197.1, 1.0),   // accent-foreground
    ok(0.704, 0.191, 22.216, 1.0),  // destructive
    ok(1.0, 0.0, 0.0, 0.1),         // border
    ok(1.0, 0.0, 0.0, 0.15),        // input
    ok(0.56, 0.021, 213.5, 1.0),    // ring
    ok(0.872, 0.007, 219.6, 1.0),   // chart-1
    ok(0.56, 0.021, 213.5, 1.0),    // chart-2
    ok(0.45, 0.017, 213.2, 1.0),    // chart-3
    ok(0.378, 0.015, 216.0, 1.0),   // chart-4
    ok(0.275, 0.011, 216.9, 1.0),   // chart-5
    ok(0.218, 0.008, 223.9, 1.0),   // sidebar
    ok(0.987, 0.002, 197.1, 1.0),   // sidebar-foreground
    ok(0.488, 0.243, 264.376, 1.0), // sidebar-primary
    ok(0.987, 0.002, 197.1, 1.0),   // sidebar-primary-foreground
    ok(0.275, 0.011, 216.9, 1.0),   // sidebar-accent
    ok(0.987, 0.002, 197.1, 1.0),   // sidebar-accent-foreground
    ok(1.0, 0.0, 0.0, 0.1),         // sidebar-border
    ok(0.56, 0.021, 213.5, 1.0),    // sidebar-ring
];

const TAUPE_LIGHT: Table = [
    ok(1.0, 0.0, 0.0, 1.0),        // background
    ok(0.147, 0.004, 49.3, 1.0),   // foreground
    ok(1.0, 0.0, 0.0, 1.0),        // card
    ok(0.147, 0.004, 49.3, 1.0),   // card-foreground
    ok(1.0, 0.0, 0.0, 1.0),        // popover
    ok(0.147, 0.004, 49.3, 1.0),   // popover-foreground
    ok(0.214, 0.009, 43.1, 1.0),   // primary
    ok(0.986, 0.002, 67.8, 1.0),   // primary-foreground
    ok(0.96, 0.002, 17.2, 1.0),    // secondary
    ok(0.214, 0.009, 43.1, 1.0),   // secondary-foreground
    ok(0.96, 0.002, 17.2, 1.0),    // muted
    ok(0.547, 0.021, 43.1, 1.0),   // muted-foreground
    ok(0.96, 0.002, 17.2, 1.0),    // accent
    ok(0.214, 0.009, 43.1, 1.0),   // accent-foreground
    ok(0.577, 0.245, 27.325, 1.0), // destructive
    ok(0.922, 0.005, 34.3, 1.0),   // border
    ok(0.922, 0.005, 34.3, 1.0),   // input
    ok(0.714, 0.014, 41.2, 1.0),   // ring
    ok(0.868, 0.007, 39.5, 1.0),   // chart-1
    ok(0.547, 0.021, 43.1, 1.0),   // chart-2
    ok(0.438, 0.017, 39.3, 1.0),   // chart-3
    ok(0.367, 0.016, 35.7, 1.0),   // chart-4
    ok(0.268, 0.011, 36.5, 1.0),   // chart-5
    ok(0.986, 0.002, 67.8, 1.0),   // sidebar
    ok(0.147, 0.004, 49.3, 1.0),   // sidebar-foreground
    ok(0.214, 0.009, 43.1, 1.0),   // sidebar-primary
    ok(0.986, 0.002, 67.8, 1.0),   // sidebar-primary-foreground
    ok(0.96, 0.002, 17.2, 1.0),    // sidebar-accent
    ok(0.214, 0.009, 43.1, 1.0),   // sidebar-accent-foreground
    ok(0.922, 0.005, 34.3, 1.0),   // sidebar-border
    ok(0.714, 0.014, 41.2, 1.0),   // sidebar-ring
];

const TAUPE_DARK: Table = [
    ok(0.147, 0.004, 49.3, 1.0),    // background
    ok(0.986, 0.002, 67.8, 1.0),    // foreground
    ok(0.214, 0.009, 43.1, 1.0),    // card
    ok(0.986, 0.002, 67.8, 1.0),    // card-foreground
    ok(0.214, 0.009, 43.1, 1.0),    // popover
    ok(0.986, 0.002, 67.8, 1.0),    // popover-foreground
    ok(0.922, 0.005, 34.3, 1.0),    // primary
    ok(0.214, 0.009, 43.1, 1.0),    // primary-foreground
    ok(0.268, 0.011, 36.5, 1.0),    // secondary
    ok(0.986, 0.002, 67.8, 1.0),    // secondary-foreground
    ok(0.268, 0.011, 36.5, 1.0),    // muted
    ok(0.714, 0.014, 41.2, 1.0),    // muted-foreground
    ok(0.268, 0.011, 36.5, 1.0),    // accent
    ok(0.986, 0.002, 67.8, 1.0),    // accent-foreground
    ok(0.704, 0.191, 22.216, 1.0),  // destructive
    ok(1.0, 0.0, 0.0, 0.1),         // border
    ok(1.0, 0.0, 0.0, 0.15),        // input
    ok(0.547, 0.021, 43.1, 1.0),    // ring
    ok(0.868, 0.007, 39.5, 1.0),    // chart-1
    ok(0.547, 0.021, 43.1, 1.0),    // chart-2
    ok(0.438, 0.017, 39.3, 1.0),    // chart-3
    ok(0.367, 0.016, 35.7, 1.0),    // chart-4
    ok(0.268, 0.011, 36.5, 1.0),    // chart-5
    ok(0.214, 0.009, 43.1, 1.0),    // sidebar
    ok(0.986, 0.002, 67.8, 1.0),    // sidebar-foreground
    ok(0.488, 0.243, 264.376, 1.0), // sidebar-primary
    ok(0.986, 0.002, 67.8, 1.0),    // sidebar-primary-foreground
    ok(0.268, 0.011, 36.5, 1.0),    // sidebar-accent
    ok(0.986, 0.002, 67.8, 1.0),    // sidebar-accent-foreground
    ok(1.0, 0.0, 0.0, 0.1),         // sidebar-border
    ok(0.547, 0.021, 43.1, 1.0),    // sidebar-ring
];
