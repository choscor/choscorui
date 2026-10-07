//! Reference software renderer for display-list fixtures and offscreen output.

use choscorui_core::{Color, DisplayCommand, DisplayList, Point, Rect};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RenderError {
    InvalidScale,
    TooLarge,
    UnsupportedText,
}

/// Straight-alpha RGBA8 pixels in row-major order.
pub struct Raster {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

impl Raster {
    pub fn render(
        list: &DisplayList,
        width: u32,
        height: u32,
        scale: f64,
    ) -> Result<Self, RenderError> {
        if !scale.is_finite() || scale <= 0.0 {
            return Err(RenderError::InvalidScale);
        }
        let len = (width as usize)
            .checked_mul(height as usize)
            .and_then(|value| value.checked_mul(4))
            .ok_or(RenderError::TooLarge)?;
        let mut pixels = Vec::new();
        pixels
            .try_reserve_exact(len)
            .map_err(|_| RenderError::TooLarge)?;
        pixels.resize(len, 0);
        let mut raster = Self {
            width,
            height,
            pixels,
        };
        if width == 0 || height == 0 {
            return Ok(raster);
        }
        let mut clips: Vec<Rect> = Vec::new();
        let mut opacities = vec![1.0];

        for command in list.commands() {
            match command {
                DisplayCommand::PushClip(rect) => clips.push(*rect),
                DisplayCommand::PopClip => {
                    clips.pop();
                }
                DisplayCommand::FillRect { rect, color } => {
                    raster.paint_shape(
                        *color,
                        scale,
                        &clips,
                        *opacities.last().unwrap(),
                        |point| rect.contains(point),
                    );
                }
                DisplayCommand::StrokeRect { rect, color, width } => {
                    raster.paint_shape(*color, scale, &clips, *opacities.last().unwrap(), |point| {
                        stroke_contains(*rect, 0.0, *width, point)
                    })
                }
                DisplayCommand::FillRoundedRect {
                    rect,
                    radius,
                    color,
                } => {
                    raster.paint_shape(*color, scale, &clips, *opacities.last().unwrap(), |point| {
                        rounded_contains(*rect, *radius, point)
                    })
                }
                DisplayCommand::StrokeRoundedRect {
                    rect,
                    radius,
                    color,
                    width,
                } => {
                    raster.paint_shape(*color, scale, &clips, *opacities.last().unwrap(), |point| {
                        stroke_contains(*rect, *radius, *width, point)
                    })
                }
                DisplayCommand::DrawText { .. } => return Err(RenderError::UnsupportedText),
                DisplayCommand::PushOpacity(opacity) => {
                    opacities.push(opacities.last().unwrap() * opacity);
                }
                DisplayCommand::PopOpacity => {
                    if opacities.len() > 1 {
                        opacities.pop();
                    }
                }
            }
        }
        Ok(raster)
    }

    pub fn width(&self) -> u32 {
        self.width
    }
    pub fn height(&self) -> u32 {
        self.height
    }
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    pub fn pixel(&self, x: u32, y: u32) -> Option<[u8; 4]> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let index = ((y as usize) * (self.width as usize) + x as usize) * 4;
        Some(self.pixels[index..index + 4].try_into().unwrap())
    }

    fn paint_shape(
        &mut self,
        color: Color,
        scale: f64,
        clips: &[Rect],
        opacity: f64,
        contains: impl Fn(Point) -> bool,
    ) {
        let color = Color {
            alpha: (f64::from(color.alpha) * opacity).round() as u8,
            ..color
        };
        for y in 0..self.height {
            for x in 0..self.width {
                let point = Point::new((f64::from(x) + 0.5) / scale, (f64::from(y) + 0.5) / scale);
                if contains(point) && clips.iter().all(|clip| clip.contains(point)) {
                    let index = ((y as usize) * (self.width as usize) + x as usize) * 4;
                    blend(&mut self.pixels[index..index + 4], color);
                }
            }
        }
    }
}

fn rounded_contains(rect: Rect, radius: f64, point: Point) -> bool {
    if !rect.contains(point) {
        return false;
    }
    let radius = radius
        .min(rect.size.width / 2.0)
        .min(rect.size.height / 2.0)
        .max(0.0);
    let center_x = point.x.clamp(
        rect.origin.x + radius,
        rect.origin.x + rect.size.width - radius,
    );
    let center_y = point.y.clamp(
        rect.origin.y + radius,
        rect.origin.y + rect.size.height - radius,
    );
    (point.x - center_x).powi(2) + (point.y - center_y).powi(2) <= radius.powi(2)
}

fn stroke_contains(rect: Rect, radius: f64, width: f64, point: Point) -> bool {
    let outer = inset(rect, -width / 2.0);
    if !rounded_contains(outer, radius + width / 2.0, point) {
        return false;
    }
    let inner = inset(rect, width / 2.0);
    inner.size.width <= 0.0
        || inner.size.height <= 0.0
        || !rounded_contains(inner, (radius - width / 2.0).max(0.0), point)
}

fn inset(rect: Rect, amount: f64) -> Rect {
    Rect::new(
        Point::new(rect.origin.x + amount, rect.origin.y + amount),
        choscorui_core::Size::new(
            rect.size.width - amount * 2.0,
            rect.size.height - amount * 2.0,
        ),
    )
}

fn blend(destination: &mut [u8], source: Color) {
    let sa = f64::from(source.alpha) / 255.0;
    let da = f64::from(destination[3]) / 255.0;
    let out_a = sa + da * (1.0 - sa);
    if out_a == 0.0 {
        return;
    }
    for (channel, value) in [source.red, source.green, source.blue]
        .into_iter()
        .enumerate()
    {
        let out =
            (f64::from(value) * sa + f64::from(destination[channel]) * da * (1.0 - sa)) / out_a;
        destination[channel] = out.round() as u8;
    }
    destination[3] = (out_a * 255.0).round() as u8;
}
