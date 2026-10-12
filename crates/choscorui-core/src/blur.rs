//! The backdrop blur that every painter applies, so that the software
//! renderer and the platform painters produce the same pixels.

/// Blurs premultiplied four-channel 8-bit pixels in place. `pixels` holds
/// `height` rows of `width` pixels, `stride` bytes apart. The blur
/// approximates a Gaussian of standard deviation `sigma` pixels with
/// three box blurs in each direction; edges repeat the nearest pixel.
/// The channel order does not matter.
pub fn blur_pixels(pixels: &mut [u8], width: usize, height: usize, stride: usize, sigma: f64) {
    if width == 0
        || height == 0
        || stride < width * 4
        || pixels.len() < stride * (height - 1) + width * 4
        || !sigma.is_finite()
        || sigma <= 0.0
    {
        return;
    }
    let radii = box_radii(sigma);
    let mut line = Vec::new();
    let mut out = Vec::new();
    for y in 0..height {
        let start = y * stride;
        line.clear();
        line.extend_from_slice(&pixels[start..start + width * 4]);
        for radius in radii {
            box_pass(&line, &mut out, radius);
            std::mem::swap(&mut line, &mut out);
        }
        pixels[start..start + width * 4].copy_from_slice(&line);
    }
    for x in 0..width {
        line.clear();
        for y in 0..height {
            let at = y * stride + x * 4;
            line.extend_from_slice(&pixels[at..at + 4]);
        }
        for radius in radii {
            box_pass(&line, &mut out, radius);
            std::mem::swap(&mut line, &mut out);
        }
        for y in 0..height {
            let at = y * stride + x * 4;
            pixels[at..at + 4].copy_from_slice(&line[y * 4..y * 4 + 4]);
        }
    }
}

/// The radii of three box blurs whose sum approximates a Gaussian of
/// standard deviation `sigma`.
fn box_radii(sigma: f64) -> [usize; 3] {
    // The ideal box width for three passes, split into two odd widths so
    // that the variance matches.
    let ideal = (12.0 * sigma * sigma / 3.0 + 1.0).sqrt();
    let mut lower = ideal.floor() as usize;
    if lower.is_multiple_of(2) {
        lower = lower.saturating_sub(1);
    }
    let lower = lower.max(1);
    let upper = lower + 2;
    let l = lower as f64;
    let wide = ((12.0 * sigma * sigma - 3.0 * l * l - 12.0 * l - 9.0) / (-4.0 * l - 4.0))
        .round()
        .clamp(0.0, 3.0) as usize;
    std::array::from_fn(|pass| if pass < wide { lower / 2 } else { upper / 2 })
}

/// One box blur of `radius` along a line of pixels.
fn box_pass(line: &[u8], out: &mut Vec<u8>, radius: usize) {
    let count = line.len() / 4;
    out.clear();
    out.resize(line.len(), 0);
    let span = 2 * radius + 1;
    let at = |index: isize, channel: usize| -> u32 {
        let clamped = index.clamp(0, count as isize - 1) as usize;
        u32::from(line[clamped * 4 + channel])
    };
    for channel in 0..4 {
        let mut sum: u32 = (-(radius as isize)..=radius as isize)
            .map(|index| at(index, channel))
            .sum();
        for index in 0..count {
            out[index * 4 + channel] = ((sum + span as u32 / 2) / span as u32) as u8;
            let i = index as isize;
            sum += at(i + radius as isize + 1, channel);
            sum -= at(i - radius as isize, channel);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_blur_spreads_a_point_and_keeps_flat_areas() {
        let mut flat = vec![200u8; 8 * 8 * 4];
        blur_pixels(&mut flat, 8, 8, 32, 2.0);
        assert!(flat.iter().all(|value| *value == 200));
        let mut point = vec![0u8; 15 * 4];
        point[7 * 4..7 * 4 + 4].copy_from_slice(&[255; 4]);
        blur_pixels(&mut point, 15, 1, 60, 2.0);
        assert!(point[7 * 4] < 255 && point[7 * 4] > point[6 * 4]);
        assert!(point[6 * 4] > point[5 * 4]);
        assert_eq!(point[6 * 4], point[8 * 4], "symmetric");
    }

    #[test]
    fn box_radii_match_the_gaussian_variance() {
        for sigma in [2.0, 4.0, 8.0, 16.0] {
            let variance: f64 = box_radii(sigma)
                .iter()
                .map(|radius| {
                    let width = (2 * radius + 1) as f64;
                    (width * width - 1.0) / 12.0
                })
                .sum();
            assert!(
                (variance / (sigma * sigma) - 1.0).abs() < 0.2,
                "{sigma}: {variance}"
            );
        }
    }
}
