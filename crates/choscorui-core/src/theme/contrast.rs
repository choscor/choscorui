//! High-contrast palette adjustment: the dim roles move away from their
//! paired surface until they reach the WCAG minimum ratios.

use super::{ColorRole, Palette};
use crate::Color;

/// Text roles need 4.5:1 against every paired surface.
pub const TEXT_RATIO: f64 = 4.5;
/// Borders, inputs, and rings need 3:1.
pub const NON_TEXT_RATIO: f64 = 3.0;

/// A role, the surfaces it must stand out from, and the minimum ratio.
pub const PAIRS: [(ColorRole, &[ColorRole], f64); 6] = [
    (
        ColorRole::MutedForeground,
        &[ColorRole::Background, ColorRole::Muted, ColorRole::Card],
        TEXT_RATIO,
    ),
    (ColorRole::Border, &[ColorRole::Background], NON_TEXT_RATIO),
    (ColorRole::Input, &[ColorRole::Background], NON_TEXT_RATIO),
    (ColorRole::Ring, &[ColorRole::Background], NON_TEXT_RATIO),
    (
        ColorRole::SidebarBorder,
        &[ColorRole::Sidebar],
        NON_TEXT_RATIO,
    ),
    (
        ColorRole::SidebarRing,
        &[ColorRole::Sidebar],
        NON_TEXT_RATIO,
    ),
];

/// Raises each paired role until it meets its ratio against every surface.
/// Raised colors become opaque.
pub fn raise(mut palette: Palette) -> Palette {
    for (role, surfaces, ratio) in PAIRS {
        let surface = palette.get(surfaces[0]);
        let mut color = composite(palette.get(role), surface);
        let target = if surface.relative_luminance() > 0.18 {
            Color::BLACK
        } else {
            Color::WHITE
        };
        let meets = |color: Color| {
            surfaces
                .iter()
                .all(|surface| color.contrast_ratio(palette.get(*surface)) >= ratio)
        };
        let mut step = 0;
        while !meets(color) && step < 50 {
            color = crate::lerp_color(color, target, 0.05);
            step += 1;
        }
        if !meets(color) {
            color = target;
        }
        palette.set(role, color);
    }
    palette
}

/// `color` drawn over the opaque `surface`.
fn composite(color: Color, surface: Color) -> Color {
    let alpha = f64::from(color.alpha) / 255.0;
    let mix = |top: u8, bottom: u8| {
        (f64::from(top) * alpha + f64::from(bottom) * (1.0 - alpha)).round() as u8
    };
    Color::rgb8(
        mix(color.red, surface.red),
        mix(color.green, surface.green),
        mix(color.blue, surface.blue),
    )
}
