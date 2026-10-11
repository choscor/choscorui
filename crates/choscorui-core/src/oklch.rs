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

impl Color {
    /// The OKLab coordinates (lightness, a, b) of the opaque color.
    pub fn to_oklab(self) -> (f64, f64, f64) {
        let linear = |channel: u8| {
            let c = f64::from(channel) / 255.0;
            if c <= 0.040_45 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        let (r, g, b) = (linear(self.red), linear(self.green), linear(self.blue));
        let l = (0.412_221_470_8 * r + 0.536_332_536_3 * g + 0.051_445_992_9 * b).cbrt();
        let m = (0.211_903_498_2 * r + 0.680_699_545_1 * g + 0.107_396_956_6 * b).cbrt();
        let s = (0.088_302_461_9 * r + 0.281_718_837_6 * g + 0.629_978_700_5 * b).cbrt();
        (
            0.210_454_255_3 * l + 0.793_617_785_0 * m - 0.004_072_046_8 * s,
            1.977_998_495_1 * l - 2.428_592_205_0 * m + 0.450_593_709_9 * s,
            0.025_904_037_1 * l + 0.782_771_766_2 * m - 0.808_675_766_0 * s,
        )
    }

    /// CSS `color-mix(in oklab, self, other t)`: `t` of `other` mixed into
    /// `self` in OKLab, with alpha mixed linearly. For the near-gray roles
    /// of the palettes this matches a mix in OKLCH.
    pub fn mix(self, other: Color, t: f64) -> Color {
        let t = if t.is_finite() {
            t.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let (l1, a1, b1) = self.to_oklab();
        let (l2, a2, b2) = other.to_oklab();
        let lerp = |x: f64, y: f64| x + (y - x) * t;
        let (l, a, b) = (lerp(l1, l2), lerp(a1, a2), lerp(b1, b2));
        let chroma = (a * a + b * b).sqrt();
        let hue = b.atan2(a).to_degrees();
        let alpha = lerp(f64::from(self.alpha), f64::from(other.alpha)) / 255.0;
        Color::from_oklch(l, chroma, hue, alpha)
    }

    /// `self` drawn over the opaque `background`.
    pub fn over(self, background: Color) -> Color {
        let alpha = f64::from(self.alpha) / 255.0;
        let blend = |top: u8, bottom: u8| {
            (f64::from(top) * alpha + f64::from(bottom) * (1.0 - alpha)).round() as u8
        };
        Color::rgb8(
            blend(self.red, background.red),
            blend(self.green, background.green),
            blend(self.blue, background.blue),
        )
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
    fn oklab_round_trips_and_mixes() {
        let color = Color::from_oklch(0.6, 0.118, 184.704, 1.0);
        let (l, a, b) = color.to_oklab();
        let back = Color::from_oklch(l, (a * a + b * b).sqrt(), b.atan2(a).to_degrees(), 1.0);
        assert_eq!(back, color);
        assert_eq!(Color::WHITE.mix(Color::BLACK, 0.0), Color::WHITE);
        assert_eq!(Color::WHITE.mix(Color::BLACK, 1.0), Color::BLACK);
        let gray = Color::WHITE.mix(Color::BLACK, 0.5);
        assert_eq!((gray.red, gray.red), (gray.green, gray.blue));
        assert_eq!(
            Color::rgba8(0, 0, 0, 128).over(Color::WHITE),
            Color::rgb8(127, 127, 127)
        );
    }

    #[test]
    fn out_of_gamut_values_clip() {
        let color = Color::from_oklch(0.7, 0.4, 145.0, 1.0);
        assert_eq!(color.alpha, 255);
        assert_eq!(Color::from_oklch(f64::NAN, 0.0, 0.0, 2.0).alpha, 255);
    }
}
