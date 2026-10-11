//! OKLCH to sRGB conversion for palette tables.

use crate::Color;

impl Color {
    /// Converts an OKLCH color to straight-alpha sRGB. `lightness` and
    /// `alpha` are in `0..=1`, `chroma` is unbounded, and `hue` is in
    /// degrees. Channels outside the sRGB gamut are clipped, as browsers
    /// display them.
    pub fn from_oklch(lightness: f64, chroma: f64, hue: f64, alpha: f64) -> Self {
        let (a, b) = {
            let radians = hue.to_radians();
            (chroma * radians.cos(), chroma * radians.sin())
        };
        let l = (lightness + 0.396_337_777_4 * a + 0.215_803_757_3 * b).powi(3);
        let m = (lightness - 0.105_561_345_8 * a - 0.063_854_172_8 * b).powi(3);
        let s = (lightness - 0.089_484_177_5 * a - 1.291_485_548_0 * b).powi(3);
        let red = 4.076_741_662_1 * l - 3.307_711_591_3 * m + 0.230_969_929_2 * s;
        let green = -1.268_438_004_6 * l + 2.609_757_401_1 * m - 0.341_319_396_5 * s;
        let blue = -0.004_196_086_3 * l - 0.703_418_614_7 * m + 1.707_614_701_0 * s;
        Self::rgba8(encode(red), encode(green), encode(blue), unit_to_u8(alpha))
    }
}

/// Linear light to an eight-bit sRGB channel.
fn encode(linear: f64) -> u8 {
    let linear = if linear.is_finite() {
        linear.clamp(0.0, 1.0)
    } else {
        0.0
    };
    let gamma = if linear <= 0.003_130_8 {
        12.92 * linear
    } else {
        1.055 * linear.powf(1.0 / 2.4) - 0.055
    };
    unit_to_u8(gamma)
}

fn unit_to_u8(value: f64) -> u8 {
    let value = if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    };
    (value * 255.0).round() as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_values_convert() {
        assert_eq!(Color::from_oklch(1.0, 0.0, 0.0, 1.0), Color::WHITE);
        assert_eq!(Color::from_oklch(0.0, 0.0, 0.0, 1.0), Color::BLACK);
        // Published sRGB equivalents of the neutral ramp.
        assert_eq!(
            Color::from_oklch(0.145, 0.0, 0.0, 1.0),
            Color::rgb8(10, 10, 10)
        );
        assert_eq!(
            Color::from_oklch(0.205, 0.0, 0.0, 1.0),
            Color::rgb8(23, 23, 23)
        );
        assert_eq!(
            Color::from_oklch(0.922, 0.0, 0.0, 1.0),
            Color::rgb8(229, 229, 229)
        );
        assert_eq!(Color::from_oklch(1.0, 0.0, 0.0, 0.1).alpha, 26);
    }

    #[test]
    fn red_destructive_is_close_to_reference_hex() {
        // oklch(0.577 0.245 27.325) is published as #e7000b.
        let color = Color::from_oklch(0.577, 0.245, 27.325, 1.0);
        let near = |x: u8, y: u8| x.abs_diff(y) <= 1;
        assert!(near(color.red, 0xe7) && near(color.green, 0x00) && near(color.blue, 0x0b));
    }

    #[test]
    fn out_of_gamut_values_clip() {
        let color = Color::from_oklch(0.7, 0.4, 145.0, 1.0);
        assert_eq!(color.alpha, 255);
        assert_eq!(Color::from_oklch(f64::NAN, 0.0, 0.0, 2.0).alpha, 255);
    }
}
